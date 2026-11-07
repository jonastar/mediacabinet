//! Provider artwork, kept in the data folder so it works without the provider.
//! The `artwork` task downloads what a library's items list, and anything it
//! hasn't got to yet is downloaded when first asked for. Design: `docs/tasks.md`.
//!
//! The work list is computed each run: what's listed and not on disk. Files go in
//! under a temporary name until they're complete.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::json;
use sqlx::SqlitePool;

use crate::tasks::{Ended, Finished, Handle, Kind, Scope, Tasks, Trigger};

const TMDB_IMAGES: &str = "https://image.tmdb.org/t/p";
/// Failures kept in a task's outcome; the rest are only counted.
const FAILURES_KEPT: usize = 200;

/// TMDB image sizes per kind (people's photos are `profile`): big enough for a TV,
/// small enough to load fast. Changing one downloads that kind again.
fn size(kind: &str) -> &'static str {
    match kind {
        "backdrop" => "w1280",
        "still" => "w780",
        "profile" => "w342",
        _ => "w500",
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    /// Not something a provider could have, or the provider says it hasn't.
    #[error("no such artwork")]
    NotFound,
    /// Says nothing about the image: it may work next time.
    #[error("artwork unavailable: {0}")]
    Unreachable(String),
}

#[derive(Clone)]
pub struct ArtworkStore {
    dir: PathBuf,
    http: reqwest::Client,
    images: String,
}

impl ArtworkStore {
    pub fn new(dir: PathBuf) -> ArtworkStore {
        ArtworkStore::with_images(dir, TMDB_IMAGES.into())
    }

    /// With images from somewhere else, for tests.
    pub fn with_images(dir: PathBuf, images: String) -> ArtworkStore {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .expect("HTTP client builds");
        ArtworkStore { dir, http, images }
    }

    /// Where an image is kept. Provider paths look like "/abc123.jpg"; anything
    /// else has no place here.
    pub fn file(&self, provider: &str, kind: &str, path: &str) -> Option<PathBuf> {
        let safe = path.starts_with('/')
            && !path.contains("..")
            && path[1..]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if provider != "tmdb" || !safe {
            return None;
        }
        Some(self.dir.join(provider).join(size(kind)).join(&path[1..]))
    }

    /// The image, from disk or else downloaded and kept.
    pub async fn get(&self, provider: &str, kind: &str, path: &str) -> Result<Vec<u8>, FetchError> {
        let file = self
            .file(provider, kind, path)
            .ok_or(FetchError::NotFound)?;
        if let Ok(bytes) = tokio::fs::read(&file).await {
            return Ok(bytes);
        }
        let url = format!("{}/{}{path}", self.images, size(kind));
        let unreachable = |e: reqwest::Error| FetchError::Unreachable(e.without_url().to_string());
        let response = self.http.get(&url).send().await.map_err(unreachable)?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(FetchError::NotFound);
        }
        if !response.status().is_success() {
            return Err(FetchError::Unreachable(format!(
                "the provider answered {}",
                response.status()
            )));
        }
        let bytes = response.bytes().await.map_err(unreachable)?.to_vec();
        if let Err(e) = keep(&file, &bytes).await {
            tracing::warn!(error = %e, "can't keep artwork");
        }
        Ok(bytes)
    }
}

/// Writes to a temporary name first, since a request and a task may fetch the
/// same image at once, and a half-written file would be served as it is.
async fn keep(file: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    if let Some(parent) = file.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let attempt = file.with_extension(format!("{}.part", NEXT.fetch_add(1, Ordering::Relaxed)));
    tokio::fs::write(&attempt, bytes).await?;
    let renamed = tokio::fs::rename(&attempt, file).await;
    if renamed.is_err() {
        let _ = tokio::fs::remove_file(&attempt).await;
    }
    renamed
}

struct Due {
    provider: String,
    kind: String,
    path: String,
}

/// What `library`'s items list and isn't on disk. Posters and backdrops come
/// before stills, of which there are far more. `retry` includes what the provider
/// said it hasn't got.
async fn due(
    pool: &SqlitePool,
    store: &ArtworkStore,
    library: i64,
    retry: bool,
) -> Result<Vec<Due>, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT DISTINCT a.provider, a.kind, a.path
         FROM item_artwork a
         JOIN item i ON i.id = a.item_id
         LEFT JOIN artwork_unavailable u
             ON u.provider = a.provider AND u.kind = a.kind AND u.path = a.path
         WHERE i.library_id = ?1 AND (u.path IS NULL OR ?2)
         ORDER BY a.kind, a.path",
    )
    .bind(library)
    .bind(retry)
    .fetch_all(pool)
    .await?;
    let mut due = Vec::new();
    for (provider, kind, path) in rows {
        let Some(file) = store.file(&provider, &kind, &path) else {
            continue;
        };
        if !tokio::fs::try_exists(&file).await.unwrap_or(false) {
            due.push(Due {
                provider,
                kind,
                path,
            });
        }
    }
    Ok(due)
}

/// Whether a run started by something else would find anything to do.
pub async fn has_work(
    pool: &SqlitePool,
    store: &ArtworkStore,
    library: i64,
) -> Result<bool, sqlx::Error> {
    Ok(!due(pool, store, library, false).await?.is_empty())
}

/// Asks for a run if a task just left `library` with artwork to download. A
/// problem here is no problem for that task.
pub(crate) async fn request_if_due(task: &Handle, library: i64, trigger: Trigger) {
    let Some(store) = &task.services().artwork else {
        return;
    };
    let requested = async {
        if has_work(task.pool(), store, library).await? {
            task.request(Kind::Artwork, Scope::Library(library), trigger)
                .await?;
        }
        Ok::<(), sqlx::Error>(())
    };
    if let Err(e) = requested.await {
        tracing::warn!(library, error = %e, "can't request artwork");
    }
}

/// Checks every library now and then. This is what catches artwork from the
/// scheduled metadata refresh, and what a run stopped by an outage left behind.
pub fn spawn_sweeper(tasks: Tasks, store: ArtworkStore, every: Duration) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            if let Err(e) = sweep(&tasks, &store).await {
                tracing::warn!(error = %e, "can't check for artwork to download");
            }
        }
    });
}

async fn sweep(tasks: &Tasks, store: &ArtworkStore) -> Result<(), sqlx::Error> {
    let libraries: Vec<i64> = sqlx::query_scalar("SELECT id FROM library")
        .fetch_all(tasks.pool())
        .await?;
    for library in libraries {
        if has_work(tasks.pool(), store, library).await? {
            tasks
                .request(Kind::Artwork, Scope::Library(library), Trigger::Scheduled)
                .await?;
        }
    }
    Ok(())
}

pub(crate) async fn run_task(task: &Handle, library: i64) -> Result<Finished, sqlx::Error> {
    let Some(store) = &task.services().artwork else {
        return Ok(Finished {
            state: Ended::Failed,
            outcome: None,
            error: Some("Artwork downloads aren't set up.".into()),
        });
    };
    let pool = task.pool();
    let by_hand = task.trigger() == Trigger::Manual;
    let rows = due(pool, store, library, by_hand).await?;
    let total = rows.len();
    tracing::info!(total, "artwork due");
    task.progress(|p| {
        p.phase = "downloading";
        p.done = 0;
        p.total = total;
    });

    let mut saved = 0;
    let mut unavailable = 0;
    let mut failures = Vec::new();
    let mut outage = None;
    let mut cancelled = false;
    for (done, row) in rows.iter().enumerate() {
        if task.cancelled() {
            cancelled = true;
            break;
        }
        match store.get(&row.provider, &row.kind, &row.path).await {
            Ok(_) => {
                saved += 1;
                sqlx::query(
                    "DELETE FROM artwork_unavailable WHERE provider = ? AND kind = ? AND path = ?",
                )
                .bind(&row.provider)
                .bind(&row.kind)
                .bind(&row.path)
                .execute(pool)
                .await?;
            }
            Err(FetchError::NotFound) => {
                unavailable += 1;
                sqlx::query(
                    "INSERT INTO artwork_unavailable (provider, kind, path) VALUES (?, ?, ?)
                     ON CONFLICT DO UPDATE SET tried_at = unixepoch()",
                )
                .bind(&row.provider)
                .bind(&row.kind)
                .bind(&row.path)
                .execute(pool)
                .await?;
                tracing::warn!(
                    kind = row.kind,
                    path = row.path,
                    "provider has no such image"
                );
                if failures.len() < FAILURES_KEPT {
                    failures.push(json!({ "kind": row.kind, "path": row.path }));
                }
            }
            Err(FetchError::Unreachable(e)) => {
                // The rest would fail the same way, and nothing is wrong with them.
                tracing::warn!(error = e, "artwork provider unreachable; stopping");
                outage = Some(e);
                break;
            }
        }
        task.progress(|p| p.done = done + 1);
    }

    // An outage is only worth telling anyone about if they asked for this: the
    // next sweep carries on by itself, and would otherwise report it every time.
    let failed = by_hand && outage.is_some();
    let error = if failed {
        Some(format!(
            "The artwork provider is unreachable; {saved} of {total} downloaded."
        ))
    } else {
        None
    };
    let state = if failed {
        Ended::Failed
    } else if cancelled {
        Ended::Cancelled
    } else {
        Ended::Ok
    };
    Ok(Finished {
        state,
        outcome: Some(json!({
            "saved": saved,
            "unavailable": unavailable,
            "unreachable": outage,
            "failures": failures,
        })),
        error,
    })
}

//! A library's inbox: a folder Media Cabinet owns, whose finished videos are
//! imported on their own. What it can't place it leaves where it is, marked as
//! needing attention. Design: `docs/library-management.md`.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::imports::{self, ImportCandidate, ImportGuess, ImportsError};
use crate::organize::{self, ImportOptions, Naming, TransferMode};
use crate::scan::{self, ProbeError};
use crate::tasks::{Ended, Finished, Handle, Kind, Scope, Tasks, Trigger};

/// A file shorter than this share of the expected runtime may be incomplete.
const SHORT: f64 = 0.5;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct InboxFile {
    pub path: String,
    /// Relative to the inbox.
    pub relative_path: String,
    pub reason: String,
    pub seen_at: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Inbox {
    pub path: Option<String>,
    /// Whether the folder can be reached now.
    pub online: bool,
    /// Videos it couldn't import alone.
    pub attention: Vec<InboxFile>,
}

pub async fn inbox_path(pool: &SqlitePool, library: i64) -> Result<Option<String>, sqlx::Error> {
    Ok(
        sqlx::query_scalar::<_, Option<String>>("SELECT inbox_path FROM library WHERE id = ?")
            .bind(library)
            .fetch_optional(pool)
            .await?
            .flatten(),
    )
}

/// The inbox of a library and what in it needs a person.
pub async fn state(pool: &SqlitePool, library: i64) -> Result<Inbox, sqlx::Error> {
    let path = inbox_path(pool, library).await?;
    let rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
        "SELECT path, reason, seen_at FROM inbox_file
         WHERE library_id = ? AND state = 'attention' ORDER BY path",
    )
    .bind(library)
    .fetch_all(pool)
    .await?;
    let base = path.clone().unwrap_or_default();
    let attention = rows
        .into_iter()
        .filter(|(p, _, _)| Path::new(p).exists())
        .map(|(p, reason, seen_at)| InboxFile {
            relative_path: scan::relative_path(Path::new(&base), Path::new(&p))
                .unwrap_or_else(|| p.clone()),
            path: p,
            reason: reason.unwrap_or_default(),
            seen_at,
        })
        .collect();
    Ok(Inbox {
        online: path.as_deref().is_some_and(|p| Path::new(p).is_dir()),
        path,
        attention,
    })
}

/// A recorded file: its identity when last looked at, and what came of it.
struct Seen {
    device: Option<i64>,
    inode: Option<i64>,
    size: i64,
    mtime: i64,
    imported: bool,
}

/// path, device, inode, size, mtime, state.
type SeenRow = (String, Option<i64>, Option<i64>, i64, i64, String);

async fn seen(pool: &SqlitePool, library: i64) -> Result<HashMap<String, Seen>, sqlx::Error> {
    let rows: Vec<SeenRow> = sqlx::query_as(
        "SELECT path, device_id, inode, size, mtime, state FROM inbox_file WHERE library_id = ?",
    )
    .bind(library)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(path, device, inode, size, mtime, state)| {
            (
                path,
                Seen {
                    device,
                    inode,
                    size,
                    mtime,
                    imported: state == "imported",
                },
            )
        })
        .collect())
}

fn identity(path: &Path) -> (Option<i64>, Option<i64>) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(m) = std::fs::metadata(path) {
            return (Some(m.dev() as i64), Some(m.ino() as i64));
        }
    }
    let _ = path;
    (None, None)
}

impl Seen {
    fn same_file(&self, f: &ImportCandidate, identity: (Option<i64>, Option<i64>)) -> bool {
        (self.device, self.inode) == identity && self.size == f.size && self.mtime == f.mtime
    }
}

/// Whether the inbox holds a finished video it hasn't looked at yet. Cheap
/// enough to ask every minute, so a task is only started when there's work.
pub async fn has_work(pool: &SqlitePool, library: i64, settle: i64) -> Result<bool, sqlx::Error> {
    let Some(path) = inbox_path(pool, library).await? else {
        return Ok(false);
    };
    let Ok(listing) = imports::list(pool, library, &path, settle).await else {
        return Ok(false);
    };
    let seen = seen(pool, library).await?;
    Ok(listing.files.iter().any(|f| {
        f.waiting.is_none()
            && !seen
                .get(&f.path)
                .is_some_and(|s| s.same_file(f, identity(Path::new(&f.path))))
    }))
}

/// Looks at every library's inbox now and then, and starts a task for those
/// with something new.
pub fn spawn_checker(tasks: Tasks, every: Duration, settle: i64) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tick.tick().await;
            if let Err(e) = check(&tasks, settle).await {
                tracing::warn!(error = %e, "can't look at the inboxes");
            }
        }
    });
}

async fn check(tasks: &Tasks, settle: i64) -> Result<(), sqlx::Error> {
    let libraries: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM library WHERE inbox_path IS NOT NULL")
            .fetch_all(tasks.pool())
            .await?;
    for library in libraries {
        if tasks
            .running(Kind::Inbox, Scope::Library(library))
            .is_none()
            && has_work(tasks.pool(), library, settle).await?
        {
            tasks
                .request(Kind::Inbox, Scope::Library(library), Trigger::Scheduled)
                .await?;
        }
    }
    Ok(())
}

/// How a guess reads to a person: `Night Harbor · S01E02`.
fn label(g: &ImportGuess) -> String {
    let name = match g.year {
        Some(y) => format!("{} ({y})", g.title),
        None => g.title.clone(),
    };
    if g.episodes.is_empty() {
        return name;
    }
    let eps: Vec<&str> = g.episodes.iter().map(|e| e.label.as_str()).collect();
    format!("{name} · {}", eps.join(" + "))
}

/// Why a person should look at this file rather than have it imported, if so.
async fn hold(
    task: &Handle,
    f: &ImportCandidate,
) -> Result<Result<(ImportGuess, Vec<i64>), String>, sqlx::Error> {
    let pool = task.pool();
    let Some(g) = f.guess.clone() else {
        return Ok(Err(
            "Not recognised: nothing in the library has this name.".into()
        ));
    };
    let items: Vec<i64> = if g.kind == "movie" {
        vec![g.item_id]
    } else {
        g.episodes.iter().map(|e| e.id).collect()
    };
    if items.is_empty() {
        return Ok(Err(format!(
            "It looks like {}, but the name doesn't say which episode.",
            label(&g)
        )));
    }
    // As sure as a scan has to be to link a file without asking.
    if g.confidence < task.services().match_settings.accept {
        return Ok(Err(format!(
            "Probably {}, but not sure enough to import it alone.",
            label(&g)
        )));
    }
    let list = serde_json::to_string(&items).expect("ids serialise");
    let has_file: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM file_link l JOIN media_file f ON f.id = l.file_id
                        WHERE l.item_id IN (SELECT value FROM json_each(?)) AND f.status = 'present')",
    )
    .bind(list.clone())
    .fetch_one(pool)
    .await?;
    if has_file {
        return Ok(Err(format!(
            "{} already has a file. Import this one by hand to replace it.",
            label(&g)
        )));
    }
    // A partial file can look finished: check its length against the runtime.
    let expected: Option<i64> = sqlx::query_scalar(
        "SELECT sum(runtime_seconds) FROM item WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(list)
    .fetch_one(pool)
    .await?;
    match task.services().prober.probe(Path::new(&f.path)).await {
        Ok(probe) => {
            if let (Some(d), Some(e)) = (probe.duration_seconds, expected)
                && e > 0
                && d < e as f64 * SHORT
            {
                return Ok(Err(format!(
                    "It's {} min long where {} min was expected, so it may not have finished.",
                    (d / 60.0).round(),
                    (e as f64 / 60.0).round()
                )));
            }
        }
        // Without ffprobe nothing can be checked; scans say so already.
        Err(ProbeError::Unavailable { .. }) => {}
        Err(e) => return Ok(Err(format!("It can't be read as a video: {e}"))),
    }
    Ok(Ok((g, items)))
}

async fn record(
    pool: &SqlitePool,
    library: i64,
    f: &ImportCandidate,
    outcome: Result<i64, &str>,
) -> Result<(), sqlx::Error> {
    let (device, inode) = identity(Path::new(&f.path));
    let (state, reason, file_id) = match outcome {
        Ok(file) => ("imported", None, Some(file)),
        Err(reason) => ("attention", Some(reason), None),
    };
    sqlx::query(
        "INSERT INTO inbox_file (library_id, path, device_id, inode, size, mtime, state, reason, file_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (library_id, path) DO UPDATE SET
             device_id = excluded.device_id, inode = excluded.inode, size = excluded.size,
             mtime = excluded.mtime, state = excluded.state, reason = excluded.reason,
             file_id = excluded.file_id, seen_at = unixepoch()",
    )
    .bind(library)
    .bind(&f.path)
    .bind(device)
    .bind(inode)
    .bind(f.size)
    .bind(f.mtime)
    .bind(state)
    .bind(reason)
    .bind(file_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Imports what's new and finished in the inbox. Run by hand, it also looks
/// again at what needed attention, in case the library has changed since.
pub(crate) async fn run_task(task: &Handle, library: i64) -> Result<Finished, sqlx::Error> {
    let pool = task.pool();
    let failed = |error: String| Finished {
        state: Ended::Failed,
        outcome: None,
        error: Some(error),
    };
    let Some(path) = inbox_path(pool, library).await? else {
        return Ok(Finished {
            state: Ended::Ok,
            outcome: None,
            error: None,
        });
    };
    let listing = match imports::list(pool, library, &path, task.services().inbox_settle).await {
        Ok(listing) => listing,
        Err(ImportsError::Db(e)) => return Err(e),
        Err(e) => return Ok(failed(format!("Can't look in the inbox {path}: {e}"))),
    };
    let again = task.trigger() == Trigger::Manual;
    let seen = seen(pool, library).await?;
    let (mut imported, mut attention, mut waiting) = (0, 0, 0);
    let todo: Vec<&ImportCandidate> = listing
        .files
        .iter()
        .filter(|f| {
            if f.waiting.is_some() {
                waiting += 1;
                return false;
            }
            match seen.get(&f.path) {
                Some(s) if s.same_file(f, identity(Path::new(&f.path))) => !s.imported && again,
                _ => true,
            }
        })
        .collect();
    task.progress(|p| {
        p.phase = "importing";
        p.total = todo.len();
    });

    for (n, f) in todo.into_iter().enumerate() {
        if task.cancelled() {
            break;
        }
        // Already in the library as this very file: a hard link made before.
        let (device, inode) = identity(Path::new(&f.path));
        let linked: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM media_file WHERE device_id = ? AND inode = ? AND status = 'present' LIMIT 1",
        )
        .bind(device)
        .bind(inode)
        .fetch_optional(pool)
        .await?;
        if let (Some(file), Some(_)) = (linked, inode) {
            record(pool, library, f, Ok(file)).await?;
            continue;
        }
        match hold(task, f).await? {
            Err(reason) => {
                tracing::info!(path = %f.relative_path, reason, "left in the inbox");
                record(pool, library, f, Err(&reason)).await?;
                attention += 1;
            }
            Ok((guess, items)) => {
                let mode = if f.same_disk {
                    TransferMode::Hardlink
                } else {
                    TransferMode::Move
                };
                let result = organize::import_file(
                    pool,
                    &task.services().prober,
                    Path::new(&f.path),
                    &items,
                    ImportOptions {
                        mode,
                        naming: Some(Naming::Layout),
                        replace: false,
                        dry_run: false,
                    },
                )
                .await;
                match result {
                    Ok(done) => {
                        tracing::info!(path = %f.relative_path, to = %done.path, item = %label(&guess), "imported");
                        record(pool, library, f, Ok(done.file_id)).await?;
                        imported += 1;
                    }
                    Err(organize::OrganizeError::Db(e)) => return Err(e),
                    Err(e) => {
                        let reason = format!("{}: {e}", label(&guess));
                        tracing::warn!(path = %f.relative_path, reason, "couldn't import");
                        record(pool, library, f, Err(&reason)).await?;
                        attention += 1;
                    }
                }
            }
        }
        task.progress(|p| p.done = n + 1);
    }

    // Forget what has left the inbox.
    let present: Vec<&str> = listing.files.iter().map(|f| f.path.as_str()).collect();
    let present = serde_json::to_string(&present).expect("paths serialise");
    if !listing.truncated {
        sqlx::query(
            "DELETE FROM inbox_file WHERE library_id = ?
               AND path NOT IN (SELECT value FROM json_each(?))",
        )
        .bind(library)
        .bind(present)
        .execute(pool)
        .await?;
    }

    Ok(Finished {
        state: if task.cancelled() {
            Ended::Cancelled
        } else {
            Ended::Ok
        },
        outcome: Some(json!({ "imported": imported, "attention": attention, "waiting": waiting })),
        error: None,
    })
}

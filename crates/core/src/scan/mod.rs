//! Scanning: discover files on a root and keep their identity across renames and
//! moves (see `plan`), attach sidecars, then probe new and changed files.
//!
//! Scans of the same root must not run concurrently; the caller serialises them.

mod hash;
mod plan;
mod probe;
mod service;
mod sidecar;
mod walk;

use std::collections::HashMap;
use std::path::PathBuf;

use futures::StreamExt;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::events;
use plan::{Existing, MatchKind, Plan};

pub use hash::partial_hash;
pub use probe::{Probe, ProbeError, Prober, Stream};
pub use service::{Phase, ScanProgress, ScanService};
pub(crate) use service::{finish_task, run_task};
pub use walk::{Found, Walk, is_sidecar, is_video, stat, walk};

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("library root {0} does not exist")]
    RootNotFound(i64),
    /// The root folder can't be read (unmounted, offline, permissions). Nothing is changed.
    #[error("library root {path} is unreachable: {source}")]
    RootUnreachable {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// The root is readable but empty while files were known on it, which usually
    /// means an unmounted mount point. Nothing is changed.
    #[error("library root {path} is empty but {known} files were present; is it mounted?")]
    RootEmpty { path: PathBuf, known: usize },
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error("scan task failed: {0}")]
    Task(#[from] tokio::task::JoinError),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ScanReport {
    pub unchanged: usize,
    pub added: usize,
    pub moved: usize,
    pub changed: usize,
    pub restored: usize,
    pub missing: usize,
    /// Files and folders that couldn't be read; files known under them were left alone.
    pub unreadable: usize,
    pub skipped_non_utf8: usize,
    /// Sidecars attached to a video after this scan.
    pub sidecars: usize,
}

/// Scans one library root and records what changed, with events, in one transaction.
pub async fn scan_root(pool: &SqlitePool, root_id: i64) -> Result<ScanReport, ScanError> {
    scan_root_with_progress(pool, root_id, |_, _, _| ()).await
}

/// `scan_root`, calling `progress(phase, done, total)` while walking (files
/// found so far, no total) and while hashing new files.
pub async fn scan_root_with_progress(
    pool: &SqlitePool,
    root_id: i64,
    progress: impl Fn(Phase, usize, usize) + Send + 'static,
) -> Result<ScanReport, ScanError> {
    let root: String = sqlx::query_scalar("SELECT path FROM library_root WHERE id = ?")
        .bind(root_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ScanError::RootNotFound(root_id))?;
    let root = PathBuf::from(root);

    let existing: Vec<Existing> = sqlx::query_as::<_, ExistingRow>(
        "SELECT id, relative_path, size, mtime, device_id, inode, content_hash, status
         FROM media_file WHERE root_id = ? AND status <> 'removed'",
    )
    .bind(root_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(Existing::from)
    .collect();
    let known = existing.iter().filter(|r| r.status == "present").count();

    let (plan, report, sidecars, unreadable) = tokio::task::spawn_blocking(move || {
        let mut walk = walk::walk_with_progress(&root, |found| progress(Phase::Walking, found, 0))
            .map_err(|source| ScanError::RootUnreachable {
                path: root.clone(),
                source,
            })?;
        if walk.files.is_empty() && walk.unreadable.is_empty() && known > 0 {
            return Err(ScanError::RootEmpty { path: root, known });
        }
        let mut report = ScanReport {
            unreadable: walk.unreadable.len(),
            skipped_non_utf8: walk.skipped_non_utf8,
            ..ScanReport::default()
        };
        let sidecars = std::mem::take(&mut walk.sidecars);
        let unreadable = Walk {
            unreadable: walk.unreadable.clone(),
            ..Walk::default()
        };
        let plan = plan::plan(existing, walk, &root, hash::partial_hash, |done, total| {
            progress(Phase::Hashing, done, total)
        });
        report.unreadable += plan.unreadable_new;
        Ok((plan, report, sidecars, unreadable))
    })
    .await??;

    apply(pool, root_id, plan, report, sidecars, unreadable).await
}

async fn apply(
    pool: &SqlitePool,
    root_id: i64,
    plan: Plan,
    mut report: ScanReport,
    sidecars: Vec<String>,
    unreadable: Walk,
) -> Result<ScanReport, ScanError> {
    let now = chrono::Utc::now().timestamp();
    let mut tx = crate::db::begin_write(pool).await?;

    // Missing first, then park moved rows on temporary paths, so swaps and files
    // moved onto a vanished file's path never collide on the unique path index.
    for (id, path) in &plan.missing {
        sqlx::query("UPDATE media_file SET status = 'missing' WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        events::emit(
            &mut tx,
            events::FILE_MISSING,
            json!({ "file_id": id, "root_id": root_id, "path": path }),
        )
        .await?;
        report.missing += 1;
    }
    for m in &plan.matched {
        if matches!(m.kind, MatchKind::Moved { .. }) {
            sqlx::query("UPDATE media_file SET relative_path = char(0) || id WHERE id = ?")
                .bind(m.id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for m in &plan.matched {
        let f = &m.found;
        let new_hash = match &m.kind {
            MatchKind::Changed { hash } => Some(hash.clone()),
            _ => None,
        };
        sqlx::query(
            "UPDATE media_file SET
                 relative_path = ?3, size = ?4, mtime = ?5, device_id = ?6, inode = ?7,
                 content_hash = CASE WHEN ?1 THEN ?2 ELSE content_hash END,
                 probed_at = CASE WHEN ?1 THEN NULL ELSE probed_at END,
                 probe_error = CASE WHEN ?1 THEN NULL ELSE probe_error END,
                 status = CASE WHEN status = 'ignored' THEN 'ignored' ELSE 'present' END,
                 last_seen = ?8
             WHERE id = ?9",
        )
        .bind(new_hash.is_some())
        .bind(new_hash.flatten())
        .bind(&f.relative_path)
        .bind(f.size)
        .bind(f.mtime)
        .bind(f.device_id)
        .bind(f.inode)
        .bind(now)
        .bind(m.id)
        .execute(&mut *tx)
        .await?;

        let payload = json!({ "file_id": m.id, "root_id": root_id, "path": f.relative_path });
        match &m.kind {
            MatchKind::Same => {
                if !m.was_missing {
                    report.unchanged += 1;
                }
            }
            MatchKind::Moved { from } => {
                let mut payload = payload.clone();
                payload["from"] = json!(from);
                events::emit(&mut tx, events::FILE_MOVED, payload).await?;
                report.moved += 1;
            }
            MatchKind::Changed { .. } => {
                events::emit(&mut tx, events::FILE_CHANGED, payload.clone()).await?;
                report.changed += 1;
            }
        }
        if m.was_missing {
            events::emit(&mut tx, events::FILE_RESTORED, payload).await?;
            report.restored += 1;
        }
    }

    for (f, hash) in &plan.added {
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO media_file
                 (root_id, library_id, relative_path, size, mtime, device_id, inode, content_hash,
                  first_seen, last_seen)
             SELECT ?1, library_id, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9 FROM library_root WHERE id = ?1
             RETURNING id",
        )
        .bind(root_id)
        .bind(&f.relative_path)
        .bind(f.size)
        .bind(f.mtime)
        .bind(f.device_id)
        .bind(f.inode)
        .bind(hash)
        .bind(now)
        .bind(now)
        .fetch_one(&mut *tx)
        .await?;
        events::emit(
            &mut tx,
            events::FILE_ADDED,
            json!({ "file_id": id, "root_id": root_id, "path": f.relative_path }),
        )
        .await?;
        report.added += 1;
    }

    report.sidecars = sync_sidecars(&mut tx, root_id, &sidecars, &unreadable).await?;

    tx.commit().await?;
    tracing::info!(root_id, ?report, "scan finished");
    Ok(report)
}

/// Makes the root's sidecar rows match what's on disk. Rows under unreadable folders
/// are kept. Returns the number of sidecars attached.
async fn sync_sidecars(
    tx: &mut sqlx::SqliteConnection,
    root_id: i64,
    sidecars: &[String],
    unreadable: &Walk,
) -> Result<usize, sqlx::Error> {
    let videos: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, relative_path FROM media_file
         WHERE root_id = ? AND status IN ('present', 'ignored')",
    )
    .bind(root_id)
    .fetch_all(&mut *tx)
    .await?;
    let ids: HashMap<&str, i64> = videos.iter().map(|(id, p)| (p.as_str(), *id)).collect();
    let paths: Vec<String> = videos.iter().map(|(_, p)| p.clone()).collect();
    let assigned = sidecar::assign(&paths, sidecars);

    let existing: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, relative_path FROM sidecar WHERE root_id = ? AND location = 'root'",
    )
    .bind(root_id)
    .fetch_all(&mut *tx)
    .await?;
    let keep: std::collections::HashSet<&str> =
        assigned.iter().map(|a| a.relative_path.as_str()).collect();
    for (id, path) in &existing {
        if !keep.contains(path.as_str()) && !unreadable.is_unreadable(path) {
            sqlx::query("DELETE FROM sidecar WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
    }

    for a in &assigned {
        sqlx::query(
            "INSERT INTO sidecar
                 (file_id, root_id, location, relative_path, kind, language, is_forced,
                  is_hearing_impaired)
             VALUES (?, ?, 'root', ?, ?, ?, ?, ?)
             ON CONFLICT (root_id, location, relative_path) DO UPDATE SET
                 file_id = excluded.file_id, kind = excluded.kind, language = excluded.language,
                 is_forced = excluded.is_forced,
                 is_hearing_impaired = excluded.is_hearing_impaired",
        )
        .bind(ids[a.video.as_str()])
        .bind(root_id)
        .bind(&a.relative_path)
        .bind(a.kind)
        .bind(&a.language)
        .bind(a.is_forced)
        .bind(a.is_hearing_impaired)
        .execute(&mut *tx)
        .await?;
    }
    Ok(assigned.len())
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeRootError {
    #[error("library root {0} does not exist")]
    RootNotFound(i64),
    /// ffprobe couldn't be started at all; no files were marked as failed.
    #[error(transparent)]
    Unavailable(ProbeError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct ProbeReport {
    pub probed: usize,
    pub failed: usize,
}

/// Probes the root's present files that haven't been probed since they were added
/// or changed. A file that can't be probed records the error and isn't retried
/// until its content changes.
pub async fn probe_root(
    pool: &SqlitePool,
    root_id: i64,
    prober: &Prober,
) -> Result<ProbeReport, ProbeRootError> {
    probe_root_with_progress(pool, root_id, prober, |_, _| ()).await
}

/// `probe_root`, calling `progress(done, total)` as files finish.
pub async fn probe_root_with_progress(
    pool: &SqlitePool,
    root_id: i64,
    prober: &Prober,
    progress: impl Fn(usize, usize),
) -> Result<ProbeReport, ProbeRootError> {
    let root: String = sqlx::query_scalar("SELECT path FROM library_root WHERE id = ?")
        .bind(root_id)
        .fetch_optional(pool)
        .await?
        .ok_or(ProbeRootError::RootNotFound(root_id))?;
    let root = PathBuf::from(root);
    let pending: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, relative_path FROM media_file
         WHERE root_id = ? AND status = 'present' AND probed_at IS NULL
         ORDER BY relative_path",
    )
    .bind(root_id)
    .fetch_all(pool)
    .await?;

    let total = pending.len();
    progress(0, total);
    let mut results = futures::stream::iter(pending)
        .map(|(id, path)| {
            let full = root.join(&path);
            async move { (id, path, prober.probe(&full).await) }
        })
        .buffer_unordered(prober.concurrency.max(1));

    let mut report = ProbeReport::default();
    while let Some((id, path, result)) = results.next().await {
        progress(report.probed + report.failed + 1, total);
        match result {
            Ok(probe) => {
                save_probe(pool, id, &probe).await?;
                report.probed += 1;
            }
            Err(err @ ProbeError::Unavailable { .. }) => {
                return Err(ProbeRootError::Unavailable(err));
            }
            Err(err) => {
                tracing::warn!(path, error = %err, "probe failed");
                sqlx::query(
                    "UPDATE media_file SET probed_at = unixepoch(), probe_error = ? WHERE id = ?",
                )
                .bind(err.to_string())
                .bind(id)
                .execute(pool)
                .await?;
                report.failed += 1;
            }
        }
    }
    tracing::info!(root_id, ?report, "probe finished");
    Ok(report)
}

async fn save_probe(pool: &SqlitePool, file_id: i64, probe: &Probe) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query("DELETE FROM media_stream WHERE file_id = ?")
        .bind(file_id)
        .execute(&mut *tx)
        .await?;
    for s in &probe.streams {
        sqlx::query(
            "INSERT INTO media_stream
                 (file_id, stream_index, kind, codec, language, channels, width, height,
                  hdr_format, bitrate, is_default, is_forced)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(file_id)
        .bind(s.index)
        .bind(s.kind)
        .bind(&s.codec)
        .bind(&s.language)
        .bind(s.channels)
        .bind(s.width)
        .bind(s.height)
        .bind(&s.hdr_format)
        .bind(s.bitrate)
        .bind(s.is_default)
        .bind(s.is_forced)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE media_file SET container = ?, duration_seconds = ?, probed_at = unixepoch(),
             probe_error = NULL
         WHERE id = ?",
    )
    .bind(&probe.container)
    .bind(probe.duration_seconds)
    .bind(file_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

/// What the last probe found in a file, as stored, with the sidecars next to it.
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct StoredProbe {
    pub file_id: i64,
    /// ffprobe's format name, e.g. `matroska,webm`.
    pub container: Option<String>,
    pub duration_seconds: Option<f64>,
    /// Null until probed; reset when the content changes.
    pub probed_at: Option<i64>,
    pub probe_error: Option<String>,
    /// In the file's own order.
    #[sqlx(skip)]
    pub streams: Vec<StoredStream>,
    #[sqlx(skip)]
    pub sidecars: Vec<StoredSidecar>,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct StoredStream {
    pub index: i64,
    /// `video`, `audio` or `subtitle`.
    pub kind: String,
    pub codec: Option<String>,
    pub language: Option<String>,
    pub channels: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub hdr_format: Option<String>,
    /// Bits per second.
    pub bitrate: Option<i64>,
    pub is_default: bool,
    pub is_forced: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct StoredSidecar {
    pub id: i64,
    /// `subtitle`, `nfo` or `artwork`.
    pub kind: String,
    /// `root` (next to the video) or `cache`.
    pub location: String,
    pub relative_path: String,
    pub language: Option<String>,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
}

/// The file's stored probe, or none if there's no such file.
pub async fn stored_probe(
    pool: &SqlitePool,
    file_id: i64,
) -> Result<Option<StoredProbe>, sqlx::Error> {
    let probe: Option<StoredProbe> = sqlx::query_as(
        "SELECT id AS file_id, container, duration_seconds, probed_at, probe_error
         FROM media_file WHERE id = ?",
    )
    .bind(file_id)
    .fetch_optional(pool)
    .await?;
    let Some(mut probe) = probe else {
        return Ok(None);
    };
    probe.streams = sqlx::query_as(
        "SELECT stream_index AS \"index\", kind, codec, language, channels, width, height,
                hdr_format, bitrate, is_default, is_forced
         FROM media_stream WHERE file_id = ? ORDER BY stream_index",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await?;
    probe.sidecars = sqlx::query_as(
        "SELECT id, kind, location, relative_path, language, is_forced, is_hearing_impaired
         FROM sidecar WHERE file_id = ? ORDER BY kind, relative_path",
    )
    .bind(file_id)
    .fetch_all(pool)
    .await?;
    Ok(Some(probe))
}

#[derive(sqlx::FromRow)]
struct ExistingRow {
    id: i64,
    relative_path: String,
    size: i64,
    mtime: i64,
    device_id: Option<i64>,
    inode: Option<i64>,
    content_hash: Option<String>,
    status: String,
}

impl From<ExistingRow> for Existing {
    fn from(r: ExistingRow) -> Self {
        Existing {
            id: r.id,
            relative_path: r.relative_path,
            size: r.size,
            mtime: r.mtime,
            device_id: r.device_id,
            inode: r.inode,
            content_hash: r.content_hash,
            status: r.status,
        }
    }
}

//! Scanning: discover files on a root and keep their identity across renames and
//! moves. See `plan` for how found files are matched to known ones.
//!
//! Scans of the same root must not run concurrently; the caller serialises them.

mod hash;
mod plan;
mod walk;

use std::path::PathBuf;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::events;
use plan::{Existing, MatchKind, Plan};

pub use hash::partial_hash;
pub use walk::{Found, Walk, walk};

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
}

/// Scans one library root and records what changed, with events, in one transaction.
pub async fn scan_root(pool: &SqlitePool, root_id: i64) -> Result<ScanReport, ScanError> {
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

    let (plan, report) = tokio::task::spawn_blocking(move || {
        let walk = walk::walk(&root).map_err(|source| ScanError::RootUnreachable {
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
        let plan = plan::plan(existing, walk, &root, hash::partial_hash);
        report.unreadable += plan.unreadable_new;
        Ok((plan, report))
    })
    .await??;

    apply(pool, root_id, plan, report).await
}

async fn apply(
    pool: &SqlitePool,
    root_id: i64,
    plan: Plan,
    mut report: ScanReport,
) -> Result<ScanReport, ScanError> {
    let now = chrono::Utc::now().timestamp();
    let mut tx = pool.begin().await?;

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
                 relative_path = ?, size = ?, mtime = ?, device_id = ?, inode = ?,
                 content_hash = CASE WHEN ? THEN ? ELSE content_hash END,
                 status = CASE WHEN status = 'ignored' THEN 'ignored' ELSE 'present' END,
                 last_seen = ?
             WHERE id = ?",
        )
        .bind(&f.relative_path)
        .bind(f.size)
        .bind(f.mtime)
        .bind(f.device_id)
        .bind(f.inode)
        .bind(new_hash.is_some())
        .bind(new_hash.flatten())
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
                 (root_id, relative_path, size, mtime, device_id, inode, content_hash,
                  first_seen, last_seen)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
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

    tx.commit().await?;
    tracing::info!(root_id, ?report, "scan finished");
    Ok(report)
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

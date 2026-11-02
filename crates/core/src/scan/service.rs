//! Scans as tasks (kind `scan`, one root each), with a schedule and filesystem
//! watching. The task runner keeps scans of a root one at a time and folds
//! requests while one runs into one more scan after it.
//!
//! Only roots that have been scanned before are scanned on a schedule or watched.
//! A new root gets its first scan when it's added; demo roots, which only exist in
//! the database, are never touched.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::json;
use sqlx::SqliteConnection;
use tokio::task::JoinHandle;
use utoipa::ToSchema;

use super::{ProbeRootError, ScanError, probe_root_with_progress, scan_root_with_progress};
use crate::events;
use crate::matcher::match_root_with_progress;
use crate::tasks::{Ended, Finished, Handle, Kind, Running, Scope, Tasks, Trigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Walking the folders and matching files to known ones.
    Walking,
    /// Fingerprinting new files, to recognise moved and copied ones.
    Hashing,
    /// Reading streams from new and changed files.
    Probing,
    /// Identifying folders and linking files to items.
    Matching,
}

impl Phase {
    fn as_str(self) -> &'static str {
        match self {
            Phase::Walking => "walking",
            Phase::Hashing => "hashing",
            Phase::Probing => "probing",
            Phase::Matching => "matching",
        }
    }

    fn of(phase: &str) -> Phase {
        match phase {
            "hashing" => Phase::Hashing,
            "probing" => Phase::Probing,
            "matching" => Phase::Matching,
            _ => Phase::Walking,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ScanProgress {
    pub phase: Phase,
    /// Files found while walking (with no total), files hashed or probed,
    /// or folders matched so far.
    pub done: usize,
    pub total: usize,
    pub started_at: i64,
    pub trigger: String,
    /// Another scan is due when this one finishes.
    pub queued_again: bool,
}

impl From<Running> for ScanProgress {
    fn from(r: Running) -> Self {
        ScanProgress {
            phase: Phase::of(r.progress.phase),
            done: r.progress.done,
            total: r.progress.total,
            started_at: r.started_at,
            trigger: r.trigger.as_str().into(),
            queued_again: r.queued,
        }
    }
}

/// Scans as tasks of kind `scan` on a root, plus their schedule and watching.
#[derive(Clone)]
pub struct ScanService {
    tasks: Tasks,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl ScanService {
    pub fn new(tasks: Tasks) -> ScanService {
        ScanService { tasks }
    }

    /// Starts a scan of `root_id`, or schedules one more after the running one.
    /// Returns true if it started now.
    pub async fn request(&self, root_id: i64, trigger: Trigger) -> Result<bool, sqlx::Error> {
        self.tasks
            .request(Kind::Scan, Scope::Root(root_id), trigger)
            .await
    }

    /// `request`, logging instead of returning a failure to queue the scan.
    async fn request_or_log(&self, root_id: i64, trigger: Trigger) {
        if let Err(e) = self.request(root_id, trigger).await {
            tracing::error!(root_id, error = %e, "can't request a scan");
        }
    }

    pub fn progress(&self, root_id: i64) -> Option<ScanProgress> {
        self.tasks
            .running(Kind::Scan, Scope::Root(root_id))
            .map(ScanProgress::from)
    }

    pub fn all_progress(&self) -> HashMap<i64, ScanProgress> {
        self.tasks
            .all_running()
            .into_iter()
            .filter_map(|(kind, scope, running)| match (kind, scope) {
                (Kind::Scan, Scope::Root(root_id)) => Some((root_id, running.into())),
                _ => None,
            })
            .collect()
    }

    /// Waits until `root_id` has no scan running or queued.
    pub async fn wait_idle(&self, root_id: i64) {
        self.tasks.wait_idle(Kind::Scan, Scope::Root(root_id)).await
    }

    /// Roots that have been scanned before: the ones to rescan and watch.
    async fn known_roots(&self) -> Result<Vec<(i64, String, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT r.id, r.path, max(t.requested_at) FROM library_root r
             JOIN task t ON t.root_id = r.id AND t.kind = 'scan' GROUP BY r.id",
        )
        .fetch_all(self.tasks.pool())
        .await
    }

    /// Rescans each known root once `interval` has passed since its last scan.
    pub fn spawn_scheduler(&self, interval: Duration) -> JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(60));
            loop {
                tick.tick().await;
                let due = now() - interval.as_secs() as i64;
                match service.known_roots().await {
                    Ok(roots) => {
                        for (id, _, last) in roots {
                            if last < due {
                                service.request_or_log(id, Trigger::Scheduled).await;
                            }
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "scheduler can't read roots"),
                }
            }
        })
    }

    /// Watches known, reachable roots and rescans one when its files have been
    /// quiet for `debounce` after a change. Network shares often send no events;
    /// the schedule covers those.
    pub fn spawn_watcher(&self, debounce: Duration) -> JoinHandle<()> {
        let service = self.clone();
        tokio::spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
            let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else {
                    return;
                };
                // Reads (our own probing and hashing included) aren't changes.
                let changed = matches!(
                    event.kind,
                    EventKind::Create(_)
                        | EventKind::Remove(_)
                        | EventKind::Modify(notify::event::ModifyKind::Data(_))
                        | EventKind::Modify(notify::event::ModifyKind::Name(_))
                        | EventKind::Modify(notify::event::ModifyKind::Any)
                );
                if changed {
                    for path in event.paths {
                        let _ = tx.send(path);
                    }
                }
            });
            let mut watcher = match watcher {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!(error = %e, "filesystem watching unavailable; relying on scheduled scans");
                    return;
                }
            };

            let mut watched: HashMap<i64, PathBuf> = HashMap::new();
            let mut pending: HashMap<i64, Instant> = HashMap::new();
            let mut refresh = tokio::time::interval(Duration::from_secs(30));
            let mut flush = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = refresh.tick() => {
                        let Ok(roots) = service.known_roots().await else {
                            continue;
                        };
                        let wanted: HashMap<i64, PathBuf> = roots
                            .into_iter()
                            .map(|(id, path, _)| (id, PathBuf::from(path)))
                            .filter(|(_, p)| p.is_dir())
                            .collect();
                        let stale: Vec<i64> = watched.keys().filter(|id| wanted.get(id) != watched.get(id)).copied().collect();
                        for id in stale {
                            if let Some(path) = watched.remove(&id) {
                                let _ = watcher.unwatch(&path);
                            }
                        }
                        for (id, path) in wanted {
                            if watched.contains_key(&id) {
                                continue;
                            }
                            match watcher.watch(&path, RecursiveMode::Recursive) {
                                Ok(()) => {
                                    tracing::info!(root_id = id, path = %path.display(), "watching for changes");
                                    watched.insert(id, path);
                                }
                                Err(e) => tracing::warn!(root_id = id, error = %e, "can't watch root; relying on scheduled scans"),
                            }
                        }
                    }
                    Some(path) = rx.recv() => {
                        if let Some((id, _)) = watched.iter().find(|(_, root)| path.starts_with(root)) {
                            pending.insert(*id, Instant::now());
                        }
                    }
                    _ = flush.tick() => {
                        let ready: HashSet<i64> = pending
                            .iter()
                            .filter(|(_, at)| at.elapsed() >= debounce)
                            .map(|(id, _)| *id)
                            .collect();
                        for id in ready {
                            pending.remove(&id);
                            service.request_or_log(id, Trigger::Watch).await;
                        }
                    }
                }
            }
        })
    }
}

/// One scan of `root_id`: walk and hash, probe, then match.
pub(crate) async fn run_task(task: &Handle, root_id: i64) -> Result<Finished, sqlx::Error> {
    let pool = task.pool();
    let progress = task.clone();
    let scanned = scan_root_with_progress(pool, root_id, move |phase, done, total| {
        progress.progress(|p| {
            p.phase = phase.as_str();
            p.done = done;
            p.total = total;
        })
    });
    let step = |phase: Phase| {
        task.progress(|p| {
            p.phase = phase.as_str();
            p.done = 0;
            p.total = 0;
        })
    };
    let counted = |done, total| {
        task.progress(|p| {
            p.done = done;
            p.total = total;
        })
    };
    let services = task.services();
    let finished = match scanned.await {
        Ok(scan) => {
            sqlx::query("UPDATE library_root SET offline_since = NULL WHERE id = ?")
                .bind(root_id)
                .execute(pool)
                .await?;
            step(Phase::Probing);
            let probe = probe_root_with_progress(pool, root_id, &services.prober, counted).await;
            let (mut report, mut problem) = match probe {
                Ok(probe) => (json!({ "scan": scan, "probe": probe }), None),
                // Files were still found; only stream details are missing.
                Err(ProbeRootError::Unavailable(e)) => (
                    json!({ "scan": scan }),
                    Some(format!("Files scanned, but not probed: {e}")),
                ),
                Err(e) => (json!({ "scan": scan }), Some(e.to_string())),
            };
            step(Phase::Matching);
            let matched = match_root_with_progress(
                pool,
                root_id,
                &services.provider,
                services.match_settings,
                counted,
            );
            match matched.await {
                Ok(matched) => {
                    if let Some(e) = &matched.provider_error {
                        problem.get_or_insert_with(|| {
                            format!("Files scanned; identifying new folders failed: {e}")
                        });
                    }
                    report["match"] = json!(matched);
                }
                Err(e) => {
                    problem.get_or_insert_with(|| format!("Files scanned; matching failed: {e}"));
                }
            }
            Finished {
                state: Ended::Ok,
                outcome: Some(report),
                error: problem,
            }
        }
        Err(e @ (ScanError::RootUnreachable { .. } | ScanError::RootEmpty { .. })) => {
            sqlx::query("UPDATE library_root SET offline_since = coalesce(offline_since, unixepoch()) WHERE id = ?")
                .bind(root_id)
                .execute(pool)
                .await?;
            Finished {
                state: Ended::Failed,
                outcome: None,
                error: Some(e.to_string()),
            }
        }
        Err(ScanError::RootNotFound(_)) => Finished {
            state: Ended::Cancelled,
            outcome: None,
            error: Some("The folder was removed.".into()),
        },
        Err(e) => Finished {
            state: Ended::Failed,
            outcome: None,
            error: Some(e.to_string()),
        },
    };
    Ok(finished)
}

/// Records the end of a scan, in the same transaction as its task.
pub(crate) async fn finish_task(
    tx: &mut SqliteConnection,
    task_id: i64,
    root_id: i64,
    state: &str,
) -> Result<(), sqlx::Error> {
    crate::wanted::sync_events(tx).await?;
    events::emit(
        tx,
        events::SCAN_FINISHED,
        json!({ "root_id": root_id, "task_id": task_id, "state": state }),
    )
    .await?;
    Ok(())
}

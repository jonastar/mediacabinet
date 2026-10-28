//! Runs scans: one at a time per root, with history, progress, a schedule and
//! filesystem watching.
//!
//! Requests for a root that is already scanning are folded into one more scan
//! after it, so a burst of changes never queues a burst of scans.
//!
//! Only roots that have been scanned before are scanned on a schedule or watched.
//! A new root gets its first scan when it's added; demo roots, which only exist in
//! the database, are never touched.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher};
use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use utoipa::ToSchema;

use super::{ProbeRootError, Prober, ScanError, probe_root_with_progress, scan_root_with_progress};
use crate::events;
use crate::matcher::{MatchSettings, match_root_with_progress};
use crate::metadata::Provider;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Added,
    Manual,
    Scheduled,
    Watch,
}

impl Trigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Trigger::Added => "added",
            Trigger::Manual => "manual",
            Trigger::Scheduled => "scheduled",
            Trigger::Watch => "watch",
        }
    }
}

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

struct Running {
    progress: ScanProgress,
    rerun: Option<Trigger>,
}

struct Inner {
    pool: SqlitePool,
    prober: Prober,
    provider: Provider,
    settings: MatchSettings,
    running: Mutex<HashMap<i64, Running>>,
}

#[derive(Clone)]
pub struct ScanService {
    inner: Arc<Inner>,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

impl ScanService {
    /// Without a metadata provider, scans still link files in folders already
    /// identified; new folders wait until a provider is configured.
    pub fn new(pool: SqlitePool, prober: Prober) -> ScanService {
        ScanService::with_matching(pool, prober, Provider::None, MatchSettings::default())
    }

    pub fn with_matching(
        pool: SqlitePool,
        prober: Prober,
        provider: Provider,
        settings: MatchSettings,
    ) -> ScanService {
        ScanService {
            inner: Arc::new(Inner {
                pool,
                prober,
                provider,
                settings,
                running: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// Marks scans left running by a previous process as failed.
    pub async fn recover(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE scan_run SET state = 'failed', finished_at = unixepoch(),
                 error = 'Interrupted: the server stopped during the scan.'
             WHERE state = 'running'",
        )
        .execute(&self.inner.pool)
        .await?;
        Ok(())
    }

    /// Starts a scan of `root_id`, or schedules one more after the running one.
    /// Returns true if it started now.
    pub fn request(&self, root_id: i64, trigger: Trigger) -> bool {
        let mut running = self.inner.running.lock().unwrap();
        if let Some(r) = running.get_mut(&root_id) {
            r.rerun = Some(trigger);
            r.progress.queued_again = true;
            return false;
        }
        running.insert(
            root_id,
            Running {
                progress: progress(Phase::Walking, trigger),
                rerun: None,
            },
        );
        drop(running);
        let service = self.clone();
        tokio::spawn(async move { service.run(root_id, trigger).await });
        true
    }

    pub fn progress(&self, root_id: i64) -> Option<ScanProgress> {
        self.inner
            .running
            .lock()
            .unwrap()
            .get(&root_id)
            .map(|r| r.progress.clone())
    }

    pub fn all_progress(&self) -> HashMap<i64, ScanProgress> {
        self.inner
            .running
            .lock()
            .unwrap()
            .iter()
            .map(|(id, r)| (*id, r.progress.clone()))
            .collect()
    }

    /// Waits until `root_id` has no scan running or queued.
    pub async fn wait_idle(&self, root_id: i64) {
        while self.progress(root_id).is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    fn set_progress(&self, root_id: i64, update: impl FnOnce(&mut ScanProgress)) {
        if let Some(r) = self.inner.running.lock().unwrap().get_mut(&root_id) {
            update(&mut r.progress);
        }
    }

    async fn run(self, root_id: i64, mut trigger: Trigger) {
        loop {
            if let Err(err) = self.run_once(root_id, trigger).await {
                tracing::error!(root_id, error = %err, "scan bookkeeping failed");
            }
            let mut running = self.inner.running.lock().unwrap();
            let entry = running
                .get_mut(&root_id)
                .expect("running scans stay registered");
            match entry.rerun.take() {
                Some(next) => {
                    trigger = next;
                    entry.progress = progress(Phase::Walking, next);
                }
                None => {
                    running.remove(&root_id);
                    return;
                }
            }
        }
    }

    async fn run_once(&self, root_id: i64, trigger: Trigger) -> Result<(), sqlx::Error> {
        let pool = &self.inner.pool;
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM library_root WHERE id = ?")
            .bind(root_id)
            .fetch_optional(pool)
            .await?;
        if exists.is_none() {
            return Ok(());
        }
        let run_id: i64 = sqlx::query_scalar(
            "INSERT INTO scan_run (root_id, trigger) VALUES (?, ?) RETURNING id",
        )
        .bind(root_id)
        .bind(trigger.as_str())
        .fetch_one(pool)
        .await?;

        let service = self.clone();
        let scanned = scan_root_with_progress(pool, root_id, move |phase, done, total| {
            service.set_progress(root_id, |p| {
                p.phase = phase;
                p.done = done;
                p.total = total;
            })
        });
        let (state, report, error) = match scanned.await {
            Ok(scan) => {
                sqlx::query("UPDATE library_root SET offline_since = NULL WHERE id = ?")
                    .bind(root_id)
                    .execute(pool)
                    .await?;
                self.set_progress(root_id, |p| {
                    p.phase = Phase::Probing;
                    p.done = 0;
                    p.total = 0;
                });
                let probe =
                    probe_root_with_progress(pool, root_id, &self.inner.prober, |done, total| {
                        self.set_progress(root_id, |p| {
                            p.done = done;
                            p.total = total;
                        })
                    })
                    .await;
                let (mut report, mut problem) = match probe {
                    Ok(probe) => (json!({ "scan": scan, "probe": probe }), None),
                    // Files were still found; only stream details are missing.
                    Err(ProbeRootError::Unavailable(e)) => (
                        json!({ "scan": scan }),
                        Some(format!("Files scanned, but not probed: {e}")),
                    ),
                    Err(e) => (json!({ "scan": scan }), Some(e.to_string())),
                };
                self.set_progress(root_id, |p| {
                    p.phase = Phase::Matching;
                    p.done = 0;
                    p.total = 0;
                });
                let matched = match_root_with_progress(
                    pool,
                    root_id,
                    &self.inner.provider,
                    self.inner.settings,
                    |done, total| {
                        self.set_progress(root_id, |p| {
                            p.done = done;
                            p.total = total;
                        })
                    },
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
                        problem
                            .get_or_insert_with(|| format!("Files scanned; matching failed: {e}"));
                    }
                }
                ("ok", Some(report), problem)
            }
            Err(e @ (ScanError::RootUnreachable { .. } | ScanError::RootEmpty { .. })) => {
                sqlx::query("UPDATE library_root SET offline_since = coalesce(offline_since, unixepoch()) WHERE id = ?")
                    .bind(root_id)
                    .execute(pool)
                    .await?;
                ("failed", None, Some(e.to_string()))
            }
            Err(ScanError::RootNotFound(_)) => {
                return Ok(());
            }
            Err(e) => ("failed", None, Some(e.to_string())),
        };

        let mut tx = crate::db::begin_write(pool).await?;
        crate::wanted::sync_events(&mut tx).await?;
        sqlx::query("UPDATE scan_run SET state = ?, finished_at = unixepoch(), report = ?, error = ? WHERE id = ?")
            .bind(state)
            .bind(report.map(|r| r.to_string()))
            .bind(&error)
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
        events::emit(
            &mut tx,
            events::SCAN_FINISHED,
            json!({ "root_id": root_id, "scan_run_id": run_id, "state": state }),
        )
        .await?;
        tx.commit().await?;
        if let Some(error) = error {
            tracing::warn!(root_id, error, "scan finished with a problem");
        }
        Ok(())
    }

    /// Roots that have been scanned before: the ones to rescan and watch.
    async fn known_roots(&self) -> Result<Vec<(i64, String, i64)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT r.id, r.path, max(s.started_at) FROM library_root r
             JOIN scan_run s ON s.root_id = r.id GROUP BY r.id",
        )
        .fetch_all(&self.inner.pool)
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
                                service.request(id, Trigger::Scheduled);
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
                            service.request(id, Trigger::Watch);
                        }
                    }
                }
            }
        })
    }
}

fn progress(phase: Phase, trigger: Trigger) -> ScanProgress {
    ScanProgress {
        phase,
        done: 0,
        total: 0,
        started_at: now(),
        trigger: trigger.as_str().into(),
        queued_again: false,
    }
}

//! Background work: one runner for every kind of task, with history, progress
//! and coalescing. Design: `docs/tasks.md`.
//!
//! A task's work is computed from current state when it runs, so a task left
//! running by a stopped server just runs again. One task per kind and scope runs
//! at a time, with at most one more queued after it; requests while one is
//! queued are folded into that one, so a burst of requests never queues a burst
//! of runs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};
use tokio::sync::{Semaphore, watch};
use tracing::Instrument;

use crate::matcher::MatchSettings;
use crate::metadata::Provider;
use crate::metadata::omdb::Omdb;
use crate::scan::{self, Prober};
use crate::thumbnails::Thumbnailer;

pub mod history;
pub mod log;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Scan,
    MetadataRefresh,
    Thumbnails,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Scan => "scan",
            Kind::MetadataRefresh => "metadata_refresh",
            Kind::Thumbnails => "thumbnails",
        }
    }

    pub fn parse(kind: &str) -> Option<Kind> {
        match kind {
            "scan" => Some(Kind::Scan),
            "metadata_refresh" => Some(Kind::MetadataRefresh),
            "thumbnails" => Some(Kind::Thumbnails),
            _ => None,
        }
    }

    /// Heavy tasks run one at a time and give way to playback.
    fn is_heavy(self) -> bool {
        self == Kind::Thumbnails
    }

    /// Whether it stops when asked once it's running. A scan runs to the end.
    pub fn can_cancel(self) -> bool {
        matches!(self, Kind::MetadataRefresh | Kind::Thumbnails)
    }
}

/// What a task works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Root(i64),
    Library(i64),
}

impl Scope {
    /// The task row's `root_id` and `library_id`.
    fn columns(self) -> (Option<i64>, Option<i64>) {
        match self {
            Scope::Root(id) => (Some(id), None),
            Scope::Library(id) => (None, Some(id)),
        }
    }

    fn from_columns(root_id: Option<i64>, library_id: Option<i64>) -> Option<Scope> {
        root_id.map(Scope::Root).or(library_id.map(Scope::Library))
    }
}

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

    pub fn parse(trigger: &str) -> Option<Trigger> {
        match trigger {
            "added" => Some(Trigger::Added),
            "manual" => Some(Trigger::Manual),
            "scheduled" => Some(Trigger::Scheduled),
            "watch" => Some(Trigger::Watch),
            _ => None,
        }
    }
}

/// How far a running task is. Kept in memory only.
#[derive(Debug, Clone, Default)]
pub struct Progress {
    /// The kind's own step name; empty before the first update.
    pub phase: &'static str,
    pub done: usize,
    /// Zero while unknown.
    pub total: usize,
}

/// A task running now.
#[derive(Debug, Clone)]
pub struct Running {
    pub task_id: i64,
    pub trigger: Trigger,
    pub started_at: i64,
    pub progress: Progress,
    /// Another run of the same kind and scope is due after this one.
    pub queued: bool,
    /// Asked to stop, and will at the next item.
    pub cancelling: bool,
}

/// How a task ended.
pub struct Finished {
    pub state: Ended,
    pub outcome: Option<Value>,
    /// Why it failed, or a problem worth showing even though it finished.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Ok,
    Failed,
    Cancelled,
}

impl Ended {
    fn as_str(self) -> &'static str {
        match self {
            Ended::Ok => "ok",
            Ended::Failed => "failed",
            Ended::Cancelled => "cancelled",
        }
    }
}

/// What tasks need from the rest of the server.
#[derive(Clone)]
pub struct Services {
    pub prober: Prober,
    /// Without a provider, scans still link files in folders already identified;
    /// new folders wait until one is configured.
    pub provider: Provider,
    pub match_settings: MatchSettings,
    /// Ratings, refreshed along with metadata when set.
    pub omdb: Option<Omdb>,
    /// Without this, scans don't ask for thumbnails.
    pub thumbnailer: Option<Thumbnailer>,
    /// Whether anything is being transcoded for playback now; heavy tasks wait.
    pub transcoding: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Default for Services {
    fn default() -> Self {
        Services {
            prober: Prober::default(),
            provider: Provider::None,
            match_settings: MatchSettings::default(),
            omdb: None,
            thumbnailer: None,
            transcoding: Arc::new(|| false),
        }
    }
}

type Key = (Kind, Scope);

struct Slot {
    running: Running,
    queued: Option<(i64, Trigger)>,
    /// Turns true to stop the running task.
    cancel: watch::Sender<bool>,
}

/// What came of asking to stop a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancel {
    /// It hadn't started, and now won't.
    Cancelled,
    /// It stops at the next item.
    Stopping,
    /// It's running, and this kind runs to the end.
    CantStop,
    /// It isn't queued or running.
    NotOpen,
}

struct Inner {
    pool: SqlitePool,
    services: Services,
    slots: Mutex<HashMap<Key, Slot>>,
    /// Held while a request decides whether to start, queue or fold, so two
    /// requests can't both queue a run.
    requesting: tokio::sync::Mutex<()>,
    /// The heavy lane: taken for as long as a heavy task runs.
    heavy: Semaphore,
}

#[derive(Clone)]
pub struct Tasks {
    inner: Arc<Inner>,
}

/// A running task's view of the runner.
#[derive(Clone)]
pub struct Handle {
    tasks: Tasks,
    key: Key,
    id: i64,
    trigger: Trigger,
    cancel: watch::Receiver<bool>,
}

impl Handle {
    pub fn id(&self) -> i64 {
        self.id
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.tasks.inner.pool
    }

    pub fn services(&self) -> &Services {
        &self.tasks.inner.services
    }

    pub fn progress(&self, update: impl FnOnce(&mut Progress)) {
        if let Some(slot) = self.tasks.inner.slots.lock().unwrap().get_mut(&self.key) {
            update(&mut slot.running.progress);
        }
    }

    pub fn trigger(&self) -> Trigger {
        self.trigger
    }

    /// Whether someone asked this task to stop; check between items.
    pub fn cancelled(&self) -> bool {
        *self.cancel.borrow()
    }

    /// Asks for another task.
    pub async fn request(
        &self,
        kind: Kind,
        scope: Scope,
        trigger: Trigger,
    ) -> Result<bool, sqlx::Error> {
        self.tasks.request(kind, scope, trigger).await
    }

    /// Returns once nothing is being transcoded, or the task is told to stop.
    /// Heavy tasks call this before each item.
    pub async fn wait_for_playback(&self) {
        let transcoding = &self.services().transcoding;
        if !transcoding() {
            return;
        }
        tracing::info!("waiting for playback to finish");
        self.progress(|p| p.phase = "waiting for playback");
        while transcoding() && !self.cancelled() {
            tokio::time::sleep(PLAYBACK_CHECK).await;
        }
    }
}

/// How often a heavy task looks whether playback has finished.
const PLAYBACK_CHECK: Duration = Duration::from_secs(1);

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn running(task_id: i64, trigger: Trigger) -> Running {
    Running {
        task_id,
        trigger,
        started_at: now(),
        progress: Progress::default(),
        queued: false,
        cancelling: false,
    }
}

impl Tasks {
    pub fn new(pool: SqlitePool, services: Services) -> Tasks {
        Tasks {
            inner: Arc::new(Inner {
                pool,
                services,
                slots: Mutex::new(HashMap::new()),
                requesting: tokio::sync::Mutex::new(()),
                heavy: Semaphore::new(1),
            }),
        }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    /// Runs tasks a previous process left running or queued.
    pub async fn resume(&self) -> Result<(), sqlx::Error> {
        let _requesting = self.inner.requesting.lock().await;
        let pool = &self.inner.pool;
        sqlx::query("UPDATE task SET state = 'queued', started_at = NULL WHERE state = 'running'")
            .execute(pool)
            .await?;
        type Open = (i64, String, Option<i64>, Option<i64>, String);
        let open: Vec<Open> = sqlx::query_as(
            "SELECT id, kind, root_id, library_id, trigger FROM task
             WHERE state = 'queued' ORDER BY id",
        )
        .fetch_all(pool)
        .await?;
        for (id, kind, root_id, library_id, trigger) in open {
            let parsed = Kind::parse(&kind)
                .zip(Scope::from_columns(root_id, library_id))
                .zip(Trigger::parse(&trigger));
            let Some((key, trigger)) = parsed else {
                sqlx::query(
                    "UPDATE task SET state = 'cancelled', finished_at = unixepoch(),
                         error = 'This version of Media Cabinet can''t run this task.'
                     WHERE id = ?",
                )
                .bind(id)
                .execute(pool)
                .await?;
                continue;
            };
            let folded = self
                .inner
                .slots
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|s| s.queued.is_some());
            if folded {
                // Already one queued: it covers this one.
                sqlx::query("DELETE FROM task WHERE id = ?")
                    .bind(id)
                    .execute(pool)
                    .await?;
                continue;
            }
            self.start_or_queue(key, id, trigger);
        }
        Ok(())
    }

    /// Runs `kind` on `scope` now, or once more after the one running. Returns
    /// true if it started now.
    pub async fn request(
        &self,
        kind: Kind,
        scope: Scope,
        trigger: Trigger,
    ) -> Result<bool, sqlx::Error> {
        let key = (kind, scope);
        let _requesting = self.inner.requesting.lock().await;
        let queued = self
            .inner
            .slots
            .lock()
            .unwrap()
            .get(&key)
            .is_some_and(|s| s.queued.is_some());
        if queued {
            return Ok(false);
        }
        let (root_id, library_id) = scope.columns();
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO task (kind, root_id, library_id, trigger) VALUES (?, ?, ?, ?)
             RETURNING id",
        )
        .bind(kind.as_str())
        .bind(root_id)
        .bind(library_id)
        .bind(trigger.as_str())
        .fetch_one(&self.inner.pool)
        .await?;
        Ok(self.start_or_queue(key, id, trigger))
    }

    /// Callers hold `requesting` and have checked nothing is queued for `key`.
    fn start_or_queue(&self, key: Key, id: i64, trigger: Trigger) -> bool {
        let mut slots = self.inner.slots.lock().unwrap();
        if let Some(slot) = slots.get_mut(&key) {
            slot.queued = Some((id, trigger));
            slot.running.queued = true;
            return false;
        }
        slots.insert(
            key,
            Slot {
                running: running(id, trigger),
                queued: None,
                cancel: watch::Sender::new(false),
            },
        );
        drop(slots);
        tokio::spawn(self.clone().run(key));
        true
    }

    pub fn running(&self, kind: Kind, scope: Scope) -> Option<Running> {
        self.inner
            .slots
            .lock()
            .unwrap()
            .get(&(kind, scope))
            .map(|s| s.running.clone())
    }

    /// Every task running now, with its kind and scope.
    pub fn all_running(&self) -> Vec<(Kind, Scope, Running)> {
        self.inner
            .slots
            .lock()
            .unwrap()
            .iter()
            .map(|((kind, scope), s)| (*kind, *scope, s.running.clone()))
            .collect()
    }

    /// Stops a task: one that hasn't started won't, one running stops at its next item.
    pub async fn cancel(&self, id: i64) -> Result<Cancel, sqlx::Error> {
        let _requesting = self.inner.requesting.lock().await;
        {
            let mut slots = self.inner.slots.lock().unwrap();
            let found = slots.iter_mut().find(|(_, slot)| {
                slot.running.task_id == id || slot.queued.is_some_and(|(queued, _)| queued == id)
            });
            let Some(((kind, _), slot)) = found else {
                return Ok(Cancel::NotOpen);
            };
            if slot.running.task_id == id {
                if !kind.can_cancel() {
                    return Ok(Cancel::CantStop);
                }
                slot.cancel.send_replace(true);
                slot.running.cancelling = true;
                return Ok(Cancel::Stopping);
            }
            slot.queued = None;
            slot.running.queued = false;
        }
        sqlx::query("UPDATE task SET state = 'cancelled', finished_at = unixepoch() WHERE id = ?")
            .bind(id)
            .execute(&self.inner.pool)
            .await?;
        Ok(Cancel::Cancelled)
    }

    /// Waits until nothing of `kind` on `scope` is running or queued.
    pub async fn wait_idle(&self, kind: Kind, scope: Scope) {
        while self.running(kind, scope).is_some() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn run(self, key: Key) {
        loop {
            let id = self.inner.slots.lock().unwrap()[&key].running.task_id;
            if let Err(err) = self.run_one(key, id).await {
                tracing::error!(task_id = id, kind = key.0.as_str(), error = %err, "task bookkeeping failed");
            }
            let mut slots = self.inner.slots.lock().unwrap();
            let slot = slots.get_mut(&key).expect("running tasks stay registered");
            match slot.queued.take() {
                Some((next, trigger)) => {
                    slot.running = running(next, trigger);
                    slot.cancel = watch::Sender::new(false);
                }
                None => {
                    slots.remove(&key);
                    return;
                }
            }
        }
    }

    /// One run, in a span that sends what it logs to the task's log.
    async fn run_one(&self, key: Key, id: i64) -> Result<(), sqlx::Error> {
        // At error level, so a quieter log filter doesn't lose the span and with
        // it the warnings logged inside.
        let span = tracing::error_span!(log::SPAN_NAME, task_id = id, kind = key.0.as_str());
        self.run_in_span(key, id).instrument(span).await
    }

    async fn run_in_span(&self, key: Key, id: i64) -> Result<(), sqlx::Error> {
        let pool = &self.inner.pool;
        let (kind, scope) = key;
        let (trigger, mut cancel) = {
            let slots = self.inner.slots.lock().unwrap();
            let slot = &slots[&key];
            (slot.running.trigger, slot.cancel.subscribe())
        };
        // Stays queued while it waits for the heavy lane. It can be cancelled
        // meanwhile, however long the task ahead of it takes.
        let _turn = if kind.is_heavy() {
            tokio::select! {
                turn = self.inner.heavy.acquire() => Some(turn.expect("the lane is never closed")),
                _ = cancel.wait_for(|cancelled| *cancelled) => None,
            }
        } else {
            None
        };
        let finished = if *cancel.borrow() {
            Finished {
                state: Ended::Cancelled,
                outcome: None,
                error: None,
            }
        } else {
            let started = sqlx::query(
                "UPDATE task SET state = 'running', started_at = unixepoch()
                 WHERE id = ? AND state = 'queued'",
            )
            .bind(id)
            .execute(pool)
            .await?;
            if started.rows_affected() == 0 {
                // Removed along with its scope.
                return Ok(());
            }
            tracing::info!(trigger = trigger.as_str(), "task started");
            let handle = Handle {
                tasks: self.clone(),
                key,
                id,
                trigger,
                cancel,
            };
            let ran = match (kind, scope) {
                (Kind::Scan, Scope::Root(root_id)) => scan::run_task(&handle, root_id).await,
                (Kind::MetadataRefresh, Scope::Library(library_id)) => {
                    crate::metadata::refresh::run_task(&handle, library_id).await
                }
                (Kind::Thumbnails, Scope::Library(library_id)) => {
                    crate::thumbnails::run_task(&handle, library_id).await
                }
                _ => Ok(Finished {
                    state: Ended::Cancelled,
                    outcome: None,
                    error: Some(format!("A {} task can't run on {scope:?}.", kind.as_str())),
                }),
            };
            ran.unwrap_or_else(|e| Finished {
                state: Ended::Failed,
                outcome: None,
                error: Some(e.to_string()),
            })
        };

        let mut tx = crate::db::begin_write(pool).await?;
        sqlx::query(
            "UPDATE task SET state = ?, finished_at = unixepoch(), outcome = ?, error = ?
             WHERE id = ?",
        )
        .bind(finished.state.as_str())
        .bind(finished.outcome.as_ref().map(Value::to_string))
        .bind(&finished.error)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        finish(&mut tx, key, id, &finished).await?;
        if let Some(entry) = activity(key, id, trigger, &finished) {
            crate::activity::record(&mut tx, &entry).await?;
        }
        tx.commit().await?;
        if let Some(error) = &finished.error {
            tracing::warn!(
                task_id = id,
                kind = kind.as_str(),
                error,
                "task finished with a problem"
            );
        }
        tracing::info!(state = finished.state.as_str(), "task finished");
        Ok(())
    }
}

/// The activity entry for a task's end, for admins: failures, problems, and
/// tasks someone asked for. Routine runs that went fine aren't news.
fn activity(
    (kind, scope): Key,
    id: i64,
    trigger: Trigger,
    finished: &Finished,
) -> Option<crate::activity::NewEntry> {
    let failed = finished.state == Ended::Failed;
    if !failed && finished.error.is_none() && trigger != Trigger::Manual {
        return None;
    }
    let (root_id, library_id) = scope.columns();
    Some(crate::activity::NewEntry::admins(
        if failed {
            crate::activity::TASK_FAILED
        } else {
            crate::activity::TASK_FINISHED
        },
        now(),
        None,
        serde_json::json!({
            "task_id": id,
            "task_kind": kind.as_str(),
            "trigger": trigger.as_str(),
            "state": finished.state.as_str(),
            "error": finished.error,
            "root_id": root_id,
            "library_id": library_id,
        }),
    ))
}

/// Each kind's bookkeeping in the transaction that records the end of a task.
async fn finish(
    tx: &mut SqliteConnection,
    (kind, scope): Key,
    id: i64,
    finished: &Finished,
) -> Result<(), sqlx::Error> {
    match (kind, scope) {
        (Kind::Scan, Scope::Root(root_id)) => {
            scan::finish_task(tx, id, root_id, finished.state.as_str()).await
        }
        // Each refreshed item recorded its own changes as it went.
        _ => Ok(()),
    }
}

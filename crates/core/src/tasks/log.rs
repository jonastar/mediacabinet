//! Task-scoped logs: a tracing layer copies events inside a task's span to
//! `task_log`, for the task inspector. Capped per task, keeping warnings and
//! errors over info, so a sweep that warns per file can't fill the database.

use std::collections::BTreeSet;
use std::fmt::Debug;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use sqlx::SqlitePool;
use tokio::sync::mpsc;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Lines kept per task.
const KEPT: i64 = 500;
/// A message longer than this is cut.
const LONGEST: usize = 2000;
/// Lines waiting to be written; past this they're dropped rather than held in memory.
const WAITING: usize = 10_000;
/// How long finished tasks keep their logs.
const RETENTION_SECONDS: i64 = 30 * 24 * 3600;

/// The name of the span tasks run in.
pub(super) const SPAN_NAME: &str = "task";

pub struct Line {
    task_id: i64,
    at: i64,
    level: &'static str,
    message: String,
    fields: Option<String>,
}

/// The layer to add to the subscriber, and the lines for `spawn_writer`.
pub fn layer() -> (TaskLogLayer, mpsc::Receiver<Line>) {
    let (lines, receiver) = mpsc::channel(WAITING);
    (TaskLogLayer { lines }, receiver)
}

pub struct TaskLogLayer {
    lines: mpsc::Sender<Line>,
}

struct TaskSpan(i64);

#[derive(Default)]
struct TaskIdVisitor(Option<i64>);

impl Visit for TaskIdVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        if field.name() == "task_id" {
            self.0 = Some(value);
        }
    }

    fn record_debug(&mut self, _field: &Field, _value: &dyn Debug) {}
}

#[derive(Default)]
struct LineVisitor {
    message: String,
    fields: Map<String, Value>,
}

impl LineVisitor {
    fn record(&mut self, field: &Field, value: Value) {
        self.fields.insert(field.name().to_string(), value);
    }
}

impl Visit for LineVisitor {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record(field, value.into());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record(field, value.into());
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record(field, value.into());
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.record(field, value.into());
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.record(field, format!("{value:?}").into());
        }
    }
}

impl<S> Layer<S> for TaskLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        if attrs.metadata().name() != SPAN_NAME {
            return;
        }
        let mut visitor = TaskIdVisitor::default();
        attrs.record(&mut visitor);
        if let (Some(task_id), Some(span)) = (visitor.0, ctx.span(id)) {
            span.extensions_mut().insert(TaskSpan(task_id));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let level = match *event.metadata().level() {
            Level::ERROR => "error",
            Level::WARN => "warn",
            Level::INFO => "info",
            _ => {
                return;
            }
        };
        let Some(scope) = ctx.event_scope(event) else {
            return;
        };
        let task_id = scope
            .from_root()
            .find_map(|span| span.extensions().get::<TaskSpan>().map(|t| t.0));
        let Some(task_id) = task_id else {
            return;
        };
        let mut visitor = LineVisitor::default();
        event.record(&mut visitor);
        let mut message = visitor.message;
        if let Some((cut, _)) = message.char_indices().nth(LONGEST) {
            message.truncate(cut);
        }
        let fields =
            (!visitor.fields.is_empty()).then(|| Value::Object(visitor.fields).to_string());
        // Never blocks: a line is dropped if the database has fallen this far behind.
        let _ = self.lines.try_send(Line {
            task_id,
            at: chrono::Utc::now().timestamp(),
            level,
            message,
            fields,
        });
    }
}

async fn write(pool: &SqlitePool, lines: &[Line]) -> Result<(), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let mut tasks = BTreeSet::new();
    for line in lines {
        tasks.insert(line.task_id);
        // The task may have gone with its scope.
        sqlx::query(
            "INSERT INTO task_log (task_id, at, level, message, fields)
             SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM task WHERE id = ?1)",
        )
        .bind(line.task_id)
        .bind(line.at)
        .bind(line.level)
        .bind(&line.message)
        .bind(&line.fields)
        .execute(&mut *tx)
        .await?;
    }
    for task in tasks {
        sqlx::query(
            "DELETE FROM task_log WHERE id IN (
                 SELECT id FROM task_log WHERE task_id = ?1
                 ORDER BY CASE level WHEN 'info' THEN 0 WHEN 'warn' THEN 1 ELSE 2 END, id
                 LIMIT max(0, (SELECT count(*) FROM task_log WHERE task_id = ?1) - ?2))",
        )
        .bind(task)
        .bind(KEPT)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

async fn prune(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM task_log WHERE task_id IN (
             SELECT id FROM task WHERE finished_at < unixepoch() - ?)",
    )
    .bind(RETENTION_SECONDS)
    .execute(pool)
    .await?;
    Ok(())
}

/// Writes logged lines to the database, in batches, and drops old tasks' logs.
pub fn spawn_writer(
    pool: SqlitePool,
    mut lines: mpsc::Receiver<Line>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut pruned: Option<Instant> = None;
        let mut batch = Vec::new();
        while lines.recv_many(&mut batch, 200).await > 0 {
            if let Err(e) = write(&pool, &batch).await {
                tracing::warn!(error = %e, "can't write task logs");
            }
            batch.clear();
            if pruned.is_none_or(|at| at.elapsed() > Duration::from_secs(3600)) {
                if let Err(e) = prune(&pool).await {
                    tracing::warn!(error = %e, "can't remove old task logs");
                }
                pruned = Some(Instant::now());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    })
}

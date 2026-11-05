//! Tasks as the API shows them: what's in the table, with the progress of those
//! running now.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::{Kind, Tasks};

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskProgress {
    /// The kind's own step name; empty before the first update.
    pub phase: String,
    pub done: usize,
    /// Zero while unknown.
    pub total: usize,
    /// Asked to stop, and will at the next item.
    pub cancelling: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskSummary {
    pub id: i64,
    /// `scan`, `metadata_refresh` or `thumbnails`.
    pub kind: String,
    /// What asked for it: `added`, `manual`, `scheduled` or `watch`.
    pub trigger: String,
    /// `queued`, `running`, `ok`, `failed` or `cancelled`.
    pub state: String,
    pub library_id: Option<i64>,
    /// The library it works on, or its root's.
    pub library_name: Option<String>,
    pub root_id: Option<i64>,
    pub root_path: Option<String>,
    pub requested_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
    /// While it has a place in the runner, including while it waits for its turn.
    pub progress: Option<TaskProgress>,
    pub cancellable: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskLogLine {
    pub at: i64,
    /// `info`, `warn` or `error`.
    pub level: String,
    pub message: String,
    #[schema(value_type = Option<Object>)]
    pub fields: Option<Value>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskDetail {
    #[serde(flatten)]
    pub task: TaskSummary,
    /// What it did, per kind. `failures` lists what went wrong, when there's a list.
    #[schema(value_type = Option<Object>)]
    pub outcome: Option<Value>,
    pub log: Vec<TaskLogLine>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TaskPage {
    /// Queued and running, newest first.
    pub open: Vec<TaskSummary>,
    /// Newest first.
    pub finished: Vec<TaskSummary>,
    /// There are more finished tasks than `limit`.
    pub has_more: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TaskFilter {
    pub kind: Option<String>,
    /// For finished tasks: `ok`, `failed` or `cancelled`.
    pub state: Option<String>,
    pub limit: i64,
}

const SELECT: &str =
    "SELECT t.id, t.kind, t.trigger, t.state, coalesce(t.library_id, r.library_id),
        l.name, t.root_id, r.path, t.requested_at, t.started_at, t.finished_at, t.error
     FROM task t
     LEFT JOIN library_root r ON r.id = t.root_id
     LEFT JOIN library l ON l.id = coalesce(t.library_id, r.library_id)";

type Row = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
    Option<i64>,
    Option<String>,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
);

fn summary(row: Row, progress: &HashMap<i64, TaskProgress>) -> TaskSummary {
    let (id, kind, trigger, state, library_id, library_name, root_id, root_path, ..) = row.clone();
    let (.., requested_at, started_at, finished_at, error) = row;
    let can_cancel = Kind::parse(&kind).is_some_and(Kind::can_cancel);
    let cancellable = state == "queued" || (state == "running" && can_cancel);
    TaskSummary {
        progress: progress.get(&id).cloned(),
        id,
        kind,
        trigger,
        state,
        library_id,
        library_name,
        root_id,
        root_path,
        requested_at,
        started_at,
        finished_at,
        error,
        cancellable,
    }
}

impl Tasks {
    fn progress_by_task(&self) -> HashMap<i64, TaskProgress> {
        self.all_running()
            .into_iter()
            .map(|(_, _, running)| {
                let progress = TaskProgress {
                    phase: running.progress.phase.to_string(),
                    done: running.progress.done,
                    total: running.progress.total,
                    cancelling: running.cancelling,
                };
                (running.task_id, progress)
            })
            .collect()
    }

    /// What's open, and the latest finished tasks matching `filter`.
    pub async fn page(&self, filter: &TaskFilter) -> Result<TaskPage, sqlx::Error> {
        let pool = self.pool();
        let progress = self.progress_by_task();
        let open: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE t.state IN ('queued', 'running')
               AND (?1 IS NULL OR t.kind = ?1)
             ORDER BY t.id DESC"
        )))
        .bind(&filter.kind)
        .fetch_all(pool)
        .await?;
        let mut finished: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE t.state NOT IN ('queued', 'running')
               AND (?1 IS NULL OR t.kind = ?1)
               AND (?2 IS NULL OR t.state = ?2)
             ORDER BY t.id DESC LIMIT ?3"
        )))
        .bind(&filter.kind)
        .bind(&filter.state)
        .bind(filter.limit + 1)
        .fetch_all(pool)
        .await?;
        let has_more = finished.len() as i64 > filter.limit;
        finished.truncate(filter.limit as usize);
        let summaries = |rows: Vec<Row>| -> Vec<TaskSummary> {
            rows.into_iter().map(|r| summary(r, &progress)).collect()
        };
        Ok(TaskPage {
            open: summaries(open),
            finished: summaries(finished),
            has_more,
        })
    }

    /// One task, with its outcome and log.
    pub async fn detail(&self, id: i64) -> Result<Option<TaskDetail>, sqlx::Error> {
        let pool = self.pool();
        let row: Option<Row> =
            sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE t.id = ?")))
                .bind(id)
                .fetch_optional(pool)
                .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let outcome: Option<String> = sqlx::query_scalar("SELECT outcome FROM task WHERE id = ?")
            .bind(id)
            .fetch_one(pool)
            .await?;
        let log: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT at, level, message, fields FROM task_log WHERE task_id = ? ORDER BY id",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;
        let json = |text: Option<String>| text.and_then(|t| serde_json::from_str(&t).ok());
        Ok(Some(TaskDetail {
            task: summary(row, &self.progress_by_task()),
            outcome: json(outcome),
            log: log
                .into_iter()
                .map(|(at, level, message, fields)| TaskLogLine {
                    at,
                    level,
                    message,
                    fields: json(fields),
                })
                .collect(),
        }))
    }
}

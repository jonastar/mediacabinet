//! Keeps metadata current: refreshes series and movies from their primary
//! provider on a schedule, and on request, one item or a whole library.
//!
//! Continuing series refresh daily; ended and cancelled series, and movies,
//! weekly. A failed refresh keeps the metadata already there, records the error,
//! and is retried next cycle. Works offline: nothing breaks while the provider is
//! unreachable.

use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use utoipa::ToSchema;

use super::import::{ImportReport, import_movie, record_refresh_error, refresh_series};
use super::{MetadataError, Provider, omdb};
use crate::tasks::{Ended, Finished, Handle, Running};

const DAY: i64 = 24 * 3600;

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    #[error("not found")]
    NotFound,
    /// The item's primary provider isn't the configured one.
    #[error("{0}")]
    WrongProvider(String),
    #[error(transparent)]
    Provider(#[from] MetadataError),
    #[error("{0}")]
    Import(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Refreshes one series or movie now.
pub async fn refresh_item(
    pool: &SqlitePool,
    provider: &Provider,
    item: i64,
) -> Result<ImportReport, RefreshError> {
    // Kind, library, primary provider and its ID.
    type Row = (String, Option<i64>, Option<String>, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "SELECT i.kind, i.library_id, i.primary_provider,
                (SELECT value FROM external_id WHERE item_id = i.id AND is_primary)
         FROM item i WHERE i.id = ? AND i.kind IN ('series', 'movie')",
    )
    .bind(item)
    .fetch_optional(pool)
    .await?;
    let Some((kind, library, primary, id)) = row else {
        return Err(RefreshError::NotFound);
    };
    let (Some(primary), Some(id)) = (primary, id) else {
        return Err(RefreshError::WrongProvider(
            "It has no primary provider ID to refresh from.".into(),
        ));
    };
    if primary != provider.name() {
        return Err(RefreshError::WrongProvider(format!(
            "Its metadata comes from {primary}, which isn't configured."
        )));
    }
    let Some(library) = library else {
        return Err(RefreshError::Import("It isn't in a library.".into()));
    };

    let result = if kind == "series" {
        match provider.series(&id).await {
            Ok(data) => refresh_series(pool, item, &data)
                .await
                .map_err(|e| RefreshError::Import(e.to_string())),
            Err(e) => Err(e.into()),
        }
    } else {
        match provider.movie(&id).await {
            Ok(data) => import_movie(pool, library, &data)
                .await
                .map_err(|e| RefreshError::Import(e.to_string())),
            Err(e) => Err(e.into()),
        }
    };
    match &result {
        Ok(_) => crate::wanted::sync_after_change(pool).await,
        Err(e) => record_refresh_error(pool, item, &e.to_string()).await?,
    }
    result
}

/// Series and movies due a refresh, oldest first.
pub async fn due(pool: &SqlitePool, provider: &str, now: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT i.id FROM item i LEFT JOIN series s ON s.item_id = i.id
         WHERE i.kind IN ('series', 'movie') AND i.primary_provider = ?
           AND coalesce(i.refreshed_at, 0) < CASE
               WHEN i.kind = 'series' AND coalesce(s.status, 'continuing') = 'continuing' THEN ? - ?
               ELSE ? - 7 * ?
           END
         ORDER BY coalesce(i.refreshed_at, 0)",
    )
    .bind(provider)
    .bind(now)
    .bind(DAY)
    .bind(now)
    .bind(DAY)
    .fetch_all(pool)
    .await
}

/// Refreshes due items every `every`, one at a time.
pub fn spawn_refresher(pool: SqlitePool, provider: Provider, every: Duration) -> JoinHandle<()> {
    tokio::spawn(async move {
        if !provider.is_configured() {
            return;
        }
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            let now = chrono::Utc::now().timestamp();
            let items = match due(&pool, provider.name(), now).await {
                Ok(items) => items,
                Err(e) => {
                    tracing::error!(error = %e, "can't list items due a refresh");
                    continue;
                }
            };
            for item in items {
                match refresh_item(&pool, &provider, item).await {
                    Ok(report) => tracing::debug!(item, ?report, "refreshed"),
                    Err(RefreshError::Provider(MetadataError::Unreachable(e))) => {
                        // Offline or rate limited: stop this round, try again next tick.
                        tracing::warn!(error = e, "metadata provider unreachable; refresh paused");
                        break;
                    }
                    Err(e) => tracing::warn!(item, error = %e, "refresh failed"),
                }
            }
        }
    })
}

/// A library refresh in progress.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RefreshProgress {
    /// Series and movies refreshed so far, of `total`.
    pub done: usize,
    pub total: usize,
    pub started_at: i64,
    pub trigger: String,
    /// Another refresh is due when this one finishes.
    pub queued_again: bool,
}

impl From<Running> for RefreshProgress {
    fn from(r: Running) -> Self {
        RefreshProgress {
            done: r.progress.done,
            total: r.progress.total,
            started_at: r.started_at,
            trigger: r.trigger.as_str().into(),
            queued_again: r.queued,
        }
    }
}

/// Failures kept in a task's outcome; the rest are only counted.
const FAILURES_KEPT: usize = 200;

/// Refreshes every series and movie in `library` from its provider, with its
/// ratings when OMDb is set up. Items refreshed since the task was requested are
/// skipped, so a task interrupted by a restart carries on where it stopped.
pub(crate) async fn run_task(task: &Handle, library: i64) -> Result<Finished, sqlx::Error> {
    let pool = task.pool();
    let services = task.services();
    let provider = &services.provider;
    if !provider.is_configured() {
        return Ok(Finished {
            state: Ended::Failed,
            outcome: None,
            error: Some(MetadataError::NotConfigured.to_string()),
        });
    }
    let requested_at: i64 = sqlx::query_scalar("SELECT requested_at FROM task WHERE id = ?")
        .bind(task.id())
        .fetch_one(pool)
        .await?;
    // Items whose metadata comes from another provider can't be refreshed here.
    let skipped: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM item WHERE library_id = ? AND kind IN ('series', 'movie')
           AND coalesce(primary_provider, '') <> ?",
    )
    .bind(library)
    .bind(provider.name())
    .fetch_one(pool)
    .await?;
    let items: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, title FROM item WHERE library_id = ? AND kind IN ('series', 'movie')
           AND primary_provider = ? AND coalesce(refreshed_at, 0) <= ?
         ORDER BY title COLLATE NOCASE, id",
    )
    .bind(library)
    .bind(provider.name())
    .bind(requested_at)
    .fetch_all(pool)
    .await?;
    task.progress(|p| {
        p.phase = "refreshing";
        p.done = 0;
        p.total = items.len();
    });

    let mut refreshed = 0;
    let mut failures = Vec::new();
    let mut failed = 0;
    let mut omdb = services.omdb.as_ref();
    let mut problem = None;
    let mut cancelled = false;
    for (done, (item, title)) in items.iter().enumerate() {
        if task.cancelled() {
            cancelled = true;
            break;
        }
        match refresh_item(pool, provider, *item).await {
            Ok(_) => refreshed += 1,
            Err(RefreshError::Provider(MetadataError::Unreachable(e))) => {
                // Offline or rate limited: the rest would fail the same way.
                problem = Some(format!(
                    "The metadata provider is unreachable ({e}); {refreshed} of {} refreshed.",
                    items.len()
                ));
                break;
            }
            Err(RefreshError::Db(e)) => return Err(e),
            Err(e) => {
                failed += 1;
                if failures.len() < FAILURES_KEPT {
                    failures
                        .push(json!({ "item_id": item, "title": title, "error": e.to_string() }));
                }
            }
        }
        // Ratings are a bonus: stop asking once OMDb is unavailable or over its limit.
        if let Some(client) = omdb {
            match omdb::refresh(pool, client, *item).await {
                Ok(()) => {}
                Err(MetadataError::Unreachable(e) | MetadataError::Auth(e)) => {
                    tracing::warn!(error = e, "OMDb unavailable; skipping ratings for the rest");
                    omdb = None;
                }
                Err(e) => tracing::warn!(item, error = %e, "can't refresh ratings"),
            }
        }
        task.progress(|p| p.done = done + 1);
    }

    let outcome = json!({
        "refreshed": refreshed,
        "failed": failed,
        "skipped": skipped,
        "failures": failures,
    });
    let error = problem.clone().or_else(|| {
        (failed > 0).then(|| format!("{failed} of {} couldn't be refreshed.", items.len()))
    });
    Ok(Finished {
        state: if problem.is_some() {
            Ended::Failed
        } else if cancelled {
            Ended::Cancelled
        } else {
            Ended::Ok
        },
        outcome: Some(outcome),
        error,
    })
}

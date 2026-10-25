//! Keeps metadata current: refreshes series and movies from their primary
//! provider on a schedule, and on request.
//!
//! Continuing series refresh daily; ended and cancelled series, and movies,
//! weekly. A failed refresh keeps the metadata already there, records the error,
//! and is retried next cycle. Works offline: nothing breaks while the provider is
//! unreachable.

use std::time::Duration;

use sqlx::SqlitePool;
use tokio::task::JoinHandle;

use super::import::{ImportReport, import_movie, record_refresh_error, refresh_series};
use super::{MetadataError, Provider};

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
    let row: Option<(String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT i.kind, i.primary_provider,
                (SELECT value FROM external_id WHERE item_id = i.id AND is_primary)
         FROM item i WHERE i.id = ? AND i.kind IN ('series', 'movie')",
    )
    .bind(item)
    .fetch_optional(pool)
    .await?;
    let Some((kind, primary, id)) = row else {
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

    let result = if kind == "series" {
        match provider.series(&id).await {
            Ok(data) => refresh_series(pool, item, &data)
                .await
                .map_err(|e| RefreshError::Import(e.to_string())),
            Err(e) => Err(e.into()),
        }
    } else {
        match provider.movie(&id).await {
            Ok(data) => import_movie(pool, &data)
                .await
                .map_err(|e| RefreshError::Import(e.to_string())),
            Err(e) => Err(e.into()),
        }
    };
    if let Err(e) = &result {
        record_refresh_error(pool, item, &e.to_string()).await?;
    }
    result
}

/// Series and movies due a refresh, oldest first.
pub async fn due(pool: &SqlitePool, provider: &str, now: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM item
         WHERE kind IN ('series', 'movie') AND primary_provider = ?
           AND coalesce(refreshed_at, 0) < CASE
               WHEN kind = 'series' AND coalesce(series_status, 'continuing') = 'continuing' THEN ? - ?
               ELSE ? - 7 * ?
           END
         ORDER BY coalesce(refreshed_at, 0)",
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

//! Per-user watch state: resume positions, played, play counts.
//!
//! Players report progress as they go. Past 90% of the duration an item counts
//! as played: the play count goes up once and the resume position clears. Under
//! 30 seconds isn't worth resuming from, so it's stored as the start. Reports
//! also make play sessions, which become watch activity (`activity::sessions`).

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::activity;

pub const PLAYED_FRACTION: f64 = 0.9;
pub const MIN_RESUME_SECONDS: f64 = 30.0;

#[derive(Debug, thiserror::Error)]
pub enum WatchError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Default, Serialize, ToSchema, sqlx::FromRow)]
pub struct ItemWatch {
    pub item_id: i64,
    pub position_seconds: f64,
    pub played: bool,
    pub play_count: i64,
    pub last_played_at: Option<i64>,
}

pub async fn get(pool: &SqlitePool, user: i64, item: i64) -> Result<ItemWatch, sqlx::Error> {
    let row: Option<ItemWatch> = sqlx::query_as(
        "SELECT item_id, position_seconds, played, play_count, last_played_at
         FROM user_item_state WHERE user_id = ? AND item_id = ?",
    )
    .bind(user)
    .bind(item)
    .fetch_optional(pool)
    .await?;
    Ok(row.unwrap_or(ItemWatch {
        item_id: item,
        ..Default::default()
    }))
}

async fn kind(pool: &SqlitePool, item: i64) -> Result<String, WatchError> {
    sqlx::query_scalar("SELECT kind FROM item WHERE id = ?")
        .bind(item)
        .fetch_optional(pool)
        .await?
        .ok_or(WatchError::NotFound)
}

/// The longest probed duration among the item's present files.
async fn file_duration(pool: &SqlitePool, item: i64) -> Result<Option<f64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT max(f.duration_seconds) FROM file_link l JOIN media_file f ON f.id = l.file_id
         WHERE l.item_id = ? AND f.status = 'present'",
    )
    .bind(item)
    .fetch_one(pool)
    .await
}

/// Records how far into an episode or movie the user is. `duration` is what the
/// player knows; without it the file's probed duration is used.
pub async fn report_progress(
    pool: &SqlitePool,
    user: i64,
    item: i64,
    position: f64,
    duration: Option<f64>,
    now: i64,
) -> Result<ItemWatch, WatchError> {
    if !position.is_finite() || position < 0.0 {
        return Err(WatchError::Invalid(
            "The position must be zero or more seconds.".into(),
        ));
    }
    let kind = kind(pool, item).await?;
    if kind != "episode" && kind != "movie" {
        return Err(WatchError::Invalid(
            "Only episodes and movies are played.".into(),
        ));
    }
    let duration = match duration.filter(|d| d.is_finite() && *d > 0.0) {
        Some(d) => Some(d),
        None => file_duration(pool, item).await?,
    };
    let current = get(pool, user, item).await?;
    let finished = duration.is_some_and(|d| position >= d * PLAYED_FRACTION);
    let mut tx = crate::db::begin_write(pool).await?;
    if finished {
        // Reports keep coming through the credits; count the play once.
        let already_counted = current.played && current.position_seconds == 0.0;
        sqlx::query(
            "INSERT INTO user_item_state (user_id, item_id, position_seconds, played, play_count, last_played_at)
             VALUES (?1, ?2, 0, 1, 1, ?3)
             ON CONFLICT (user_id, item_id) DO UPDATE SET position_seconds = 0, played = 1,
                 play_count = play_count + ?4, last_played_at = ?3",
        )
        .bind(user)
        .bind(item)
        .bind(now)
        .bind(i64::from(!already_counted))
        .execute(&mut *tx)
        .await?;
    } else {
        let position = if position < MIN_RESUME_SECONDS {
            0.0
        } else {
            position
        };
        sqlx::query(
            "INSERT INTO user_item_state (user_id, item_id, position_seconds, last_played_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (user_id, item_id) DO UPDATE SET position_seconds = ?3, last_played_at = ?4",
        )
        .bind(user)
        .bind(item)
        .bind(position)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    }
    activity::sessions::report(&mut tx, user, item, position, duration, finished, now).await?;
    tx.commit().await?;
    Ok(get(pool, user, item).await?)
}

/// Which of a series' episodes to mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    All,
    /// An ordering season of the series.
    Season(i64),
    /// Episodes of the series.
    Episodes(Vec<i64>),
}

/// Marks an episode or movie, or a series' episodes (all, one season of an
/// ordering, or a list), as played or not. Returns how many items changed.
pub async fn set_played(
    pool: &SqlitePool,
    user: i64,
    item: i64,
    scope: Scope,
    played: bool,
    now: i64,
) -> Result<u64, WatchError> {
    let kind = kind(pool, item).await?;
    let targets: Vec<i64> = match (kind.as_str(), scope) {
        ("series", Scope::All) => {
            sqlx::query_scalar("SELECT id FROM item WHERE parent_id = ? AND kind = 'episode'")
                .bind(item)
                .fetch_all(pool)
                .await?
        }
        ("series", Scope::Season(season)) => {
            let targets: Vec<i64> = sqlx::query_scalar(
                "SELECT DISTINCT e.item_id FROM ordering_entry e
                 JOIN ordering o ON o.id = e.ordering_id
                 WHERE e.ordering_season_id = ? AND o.series_id = ?",
            )
            .bind(season)
            .bind(item)
            .fetch_all(pool)
            .await?;
            if targets.is_empty() {
                return Err(WatchError::Invalid(
                    "That season isn't part of this series.".into(),
                ));
            }
            targets
        }
        ("series", Scope::Episodes(episodes)) => {
            let targets: Vec<i64> = sqlx::query_scalar(
                "SELECT id FROM item WHERE parent_id = ? AND kind = 'episode'
                 AND id IN (SELECT value FROM json_each(?))",
            )
            .bind(item)
            .bind(serde_json::json!(episodes).to_string())
            .fetch_all(pool)
            .await?;
            if targets.len() != episodes.len() {
                return Err(WatchError::Invalid(
                    "Those aren't all episodes of this series.".into(),
                ));
            }
            targets
        }
        ("episode" | "movie", Scope::All) => vec![item],
        _ => {
            return Err(WatchError::Invalid(
                "Seasons and episodes only apply to a series.".into(),
            ));
        }
    };
    let mut tx = crate::db::begin_write(pool).await?;
    let mut changed = 0;
    for target in targets {
        let done = if played {
            sqlx::query(
                "INSERT INTO user_item_state (user_id, item_id, played, play_count, last_played_at)
                 VALUES (?1, ?2, 1, 1, ?3)
                 ON CONFLICT (user_id, item_id) DO UPDATE SET played = 1, position_seconds = 0,
                     play_count = max(play_count, 1), last_played_at = coalesce(last_played_at, ?3)
                 WHERE NOT played OR position_seconds > 0",
            )
            .bind(user)
            .bind(target)
            .bind(now)
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "UPDATE user_item_state SET played = 0, position_seconds = 0
                 WHERE user_id = ? AND item_id = ? AND (played OR position_seconds > 0)",
            )
            .bind(user)
            .bind(target)
            .execute(&mut *tx)
            .await?
        };
        changed += done.rows_affected();
    }
    tx.commit().await?;
    Ok(changed)
}

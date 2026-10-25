//! Wanted items, the calendar, and custom status from external tools.
//!
//! What's wanted is defined by the `item_wanted` view (see migration 0004). This
//! module lists it, and turns changes in it into `item.wanted` / `item.unwanted`
//! events by comparing with `wanted_snapshot`, the set as of the last events.

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};
use utoipa::{IntoParams, ToSchema};

use crate::catalog::{Availability, EpisodeNumber, ExternalStatus, NumberStyle};
use crate::events;

#[derive(Debug, thiserror::Error)]
pub enum WantedError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Emits `item.wanted` / `item.unwanted` for items whose wanted state changed since
/// the last call, and records the new set. Call it inside the transaction that made
/// the change, or periodically for changes time makes (episodes airing).
pub async fn sync_events(conn: &mut SqliteConnection) -> Result<(usize, usize), sqlx::Error> {
    let became: Vec<(i64, Option<i64>, String)> = sqlx::query_as(
        "SELECT item_id, parent_id, reason FROM item_wanted
         WHERE item_id NOT IN (SELECT item_id FROM wanted_snapshot) ORDER BY item_id",
    )
    .fetch_all(&mut *conn)
    .await?;
    let stopped: Vec<i64> = sqlx::query_scalar(
        "SELECT item_id FROM wanted_snapshot
         WHERE item_id NOT IN (SELECT item_id FROM item_wanted) ORDER BY item_id",
    )
    .fetch_all(&mut *conn)
    .await?;
    for (item, series, reason) in &became {
        events::emit(
            conn,
            events::ITEM_WANTED,
            json!({ "item_id": item, "series_id": series, "reason": reason }),
        )
        .await?;
        sqlx::query("INSERT INTO wanted_snapshot (item_id) VALUES (?)")
            .bind(item)
            .execute(&mut *conn)
            .await?;
    }
    for item in &stopped {
        events::emit(conn, events::ITEM_UNWANTED, json!({ "item_id": item })).await?;
        sqlx::query("DELETE FROM wanted_snapshot WHERE item_id = ?")
            .bind(item)
            .execute(&mut *conn)
            .await?;
    }
    Ok((became.len(), stopped.len()))
}

/// `sync_events` in its own transaction.
pub async fn sync(pool: &SqlitePool) -> Result<(usize, usize), sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let changes = sync_events(&mut tx).await?;
    tx.commit().await?;
    Ok(changes)
}

/// Syncs wanted events every `every`, for changes time makes: an episode airing
/// becomes wanted without anything else happening.
pub fn spawn_syncer(pool: SqlitePool, every: std::time::Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(every);
        loop {
            tick.tick().await;
            if let Err(e) = sync(&pool).await {
                tracing::error!(error = %e, "can't sync wanted events");
            }
        }
    })
}

/// Syncs after a change, logging rather than failing it: the change itself is
/// done, and the periodic sync catches up if this one fails.
pub async fn sync_after_change(pool: &SqlitePool) {
    if let Err(e) = sync(pool).await {
        tracing::warn!(error = %e, "can't sync wanted events; the periodic sync will");
    }
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct WantedFilter {
    /// Only this library.
    pub library_id: Option<i64>,
    /// Only this series (its episodes).
    pub series_id: Option<i64>,
    /// `missing`, `not_acquired` or `upgrade`.
    pub reason: Option<String>,
    /// `episode` or `movie`.
    pub kind: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WantedItem {
    pub item_id: i64,
    /// `episode` or `movie`.
    pub kind: String,
    pub title: String,
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    /// Number in the series' default ordering.
    pub number: Option<EpisodeNumber>,
    pub airs_at: Option<i64>,
    pub airs_time_known: bool,
    pub availability: Availability,
    /// `missing`, `not_acquired` or `upgrade`.
    pub reason: String,
    pub quality_profile: Option<String>,
    /// Height of the best present file (upgrades), and the cutoff it must reach.
    pub best_height: Option<i64>,
    pub cutoff_height: Option<i64>,
    /// Current external statuses (stale ones included, flagged).
    pub statuses: Vec<ExternalStatus>,
    /// Provider IDs, so acquisition tools can look the item up.
    pub external_ids: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct WantedPage {
    pub items: Vec<WantedItem>,
    pub total: i64,
}

#[derive(sqlx::FromRow)]
struct WantedRow {
    item_id: i64,
    kind: String,
    title: String,
    series_id: Option<i64>,
    series_title: Option<String>,
    season: Option<i64>,
    number: Option<i64>,
    airs_at: Option<i64>,
    airs_tz: Option<String>,
    availability: String,
    reason: String,
    quality_profile: Option<String>,
    best_height: Option<i64>,
    cutoff_height: Option<i64>,
}

/// The library's wanted items, oldest air date first.
pub async fn list(
    pool: &SqlitePool,
    filter: &WantedFilter,
    now: i64,
) -> Result<WantedPage, WantedError> {
    let limit = filter.limit.unwrap_or(100).clamp(1, 1000);
    let offset = filter.offset.unwrap_or(0).max(0);
    if let Some(r) = &filter.reason
        && !["missing", "not_acquired", "upgrade"].contains(&r.as_str())
    {
        return Err(WantedError::Invalid(
            "reason is missing, not_acquired or upgrade".into(),
        ));
    }
    // An episode belongs to its series' libraries: where the series has a stored
    // folder, or where any of its episodes has a file. A movie, where its files are.
    const WHERE: &str = "
        WHERE (?1 IS NULL OR w.parent_id = ?1)
          AND (?2 IS NULL OR w.reason = ?2)
          AND (?3 IS NULL OR w.kind = ?3)
          AND (?4 IS NULL
            OR EXISTS (
              SELECT 1 FROM series_location sl JOIN library_root r ON r.id = sl.root_id
              WHERE sl.series_item_id = w.parent_id AND r.library_id = ?4)
            OR EXISTS (
              SELECT 1 FROM file_link l
              JOIN item x ON x.id = l.item_id
              JOIN media_file f ON f.id = l.file_id
              JOIN library_root r ON r.id = f.root_id
              WHERE r.library_id = ?4
                AND (x.id = w.item_id OR (w.parent_id IS NOT NULL AND x.parent_id = w.parent_id))))";
    let total: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT count(*) FROM item_wanted w {WHERE}"
    )))
    .bind(filter.series_id)
    .bind(&filter.reason)
    .bind(&filter.kind)
    .bind(filter.library_id)
    .fetch_one(pool)
    .await?;
    let rows: Vec<WantedRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT w.item_id, w.kind, i.title, s.id AS series_id, s.title AS series_title,
                os.number AS season, oe.episode_number AS number, i.airs_at, i.airs_tz,
                w.availability, w.reason, qp.name AS quality_profile, w.best_height, w.cutoff_height
         FROM item_wanted w
         JOIN item i ON i.id = w.item_id
         LEFT JOIN item s ON s.id = w.parent_id
         LEFT JOIN ordering o ON o.series_id = s.id AND o.is_default
         LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = i.id
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         LEFT JOIN quality_profile qp ON qp.id = w.quality_profile_id
         {WHERE}
         GROUP BY w.item_id
         ORDER BY i.airs_at IS NULL, i.airs_at, s.title, oe.position
         LIMIT ?5 OFFSET ?6"
    )))
    .bind(filter.series_id)
    .bind(&filter.reason)
    .bind(&filter.kind)
    .bind(filter.library_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for r in rows {
        items.push(WantedItem {
            statuses: statuses(pool, r.item_id, now).await?,
            external_ids: sqlx::query_as(
                "SELECT provider, value FROM external_id WHERE item_id = ? ORDER BY is_primary DESC, provider",
            )
            .bind(r.series_id.unwrap_or(r.item_id))
            .fetch_all(pool)
            .await?,
            item_id: r.item_id,
            kind: r.kind,
            title: r.title,
            series_id: r.series_id,
            series_title: r.series_title,
            number: number(r.season, r.number),
            airs_at: r.airs_at,
            airs_time_known: r.airs_tz.is_some(),
            availability: Availability::parse(&r.availability),
            reason: r.reason,
            quality_profile: r.quality_profile,
            best_height: r.best_height,
            cutoff_height: r.cutoff_height,
        });
    }
    Ok(WantedPage { items, total })
}

fn number(season: Option<i64>, number: Option<i64>) -> Option<EpisodeNumber> {
    number.map(|n| EpisodeNumber {
        style: if season.is_some() {
            NumberStyle::Season
        } else {
            NumberStyle::Absolute
        },
        season,
        number: Some(n),
    })
}

async fn statuses(
    pool: &SqlitePool,
    item: i64,
    now: i64,
) -> Result<Vec<ExternalStatus>, sqlx::Error> {
    let rows: Vec<(String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT source, status, detail, updated_at, expires_at FROM item_status
         WHERE item_id = ? ORDER BY updated_at DESC",
    )
    .bind(item)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(source, status, detail, updated_at, expires_at)| ExternalStatus {
                source,
                status,
                detail: detail.and_then(|d| serde_json::from_str(&d).ok()),
                updated_at,
                expires_at,
                stale: expires_at <= now,
            },
        )
        .collect())
}

// ── Calendar ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CalendarEntry {
    pub item_id: i64,
    /// `episode` or `movie`.
    pub kind: String,
    pub title: String,
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    pub number: Option<EpisodeNumber>,
    pub airs_at: i64,
    pub airs_time_known: bool,
    pub availability: Availability,
    pub monitored: bool,
    pub wanted: bool,
}

#[derive(sqlx::FromRow)]
struct CalendarRow {
    item_id: i64,
    kind: String,
    title: String,
    series_id: Option<i64>,
    series_title: Option<String>,
    season: Option<i64>,
    number: Option<i64>,
    airs_at: i64,
    airs_tz: Option<String>,
    availability: String,
    monitored: bool,
    wanted: bool,
}

/// Episodes and movie releases between `from` and `to` (Unix seconds), by air time.
pub async fn calendar(
    pool: &SqlitePool,
    from: i64,
    to: i64,
) -> Result<Vec<CalendarEntry>, WantedError> {
    if to <= from || to - from > 400 * 86400 {
        return Err(WantedError::Invalid(
            "Choose a range of up to 400 days, with from before to.".into(),
        ));
    }
    let rows: Vec<CalendarRow> = sqlx::query_as(
        "SELECT i.id AS item_id, i.kind, i.title, s.id AS series_id, s.title AS series_title,
                os.number AS season, oe.episode_number AS number, i.airs_at, i.airs_tz,
                st.availability, st.monitored,
                EXISTS (SELECT 1 FROM item_wanted w WHERE w.item_id = i.id) AS wanted
             FROM item i
             JOIN item_state st ON st.item_id = i.id
             LEFT JOIN item s ON s.id = i.parent_id
             LEFT JOIN ordering o ON o.series_id = s.id AND o.is_default
             LEFT JOIN ordering_entry oe ON oe.ordering_id = o.id AND oe.item_id = i.id
             LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
             WHERE i.airs_at >= ? AND i.airs_at < ?
             GROUP BY i.id
             ORDER BY i.airs_at, s.title, oe.position",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| CalendarEntry {
            item_id: r.item_id,
            kind: r.kind,
            title: r.title,
            series_id: r.series_id,
            series_title: r.series_title,
            number: number(r.season, r.number),
            airs_at: r.airs_at,
            airs_time_known: r.airs_tz.is_some(),
            availability: Availability::parse(&r.availability),
            monitored: r.monitored,
            wanted: r.wanted,
        })
        .collect())
}

// ── Custom status ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct StatusUpdate {
    /// Free text. Well-known values the UI styles: searching, queued, downloading,
    /// importing, failed.
    pub status: String,
    /// JSON detail, e.g. `{"progress": 0.4, "eta_seconds": 540}`.
    #[schema(value_type = Option<Object>)]
    pub detail: Option<serde_json::Value>,
    /// Seconds until the status counts as stale. Default one hour, at most a week.
    pub expires_in_seconds: Option<i64>,
}

/// Sets an external tool's status for an item. Statuses expire so a crashed tool
/// can't leave "downloading" forever.
pub async fn set_status(
    pool: &SqlitePool,
    item: i64,
    source: &str,
    update: &StatusUpdate,
) -> Result<(), WantedError> {
    if source.trim().is_empty() || update.status.trim().is_empty() {
        return Err(WantedError::Invalid(
            "Source and status are required.".into(),
        ));
    }
    let ttl = update
        .expires_in_seconds
        .unwrap_or(3600)
        .clamp(60, 7 * 86400);
    let exists: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM item WHERE id = ?)")
        .bind(item)
        .fetch_one(pool)
        .await?;
    if !exists {
        return Err(WantedError::NotFound);
    }
    let mut tx = crate::db::begin_write(pool).await?;
    sqlx::query(
        "INSERT INTO item_status (item_id, source, status, detail, updated_at, expires_at)
         VALUES (?1, ?2, ?3, ?4, unixepoch(), unixepoch() + ?5)
         ON CONFLICT (item_id, source) DO UPDATE SET
             status = excluded.status, detail = excluded.detail,
             updated_at = excluded.updated_at, expires_at = excluded.expires_at",
    )
    .bind(item)
    .bind(source.trim())
    .bind(update.status.trim())
    .bind(update.detail.as_ref().map(|d| d.to_string()))
    .bind(ttl)
    .execute(&mut *tx)
    .await?;
    events::emit(
        &mut tx,
        events::STATUS_CHANGED,
        json!({ "item_id": item, "source": source, "status": update.status, "detail": update.detail }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_status(pool: &SqlitePool, item: i64, source: &str) -> Result<(), WantedError> {
    let mut tx = crate::db::begin_write(pool).await?;
    let done = sqlx::query("DELETE FROM item_status WHERE item_id = ? AND source = ?")
        .bind(item)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    if done.rows_affected() == 0 {
        return Err(WantedError::NotFound);
    }
    events::emit(
        &mut tx,
        events::STATUS_CHANGED,
        json!({ "item_id": item, "source": source, "status": null }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Sets a series' or movie's quality profile, and for a series which playlist
/// wanted follows (None: all episodes).
pub async fn set_monitoring(
    pool: &SqlitePool,
    item: i64,
    quality_profile_id: Option<Option<i64>>,
    wanted_playlist: Option<Option<i64>>,
) -> Result<(), WantedError> {
    let kind: String = sqlx::query_scalar("SELECT kind FROM item WHERE id = ?")
        .bind(item)
        .fetch_optional(pool)
        .await?
        .ok_or(WantedError::NotFound)?;
    let mut tx = crate::db::begin_write(pool).await?;
    if let Some(profile) = quality_profile_id {
        sqlx::query(
            "INSERT INTO monitor (item_id, quality_profile_id) VALUES (?1, ?2)
             ON CONFLICT (item_id) DO UPDATE SET quality_profile_id = ?2",
        )
        .bind(item)
        .bind(profile)
        .execute(&mut *tx)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_foreign_key_violation() => {
                WantedError::Invalid("No such quality profile.".into())
            }
            _ => e.into(),
        })?;
    }
    if let Some(playlist) = wanted_playlist {
        if kind != "series" {
            return Err(WantedError::Invalid(
                "Only a series can follow a playlist.".into(),
            ));
        }
        if let Some(p) = playlist {
            let belongs: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM playlist pl LEFT JOIN ordering o ON o.id = pl.base_ordering_id
                 WHERE pl.id = ?1 AND (o.series_id = ?2 OR EXISTS (
                     SELECT 1 FROM playlist_entry pe JOIN item e ON e.id = pe.item_id
                     WHERE pe.playlist_id = pl.id AND e.parent_id = ?2)))",
            )
            .bind(p)
            .bind(item)
            .fetch_one(&mut *tx)
            .await?;
            if !belongs {
                return Err(WantedError::Invalid(
                    "That playlist isn't for this series.".into(),
                ));
            }
        }
        sqlx::query("UPDATE item SET wanted_playlist_id = ? WHERE id = ?")
            .bind(playlist)
            .bind(item)
            .execute(&mut *tx)
            .await?;
    }
    sync_events(&mut tx).await?;
    tx.commit().await?;
    Ok(())
}

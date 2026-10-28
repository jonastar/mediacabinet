//! The event outbox. Events are written in the same transaction as the change
//! they describe, then delivered to consumers from a cursor.

use serde_json::Value;
use sqlx::SqliteConnection;

pub const FILE_ADDED: &str = "file.added";
pub const FILE_CHANGED: &str = "file.changed";
pub const FILE_IGNORED: &str = "file.ignored";
pub const FILE_MISSING: &str = "file.missing";
pub const FILE_MOVED: &str = "file.moved";
pub const FILE_REMOVED: &str = "file.removed";
pub const FILE_RESTORED: &str = "file.restored";
pub const FILE_UPGRADED: &str = "file.upgraded";
pub const ITEM_ADDED: &str = "item.added";
pub const ITEM_EDITED: &str = "item.edited";
pub const ITEM_REFRESHED: &str = "item.refreshed";
pub const ITEM_UNWANTED: &str = "item.unwanted";
pub const ITEM_WANTED: &str = "item.wanted";
pub const LIBRARY_ADDED: &str = "library.added";
pub const LINK_CHANGED: &str = "link.changed";
pub const LINK_CREATED: &str = "link.created";
pub const LINK_REMOVED: &str = "link.removed";
pub const MATCH_NEEDS_REVIEW: &str = "match.needs_review";
pub const LIBRARY_REMOVED: &str = "library.removed";
pub const ROOT_ADDED: &str = "root.added";
pub const ROOT_CHANGED: &str = "root.changed";
pub const ROOT_REMOVED: &str = "root.removed";
pub const SCAN_FINISHED: &str = "scan.finished";
pub const SERIES_MOVED: &str = "series.moved";
pub const SERIES_REFRESHED: &str = "series.refreshed";
pub const STATUS_CHANGED: &str = "status.changed";

pub async fn emit(
    conn: &mut SqliteConnection,
    kind: &str,
    payload: Value,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("INSERT INTO event (type, payload) VALUES (?, ?) RETURNING id")
        .bind(kind)
        .bind(payload.to_string())
        .fetch_one(conn)
        .await
}

#[derive(Debug, Clone, serde::Serialize, utoipa::ToSchema)]
pub struct EventRecord {
    /// Increasing, never reused: a consumer's cursor.
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    #[schema(value_type = Object)]
    pub payload: Value,
    pub created_at: i64,
}

fn record((id, kind, payload, created_at): (i64, String, String, i64)) -> EventRecord {
    EventRecord {
        id,
        kind,
        payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
        created_at,
    }
}

/// Events after `cursor`, oldest first, optionally only some types.
pub async fn since(
    pool: &sqlx::SqlitePool,
    cursor: i64,
    limit: i64,
    types: &[String],
) -> Result<Vec<EventRecord>, sqlx::Error> {
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, type, payload, created_at FROM event
         WHERE id > ?1 AND (?2 = '[]' OR type IN (SELECT value FROM json_each(?2)))
         ORDER BY id LIMIT ?3",
    )
    .bind(cursor)
    .bind(serde_json::json!(types).to_string())
    .bind(limit.clamp(1, 1000))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(record).collect())
}

/// An item's history, newest first: its own events and those of the files linked
/// to it now. Earlier files show up through the item's own link events.
pub async fn for_item(
    pool: &sqlx::SqlitePool,
    item: i64,
    limit: i64,
) -> Result<Vec<EventRecord>, sqlx::Error> {
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as(
        "SELECT id, type, payload, created_at FROM event
         WHERE json_extract(payload, '$.item_id') = ?1
         UNION
         SELECT id, type, payload, created_at FROM event
         -- + drops file_id's affinity, so the comparison can use event_file.
         WHERE json_extract(payload, '$.file_id') IN (SELECT +file_id FROM file_link WHERE item_id = ?1)
         ORDER BY id DESC LIMIT ?2",
    )
    .bind(item)
    .bind(limit.clamp(1, 500))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(record).collect())
}

/// The newest event's ID, or 0: where a new consumer starts to see only new events.
pub async fn latest(pool: &sqlx::SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT coalesce(max(id), 0) FROM event")
        .fetch_one(pool)
        .await
}

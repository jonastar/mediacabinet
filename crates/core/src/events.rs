//! The event outbox. Events are written in the same transaction as the change
//! they describe, then delivered to consumers from a cursor.

use serde_json::Value;
use sqlx::SqliteConnection;

pub const FILE_ADDED: &str = "file.added";
pub const FILE_CHANGED: &str = "file.changed";
pub const FILE_MISSING: &str = "file.missing";
pub const FILE_MOVED: &str = "file.moved";
pub const FILE_RESTORED: &str = "file.restored";
pub const ITEM_UNWANTED: &str = "item.unwanted";
pub const ITEM_WANTED: &str = "item.wanted";
pub const LIBRARY_ADDED: &str = "library.added";
pub const LIBRARY_REMOVED: &str = "library.removed";
pub const ROOT_ADDED: &str = "root.added";
pub const ROOT_CHANGED: &str = "root.changed";
pub const ROOT_REMOVED: &str = "root.removed";
pub const SCAN_FINISHED: &str = "scan.finished";

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

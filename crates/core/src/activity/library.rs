//! Library additions, read from the event outbox from the log's own cursor.
//!
//! An episode or movie counts as added when it gets its first file link ever;
//! later links (upgrades, a second version) aren't news. Additions to the same
//! series or movie close together in time, as one scan or import makes them,
//! grow one entry instead of adding more. A folder library's videos aren't
//! announced: its first scan could add thousands of unrelated clips, one entry each.

use std::collections::{BTreeMap, btree_map};

use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

use super::{Audience, ITEMS_ADDED, NewEntry};
use crate::events;

const CONSUMER: &str = "activity";
const BATCH: i64 = 500;
/// Additions this close to an entry's latest one join it.
pub const GROUP_WINDOW: i64 = 30 * 60;

/// One entry's worth of additions: a series' episodes, or a movie.
#[derive(Default)]
struct Group {
    library: i64,
    items: Vec<i64>,
    events: Vec<i64>,
    at: i64,
    /// The series had no episode linked before these.
    new_series: bool,
}

/// Turns new `link.created` events into `items_added` entries. Returns how many
/// events it read.
pub async fn sync(pool: &SqlitePool) -> Result<usize, sqlx::Error> {
    let mut read = 0;
    loop {
        let mut tx = crate::db::begin_write(pool).await?;
        let cursor: i64 = sqlx::query_scalar("SELECT cursor FROM event_consumer WHERE name = ?")
            .bind(CONSUMER)
            .fetch_optional(&mut *tx)
            .await?
            .unwrap_or(0);
        let batch: Vec<(i64, i64, i64)> = sqlx::query_as(
            "SELECT id, json_extract(payload, '$.item_id'), created_at FROM event
             WHERE id > ? AND type = ? AND json_type(payload, '$.item_id') = 'integer'
             ORDER BY id LIMIT ?",
        )
        .bind(cursor)
        .bind(events::LINK_CREATED)
        .bind(BATCH)
        .fetch_all(&mut *tx)
        .await?;
        // Without a full batch, everything up to the newest event has been seen;
        // the write lock means no earlier event can still arrive.
        let next = if batch.len() as i64 == BATCH {
            batch.last().map_or(cursor, |e| e.0)
        } else {
            sqlx::query_scalar("SELECT coalesce(max(id), 0) FROM event")
                .fetch_one(&mut *tx)
                .await?
        };
        let mut groups: BTreeMap<i64, Group> = BTreeMap::new();
        for &(event, item, at) in &batch {
            add(&mut tx, &mut groups, event, item, at).await?;
        }
        for (owner, group) in groups {
            write(&mut tx, owner, group).await?;
        }
        sqlx::query(
            "INSERT INTO event_consumer (name, cursor) VALUES (?1, ?2)
             ON CONFLICT (name) DO UPDATE SET cursor = ?2",
        )
        .bind(CONSUMER)
        .bind(next)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        read += batch.len();
        if (batch.len() as i64) < BATCH {
            return Ok(read);
        }
    }
}

async fn add(
    conn: &mut SqliteConnection,
    groups: &mut BTreeMap<i64, Group>,
    event: i64,
    item: i64,
    at: i64,
) -> Result<(), sqlx::Error> {
    let first_link: bool = sqlx::query_scalar(
        "SELECT NOT EXISTS (SELECT 1 FROM event
                            WHERE json_extract(payload, '$.item_id') = ?1 AND type = ?2 AND id < ?3)",
    )
    .bind(item)
    .bind(events::LINK_CREATED)
    .bind(event)
    .fetch_one(&mut *conn)
    .await?;
    if !first_link {
        return Ok(());
    }
    let row: Option<(String, Option<i64>, i64)> = sqlx::query_as(
        "SELECT i.kind, i.parent_id, i.library_id FROM item i
         JOIN library l ON l.id = i.library_id WHERE i.id = ? AND l.kind <> 'folders'",
    )
    .bind(item)
    .fetch_optional(&mut *conn)
    .await?;
    let (owner, library) = match row {
        Some((kind, Some(series), library)) if kind == "episode" => (series, library),
        Some((kind, None, library)) if kind == "movie" => (item, library),
        // Gone since, in a folder library, or not something people watch.
        _ => return Ok(()),
    };
    let group = match groups.entry(owner) {
        btree_map::Entry::Occupied(group) => group.into_mut(),
        btree_map::Entry::Vacant(slot) => {
            let new_series = owner != item && first_in_series(conn, owner, event).await?;
            slot.insert(Group {
                library,
                new_series,
                ..Default::default()
            })
        }
    };
    if !group.items.contains(&item) {
        group.items.push(item);
    }
    group.events.push(event);
    group.at = group.at.max(at);
    Ok(())
}

/// Whether no episode of the series was linked before `event`.
async fn first_in_series(
    conn: &mut SqliteConnection,
    series: i64,
    event: i64,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT NOT EXISTS (
             SELECT 1 FROM item i JOIN event e
                 ON json_extract(e.payload, '$.item_id') = i.id AND e.type = ?2 AND e.id < ?3
             WHERE i.parent_id = ?1)",
    )
    .bind(series)
    .bind(events::LINK_CREATED)
    .bind(event)
    .fetch_one(conn)
    .await
}

/// Grows the owner's latest entry if it's recent enough, or writes a new one.
async fn write(conn: &mut SqliteConnection, owner: i64, group: Group) -> Result<(), sqlx::Error> {
    let recent: Option<(i64, String, Option<String>)> = sqlx::query_as(
        "SELECT id, payload, event_ids FROM activity
         WHERE item_id = ? AND kind = ? AND at >= ? ORDER BY at DESC LIMIT 1",
    )
    .bind(owner)
    .bind(ITEMS_ADDED)
    .bind(group.at - GROUP_WINDOW)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((id, payload, event_ids)) = recent else {
        super::record(
            conn,
            &NewEntry {
                kind: ITEMS_ADDED,
                at: group.at,
                actor: None,
                subject: None,
                library: Some(group.library),
                item: Some(owner),
                audience: Audience::Everyone,
                payload: json!({ "items": group.items, "new_series": group.new_series }),
                event_ids: Some(group.events),
            },
        )
        .await?;
        return Ok(());
    };
    let mut payload: Value = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
    let mut items: Vec<i64> = serde_json::from_value(payload["items"].take()).unwrap_or_default();
    for item in group.items {
        if !items.contains(&item) {
            items.push(item);
        }
    }
    payload["items"] = json!(items);
    let mut events: Vec<i64> = event_ids
        .and_then(|ids| serde_json::from_str(&ids).ok())
        .unwrap_or_default();
    events.extend(group.events);
    sqlx::query("UPDATE activity SET at = max(at, ?), payload = ?, event_ids = ? WHERE id = ?")
        .bind(group.at)
        .bind(payload.to_string())
        .bind(json!(events).to_string())
        .bind(id)
        .execute(&mut *conn)
        .await?;
    events::emit(conn, events::ACTIVITY_ADDED, json!({ "activity_id": id })).await?;
    Ok(())
}

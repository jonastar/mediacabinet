//! The activity log: a readable history for people. Design: `docs/activity.md`.
//!
//! Unlike the event outbox, entries are grouped ("5 new episodes of X"),
//! attributed to the user who acted, and filtered per viewer by their audience.
//! Library additions come from the outbox ([`library`]); what people watched
//! comes from play sessions ([`sessions`]); the rest is written directly where it
//! happens.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};
use utoipa::{IntoParams, ToSchema};

use crate::catalog::{EpisodeNumber, NumberStyle};
use crate::events;

pub mod library;
pub mod sessions;

pub const ITEMS_ADDED: &str = "items_added";
pub const WATCHED: &str = "watched";
pub const STOPPED_WATCHING: &str = "stopped_watching";
pub const USER_CREATED: &str = "user_created";
pub const USER_REMOVED: &str = "user_removed";
/// Renamed, password reset, or made or no longer an admin, by an admin.
pub const USER_CHANGED: &str = "user_changed";
/// Approved, declined, turned off or back on.
pub const USER_STATUS: &str = "user_status";
pub const LOGIN: &str = "login";
/// An account at a sign-in provider linked to a user, or unlinked.
pub const LOGIN_LINKED: &str = "login_linked";
pub const LOGIN_UNLINKED: &str = "login_unlinked";
pub const LOGIN_FAILED: &str = "login_failed";
pub const CONFIG_CHANGED: &str = "config_changed";
pub const TASK_FINISHED: &str = "task_finished";
pub const TASK_FAILED: &str = "task_failed";

/// What people watched; a user can clear their own.
pub const WATCH_KINDS: &[&str] = &[WATCHED, STOPPED_WATCHING];

/// How long entries are kept, per kind. Unlisted kinds use [`KEEP_DEFAULT`].
const KEEP_SHORT: i64 = 90 * 86400;
const KEEP_DEFAULT: i64 = 365 * 86400;
const SHORT_KINDS: &[&str] = &[WATCHED, STOPPED_WATCHING, LOGIN, LOGIN_FAILED];

#[derive(Debug, thiserror::Error)]
pub enum ActivityError {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Who may see an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Audience {
    /// All users who can access the entry's library.
    Everyone,
    Admins,
    /// The user it's about, and admins.
    Subject,
}

impl Audience {
    pub fn as_str(self) -> &'static str {
        match self {
            Audience::Everyone => "everyone",
            Audience::Admins => "admins",
            Audience::Subject => "subject",
        }
    }

    fn parse(audience: &str) -> Audience {
        match audience {
            "everyone" => Audience::Everyone,
            "subject" => Audience::Subject,
            _ => Audience::Admins,
        }
    }
}

/// An entry to write.
#[derive(Debug, Clone)]
pub struct NewEntry {
    pub kind: &'static str,
    pub at: i64,
    pub actor: Option<i64>,
    pub subject: Option<i64>,
    pub library: Option<i64>,
    pub item: Option<i64>,
    pub audience: Audience,
    pub payload: Value,
    pub event_ids: Option<Vec<i64>>,
}

impl NewEntry {
    /// An entry only admins see, about nothing in a library.
    pub fn admins(kind: &'static str, at: i64, actor: Option<i64>, payload: Value) -> NewEntry {
        NewEntry {
            kind,
            at,
            actor,
            subject: None,
            library: None,
            item: None,
            audience: Audience::Admins,
            payload,
            event_ids: None,
        }
    }
}

/// Writes an entry and announces it on the event stream, for live updates.
pub async fn record(conn: &mut SqliteConnection, entry: &NewEntry) -> Result<i64, sqlx::Error> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO activity (kind, at, actor_user_id, subject_user_id, library_id, item_id,
             audience, payload, event_ids)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(entry.kind)
    .bind(entry.at)
    .bind(entry.actor)
    .bind(entry.subject)
    .bind(entry.library)
    .bind(entry.item)
    .bind(entry.audience.as_str())
    .bind(entry.payload.to_string())
    .bind(entry.event_ids.as_ref().map(|ids| json!(ids).to_string()))
    .fetch_one(&mut *conn)
    .await?;
    events::emit(conn, events::ACTIVITY_ADDED, json!({ "activity_id": id })).await?;
    Ok(id)
}

/// Records an entry on its own, logging rather than failing: what it describes
/// has already happened.
pub async fn record_or_warn(pool: &SqlitePool, entry: NewEntry) {
    let written = async {
        let mut tx = crate::db::begin_write(pool).await?;
        record(&mut tx, &entry).await?;
        tx.commit().await
    };
    if let Err(e) = written.await {
        tracing::warn!(kind = entry.kind, error = %e, "can't record activity");
    }
}

/// A configuration change, naming the fields that changed but never their values.
pub fn config_changed(
    at: i64,
    actor: Option<i64>,
    target: &str,
    action: &str,
    id: i64,
    name: Option<&str>,
    fields: &[&str],
) -> NewEntry {
    NewEntry::admins(
        CONFIG_CHANGED,
        at,
        actor,
        json!({ "target": target, "action": action, "id": id, "name": name, "fields": fields }),
    )
}

// ── Reading ────────────────────────────────────────────────────────────────

/// Who is looking.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    pub user: i64,
    pub admin: bool,
}

#[derive(Debug, Default, Deserialize, IntoParams)]
pub struct ActivityFilter {
    /// Comma-separated kinds; default all.
    pub kinds: Option<String>,
    pub library_id: Option<i64>,
    /// Entries about this item, its episodes, or (for episodes) additions that include it.
    pub item_id: Option<i64>,
    /// Entries where this user acted or is the subject.
    pub user_id: Option<i64>,
    /// The `next` cursor of the previous page.
    pub before: Option<String>,
    /// Default 50, max 200.
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UserRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ItemRef {
    pub id: i64,
    /// `series`, `episode` or `movie`.
    pub kind: String,
    pub title: String,
    pub library_id: i64,
    /// Episodes: their series.
    pub series_id: Option<i64>,
    pub series_title: Option<String>,
    /// Episodes: the number in the default ordering, or else the first one.
    pub number: Option<EpisodeNumber>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActivityEntry {
    pub id: i64,
    pub kind: String,
    pub at: i64,
    pub audience: Audience,
    pub actor: Option<UserRef>,
    pub subject: Option<UserRef>,
    pub library_id: Option<i64>,
    pub item: Option<ItemRef>,
    /// Per kind. `items_added`: `items` (episode or movie IDs) and `new_series`.
    /// Watching: `position_start`, `position_end`, `duration_seconds`.
    /// `config_changed`: `target`, `action`, `id`, `name`, `fields`.
    /// Tasks: `task_id`, `task_kind`, `trigger`, `error`, scope IDs.
    #[schema(value_type = Object)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ActivityPage {
    pub entries: Vec<ActivityEntry>,
    /// Pass as `before` for older entries; none when there are no more.
    pub next: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    kind: String,
    at: i64,
    audience: String,
    actor_id: Option<i64>,
    actor_name: Option<String>,
    subject_id: Option<i64>,
    subject_name: Option<String>,
    library_id: Option<i64>,
    item_id: Option<i64>,
    item_kind: Option<String>,
    item_title: Option<String>,
    item_library: Option<i64>,
    series_id: Option<i64>,
    series_title: Option<String>,
    season: Option<i64>,
    number: Option<i64>,
    payload: String,
}

fn parse_cursor(before: &str) -> Result<(i64, i64), ActivityError> {
    before
        .split_once(':')
        .and_then(|(at, id)| Some((at.parse().ok()?, id.parse().ok()?)))
        .ok_or_else(|| ActivityError::Invalid("That isn't a cursor from this list.".into()))
}

/// Libraries the viewer can access. Everyone can access every library until
/// per-user library scoping lands; then this is the one place to change.
const VIEWER_LIBRARIES: &str = "SELECT id FROM library";

/// Entries the viewer may see, newest first.
pub async fn list(
    pool: &SqlitePool,
    viewer: Viewer,
    filter: &ActivityFilter,
) -> Result<ActivityPage, ActivityError> {
    let limit = filter.limit.unwrap_or(50).clamp(1, 200);
    let kinds: Vec<&str> = filter
        .kinds
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .collect();
    let before = filter.before.as_deref().map(parse_cursor).transpose()?;
    let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT a.id, a.kind, a.at, a.audience,
                a.actor_user_id AS actor_id, actor.name AS actor_name,
                a.subject_user_id AS subject_id, subject.name AS subject_name,
                a.library_id, a.item_id, i.kind AS item_kind, i.title AS item_title,
                i.library_id AS item_library, s.id AS series_id, s.title AS series_title,
                os.number AS season, oe.episode_number AS number,
                a.payload
         FROM activity a
         LEFT JOIN app_user actor ON actor.id = a.actor_user_id
         LEFT JOIN app_user subject ON subject.id = a.subject_user_id
         LEFT JOIN item i ON i.id = a.item_id
         LEFT JOIN item s ON s.id = i.parent_id
         -- Episodes: their number in the default ordering, or else the first one.
         LEFT JOIN ordering_entry oe ON oe.rowid = (
             SELECT e.rowid FROM ordering_entry e JOIN ordering o ON o.id = e.ordering_id
             WHERE e.item_id = i.id AND i.kind = 'episode'
             ORDER BY o.is_default DESC, o.id, e.position LIMIT 1)
         LEFT JOIN ordering_season os ON os.id = oe.ordering_season_id
         WHERE (?1 OR a.audience = 'everyone'
                OR (a.audience = 'subject' AND a.subject_user_id = ?2))
           AND (a.library_id IS NULL OR a.library_id IN ({VIEWER_LIBRARIES}))
           AND (?3 = '[]' OR a.kind IN (SELECT value FROM json_each(?3)))
           AND (?4 IS NULL OR a.library_id = ?4)
           AND (?5 IS NULL OR a.item_id = ?5 OR i.parent_id = ?5
                OR (a.kind = '{ITEMS_ADDED}'
                    AND a.item_id = (SELECT parent_id FROM item WHERE id = ?5)
                    AND EXISTS (SELECT 1 FROM json_each(a.payload, '$.items') WHERE value = ?5)))
           AND (?6 IS NULL OR a.actor_user_id = ?6 OR a.subject_user_id = ?6)
           AND (?7 IS NULL OR a.at < ?7 OR (a.at = ?7 AND a.id < ?8))
         ORDER BY a.at DESC, a.id DESC
         LIMIT ?9"
    )))
    .bind(viewer.admin)
    .bind(viewer.user)
    .bind(json!(kinds).to_string())
    .bind(filter.library_id)
    .bind(filter.item_id)
    .bind(filter.user_id)
    .bind(before.map(|(at, _)| at))
    .bind(before.map(|(_, id)| id))
    .bind(limit + 1)
    .fetch_all(pool)
    .await?;
    let more = rows.len() as i64 > limit;
    let entries: Vec<ActivityEntry> = rows.into_iter().take(limit as usize).map(entry).collect();
    let next = if more {
        entries.last().map(|e| format!("{}:{}", e.at, e.id))
    } else {
        None
    };
    Ok(ActivityPage { entries, next })
}

fn entry(r: Row) -> ActivityEntry {
    let user = |id: Option<i64>, name: Option<String>| {
        Some(UserRef {
            id: id?,
            name: name?,
        })
    };
    let item = match (r.item_id, r.item_kind, r.item_title, r.item_library) {
        (Some(id), Some(kind), Some(title), Some(library_id)) => Some(ItemRef {
            id,
            kind,
            title,
            library_id,
            series_id: r.series_id,
            series_title: r.series_title,
            number: r.number.map(|n| EpisodeNumber {
                style: if r.season.is_some() {
                    NumberStyle::Season
                } else {
                    NumberStyle::Absolute
                },
                season: r.season,
                number: Some(n),
            }),
        }),
        _ => None,
    };
    ActivityEntry {
        id: r.id,
        kind: r.kind,
        at: r.at,
        audience: Audience::parse(&r.audience),
        actor: user(r.actor_id, r.actor_name),
        subject: user(r.subject_id, r.subject_name),
        library_id: r.library_id,
        item,
        payload: serde_json::from_str(&r.payload).unwrap_or(Value::Null),
    }
}

// ── Privacy and retention ──────────────────────────────────────────────────

/// Clears what the user watched from the activity log, and their finished play
/// sessions. Watch state (resume points, played) stays. Returns entries removed.
pub async fn clear_watch(pool: &SqlitePool, user: i64) -> Result<u64, sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let removed = sqlx::query(
        "DELETE FROM activity WHERE subject_user_id = ?
         AND kind IN (SELECT value FROM json_each(?))",
    )
    .bind(user)
    .bind(json!(WATCH_KINDS).to_string())
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM play_session WHERE user_id = ? AND ended_at IS NOT NULL")
        .bind(user)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(removed)
}

/// Removes entries past their kind's retention, and old play sessions.
pub async fn prune(pool: &SqlitePool, now: i64) -> Result<u64, sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let removed = sqlx::query(
        "DELETE FROM activity WHERE at < CASE WHEN kind IN (SELECT value FROM json_each(?1))
                                         THEN ?2 ELSE ?3 END",
    )
    .bind(json!(SHORT_KINDS).to_string())
    .bind(now - KEEP_SHORT)
    .bind(now - KEEP_DEFAULT)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query("DELETE FROM play_session WHERE ended_at < ?")
        .bind(now - KEEP_SHORT)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(removed)
}

/// Keeps the log up to date in the background: library additions from the
/// outbox every few seconds, play sessions that went quiet every minute, and
/// pruning once an hour.
pub fn spawn_worker(pool: SqlitePool) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
        let mut ticks: u64 = 0;
        loop {
            tick.tick().await;
            let now = crate::catalog::now();
            if let Err(e) = library::sync(&pool).await {
                tracing::warn!(error = %e, "can't record library additions");
            }
            if ticks.is_multiple_of(12)
                && let Err(e) = sessions::end_quiet(&pool, now).await
            {
                tracing::warn!(error = %e, "can't end quiet play sessions");
            }
            if ticks.is_multiple_of(720)
                && let Err(e) = prune(&pool, now).await
            {
                tracing::warn!(error = %e, "can't prune activity");
            }
            ticks += 1;
        }
    })
}

//! Play sessions: from the first progress report for an item until reports stop
//! for [`QUIET_AFTER`], or the item is finished. A session that lasted
//! [`MIN_SECONDS`] or reached the end becomes a `watched` or `stopped_watching`
//! entry; sampling and skipping around produce nothing.
//!
//! Sessions remember whether the user was incognito when they started; their
//! entries are then only for the user and admins, whatever the setting is later.

use serde_json::json;
use sqlx::{SqliteConnection, SqlitePool};

use super::{Audience, NewEntry, STOPPED_WATCHING, WATCHED};

pub const QUIET_AFTER: i64 = 10 * 60;
pub const MIN_SECONDS: i64 = 5 * 60;

#[derive(sqlx::FromRow)]
struct Session {
    id: i64,
    user_id: i64,
    item_id: i64,
    started_at: i64,
    last_report_at: i64,
    position_start: f64,
    position_end: f64,
    incognito: bool,
}

const COLUMNS: &str =
    "id, user_id, item_id, started_at, last_report_at, position_start, position_end, incognito";

/// Starts or extends the user's session for the item. `finished` means this
/// report reached the played threshold, which ends the session.
pub async fn report(
    conn: &mut SqliteConnection,
    user: i64,
    item: i64,
    position: f64,
    duration: Option<f64>,
    finished: bool,
    now: i64,
) -> Result<(), sqlx::Error> {
    let open: Option<Session> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM play_session WHERE user_id = ? AND item_id = ? AND ended_at IS NULL"
    )))
    .bind(user)
    .bind(item)
    .fetch_optional(&mut *conn)
    .await?;
    let open = match open {
        Some(s) if now - s.last_report_at > QUIET_AFTER => {
            end(conn, &s, s.last_report_at, false, duration).await?;
            None
        }
        open => open,
    };
    if let Some(mut session) = open {
        sqlx::query("UPDATE play_session SET last_report_at = ?, position_end = ? WHERE id = ?")
            .bind(now)
            .bind(position)
            .bind(session.id)
            .execute(&mut *conn)
            .await?;
        if finished {
            session.last_report_at = now;
            session.position_end = position;
            end(conn, &session, now, true, duration).await?;
        }
        return Ok(());
    }
    if finished {
        // Reports keep coming through the credits; they belong to the session
        // that just finished.
        let just_finished: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM play_session WHERE user_id = ? AND item_id = ?
                            AND finished AND ended_at >= ?)",
        )
        .bind(user)
        .bind(item)
        .bind(now - QUIET_AFTER)
        .fetch_one(&mut *conn)
        .await?;
        if just_finished {
            return Ok(());
        }
    }
    let session: Session = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "INSERT INTO play_session (user_id, item_id, started_at, last_report_at, position_start,
             position_end, incognito)
         VALUES (?1, ?2, ?3, ?3, ?4, ?4, (SELECT incognito FROM app_user WHERE id = ?1))
         RETURNING {COLUMNS}"
    )))
    .bind(user)
    .bind(item)
    .bind(now)
    .bind(position)
    .fetch_one(&mut *conn)
    .await?;
    if finished {
        end(conn, &session, now, true, duration).await?;
    }
    Ok(())
}

/// Ends sessions with no report for [`QUIET_AFTER`], as of their last report.
pub async fn end_quiet(pool: &SqlitePool, now: i64) -> Result<usize, sqlx::Error> {
    let mut tx = crate::db::begin_write(pool).await?;
    let quiet: Vec<Session> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM play_session WHERE ended_at IS NULL AND last_report_at < ?"
    )))
    .bind(now - QUIET_AFTER)
    .fetch_all(&mut *tx)
    .await?;
    for session in &quiet {
        end(&mut tx, session, session.last_report_at, false, None).await?;
    }
    tx.commit().await?;
    Ok(quiet.len())
}

async fn end(
    conn: &mut SqliteConnection,
    session: &Session,
    at: i64,
    finished: bool,
    duration: Option<f64>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE play_session SET ended_at = ?, finished = ? WHERE id = ?")
        .bind(at)
        .bind(finished)
        .bind(session.id)
        .execute(&mut *conn)
        .await?;
    if !finished && session.last_report_at - session.started_at < MIN_SECONDS {
        return Ok(());
    }
    let (library, known_duration): (Option<i64>, Option<f64>) = sqlx::query_as(
        "SELECT i.library_id, CAST(coalesce(
             (SELECT max(f.duration_seconds) FROM file_link l JOIN media_file f ON f.id = l.file_id
              WHERE l.item_id = i.id AND f.status = 'present'),
             i.runtime_seconds) AS REAL)
         FROM item i WHERE i.id = ?",
    )
    .bind(session.item_id)
    .fetch_optional(&mut *conn)
    .await?
    .unwrap_or_default();
    let duration = duration.or(known_duration);
    let audience = if session.incognito {
        Audience::Subject
    } else {
        Audience::Everyone
    };
    super::record(
        conn,
        &NewEntry {
            kind: if finished {
                WATCHED
            } else {
                STOPPED_WATCHING
            },
            at,
            actor: Some(session.user_id),
            subject: Some(session.user_id),
            library,
            item: Some(session.item_id),
            audience,
            payload: json!({
                "session_id": session.id,
                "position_start": session.position_start,
                "position_end": session.position_end,
                "duration_seconds": duration,
            }),
            event_ids: None,
        },
    )
    .await?;
    Ok(())
}

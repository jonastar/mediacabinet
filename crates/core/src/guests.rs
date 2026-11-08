//! Guests: people trying a demo server, or reviewing an app, without an account.
//!
//! When guest access is on, "Continue as guest" makes a throwaway account for
//! that one visitor and signs it in, so guests never see each other's watch
//! history or playlists. A guest is a user who isn't an admin, is always
//! incognito, can't change their account or link another one, and has no
//! password, so nobody can sign in as them again. A guest goes when their last
//! session does: on signing out, or once their sessions expire. Past
//! [`MAX_GUESTS`], the guest seen least recently makes room for the new one.
//!
//! Turning guest access off removes every guest. The design is in `docs/guests.md`.

use sqlx::SqlitePool;

use crate::accounts::{self, AccountError, User};

/// Guests kept at once.
pub const MAX_GUESTS: i64 = 1000;

/// Makes a guest and opens a browser session for them. Returns the guest and
/// the session token for the cookie.
pub async fn sign_in(pool: &SqlitePool) -> Result<(User, String), AccountError> {
    let id = create(pool).await?;
    accounts::open_session(pool, id).await
}

/// Makes a guest and opens a session for a device (a TV or app). Returns the
/// guest and the device's token.
pub async fn sign_in_device(
    pool: &SqlitePool,
    device_name: &str,
) -> Result<(User, String), AccountError> {
    let device_name = accounts::check_device_name(device_name)?;
    let id = create(pool).await?;
    accounts::open_device_session(pool, id, &device_name).await
}

async fn create(pool: &SqlitePool) -> Result<i64, AccountError> {
    sweep(pool).await?;
    make_room(pool).await?;
    loop {
        let mut bytes = [0u8; 3];
        getrandom::fill(&mut bytes).expect("the OS random source works");
        let name = format!("guest-{}", hex::encode(bytes));
        let created: Result<i64, sqlx::Error> = sqlx::query_scalar(
            "INSERT INTO app_user (name, is_admin, incognito, is_guest) VALUES (?, 0, 1, 1)
             RETURNING id",
        )
        .bind(&name)
        .fetch_one(pool)
        .await;
        match created {
            Ok(id) => {
                return Ok(id);
            }
            // Taken: pick another name.
            Err(sqlx::Error::Database(d)) if d.is_unique_violation() => {
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }
}

/// Removes guests who have no session left that's still good.
pub async fn sweep(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query(
        "DELETE FROM app_user WHERE is_guest AND NOT EXISTS
             (SELECT 1 FROM session s WHERE s.user_id = app_user.id AND s.expires_at > unixepoch())",
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}

/// Removes the guests seen least recently, leaving room for one more.
async fn make_room(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM app_user WHERE id IN (
             SELECT u.id FROM app_user u
             WHERE u.is_guest
             ORDER BY (SELECT max(s.last_seen_at) FROM session s WHERE s.user_id = u.id), u.id
             LIMIT max(0, (SELECT count(*) FROM app_user WHERE is_guest) - ? + 1))",
    )
    .bind(MAX_GUESTS)
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes a guest once they have no session left, such as after signing out.
pub async fn forget_if_gone(pool: &SqlitePool, user: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM app_user WHERE id = ? AND is_guest
             AND NOT EXISTS (SELECT 1 FROM session s WHERE s.user_id = app_user.id)",
    )
    .bind(user)
    .execute(pool)
    .await?;
    Ok(())
}

/// Removes every guest, for when guest access is turned off.
pub async fn remove_all(pool: &SqlitePool) -> Result<u64, sqlx::Error> {
    let done = sqlx::query("DELETE FROM app_user WHERE is_guest")
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}

/// How many guests there are now.
pub async fn count(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM app_user WHERE is_guest")
        .fetch_one(pool)
        .await
}

/// Whether "Continue as guest" is offered. `env` is `CABINET_GUEST_ACCESS`,
/// which wins when set.
pub async fn enabled(pool: &SqlitePool, env: Option<bool>) -> Result<bool, sqlx::Error> {
    if let Some(on) = env {
        return Ok(on);
    }
    sqlx::query_scalar("SELECT guest_access FROM auth_setting WHERE id = 1")
        .fetch_optional(pool)
        .await
        .map(|v| v.unwrap_or(false))
}

/// Turns guest access on or off; off removes every guest.
pub async fn set_enabled(pool: &SqlitePool, on: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_setting SET guest_access = ? WHERE id = 1")
        .bind(on)
        .execute(pool)
        .await?;
    if !on {
        remove_all(pool).await?;
    }
    Ok(())
}

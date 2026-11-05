//! Users, passwords and sessions.
//!
//! Passwords are hashed with Argon2id. A session is a random token in an
//! HttpOnly cookie; only its SHA-256 is stored. Sessions last 30 days from last
//! use. The first run has one admin without a password: setup gives it one.
//!
//! A device (TV or app) has a session too, with a name: its token is sent as
//! `Authorization: Bearer` and lasts a year from last use, since a TV can sit
//! unused for months. For requests that can't send a header, such as `<img>`
//! and `<video>`, a session can get a short-lived media token.
//!
//! Users who sign in with another account ([`crate::sign_in`]) may have no
//! password, and may be pending until an admin lets them in.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use utoipa::ToSchema;

pub const SESSION_DAYS: i64 = 30;
pub const DEVICE_SESSION_DAYS: i64 = 365;
pub const MEDIA_TOKEN_SECONDS: i64 = 86400;

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("Wrong name or password.")]
    BadLogin,
    #[error("This account has been turned off. Ask an admin.")]
    Disabled,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// Whether a user may use the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum UserStatus {
    Active,
    /// Signed in with another account; waiting for an admin.
    Pending,
    /// Declined or turned off by an admin; can't sign in.
    Disabled,
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
    pub created_at: i64,
    /// Hides what the user watches from other users, not from admins.
    pub incognito: bool,
    pub email: Option<String>,
    pub status: UserStatus,
}

const USER_COLUMNS: &str = "id, name, is_admin, created_at, incognito, email, status";

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

async fn hash_password(password: String) -> Result<String, AccountError> {
    if password.chars().count() < 8 {
        return Err(AccountError::Invalid(
            "Use a password of at least 8 characters.".into(),
        ));
    }
    tokio::task::spawn_blocking(move || {
        let mut salt = [0u8; 16];
        getrandom::fill(&mut salt).expect("the OS random source works");
        let salt = SaltString::encode_b64(&salt).expect("16 bytes is a valid salt");
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AccountError::Invalid(e.to_string()))
    })
    .await
    .map_err(|e| AccountError::Invalid(e.to_string()))?
}

async fn verify_password(password: String, hash: String) -> bool {
    tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false)
}

fn check_email(email: &str) -> Result<Option<String>, AccountError> {
    let email = email.trim();
    if email.is_empty() {
        return Ok(None);
    }
    if email.len() > 254 || !email.contains('@') {
        return Err(AccountError::Invalid(
            "That doesn't look like an email address.".into(),
        ));
    }
    Ok(Some(email.to_string()))
}

fn check_name(name: &str) -> Result<String, AccountError> {
    let name = name.trim();
    if name.is_empty() || name.len() > 64 {
        return Err(AccountError::Invalid(
            "Choose a name of 1 to 64 characters.".into(),
        ));
    }
    Ok(name.to_string())
}

/// True until some account has a password: the app needs setting up.
pub async fn setup_needed(pool: &SqlitePool) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT NOT EXISTS (SELECT 1 FROM app_user WHERE password_hash IS NOT NULL)")
        .fetch_one(pool)
        .await
}

/// First run: gives the first admin a name and password. Refused once any
/// account has a password.
pub async fn setup(pool: &SqlitePool, name: &str, password: &str) -> Result<User, AccountError> {
    if !setup_needed(pool).await? {
        return Err(AccountError::Invalid(
            "Already set up; log in instead.".into(),
        ));
    }
    let name = check_name(name)?;
    let hash = hash_password(password.to_string()).await?;
    let admin = crate::catalog::ensure_default_user(pool).await?;
    sqlx::query("UPDATE app_user SET name = ?, password_hash = ?, is_admin = 1 WHERE id = ?")
        .bind(&name)
        .bind(hash)
        .bind(admin)
        .execute(pool)
        .await?;
    user(pool, admin).await
}

pub async fn user(pool: &SqlitePool, id: i64) -> Result<User, AccountError> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM app_user WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or(AccountError::NotFound)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<User>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM app_user ORDER BY id"
    )))
    .fetch_all(pool)
    .await
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    password: &str,
    is_admin: bool,
) -> Result<User, AccountError> {
    let name = check_name(name)?;
    let hash = hash_password(password.to_string()).await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO app_user (name, password_hash, is_admin) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(&name)
    .bind(hash)
    .bind(is_admin)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            AccountError::Invalid("That name is taken.".into())
        }
        _ => e.into(),
    })?;
    user(pool, id).await
}

/// Changes a user's name, password or admin flag. The last admin can't lose it.
pub async fn update(
    pool: &SqlitePool,
    id: i64,
    name: Option<&str>,
    password: Option<&str>,
    is_admin: Option<bool>,
) -> Result<User, AccountError> {
    let current = user(pool, id).await?;
    if is_admin == Some(false) && current.is_admin && admins(pool).await? <= 1 {
        return Err(AccountError::Invalid(
            "There must be at least one admin.".into(),
        ));
    }
    if let Some(name) = name {
        sqlx::query("UPDATE app_user SET name = ? WHERE id = ?")
            .bind(check_name(name)?)
            .bind(id)
            .execute(pool)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(d) if d.is_unique_violation() => {
                    AccountError::Invalid("That name is taken.".into())
                }
                _ => e.into(),
            })?;
    }
    if let Some(password) = password {
        let hash = hash_password(password.to_string()).await?;
        sqlx::query("UPDATE app_user SET password_hash = ? WHERE id = ?")
            .bind(hash)
            .bind(id)
            .execute(pool)
            .await?;
        // A new password signs out everywhere else.
        sqlx::query("DELETE FROM session WHERE user_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    }
    if let Some(admin) = is_admin {
        sqlx::query("UPDATE app_user SET is_admin = ? WHERE id = ?")
            .bind(admin)
            .bind(id)
            .execute(pool)
            .await?;
    }
    user(pool, id).await
}

/// Turns incognito on or off. Sessions keep the setting they started with, so
/// this neither exposes nor hides what was watched before.
pub async fn set_incognito(pool: &SqlitePool, id: i64, on: bool) -> Result<User, AccountError> {
    sqlx::query("UPDATE app_user SET incognito = ? WHERE id = ?")
        .bind(on)
        .bind(id)
        .execute(pool)
        .await?;
    user(pool, id).await
}

async fn admins(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM app_user WHERE is_admin AND status = 'active'")
        .fetch_one(pool)
        .await
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), AccountError> {
    let current = user(pool, id).await?;
    if current.is_admin && admins(pool).await? <= 1 {
        return Err(AccountError::Invalid(
            "There must be at least one admin.".into(),
        ));
    }
    sqlx::query("DELETE FROM app_user WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Checks a name and password and opens a session. Returns the user and the
/// session token for the cookie.
pub async fn login(
    pool: &SqlitePool,
    name: &str,
    password: &str,
) -> Result<(User, String), AccountError> {
    let id = check_login(pool, name, password).await?;
    open_session(pool, id).await
}

/// Checks a name and password and opens a session for a device. Returns the
/// user and the device's token.
pub async fn login_device(
    pool: &SqlitePool,
    name: &str,
    password: &str,
    device_name: &str,
) -> Result<(User, String), AccountError> {
    let device_name = check_device_name(device_name)?;
    let id = check_login(pool, name, password).await?;
    open_device_session(pool, id, &device_name).await
}

async fn check_login(pool: &SqlitePool, name: &str, password: &str) -> Result<i64, AccountError> {
    let row: Option<(i64, Option<String>)> =
        sqlx::query_as("SELECT id, password_hash FROM app_user WHERE name = ? COLLATE NOCASE")
            .bind(name.trim())
            .fetch_optional(pool)
            .await?;
    let id = match row {
        Some((id, Some(hash))) => {
            if !verify_password(password.to_string(), hash).await {
                return Err(AccountError::BadLogin);
            }
            id
        }
        _ => {
            // Spend the same time as a real check, so timing doesn't reveal names.
            let _ = hash_password("not a real password".into()).await;
            return Err(AccountError::BadLogin);
        }
    };
    Ok(id)
}

/// Opens a session for a user whose identity is already proven. Refused for
/// disabled users. Returns the user and the session token for the cookie.
pub async fn open_session(pool: &SqlitePool, id: i64) -> Result<(User, String), AccountError> {
    open(pool, id, None, SESSION_DAYS).await
}

/// Opens a session for a device whose user is already proven. Refused for
/// disabled users. Returns the user and the device's token.
pub async fn open_device_session(
    pool: &SqlitePool,
    id: i64,
    device_name: &str,
) -> Result<(User, String), AccountError> {
    let device_name = check_device_name(device_name)?;
    open(pool, id, Some(&device_name), DEVICE_SESSION_DAYS).await
}

pub(crate) fn check_device_name(name: &str) -> Result<String, AccountError> {
    let name: String = name.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 64 {
        return Err(AccountError::Invalid(
            "Choose a device name of 1 to 64 characters.".into(),
        ));
    }
    Ok(name.to_string())
}

async fn open(
    pool: &SqlitePool,
    id: i64,
    device_name: Option<&str>,
    days: i64,
) -> Result<(User, String), AccountError> {
    let user = user(pool, id).await?;
    if user.status == UserStatus::Disabled {
        return Err(AccountError::Disabled);
    }
    let token = crate::apikeys::random_secret().replacen("mc_", "", 1);
    sqlx::query(
        "INSERT INTO session (user_id, token_hash, device_name, expires_at)
         VALUES (?, ?, ?, unixepoch() + ?)",
    )
    .bind(id)
    .bind(hash_token(&token))
    .bind(device_name)
    .bind(days * 86400)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM session WHERE expires_at < unixepoch()")
        .execute(pool)
        .await?;
    Ok((user, token))
}

/// The user a session token belongs to, extending the session as it's used.
pub async fn session_user(pool: &SqlitePool, token: &str) -> Result<Option<User>, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "UPDATE session SET last_seen_at = unixepoch(),
             expires_at = unixepoch() + 86400 * CASE WHEN device_name IS NULL THEN ? ELSE ? END
         WHERE token_hash = ? AND expires_at > unixepoch() RETURNING user_id",
    )
    .bind(SESSION_DAYS)
    .bind(DEVICE_SESSION_DAYS)
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?;
    match id {
        Some(id) => Ok(user(pool, id)
            .await
            .ok()
            .filter(|u| u.status != UserStatus::Disabled)),
        None => Ok(None),
    }
}

pub async fn logout(pool: &SqlitePool, token: &str) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM session WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

/// A TV or app signed in to an account.
#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct Device {
    pub id: i64,
    pub name: String,
    pub signed_in_at: i64,
    pub last_seen_at: i64,
}

/// The user's signed-in devices, most recently used first.
pub async fn devices(pool: &SqlitePool, user: i64) -> Result<Vec<Device>, sqlx::Error> {
    sqlx::query_as(
        "SELECT id, device_name AS name, created_at AS signed_in_at, last_seen_at FROM session
         WHERE user_id = ? AND device_name IS NOT NULL AND expires_at > unixepoch()
         ORDER BY last_seen_at DESC, id DESC",
    )
    .bind(user)
    .fetch_all(pool)
    .await
}

/// Signs out one of the user's devices. False if they have no such device.
pub async fn sign_out_device(pool: &SqlitePool, user: i64, id: i64) -> Result<bool, sqlx::Error> {
    let done =
        sqlx::query("DELETE FROM session WHERE id = ? AND user_id = ? AND device_name IS NOT NULL")
            .bind(id)
            .bind(user)
            .execute(pool)
            .await?;
    Ok(done.rows_affected() > 0)
}

/// A token for media requests, which can't send a header.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MediaToken {
    /// Sent as `?media_token=` on artwork, streams and subtitles only.
    pub token: String,
    pub expires_at: i64,
}

/// Creates a media token under a session, given that session's token. None if
/// the session has ended. It ends with the session, or after a day.
pub async fn create_media_token(
    pool: &SqlitePool,
    session_token: &str,
) -> Result<Option<MediaToken>, sqlx::Error> {
    let session: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM session WHERE token_hash = ? AND expires_at > unixepoch()",
    )
    .bind(hash_token(session_token))
    .fetch_optional(pool)
    .await?;
    let Some(session) = session else {
        return Ok(None);
    };
    sqlx::query("DELETE FROM media_token WHERE expires_at < unixepoch()")
        .execute(pool)
        .await?;
    let token = crate::apikeys::random_secret().replacen("mc_", "", 1);
    let expires_at: i64 = sqlx::query_scalar(
        "INSERT INTO media_token (session_id, token_hash, expires_at)
         VALUES (?, ?, unixepoch() + ?) RETURNING expires_at",
    )
    .bind(session)
    .bind(hash_token(&token))
    .bind(MEDIA_TOKEN_SECONDS)
    .fetch_one(pool)
    .await?;
    Ok(Some(MediaToken { token, expires_at }))
}

/// The user a media token belongs to. Using one doesn't extend anything.
pub async fn media_token_user(pool: &SqlitePool, token: &str) -> Result<Option<User>, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "SELECT s.user_id FROM media_token m JOIN session s ON s.id = m.session_id
         WHERE m.token_hash = ? AND m.expires_at > unixepoch() AND s.expires_at > unixepoch()",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?;
    match id {
        Some(id) => Ok(user(pool, id)
            .await
            .ok()
            .filter(|u| u.status != UserStatus::Disabled)),
        None => Ok(None),
    }
}

/// Whether `password` is the user's current one.
pub async fn check_password(
    pool: &SqlitePool,
    id: i64,
    password: &str,
) -> Result<bool, sqlx::Error> {
    let hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM app_user WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
            .flatten();
    match hash {
        Some(hash) => Ok(verify_password(password.to_string(), hash).await),
        None => Ok(false),
    }
}

/// Whether the user has a password, as well as or instead of linked accounts.
pub async fn has_password(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("SELECT password_hash IS NOT NULL FROM app_user WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map(|v| v.unwrap_or(false))
}

/// Sets or clears a user's email address. Unique among users.
pub async fn set_email(pool: &SqlitePool, id: i64, email: &str) -> Result<User, AccountError> {
    let email = check_email(email)?;
    sqlx::query("UPDATE app_user SET email = ? WHERE id = ?")
        .bind(email)
        .bind(id)
        .execute(pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => {
                AccountError::Invalid("Another account has that email address.".into())
            }
            _ => e.into(),
        })?;
    user(pool, id).await
}

/// The user with this email address, ignoring case.
pub async fn by_email(pool: &SqlitePool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {USER_COLUMNS} FROM app_user WHERE email = ? COLLATE NOCASE"
    )))
    .bind(email.trim())
    .fetch_optional(pool)
    .await
}

/// Approves, declines or turns off a user. Admins can't be turned off;
/// turning someone off ends their sessions.
pub async fn set_status(
    pool: &SqlitePool,
    id: i64,
    status: UserStatus,
) -> Result<User, AccountError> {
    let current = user(pool, id).await?;
    if status == UserStatus::Disabled && current.is_admin {
        return Err(AccountError::Invalid(
            "Admins can't be turned off. Make them a viewer first.".into(),
        ));
    }
    sqlx::query("UPDATE app_user SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    if status == UserStatus::Disabled {
        sqlx::query("DELETE FROM session WHERE user_id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    }
    user(pool, id).await
}

/// Makes a user an admin or not without the checks of [`update`], apart from
/// keeping one admin. Returns false if that rule kept them an admin.
pub async fn set_admin(pool: &SqlitePool, id: i64, admin: bool) -> Result<bool, AccountError> {
    let current = user(pool, id).await?;
    if current.is_admin == admin {
        return Ok(true);
    }
    if !admin && current.status == UserStatus::Active && admins(pool).await? <= 1 {
        return Ok(false);
    }
    sqlx::query("UPDATE app_user SET is_admin = ? WHERE id = ?")
        .bind(admin)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(true)
}

/// A user for someone who signed in with another account: no password, a
/// name based on `name` (numbered if taken), and the email if no one has it.
pub async fn create_external(
    pool: &SqlitePool,
    name: &str,
    email: Option<&str>,
    is_admin: bool,
    status: UserStatus,
) -> Result<User, AccountError> {
    let base: String = name.trim().chars().take(60).collect();
    let base = if base.is_empty() {
        "user".to_string()
    } else {
        base
    };
    let email = match email {
        Some(email) => match check_email(email) {
            Ok(Some(email)) if by_email(pool, &email).await?.is_none() => Some(email),
            _ => None,
        },
        None => None,
    };
    for n in 1.. {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        let taken: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM app_user WHERE name = ? COLLATE NOCASE)",
        )
        .bind(&name)
        .fetch_one(pool)
        .await?;
        if taken {
            continue;
        }
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO app_user (name, email, is_admin, status) VALUES (?, ?, ?, ?) RETURNING id",
        )
        .bind(&name)
        .bind(&email)
        .bind(is_admin)
        .bind(status)
        .fetch_one(pool)
        .await?;
        return user(pool, id).await;
    }
    unreachable!("some number is free")
}

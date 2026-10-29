//! Users, passwords and sessions.
//!
//! Passwords are hashed with Argon2id. A session is a random token in an
//! HttpOnly cookie; only its SHA-256 is stored. Sessions last 30 days from last
//! use. The first run has one admin without a password: setup gives it one.

use argon2::Argon2;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use utoipa::ToSchema;

pub const SESSION_DAYS: i64 = 30;

#[derive(Debug, thiserror::Error)]
pub enum AccountError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error("Wrong name or password.")]
    BadLogin,
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, ToSchema, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub is_admin: bool,
    pub created_at: i64,
    /// Hides what the user watches from other users, not from admins.
    pub incognito: bool,
}

const USER_COLUMNS: &str = "id, name, is_admin, created_at, incognito";

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
    sqlx::query_scalar("SELECT count(*) FROM app_user WHERE is_admin")
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
    let token = crate::apikeys::random_secret().replacen("mc_", "", 1);
    sqlx::query(
        "INSERT INTO session (user_id, token_hash, expires_at) VALUES (?, ?, unixepoch() + ?)",
    )
    .bind(id)
    .bind(hash_token(&token))
    .bind(SESSION_DAYS * 86400)
    .execute(pool)
    .await?;
    sqlx::query("DELETE FROM session WHERE expires_at < unixepoch()")
        .execute(pool)
        .await?;
    Ok((user(pool, id).await?, token))
}

/// The user a session token belongs to, extending the session as it's used.
pub async fn session_user(pool: &SqlitePool, token: &str) -> Result<Option<User>, sqlx::Error> {
    let id: Option<i64> = sqlx::query_scalar(
        "UPDATE session SET last_seen_at = unixepoch(), expires_at = unixepoch() + ?
         WHERE token_hash = ? AND expires_at > unixepoch() RETURNING user_id",
    )
    .bind(SESSION_DAYS * 86400)
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?;
    match id {
        Some(id) => Ok(user(pool, id).await.ok()),
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

//! API keys for external tools. A key is shown once when created; only its
//! SHA-256 hash is stored.

use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiKey {
    pub id: i64,
    pub name: String,
    /// The key's first characters, to recognise it.
    pub prefix: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NewApiKey {
    #[serde(flatten)]
    pub key: ApiKey,
    /// The key itself. Shown only now; store it in the tool that uses it.
    pub secret: String,
}

fn hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

pub fn random_secret() -> String {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    format!("mc_{}", hex::encode(bytes))
}

pub async fn create(pool: &SqlitePool, name: &str) -> Result<NewApiKey, sqlx::Error> {
    let secret = random_secret();
    let prefix: String = secret.chars().take(10).collect();
    let name = if name.trim().is_empty() {
        "Unnamed"
    } else {
        name.trim()
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO api_key (name, prefix, key_hash) VALUES (?, ?, ?) RETURNING id",
    )
    .bind(name)
    .bind(&prefix)
    .bind(hash(&secret))
    .fetch_one(pool)
    .await?;
    let key = list(pool)
        .await?
        .into_iter()
        .find(|k| k.id == id)
        .expect("just created");
    Ok(NewApiKey { key, secret })
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<ApiKey>, sqlx::Error> {
    let rows: Vec<(i64, String, String, i64, Option<i64>)> = sqlx::query_as(
        "SELECT id, name, prefix, created_at, last_used_at FROM api_key ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, prefix, created_at, last_used_at)| ApiKey {
            id,
            name,
            prefix,
            created_at,
            last_used_at,
        })
        .collect())
}

/// Revokes a key. Returns false if there was none with that ID.
pub async fn revoke(pool: &SqlitePool, id: i64) -> Result<bool, sqlx::Error> {
    let done = sqlx::query("DELETE FROM api_key WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Checks a presented key, recording when it was last used.
pub async fn verify(pool: &SqlitePool, secret: &str) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE api_key SET last_used_at = unixepoch() WHERE key_hash = ? RETURNING id",
    )
    .bind(hash(secret))
    .fetch_optional(pool)
    .await
}

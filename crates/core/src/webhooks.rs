//! Outgoing webhooks: event subscriptions delivered by POST, signed, in order,
//! resuming from each subscription's cursor so nothing is lost while a receiver
//! is down. Failures retry with exponential backoff, up to an hour apart.
//!
//! Body: `{"webhook_id": 1, "events": [{"id", "type", "payload", "created_at"}…]}`.
//! Headers: `X-Cabinet-Signature: sha256=<hex HMAC-SHA256 of the body with the
//! webhook's secret>`, and `X-Cabinet-Cursor: <last event id in the batch>`.

use std::time::Duration;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Sha256;
use sqlx::SqlitePool;
use tokio::task::JoinHandle;
use utoipa::ToSchema;

use crate::events::{self, EventRecord};

const BATCH: i64 = 100;
/// How many events to scan past when none match a subscription's types.
const SCAN: i64 = 1000;

#[derive(Debug, thiserror::Error)]
pub enum WebhookError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Webhook {
    pub id: i64,
    pub name: String,
    pub url: String,
    /// Event types delivered; empty means all.
    pub event_types: Vec<String>,
    pub enabled: bool,
    /// The last event delivered.
    pub cursor: i64,
    /// Events waiting to be delivered.
    pub pending: i64,
    pub failures: i64,
    pub next_attempt_at: i64,
    pub last_error: Option<String>,
    pub last_delivered_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NewWebhook {
    #[serde(flatten)]
    pub webhook: Webhook,
    /// For verifying signatures. Shown only now.
    pub secret: String,
}

#[derive(Debug, Default, Deserialize, ToSchema)]
pub struct WebhookInput {
    pub name: Option<String>,
    pub url: Option<String>,
    pub event_types: Option<Vec<String>>,
    pub enabled: Option<bool>,
    /// Start from the oldest kept event instead of now (create only).
    #[serde(default)]
    pub from_start: bool,
}

fn check_url(url: &str) -> Result<(), WebhookError> {
    let ok = (url.starts_with("http://") || url.starts_with("https://")) && url.len() > 10;
    if ok {
        Ok(())
    } else {
        Err(WebhookError::Invalid(
            "The URL must start with http:// or https://.".into(),
        ))
    }
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Webhook>, WebhookError> {
    let rows: Vec<(
        i64,
        String,
        String,
        String,
        bool,
        i64,
        i64,
        i64,
        Option<String>,
        Option<i64>,
        i64,
    )> = sqlx::query_as(
        "SELECT id, name, url, event_types, enabled, cursor, failures, next_attempt_at, last_error,
                last_delivered_at, created_at FROM webhook ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    let mut out = Vec::new();
    for (
        id,
        name,
        url,
        types,
        enabled,
        cursor,
        failures,
        next_attempt_at,
        last_error,
        last_delivered_at,
        created_at,
    ) in rows
    {
        let event_types: Vec<String> = serde_json::from_str(&types).unwrap_or_default();
        let pending: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM event WHERE id > ?1 AND (?2 = '[]' OR type IN (SELECT value FROM json_each(?2)))",
        )
        .bind(cursor)
        .bind(&types)
        .fetch_one(pool)
        .await?;
        out.push(Webhook {
            id,
            name,
            url,
            event_types,
            enabled,
            cursor,
            pending,
            failures,
            next_attempt_at,
            last_error,
            last_delivered_at,
            created_at,
        });
    }
    Ok(out)
}

async fn get(pool: &SqlitePool, id: i64) -> Result<Webhook, WebhookError> {
    list(pool)
        .await?
        .into_iter()
        .find(|w| w.id == id)
        .ok_or(WebhookError::NotFound)
}

pub async fn create(pool: &SqlitePool, input: WebhookInput) -> Result<NewWebhook, WebhookError> {
    let url = input.url.unwrap_or_default();
    check_url(&url)?;
    let secret = crate::apikeys::random_secret().replacen("mc_", "whsec_", 1);
    let cursor = if input.from_start {
        0
    } else {
        events::latest(pool).await?
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO webhook (name, url, event_types, secret, cursor, enabled)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(input.name.unwrap_or_default().trim())
    .bind(url.trim())
    .bind(json!(input.event_types.unwrap_or_default()).to_string())
    .bind(&secret)
    .bind(cursor)
    .bind(input.enabled.unwrap_or(true))
    .fetch_one(pool)
    .await?;
    Ok(NewWebhook {
        webhook: get(pool, id).await?,
        secret,
    })
}

pub async fn update(
    pool: &SqlitePool,
    id: i64,
    input: WebhookInput,
) -> Result<Webhook, WebhookError> {
    get(pool, id).await?;
    if let Some(url) = &input.url {
        check_url(url)?;
    }
    sqlx::query(
        "UPDATE webhook SET name = coalesce(?, name), url = coalesce(?, url),
             event_types = coalesce(?, event_types), enabled = coalesce(?, enabled),
             -- Re-enabling or editing retries straight away.
             next_attempt_at = 0
         WHERE id = ?",
    )
    .bind(input.name.map(|n| n.trim().to_string()))
    .bind(input.url.map(|u| u.trim().to_string()))
    .bind(input.event_types.map(|t| json!(t).to_string()))
    .bind(input.enabled)
    .bind(id)
    .execute(pool)
    .await?;
    get(pool, id).await
}

pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), WebhookError> {
    let done = sqlx::query("DELETE FROM webhook WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if done.rows_affected() == 0 {
        return Err(WebhookError::NotFound);
    }
    Ok(())
}

pub fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC takes any key length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

#[derive(Clone)]
pub struct Deliverer {
    pool: SqlitePool,
    http: reqwest::Client,
}

impl Deliverer {
    pub fn new(pool: SqlitePool) -> Deliverer {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("MediaCabinet/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("HTTP client builds");
        Deliverer { pool, http }
    }

    async fn post(
        &self,
        url: &str,
        secret: &str,
        body: Vec<u8>,
        cursor: i64,
    ) -> Result<(), String> {
        let response = self
            .http
            .post(url)
            .header("content-type", "application/json")
            .header("x-cabinet-signature", sign(secret, &body))
            .header("x-cabinet-cursor", cursor.to_string())
            .body(body)
            .send()
            .await
            .map_err(|e| e.without_url().to_string())?;
        if response.status().is_success() {
            Ok(())
        } else {
            Err(format!("receiver answered {}", response.status()))
        }
    }

    /// Sends a test event without moving the cursor.
    pub async fn test(&self, id: i64) -> Result<Result<(), String>, WebhookError> {
        let (url, secret): (String, Option<String>) =
            sqlx::query_as("SELECT url, secret FROM webhook WHERE id = ?")
                .bind(id)
                .fetch_optional(&self.pool)
                .await?
                .ok_or(WebhookError::NotFound)?;
        let event = EventRecord {
            id: 0,
            kind: "webhook.test".into(),
            payload: json!({ "webhook_id": id }),
            created_at: chrono::Utc::now().timestamp(),
        };
        let body = serde_json::to_vec(&json!({ "webhook_id": id, "events": [event] }))
            .expect("serialises");
        Ok(self.post(&url, &secret.unwrap_or_default(), body, 0).await)
    }

    /// Delivers what's due for each enabled webhook. Returns events delivered.
    pub async fn deliver_due(&self) -> Result<usize, sqlx::Error> {
        let now = chrono::Utc::now().timestamp();
        let due: Vec<(i64, String, String, Option<String>, i64, i64)> = sqlx::query_as(
            "SELECT id, url, event_types, secret, cursor, failures FROM webhook
             WHERE enabled AND next_attempt_at <= ? ORDER BY id",
        )
        .bind(now)
        .fetch_all(&self.pool)
        .await?;
        let mut delivered = 0;
        for (id, url, types, secret, cursor, failures) in due {
            let types: Vec<String> = serde_json::from_str(&types).unwrap_or_default();
            let batch = events::since(&self.pool, cursor, BATCH, &types).await?;
            if batch.is_empty() {
                // Skip past events this subscription doesn't want.
                let skipped: Option<i64> = sqlx::query_scalar(
                    "SELECT max(id) FROM (SELECT id FROM event WHERE id > ? ORDER BY id LIMIT ?)",
                )
                .bind(cursor)
                .bind(SCAN)
                .fetch_one(&self.pool)
                .await?;
                if let Some(last) = skipped {
                    sqlx::query("UPDATE webhook SET cursor = ? WHERE id = ?")
                        .bind(last)
                        .bind(id)
                        .execute(&self.pool)
                        .await?;
                }
                continue;
            }
            let last = batch.last().map(|e| e.id).unwrap_or(cursor);
            let count = batch.len();
            let body = serde_json::to_vec(&json!({ "webhook_id": id, "events": batch }))
                .expect("serialises");
            match self
                .post(&url, &secret.unwrap_or_default(), body, last)
                .await
            {
                Ok(()) => {
                    sqlx::query(
                        "UPDATE webhook SET cursor = ?, failures = 0, next_attempt_at = 0, last_error = NULL,
                             last_delivered_at = unixepoch() WHERE id = ?",
                    )
                    .bind(last)
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                    delivered += count;
                }
                Err(error) => {
                    let backoff = (5i64 << failures.min(10)).min(3600);
                    tracing::warn!(
                        webhook = id,
                        error,
                        retry_in = backoff,
                        "webhook delivery failed"
                    );
                    sqlx::query(
                        "UPDATE webhook SET failures = failures + 1, next_attempt_at = ?, last_error = ? WHERE id = ?",
                    )
                    .bind(now + backoff)
                    .bind(error)
                    .bind(id)
                    .execute(&self.pool)
                    .await?;
                }
            }
        }
        Ok(delivered)
    }

    pub fn spawn(self, every: Duration) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            loop {
                tick.tick().await;
                if let Err(e) = self.deliver_due().await {
                    tracing::error!(error = %e, "webhook delivery round failed");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn signatures_are_hmac_sha256() {
        // RFC 4231 test case 2.
        assert_eq!(
            super::sign("Jefe", b"what do ya want for nothing?"),
            "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }
}

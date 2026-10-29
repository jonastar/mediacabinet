//! The event stream and webhook subscriptions.

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use cabinet_core::events::{self, EventRecord};
use cabinet_core::webhooks::{self, NewWebhook, Webhook, WebhookError, WebhookInput};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::activity::config_changed;
use super::auth::ActingUser;
use super::error::ErrorBody;
use crate::AppState;

impl From<WebhookError> for ApiError {
    fn from(err: WebhookError) -> Self {
        match err {
            WebhookError::NotFound => ApiError::NotFound,
            WebhookError::Invalid(m) => ApiError::BadRequest(m),
            WebhookError::Db(e) => ApiError::Internal(e.to_string()),
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct EventsQuery {
    /// Events after this ID. Default: only new events. SSE clients may send
    /// Last-Event-ID instead.
    after: Option<i64>,
    /// Comma-separated event types; default all.
    types: Option<String>,
    /// Long-poll: wait up to this many seconds (max 60) for an event to arrive.
    wait: Option<u64>,
    limit: Option<i64>,
}

#[derive(Serialize, ToSchema)]
pub struct EventsPage {
    events: Vec<EventRecord>,
    /// Pass as `after` next time.
    cursor: i64,
}

#[derive(Deserialize, IntoParams)]
pub struct ItemEventsQuery {
    /// How many, newest first (default 50, max 500).
    limit: Option<i64>,
}

/// An item's history, newest first: its own events and those of its files.
#[utoipa::path(
    get,
    path = "/items/{id}/events",
    tag = "events",
    params(("id" = i64, Path), ItemEventsQuery),
    responses((status = 200, body = Vec<EventRecord>))
)]
pub async fn item_events(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Query(q): Query<ItemEventsQuery>,
) -> Result<Json<Vec<EventRecord>>, ApiError> {
    Ok(Json(
        events::for_item(&state.db, id, q.limit.unwrap_or(50)).await?,
    ))
}

/// The event outbox, from a cursor. With `Accept: text/event-stream` it streams
/// (SSE, resumable with Last-Event-ID); otherwise it returns a page, long-polling
/// with `wait`. Consumers that keep their cursor never miss an event.
#[utoipa::path(
    get,
    path = "/events",
    tag = "events",
    params(EventsQuery),
    responses((status = 200, body = EventsPage, description = "JSON page; or text/event-stream when requested"))
)]
pub async fn events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<EventsQuery>,
) -> Result<Response, ApiError> {
    let types: Vec<String> = q
        .types
        .as_deref()
        .map(|t| {
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let last_event_id = headers
        .get("last-event-id")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok());
    let cursor = match q.after.or(last_event_id) {
        Some(c) => c,
        None => events::latest(&state.db).await?,
    };
    let limit = q.limit.unwrap_or(100);
    let wants_stream = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|a| a.contains("text/event-stream"));

    if wants_stream {
        let pool = state.db.clone();
        let stream = futures::stream::unfold(
            (pool, cursor, types),
            move |(pool, cursor, types)| async move {
                loop {
                    match events::since(&pool, cursor, limit, &types).await {
                        Ok(batch) if !batch.is_empty() => {
                            let next = batch.last().map(|e| e.id).unwrap_or(cursor);
                            return Some((batch, (pool, next, types)));
                        }
                        Ok(_) => tokio::time::sleep(Duration::from_millis(500)).await,
                        Err(e) => {
                            tracing::warn!(error = %e, "event stream query failed");
                            tokio::time::sleep(Duration::from_secs(2)).await;
                        }
                    }
                }
            },
        )
        .flat_map(futures::stream::iter)
        // Never ends by itself; without this, shutdown waits on every open tab.
        .take_until(crate::shutting_down(state.shutdown.clone()))
        .map(|e| {
            Ok::<_, Infallible>(
                Event::default()
                    .id(e.id.to_string())
                    .event(&e.kind)
                    .json_data(&e)
                    .expect("events serialise"),
            )
        });
        return Ok(Sse::new(stream)
            .keep_alive(KeepAlive::default())
            .into_response());
    }

    let deadline = tokio::time::Instant::now() + Duration::from_secs(q.wait.unwrap_or(0).min(60));
    loop {
        let batch = events::since(&state.db, cursor, limit, &types).await?;
        if !batch.is_empty() || tokio::time::Instant::now() >= deadline {
            let next = batch.last().map(|e| e.id).unwrap_or(cursor);
            return Ok(Json(EventsPage {
                events: batch,
                cursor: next,
            })
            .into_response());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[utoipa::path(get, path = "/webhooks", tag = "events", responses((status = 200, body = Vec<Webhook>)))]
pub async fn list_webhooks(State(state): State<AppState>) -> Result<Json<Vec<Webhook>>, ApiError> {
    Ok(Json(webhooks::list(&state.db).await?))
}

/// Subscribes a URL to events. The signing secret is in the response only.
#[utoipa::path(
    post,
    path = "/webhooks",
    tag = "events",
    request_body = WebhookInput,
    responses((status = 201, body = NewWebhook), (status = 400, body = ErrorBody))
)]
pub async fn create_webhook(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Json(body): Json<WebhookInput>,
) -> Result<(StatusCode, Json<NewWebhook>), ApiError> {
    let created = webhooks::create(&state.db, body).await?;
    let hook = &created.webhook;
    config_changed(
        &state,
        actor,
        "webhook",
        "created",
        hook.id,
        Some(&hook.name),
        &[],
    )
    .await;
    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    patch,
    path = "/webhooks/{id}",
    tag = "events",
    params(("id" = i64, Path)),
    request_body = WebhookInput,
    responses((status = 200, body = Webhook), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn update_webhook(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<WebhookInput>,
) -> Result<Json<Webhook>, ApiError> {
    let given = [
        ("name", body.name.is_some()),
        ("url", body.url.is_some()),
        ("event_types", body.event_types.is_some()),
        ("enabled", body.enabled.is_some()),
    ];
    let fields: Vec<&str> = given.iter().filter(|f| f.1).map(|f| f.0).collect();
    let hook = webhooks::update(&state.db, id, body).await?;
    config_changed(
        &state,
        actor,
        "webhook",
        "changed",
        id,
        Some(&hook.name),
        &fields,
    )
    .await;
    Ok(Json(hook))
}

#[utoipa::path(
    delete,
    path = "/webhooks/{id}",
    tag = "events",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn delete_webhook(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let name = webhooks::list(&state.db)
        .await?
        .into_iter()
        .find(|w| w.id == id)
        .map(|w| w.name);
    webhooks::delete(&state.db, id).await?;
    config_changed(
        &state,
        actor,
        "webhook",
        "removed",
        id,
        name.as_deref(),
        &[],
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, ToSchema)]
pub struct TestResult {
    ok: bool,
    error: Option<String>,
}

/// Sends a `webhook.test` event now, without moving the cursor.
#[utoipa::path(
    post,
    path = "/webhooks/{id}/test",
    tag = "events",
    params(("id" = i64, Path)),
    responses((status = 200, body = TestResult), (status = 404, body = ErrorBody))
)]
pub async fn test_webhook(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<TestResult>, ApiError> {
    let result = webhooks::Deliverer::new(state.db.clone()).test(id).await?;
    Ok(Json(TestResult {
        ok: result.is_ok(),
        error: result.err(),
    }))
}

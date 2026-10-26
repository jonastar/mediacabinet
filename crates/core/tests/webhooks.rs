//! Webhook delivery against a local receiver.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::post;
use cabinet_core::events;
use cabinet_core::webhooks::{self, Deliverer, WebhookInput};
use serde_json::{Value, json};
use sqlx::SqlitePool;

#[derive(Clone, Default)]
struct Receiver {
    bodies: Arc<Mutex<Vec<(String, String)>>>,
    fail: Arc<Mutex<bool>>,
}

async fn receiver() -> (Receiver, String) {
    let r = Receiver::default();
    let state = r.clone();
    let app = Router::new().route(
        "/hook",
        post(move |headers: HeaderMap, body: String| {
            let state = state.clone();
            async move {
                if *state.fail.lock().unwrap() {
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }
                let signature = headers["x-cabinet-signature"].to_str().unwrap().to_string();
                state.bodies.lock().unwrap().push((signature, body));
                StatusCode::OK
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/hook", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (r, url)
}

async fn emit(pool: &SqlitePool, kind: &str) {
    let mut conn = pool.acquire().await.unwrap();
    events::emit(&mut conn, kind, json!({ "n": 1 }))
        .await
        .unwrap();
}

#[tokio::test]
async fn delivers_signed_batches_in_order_and_resumes_after_failures() {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    let (rx, url) = receiver().await;
    emit(&pool, "file.added").await; // Before the subscription: not delivered.

    let hook = webhooks::create(
        &pool,
        WebhookInput {
            name: Some("test".into()),
            url: Some(url.clone()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let deliverer = Deliverer::new(pool.clone());
    emit(&pool, "file.added").await;
    emit(&pool, "item.wanted").await;

    assert_eq!(deliverer.deliver_due().await.unwrap(), 2);
    let (signature, body) = rx.bodies.lock().unwrap()[0].clone();
    assert_eq!(
        signature,
        webhooks::sign(&hook.secret, body.as_bytes()),
        "signed with the secret"
    );
    let body: Value = serde_json::from_str(&body).unwrap();
    let kinds: Vec<&str> = body["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["file.added", "item.wanted"]);
    assert_eq!(deliverer.deliver_due().await.unwrap(), 0, "nothing new");

    // The receiver is down: nothing is lost, and it backs off.
    *rx.fail.lock().unwrap() = true;
    emit(&pool, "file.missing").await;
    assert_eq!(deliverer.deliver_due().await.unwrap(), 0);
    let w = &webhooks::list(&pool).await.unwrap()[0];
    assert_eq!((w.failures, w.pending), (1, 1));
    assert!(w.last_error.as_deref().unwrap().contains("500"));
    assert_eq!(
        deliverer.deliver_due().await.unwrap(),
        0,
        "waiting out the backoff"
    );

    // Back up; editing the webhook retries straight away.
    *rx.fail.lock().unwrap() = false;
    webhooks::update(&pool, hook.webhook.id, WebhookInput::default())
        .await
        .unwrap();
    assert_eq!(deliverer.deliver_due().await.unwrap(), 1);
    let w = &webhooks::list(&pool).await.unwrap()[0];
    assert_eq!((w.failures, w.pending), (0, 0));
}

#[tokio::test]
async fn filters_by_event_type_and_skips_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    let (rx, url) = receiver().await;
    webhooks::create(
        &pool,
        WebhookInput {
            url: Some(url),
            event_types: Some(vec!["item.wanted".into()]),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let deliverer = Deliverer::new(pool.clone());
    emit(&pool, "file.added").await;
    emit(&pool, "file.added").await;
    assert_eq!(deliverer.deliver_due().await.unwrap(), 0);
    assert_eq!(
        webhooks::list(&pool).await.unwrap()[0].pending,
        0,
        "skipped, not stuck"
    );
    emit(&pool, "item.wanted").await;
    assert_eq!(deliverer.deliver_due().await.unwrap(), 1);
    assert_eq!(rx.bodies.lock().unwrap().len(), 1);

    assert!(
        webhooks::create(
            &pool,
            WebhookInput {
                url: Some("ftp://x".into()),
                ..Default::default()
            }
        )
        .await
        .is_err()
    );
}

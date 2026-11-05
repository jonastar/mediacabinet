//! What TVs and apps need beyond the web UI: signing in with a device token,
//! pairing through a browser, signed-in devices, media tokens, the server's
//! details and CORS for packaged TV apps. The plan is in `docs/clients.md`.

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use cabinet_core::accounts::{self, AccountError, Device, MediaToken, User};
use cabinet_core::pairing::{self, Poll};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::cors::{AllowOrigin, CorsLayer};
use utoipa::{IntoParams, ToSchema};

use super::accounts::{record_failed_login, record_login};
use super::auth::{self, SignedIn};
use super::error::ErrorBody;
use super::{ApiError, MIN_APP_VERSION};
use crate::AppState;

#[derive(Serialize, ToSchema)]
pub struct ServerInfo {
    /// What to call this server; `CABINET_SERVER_NAME`, or "Media Cabinet".
    name: String,
    version: &'static str,
    /// Apps older than this should ask to be updated.
    min_app_version: &'static str,
}

/// What an app shows on its connect screen. Public.
#[utoipa::path(get, path = "/server-info", tag = "devices", responses((status = 200, body = ServerInfo)))]
pub async fn server_info() -> Json<ServerInfo> {
    let name = std::env::var("CABINET_SERVER_NAME")
        .ok()
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Media Cabinet".into());
    Json(ServerInfo {
        name,
        version: env!("CARGO_PKG_VERSION"),
        min_app_version: MIN_APP_VERSION,
    })
}

#[derive(Deserialize, ToSchema)]
pub struct DeviceCredentials {
    name: String,
    password: String,
    /// Shown in Account, where it can be signed out.
    device_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct DeviceSession {
    /// Send as `Authorization: Bearer`. Lasts a year from last use.
    token: String,
    user: User,
}

/// Signs a device in with a name and password. Returns its token; no cookie.
#[utoipa::path(
    post,
    path = "/auth/device",
    tag = "devices",
    request_body = DeviceCredentials,
    responses((status = 200, body = DeviceSession), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody))
)]
pub async fn device_login(
    State(state): State<AppState>,
    Json(body): Json<DeviceCredentials>,
) -> Result<Json<DeviceSession>, ApiError> {
    match accounts::login_device(&state.db, &body.name, &body.password, &body.device_name).await {
        Ok((user, token)) => {
            let details = json!({ "via": "password", "device": body.device_name.trim() });
            record_login(&state, user.id, details).await;
            Ok(Json(DeviceSession { token, user }))
        }
        Err(AccountError::BadLogin) => {
            record_failed_login(&state, &body.name).await;
            Err(AccountError::BadLogin.into())
        }
        Err(e) => Err(e.into()),
    }
}

#[derive(Deserialize, ToSchema)]
pub struct PairingStart {
    /// Shown to the person approving, and in Account afterwards.
    device_name: String,
}

#[derive(Serialize, ToSchema)]
pub struct PairingStarted {
    /// For the person to enter, as `XXXX-XXXX`.
    code: String,
    /// For this device alone, to poll with.
    secret: String,
    /// Where to send the person on this server, with the code filled in. Open
    /// it in the system browser, or show it as text and a QR code.
    path: String,
    expires_in_seconds: u64,
    poll_every_seconds: u64,
}

/// Starts pairing a device. Someone signed in on the web approves the code, and
/// the device polls until then. Rate limited.
#[utoipa::path(
    post,
    path = "/auth/pair",
    tag = "devices",
    request_body = PairingStart,
    responses((status = 200, body = PairingStarted), (status = 400, body = ErrorBody), (status = 429, body = ErrorBody))
)]
pub async fn start_pairing(
    State(state): State<AppState>,
    Json(body): Json<PairingStart>,
) -> Result<Json<PairingStarted>, ApiError> {
    let started = state.pairings.start(&body.device_name)?;
    Ok(Json(PairingStarted {
        path: format!("/pair?code={}", started.code),
        code: started.code,
        secret: started.secret,
        expires_in_seconds: pairing::WAIT.as_secs(),
        poll_every_seconds: pairing::POLL_SECONDS,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct PairingPoll {
    secret: String,
}

#[derive(Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum PairingState {
    /// Nobody has answered yet: ask again.
    Waiting,
    /// Approved: `session` is set, and given only this once.
    Approved,
    Declined,
    /// Took too long, or was already collected: start again.
    Expired,
}

#[derive(Serialize, ToSchema)]
pub struct PairingStatus {
    state: PairingState,
    session: Option<DeviceSession>,
}

/// How a pairing is going, for the device that started it.
#[utoipa::path(
    post,
    path = "/auth/pair/poll",
    tag = "devices",
    request_body = PairingPoll,
    responses((status = 200, body = PairingStatus), (status = 401, body = ErrorBody))
)]
pub async fn poll_pairing(
    State(state): State<AppState>,
    Json(body): Json<PairingPoll>,
) -> Result<Json<PairingStatus>, ApiError> {
    let answered = |state| {
        Json(PairingStatus {
            state,
            session: None,
        })
    };
    match state.pairings.poll(&body.secret) {
        Poll::Waiting => Ok(answered(PairingState::Waiting)),
        Poll::Declined => Ok(answered(PairingState::Declined)),
        Poll::Expired => Ok(answered(PairingState::Expired)),
        Poll::Approved { user, device_name } => {
            let (user, token) =
                accounts::open_device_session(&state.db, user, &device_name).await?;
            let details = json!({ "via": "pairing", "device": device_name });
            record_login(&state, user.id, details).await;
            Ok(Json(PairingStatus {
                state: PairingState::Approved,
                session: Some(DeviceSession { token, user }),
            }))
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct PairingCode {
    code: String,
}

#[derive(Serialize, ToSchema)]
pub struct PairingRequest {
    device_name: String,
}

/// The device waiting behind a code, to check before approving it. Wrong codes
/// are limited per user.
#[utoipa::path(
    get,
    path = "/me/pair",
    tag = "devices",
    params(PairingCode),
    responses((status = 200, body = PairingRequest), (status = 400, body = ErrorBody), (status = 429, body = ErrorBody))
)]
pub async fn pairing(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
    Query(query): Query<PairingCode>,
) -> Result<Json<PairingRequest>, ApiError> {
    let device_name = state.pairings.device_name(me.id, &query.code)?;
    Ok(Json(PairingRequest { device_name }))
}

#[derive(Deserialize, ToSchema)]
pub struct PairingAnswer {
    code: String,
    approve: bool,
}

/// Signs the device behind a code in to your account, or turns it away.
#[utoipa::path(
    post,
    path = "/me/pair",
    tag = "devices",
    request_body = PairingAnswer,
    responses((status = 200, body = PairingRequest), (status = 400, body = ErrorBody), (status = 429, body = ErrorBody))
)]
pub async fn answer_pairing(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
    Json(body): Json<PairingAnswer>,
) -> Result<Json<PairingRequest>, ApiError> {
    let device_name = state.pairings.answer(me.id, &body.code, body.approve)?;
    Ok(Json(PairingRequest { device_name }))
}

/// TVs and apps signed in to your account.
#[utoipa::path(get, path = "/me/devices", tag = "devices", responses((status = 200, body = Vec<Device>)))]
pub async fn my_devices(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<Vec<Device>>, ApiError> {
    Ok(Json(accounts::devices(&state.db, me.id).await?))
}

/// Signs out one of your devices.
#[utoipa::path(
    delete,
    path = "/me/devices/{id}",
    tag = "devices",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 404, body = ErrorBody))
)]
pub async fn sign_out_device(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    if accounts::sign_out_device(&state.db, me.id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

/// A token for requests that can't send a header, such as `<img>` and `<video>`:
/// add `?media_token=` to artwork, stream and subtitle URLs. It lasts a day, or
/// until this session ends. Never put the device token itself in a URL.
#[utoipa::path(post, path = "/me/media-token", tag = "devices", responses((status = 200, body = MediaToken), (status = 400, body = ErrorBody)))]
pub async fn media_token(
    State(state): State<AppState>,
    _me: SignedIn,
    headers: HeaderMap,
) -> Result<Json<MediaToken>, ApiError> {
    let token = auth::session_token(&headers)
        .ok_or_else(|| ApiError::Unauthorized("Log in first.".into()))?;
    accounts::create_media_token(&state.db, &token)
        .await?
        .map(Json)
        .ok_or_else(|| ApiError::Unauthorized("Log in first.".into()))
}

/// The `Origin`s packaged TV apps are expected to send, until tested on real
/// TVs. `CABINET_CORS_ORIGINS` adds more, separated by commas, such as a dev
/// server's.
const APP_ORIGINS: &[&str] = &["null", "file://"];

/// Which sign-in endpoints a packaged app needs. The rest stay out of reach of
/// other sites: above all setup, which would let any page a person visits claim
/// a new server on their network.
fn cors_allows(path: &str) -> bool {
    match path.strip_prefix("/api/auth/") {
        Some(rest) => matches!(rest, "status" | "logout" | "pair" | "pair/poll"),
        None => true,
    }
}

/// CORS for packaged TV apps, which are cross-site to the server. They send
/// bearer tokens, so credentials stay off and no web session is exposed.
pub fn cors() -> CorsLayer {
    let mut origins: Vec<HeaderValue> = APP_ORIGINS
        .iter()
        .map(|origin| HeaderValue::from_static(origin))
        .collect();
    let extra = std::env::var("CABINET_CORS_ORIGINS").unwrap_or_default();
    origins.extend(
        extra
            .split(',')
            .filter_map(|origin| HeaderValue::from_str(origin.trim()).ok())
            .filter(|origin| !origin.is_empty()),
    );
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, request| {
            origins.contains(origin) && cors_allows(request.uri.path())
        }))
        .allow_methods([
            Method::GET,
            Method::HEAD,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
        ])
        .allow_headers([AUTHORIZATION, CONTENT_TYPE])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use tower::ServiceExt;

    fn app() -> Router {
        Router::new()
            .route("/api/home", get(|| async { "home" }))
            .route("/api/auth/setup", post(|| async { "setup" }))
            .route("/api/auth/pair", post(|| async { "pair" }))
            .layer(cors())
    }

    async fn preflight(path: &str, origin: &str) -> Option<String> {
        let response = app()
            .oneshot(
                Request::options(path)
                    .header("origin", origin)
                    .header("access-control-request-method", "POST")
                    .header("access-control-request-headers", "authorization")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.headers().get("access-control-allow-credentials"),
            None
        );
        response
            .headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap().to_string())
    }

    #[tokio::test]
    async fn packaged_apps_are_allowed_without_credentials() {
        assert_eq!(
            preflight("/api/home", "null").await.as_deref(),
            Some("null")
        );
        assert_eq!(
            preflight("/api/auth/pair", "file://").await.as_deref(),
            Some("file://")
        );
    }

    #[tokio::test]
    async fn other_sites_are_not() {
        assert_eq!(preflight("/api/home", "https://evil.example").await, None);
    }

    #[tokio::test]
    async fn setup_stays_out_of_reach() {
        assert_eq!(preflight("/api/auth/setup", "null").await, None);
    }

    #[test]
    fn sign_in_endpoints_are_opt_in() {
        assert!(cors_allows("/api/home"));
        assert!(cors_allows("/api/auth/pair/poll"));
        assert!(cors_allows("/api/auth/logout"));
        assert!(!cors_allows("/api/auth/setup"));
        assert!(!cors_allows("/api/auth/login"));
        assert!(!cors_allows("/api/auth/device"));
        assert!(!cors_allows("/api/auth/oidc/x/start"));
    }
}

//! Who is calling, and what they may do.
//!
//! Every `/api` request needs a session cookie (the web UI), a device token
//! (`Authorization: Bearer <token>`, for TVs and apps) or an API key
//! (`X-Api-Key: <key>` or `?apikey=<key>`, for tools), except health, server
//! info, the login endpoints and the OpenAPI document. API keys act as an admin.
//!
//! Requests that can't send a header (`<img>`, `<video>`) may use a media token
//! (`?media_token=<token>`) instead, to read artwork, streams and subtitles and
//! nothing else.
//!
//! Changes need an admin, apart from a user's own things: their account, watch
//! state, views, playlists and watch activity. A few reads need an admin too: keys, webhooks,
//! users, the review queue, tasks, an item's event history, a file's probe and
//! browsing the server's folders.
//!
//! Users who aren't active (waiting for approval) may do nothing but sign out.

use axum::extract::{FromRequestParts, Request, State};
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, Uri};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use cabinet_core::accounts::{User, UserStatus};

use super::ApiError;
use crate::AppState;

pub const SESSION_COOKIE: &str = "mc_session";

#[derive(Clone)]
pub enum Caller {
    User(User),
    Key,
}

fn is_public(path: &str) -> bool {
    path == "/api/health"
        || path == "/api/server-info"
        || path == "/api/openapi.json"
        || path.starts_with("/api/auth/")
}

/// What a media token may read.
fn is_media_read(method: &Method, path: &str) -> bool {
    if method != Method::GET && method != Method::HEAD {
        return false;
    }
    let Some(path) = path.strip_prefix("/api/") else {
        return false;
    };
    let parts: Vec<&str> = path.split('/').collect();
    matches!(
        parts.as_slice(),
        ["artwork", _, _]
            | ["people", _, "photo"]
            | ["files", _, "stream"]
            | ["files", _, "subtitles", _]
            | ["play", _, _]
            | ["sidecars", _, "subtitles"]
    )
}

const ADMIN_READS: &[&str] = &[
    "/api/api-keys",
    "/api/auth-providers",
    "/api/webhooks",
    "/api/users",
    "/api/review",
    "/api/folders",
    "/api/metadata/",
    "/api/tasks",
];

/// Changes any signed-in user may make.
fn is_own_change(path: &str) -> bool {
    if path == "/api/me"
        || path.starts_with("/api/me/")
        || path.starts_with("/api/playlists")
        || path.starts_with("/api/play/")
    {
        return true;
    }
    if path == "/api/activity/mine/watch" {
        return true;
    }
    if path.starts_with("/api/series/") && path.ends_with("/view") {
        return true;
    }
    path.starts_with("/api/items/")
        && (path.ends_with("/progress") || path.ends_with("/played") || path.ends_with("/play"))
}

fn needs_admin(method: &Method, path: &str) -> bool {
    if method == Method::GET || method == Method::HEAD {
        ADMIN_READS.iter().any(|prefix| path.starts_with(prefix))
            || (path.starts_with("/api/items/") && path.ends_with("/events"))
            || (path.starts_with("/api/files/") && path.ends_with("/probe"))
    } else {
        !is_own_change(path)
    }
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(axum::http::header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .find_map(|pair| {
            let (k, v) = pair.trim().split_once('=')?;
            (k == name).then(|| v.to_string())
        })
}

/// The token of the session this request claims: a device's bearer token, or
/// else the session cookie.
pub fn session_token(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split_once(' '))
        .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
        .map(|(_, token)| token.trim().to_string());
    bearer.or_else(|| cookie(headers, SESSION_COOKIE))
}

fn api_key(headers: &HeaderMap, uri: &Uri) -> Option<String> {
    let header = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    header.or_else(|| {
        uri.query()
            .and_then(|q| url_param(q, "apikey").or_else(|| url_param(q, "api_key")))
    })
}

async fn identify(
    state: &AppState,
    method: &Method,
    headers: &HeaderMap,
    uri: &Uri,
) -> Result<Option<Caller>, ApiError> {
    if let Some(token) = session_token(headers)
        && let Some(user) = cabinet_core::accounts::session_user(&state.db, &token).await?
    {
        return Ok(Some(Caller::User(user)));
    }
    if let Some(key) = api_key(headers, uri) {
        if cabinet_core::apikeys::verify(&state.db, &key)
            .await?
            .is_some()
        {
            return Ok(Some(Caller::Key));
        }
        return Err(ApiError::Unauthorized("That API key isn't valid.".into()));
    }
    if is_media_read(method, uri.path())
        && let Some(token) = uri.query().and_then(|q| url_param(q, "media_token"))
        && let Some(user) = cabinet_core::accounts::media_token_user(&state.db, &token).await?
    {
        return Ok(Some(Caller::User(user)));
    }
    Ok(None)
}

/// Middleware: identifies the caller and enforces the rules above.
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if !path.starts_with("/api/") || is_public(&path) {
        return next.run(request).await;
    }
    let caller = match identify(&state, request.method(), request.headers(), request.uri()).await {
        Ok(Some(caller)) => caller,
        Ok(None) => {
            return ApiError::Unauthorized("Log in, or send a device token or an API key.".into())
                .into_response();
        }
        Err(e) => return e.into_response(),
    };
    if let Caller::User(user) = &caller
        && user.status != UserStatus::Active
    {
        return ApiError::ForbiddenBecause("Waiting for an admin to let you in.".into())
            .into_response();
    }
    let admin = match &caller {
        Caller::Key => true,
        Caller::User(user) => user.is_admin,
    };
    if !admin && needs_admin(request.method(), &path) {
        return ApiError::Forbidden.into_response();
    }
    request.extensions_mut().insert(caller);
    next.run(request).await
}

fn caller(parts: &Parts) -> Result<Caller, ApiError> {
    parts
        .extensions
        .get::<Caller>()
        .cloned()
        .ok_or_else(|| ApiError::Unauthorized("Log in first.".into()))
}

/// The user per-user state belongs to: the signed-in user, or for an API key,
/// the first admin.
pub struct Viewer(pub i64);

impl FromRequestParts<AppState> for Viewer {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match caller(parts)? {
            Caller::User(user) => Ok(Viewer(user.id)),
            Caller::Key => cabinet_core::catalog::default_user(&state.db)
                .await?
                .map(Viewer)
                .ok_or_else(|| ApiError::BadRequest("There are no users yet.".into())),
        }
    }
}

/// The viewer plus whether they're an admin (API keys are).
pub struct Actor(pub cabinet_core::playlists::Actor);

impl FromRequestParts<AppState> for Actor {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let admin = match caller(parts)? {
            Caller::User(user) => user.is_admin,
            Caller::Key => true,
        };
        let Viewer(user) = Viewer::from_request_parts(parts, state).await?;
        Ok(Actor(cabinet_core::playlists::Actor { user, admin }))
    }
}

/// Who to credit with a change in the activity log: the signed-in user, or no
/// one for an API key.
pub struct ActingUser(pub Option<i64>);

impl FromRequestParts<AppState> for ActingUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match caller(parts)? {
            Caller::User(user) => Ok(ActingUser(Some(user.id))),
            Caller::Key => Ok(ActingUser(None)),
        }
    }
}

/// A signed-in user; API keys don't count.
pub struct SignedIn(pub User);

impl FromRequestParts<AppState> for SignedIn {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        match caller(parts)? {
            Caller::User(user) => Ok(SignedIn(user)),
            Caller::Key => Err(ApiError::BadRequest(
                "This needs a signed-in user, not an API key.".into(),
            )),
        }
    }
}

/// Extractor for endpoints made for tools: requires a valid API key.
pub struct ApiKeyAuth;

impl FromRequestParts<AppState> for ApiKeyAuth {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let Some(key) = api_key(&parts.headers, &parts.uri) else {
            return Err(ApiError::Unauthorized(
                "An API key is required: X-Api-Key header or ?apikey=.".into(),
            ));
        };
        match cabinet_core::apikeys::verify(&state.db, &key).await? {
            Some(_) => Ok(ApiKeyAuth),
            None => Err(ApiError::Unauthorized("That API key isn't valid.".into())),
        }
    }
}

fn url_param(query: &str, name: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=')?;
        (k == name).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy() {
        assert!(!needs_admin(&Method::GET, "/api/series/1"));
        assert!(needs_admin(&Method::GET, "/api/api-keys"));
        assert!(needs_admin(&Method::GET, "/api/folders?path=/"));
        assert!(needs_admin(&Method::GET, "/api/items/4/events"));
        assert!(needs_admin(&Method::GET, "/api/files/3/probe"));
        assert!(!needs_admin(&Method::GET, "/api/files/3/stream"));
        assert!(needs_admin(&Method::DELETE, "/api/files/3/links/4"));
        assert!(needs_admin(&Method::POST, "/api/libraries"));
        assert!(needs_admin(&Method::PATCH, "/api/items/4"));
        assert!(!needs_admin(&Method::PUT, "/api/series/1/view"));
        assert!(!needs_admin(&Method::PUT, "/api/items/4/progress"));
        assert!(!needs_admin(&Method::POST, "/api/items/4/play"));
        assert!(!needs_admin(&Method::DELETE, "/api/play/abc"));
        assert!(!needs_admin(&Method::POST, "/api/playlists"));
        assert!(!needs_admin(&Method::PATCH, "/api/me"));
        assert!(!needs_admin(&Method::DELETE, "/api/me/logins/3"));
        assert!(needs_admin(&Method::GET, "/api/auth-providers"));
        assert!(needs_admin(&Method::POST, "/api/auth-providers"));
        assert!(needs_admin(&Method::POST, "/api/auth-providers/try"));
        assert!(needs_admin(&Method::GET, "/api/auth-providers/tests/abc"));
        assert!(needs_admin(&Method::PUT, "/api/auth-settings"));
        assert!(needs_admin(&Method::PUT, "/api/users/2/status"));
        assert!(is_public("/api/auth/oidc/authentik/callback"));
        assert!(!is_public("/api/auth-providers"));
        assert!(needs_admin(&Method::PATCH, "/api/users/2"));
        assert!(!needs_admin(&Method::GET, "/api/activity"));
        assert!(!needs_admin(&Method::DELETE, "/api/activity/mine/watch"));
        assert!(needs_admin(&Method::DELETE, "/api/activity/3"));
        assert!(needs_admin(&Method::GET, "/api/tasks"));
        assert!(needs_admin(&Method::GET, "/api/tasks/3"));
        assert!(needs_admin(&Method::POST, "/api/tasks/3/cancel"));
        assert!(needs_admin(&Method::POST, "/api/libraries/3/thumbnails"));
        assert!(!needs_admin(&Method::GET, "/api/artwork/3/thumbnail"));
        assert!(!needs_admin(&Method::GET, "/api/me/devices"));
        assert!(!needs_admin(&Method::DELETE, "/api/me/devices/3"));
        assert!(!needs_admin(&Method::POST, "/api/me/pair"));
        assert!(!needs_admin(&Method::POST, "/api/me/media-token"));
        assert!(is_public("/api/server-info"));
        assert!(is_public("/api/auth/pair"));
        assert!(is_public("/api/auth/pair/poll"));
        assert!(!is_public("/api/me/pair"));
    }

    #[test]
    fn media_tokens_reach_media_only() {
        for path in [
            "/api/artwork/3/poster",
            "/api/people/3/photo",
            "/api/files/3/stream",
            "/api/files/3/subtitles/2",
            "/api/play/abc/index.m3u8",
            "/api/sidecars/3/subtitles",
        ] {
            assert!(is_media_read(&Method::GET, path), "{path}");
            assert!(is_media_read(&Method::HEAD, path), "{path}");
            assert!(!is_media_read(&Method::POST, path), "{path}");
            assert!(!is_media_read(&Method::DELETE, path), "{path}");
        }
        for path in [
            "/api/me",
            "/api/me/media-token",
            "/api/me/devices",
            "/api/home",
            "/api/users",
            "/api/api-keys",
            // A stream's stats tell admins the ffmpeg command, and DELETE stops it.
            "/api/play/abc",
            "/api/files/3/probe",
            "/api/files/3/stream/extra",
            "/api/artwork/3",
            "/artwork/3/poster",
        ] {
            assert!(!is_media_read(&Method::GET, path), "{path}");
        }
    }

    #[test]
    fn session_tokens_come_from_the_header_or_the_cookie() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut map = HeaderMap::new();
            for (name, value) in pairs {
                map.insert(*name, value.parse().unwrap());
            }
            map
        };
        assert_eq!(session_token(&headers(&[])), None);
        assert_eq!(
            session_token(&headers(&[("authorization", "Bearer abc")])).as_deref(),
            Some("abc")
        );
        assert_eq!(
            session_token(&headers(&[("authorization", "bearer abc")])).as_deref(),
            Some("abc")
        );
        assert_eq!(
            session_token(&headers(&[("cookie", "mc_session=def")])).as_deref(),
            Some("def")
        );
        assert_eq!(
            session_token(&headers(&[
                ("authorization", "Bearer abc"),
                ("cookie", "mc_session=def")
            ]))
            .as_deref(),
            Some("abc")
        );
        // Another scheme, such as a proxy's own password, isn't a device token.
        assert_eq!(
            session_token(&headers(&[
                ("authorization", "Basic abc"),
                ("cookie", "mc_session=def")
            ]))
            .as_deref(),
            Some("def")
        );
    }
}

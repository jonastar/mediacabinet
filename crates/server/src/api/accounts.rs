//! Logging in, the first-run setup, your own account, and managing users.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use cabinet_core::accounts::{self, User};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::ApiError;
use super::auth::{self, SESSION_COOKIE, SignedIn};
use super::error::ErrorBody;
use crate::AppState;

#[derive(Serialize, ToSchema)]
pub struct AuthStatus {
    /// No account has a password yet: show the setup screen.
    setup_needed: bool,
    /// The signed-in user, if any.
    user: Option<User>,
}

#[derive(Deserialize, ToSchema)]
pub struct Credentials {
    name: String,
    password: String,
}

/// Secure when the request reached us over HTTPS (directly or via a proxy).
fn session_cookie(headers: &HeaderMap, value: &str, max_age: i64) -> HeaderValue {
    let https = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let secure = if https {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("cookie is ASCII")
}

fn signed_in(headers: &HeaderMap, user: User, token: &str) -> Response {
    let cookie = session_cookie(headers, token, accounts::SESSION_DAYS * 86400);
    ([(SET_COOKIE, cookie)], Json(user)).into_response()
}

/// Whether the app needs setting up, and who is signed in.
#[utoipa::path(get, path = "/auth/status", tag = "accounts", responses((status = 200, body = AuthStatus)))]
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<AuthStatus>, ApiError> {
    let user = match auth::cookie(&headers, SESSION_COOKIE) {
        Some(token) => accounts::session_user(&state.db, &token).await?,
        None => None,
    };
    Ok(Json(AuthStatus {
        setup_needed: accounts::setup_needed(&state.db).await?,
        user,
    }))
}

/// First run: creates the admin account and signs it in. Refused once set up.
#[utoipa::path(
    post,
    path = "/auth/setup",
    tag = "accounts",
    request_body = Credentials,
    responses((status = 200, body = User), (status = 400, body = ErrorBody))
)]
pub async fn setup(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    accounts::setup(&state.db, &body.name, &body.password).await?;
    let (user, token) = accounts::login(&state.db, &body.name, &body.password).await?;
    Ok(signed_in(&headers, user, &token))
}

/// Signs in with a name and password, setting the session cookie.
#[utoipa::path(
    post,
    path = "/auth/login",
    tag = "accounts",
    request_body = Credentials,
    responses((status = 200, body = User), (status = 401, body = ErrorBody))
)]
pub async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Credentials>,
) -> Result<Response, ApiError> {
    let (user, token) = accounts::login(&state.db, &body.name, &body.password).await?;
    Ok(signed_in(&headers, user, &token))
}

/// Ends this session.
#[utoipa::path(post, path = "/auth/logout", tag = "accounts", responses((status = 204)))]
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = auth::cookie(&headers, SESSION_COOKIE) {
        accounts::logout(&state.db, &token).await?;
    }
    let cookie = session_cookie(&headers, "", 0);
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response())
}

#[derive(Deserialize, ToSchema)]
pub struct MeUpdate {
    name: Option<String>,
    /// A new password; needs `current_password`.
    password: Option<String>,
    current_password: Option<String>,
}

/// Changes your own name or password. A new password signs out other sessions.
#[utoipa::path(
    patch,
    path = "/me",
    tag = "accounts",
    request_body = MeUpdate,
    responses((status = 200, body = User), (status = 400, body = ErrorBody))
)]
pub async fn update_me(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
    Json(body): Json<MeUpdate>,
) -> Result<Json<User>, ApiError> {
    if body.password.is_some() {
        let current = body.current_password.as_deref().unwrap_or_default();
        if !accounts::check_password(&state.db, me.id, current).await? {
            return Err(ApiError::BadRequest(
                "Your current password isn't right.".into(),
            ));
        }
    }
    Ok(Json(
        accounts::update(
            &state.db,
            me.id,
            body.name.as_deref(),
            body.password.as_deref(),
            None,
        )
        .await?,
    ))
}

/// Everyone with an account.
#[utoipa::path(get, path = "/users", tag = "accounts", responses((status = 200, body = Vec<User>)))]
pub async fn list_users(State(state): State<AppState>) -> Result<Json<Vec<User>>, ApiError> {
    Ok(Json(accounts::list(&state.db).await?))
}

#[derive(Deserialize, ToSchema)]
pub struct NewUser {
    name: String,
    password: String,
    #[serde(default)]
    is_admin: bool,
}

/// Adds an account.
#[utoipa::path(
    post,
    path = "/users",
    tag = "accounts",
    request_body = NewUser,
    responses((status = 201, body = User), (status = 400, body = ErrorBody))
)]
pub async fn create_user(
    State(state): State<AppState>,
    Json(body): Json<NewUser>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let user = accounts::create(&state.db, &body.name, &body.password, body.is_admin).await?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Deserialize, ToSchema)]
pub struct UserUpdate {
    name: Option<String>,
    /// Sets a new password, signing the user out everywhere.
    password: Option<String>,
    is_admin: Option<bool>,
}

/// Renames a user, resets their password or changes whether they're an admin.
#[utoipa::path(
    patch,
    path = "/users/{id}",
    tag = "accounts",
    params(("id" = i64, Path)),
    request_body = UserUpdate,
    responses((status = 200, body = User), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<UserUpdate>,
) -> Result<Json<User>, ApiError> {
    Ok(Json(
        accounts::update(
            &state.db,
            id,
            body.name.as_deref(),
            body.password.as_deref(),
            body.is_admin,
        )
        .await?,
    ))
}

/// Removes an account and its watch state. The last admin can't be removed.
#[utoipa::path(
    delete,
    path = "/users/{id}",
    tag = "accounts",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    accounts::delete(&state.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

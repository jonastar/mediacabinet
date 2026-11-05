//! Logging in, the first-run setup, your own account, and managing users.
//! Signing in with another account is in [`super::sign_in`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use cabinet_core::accounts::{self, AccountError, User, UserStatus};
use cabinet_core::activity::{self, NewEntry};
use cabinet_core::catalog::now;
use cabinet_core::sign_in::providers::{self, ProviderButton};
use cabinet_core::sign_in::{self, Login};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::ToSchema;

use super::ApiError;
use super::auth::{self, ActingUser, SESSION_COOKIE, SignedIn};
use super::error::ErrorBody;
use crate::AppState;

#[derive(Serialize, ToSchema)]
pub struct AuthStatus {
    /// No account has a password yet: show the setup screen.
    setup_needed: bool,
    /// The signed-in user, if any. A pending one may only sign out.
    user: Option<User>,
    /// Other accounts people can sign in with.
    providers: Vec<ProviderButton>,
    /// Whether the login page shows the password form up front.
    password_sign_in: bool,
}

#[derive(Deserialize, ToSchema)]
pub struct Credentials {
    name: String,
    password: String,
}

/// Whether the request reached us over HTTPS (directly or via a proxy).
pub(super) fn https(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("https"))
}

/// An HttpOnly, SameSite=Lax cookie; Secure over HTTPS.
pub(super) fn cookie(
    headers: &HeaderMap,
    name: &str,
    path: &str,
    value: &str,
    max_age: i64,
) -> HeaderValue {
    let secure = if https(headers) {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{name}={value}; Path={path}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("cookie is ASCII")
}

pub(super) fn session_cookie(headers: &HeaderMap, value: &str, max_age: i64) -> HeaderValue {
    cookie(headers, SESSION_COOKIE, "/", value, max_age)
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
    let user = match auth::session_token(&headers) {
        Some(token) => accounts::session_user(&state.db, &token).await?,
        None => None,
    };
    Ok(Json(AuthStatus {
        setup_needed: accounts::setup_needed(&state.db).await?,
        user,
        providers: providers::buttons(&state.db).await?,
        password_sign_in: providers::password_sign_in(&state.db, state.sign_in.password_env())
            .await?,
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
    match accounts::login(&state.db, &body.name, &body.password).await {
        Ok((user, token)) => {
            record_login(&state, user.id, json!({ "via": "password" })).await;
            Ok(signed_in(&headers, user, &token))
        }
        Err(AccountError::BadLogin) => {
            record_failed_login(&state, &body.name).await;
            Err(AccountError::BadLogin.into())
        }
        Err(e) => Err(e.into()),
    }
}

pub(super) async fn record_login(state: &AppState, user: i64, details: serde_json::Value) {
    let mut entry = NewEntry::admins(activity::LOGIN, now(), Some(user), details);
    entry.subject = Some(user);
    activity::record_or_warn(&state.db, entry).await;
}

pub(super) async fn record_failed_login(state: &AppState, name: &str) {
    let name: String = name.trim().chars().take(64).collect();
    let entry = NewEntry::admins(activity::LOGIN_FAILED, now(), None, json!({ "name": name }));
    activity::record_or_warn(&state.db, entry).await;
}

/// Ends this session, or for a device, signs it out.
#[utoipa::path(post, path = "/auth/logout", tag = "accounts", responses((status = 204)))]
pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    if let Some(token) = auth::session_token(&headers) {
        accounts::logout(&state.db, &token).await?;
    }
    let cookie = session_cookie(&headers, "", 0);
    Ok((StatusCode::NO_CONTENT, [(SET_COOKIE, cookie)]).into_response())
}

#[derive(Deserialize, ToSchema)]
pub struct MeUpdate {
    name: Option<String>,
    /// Empty to remove it.
    email: Option<String>,
    /// A new password; needs `current_password` if there is one.
    password: Option<String>,
    current_password: Option<String>,
    /// Hide what you watch from other users (not from admins), from now on.
    incognito: Option<bool>,
}

/// Changes your own name, email or password. A new password signs out your
/// other sessions; this one gets a new cookie.
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
    headers: HeaderMap,
    Json(body): Json<MeUpdate>,
) -> Result<Response, ApiError> {
    if let Some(on) = body.incognito {
        accounts::set_incognito(&state.db, me.id, on).await?;
    }
    if let Some(email) = &body.email {
        accounts::set_email(&state.db, me.id, email).await?;
    }
    let Some(password) = body.password else {
        let user = accounts::update(&state.db, me.id, body.name.as_deref(), None, None).await?;
        return Ok(Json(user).into_response());
    };
    // Someone who only signs in with another account sets a first password.
    let current = body.current_password.as_deref().unwrap_or_default();
    if accounts::has_password(&state.db, me.id).await?
        && !accounts::check_password(&state.db, me.id, current).await?
    {
        return Err(ApiError::BadRequest(
            "Your current password isn't right.".into(),
        ));
    }
    let user = accounts::update(
        &state.db,
        me.id,
        body.name.as_deref(),
        Some(&password),
        None,
    )
    .await?;
    // Every session ended; this one carries on with a new one.
    let (user, token) = accounts::login(&state.db, &user.name, &password).await?;
    Ok(signed_in(&headers, user, &token))
}

/// A user as admins see them.
#[derive(Serialize, ToSchema)]
pub struct ManagedUser {
    #[serde(flatten)]
    user: User,
    has_password: bool,
    /// Accounts at sign-in providers.
    logins: Vec<Login>,
    /// The provider whose groups decide whether they're an admin, if any.
    role_from: Option<String>,
}

/// Everyone with an account.
#[utoipa::path(get, path = "/users", tag = "accounts", responses((status = 200, body = Vec<ManagedUser>)))]
pub async fn list_users(State(state): State<AppState>) -> Result<Json<Vec<ManagedUser>>, ApiError> {
    let mut all = vec![];
    for user in accounts::list(&state.db).await? {
        all.push(ManagedUser {
            has_password: accounts::has_password(&state.db, user.id).await?,
            logins: sign_in::logins(&state.db, user.id).await?,
            role_from: sign_in::role_source(&state.db, user.id).await?,
            user,
        });
    }
    Ok(Json(all))
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
    ActingUser(actor): ActingUser,
    Json(body): Json<NewUser>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let user = accounts::create(&state.db, &body.name, &body.password, body.is_admin).await?;
    let mut entry = NewEntry::admins(
        activity::USER_CREATED,
        now(),
        actor,
        json!({ "name": user.name, "is_admin": user.is_admin }),
    );
    entry.subject = Some(user.id);
    activity::record_or_warn(&state.db, entry).await;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Deserialize, ToSchema)]
pub struct UserUpdate {
    name: Option<String>,
    /// Empty to remove it.
    email: Option<String>,
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
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<UserUpdate>,
) -> Result<Json<User>, ApiError> {
    let before = accounts::user(&state.db, id).await?;
    if body.is_admin.is_some_and(|admin| admin != before.is_admin)
        && let Some(provider) = sign_in::role_source(&state.db, id).await?
    {
        return Err(ApiError::BadRequest(format!(
            "{provider} decides whether {} is an admin, from their groups.",
            before.name
        )));
    }
    if let Some(email) = &body.email {
        accounts::set_email(&state.db, id, email).await?;
    }
    let user = accounts::update(
        &state.db,
        id,
        body.name.as_deref(),
        body.password.as_deref(),
        body.is_admin,
    )
    .await?;
    let changed = [
        ("name", user.name != before.name),
        ("email", user.email != before.email),
        ("password", body.password.is_some()),
        ("is_admin", user.is_admin != before.is_admin),
    ];
    let fields: Vec<&str> = changed.iter().filter(|c| c.1).map(|c| c.0).collect();
    if !fields.is_empty() {
        let mut entry = NewEntry::admins(
            activity::USER_CHANGED,
            now(),
            actor,
            json!({ "name": user.name, "is_admin": user.is_admin, "fields": fields }),
        );
        entry.subject = Some(user.id);
        activity::record_or_warn(&state.db, entry).await;
    }
    Ok(Json(user))
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
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    let user = accounts::user(&state.db, id).await?;
    accounts::delete(&state.db, id).await?;
    // Not about them: what's about a removed user goes with them.
    let entry = NewEntry::admins(
        activity::USER_REMOVED,
        now(),
        actor,
        json!({ "name": user.name }),
    );
    activity::record_or_warn(&state.db, entry).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct StatusChange {
    status: UserStatus,
}

/// Approves or declines someone waiting, or turns a user off or back on.
/// Turning someone off signs them out; admins can't be turned off.
#[utoipa::path(
    put,
    path = "/users/{id}/status",
    tag = "accounts",
    params(("id" = i64, Path)),
    request_body = StatusChange,
    responses((status = 200, body = User), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn set_user_status(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(id): Path<i64>,
    Json(body): Json<StatusChange>,
) -> Result<Json<User>, ApiError> {
    let before = accounts::user(&state.db, id).await?;
    if body.status == UserStatus::Pending {
        return Err(ApiError::BadRequest(
            "Approve or decline; nobody goes back to waiting.".into(),
        ));
    }
    let user = accounts::set_status(&state.db, id, body.status).await?;
    if user.status != before.status {
        let mut entry = NewEntry::admins(
            activity::USER_STATUS,
            now(),
            actor,
            json!({ "name": user.name, "status": user.status, "from": before.status }),
        );
        entry.subject = Some(user.id);
        activity::record_or_warn(&state.db, entry).await;
    }
    Ok(Json(user))
}

/// How you sign in: a password, linked accounts, and providers to link.
#[derive(Serialize, ToSchema)]
pub struct MySignIn {
    has_password: bool,
    logins: Vec<Login>,
    providers: Vec<ProviderButton>,
}

#[utoipa::path(get, path = "/me/sign-in", tag = "accounts", responses((status = 200, body = MySignIn)))]
pub async fn my_sign_in(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
) -> Result<Json<MySignIn>, ApiError> {
    Ok(Json(MySignIn {
        has_password: accounts::has_password(&state.db, me.id).await?,
        logins: sign_in::logins(&state.db, me.id).await?,
        providers: providers::buttons(&state.db).await?,
    }))
}

/// Unlinks one of your accounts, if you still have another way in.
#[utoipa::path(
    delete,
    path = "/me/logins/{id}",
    tag = "accounts",
    params(("id" = i64, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn unlink_mine(
    State(state): State<AppState>,
    SignedIn(me): SignedIn,
    Path(id): Path<i64>,
) -> Result<StatusCode, ApiError> {
    sign_in::unlink(&state.db, me.id, id, Some(me.id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Unlinks a user's account, if they still have another way in.
#[utoipa::path(
    delete,
    path = "/users/{id}/logins/{login}",
    tag = "accounts",
    params(("id" = i64, Path), ("login" = i64, Path)),
    responses((status = 204), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn unlink_user(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path((id, login)): Path<(i64, i64)>,
) -> Result<StatusCode, ApiError> {
    sign_in::unlink(&state.db, id, login, actor).await?;
    Ok(StatusCode::NO_CONTENT)
}

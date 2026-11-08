//! Signing in with another account: the round trip to the provider, the "Is
//! this you?" step, and setting providers up. Design: `docs/sign-in.md`.
//!
//! The round trip is two browser redirects, so its endpoints answer with
//! redirects back into the web app rather than JSON: `/` when signed in,
//! `/?sign_in=confirm` to ask "Is this you?", and `?sign_in_error=` otherwise.
//! A test sign-in's window ends on a small page that hands the report's token
//! to the admin's page.

use axum::Json;
use axum::extract::{Form, Path, Query, State};
use axum::http::header::{CONTENT_TYPE, LOCATION, SET_COOKIE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use cabinet_core::accounts::{self, User};
use cabinet_core::activity::{self, NewEntry};
use cabinet_core::catalog::now;
use cabinet_core::guests;
use cabinet_core::sign_in::providers::{self, ProviderButton, ProviderInput, ProviderView};
use cabinet_core::sign_in::{self, CheckReport, Outcome, Purpose, SignInError, TestReport};
use serde::{Deserialize, Serialize};
use serde_json::json;
use utoipa::{IntoParams, ToSchema};

use super::ApiError;
use super::accounts::{cookie, https, session_cookie};
use super::activity::config_changed;
use super::auth::{self, ActingUser, SESSION_COOKIE};
use super::error::ErrorBody;
use crate::AppState;

/// Ties a started sign-in to this browser; holds its `state`.
const FLOW_COOKIE: &str = "mc_oidc";
/// An account waiting for "Is this you?".
const LINK_COOKIE: &str = "mc_oidc_link";
const COOKIE_PATH: &str = "/api/auth/oidc";
const WAIT_SECONDS: i64 = 10 * 60;

/// `https://media.example.com/api/auth/oidc/` as the browser sees this
/// server, behind a proxy or not.
fn callback_base(headers: &HeaderMap) -> String {
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(',').next())
            .map(|v| v.trim().to_string())
    };
    let scheme = if https(headers) {
        "https"
    } else {
        "http"
    };
    let host = header("x-forwarded-host")
        .or_else(|| header("host"))
        .unwrap_or_else(|| "localhost".into());
    format!("{scheme}://{host}/api/auth/oidc/")
}

fn redirect_uri(headers: &HeaderMap, provider: &sign_in::Provider) -> String {
    match &provider.settings.redirect_url {
        Some(url) => url.clone(),
        None => format!("{}{}/callback", callback_base(headers), provider.key),
    }
}

fn redirect(to: &str, cookies: Vec<HeaderValue>) -> Response {
    let mut response = (StatusCode::SEE_OTHER, [(LOCATION, to.to_string())]).into_response();
    for c in cookies {
        response.headers_mut().append(SET_COOKIE, c);
    }
    response
}

fn encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// What to tell the person when a sign-in fails; details go to the log.
fn reason(err: &SignInError) -> String {
    match err {
        SignInError::Db(_) => "Something went wrong. The server log has the details.".into(),
        other => other.to_string(),
    }
}

fn failed(page: &str, err: &SignInError, cookies: Vec<HeaderValue>) -> Response {
    redirect(
        &format!("{page}?sign_in_error={}", encode(&reason(err))),
        cookies,
    )
}

async fn session_user(state: &AppState, headers: &HeaderMap) -> Result<Option<User>, ApiError> {
    match auth::cookie(headers, SESSION_COOKIE) {
        Some(token) => Ok(accounts::session_user(&state.db, &token).await?),
        None => Ok(None),
    }
}

async fn enabled_provider(state: &AppState, key: &str) -> Result<sign_in::Provider, ApiError> {
    match providers::by_key(&state.db, key).await? {
        Some(p) if p.settings.enabled => Ok(p),
        _ => Err(ApiError::NotFound),
    }
}

#[derive(Deserialize, IntoParams)]
pub struct StartQuery {
    /// Link the account to the signed-in user instead of signing in.
    #[serde(default)]
    link: bool,
    /// Also complete the waiting "Is this you?" if this turns out to be them.
    #[serde(default)]
    then_confirm: bool,
}

/// Sends the browser to the provider to sign in, or to link an account.
#[utoipa::path(
    get,
    path = "/auth/oidc/{key}/start",
    tag = "sign-in",
    params(("key" = String, Path), StartQuery),
    responses((status = 303, description = "To the provider"), (status = 404, body = ErrorBody))
)]
pub async fn start(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<StartQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let provider = enabled_provider(&state, &key).await?;
    let purpose = if query.link {
        match session_user(&state, &headers).await? {
            Some(user) if user.is_guest => {
                return Err(ApiError::ForbiddenBecause(
                    "Guests can't link an account.".into(),
                ));
            }
            Some(user) => Purpose::Link { user: user.id },
            None => return Err(ApiError::Unauthorized("Log in first.".into())),
        }
    } else {
        Purpose::SignIn {
            then_link: query
                .then_confirm
                .then(|| auth::cookie(&headers, LINK_COOKIE))
                .flatten(),
        }
    };
    let page = if query.link {
        "/account"
    } else {
        "/"
    };
    match state
        .sign_in
        .start(&provider, redirect_uri(&headers, &provider), purpose)
        .await
    {
        Ok((flow_state, url)) => {
            let c = cookie(
                &headers,
                FLOW_COOKIE,
                COOKIE_PATH,
                &flow_state,
                WAIT_SECONDS,
            );
            Ok(redirect(&url, vec![c]))
        }
        Err(e) => {
            tracing::warn!(provider = key, error = %e, "can't start signing in");
            Ok(failed(page, &e, vec![]))
        }
    }
}

#[derive(Deserialize, IntoParams)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

/// Where the provider sends the browser back.
#[utoipa::path(
    get,
    path = "/auth/oidc/{key}/callback",
    tag = "sign-in",
    params(("key" = String, Path), CallbackQuery),
    responses((status = 303, description = "Back into the app"))
)]
pub async fn callback(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Query(query): Query<CallbackQuery>,
    headers: HeaderMap,
) -> Response {
    let clear_flow = cookie(&headers, FLOW_COOKIE, COOKIE_PATH, "", 0);
    let fail = |page: &str, err: SignInError| failed(page, &err, vec![clear_flow.clone()]);
    let page = if session_user(&state, &headers)
        .await
        .ok()
        .flatten()
        .is_some()
    {
        "/account"
    } else {
        "/"
    };

    let Some(flow_state) = query.state.filter(|s| !s.is_empty()) else {
        return fail(
            page,
            SignInError::Invalid("The provider sent no state.".into()),
        );
    };
    // The state in the URL must be the one this browser started with.
    if auth::cookie(&headers, FLOW_COOKIE).as_deref() != Some(flow_state.as_str()) {
        return fail(
            page,
            SignInError::Invalid(
                "This sign-in was started in another browser or tab. Try again.".into(),
            ),
        );
    }
    if state.sign_in.is_test(&flow_state) {
        let answer = match (&query.code, &query.error) {
            (_, Some(error)) => Err((error.as_str(), query.error_description.as_deref())),
            (Some(code), None) => Ok(code.as_str()),
            (None, None) => Err(("no_code", Some("The provider sent no code."))),
        };
        let token = state
            .sign_in
            .finish_test(&state.db, &flow_state, answer)
            .await;
        return test_done(&token, clear_flow);
    }
    if let Some(error) = query.error {
        let message = if error == "access_denied" || error == "user_cancelled_authorize" {
            "Signing in was cancelled.".to_string()
        } else {
            format!(
                "The provider said: {}",
                query.error_description.unwrap_or(error)
            )
        };
        return fail(page, SignInError::Invalid(message));
    }
    let Some(code) = query.code else {
        return fail(
            page,
            SignInError::Invalid("The provider sent no code.".into()),
        );
    };

    let (flow, provider, identity) = match state.sign_in.finish(&state.db, &flow_state, &code).await
    {
        Ok(done) => done,
        Err(e) => {
            tracing::warn!(provider = key, error = %e, "signing in failed");
            return fail(page, e);
        }
    };
    if provider.key != key {
        return fail(
            page,
            SignInError::Invalid("That came back to the wrong provider.".into()),
        );
    }
    let via = provider.settings.name.clone();

    match flow.purpose {
        Purpose::Test { .. } => fail(page, SignInError::Invalid("That was a test.".into())),
        Purpose::Link { user } => {
            match sign_in::link(&state.db, &provider, &identity, user, Some(user)).await {
                Ok(()) => redirect(
                    &format!("/account?linked={}", encode(&provider.key)),
                    vec![clear_flow.clone()],
                ),
                Err(e) => fail("/account", e),
            }
        }
        Purpose::SignIn { then_link } => {
            let outcome = sign_in::sign_in(&state.db, &provider, &identity).await;
            let user = match outcome {
                Ok(Outcome::SignedIn(user)) => user,
                Ok(Outcome::Confirm(user)) => {
                    let token = state.sign_in.hold_link(identity, user.id);
                    let c = cookie(&headers, LINK_COOKIE, COOKIE_PATH, &token, WAIT_SECONDS);
                    return redirect("/?sign_in=confirm", vec![clear_flow.clone(), c]);
                }
                Err(e) => {
                    if let SignInError::Refused(why) = &e {
                        let entry = NewEntry::admins(
                            activity::LOGIN_FAILED,
                            now(),
                            None,
                            json!({ "name": identity.name, "via": via, "reason": why }),
                        );
                        activity::record_or_warn(&state.db, entry).await;
                    }
                    return fail("/", e);
                }
            };
            let mut cookies = vec![clear_flow.clone()];
            // Proved it's them with another provider: link the waiting one too.
            if let Some(token) = then_link
                && let Some(waiting) = state.sign_in.pending_link(&token)
                && waiting.user == user.id
            {
                state.sign_in.take_link(&token);
                cookies.push(cookie(&headers, LINK_COOKIE, COOKIE_PATH, "", 0));
                if let Ok(Some(other)) =
                    providers::by_id(&state.db, waiting.identity.provider_id).await
                    && let Err(e) =
                        sign_in::link(&state.db, &other, &waiting.identity, user.id, Some(user.id))
                            .await
                {
                    tracing::warn!(error = %e, "can't link the confirmed account");
                }
            }
            match open_session(&state, &headers, user, &via).await {
                Ok(session) => {
                    cookies.push(session);
                    redirect("/", cookies)
                }
                Err(e) => fail("/", e),
            }
        }
    }
}

#[derive(Deserialize, ToSchema)]
pub struct PostedCallback {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    /// Apple's, on someone's first sign-in: their name and email as JSON.
    user: Option<String>,
}

/// Where a provider posts the browser back (`response_mode=form_post`, which
/// Apple uses). The sign-in cookie is SameSite=Lax, so a cross-site POST
/// arrives without it: this answers with a page that carries on to the GET
/// callback from this site, where it does. Apple's one-time name is kept with
/// the sign-in rather than put in the URL.
#[utoipa::path(
    post,
    path = "/auth/oidc/{key}/callback",
    tag = "sign-in",
    params(("key" = String, Path)),
    request_body(content = PostedCallback, content_type = "application/x-www-form-urlencoded"),
    responses((status = 200, description = "A page that carries on to the GET callback"))
)]
pub async fn posted_callback(
    State(state): State<AppState>,
    Path(key): Path<String>,
    Form(form): Form<PostedCallback>,
) -> Response {
    if let (Some(flow_state), Some(user)) = (&form.state, &form.user)
        && let Some(name) = cabinet_core::apple::name_from_user(user)
    {
        state.sign_in.note_name(flow_state, name);
    }
    let query: Vec<String> = [
        ("code", &form.code),
        ("state", &form.state),
        ("error", &form.error),
        ("error_description", &form.error_description),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.as_ref().map(|v| format!("{k}={}", encode(v))))
    .collect();
    let url = format!(
        "/api/auth/oidc/{}/callback?{}",
        encode(&key),
        query.join("&")
    );
    // `encode` leaves only URL-safe characters; `&` is the one to escape in HTML.
    let attr = url.replace('&', "&amp;");
    let page = format!(
        r#"<!doctype html>
<meta charset="utf-8">
<meta http-equiv="refresh" content="0;url={attr}">
<title>Signing in…</title>
<p><a href="{attr}">Continue signing in</a></p>
<script>location.replace('{url}')</script>
"#
    );
    (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/html; charset=utf-8")],
        page,
    )
        .into_response()
}

/// Closes a test sign-in's window once the page that opened it has the
/// report's token, or shows the report in the window if that page is gone.
fn test_done(token: &str, clear_flow: HeaderValue) -> Response {
    const PAGE: &str = r#"<!doctype html>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width">
<title>Sign-in test</title>
<style>
  body { margin: 0; min-height: 100vh; display: grid; place-items: center;
    font: 15px system-ui, sans-serif; background: #111318; color: #c9ccd3; }
</style>
<p>Test finished. You can close this window.</p>
<script>
  const token = 'TOKEN'
  const channel = new BroadcastChannel('mc-sign-in-test')
  channel.onmessage = (e) => {
    if (e.data && e.data.received === token) {
      window.close()
    }
  }
  channel.postMessage({ token })
  setTimeout(() => location.replace('/manage/users?sign_in_test=' + token), 1500)
</script>
"#;
    // The token is base64url, safe in a script string.
    let mut response = (
        StatusCode::OK,
        [(CONTENT_TYPE, "text/html; charset=utf-8")],
        PAGE.replace("TOKEN", token),
    )
        .into_response();
    response.headers_mut().append(SET_COOKIE, clear_flow);
    response
}

/// Opens a session for someone who signed in with `via`, and records it.
async fn open_session(
    state: &AppState,
    headers: &HeaderMap,
    user: User,
    via: &str,
) -> Result<HeaderValue, SignInError> {
    let (user, token) = accounts::open_session(&state.db, user.id).await?;
    let mut entry = NewEntry::admins(activity::LOGIN, now(), Some(user.id), json!({ "via": via }));
    entry.subject = Some(user.id);
    activity::record_or_warn(&state.db, entry).await;
    Ok(session_cookie(
        headers,
        &token,
        accounts::SESSION_DAYS * 86400,
    ))
}

/// "Is this you?": an account whose verified email matched a user's.
#[derive(Serialize, ToSchema)]
pub struct Confirmation {
    /// The provider they signed in with.
    provider: String,
    /// The Media Cabinet account with that email.
    account: String,
    /// Whether they can prove it with the account's password.
    has_password: bool,
    /// Or by signing in with a provider the account is already linked to.
    other_providers: Vec<ProviderButton>,
}

fn waiting(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(String, sign_in::PendingLink), ApiError> {
    auth::cookie(headers, LINK_COOKIE)
        .and_then(|token| state.sign_in.pending_link(&token).map(|link| (token, link)))
        .ok_or_else(|| ApiError::BadRequest("That took too long. Sign in again.".into()))
}

/// What the "Is this you?" screen shows.
#[utoipa::path(
    get,
    path = "/auth/oidc/confirm",
    tag = "sign-in",
    responses((status = 200, body = Confirmation), (status = 400, body = ErrorBody))
)]
pub async fn confirmation(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Confirmation>, ApiError> {
    let (_, link) = waiting(&state, &headers)?;
    let user = accounts::user(&state.db, link.user).await?;
    let provider = providers::by_id(&state.db, link.identity.provider_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let others = sign_in::logins(&state.db, user.id).await?;
    let other_providers = providers::buttons(&state.db)
        .await?
        .into_iter()
        .filter(|b| others.iter().any(|l| l.provider_key == b.key))
        .collect();
    Ok(Json(Confirmation {
        provider: provider.settings.name,
        account: user.name,
        has_password: accounts::has_password(&state.db, user.id).await?,
        other_providers,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct ConfirmBody {
    /// The matched account's password.
    password: String,
}

/// It's me: proves it with the account's password, links the two and signs in.
#[utoipa::path(
    post,
    path = "/auth/oidc/confirm",
    tag = "sign-in",
    request_body = ConfirmBody,
    responses((status = 200, body = User), (status = 400, body = ErrorBody), (status = 401, body = ErrorBody))
)]
pub async fn confirm(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ConfirmBody>,
) -> Result<Response, ApiError> {
    let (token, link) = waiting(&state, &headers)?;
    let user = accounts::user(&state.db, link.user).await?;
    if !accounts::check_password(&state.db, user.id, &body.password).await? {
        let entry = NewEntry::admins(
            activity::LOGIN_FAILED,
            now(),
            None,
            json!({ "name": user.name }),
        );
        activity::record_or_warn(&state.db, entry).await;
        return Err(ApiError::Unauthorized("That password isn't right.".into()));
    }
    state.sign_in.take_link(&token);
    let provider = providers::by_id(&state.db, link.identity.provider_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    sign_in::link(&state.db, &provider, &link.identity, user.id, Some(user.id)).await?;
    let Outcome::SignedIn(user) = sign_in::sign_in(&state.db, &provider, &link.identity).await?
    else {
        return Err(ApiError::Internal("linked, but not signed in".into()));
    };
    finish_confirming(&state, &headers, user, &provider.settings.name).await
}

/// It isn't me: makes a new account for the provider's account instead.
#[utoipa::path(
    post,
    path = "/auth/oidc/confirm/decline",
    tag = "sign-in",
    responses((status = 200, body = User), (status = 400, body = ErrorBody))
)]
pub async fn decline(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let (token, link) = waiting(&state, &headers)?;
    state.sign_in.take_link(&token);
    let provider = providers::by_id(&state.db, link.identity.provider_id)
        .await?
        .ok_or(ApiError::NotFound)?;
    let user = sign_in::create_user(&state.db, &provider, &link.identity).await?;
    finish_confirming(&state, &headers, user, &provider.settings.name).await
}

async fn finish_confirming(
    state: &AppState,
    headers: &HeaderMap,
    user: User,
    via: &str,
) -> Result<Response, ApiError> {
    let session = open_session(state, headers, user.clone(), via).await?;
    let mut response = Json(user).into_response();
    let h = response.headers_mut();
    h.append(SET_COOKIE, session);
    h.append(SET_COOKIE, cookie(headers, LINK_COOKIE, COOKIE_PATH, "", 0));
    Ok(response)
}

// Setting providers up (admins).

/// Sign-in providers and the password switch, for Manage → Users.
#[derive(Serialize, ToSchema)]
pub struct SignInSettings {
    providers: Vec<ProviderView>,
    password_sign_in: bool,
    /// `CABINET_PASSWORD_SIGN_IN` is set, so the switch is read-only.
    password_sign_in_from_env: bool,
    /// Whether people may continue as a guest.
    guest_access: bool,
    /// `CABINET_GUEST_ACCESS` is set, so the switch is read-only.
    guest_access_from_env: bool,
    /// Guests there are now.
    guests: i64,
    /// A provider's redirect URL is this plus `{key}/callback`.
    callback_base: String,
}

#[utoipa::path(
    get,
    path = "/auth-providers",
    tag = "sign-in",
    responses((status = 200, body = SignInSettings))
)]
pub async fn list_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<SignInSettings>, ApiError> {
    let mut views = vec![];
    for p in providers::list(&state.db).await? {
        views.push(providers::view(&state.db, p, state.sign_in.env()).await?);
    }
    let env = state.sign_in.password_env();
    Ok(Json(SignInSettings {
        providers: views,
        password_sign_in: providers::password_sign_in(&state.db, env).await?,
        password_sign_in_from_env: env.is_some(),
        guest_access: guests::enabled(&state.db, state.sign_in.guest_env()).await?,
        guest_access_from_env: state.sign_in.guest_env().is_some(),
        guests: guests::count(&state.db).await?,
        callback_base: callback_base(&headers),
    }))
}

async fn view(state: &AppState, p: sign_in::Provider) -> Result<Json<ProviderView>, ApiError> {
    Ok(Json(
        providers::view(&state.db, p, state.sign_in.env()).await?,
    ))
}

/// The fields that differ, for the activity log (never the values).
fn changed_fields(a: &providers::Settings, b: &providers::Settings) -> Vec<&'static str> {
    [
        ("kind", a.kind != b.kind),
        ("name", a.name != b.name),
        ("issuer", a.issuer != b.issuer),
        ("client_id", a.client_id != b.client_id),
        ("client_secret", a.client_secret != b.client_secret),
        ("new_users", a.new_users != b.new_users),
        ("groups_claim", a.groups_claim != b.groups_claim),
        ("admin_group", a.admin_group != b.admin_group),
        ("allowed_groups", a.allowed_groups != b.allowed_groups),
        ("scopes", a.scopes != b.scopes),
        ("redirect_url", a.redirect_url != b.redirect_url),
        ("enabled", a.enabled != b.enabled),
        ("authorize_url", a.authorize_url != b.authorize_url),
        ("token_url", a.token_url != b.token_url),
        ("userinfo_url", a.userinfo_url != b.userinfo_url),
        ("id_field", a.id_field != b.id_field),
        ("name_field", a.name_field != b.name_field),
        ("email_field", a.email_field != b.email_field),
        ("verified_field", a.verified_field != b.verified_field),
        ("preset", a.preset != b.preset),
        ("team_id", a.team_id != b.team_id),
        ("key_id", a.key_id != b.key_id),
    ]
    .into_iter()
    .filter(|f| f.1)
    .map(|f| f.0)
    .collect()
}

/// Adds a provider.
#[utoipa::path(
    post,
    path = "/auth-providers",
    tag = "sign-in",
    request_body = ProviderInput,
    responses((status = 201, body = ProviderView), (status = 400, body = ErrorBody))
)]
pub async fn create_provider(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Json(body): Json<ProviderInput>,
) -> Result<(StatusCode, Json<ProviderView>), ApiError> {
    let key = body
        .key
        .clone()
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| providers::key_from_name(&body.name));
    let p = providers::create(&state.db, key.trim(), body.settings(None)).await?;
    config_changed(
        &state,
        actor,
        "sign_in_provider",
        "created",
        p.id,
        Some(&p.settings.name),
        &[],
    )
    .await;
    Ok((StatusCode::CREATED, view(&state, p).await?))
}

/// Changes a provider. The client secret is kept unless a new one is given.
#[utoipa::path(
    put,
    path = "/auth-providers/{key}",
    tag = "sign-in",
    params(("key" = String, Path)),
    request_body = ProviderInput,
    responses((status = 200, body = ProviderView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn update_provider(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(key): Path<String>,
    Json(body): Json<ProviderInput>,
) -> Result<Json<ProviderView>, ApiError> {
    let before = providers::by_key(&state.db, &key)
        .await?
        .ok_or(ApiError::NotFound)?;
    let new_key = body.key.clone().filter(|k| !k.trim().is_empty());
    let settings = body.settings(Some(&before.settings.client_secret));
    let p = providers::update(&state.db, &key, new_key.as_deref(), settings).await?;
    let mut fields = changed_fields(&before.settings, &p.settings);
    if p.key != before.key {
        fields.push("key");
    }
    config_changed(
        &state,
        actor,
        "sign_in_provider",
        "changed",
        p.id,
        Some(&p.settings.name),
        &fields,
    )
    .await;
    view(&state, p).await
}

#[derive(Serialize, ToSchema)]
pub struct Removed {
    /// People who could only sign in with it, now turned off.
    disabled_users: usize,
}

/// Removes a provider and every account linked through it. People who could
/// only sign in with it are turned off until an admin gives them a password.
#[utoipa::path(
    delete,
    path = "/auth-providers/{key}",
    tag = "sign-in",
    params(("key" = String, Path)),
    responses((status = 200, body = Removed), (status = 404, body = ErrorBody))
)]
pub async fn delete_provider(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(key): Path<String>,
) -> Result<Json<Removed>, ApiError> {
    let (p, disabled_users) = providers::delete(&state.db, &key).await?;
    config_changed(
        &state,
        actor,
        "sign_in_provider",
        "removed",
        p.id,
        Some(&p.settings.name),
        &[],
    )
    .await;
    Ok(Json(Removed { disabled_users }))
}

/// Edit a provider from environment variables here instead; its variables
/// are ignored from then on.
#[utoipa::path(
    post,
    path = "/auth-providers/{key}/override",
    tag = "sign-in",
    params(("key" = String, Path)),
    responses((status = 200, body = ProviderView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn take_over(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(key): Path<String>,
) -> Result<Json<ProviderView>, ApiError> {
    let p = providers::take_over(&state.db, &key).await?;
    config_changed(
        &state,
        actor,
        "sign_in_provider",
        "overridden",
        p.id,
        Some(&p.settings.name),
        &[],
    )
    .await;
    view(&state, p).await
}

/// Go back to the environment: reapplies the provider's variables.
#[utoipa::path(
    delete,
    path = "/auth-providers/{key}/override",
    tag = "sign-in",
    params(("key" = String, Path)),
    responses((status = 200, body = ProviderView), (status = 400, body = ErrorBody), (status = 404, body = ErrorBody))
)]
pub async fn give_back(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Path(key): Path<String>,
) -> Result<Json<ProviderView>, ApiError> {
    let p = providers::give_back(&state.db, &key, state.sign_in.env()).await?;
    config_changed(
        &state,
        actor,
        "sign_in_provider",
        "from_env",
        p.id,
        Some(&p.settings.name),
        &[],
    )
    .await;
    view(&state, p).await
}

#[derive(Deserialize, ToSchema)]
pub struct IssuerTest {
    issuer: String,
}

#[derive(Serialize, ToSchema)]
pub struct IssuerFound {
    /// The issuer as the provider names itself; save this one.
    issuer: String,
}

/// Checks an issuer URL by fetching its OpenID configuration.
#[utoipa::path(
    post,
    path = "/auth-providers/test",
    tag = "sign-in",
    request_body = IssuerTest,
    responses((status = 200, body = IssuerFound), (status = 400, body = ErrorBody))
)]
pub async fn test_issuer(
    State(state): State<AppState>,
    Json(body): Json<IssuerTest>,
) -> Result<Json<IssuerFound>, ApiError> {
    let issuer = state
        .sign_in
        .test(&body.issuer)
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;
    Ok(Json(IssuerFound { issuer }))
}

/// Settings to try, saved or not.
#[derive(Deserialize, ToSchema)]
pub struct ProviderDraft {
    /// The provider being edited: its secret is used when none is given.
    existing: Option<String>,
    provider: ProviderInput,
}

/// A draft's settings, the provider it edits, and its redirect URL.
async fn draft(
    state: &AppState,
    headers: &HeaderMap,
    body: ProviderDraft,
) -> Result<(providers::Settings, Option<i64>, String), ApiError> {
    let existing = match &body.existing {
        Some(key) => Some(
            providers::by_key(&state.db, key)
                .await?
                .ok_or(ApiError::NotFound)?,
        ),
        None => None,
    };
    let key = body
        .provider
        .key
        .clone()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .or_else(|| existing.as_ref().map(|p| p.key.clone()))
        .unwrap_or_else(|| providers::key_from_name(&body.provider.name));
    let settings = body
        .provider
        .settings(existing.as_ref().map(|p| p.settings.client_secret.as_str()));
    let redirect = settings
        .redirect_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}{key}/callback", callback_base(headers)));
    Ok((settings, existing.map(|p| p.id), redirect))
}

/// Checks settings with the provider without anyone signing in: the issuer,
/// the scopes it lists, and whether it takes the client ID and secret.
#[utoipa::path(
    post,
    path = "/auth-providers/check",
    tag = "sign-in",
    request_body = ProviderDraft,
    responses((status = 200, body = CheckReport), (status = 404, body = ErrorBody))
)]
pub async fn check_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProviderDraft>,
) -> Result<Json<CheckReport>, ApiError> {
    let (settings, _, redirect) = draft(&state, &headers, body).await?;
    Ok(Json(state.sign_in.check(settings, &redirect).await))
}

#[derive(Serialize, ToSchema)]
pub struct TestStarted {
    /// Open this in a new window; it ends by handing over the report's token.
    url: String,
}

/// Starts a test sign-in with settings, saved or not. Nobody is signed in
/// and nothing is saved; the report is at `/auth-providers/tests/{token}`.
#[utoipa::path(
    post,
    path = "/auth-providers/try",
    tag = "sign-in",
    request_body = ProviderDraft,
    responses((status = 200, body = TestStarted), (status = 400, body = ErrorBody))
)]
pub async fn try_provider(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ProviderDraft>,
) -> Result<Response, ApiError> {
    let (settings, existing, redirect) = draft(&state, &headers, body).await?;
    let (flow_state, url) = state
        .sign_in
        .start_test(settings, existing, redirect)
        .await?;
    let mut response = Json(TestStarted { url }).into_response();
    response.headers_mut().append(
        SET_COOKIE,
        cookie(
            &headers,
            FLOW_COOKIE,
            COOKIE_PATH,
            &flow_state,
            WAIT_SECONDS,
        ),
    );
    Ok(response)
}

/// What came back from a test sign-in, for ten minutes after.
#[utoipa::path(
    get,
    path = "/auth-providers/tests/{token}",
    tag = "sign-in",
    params(("token" = String, Path)),
    responses((status = 200, body = TestReport), (status = 404, body = ErrorBody))
)]
pub async fn test_report(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<Json<TestReport>, ApiError> {
    state
        .sign_in
        .test_report(&token)
        .map(Json)
        .ok_or(ApiError::NotFound)
}

#[derive(Deserialize, ToSchema)]
pub struct AuthSettingsInput {
    /// Show the password form on the login page up front.
    password_sign_in: Option<bool>,
    /// Let people continue as a guest. Turning it off removes every guest.
    guest_access: Option<bool>,
}

#[utoipa::path(
    put,
    path = "/auth-settings",
    tag = "sign-in",
    request_body = AuthSettingsInput,
    responses((status = 204), (status = 400, body = ErrorBody))
)]
pub async fn set_auth_settings(
    State(state): State<AppState>,
    ActingUser(actor): ActingUser,
    Json(body): Json<AuthSettingsInput>,
) -> Result<StatusCode, ApiError> {
    if body.password_sign_in.is_some() && state.sign_in.password_env().is_some() {
        return Err(ApiError::BadRequest(
            "CABINET_PASSWORD_SIGN_IN decides this.".into(),
        ));
    }
    if body.guest_access.is_some() && state.sign_in.guest_env().is_some() {
        return Err(ApiError::BadRequest(
            "CABINET_GUEST_ACCESS decides this.".into(),
        ));
    }
    let mut changed = vec![];
    if let Some(on) = body.password_sign_in {
        providers::set_password_sign_in(&state.db, on).await?;
        changed.push("password_sign_in");
    }
    if let Some(on) = body.guest_access {
        guests::set_enabled(&state.db, on).await?;
        changed.push("guest_access");
    }
    if !changed.is_empty() {
        config_changed(&state, actor, "sign_in", "changed", 1, None, &changed).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

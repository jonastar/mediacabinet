//! Signing in with another account, against a fake provider on loopback.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::{Form, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use cabinet_core::accounts::{self, UserStatus};
use cabinet_core::sign_in::providers::{self, Kind, Settings};
use cabinet_core::sign_in::{self, Identity, Level, Outcome, Purpose, SignIn, SignInError};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use sqlx::types::Json as SqlJson;
use tempfile::TempDir;

/// A code the fake provider handed out, and what it was issued for.
struct Issued {
    nonce: String,
    challenge: String,
    claims: Value,
}

#[derive(Default)]
struct Fake {
    issuer: String,
    codes: HashMap<String, Issued>,
    /// Claims only userinfo has.
    userinfo: Value,
    /// Overrides for the ID token's own claims, to test the checks.
    tamper: Value,
    /// What a plain OAuth2 user endpoint says.
    me: Value,
}

type Shared = Arc<Mutex<Fake>>;

async fn discovery(State(fake): State<Shared>) -> Json<Value> {
    let issuer = fake.lock().unwrap().issuer.clone();
    Json(json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "userinfo_endpoint": format!("{issuer}/userinfo"),
        "scopes_supported": ["openid", "profile", "email", "groups"],
    }))
}

fn jwt(claims: &Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    format!(
        "{header}.{}.not-checked",
        URL_SAFE_NO_PAD.encode(claims.to_string())
    )
}

async fn token(
    State(fake): State<Shared>,
    headers: HeaderMap,
    Form(form): Form<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let refuse = |why: &str| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "invalid_grant", "error_description": why })),
        )
    };
    let expected = format!("Basic {}", STANDARD.encode("cabinet:s3cret"));
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some(&expected) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid_client" })),
        );
    }
    let mut fake = fake.lock().unwrap();
    let Some(issued) = fake.codes.remove(&form["code"]) else {
        return refuse("unknown code");
    };
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(form["code_verifier"].as_bytes()));
    if challenge != issued.challenge {
        return refuse("PKCE verifier doesn't match");
    }
    let now = cabinet_core::catalog::now();
    let mut claims = json!({
        "iss": fake.issuer,
        "aud": "cabinet",
        "exp": now + 300,
        "iat": now,
        "nonce": issued.nonce,
    });
    let map = claims.as_object_mut().unwrap();
    for (k, v) in issued.claims.as_object().unwrap() {
        map.insert(k.clone(), v.clone());
    }
    if let Some(tamper) = fake.tamper.as_object() {
        for (k, v) in tamper {
            map.insert(k.clone(), v.clone());
        }
    }
    let sub = map["sub"].clone();
    fake.userinfo["sub"] = sub;
    (
        StatusCode::OK,
        Json(json!({ "id_token": jwt(&claims), "access_token": "at", "token_type": "Bearer" })),
    )
}

async fn userinfo(State(fake): State<Shared>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer at") {
        return (StatusCode::UNAUTHORIZED, Json(json!({})));
    }
    (StatusCode::OK, Json(fake.lock().unwrap().userinfo.clone()))
}

async fn me(State(fake): State<Shared>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer at") {
        return (StatusCode::UNAUTHORIZED, Json(json!({})));
    }
    (StatusCode::OK, Json(fake.lock().unwrap().me.clone()))
}

struct World {
    _dir: TempDir,
    pool: SqlitePool,
    fake: Shared,
    sign_in: SignIn,
    issuer: String,
}

async fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let pool = cabinet_core::db::open(&dir.path().join("cabinet.db"))
        .await
        .unwrap();
    accounts::setup(&pool, "admin", "long enough")
        .await
        .unwrap();
    let fake: Shared = Arc::default();
    let app = Router::new()
        .route("/.well-known/openid-configuration", get(discovery))
        .route("/token", post(token))
        .route("/userinfo", get(userinfo))
        .route("/api/me", get(me))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    fake.lock().unwrap().issuer = issuer.clone();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    World {
        _dir: dir,
        pool,
        fake,
        sign_in: SignIn::new(vec![], None),
        issuer,
    }
}

impl World {
    async fn provider(&self, change: impl FnOnce(&mut Settings)) -> sign_in::Provider {
        let mut s = Settings::new("Fake", &self.issuer, "cabinet", "s3cret");
        change(&mut s);
        providers::create(&self.pool, "fake", s).await.unwrap()
    }

    /// Starts a sign-in, has the fake provider approve it as someone with
    /// `claims`, and finishes it.
    async fn round_trip(
        &self,
        provider: &sign_in::Provider,
        purpose: Purpose,
        claims: Value,
    ) -> Result<(sign_in::Flow, sign_in::Provider, Identity), SignInError> {
        let (state, url) = self
            .sign_in
            .start(
                provider,
                "http://localhost/api/auth/oidc/fake/callback".into(),
                purpose,
            )
            .await?;
        let code = self.approve(&state, &url, claims);
        self.sign_in.finish(&self.pool, &state, &code).await
    }

    /// Has the fake provider approve the sign-in sent to `url` as someone
    /// with `claims`. Returns the code.
    fn approve(&self, state: &str, url: &str, claims: Value) -> String {
        let url = reqwest::Url::parse(url).unwrap();
        let q: HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["state"], state);
        assert_eq!(q["code_challenge_method"], "S256");
        let code = format!("code-{state}");
        self.fake.lock().unwrap().codes.insert(
            code.clone(),
            Issued {
                nonce: q.get("nonce").cloned().unwrap_or_default(),
                challenge: q["code_challenge"].clone(),
                claims,
            },
        );
        code
    }

    async fn sign_in(&self, provider: &sign_in::Provider, claims: Value) -> Outcome {
        let (_, provider, identity) = self
            .round_trip(provider, Purpose::SignIn { then_link: None }, claims)
            .await
            .unwrap();
        sign_in::sign_in(&self.pool, &provider, &identity)
            .await
            .unwrap()
    }
}

fn signed_in(outcome: Outcome) -> accounts::User {
    match outcome {
        Outcome::SignedIn(user) => user,
        other => panic!("not signed in: {other:?}"),
    }
}

#[tokio::test]
async fn new_people_wait_for_approval() {
    let w = world().await;
    let p = w.provider(|_| {}).await;
    let sam = json!({ "sub": "u1", "preferred_username": "sam", "email": "sam@example.com", "email_verified": true });
    let user = signed_in(w.sign_in(&p, sam.clone()).await);
    assert_eq!(user.name, "sam");
    assert_eq!(user.status, UserStatus::Pending);
    assert!(!user.is_admin);
    assert_eq!(user.email.as_deref(), Some("sam@example.com"));
    assert!(!accounts::has_password(&w.pool, user.id).await.unwrap());

    // Coming back is the same user, still waiting.
    let again = signed_in(w.sign_in(&p, sam.clone()).await);
    assert_eq!(again.id, user.id);
    assert_eq!(again.status, UserStatus::Pending);

    // Declined: can't sign in any more.
    accounts::set_status(&w.pool, user.id, UserStatus::Disabled)
        .await
        .unwrap();
    let (_, p2, identity) = w
        .round_trip(&p, Purpose::SignIn { then_link: None }, sam)
        .await
        .unwrap();
    assert!(matches!(
        sign_in::sign_in(&w.pool, &p2, &identity).await,
        Err(SignInError::Refused(_))
    ));
}

#[tokio::test]
async fn names_are_numbered_and_providers_can_let_people_in() {
    let w = world().await;
    let p = w.provider(|s| s.new_users = UserStatus::Active).await;
    let user = signed_in(
        w.sign_in(&p, json!({ "sub": "u2", "preferred_username": "Admin" }))
            .await,
    );
    assert_eq!(user.name, "Admin-2");
    assert_eq!(user.status, UserStatus::Active);
    // No email at all: the name falls back.
    let user = signed_in(w.sign_in(&p, json!({ "sub": "u3" })).await);
    assert_eq!(user.name, "user");
}

#[tokio::test]
async fn a_matching_email_needs_proof() {
    let w = world().await;
    let jo = accounts::create(&w.pool, "jo", "jo's password", false)
        .await
        .unwrap();
    accounts::set_email(&w.pool, jo.id, "Jo@Example.com")
        .await
        .unwrap();
    let p = w.provider(|_| {}).await;

    // Unverified: no match, a new user.
    let other = signed_in(
        w.sign_in(
            &p,
            json!({ "sub": "x", "email": "jo@example.com", "email_verified": false, "name": "Jo" }),
        )
        .await,
    );
    assert_ne!(other.id, jo.id);
    assert_eq!(other.email, None);

    let claims = json!({ "sub": "jo-sub", "email": "jo@example.com", "email_verified": true });
    let outcome = w.sign_in(&p, claims.clone()).await;
    let Outcome::Confirm(matched) = outcome else {
        panic!("expected a confirmation: {outcome:?}");
    };
    assert_eq!(matched.id, jo.id);

    // Proven (the server checks the password): linked, and known from then on.
    let (_, p2, identity) = w
        .round_trip(&p, Purpose::SignIn { then_link: None }, claims.clone())
        .await
        .unwrap();
    sign_in::link(&w.pool, &p2, &identity, jo.id, Some(jo.id))
        .await
        .unwrap();
    assert_eq!(signed_in(w.sign_in(&p, claims).await).id, jo.id);
    let logins = sign_in::logins(&w.pool, jo.id).await.unwrap();
    assert_eq!(logins.len(), 1);
    assert_eq!(logins[0].provider_key, "fake");

    // The same provider account can't be linked to someone else.
    assert!(matches!(
        sign_in::link(&w.pool, &p2, &identity, other.id, None).await,
        Err(SignInError::Invalid(_))
    ));
    // Jo has a password, so the link can go.
    sign_in::unlink(&w.pool, jo.id, logins[0].id, Some(jo.id))
        .await
        .unwrap();
}

#[tokio::test]
async fn the_only_way_in_stays() {
    let w = world().await;
    let p = w.provider(|s| s.new_users = UserStatus::Active).await;
    let user = signed_in(w.sign_in(&p, json!({ "sub": "only" })).await);
    let login = sign_in::logins(&w.pool, user.id).await.unwrap()[0].id;
    assert!(matches!(
        sign_in::unlink(&w.pool, user.id, login, None).await,
        Err(SignInError::Invalid(_))
    ));

    // Removing the provider turns them off until an admin gives them a password.
    let (_, stranded) = providers::delete(&w.pool, "fake").await.unwrap();
    assert_eq!(stranded, 1);
    let user = accounts::user(&w.pool, user.id).await.unwrap();
    assert_eq!(user.status, UserStatus::Disabled);
}

#[tokio::test]
async fn groups_decide_access_and_roles() {
    let w = world().await;
    let p = w
        .provider(|s| {
            s.admin_group = Some("media-admins".into());
            s.allowed_groups = SqlJson(vec!["family".into()]);
        })
        .await;

    // Not in an allowed group: refused, and no user is made.
    let (_, p2, stranger) = w
        .round_trip(
            &p,
            Purpose::SignIn { then_link: None },
            json!({ "sub": "s", "groups": ["neighbours"] }),
        )
        .await
        .unwrap();
    assert!(matches!(
        sign_in::sign_in(&w.pool, &p2, &stranger).await,
        Err(SignInError::Refused(_))
    ));
    assert_eq!(accounts::list(&w.pool).await.unwrap().len(), 1);

    // In an allowed group: let straight in. Groups from userinfo count too.
    w.fake.lock().unwrap().userinfo = json!({ "groups": ["family", "media-admins"] });
    let kim = signed_in(w.sign_in(&p, json!({ "sub": "kim" })).await);
    assert_eq!(kim.status, UserStatus::Active);
    assert!(kim.is_admin);
    assert_eq!(
        sign_in::role_source(&w.pool, kim.id)
            .await
            .unwrap()
            .as_deref(),
        Some("Fake")
    );

    // Leaving the admin group: a viewer from the next sign-in.
    w.fake.lock().unwrap().userinfo = json!({ "groups": ["family"] });
    let kim = signed_in(w.sign_in(&p, json!({ "sub": "kim" })).await);
    assert!(!kim.is_admin);
}

#[tokio::test]
async fn the_last_admin_stays_an_admin() {
    let w = world().await;
    let p = w.provider(|s| s.admin_group = Some("admins".into())).await;
    let admin = accounts::list(&w.pool).await.unwrap()[0].clone();
    let (_, p2, identity) = w
        .round_trip(
            &p,
            Purpose::SignIn { then_link: None },
            json!({ "sub": "boss", "groups": [] }),
        )
        .await
        .unwrap();
    sign_in::link(&w.pool, &p2, &identity, admin.id, None)
        .await
        .unwrap();
    let after = signed_in(w.sign_in(&p, json!({ "sub": "boss", "groups": [] })).await);
    assert!(after.is_admin, "still the only admin");
}

#[tokio::test]
async fn bad_responses_are_refused() {
    let w = world().await;
    let p = w.provider(|_| {}).await;
    for tamper in [
        json!({ "nonce": "someone else's" }),
        json!({ "aud": "another-client" }),
        json!({ "iss": "https://evil.example.com" }),
        json!({ "exp": 1 }),
    ] {
        w.fake.lock().unwrap().tamper = tamper.clone();
        let result = w
            .round_trip(
                &p,
                Purpose::SignIn { then_link: None },
                json!({ "sub": "t" }),
            )
            .await;
        assert!(
            matches!(result, Err(SignInError::Oidc(_))),
            "accepted {tamper}"
        );
    }
    w.fake.lock().unwrap().tamper = Value::Null;

    // A started sign-in finishes once.
    let (state, _) = w
        .sign_in
        .start(
            &p,
            "http://localhost/cb".into(),
            Purpose::SignIn { then_link: None },
        )
        .await
        .unwrap();
    let first = w.sign_in.finish(&w.pool, &state, "made-up").await;
    assert!(matches!(first, Err(SignInError::Oidc(_))), "{first:?}");
    let second = w.sign_in.finish(&w.pool, &state, "made-up").await;
    assert!(matches!(second, Err(SignInError::Invalid(_))), "{second:?}");
}

#[tokio::test]
async fn providers_from_the_environment() {
    let w = world().await;
    let vars: Vec<(String, String)> = [
        ("CABINET_OIDC_HOME_ISSUER", w.issuer.as_str()),
        ("CABINET_OIDC_HOME_CLIENT_ID", "cabinet"),
        ("CABINET_OIDC_HOME_CLIENT_SECRET", "s3cret"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let (env, problems) = sign_in::env::parse(&vars);
    assert!(problems.is_empty());
    assert!(providers::sync_env(&w.pool, &env).await.unwrap().is_empty());
    let home = providers::by_key(&w.pool, "home").await.unwrap().unwrap();
    assert!(home.from_env());
    assert_eq!(home.settings.name, "Home");

    // Managed by the environment: the UI can't change it until it takes over.
    let mut renamed = home.settings.clone();
    renamed.name = "Family".into();
    assert!(
        providers::update(&w.pool, "home", None, renamed.clone())
            .await
            .is_err()
    );
    providers::take_over(&w.pool, "home").await.unwrap();
    providers::update(&w.pool, "home", None, renamed)
        .await
        .unwrap();
    // Taken over: a restart leaves it alone, and says so.
    let warnings = providers::sync_env(&w.pool, &env).await.unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let home = providers::by_key(&w.pool, "home").await.unwrap().unwrap();
    assert_eq!(home.settings.name, "Family");
    // Going back reapplies the variables.
    let home = providers::give_back(&w.pool, "home", &env).await.unwrap();
    assert_eq!(home.settings.name, "Home");
    assert!(home.from_env());

    // Variables gone: turned off, not deleted.
    let warnings = providers::sync_env(&w.pool, &[]).await.unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    let home = providers::by_key(&w.pool, "home").await.unwrap().unwrap();
    assert!(!home.settings.enabled);
}

#[tokio::test]
async fn settings_can_be_checked_and_tried() {
    let w = world().await;
    let redirect = "http://localhost/api/auth/oidc/fake/callback";
    let mut s = Settings::new("Fake", &w.issuer, "cabinet", "s3cret");
    s.scopes = "openid email offline_access".into();
    let has = |notes: &[sign_in::Note], level: Level, text: &str| {
        notes
            .iter()
            .any(|n| n.level == level && n.text.contains(text))
    };

    let report = w.sign_in.check(s.clone(), redirect).await;
    assert_eq!(report.issuer.as_deref(), Some(w.issuer.as_str()));
    assert!(
        has(&report.notes, Level::Warning, "`offline_access`"),
        "{report:?}"
    );
    assert!(has(
        &report.notes,
        Level::Good,
        "client ID and secret are right"
    ));
    s.client_secret = "wrong".into();
    let report = w.sign_in.check(s.clone(), redirect).await;
    assert!(
        has(&report.notes, Level::Problem, "rejected the client"),
        "{report:?}"
    );

    // The provider refuses a scope.
    let (state, _) = w
        .sign_in
        .start_test(s.clone(), None, redirect.into())
        .await
        .unwrap();
    assert!(w.sign_in.is_test(&state));
    let token = w
        .sign_in
        .finish_test(
            &w.pool,
            &state,
            Err(("invalid_scope", Some("offline_access"))),
        )
        .await;
    let report = w.sign_in.test_report(&token).unwrap();
    assert!(
        has(&report.notes, Level::Problem, "refused the scopes"),
        "{report:?}"
    );

    // A real round trip with unsaved settings: reported, and nothing changes.
    let p = w.provider(|_| {}).await;
    s.client_secret = "s3cret".into();
    s.scopes = "openid email".into();
    s.admin_group = Some("admins".into());
    let (state, url) = w
        .sign_in
        .start_test(s.clone(), Some(p.id), redirect.into())
        .await
        .unwrap();
    let kim = json!({ "sub": "k1", "preferred_username": "kim", "email": "kim@example.com", "groups": ["admins"] });
    let code = w.approve(&state, &url, kim);
    let token = w.sign_in.finish_test(&w.pool, &state, Ok(&code)).await;
    let report = w.sign_in.test_report(&token).unwrap();
    assert_eq!(report.scopes, "openid email groups");
    assert_eq!(report.groups, Some(vec!["admins".to_string()]));
    assert_eq!(report.email.as_deref(), Some("kim@example.com"));
    assert!(!report.email_verified);
    assert!(has(&report.notes, Level::Warning, "isn't marked verified"));
    assert_eq!(
        report.outcome.as_deref(),
        Some("A new user, kim, would be made as an admin, who can watch right away.")
    );
    let logins: i64 = sqlx::query_scalar("SELECT count(*) FROM user_login")
        .fetch_one(&w.pool)
        .await
        .unwrap();
    assert_eq!(logins, 0);
    assert_eq!(accounts::list(&w.pool).await.unwrap().len(), 1);

    // A test isn't a sign-in.
    let (state, url) = w
        .sign_in
        .start_test(s, Some(p.id), redirect.into())
        .await
        .unwrap();
    let code = w.approve(&state, &url, json!({ "sub": "k1" }));
    assert!(w.sign_in.finish(&w.pool, &state, &code).await.is_err());
}

#[tokio::test]
async fn plain_oauth2_providers() {
    let w = world().await;
    let mut s = Settings::oauth2(Kind::OAuth2, "Forum", "cabinet", "s3cret");
    s.authorize_url = Some(format!("{}/authorize", w.issuer));
    s.token_url = Some(format!("{}/token", w.issuer));
    s.userinfo_url = Some(format!("{}/api/me", w.issuer));
    s.id_field = Some("user.id".into());
    s.name_field = Some("user.login".into());
    s.email_field = Some("user.mail".into());
    s.groups_claim = "user.roles".into();
    s.allowed_groups = SqlJson(vec!["members".into()]);
    let p = providers::create(&w.pool, "forum", s).await.unwrap();
    assert_eq!(p.settings.scopes, "");

    let (_, url) = w
        .sign_in
        .start(
            &p,
            "http://localhost/api/auth/oidc/forum/callback".into(),
            Purpose::SignIn { then_link: None },
        )
        .await
        .unwrap();
    assert!(url.starts_with(&format!("{}/authorize?", w.issuer)));
    assert!(!url.contains("nonce="), "{url}");

    w.fake.lock().unwrap().me = json!({
        "user": { "id": 7, "login": "ana", "mail": "ana@example.com", "roles": ["members"] }
    });
    // The fake's ID token claims are ignored: the user endpoint decides.
    let (_, _, identity) = w
        .round_trip(
            &p,
            Purpose::SignIn { then_link: None },
            json!({ "sub": "not-this" }),
        )
        .await
        .unwrap();
    assert_eq!(identity.subject, "7");
    assert_eq!(identity.name, "ana");
    assert_eq!(identity.groups, ["members"]);
    // No verified field: the email isn't trusted.
    assert_eq!(identity.email, None);
    let user = signed_in(sign_in::sign_in(&w.pool, &p, &identity).await.unwrap());
    assert_eq!(user.status, UserStatus::Active);

    // Not in an allowed group.
    w.fake.lock().unwrap().me = json!({ "user": { "id": 8, "login": "bo", "roles": [] } });
    let (_, _, identity) = w
        .round_trip(
            &p,
            Purpose::SignIn { then_link: None },
            json!({ "sub": "x" }),
        )
        .await
        .unwrap();
    assert!(matches!(
        sign_in::sign_in(&w.pool, &p, &identity).await,
        Err(SignInError::Refused(_))
    ));

    // Presets don't take groups.
    let mut github = Settings::oauth2(Kind::Github, "GitHub", "c", "s");
    github.admin_group = Some("admins".into());
    assert!(providers::create(&w.pool, "github", github).await.is_err());
}

#[tokio::test]
async fn presets_and_apple() {
    let w = world().await;

    // A preset fills in what it fixes, and keeps groups away where there are none.
    let mut google = Settings::new("Google", "", "c", "s");
    google.preset = Some("google".into());
    google.scopes = String::new();
    let checked = google.clone().checked().unwrap();
    assert_eq!(checked.issuer, "https://accounts.google.com");
    assert_eq!(checked.scopes, "openid profile email");
    google.admin_group = Some("admins".into());
    assert!(google.checked().is_err());
    let mut wrong = Settings::new("X", &w.issuer, "c", "s");
    wrong.preset = Some("github".into());
    assert!(wrong.checked().is_err(), "GitHub isn't OpenID Connect here");

    // Apple: the private key is the secret, and must sign.
    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = ring::signature::EcdsaKeyPair::generate_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        &rng,
    )
    .unwrap();
    let pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(pkcs8.as_ref())
    );
    let mut apple = Settings::new("Apple", "", "com.example.media", &pem);
    apple.preset = Some("apple".into());
    apple.scopes = String::new();
    assert!(
        apple.clone().checked().is_err(),
        "needs the Team ID and Key ID"
    );
    apple.team_id = Some("TEAM123456".into());
    apple.key_id = Some("KEY1234567".into());
    let checked = apple.clone().checked().unwrap();
    assert_eq!(checked.issuer, "https://appleid.apple.com");
    assert_eq!(checked.scopes, "openid email name");
    assert_ne!(checked.secret_now(0).unwrap(), pem);
    apple.client_secret = "not a key".into();
    assert!(apple.checked().is_err());

    // The name Apple posts on a first sign-in names the new user.
    let p = w.provider(|_| {}).await;
    let (state, url) = w
        .sign_in
        .start(
            &p,
            "http://localhost/api/auth/oidc/fake/callback".into(),
            Purpose::SignIn { then_link: None },
        )
        .await
        .unwrap();
    w.sign_in.note_name(&state, "Ana Lima".into());
    let code = w.approve(&state, &url, json!({ "sub": "apple-1" }));
    let (_, _, identity) = w.sign_in.finish(&w.pool, &state, &code).await.unwrap();
    assert_eq!(identity.name, "Ana Lima");
}

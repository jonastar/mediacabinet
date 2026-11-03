//! Signing in with another account. Design: `docs/sign-in.md`.
//!
//! [`SignIn`] runs the round trip to the provider ([`crate::oidc`]) and keeps
//! what's half done in memory: sign-ins started (by `state`) and accounts
//! waiting to be confirmed as someone's ("Is this you?"), ten minutes each.
//! Losing them on a restart only means signing in again.
//!
//! An account at a provider is known by provider and `sub`, never by email or
//! name. Coming back from a provider, it's either linked to a user (signed in),
//! its verified email matches a user (who has to prove it's them before the
//! two are linked), or it gets a new user.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::json;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use crate::accounts::{self, AccountError, User, UserStatus};
use crate::activity::{self, NewEntry};
use crate::catalog::now;
use crate::oauth2;
use crate::oidc::{self, Discovery, OidcError};

mod check;
pub mod env;
pub mod presets;
pub mod providers;

pub use check::{CheckReport, Level, Note, TestReport};
pub use env::EnvProvider;
pub use providers::{Kind, Provider, Settings};

#[derive(Debug, thiserror::Error)]
pub enum SignInError {
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Invalid(String),
    /// Not let in: not in an allowed group, turned off, provider disabled.
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Oidc(#[from] OidcError),
    #[error(transparent)]
    Account(#[from] AccountError),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

/// How long a started sign-in or an unconfirmed link waits.
const WAIT: Duration = Duration::from_secs(10 * 60);
/// At most this many of each wait; the oldest go first.
const MAX_WAITING: usize = 1000;
/// How long a provider's discovery document is reused.
const DISCOVERY_TTL: Duration = Duration::from_secs(60 * 60);

/// What a sign-in is for.
#[derive(Debug, Clone)]
pub enum Purpose {
    /// Signing in. `then_link` is an unconfirmed link to complete if this
    /// turns out to be the user it's waiting for.
    SignIn { then_link: Option<String> },
    /// Linking another account to a signed-in user.
    Link { user: i64 },
    /// An admin trying settings, saved or not: nobody signs in, and what
    /// came back is reported. `existing` is the provider being edited.
    Test {
        settings: Box<Settings>,
        existing: Option<i64>,
    },
}

/// A sign-in sent to the provider, waiting for it to come back.
#[derive(Debug, Clone)]
pub struct Flow {
    pub provider_id: i64,
    pub purpose: Purpose,
    nonce: String,
    verifier: String,
    redirect_uri: String,
    /// As asked for, `groups` included.
    scopes: String,
    /// The name Apple sends only on someone's first sign-in.
    name: Option<String>,
    started: Instant,
}

/// Someone back from a provider, checked.
#[derive(Debug, Clone)]
pub struct Identity {
    pub provider_id: i64,
    pub subject: String,
    /// Only if the provider says it's verified.
    pub email: Option<String>,
    /// A name to start from for a new user.
    pub name: String,
    pub groups: Vec<String>,
}

/// An account whose email matched a user's, waiting for them to prove it's
/// theirs.
#[derive(Debug, Clone)]
pub struct PendingLink {
    pub identity: Identity,
    pub user: i64,
    started: Instant,
}

struct Waiting<T> {
    entries: HashMap<String, (Instant, T)>,
}

impl<T: Clone> Waiting<T> {
    fn new() -> Self {
        Waiting {
            entries: HashMap::new(),
        }
    }

    fn put(&mut self, key: String, value: T) {
        self.entries.retain(|_, (at, _)| at.elapsed() < WAIT);
        if self.entries.len() >= MAX_WAITING
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (at, _))| *at)
                .map(|(k, _)| k.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (Instant::now(), value));
    }

    fn get(&self, key: &str) -> Option<T> {
        self.entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < WAIT)
            .map(|(_, v)| v.clone())
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut T> {
        self.entries
            .get_mut(key)
            .filter(|(at, _)| at.elapsed() < WAIT)
            .map(|(_, v)| v)
    }

    fn take(&mut self, key: &str) -> Option<T> {
        self.entries
            .remove(key)
            .filter(|(at, _)| at.elapsed() < WAIT)
            .map(|(_, v)| v)
    }
}

struct Inner {
    http: reqwest::Client,
    env: Vec<EnvProvider>,
    password_env: Option<bool>,
    discovery: Mutex<HashMap<String, (Instant, Arc<Discovery>)>>,
    flows: Mutex<Waiting<Flow>>,
    links: Mutex<Waiting<PendingLink>>,
    tests: Mutex<Waiting<TestReport>>,
}

/// Sign-ins in progress, and what the environment says about providers.
#[derive(Clone)]
pub struct SignIn(Arc<Inner>);

impl SignIn {
    pub fn new(env: Vec<EnvProvider>, password_env: Option<bool>) -> SignIn {
        SignIn(Arc::new(Inner {
            http: oidc::http_client(),
            env,
            password_env,
            discovery: Mutex::new(HashMap::new()),
            flows: Mutex::new(Waiting::new()),
            links: Mutex::new(Waiting::new()),
            tests: Mutex::new(Waiting::new()),
        }))
    }

    /// Providers from `CABINET_OIDC_*`, as read at startup.
    pub fn env(&self) -> &[EnvProvider] {
        &self.0.env
    }

    /// `CABINET_PASSWORD_SIGN_IN`, if set.
    pub fn password_env(&self) -> Option<bool> {
        self.0.password_env
    }

    async fn discovery(&self, issuer: &str) -> Result<Arc<Discovery>, OidcError> {
        if let Some((at, doc)) = self.0.discovery.lock().unwrap().get(issuer)
            && at.elapsed() < DISCOVERY_TTL
        {
            return Ok(doc.clone());
        }
        let doc = Arc::new(oidc::discover(&self.0.http, issuer).await?);
        self.0
            .discovery
            .lock()
            .unwrap()
            .insert(issuer.to_string(), (Instant::now(), doc.clone()));
        Ok(doc)
    }

    /// Where a provider's endpoints are: its discovery document for OIDC,
    /// built in or set by hand for the others.
    async fn endpoints(&self, s: &Settings) -> Result<Arc<Discovery>, OidcError> {
        match oauth2::discovery(s) {
            Some(doc) => Ok(Arc::new(doc)),
            None => self.discovery(&s.issuer).await,
        }
    }

    fn forget(&self, issuer: &str) {
        self.0.discovery.lock().unwrap().remove(issuer);
    }

    /// Checks an issuer URL by fetching its discovery document. Returns the
    /// issuer as the provider names itself.
    pub async fn test(&self, issuer: &str) -> Result<String, OidcError> {
        let doc = oidc::discover(&self.0.http, issuer.trim()).await?;
        self.forget(&doc.issuer);
        Ok(doc.issuer)
    }

    /// Starts a sign-in. Returns the `state` (also the cookie that ties the
    /// sign-in to this browser) and where to send the browser.
    pub async fn start(
        &self,
        provider: &Provider,
        redirect_uri: String,
        purpose: Purpose,
    ) -> Result<(String, String), SignInError> {
        let s = &provider.settings;
        if !s.enabled {
            return Err(SignInError::Refused(format!(
                "Signing in with {} is turned off.",
                s.name
            )));
        }
        self.start_flow(s, provider.id, redirect_uri, purpose).await
    }

    async fn start_flow(
        &self,
        s: &Settings,
        provider_id: i64,
        redirect_uri: String,
        purpose: Purpose,
    ) -> Result<(String, String), SignInError> {
        let doc = self.endpoints(s).await?;
        let state = oidc::random_token();
        let flow = Flow {
            provider_id,
            purpose,
            nonce: oidc::random_token(),
            verifier: oidc::random_token(),
            redirect_uri,
            scopes: check::scopes(s, &doc),
            name: None,
            started: Instant::now(),
        };
        let url = oidc::authorize_url(
            &doc,
            &oidc::AuthRequest {
                client_id: &s.client_id,
                redirect_uri: &flow.redirect_uri,
                scopes: &flow.scopes,
                state: &state,
                nonce: (s.kind == Kind::Oidc).then_some(flow.nonce.as_str()),
                verifier: &flow.verifier,
                form_post: s.is_apple(),
            },
        )?;
        self.0.flows.lock().unwrap().put(state.clone(), flow);
        Ok((state, url))
    }

    /// The provider sent the browser back with a code: exchanges it and
    /// checks who it is. Each started sign-in can finish once.
    pub async fn finish(
        &self,
        pool: &SqlitePool,
        state: &str,
        code: &str,
    ) -> Result<(Flow, Provider, Identity), SignInError> {
        let Some(flow) = self.0.flows.lock().unwrap().take(state) else {
            return Err(SignInError::Invalid(
                "This sign-in expired or was already used. Try again.".into(),
            ));
        };
        debug_assert!(flow.started.elapsed() < WAIT);
        if matches!(flow.purpose, Purpose::Test { .. }) {
            return Err(SignInError::Invalid("That was a test.".into()));
        }
        let provider = providers::by_id(pool, flow.provider_id)
            .await?
            .ok_or_else(|| SignInError::Invalid("That provider was removed.".into()))?;
        let s = &provider.settings;
        if !s.enabled {
            return Err(SignInError::Refused(format!(
                "Signing in with {} is turned off.",
                s.name
            )));
        }
        let (claims, _) = self.claims(s, &flow, code).await?;
        let identity = identity(provider.id, s, claims);
        Ok((flow, provider, identity))
    }

    /// Trades the code for the account's claims, from userinfo too when the
    /// ID token lacks the email or groups. Also returns the granted scopes.
    async fn claims(
        &self,
        s: &Settings,
        flow: &Flow,
        code: &str,
    ) -> Result<(oidc::Claims, Option<String>), OidcError> {
        let result = async {
            let doc = self.endpoints(s).await?;
            let tokens = oidc::exchange(
                &self.0.http,
                &doc,
                &s.client_id,
                &s.secret_now(now())?,
                code,
                &flow.redirect_uri,
                &flow.verifier,
            )
            .await?;
            if s.kind != Kind::Oidc {
                let access = tokens.access_token.as_deref().ok_or_else(|| {
                    OidcError::Invalid("The provider sent no access token.".into())
                })?;
                let claims = oauth2::claims(&self.0.http, s, &doc, access).await?;
                return Ok((claims, tokens.scope));
            }
            let id_token = tokens.id_token.as_deref().ok_or_else(|| {
                OidcError::Invalid(
                    "The provider sent no ID token; is `openid` in the scopes?".into(),
                )
            })?;
            let mut claims = oidc::check_id_token(
                id_token,
                &oidc::Expected {
                    issuer: &doc.issuer,
                    client_id: &s.client_id,
                    nonce: &flow.nonce,
                    now: now(),
                },
            )?;
            let missing = claims.verified_email().is_none()
                || (check::wants_groups(s) && claims.groups(&s.groups_claim).is_none());
            if missing && let Some(access) = &tokens.access_token {
                let more = oidc::userinfo(&self.0.http, &doc, access, &claims.sub).await?;
                claims.merge(more);
            }
            if let Some(name) = &flow.name {
                let mut more = serde_json::Map::new();
                more.insert("name".into(), name.clone().into());
                claims.merge(more);
            }
            Ok((claims, tokens.scope))
        }
        .await;
        if result.is_err() {
            // The provider may have changed; look again next time.
            self.forget(&s.issuer);
        }
        result
    }

    /// Keeps the name Apple posted with a sign-in, for when it finishes.
    pub fn note_name(&self, state: &str, name: String) {
        if let Some(flow) = self.0.flows.lock().unwrap().get_mut(state) {
            flow.name = Some(name);
        }
    }

    /// Keeps an account whose email matched `user` until they prove it's
    /// them. Returns the token for the cookie.
    pub fn hold_link(&self, identity: Identity, user: i64) -> String {
        let token = oidc::random_token();
        let link = PendingLink {
            identity,
            user,
            started: Instant::now(),
        };
        self.0.links.lock().unwrap().put(token.clone(), link);
        token
    }

    pub fn pending_link(&self, token: &str) -> Option<PendingLink> {
        self.0.links.lock().unwrap().get(token)
    }

    pub fn take_link(&self, token: &str) -> Option<PendingLink> {
        let link = self.0.links.lock().unwrap().take(token);
        link.filter(|l| l.started.elapsed() < WAIT)
    }
}

fn identity(provider_id: i64, s: &Settings, claims: oidc::Claims) -> Identity {
    Identity {
        provider_id,
        email: claims.verified_email(),
        name: claims.suggested_name(),
        groups: claims.groups(&s.groups_claim).unwrap_or_default(),
        subject: claims.sub,
    }
}

/// What a provider's groups say about someone.
struct Access {
    /// Set when the provider decides who is an admin.
    admin: Option<bool>,
    /// In an allowed group: being let in is the approval.
    approved: bool,
}

fn access(s: &Settings, identity: &Identity) -> Result<Access, SignInError> {
    let allowed = &s.allowed_groups.0;
    let approved = !allowed.is_empty();
    if approved && !identity.groups.iter().any(|g| allowed.contains(g)) {
        return Err(SignInError::Refused(format!(
            "Your {} account isn't in a group that may use Media Cabinet.",
            s.name
        )));
    }
    Ok(Access {
        admin: s.admin_group.as_ref().map(|g| identity.groups.contains(g)),
        approved,
    })
}

/// Where someone back from a provider goes next.
#[derive(Debug)]
pub enum Outcome {
    SignedIn(User),
    /// Their verified email is this user's: ask them to prove it's them.
    Confirm(User),
}

async fn login_user(
    pool: &SqlitePool,
    provider_id: i64,
    subject: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT user_id FROM user_login WHERE provider_id = ? AND subject = ?")
        .bind(provider_id)
        .bind(subject)
        .fetch_optional(pool)
        .await
}

async fn has_link(pool: &SqlitePool, user: i64, provider_id: i64) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM user_login WHERE user_id = ? AND provider_id = ?)",
    )
    .bind(user)
    .bind(provider_id)
    .fetch_one(pool)
    .await
}

/// Someone is back from a provider, signing in.
pub async fn sign_in(
    pool: &SqlitePool,
    provider: &Provider,
    identity: &Identity,
) -> Result<Outcome, SignInError> {
    let access = access(&provider.settings, identity)?;
    if let Some(user) = login_user(pool, provider.id, &identity.subject).await? {
        return Ok(Outcome::SignedIn(
            signed_in(pool, provider, identity, user, access).await?,
        ));
    }
    if let Some(email) = &identity.email
        && let Some(user) = accounts::by_email(pool, email).await?
        && !has_link(pool, user.id, provider.id).await?
    {
        return Ok(Outcome::Confirm(user));
    }
    create_user(pool, provider, identity)
        .await
        .map(Outcome::SignedIn)
}

/// A linked user signing in: their role and approval follow the provider's
/// groups, if it's set up that way.
async fn signed_in(
    pool: &SqlitePool,
    provider: &Provider,
    identity: &Identity,
    user: i64,
    access: Access,
) -> Result<User, SignInError> {
    let via = &provider.settings.name;
    let current = accounts::user(pool, user).await?;
    if current.status == UserStatus::Disabled {
        return Err(SignInError::Refused(
            "Your access was declined or turned off. Ask an admin.".into(),
        ));
    }
    sqlx::query(
        "UPDATE user_login SET last_used_at = ?, email = coalesce(?, email)
         WHERE provider_id = ? AND subject = ?",
    )
    .bind(now())
    .bind(&identity.email)
    .bind(provider.id)
    .bind(&identity.subject)
    .execute(pool)
    .await?;
    if let Some(admin) = access.admin
        && admin != current.is_admin
    {
        let changed = accounts::set_admin(pool, user, admin).await?;
        let mut entry = NewEntry::admins(
            activity::USER_CHANGED,
            now(),
            None,
            if changed {
                json!({ "name": current.name, "is_admin": admin, "fields": ["is_admin"], "via": via })
            } else {
                json!({ "name": current.name, "is_admin": true, "fields": [], "via": via, "kept_admin": true })
            },
        );
        entry.subject = Some(user);
        activity::record_or_warn(pool, entry).await;
    }
    if current.status == UserStatus::Pending && (access.approved || access.admin == Some(true)) {
        accounts::set_status(pool, user, UserStatus::Active).await?;
        let mut entry = NewEntry::admins(
            activity::USER_STATUS,
            now(),
            None,
            json!({ "name": current.name, "status": "active", "via": via }),
        );
        entry.subject = Some(user);
        activity::record_or_warn(pool, entry).await;
    }
    Ok(accounts::user(pool, user).await?)
}

/// A new user for someone back from a provider; pending unless the provider
/// lets new people straight in.
pub async fn create_user(
    pool: &SqlitePool,
    provider: &Provider,
    identity: &Identity,
) -> Result<User, SignInError> {
    let access = access(&provider.settings, identity)?;
    let s = &provider.settings;
    let admin = access.admin.unwrap_or(false);
    let status = if s.new_users == UserStatus::Active || access.approved || admin {
        UserStatus::Active
    } else {
        UserStatus::Pending
    };
    let user = accounts::create_external(
        pool,
        &identity.name,
        identity.email.as_deref(),
        admin,
        status,
    )
    .await?;
    insert_link(pool, identity, user.id).await?;
    let mut entry = NewEntry::admins(
        activity::USER_CREATED,
        now(),
        None,
        json!({
            "name": user.name,
            "is_admin": user.is_admin,
            "via": s.name,
            "pending": status == UserStatus::Pending,
        }),
    );
    entry.subject = Some(user.id);
    activity::record_or_warn(pool, entry).await;
    Ok(user)
}

async fn insert_link(pool: &SqlitePool, identity: &Identity, user: i64) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_login (user_id, provider_id, subject, email, last_used_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(user)
    .bind(identity.provider_id)
    .bind(&identity.subject)
    .bind(&identity.email)
    .bind(now())
    .execute(pool)
    .await?;
    Ok(())
}

/// Links an account at a provider to a user who has proven who they are.
pub async fn link(
    pool: &SqlitePool,
    provider: &Provider,
    identity: &Identity,
    user: i64,
    actor: Option<i64>,
) -> Result<(), SignInError> {
    let name = &provider.settings.name;
    match login_user(pool, provider.id, &identity.subject).await? {
        Some(owner) if owner == user => return Ok(()),
        Some(_) => {
            return Err(SignInError::Invalid(format!(
                "That {name} account is already linked to someone else."
            )));
        }
        None => {}
    }
    if has_link(pool, user, provider.id).await? {
        return Err(SignInError::Invalid(format!(
            "You already have a different {name} account linked. Unlink it first."
        )));
    }
    insert_link(pool, identity, user).await?;
    let target = accounts::user(pool, user).await?;
    let mut entry = NewEntry::admins(
        activity::LOGIN_LINKED,
        now(),
        actor,
        json!({ "name": target.name, "via": name }),
    );
    entry.subject = Some(user);
    activity::record_or_warn(pool, entry).await;
    Ok(())
}

/// A linked account, as its user and admins see it.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct Login {
    pub id: i64,
    pub provider_key: String,
    pub provider_name: String,
    /// The account's email at the provider, as last seen.
    pub email: Option<String>,
    pub linked_at: i64,
    pub last_used_at: Option<i64>,
}

pub async fn logins(pool: &SqlitePool, user: i64) -> Result<Vec<Login>, sqlx::Error> {
    sqlx::query_as(
        "SELECT l.id, p.key AS provider_key, p.name AS provider_name, l.email, l.linked_at,
                l.last_used_at
         FROM user_login l JOIN auth_provider p ON p.id = l.provider_id
         WHERE l.user_id = ? ORDER BY p.name",
    )
    .bind(user)
    .fetch_all(pool)
    .await
}

/// Removes a linked account, if the user still has another way in.
pub async fn unlink(
    pool: &SqlitePool,
    user: i64,
    login: i64,
    actor: Option<i64>,
) -> Result<(), SignInError> {
    let all = logins(pool, user).await?;
    let Some(gone) = all.iter().find(|l| l.id == login) else {
        return Err(SignInError::NotFound);
    };
    if all.len() == 1 && !accounts::has_password(pool, user).await? {
        return Err(SignInError::Invalid(format!(
            "{} is the only way to sign in to this account. Set a password first.",
            gone.provider_name
        )));
    }
    sqlx::query("DELETE FROM user_login WHERE id = ? AND user_id = ?")
        .bind(login)
        .bind(user)
        .execute(pool)
        .await?;
    let target = accounts::user(pool, user).await?;
    let mut entry = NewEntry::admins(
        activity::LOGIN_UNLINKED,
        now(),
        actor,
        json!({ "name": target.name, "via": gone.provider_name }),
    );
    entry.subject = Some(user);
    activity::record_or_warn(pool, entry).await;
    Ok(())
}

/// The provider that decides whether this user is an admin, if any: the
/// role-mapping provider they signed in with last.
pub async fn role_source(pool: &SqlitePool, user: i64) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT p.name FROM user_login l JOIN auth_provider p ON p.id = l.provider_id
         WHERE l.user_id = ? AND p.admin_group IS NOT NULL AND p.enabled
         ORDER BY l.last_used_at DESC NULLS LAST LIMIT 1",
    )
    .bind(user)
    .fetch_optional(pool)
    .await
}

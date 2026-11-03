//! Sign-in providers: stored in `auth_provider`, made in the UI or from
//! `CABINET_OIDC_*` ([`super::env`]).

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::types::Json;
use utoipa::ToSchema;

use super::SignInError;
use super::env::EnvProvider;
use super::presets;
use crate::accounts::UserStatus;
use crate::{apple, oidc};

pub const DEFAULT_SCOPES: &str = "openid profile email";
pub const DEFAULT_GROUPS_CLAIM: &str = "groups";

/// How a provider signs people in.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ToSchema, sqlx::Type,
)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum Kind {
    /// OpenID Connect: everything else comes from the issuer's discovery.
    #[default]
    Oidc,
    Github,
    Discord,
    /// Plain OAuth2 with a user endpoint, set up by hand.
    #[serde(rename = "oauth2")]
    #[sqlx(rename = "oauth2")]
    OAuth2,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Oidc => "OpenID Connect",
            Kind::Github => "GitHub",
            Kind::Discord => "Discord",
            Kind::OAuth2 => "OAuth2",
        }
    }

    pub fn default_scopes(self) -> &'static str {
        match self {
            Kind::Oidc => DEFAULT_SCOPES,
            Kind::Github => "read:user user:email",
            Kind::Discord => "identify email",
            Kind::OAuth2 => "",
        }
    }

    /// GitHub and Discord sign-ins don't use groups.
    pub fn has_groups(self) -> bool {
        matches!(self, Kind::Oidc | Kind::OAuth2)
    }
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Provider {
    pub id: i64,
    pub key: String,
    /// `ui` or `env`.
    pub source: String,
    /// An env provider edited in the UI instead; its env vars are ignored.
    pub overridden: bool,
    #[sqlx(flatten)]
    pub settings: Settings,
}

impl Provider {
    /// Whether the environment manages it, so the UI may not change it.
    pub fn from_env(&self) -> bool {
        self.source == "env" && !self.overridden
    }

    /// Whether signing in with it decides who is an admin.
    pub fn maps_roles(&self) -> bool {
        self.settings.admin_group.is_some()
    }
}

/// Everything about a provider that can be set.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct Settings {
    pub kind: Kind,
    pub name: String,
    /// OIDC only; empty for the others.
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    /// `Active` or `Pending`.
    pub new_users: UserStatus,
    pub groups_claim: String,
    pub admin_group: Option<String>,
    pub allowed_groups: Json<Vec<String>>,
    pub scopes: String,
    pub redirect_url: Option<String>,
    pub enabled: bool,
    /// Plain OAuth2 (`Kind::OAuth2`) only, as are the fields below.
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    /// Dotted paths into the user endpoint's answer.
    pub id_field: Option<String>,
    pub name_field: Option<String>,
    pub email_field: Option<String>,
    /// Unset: emails count as unverified.
    pub verified_field: Option<String>,
    /// Picked by name in the form; see [`super::presets`].
    pub preset: Option<String>,
    /// Sign in with Apple only, with the private key as the client secret.
    pub team_id: Option<String>,
    pub key_id: Option<String>,
}

impl Settings {
    /// Defaults for everything but what has to be given.
    pub fn new(name: &str, issuer: &str, client_id: &str, client_secret: &str) -> Settings {
        Settings {
            kind: Kind::Oidc,
            name: name.into(),
            issuer: issuer.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            new_users: UserStatus::Pending,
            groups_claim: DEFAULT_GROUPS_CLAIM.into(),
            admin_group: None,
            allowed_groups: Json(vec![]),
            scopes: DEFAULT_SCOPES.into(),
            redirect_url: None,
            enabled: true,
            authorize_url: None,
            token_url: None,
            userinfo_url: None,
            id_field: None,
            name_field: None,
            email_field: None,
            verified_field: None,
            preset: None,
            team_id: None,
            key_id: None,
        }
    }

    pub fn is_apple(&self) -> bool {
        self.preset.as_deref() == Some("apple")
    }

    /// The client secret to send now: Apple's is signed for each request.
    pub fn secret_now(&self, now: i64) -> Result<String, oidc::OidcError> {
        if !self.is_apple() {
            return Ok(self.client_secret.clone());
        }
        apple::client_secret(
            self.team_id.as_deref().unwrap_or_default(),
            self.key_id.as_deref().unwrap_or_default(),
            &self.client_id,
            &self.client_secret,
            now,
        )
    }

    /// A GitHub, Discord or plain OAuth2 provider; set the URLs and fields of
    /// a plain one afterwards.
    pub fn oauth2(kind: Kind, name: &str, client_id: &str, client_secret: &str) -> Settings {
        Settings {
            kind,
            scopes: kind.default_scopes().into(),
            ..Settings::new(name, "", client_id, client_secret)
        }
    }

    /// Tidies and checks the settings, as saving them would.
    pub fn checked(mut self) -> Result<Settings, SignInError> {
        let invalid = |m: &str| Err(SignInError::Invalid(m.into()));
        self.name = self.name.trim().to_string();
        self.issuer = self.issuer.trim().to_string();
        self.client_id = self.client_id.trim().to_string();
        self.client_secret = self.client_secret.trim().to_string();
        self.groups_claim = self.groups_claim.trim().to_string();
        if self.name.is_empty() || self.name.chars().count() > 64 {
            return invalid("Give the provider a name of 1 to 64 characters.");
        }
        if self.client_id.is_empty() {
            return invalid("Enter the client ID from the provider.");
        }
        if self.client_secret.is_empty() {
            return invalid("Enter the client secret from the provider.");
        }
        if self.new_users == UserStatus::Disabled {
            return invalid("New people are either let in or wait for approval.");
        }
        if self.groups_claim.is_empty() {
            self.groups_claim = DEFAULT_GROUPS_CLAIM.into();
        }
        self.admin_group = self
            .admin_group
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty());
        let mut allowed: Vec<String> = self
            .allowed_groups
            .0
            .iter()
            .map(|g| g.trim().to_string())
            .filter(|g| !g.is_empty())
            .collect();
        allowed.dedup();
        self.allowed_groups = Json(allowed);
        let tidy = |v: Option<String>| v.map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        self.preset = tidy(self.preset);
        let preset = match &self.preset {
            Some(key) => Some(
                presets::find(key)
                    .filter(|p| p.kind == self.kind)
                    .ok_or_else(|| SignInError::Invalid(format!("Unknown provider: {key}.")))?,
            ),
            None => None,
        };
        let mut scopes: Vec<&str> = self.scopes.split_whitespace().collect();
        if scopes.is_empty() {
            let default = preset.and_then(|p| p.scopes);
            scopes = default
                .unwrap_or(self.kind.default_scopes())
                .split_whitespace()
                .collect();
        }
        if self.kind == Kind::Oidc && !scopes.contains(&"openid") {
            scopes.insert(0, "openid");
        }
        self.scopes = scopes.join(" ");
        let groups = self.kind.has_groups() && preset.is_none_or(|p| p.groups);
        if !groups && (self.admin_group.is_some() || !self.allowed_groups.0.is_empty()) {
            return Err(SignInError::Invalid(format!(
                "{} sign-in doesn't use groups; leave the admin group and allowed groups empty.",
                preset.map_or(self.kind.label(), |p| p.label)
            )));
        }
        match preset.map(|p| p.issuer) {
            Some(presets::Issuer::Fixed(issuer)) => self.issuer = issuer.into(),
            Some(presets::Issuer::Default(issuer)) if self.issuer.is_empty() => {
                self.issuer = issuer.into()
            }
            _ => {}
        }
        if self.is_apple() {
            self.team_id = tidy(self.team_id);
            self.key_id = tidy(self.key_id);
            if self.team_id.is_none() || self.key_id.is_none() {
                return invalid("Enter the Team ID and Key ID from Apple's developer site.");
            }
            // Signing once now finds a key that isn't one.
            self.secret_now(0)
                .map_err(|e| SignInError::Invalid(e.to_string()))?;
        } else {
            self.team_id = None;
            self.key_id = None;
        }
        if self.kind == Kind::Oidc {
            if !oidc::is_secure(&self.issuer) {
                return invalid("The issuer URL must be an https:// address.");
            }
        } else {
            self.issuer = String::new();
        }
        if self.kind == Kind::OAuth2 {
            self.authorize_url = tidy(self.authorize_url);
            self.token_url = tidy(self.token_url);
            self.userinfo_url = tidy(self.userinfo_url);
            for (what, url) in [
                ("authorization URL", &self.authorize_url),
                ("token URL", &self.token_url),
                ("user URL", &self.userinfo_url),
            ] {
                match url {
                    Some(url) if oidc::is_secure(url) => {}
                    Some(_) => {
                        return Err(SignInError::Invalid(format!(
                            "The {what} must be an https:// address."
                        )));
                    }
                    None => {
                        return Err(SignInError::Invalid(format!("Enter the {what}.")));
                    }
                }
            }
            self.id_field = tidy(self.id_field);
            self.name_field = tidy(self.name_field);
            self.email_field = tidy(self.email_field);
            self.verified_field = tidy(self.verified_field);
        } else {
            self.authorize_url = None;
            self.token_url = None;
            self.userinfo_url = None;
            self.id_field = None;
            self.name_field = None;
            self.email_field = None;
            self.verified_field = None;
        }
        self.redirect_url = self
            .redirect_url
            .map(|u| u.trim().to_string())
            .filter(|u| !u.is_empty());
        if let Some(url) = &self.redirect_url
            && reqwest::Url::parse(url).is_err()
        {
            return invalid("The redirect URL override isn't a URL.");
        }
        Ok(self)
    }
}

/// A provider's key from its name: lower case, letters, digits and dashes.
pub fn key_from_name(name: &str) -> String {
    let mut key = String::new();
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            key.push(c.to_ascii_lowercase());
        } else if !key.is_empty() && !key.ends_with('-') {
            key.push('-');
        }
    }
    key.trim_end_matches('-').chars().take(32).collect()
}

pub fn check_key(key: &str) -> Result<(), SignInError> {
    let ok = !key.is_empty()
        && key.len() <= 32
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !key.starts_with('-')
        && !key.ends_with('-');
    if ok {
        return Ok(());
    }
    Err(SignInError::Invalid(
        "A provider's key is 1 to 32 lower-case letters, digits and dashes.".into(),
    ))
}

const COLUMNS: &str = "id, key, source, overridden, kind, name, issuer, client_id, client_secret,
    new_users, groups_claim, admin_group, allowed_groups, scopes, redirect_url, enabled,
    authorize_url, token_url, userinfo_url, id_field, name_field, email_field, verified_field,
    preset, team_id, key_id";

pub async fn list(pool: &SqlitePool) -> Result<Vec<Provider>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM auth_provider ORDER BY created_at, id"
    )))
    .fetch_all(pool)
    .await
}

pub async fn by_key(pool: &SqlitePool, key: &str) -> Result<Option<Provider>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM auth_provider WHERE key = ?"
    )))
    .bind(key)
    .fetch_optional(pool)
    .await
}

pub async fn by_id(pool: &SqlitePool, id: i64) -> Result<Option<Provider>, sqlx::Error> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM auth_provider WHERE id = ?"
    )))
    .bind(id)
    .fetch_optional(pool)
    .await
}

async fn get(pool: &SqlitePool, key: &str) -> Result<Provider, SignInError> {
    by_key(pool, key).await?.ok_or(SignInError::NotFound)
}

async fn write(pool: &SqlitePool, id: i64, s: &Settings) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE auth_provider SET kind = ?, name = ?, issuer = ?, client_id = ?,
             client_secret = ?, new_users = ?, groups_claim = ?, admin_group = ?,
             allowed_groups = ?, scopes = ?, redirect_url = ?, enabled = ?, authorize_url = ?,
             token_url = ?, userinfo_url = ?, id_field = ?, name_field = ?, email_field = ?,
             verified_field = ?, preset = ?, team_id = ?, key_id = ?
         WHERE id = ?",
    )
    .bind(s.kind)
    .bind(&s.name)
    .bind(&s.issuer)
    .bind(&s.client_id)
    .bind(&s.client_secret)
    .bind(s.new_users)
    .bind(&s.groups_claim)
    .bind(&s.admin_group)
    .bind(&s.allowed_groups)
    .bind(&s.scopes)
    .bind(&s.redirect_url)
    .bind(s.enabled)
    .bind(&s.authorize_url)
    .bind(&s.token_url)
    .bind(&s.userinfo_url)
    .bind(&s.id_field)
    .bind(&s.name_field)
    .bind(&s.email_field)
    .bind(&s.verified_field)
    .bind(&s.preset)
    .bind(&s.team_id)
    .bind(&s.key_id)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert(
    pool: &SqlitePool,
    key: &str,
    source: &str,
    s: &Settings,
) -> Result<Provider, SignInError> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO auth_provider (key, source, name, issuer, client_id, client_secret)
         VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(key)
    .bind(source)
    .bind(&s.name)
    .bind(&s.issuer)
    .bind(&s.client_id)
    .bind(&s.client_secret)
    .fetch_one(pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => {
            SignInError::Invalid(format!("There's already a provider with the key {key}."))
        }
        _ => e.into(),
    })?;
    write(pool, id, s).await?;
    by_id(pool, id).await?.ok_or(SignInError::NotFound)
}

/// Adds a provider made in the UI.
pub async fn create(pool: &SqlitePool, key: &str, s: Settings) -> Result<Provider, SignInError> {
    check_key(key)?;
    insert(pool, key, "ui", &s.checked()?).await
}

/// How many people have linked an account at the provider.
pub async fn linked_users(pool: &SqlitePool, id: i64) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("SELECT count(*) FROM user_login WHERE provider_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
}

/// People who can only sign in with this provider: no password, no other link.
pub async fn sole_users(pool: &SqlitePool, id: i64) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT u.id FROM app_user u JOIN user_login l ON l.user_id = u.id
         WHERE l.provider_id = ? AND u.password_hash IS NULL
           AND NOT EXISTS (SELECT 1 FROM user_login o
                           WHERE o.user_id = u.id AND o.provider_id != l.provider_id)",
    )
    .bind(id)
    .fetch_all(pool)
    .await
}

/// Changes a provider made in the UI, or an env provider that was overridden.
/// A new key is only allowed while no one has linked an account.
pub async fn update(
    pool: &SqlitePool,
    key: &str,
    new_key: Option<&str>,
    s: Settings,
) -> Result<Provider, SignInError> {
    let current = get(pool, key).await?;
    if current.from_env() {
        return Err(SignInError::Invalid(
            "This provider comes from environment variables. Choose \"Edit here instead\" first."
                .into(),
        ));
    }
    let s = s.checked()?;
    if let Some(new_key) = new_key
        && new_key != key
    {
        check_key(new_key)?;
        if current.source == "env" {
            return Err(SignInError::Invalid(
                "The key of a provider from environment variables can't change.".into(),
            ));
        }
        if linked_users(pool, current.id).await? > 0 {
            return Err(SignInError::Invalid(
                "The key can't change once someone has signed in with it.".into(),
            ));
        }
        sqlx::query("UPDATE auth_provider SET key = ? WHERE id = ?")
            .bind(new_key)
            .bind(current.id)
            .execute(pool)
            .await
            .map_err(|e| match &e {
                sqlx::Error::Database(d) if d.is_unique_violation() => SignInError::Invalid(
                    format!("There's already a provider with the key {new_key}."),
                ),
                _ => e.into(),
            })?;
    }
    write(pool, current.id, &s).await?;
    by_id(pool, current.id).await?.ok_or(SignInError::NotFound)
}

/// Removes a provider and its links. People who could only sign in with it
/// are turned off until an admin gives them a password. Returns how many.
pub async fn delete(pool: &SqlitePool, key: &str) -> Result<(Provider, usize), SignInError> {
    let current = get(pool, key).await?;
    let stranded = sole_users(pool, current.id).await?;
    let mut tx = crate::db::begin_write(pool).await?;
    for user in &stranded {
        sqlx::query("UPDATE app_user SET status = 'disabled' WHERE id = ? AND NOT is_admin")
            .bind(user)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM session WHERE user_id = ?")
            .bind(user)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("DELETE FROM auth_provider WHERE id = ?")
        .bind(current.id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((current, stranded.len()))
}

/// Edit an env provider here instead: from now on its env vars are ignored.
pub async fn take_over(pool: &SqlitePool, key: &str) -> Result<Provider, SignInError> {
    let current = get(pool, key).await?;
    if current.source != "env" {
        return Err(SignInError::Invalid(
            "This provider was made here, not from environment variables.".into(),
        ));
    }
    sqlx::query("UPDATE auth_provider SET overridden = 1 WHERE id = ?")
        .bind(current.id)
        .execute(pool)
        .await?;
    get(pool, key).await
}

/// Go back to the environment: reapplies the env vars and stops editing here.
pub async fn give_back(
    pool: &SqlitePool,
    key: &str,
    env: &[EnvProvider],
) -> Result<Provider, SignInError> {
    let current = get(pool, key).await?;
    let Some(from_env) = env.iter().find(|p| p.key == key) else {
        return Err(SignInError::Invalid(format!(
            "No {}* variables are set any more.",
            super::env::prefix(key)
        )));
    };
    write(pool, current.id, &from_env.settings).await?;
    sqlx::query("UPDATE auth_provider SET overridden = 0 WHERE id = ?")
        .bind(current.id)
        .execute(pool)
        .await?;
    get(pool, key).await
}

/// Writes env providers to the database at startup. Returns warnings to log.
pub async fn sync_env(pool: &SqlitePool, env: &[EnvProvider]) -> Result<Vec<String>, SignInError> {
    let mut warnings = vec![];
    for p in env {
        let vars = format!("{}*", super::env::prefix(&p.key));
        match by_key(pool, &p.key).await? {
            None => {
                insert(pool, &p.key, "env", &p.settings).await?;
            }
            Some(existing) if existing.source == "ui" => warnings.push(format!(
                "{vars} are ignored: a provider made in Media Cabinet already uses the key {}",
                p.key
            )),
            Some(existing) if existing.overridden => warnings.push(format!(
                "{vars} are ignored: the provider is edited in Media Cabinet instead \
                 (choose \"Go back to the environment\" to use them)"
            )),
            Some(existing) => {
                if existing.settings != p.settings {
                    write(pool, existing.id, &p.settings).await?;
                }
            }
        }
    }
    for gone in list(pool).await? {
        if gone.from_env() && gone.settings.enabled && !env.iter().any(|p| p.key == gone.key) {
            sqlx::query("UPDATE auth_provider SET enabled = 0 WHERE id = ?")
                .bind(gone.id)
                .execute(pool)
                .await?;
            warnings.push(format!(
                "{}* are no longer set: the provider {} is turned off; delete it in Manage → Users if it's gone for good",
                super::env::prefix(&gone.key),
                gone.key
            ));
        }
    }
    Ok(warnings)
}

/// A button on the login page.
#[derive(Debug, Serialize, ToSchema, sqlx::FromRow)]
pub struct ProviderButton {
    pub key: String,
    pub name: String,
}

/// The providers people can sign in with now.
pub async fn buttons(pool: &SqlitePool) -> Result<Vec<ProviderButton>, sqlx::Error> {
    sqlx::query_as("SELECT key, name FROM auth_provider WHERE enabled ORDER BY created_at, id")
        .fetch_all(pool)
        .await
}

/// Whether the login page shows the password form. `env` is
/// `CABINET_PASSWORD_SIGN_IN`, which wins when set.
pub async fn password_sign_in(pool: &SqlitePool, env: Option<bool>) -> Result<bool, sqlx::Error> {
    if let Some(on) = env {
        return Ok(on);
    }
    sqlx::query_scalar("SELECT password_sign_in FROM auth_setting WHERE id = 1")
        .fetch_optional(pool)
        .await
        .map(|v| v.unwrap_or(true))
}

pub async fn set_password_sign_in(pool: &SqlitePool, on: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE auth_setting SET password_sign_in = ? WHERE id = 1")
        .bind(on)
        .execute(pool)
        .await?;
    Ok(())
}

/// A provider as admins see it: never the secret.
#[derive(Debug, Serialize, ToSchema)]
pub struct ProviderView {
    pub key: String,
    pub kind: Kind,
    pub name: String,
    pub issuer: String,
    pub client_id: String,
    pub has_secret: bool,
    pub new_users: UserStatus,
    pub groups_claim: String,
    pub admin_group: Option<String>,
    pub allowed_groups: Vec<String>,
    pub scopes: String,
    pub redirect_url: Option<String>,
    pub enabled: bool,
    pub authorize_url: Option<String>,
    pub token_url: Option<String>,
    pub userinfo_url: Option<String>,
    pub id_field: Option<String>,
    pub name_field: Option<String>,
    pub email_field: Option<String>,
    pub verified_field: Option<String>,
    pub preset: Option<String>,
    pub team_id: Option<String>,
    pub key_id: Option<String>,
    /// Set for providers from environment variables.
    pub env: Option<EnvInfo>,
    /// People with a linked account.
    pub users: i64,
    /// People who can sign in only with this provider.
    pub sole_users: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnvInfo {
    /// e.g. `CABINET_OIDC_AUTHENTIK_`; a field's variable is this plus its
    /// name in upper case.
    pub prefix: String,
    /// The fields set by a variable; the rest have their defaults.
    pub fields: Vec<String>,
    /// Edited here instead; the variables are ignored.
    pub overridden: bool,
    /// The variables are no longer set.
    pub missing: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ProviderInput {
    /// Defaults to one made from the name.
    pub key: Option<String>,
    #[serde(default)]
    pub kind: Kind,
    pub name: String,
    /// OIDC only.
    #[serde(default)]
    pub issuer: String,
    pub client_id: String,
    /// Leave out to keep the current one.
    pub client_secret: Option<String>,
    #[serde(default = "pending")]
    pub new_users: UserStatus,
    #[serde(default)]
    pub groups_claim: Option<String>,
    #[serde(default)]
    pub admin_group: Option<String>,
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default)]
    pub scopes: Option<String>,
    #[serde(default)]
    pub redirect_url: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Plain OAuth2 only, as are the fields below.
    #[serde(default)]
    pub authorize_url: Option<String>,
    #[serde(default)]
    pub token_url: Option<String>,
    #[serde(default)]
    pub userinfo_url: Option<String>,
    #[serde(default)]
    pub id_field: Option<String>,
    #[serde(default)]
    pub name_field: Option<String>,
    #[serde(default)]
    pub email_field: Option<String>,
    #[serde(default)]
    pub verified_field: Option<String>,
    /// Picked by name in the form, e.g. `keycloak`.
    #[serde(default)]
    pub preset: Option<String>,
    /// Sign in with Apple only, as is `key_id`; the private key goes in
    /// `client_secret`.
    #[serde(default)]
    pub team_id: Option<String>,
    #[serde(default)]
    pub key_id: Option<String>,
}

fn pending() -> UserStatus {
    UserStatus::Pending
}

fn yes() -> bool {
    true
}

impl ProviderInput {
    /// The settings, keeping `secret` when none is given.
    pub fn settings(self, secret: Option<&str>) -> Settings {
        let client_secret = self
            .client_secret
            .filter(|s| !s.trim().is_empty())
            .or_else(|| secret.map(str::to_string))
            .unwrap_or_default();
        Settings {
            kind: self.kind,
            name: self.name,
            issuer: self.issuer,
            client_id: self.client_id,
            client_secret,
            new_users: self.new_users,
            groups_claim: self
                .groups_claim
                .unwrap_or_else(|| DEFAULT_GROUPS_CLAIM.into()),
            admin_group: self.admin_group,
            allowed_groups: Json(self.allowed_groups),
            // Empty: the preset's or type's own, filled in by `checked`.
            scopes: self.scopes.unwrap_or_default(),
            redirect_url: self.redirect_url,
            enabled: self.enabled,
            authorize_url: self.authorize_url,
            token_url: self.token_url,
            userinfo_url: self.userinfo_url,
            id_field: self.id_field,
            name_field: self.name_field,
            email_field: self.email_field,
            verified_field: self.verified_field,
            preset: self.preset,
            team_id: self.team_id,
            key_id: self.key_id,
        }
    }
}

pub async fn view(
    pool: &SqlitePool,
    p: Provider,
    env: &[EnvProvider],
) -> Result<ProviderView, sqlx::Error> {
    let users = linked_users(pool, p.id).await?;
    let sole = sole_users(pool, p.id).await?.len() as i64;
    let env_info = (p.source == "env").then(|| {
        let from_env = env.iter().find(|e| e.key == p.key);
        EnvInfo {
            prefix: super::env::prefix(&p.key),
            fields: from_env
                .map(|e| e.fields.iter().map(|f| f.to_string()).collect())
                .unwrap_or_default(),
            overridden: p.overridden,
            missing: from_env.is_none(),
        }
    });
    let s = p.settings;
    Ok(ProviderView {
        key: p.key,
        kind: s.kind,
        name: s.name,
        issuer: s.issuer,
        client_id: s.client_id,
        has_secret: !s.client_secret.is_empty(),
        new_users: s.new_users,
        groups_claim: s.groups_claim,
        admin_group: s.admin_group,
        allowed_groups: s.allowed_groups.0,
        scopes: s.scopes,
        redirect_url: s.redirect_url,
        enabled: s.enabled,
        authorize_url: s.authorize_url,
        token_url: s.token_url,
        userinfo_url: s.userinfo_url,
        id_field: s.id_field,
        name_field: s.name_field,
        email_field: s.email_field,
        verified_field: s.verified_field,
        preset: s.preset,
        team_id: s.team_id,
        key_id: s.key_id,
        env: env_info,
        users,
        sole_users: sole,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_from_names() {
        assert_eq!(key_from_name("Authentik"), "authentik");
        assert_eq!(key_from_name("  Pocket ID! "), "pocket-id");
        assert_eq!(key_from_name("My  Google / Work"), "my-google-work");
        assert!(check_key("pocket-id").is_ok());
        assert!(check_key("Pocket").is_err());
        assert!(check_key("-a").is_err());
        assert!(check_key("").is_err());
    }

    #[test]
    fn settings_are_tidied() {
        let mut s = Settings::new(" Authentik ", "https://auth.example.com/", "id", "secret");
        s.scopes = "profile email".into();
        s.admin_group = Some("  ".into());
        s.allowed_groups = Json(vec!["a".into(), " ".into(), "a".into()]);
        let s = s.checked().unwrap();
        assert_eq!(s.name, "Authentik");
        assert_eq!(s.scopes, "openid profile email");
        assert_eq!(s.admin_group, None);
        assert_eq!(s.allowed_groups.0, ["a"]);
        assert!(
            Settings::new("x", "http://auth.lan", "id", "secret")
                .checked()
                .is_err()
        );
        assert!(Settings::new("x", "https://a", "id", "").checked().is_err());
    }
}

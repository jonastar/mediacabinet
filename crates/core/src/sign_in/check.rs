//! Trying a provider's settings before anyone depends on them. Design:
//! `docs/sign-in.md`.
//!
//! [`SignIn::check`] asks the provider without anyone signing in: its
//! discovery document (issuer, scopes, claims) and its token endpoint (the
//! client ID and secret). [`SignIn::start_test`] is a real sign-in, by the
//! admin, that changes nothing: what came back, and what a real sign-in would
//! have done, is kept for the admin's page to fetch.

use serde::Serialize;
use sqlx::SqlitePool;
use utoipa::ToSchema;

use super::{
    Flow, Identity, Kind, Purpose, Settings, SignIn, SignInError, access, has_link, identity,
    login_user,
};
use crate::accounts::{self, UserStatus};
use crate::oauth2;
use crate::oidc::{self, ClientCheck, Discovery};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Good,
    Info,
    Warning,
    Problem,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Note {
    pub level: Level,
    pub text: String,
}

fn note(level: Level, text: impl Into<String>) -> Note {
    Note {
        level,
        text: text.into(),
    }
}

/// What the provider says about the settings, without anyone signing in.
#[derive(Debug, Serialize, ToSchema)]
pub struct CheckReport {
    /// The issuer as the provider names itself, if it was found.
    pub issuer: Option<String>,
    /// The scopes a sign-in would ask for.
    pub scopes: Option<String>,
    pub notes: Vec<Note>,
}

/// What came back from a test sign-in. Nothing was saved.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TestReport {
    pub provider: String,
    /// What a real sign-in would have done with this account.
    pub outcome: Option<String>,
    /// As asked for.
    pub scopes: String,
    /// As granted, if the provider said.
    pub granted_scopes: Option<String>,
    pub subject: Option<String>,
    /// The name a new user would get.
    pub name: Option<String>,
    pub email: Option<String>,
    pub email_verified: bool,
    /// Missing when the groups claim wasn't there.
    pub groups: Option<Vec<String>>,
    /// Every claim's name, from the ID token and userinfo.
    pub claims: Vec<String>,
    pub notes: Vec<Note>,
}

impl TestReport {
    fn new(provider: &str, scopes: &str) -> TestReport {
        TestReport {
            provider: provider.into(),
            outcome: None,
            scopes: scopes.into(),
            granted_scopes: None,
            subject: None,
            name: None,
            email: None,
            email_verified: false,
            groups: None,
            claims: vec![],
            notes: vec![],
        }
    }

    fn failed(mut self, text: impl Into<String>) -> TestReport {
        self.notes.push(note(Level::Problem, text));
        self
    }
}

pub(super) fn wants_groups(s: &Settings) -> bool {
    s.admin_group.is_some() || !s.allowed_groups.0.is_empty()
}

/// Scopes are space-separated; GitHub reports granted ones comma-separated.
fn has_scope(scopes: &str, scope: &str) -> bool {
    scopes
        .split(|c: char| c.is_whitespace() || c == ',')
        .any(|s| s == scope)
}

/// Whether the provider lists `scope`; `None` when it doesn't list scopes.
fn offers(doc: &Discovery, scope: &str) -> Option<bool> {
    doc.scopes_supported
        .as_ref()
        .map(|list| list.iter().any(|s| s == scope))
}

/// The scopes a sign-in asks for: the provider's own, plus `groups` when
/// groups are used and the provider offers that scope.
pub(super) fn scopes(s: &Settings, doc: &Discovery) -> String {
    let mut scopes = s.scopes.clone();
    if wants_groups(s) && offers(doc, "groups") == Some(true) && !has_scope(&scopes, "groups") {
        scopes.push_str(" groups");
    }
    scopes
}

fn listing(items: &[impl AsRef<str>]) -> String {
    items
        .iter()
        .map(|i| format!("`{}`", i.as_ref()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Notes on the scopes asked for, from what the provider lists.
fn scope_notes(s: &Settings, doc: &Discovery) -> Vec<Note> {
    let asked = scopes(s, doc);
    let mut notes = vec![];
    match &doc.scopes_supported {
        Some(supported) => {
            let unknown: Vec<&str> = asked
                .split_whitespace()
                .filter(|a| !supported.iter().any(|s| s == a))
                .collect();
            if unknown.is_empty() {
                notes.push(note(
                    Level::Good,
                    format!("The provider offers every scope asked for: {asked}."),
                ));
            } else {
                notes.push(note(
                    Level::Warning,
                    format!(
                        "The provider doesn't list {} among its scopes and may refuse to sign \
                         anyone in. Remove {} under Scopes, or try signing in to be sure.",
                        listing(&unknown),
                        if unknown.len() == 1 {
                            "it"
                        } else {
                            "them"
                        },
                    ),
                ));
            }
        }
        None => notes.push(note(
            Level::Info,
            "The provider doesn't list its scopes. Try signing in to be sure it accepts them.",
        )),
    }
    if wants_groups(s) {
        let claim_listed = doc
            .claims_supported
            .as_ref()
            .map(|list| list.contains(&s.groups_claim));
        if has_scope(&asked, "groups") && !has_scope(&s.scopes, "groups") {
            notes.push(note(
                Level::Info,
                "Also asks for the `groups` scope, for the admin group and allowed groups.",
            ));
        } else if offers(doc, "groups") != Some(true) && claim_listed != Some(true) {
            notes.push(note(
                Level::Warning,
                format!(
                    "The provider doesn't list a `groups` scope or a `{}` claim. Try signing in \
                     to see whether groups come back.",
                    s.groups_claim
                ),
            ));
        }
    }
    notes
}

impl SignIn {
    /// Checks settings with the provider, without anyone signing in. The
    /// made-up code sent to check the client goes with `redirect_uri`, as a
    /// real one would.
    pub async fn check(&self, settings: Settings, redirect_uri: &str) -> CheckReport {
        let mut report = CheckReport {
            issuer: None,
            scopes: None,
            notes: vec![],
        };
        let s = match settings.checked() {
            Ok(s) => s,
            Err(e) => {
                report.notes.push(note(Level::Problem, e.to_string()));
                return report;
            }
        };
        let doc = match oauth2::discovery(&s) {
            Some(doc) => {
                report.notes.push(note(
                    Level::Info,
                    format!(
                        "{} doesn't publish its scopes. Try signing in to be sure it accepts \
                         them.",
                        if s.kind == Kind::OAuth2 {
                            "The provider"
                        } else {
                            s.kind.label()
                        }
                    ),
                ));
                doc
            }
            None => match oidc::discover(&self.0.http, &s.issuer).await {
                Ok(doc) => {
                    self.forget(&doc.issuer);
                    report.notes.push(note(
                        Level::Good,
                        format!("Found the provider at {}.", doc.issuer),
                    ));
                    report.notes.extend(scope_notes(&s, &doc));
                    report.issuer = Some(doc.issuer.clone());
                    doc
                }
                Err(e) => {
                    report.notes.push(note(Level::Problem, e.to_string()));
                    return report;
                }
            },
        };
        let client = match s.secret_now(crate::catalog::now()) {
            Ok(secret) => {
                oidc::check_client(&self.0.http, &doc, &s.client_id, &secret, redirect_uri).await
            }
            Err(e) => Err(e),
        };
        report.notes.push(match client {
            Ok(ClientCheck::Accepted) => note(
                Level::Good,
                "The client ID and secret are right: the provider only objected to the made-up \
                 code it was sent to check them.",
            ),
            Ok(ClientCheck::Rejected(why)) => note(
                Level::Problem,
                format!("The provider rejected the client ID or secret: {why}."),
            ),
            Ok(ClientCheck::Unclear(why)) => note(
                Level::Info,
                format!(
                    "Couldn't tell whether the client ID and secret are right (the provider \
                     said {why}). Try signing in."
                ),
            ),
            Err(e) => note(Level::Problem, e.to_string()),
        });
        report.scopes = Some(scopes(&s, &doc));
        report
    }

    /// Starts a test sign-in with `settings`, which needn't be saved or
    /// turned on. `existing` is the provider being edited, if any.
    pub async fn start_test(
        &self,
        settings: Settings,
        existing: Option<i64>,
        redirect_uri: String,
    ) -> Result<(String, String), SignInError> {
        let s = settings.checked()?;
        let purpose = Purpose::Test {
            settings: Box::new(s.clone()),
            existing,
        };
        self.start_flow(&s, existing.unwrap_or(0), redirect_uri, purpose)
            .await
    }

    /// Whether `state` is a test sign-in waiting to come back.
    pub fn is_test(&self, state: &str) -> bool {
        self.0
            .flows
            .lock()
            .unwrap()
            .get(state)
            .is_some_and(|f| matches!(f.purpose, Purpose::Test { .. }))
    }

    /// Finishes a test sign-in. `answer` is the provider's code, or its error
    /// and description. Returns the token to fetch the report with.
    pub async fn finish_test(
        &self,
        pool: &SqlitePool,
        state: &str,
        answer: Result<&str, (&str, Option<&str>)>,
    ) -> String {
        let flow = self.0.flows.lock().unwrap().take(state);
        let report = match flow {
            Some(flow) => self.report(pool, flow, answer).await,
            None => TestReport::new("the provider", "")
                .failed("This test expired or was already used. Try again."),
        };
        let token = oidc::random_token();
        self.0.tests.lock().unwrap().put(token.clone(), report);
        token
    }

    /// A finished test's report, for ten minutes.
    pub fn test_report(&self, token: &str) -> Option<TestReport> {
        self.0.tests.lock().unwrap().get(token)
    }

    async fn report(
        &self,
        pool: &SqlitePool,
        flow: Flow,
        answer: Result<&str, (&str, Option<&str>)>,
    ) -> TestReport {
        let Purpose::Test { settings, existing } = &flow.purpose else {
            return TestReport::new("the provider", "").failed("That wasn't a test.");
        };
        let s = settings.as_ref();
        let mut report = TestReport::new(&s.name, &flow.scopes);
        let code = match answer {
            Ok(code) => code,
            Err((error, description)) => {
                let said = description.unwrap_or(error);
                return report.failed(match error {
                    "invalid_scope" => format!(
                        "The provider refused the scopes asked for (`{}`): {said}. Remove the ones \
                         it doesn't know under Scopes.",
                        flow.scopes
                    ),
                    "access_denied" | "user_cancelled_authorize" => format!(
                        "The sign-in was cancelled, or the provider didn't let you in: {said}."
                    ),
                    _ => format!("The provider said: {said}"),
                });
            }
        };
        let (claims, granted) = match self.claims(s, &flow, code).await {
            Ok(found) => found,
            Err(e) => {
                let text = e.to_string();
                let hint = if text.contains("invalid_client") {
                    " Check the client ID and secret."
                } else {
                    ""
                };
                return report.failed(format!("{text}.{hint}"));
            }
        };

        let mut names: Vec<String> = claims.all.keys().cloned().collect();
        names.sort();
        report.claims = names;
        report.subject = Some(claims.sub.clone());
        report.name = Some(claims.suggested_name());
        report.email = claims
            .all
            .get("email")
            .and_then(|e| e.as_str())
            .map(str::to_string);
        report.email_verified = claims.verified_email().is_some();
        report.groups = claims.groups(&s.groups_claim);
        report.granted_scopes = granted.clone();

        if let Some(granted) = &granted {
            let missing: Vec<&str> = flow
                .scopes
                .split_whitespace()
                .filter(|s| !has_scope(granted, s))
                .collect();
            if !missing.is_empty() {
                report.notes.push(note(
                    Level::Warning,
                    format!("The provider didn't grant {}.", listing(&missing)),
                ));
            }
        }
        let no_matching = "so people can't be matched to existing users by email";
        if report.email.is_none() {
            report.notes.push(note(
                Level::Warning,
                match s.kind {
                    Kind::Github => format!(
                        "No email came back, {no_matching}. GitHub only sends a verified primary \
                         email, with `user:email` among the scopes."
                    ),
                    Kind::OAuth2 => format!(
                        "No email came back, {no_matching}. Compare the email field with the \
                         fields below."
                    ),
                    _ => format!("No email came back, {no_matching}. Is `email` among the scopes?"),
                },
            ));
        } else if !report.email_verified {
            report.notes.push(note(
                Level::Warning,
                if s.kind == Kind::OAuth2 && s.verified_field.is_none() {
                    format!("Without a verified field, emails count as unverified, {no_matching}.")
                } else {
                    format!("The email isn't marked verified, {no_matching}.")
                },
            ));
        }
        match (wants_groups(s), &report.groups) {
            (true, None) => report.notes.push(note(
                Level::Problem,
                format!(
                    "No `{}` claim came back, so the admin group and allowed groups can't work. \
                     Compare the groups claim with the claims below, or have the provider send \
                     groups.",
                    s.groups_claim
                ),
            )),
            (true, Some(groups)) if groups.is_empty() => report.notes.push(note(
                Level::Warning,
                "The groups claim came back empty: this account isn't in any group.",
            )),
            (true, Some(groups)) => report.notes.push(note(
                Level::Good,
                format!("Groups came back: {}.", listing(groups)),
            )),
            (false, Some(groups)) if !groups.is_empty() => report.notes.push(note(
                Level::Info,
                format!(
                    "Groups came back ({}). Set an admin group or allowed groups under Advanced \
                     to use them.",
                    listing(groups)
                ),
            )),
            _ => {}
        }

        let identity = identity(existing.unwrap_or(0), s, claims);
        match preview(pool, s, *existing, &identity).await {
            Ok(outcome) => report.outcome = Some(outcome),
            Err(e) => report.notes.push(note(Level::Problem, e.to_string())),
        }
        report
    }
}

/// What a real sign-in would do with this account, without doing it.
async fn preview(
    pool: &SqlitePool,
    s: &Settings,
    existing: Option<i64>,
    identity: &Identity,
) -> Result<String, SignInError> {
    let Ok(access) = access(s, identity) else {
        return Ok("They'd be turned away: they aren't in any of the allowed groups.".into());
    };
    if let Some(provider) = existing
        && let Some(user) = login_user(pool, provider, &identity.subject).await?
    {
        let user = accounts::user(pool, user).await?;
        if user.status == UserStatus::Disabled {
            return Ok(format!(
                "Linked to {}, who is turned off, so they'd be turned away.",
                user.name
            ));
        }
        let mut text = format!("Signs in as {}", user.name);
        match access.admin {
            Some(true) if !user.is_admin => text.push_str(", who becomes an admin"),
            Some(false) if user.is_admin => {
                text.push_str(", who stops being an admin unless they're the last one")
            }
            _ => {}
        }
        if user.status == UserStatus::Pending {
            if access.approved || access.admin == Some(true) {
                text.push_str(", and is let in");
            } else {
                text.push_str(", who is still waiting to be let in");
            }
        }
        text.push('.');
        return Ok(text);
    }
    if let Some(email) = &identity.email
        && let Some(user) = accounts::by_email(pool, email).await?
    {
        let linked = match existing {
            Some(provider) => has_link(pool, user.id, provider).await?,
            None => false,
        };
        if !linked {
            return Ok(format!(
                "Their email is {0}'s, so they'd be asked to prove they're {0} before the two \
                 are linked.",
                user.name
            ));
        }
    }
    let admin = access.admin.unwrap_or(false);
    let active = s.new_users == UserStatus::Active || access.approved || admin;
    Ok(format!(
        "A new user, {}, would be made{}, {}.",
        identity.name,
        if admin {
            " as an admin"
        } else {
            ""
        },
        if active {
            "who can watch right away"
        } else {
            "waiting for an admin to let them in"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(scopes: Option<&[&str]>, claims: Option<&[&str]>) -> Discovery {
        let list = |l: Option<&[&str]>| l.map(|l| l.iter().map(|s| s.to_string()).collect());
        Discovery {
            issuer: "https://id.example.com".into(),
            authorization_endpoint: "https://id.example.com/authorize".into(),
            token_endpoint: "https://id.example.com/token".into(),
            userinfo_endpoint: None,
            token_endpoint_auth_methods_supported: None,
            scopes_supported: list(scopes),
            claims_supported: list(claims),
        }
    }

    fn settings(scopes: &str, admin_group: Option<&str>) -> Settings {
        let mut s = Settings::new("Id", "https://id.example.com", "c", "s");
        s.scopes = scopes.into();
        s.admin_group = admin_group.map(str::to_string);
        s
    }

    fn levels(notes: &[Note]) -> Vec<Level> {
        notes.iter().map(|n| n.level).collect()
    }

    #[test]
    fn adds_groups_only_when_used_and_offered() {
        let offered = doc(Some(&["openid", "email", "groups"]), None);
        let plain = settings("openid email", None);
        let grouped = settings("openid email", Some("admins"));
        assert_eq!(scopes(&plain, &offered), "openid email");
        assert_eq!(scopes(&grouped, &offered), "openid email groups");
        assert_eq!(
            scopes(&grouped, &doc(Some(&["openid", "email"]), None)),
            "openid email"
        );
        assert_eq!(scopes(&grouped, &doc(None, None)), "openid email");
    }

    #[test]
    fn notes_unknown_scopes() {
        let d = doc(Some(&["openid", "profile", "email"]), None);
        let notes = scope_notes(&settings("openid profile email offline", None), &d);
        assert_eq!(levels(&notes), [Level::Warning]);
        assert!(notes[0].text.contains("`offline`"), "{notes:?}");

        let notes = scope_notes(&settings("openid email", None), &d);
        assert_eq!(levels(&notes), [Level::Good]);

        let notes = scope_notes(&settings("openid email", None), &doc(None, None));
        assert_eq!(levels(&notes), [Level::Info]);
    }

    #[test]
    fn notes_where_groups_come_from() {
        let s = settings("openid email", Some("admins"));
        let offered = doc(Some(&["openid", "email", "groups"]), None);
        assert_eq!(
            levels(&scope_notes(&s, &offered)),
            [Level::Good, Level::Info]
        );

        let claim_only = doc(Some(&["openid", "email"]), Some(&["sub", "groups"]));
        assert_eq!(levels(&scope_notes(&s, &claim_only)), [Level::Good]);

        let neither = doc(Some(&["openid", "email"]), Some(&["sub"]));
        assert_eq!(
            levels(&scope_notes(&s, &neither)),
            [Level::Good, Level::Warning]
        );
    }
}

//! Plain OAuth2 sign-in, for providers without OpenID Connect: GitHub,
//! Discord, or any with a user endpoint. Design: `docs/sign-in.md`.
//!
//! The round trip is the one in [`crate::oidc`] without the ID token: who
//! someone is comes from the user endpoint, fetched with the access token over
//! TLS, which is as much as the OIDC path trusts its tokens too. The answer is
//! turned into OIDC claims (`sub`, `preferred_username`, `name`, `email`,
//! `email_verified`), so everything after works the same.

use serde_json::Value;

use crate::oidc::{Claims, Discovery, OidcError};
use crate::sign_in::{Kind, Settings};

/// The endpoints for a provider that isn't OIDC, in the shape discovery
/// gives them. `None` for OIDC.
pub fn discovery(s: &Settings) -> Option<Discovery> {
    let (authorize, token, user, methods) = match s.kind {
        Kind::Oidc => return None,
        Kind::Github => (
            "https://github.com/login/oauth/authorize",
            "https://github.com/login/oauth/access_token",
            "https://api.github.com/user",
            Some(vec!["client_secret_post".to_string()]),
        ),
        Kind::Discord => (
            "https://discord.com/oauth2/authorize",
            "https://discord.com/api/oauth2/token",
            "https://discord.com/api/users/@me",
            None,
        ),
        Kind::OAuth2 => (
            s.authorize_url.as_deref().unwrap_or_default(),
            s.token_url.as_deref().unwrap_or_default(),
            s.userinfo_url.as_deref().unwrap_or_default(),
            None,
        ),
    };
    Some(Discovery {
        issuer: String::new(),
        authorization_endpoint: authorize.into(),
        token_endpoint: token.into(),
        userinfo_endpoint: Some(user.into()),
        token_endpoint_auth_methods_supported: methods,
        scopes_supported: None,
        claims_supported: None,
    })
}

async fn get(http: &reqwest::Client, url: &str, access_token: &str) -> Result<Value, OidcError> {
    let response = http
        .get(url)
        .bearer_auth(access_token)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(OidcError::Refused(format!(
            "{url} answered HTTP {}",
            response.status().as_u16()
        )));
    }
    response
        .json()
        .await
        .map_err(|e| OidcError::Invalid(format!("The answer from {url} can't be read: {e}")))
}

/// A dotted path into a JSON object: `data.user.id`.
fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |v, part| v.get(part))
}

fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => s == "true",
        _ => false,
    }
}

/// Who signed in, from the user endpoint.
pub async fn claims(
    http: &reqwest::Client,
    s: &Settings,
    doc: &Discovery,
    access_token: &str,
) -> Result<Claims, OidcError> {
    let url = doc.userinfo_endpoint.as_deref().unwrap_or_default();
    let user = get(http, url, access_token).await?;
    let mut all = match &user {
        Value::Object(map) => map.clone(),
        _ => {
            return Err(OidcError::Invalid(format!(
                "The answer from {url} isn't a JSON object."
            )));
        }
    };
    let id_field = s.id_field.as_deref().unwrap_or("id");
    let Some(sub) = text(at(&user, id_field)) else {
        return Err(OidcError::Invalid(format!(
            "The answer from {url} has no `{id_field}` to know the account by."
        )));
    };
    let mut set = |name: &str, value: Option<Value>| match value {
        Some(v) => {
            all.insert(name.into(), v);
        }
        None => {
            all.remove(name);
        }
    };
    match s.kind {
        Kind::Github => {
            set("preferred_username", user.get("login").cloned());
            // The profile's public email isn't necessarily verified; the
            // primary one from /user/emails says (with the `user:email` scope).
            let emails = get(http, "https://api.github.com/user/emails", access_token)
                .await
                .unwrap_or_default();
            let primary = emails.as_array().and_then(|list| {
                list.iter()
                    .find(|e| truthy(e.get("primary")) && truthy(e.get("verified")))
            });
            set("email", primary.and_then(|e| e.get("email")).cloned());
            set("email_verified", primary.map(|_| Value::Bool(true)));
        }
        Kind::Discord => {
            set("preferred_username", user.get("username").cloned());
            set("name", user.get("global_name").cloned());
            set(
                "email_verified",
                Some(Value::Bool(truthy(user.get("verified")))),
            );
        }
        Kind::OAuth2 => {
            if let Some(field) = &s.name_field {
                set("preferred_username", at(&user, field).cloned());
            } else if user.get("preferred_username").is_none() {
                let name = ["login", "username"].iter().find_map(|f| user.get(*f));
                set("preferred_username", name.cloned());
            }
            let email = at(&user, s.email_field.as_deref().unwrap_or("email")).cloned();
            set("email", email);
            // Without a field saying so, an email isn't trusted to match people.
            let verified = s
                .verified_field
                .as_deref()
                .is_some_and(|f| truthy(at(&user, f)));
            set("email_verified", Some(Value::Bool(verified)));
            if s.groups_claim.contains('.') {
                set(&s.groups_claim, at(&user, &s.groups_claim).cloned());
            }
        }
        Kind::Oidc => {}
    }
    all.insert("sub".into(), Value::String(sub.clone()));
    Ok(Claims { sub, all })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_dotted_paths() {
        let v = json!({ "data": { "id": 42, "name": " ana " }, "ok": "true" });
        assert_eq!(text(at(&v, "data.id")).as_deref(), Some("42"));
        assert_eq!(text(at(&v, "data.name")).as_deref(), Some("ana"));
        assert_eq!(text(at(&v, "data.missing")), None);
        assert!(truthy(at(&v, "ok")));
        assert!(!truthy(at(&v, "data")));
    }

    #[test]
    fn presets_have_endpoints() {
        let github = discovery(&Settings::oauth2(Kind::Github, "GitHub", "c", "s")).unwrap();
        assert_eq!(
            github.token_endpoint_auth_methods_supported.as_deref(),
            Some(&["client_secret_post".to_string()][..])
        );
        assert!(discovery(&Settings::new("Id", "https://id.example.com", "c", "s")).is_none());
    }
}

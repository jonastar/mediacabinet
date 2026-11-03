//! OpenID Connect: the authorization code flow with PKCE, by hand. Design:
//! `docs/sign-in.md`.
//!
//! ID tokens are only ever taken from the token endpoint, over TLS, in exchange
//! for a code and our client secret. OIDC Core §3.1.3.7 lets a client trust
//! such a token on the strength of the TLS connection instead of its signature,
//! so there are no signing keys here. Everything else is checked: issuer,
//! audience, expiry and nonce. Anything that would take a token through the
//! browser needs signature checking first.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// How far the provider's clock may be from ours.
const CLOCK_SKEW: i64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum OidcError {
    /// Our setup is wrong: an insecure URL, an issuer that doesn't match.
    #[error("{0}")]
    Config(String),
    #[error("Can't reach the provider: {0}")]
    Unreachable(String),
    /// The provider said no, e.g. a code that was already used.
    #[error("The provider refused: {0}")]
    Refused(String),
    /// A response that fails the checks.
    #[error("{0}")]
    Invalid(String),
}

impl From<reqwest::Error> for OidcError {
    fn from(err: reqwest::Error) -> Self {
        OidcError::Unreachable(err.without_url().to_string())
    }
}

/// The parts of a provider's discovery document we use.
#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: Option<String>,
    pub token_endpoint_auth_methods_supported: Option<Vec<String>>,
    pub scopes_supported: Option<Vec<String>>,
    pub claims_supported: Option<Vec<String>>,
}

/// HTTPS, or plain HTTP to this machine (for development). Without TLS, an ID
/// token taken on trust could be forged by anyone on the network.
pub fn is_secure(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    match url.scheme() {
        "https" => true,
        "http" => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ),
        _ => false,
    }
}

fn require_secure(what: &str, url: &str) -> Result<(), OidcError> {
    if is_secure(url) {
        return Ok(());
    }
    Err(OidcError::Config(format!(
        "The {what} must be an https:// address: {url}"
    )))
}

/// Issuers are compared exactly, apart from a trailing slash people often
/// leave off when typing one in.
pub fn same_issuer(a: &str, b: &str) -> bool {
    a.trim_end_matches('/') == b.trim_end_matches('/')
}

/// A client for talking to providers: short timeouts, and redirects only to
/// secure addresses.
pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        // GitHub's API refuses requests without one.
        .user_agent(concat!("media-cabinet/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 5 || !is_secure(attempt.url().as_str()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .expect("the HTTP client builds")
}

/// Fetches and checks the provider's discovery document.
pub async fn discover(http: &reqwest::Client, issuer: &str) -> Result<Discovery, OidcError> {
    require_secure("issuer URL", issuer)?;
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let response = http.get(&url).send().await?;
    if !response.status().is_success() {
        return Err(OidcError::Config(format!(
            "No OpenID configuration at {url} (HTTP {}). Check the issuer URL.",
            response.status().as_u16()
        )));
    }
    let doc: Discovery = response.json().await.map_err(|e| {
        OidcError::Invalid(format!(
            "The OpenID configuration at {url} can't be read: {e}"
        ))
    })?;
    if !same_issuer(&doc.issuer, issuer) {
        return Err(OidcError::Config(format!(
            "The provider calls itself {}, not {issuer}. Use that as the issuer URL.",
            doc.issuer
        )));
    }
    require_secure("authorization endpoint", &doc.authorization_endpoint)?;
    require_secure("token endpoint", &doc.token_endpoint)?;
    if let Some(userinfo) = &doc.userinfo_endpoint {
        require_secure("userinfo endpoint", userinfo)?;
    }
    Ok(doc)
}

/// 32 random bytes, URL-safe: for `state`, `nonce` and the PKCE verifier.
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The S256 PKCE challenge for a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Where to send the browser to sign in.
pub struct AuthRequest<'a> {
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub scopes: &'a str,
    pub state: &'a str,
    /// OIDC only.
    pub nonce: Option<&'a str>,
    pub verifier: &'a str,
    /// Have the provider post the browser back instead of redirecting
    /// (Apple, when asked for the email or name).
    pub form_post: bool,
}

pub fn authorize_url(doc: &Discovery, req: &AuthRequest) -> Result<String, OidcError> {
    let mut url = Url::parse(&doc.authorization_endpoint)
        .map_err(|e| OidcError::Invalid(format!("The authorization endpoint isn't a URL: {e}")))?;
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", req.client_id)
        .append_pair("redirect_uri", req.redirect_uri)
        .append_pair("state", req.state)
        .append_pair("code_challenge", &pkce_challenge(req.verifier))
        .append_pair("code_challenge_method", "S256");
    // Plain OAuth2 providers may have no scopes; an empty one can be refused.
    if !req.scopes.is_empty() {
        url.query_pairs_mut().append_pair("scope", req.scopes);
    }
    if let Some(nonce) = req.nonce {
        url.query_pairs_mut().append_pair("nonce", nonce);
    }
    if req.form_post {
        url.query_pairs_mut()
            .append_pair("response_mode", "form_post");
    }
    Ok(url.into())
}

/// What the token endpoint hands back. Refresh tokens are ignored.
pub struct Tokens {
    /// OIDC only.
    pub id_token: Option<String>,
    pub access_token: Option<String>,
    /// The scopes granted, if the provider says (when they differ from those
    /// asked for, it must).
    pub scope: Option<String>,
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    access_token: Option<String>,
    scope: Option<String>,
}

#[derive(Deserialize)]
struct ErrorResponse {
    error: String,
    error_description: Option<String>,
}

/// RFC 6749 §2.3.1: the client ID and secret are form-encoded before going
/// into the Basic header.
fn form_encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Exchanges the code from the redirect for tokens, authenticating with the
/// client secret.
/// Posts to the token endpoint, authenticating with the client secret the
/// way the provider prefers. Returns the status and body.
async fn token_request(
    http: &reqwest::Client,
    doc: &Discovery,
    client_id: &str,
    client_secret: &str,
    mut form: Vec<(&str, &str)>,
) -> Result<(reqwest::StatusCode, String), OidcError> {
    let methods = doc.token_endpoint_auth_methods_supported.as_deref();
    let basic = methods.is_none_or(|m| m.iter().any(|m| m == "client_secret_basic"));
    let post = methods.is_some_and(|m| m.iter().any(|m| m == "client_secret_post"));
    let mut request = http.post(&doc.token_endpoint);
    if basic {
        request = request.basic_auth(form_encode(client_id), Some(form_encode(client_secret)));
    } else if post {
        form.push(("client_id", client_id));
        form.push(("client_secret", client_secret));
    } else {
        return Err(OidcError::Config(
            "The provider wants a client authentication method other than a client secret.".into(),
        ));
    }
    let response = request
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&form)
        .send()
        .await?;
    let status = response.status();
    Ok((status, response.text().await?))
}

pub async fn exchange(
    http: &reqwest::Client,
    doc: &Discovery,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<Tokens, OidcError> {
    let form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    let (status, body) = token_request(http, doc, client_id, client_secret, form).await?;
    // GitHub answers errors with 200.
    let error = serde_json::from_str::<ErrorResponse>(&body);
    if !status.is_success() || error.is_ok() {
        return Err(match error {
            Ok(e) => OidcError::Refused(match e.error_description {
                Some(d) => format!("{} ({d})", e.error),
                None => e.error,
            }),
            Err(_) => OidcError::Refused(format!("HTTP {}", status.as_u16())),
        });
    }
    let tokens: TokenResponse = serde_json::from_str(&body)
        .map_err(|e| OidcError::Invalid(format!("The token response can't be read: {e}")))?;
    Ok(Tokens {
        id_token: tokens.id_token,
        access_token: tokens.access_token,
        scope: tokens.scope,
    })
}

/// What the token endpoint made of our client ID and secret.
#[derive(Debug, PartialEq)]
pub enum ClientCheck {
    Accepted,
    Rejected(String),
    /// An answer that doesn't say either way.
    Unclear(String),
}

/// Checks the client ID and secret without anyone signing in, by trading a
/// made-up code. RFC 6749 §5.2 has the provider authenticate the client
/// first: `invalid_client` means the credentials are wrong, `invalid_grant`
/// means only the code was. GitHub says `incorrect_client_credentials` and
/// `bad_verification_code` instead.
pub async fn check_client(
    http: &reqwest::Client,
    doc: &Discovery,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
) -> Result<ClientCheck, OidcError> {
    let code = random_token();
    let verifier = random_token();
    let form = vec![
        ("grant_type", "authorization_code"),
        ("code", code.as_str()),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier.as_str()),
    ];
    let (status, body) = token_request(http, doc, client_id, client_secret, form).await?;
    let said = |e: &ErrorResponse| match &e.error_description {
        Some(d) => format!("{} ({d})", e.error),
        None => e.error.clone(),
    };
    Ok(match serde_json::from_str::<ErrorResponse>(&body) {
        Ok(e) if matches!(e.error.as_str(), "invalid_grant" | "bad_verification_code") => {
            ClientCheck::Accepted
        }
        Ok(e)
            if matches!(
                e.error.as_str(),
                "invalid_client" | "unauthorized_client" | "incorrect_client_credentials"
            ) =>
        {
            ClientCheck::Rejected(said(&e))
        }
        Ok(e) => ClientCheck::Unclear(said(&e)),
        Err(_) if status == reqwest::StatusCode::UNAUTHORIZED => {
            ClientCheck::Rejected("HTTP 401".into())
        }
        Err(_) => ClientCheck::Unclear(format!("HTTP {}", status.as_u16())),
    })
}

#[derive(Debug, Clone)]
pub struct Claims {
    pub sub: String,
    pub all: Map<String, Value>,
}

impl Claims {
    fn text(&self, name: &str) -> Option<String> {
        self.all
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    /// The email address, only if the provider says it's verified.
    pub fn verified_email(&self) -> Option<String> {
        let verified = match self.all.get("email_verified") {
            Some(Value::Bool(v)) => *v,
            // Some providers send it as a string.
            Some(Value::String(v)) => v == "true",
            _ => false,
        };
        self.text("email").filter(|_| verified)
    }

    /// A name to start from for a new account.
    pub fn suggested_name(&self) -> String {
        self.text("preferred_username")
            .or_else(|| self.text("name"))
            .or_else(|| {
                self.text("email")
                    .and_then(|e| e.split('@').next().map(str::to_string))
            })
            .unwrap_or_else(|| "user".into())
    }

    /// The groups in `claim`: a list, or a single name.
    pub fn groups(&self, claim: &str) -> Option<Vec<String>> {
        match self.all.get(claim)? {
            Value::Array(items) => Some(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
            ),
            Value::String(one) => Some(vec![one.clone()]),
            // Zitadel's roles: `{ "role": { org: domain } }`.
            Value::Object(map) => Some(map.keys().cloned().collect()),
            _ => None,
        }
    }

    /// Adds claims from userinfo that the ID token didn't have.
    pub fn merge(&mut self, more: Map<String, Value>) {
        for (k, v) in more {
            self.all.entry(k).or_insert(v);
        }
    }
}

fn decode_part(part: &str) -> Result<Map<String, Value>, OidcError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(part.trim_end_matches('='))
        .map_err(|_| OidcError::Invalid("The ID token isn't valid base64.".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| OidcError::Invalid("The ID token's claims aren't a JSON object.".into()))
}

/// What an ID token must say to be accepted.
pub struct Expected<'a> {
    pub issuer: &'a str,
    pub client_id: &'a str,
    pub nonce: &'a str,
    pub now: i64,
}

/// Reads an ID token from the token endpoint and checks its claims. The
/// signature isn't checked: see the module docs for why that's sound here.
pub fn check_id_token(id_token: &str, expected: &Expected) -> Result<Claims, OidcError> {
    let mut parts = id_token.split('.');
    let (Some(_header), Some(payload), Some(_signature), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(OidcError::Invalid("The ID token isn't a JWT.".into()));
    };
    let all = decode_part(payload)?;
    let invalid = |why: &str| Err(OidcError::Invalid(format!("The ID token {why}.")));

    if all.get("iss").and_then(Value::as_str) != Some(expected.issuer) {
        return invalid("is from a different issuer");
    }
    let audiences: Vec<&str> = match all.get("aud") {
        Some(Value::String(one)) => vec![one.as_str()],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    };
    if !audiences.contains(&expected.client_id) {
        return invalid("is for a different client");
    }
    match all.get("azp").and_then(Value::as_str) {
        Some(azp) if azp != expected.client_id => return invalid("is for a different client"),
        None if audiences.len() > 1 => return invalid("has several audiences and no azp"),
        _ => {}
    }
    match all.get("exp").and_then(Value::as_i64) {
        Some(exp) if exp + CLOCK_SKEW > expected.now => {}
        _ => return invalid("has expired; check this server's clock"),
    }
    if let Some(iat) = all.get("iat").and_then(Value::as_i64)
        && iat - CLOCK_SKEW > expected.now
    {
        return invalid("was issued in the future; check this server's clock");
    }
    if all.get("nonce").and_then(Value::as_str) != Some(expected.nonce) {
        return invalid("doesn't match this sign-in");
    }
    let Some(sub) = all
        .get("sub")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
    else {
        return invalid("has no subject");
    };
    Ok(Claims {
        sub: sub.to_string(),
        all,
    })
}

/// Claims from the userinfo endpoint. Its `sub` must be the ID token's.
pub async fn userinfo(
    http: &reqwest::Client,
    doc: &Discovery,
    access_token: &str,
    sub: &str,
) -> Result<Map<String, Value>, OidcError> {
    let Some(endpoint) = &doc.userinfo_endpoint else {
        return Ok(Map::new());
    };
    let response = http.get(endpoint).bearer_auth(access_token).send().await?;
    if !response.status().is_success() {
        return Err(OidcError::Refused(format!(
            "userinfo answered HTTP {}",
            response.status().as_u16()
        )));
    }
    let body = response.text().await?;
    // Usually JSON; a signed JWT if the client is set up that way.
    let claims = match serde_json::from_str::<Map<String, Value>>(&body) {
        Ok(claims) => claims,
        Err(_) => match body.trim().split('.').nth(1) {
            Some(payload) => decode_part(payload)?,
            None => {
                return Err(OidcError::Invalid(
                    "The userinfo response can't be read.".into(),
                ));
            }
        },
    };
    if claims.get("sub").and_then(Value::as_str) != Some(sub) {
        return Err(OidcError::Invalid(
            "The userinfo response is about someone else.".into(),
        ));
    }
    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn token(claims: Value) -> String {
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
        let body = URL_SAFE_NO_PAD.encode(claims.to_string());
        format!("{header}.{body}.signature")
    }

    const EXPECTED: Expected = Expected {
        issuer: "https://auth.example.com",
        client_id: "cabinet",
        nonce: "n0nce",
        now: 1_000_000,
    };

    fn good() -> Value {
        json!({
            "iss": "https://auth.example.com",
            "aud": "cabinet",
            "sub": "abc",
            "exp": 1_000_300,
            "iat": 999_990,
            "nonce": "n0nce",
            "email": "sam@example.com",
            "email_verified": true,
            "preferred_username": "sam",
        })
    }

    fn with(change: impl FnOnce(&mut Map<String, Value>)) -> Value {
        let mut claims = good();
        change(claims.as_object_mut().unwrap());
        claims
    }

    #[test]
    fn accepts_a_good_token() {
        let claims = check_id_token(&token(good()), &EXPECTED).unwrap();
        assert_eq!(claims.sub, "abc");
        assert_eq!(claims.verified_email().as_deref(), Some("sam@example.com"));
        assert_eq!(claims.suggested_name(), "sam");
    }

    #[test]
    fn refuses_bad_tokens() {
        let bad = [
            with(|c| {
                c.insert("iss".into(), json!("https://evil.example.com"));
            }),
            with(|c| {
                c.insert("aud".into(), json!("someone-else"));
            }),
            with(|c| {
                c.insert("aud".into(), json!(["cabinet", "other"]));
            }),
            with(|c| {
                c.insert("azp".into(), json!("other"));
            }),
            with(|c| {
                c.insert("exp".into(), json!(999_000));
            }),
            with(|c| {
                c.remove("exp");
            }),
            with(|c| {
                c.insert("iat".into(), json!(1_000_500));
            }),
            with(|c| {
                c.insert("nonce".into(), json!("replayed"));
            }),
            with(|c| {
                c.remove("nonce");
            }),
            with(|c| {
                c.insert("sub".into(), json!(""));
            }),
        ];
        for claims in bad {
            assert!(
                check_id_token(&token(claims.clone()), &EXPECTED).is_err(),
                "accepted {claims}"
            );
        }
        assert!(check_id_token("not.a-jwt", &EXPECTED).is_err());
    }

    #[test]
    fn several_audiences_need_azp() {
        let claims = with(|c| {
            c.insert("aud".into(), json!(["cabinet", "other"]));
            c.insert("azp".into(), json!("cabinet"));
        });
        assert!(check_id_token(&token(claims), &EXPECTED).is_ok());
    }

    #[test]
    fn unverified_email_is_ignored() {
        let claims = with(|c| {
            c.insert("email_verified".into(), json!(false));
        });
        let claims = check_id_token(&token(claims), &EXPECTED).unwrap();
        assert_eq!(claims.verified_email(), None);
        let claims = with(|c| {
            c.insert("email_verified".into(), json!("true"));
        });
        let claims = check_id_token(&token(claims), &EXPECTED).unwrap();
        assert!(claims.verified_email().is_some());
    }

    #[test]
    fn groups_as_list_or_one() {
        let claims = with(|c| {
            c.insert("groups".into(), json!(["a", "b"]));
            c.insert("role".into(), json!("admin"));
        });
        let claims = check_id_token(&token(claims), &EXPECTED).unwrap();
        assert_eq!(claims.groups("groups").unwrap(), ["a", "b"]);
        assert_eq!(claims.groups("role").unwrap(), ["admin"]);
        assert_eq!(claims.groups("missing"), None);
    }

    #[test]
    fn only_secure_urls() {
        assert!(is_secure("https://auth.example.com"));
        assert!(is_secure("http://localhost:9000/application/o/x/"));
        assert!(is_secure("http://127.0.0.1:9000"));
        assert!(!is_secure("http://auth.lan:9000"));
        assert!(!is_secure("http://192.168.1.2"));
        assert!(!is_secure("ftp://auth.example.com"));
        assert!(!is_secure("not a url"));
    }

    #[test]
    fn pkce_matches_the_rfc_example() {
        // RFC 7636, appendix B.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn client_credentials_are_form_encoded() {
        assert_eq!(form_encode("abc-1.2_3~"), "abc-1.2_3~");
        assert_eq!(form_encode("a b:c%"), "a%20b%3Ac%25");
    }
}

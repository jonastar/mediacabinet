//! Sign in with Apple: OpenID Connect, except that the client secret is a
//! short-lived JWT signed with a private key from Apple's developer site, and
//! that Apple posts people back (`response_mode=form_post`) when asked for
//! their email or name. Design: `docs/sign-in.md`.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use ring::rand::SystemRandom;
use ring::signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair};
use serde_json::json;

use crate::oidc::OidcError;

pub const ISSUER: &str = "https://appleid.apple.com";

/// How long a client secret is good for; a new one is signed for each request.
const SECRET_SECONDS: i64 = 5 * 60;

/// The key's bytes from the `.p8` file's PEM.
fn der(private_key: &str) -> Option<Vec<u8>> {
    let body: String = private_key
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("-----"))
        .collect();
    STANDARD.decode(body).ok()
}

/// The client secret for one request: an ES256 JWT from the Team ID, the
/// key's ID and the Services ID (the client ID).
pub fn client_secret(
    team_id: &str,
    key_id: &str,
    client_id: &str,
    private_key: &str,
    now: i64,
) -> Result<String, OidcError> {
    let not_a_key = || {
        OidcError::Config(
            "The private key isn't a Sign in with Apple key; paste the whole .p8 file.".into(),
        )
    };
    let rng = SystemRandom::new();
    let der = der(private_key).ok_or_else(not_a_key)?;
    let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &der, &rng)
        .map_err(|_| not_a_key())?;
    let part = |v: serde_json::Value| URL_SAFE_NO_PAD.encode(v.to_string());
    let input = format!(
        "{}.{}",
        part(json!({ "alg": "ES256", "kid": key_id })),
        part(json!({
            "iss": team_id,
            "iat": now,
            "exp": now + SECRET_SECONDS,
            "aud": ISSUER,
            "sub": client_id,
        }))
    );
    let signature = key
        .sign(&rng, input.as_bytes())
        .map_err(|_| OidcError::Config("Can't sign with the private key.".into()))?;
    Ok(format!(
        "{input}.{}",
        URL_SAFE_NO_PAD.encode(signature.as_ref())
    ))
}

/// The name in the `user` field Apple posts on someone's first sign-in:
/// `{"name": {"firstName": …, "lastName": …}}`.
pub fn name_from_user(user: &str) -> Option<String> {
    let user: serde_json::Value = serde_json::from_str(user).ok()?;
    let name = user.get("name")?;
    let part = |k: &str| {
        name.get(k)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|v| !v.is_empty())
    };
    let full = [part("firstName"), part("lastName")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    (!full.is_empty()).then_some(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_FIXED, KeyPair, UnparsedPublicKey};

    fn pem(der: &[u8]) -> String {
        let body = STANDARD.encode(der);
        let lines: Vec<&str> = body
            .as_bytes()
            .chunks(64)
            .map(|c| std::str::from_utf8(c).unwrap())
            .collect();
        format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            lines.join("\n")
        )
    }

    #[test]
    fn signs_a_client_secret() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let key = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let secret = client_secret(
            "TEAM123456",
            "KEY1234567",
            "com.example.media",
            &pem(pkcs8.as_ref()),
            1000,
        )
        .unwrap();

        let (input, signature) = secret.rsplit_once('.').unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, key.public_key().as_ref())
            .verify(
                input.as_bytes(),
                &URL_SAFE_NO_PAD.decode(signature).unwrap(),
            )
            .expect("a valid ES256 signature");
        let (header, claims) = input.split_once('.').unwrap();
        let decode = |p: &str| -> serde_json::Value {
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(p).unwrap()).unwrap()
        };
        assert_eq!(
            decode(header),
            json!({ "alg": "ES256", "kid": "KEY1234567" })
        );
        assert_eq!(
            decode(claims),
            json!({ "iss": "TEAM123456", "iat": 1000, "exp": 1300,
                    "aud": ISSUER, "sub": "com.example.media" })
        );
    }

    #[test]
    fn refuses_other_keys() {
        assert!(client_secret("T", "K", "c", "not a key", 0).is_err());
        assert!(client_secret("T", "K", "c", &pem(b"nonsense"), 0).is_err());
    }

    #[test]
    fn reads_the_name() {
        let user = r#"{"name":{"firstName":"Ana","lastName":"Lima"},"email":"a@b.c"}"#;
        assert_eq!(name_from_user(user).as_deref(), Some("Ana Lima"));
        assert_eq!(name_from_user(r#"{"email":"a@b.c"}"#), None);
        assert_eq!(name_from_user("nope"), None);
    }
}

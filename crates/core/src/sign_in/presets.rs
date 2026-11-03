//! Providers the form knows by name. The web app has their setup steps and
//! fields; this has what the server needs too, for checking settings and for
//! `CABINET_OIDC_{KEY}_TYPE`: the type, a fixed or default issuer, default
//! scopes, and whether groups are used. Design: `docs/sign-in.md`.

use super::providers::Kind;
use crate::apple;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Issuer {
    /// Entered (or built from parts in the form).
    Asked,
    Fixed(&'static str),
    /// Unless another is entered: a self-hosted GitLab.
    Default(&'static str),
}

#[derive(Debug)]
pub struct Preset {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub issuer: Issuer,
    /// Instead of the type's default scopes.
    pub scopes: Option<&'static str>,
    /// Whether the provider can send groups.
    pub groups: bool,
}

const fn oidc(key: &'static str, label: &'static str, issuer: Issuer, groups: bool) -> Preset {
    Preset {
        key,
        label,
        kind: Kind::Oidc,
        issuer,
        scopes: None,
        groups,
    }
}

pub const PRESETS: &[Preset] = &[
    oidc("authentik", "Authentik", Issuer::Asked, true),
    oidc("authelia", "Authelia", Issuer::Asked, true),
    oidc("keycloak", "Keycloak", Issuer::Asked, true),
    oidc("pocket-id", "Pocket ID", Issuer::Asked, true),
    oidc("kanidm", "Kanidm", Issuer::Asked, true),
    oidc("zitadel", "Zitadel", Issuer::Asked, true),
    oidc(
        "google",
        "Google",
        Issuer::Fixed("https://accounts.google.com"),
        false,
    ),
    oidc("microsoft", "Microsoft", Issuer::Asked, true),
    oidc(
        "gitlab",
        "GitLab",
        Issuer::Default("https://gitlab.com"),
        true,
    ),
    Preset {
        key: "apple",
        label: "Apple",
        kind: Kind::Oidc,
        issuer: Issuer::Fixed(apple::ISSUER),
        scopes: Some("openid email name"),
        groups: false,
    },
    Preset {
        key: "github",
        label: "GitHub",
        kind: Kind::Github,
        issuer: Issuer::Asked,
        scopes: None,
        groups: false,
    },
    Preset {
        key: "discord",
        label: "Discord",
        kind: Kind::Discord,
        issuer: Issuer::Asked,
        scopes: None,
        groups: false,
    },
];

pub fn find(key: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.key == key)
}

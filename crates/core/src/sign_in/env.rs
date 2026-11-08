//! Providers from environment variables: `CABINET_OIDC_{KEY}_{SETTING}`.
//!
//! Settings are a fixed list matched from the end, so a key can contain
//! underscores: `CABINET_OIDC_POCKET_ID_ISSUER` is the key `pocket-id`. A
//! provider needs `CLIENT_ID` and `CLIENT_SECRET`, and `ISSUER` unless `TYPE`
//! names a provider with a fixed one (`google`, `apple`, `github`…) or is
//! `oauth2` (which needs the three URLs); the rest have the UI's defaults.

use sqlx::types::Json;

use super::presets;
use super::providers::{Kind, Settings};
use crate::accounts::UserStatus;

const PREFIX: &str = "CABINET_OIDC_";

/// Longest first, so `_CLIENT_SECRET_FILE` isn't read as `_FILE`.
const SETTINGS: &[&str] = &[
    "CLIENT_SECRET_FILE",
    "VERIFIED_FIELD",
    "TEAM_ID",
    "KEY_ID",
    "ALLOWED_GROUPS",
    "AUTHORIZE_URL",
    "CLIENT_SECRET",
    "GROUPS_CLAIM",
    "REDIRECT_URL",
    "USERINFO_URL",
    "EMAIL_FIELD",
    "ADMIN_GROUP",
    "NAME_FIELD",
    "TOKEN_URL",
    "NEW_USERS",
    "CLIENT_ID",
    "ID_FIELD",
    "ENABLED",
    "ISSUER",
    "SCOPES",
    "TYPE",
    "NAME",
];

/// A provider described by environment variables.
#[derive(Debug, Clone)]
pub struct EnvProvider {
    pub key: String,
    pub settings: Settings,
    /// The settings given, in lower case (`client_secret` for either secret
    /// variable), for showing which fields the environment controls.
    pub fields: Vec<&'static str>,
}

/// `CABINET_OIDC_POCKET_ID_` for the key `pocket-id`.
pub fn prefix(key: &str) -> String {
    format!("{PREFIX}{}_", key.replace('-', "_").to_uppercase())
}

fn field(setting: &str) -> &'static str {
    match setting {
        "CLIENT_SECRET_FILE" | "CLIENT_SECRET" => "client_secret",
        "ALLOWED_GROUPS" => "allowed_groups",
        "GROUPS_CLAIM" => "groups_claim",
        "REDIRECT_URL" => "redirect_url",
        "ADMIN_GROUP" => "admin_group",
        "NEW_USERS" => "new_users",
        "CLIENT_ID" => "client_id",
        "ENABLED" => "enabled",
        "ISSUER" => "issuer",
        "SCOPES" => "scopes",
        "TYPE" => "kind",
        "AUTHORIZE_URL" => "authorize_url",
        "TOKEN_URL" => "token_url",
        "USERINFO_URL" => "userinfo_url",
        "ID_FIELD" => "id_field",
        "NAME_FIELD" => "name_field",
        "EMAIL_FIELD" => "email_field",
        "VERIFIED_FIELD" => "verified_field",
        "TEAM_ID" => "team_id",
        "KEY_ID" => "key_id",
        _ => "name",
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// `CABINET_PASSWORD_SIGN_IN`, if set.
pub fn password_sign_in(vars: &[(String, String)]) -> Result<Option<bool>, String> {
    switch(vars, "CABINET_PASSWORD_SIGN_IN")
}

/// `CABINET_GUEST_ACCESS`, if set.
pub fn guest_access(vars: &[(String, String)]) -> Result<Option<bool>, String> {
    switch(vars, "CABINET_GUEST_ACCESS")
}

fn switch(vars: &[(String, String)], name: &str) -> Result<Option<bool>, String> {
    match vars.iter().find(|(k, _)| k == name) {
        Some((_, v)) => parse_bool(v)
            .map(Some)
            .ok_or_else(|| format!("{name} must be true or false")),
        None => Ok(None),
    }
}

/// "Pocket Id" for the key `pocket-id`.
fn title_case(key: &str) -> String {
    key.split('-')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reads providers from the environment. Providers with a missing or invalid
/// setting are left out, with a problem to log; the server still starts.
pub fn parse(vars: &[(String, String)]) -> (Vec<EnvProvider>, Vec<String>) {
    let mut found: Vec<(String, Vec<(&'static str, String)>)> = vec![];
    for (name, value) in vars {
        let Some(rest) = name.strip_prefix(PREFIX) else {
            continue;
        };
        let Some((key, setting)) = SETTINGS.iter().find_map(|s| {
            rest.strip_suffix(s)
                .and_then(|k| k.strip_suffix('_'))
                .filter(|k| !k.is_empty())
                .map(|k| (k, *s))
        }) else {
            continue;
        };
        let key = key.to_lowercase().replace('_', "-");
        match found.iter_mut().find(|(k, _)| *k == key) {
            Some((_, values)) => values.push((setting, value.clone())),
            None => found.push((key, vec![(setting, value.clone())])),
        }
    }

    let mut providers = vec![];
    let mut problems = vec![];
    for (key, values) in found {
        let vars = format!("{}*", prefix(&key));
        match provider(&key, &values) {
            Ok(p) => providers.push(p),
            Err(why) => problems.push(format!("{vars}: {why}; this provider is left out")),
        }
    }
    providers.sort_by(|a, b| a.key.cmp(&b.key));
    (providers, problems)
}

fn provider(key: &str, values: &[(&'static str, String)]) -> Result<EnvProvider, String> {
    super::providers::check_key(key).map_err(|e| e.to_string())?;
    let get = |setting: &str| {
        values
            .iter()
            .find(|(s, _)| *s == setting)
            .map(|(_, v)| v.trim().to_string())
    };
    let (kind, preset) = match get("TYPE").map(|t| t.to_ascii_lowercase()).as_deref() {
        None | Some("oidc") => (Kind::Oidc, None),
        Some("oauth2") => (Kind::OAuth2, None),
        Some(other) => match presets::find(other) {
            Some(p) => (p.kind, Some(p)),
            None => {
                let names: Vec<&str> = presets::PRESETS.iter().map(|p| p.key).collect();
                return Err(format!(
                    "TYPE must be oidc, oauth2 or one of {}",
                    names.join(", ")
                ));
            }
        },
    };
    let client_id = get("CLIENT_ID").ok_or("CLIENT_ID is missing")?;
    let secret = match (get("CLIENT_SECRET"), get("CLIENT_SECRET_FILE")) {
        (Some(secret), _) => secret,
        (None, Some(path)) => std::fs::read_to_string(&path)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("can't read CLIENT_SECRET_FILE {path}: {e}"))?,
        (None, None) => return Err("CLIENT_SECRET or CLIENT_SECRET_FILE is missing".into()),
    };
    let name = get("NAME").unwrap_or_else(|| match preset {
        Some(p) => p.label.to_string(),
        None => title_case(key),
    });
    let mut s = match kind {
        Kind::Oidc => {
            let asked = preset.is_none_or(|p| p.issuer == presets::Issuer::Asked);
            let issuer = match get("ISSUER") {
                Some(issuer) => issuer,
                None if asked => return Err("ISSUER is missing".into()),
                None => String::new(),
            };
            Settings::new(&name, &issuer, &client_id, &secret)
        }
        _ => Settings::oauth2(kind, &name, &client_id, &secret),
    };
    s.preset = preset.map(|p| p.key.to_string());
    s.team_id = get("TEAM_ID");
    s.key_id = get("KEY_ID");
    // Left empty, `checked` fills in the preset's or type's own.
    s.scopes = String::new();
    s.authorize_url = get("AUTHORIZE_URL");
    s.token_url = get("TOKEN_URL");
    s.userinfo_url = get("USERINFO_URL");
    s.id_field = get("ID_FIELD");
    s.name_field = get("NAME_FIELD");
    s.email_field = get("EMAIL_FIELD");
    s.verified_field = get("VERIFIED_FIELD");
    if let Some(v) = get("NEW_USERS") {
        s.new_users = match v.to_ascii_lowercase().as_str() {
            "pending" => UserStatus::Pending,
            "active" => UserStatus::Active,
            _ => return Err("NEW_USERS must be pending or active".into()),
        };
    }
    if let Some(v) = get("GROUPS_CLAIM") {
        s.groups_claim = v;
    }
    s.admin_group = get("ADMIN_GROUP");
    if let Some(v) = get("ALLOWED_GROUPS") {
        s.allowed_groups = Json(v.split(',').map(|g| g.trim().to_string()).collect());
    }
    if let Some(v) = get("SCOPES") {
        s.scopes = v;
    }
    s.redirect_url = get("REDIRECT_URL");
    if let Some(v) = get("ENABLED") {
        s.enabled = parse_bool(&v).ok_or("ENABLED must be true or false")?;
    }
    let settings = s.checked().map_err(|e| e.to_string())?;
    let mut fields: Vec<&'static str> = values.iter().map(|(s, _)| field(s)).collect();
    fields.sort_unstable();
    fields.dedup();
    Ok(EnvProvider {
        key: key.to_string(),
        settings,
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn reads_providers() {
        let (providers, problems) = parse(&vars(&[
            ("CABINET_OIDC_POCKET_ID_ISSUER", "https://id.example.com"),
            ("CABINET_OIDC_POCKET_ID_CLIENT_ID", "cabinet"),
            ("CABINET_OIDC_POCKET_ID_CLIENT_SECRET", "s3cret"),
            ("CABINET_OIDC_POCKET_ID_ADMIN_GROUP", "media-admins"),
            ("CABINET_OIDC_POCKET_ID_ALLOWED_GROUPS", "family, friends"),
            ("CABINET_OIDC_POCKET_ID_NEW_USERS", "active"),
            ("CABINET_TMDB_KEY", "unrelated"),
        ]));
        assert!(problems.is_empty(), "{problems:?}");
        let [p] = providers.as_slice() else {
            panic!("one provider: {providers:?}");
        };
        assert_eq!(p.key, "pocket-id");
        assert_eq!(p.settings.name, "Pocket Id");
        assert_eq!(p.settings.client_secret, "s3cret");
        assert_eq!(p.settings.admin_group.as_deref(), Some("media-admins"));
        assert_eq!(p.settings.allowed_groups.0, ["family", "friends"]);
        assert_eq!(p.settings.new_users, UserStatus::Active);
        assert_eq!(p.settings.scopes, "openid profile email");
        assert_eq!(
            p.fields,
            [
                "admin_group",
                "allowed_groups",
                "client_id",
                "client_secret",
                "issuer",
                "new_users"
            ]
        );
        assert_eq!(prefix(&p.key), "CABINET_OIDC_POCKET_ID_");
    }

    #[test]
    fn reads_the_secret_from_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        std::fs::write(&path, "from-file\n").unwrap();
        let (providers, problems) = parse(&vars(&[
            (
                "CABINET_OIDC_AUTHENTIK_ISSUER",
                "https://auth.example.com/o/x/",
            ),
            ("CABINET_OIDC_AUTHENTIK_CLIENT_ID", "cabinet"),
            (
                "CABINET_OIDC_AUTHENTIK_CLIENT_SECRET_FILE",
                path.to_str().unwrap(),
            ),
        ]));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(providers[0].settings.client_secret, "from-file");
        assert_eq!(providers[0].settings.name, "Authentik");
    }

    #[test]
    fn leaves_out_broken_providers() {
        let (providers, problems) = parse(&vars(&[
            ("CABINET_OIDC_A_ISSUER", "https://a.example.com"),
            ("CABINET_OIDC_B_ISSUER", "http://b.lan"),
            ("CABINET_OIDC_B_CLIENT_ID", "x"),
            ("CABINET_OIDC_B_CLIENT_SECRET", "y"),
            ("CABINET_OIDC_C_ISSUER", "https://c.example.com"),
            ("CABINET_OIDC_C_CLIENT_ID", "x"),
            ("CABINET_OIDC_C_CLIENT_SECRET", "y"),
            ("CABINET_OIDC_C_ENABLED", "maybe"),
        ]));
        assert!(providers.is_empty(), "{providers:?}");
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("CLIENT_ID is missing"));
    }

    #[test]
    fn reads_oauth2_providers() {
        let (providers, problems) = parse(&vars(&[
            ("CABINET_OIDC_GITHUB_TYPE", "github"),
            ("CABINET_OIDC_GITHUB_CLIENT_ID", "abc"),
            ("CABINET_OIDC_GITHUB_CLIENT_SECRET", "xyz"),
            ("CABINET_OIDC_FORUM_TYPE", "oauth2"),
            ("CABINET_OIDC_FORUM_CLIENT_ID", "abc"),
            ("CABINET_OIDC_FORUM_CLIENT_SECRET", "xyz"),
            (
                "CABINET_OIDC_FORUM_AUTHORIZE_URL",
                "https://forum.example.com/oauth/authorize",
            ),
            (
                "CABINET_OIDC_FORUM_TOKEN_URL",
                "https://forum.example.com/oauth/token",
            ),
            (
                "CABINET_OIDC_FORUM_USERINFO_URL",
                "https://forum.example.com/api/me",
            ),
            ("CABINET_OIDC_FORUM_ID_FIELD", "user.id"),
            ("CABINET_OIDC_DISCORD_TYPE", "discord"),
            ("CABINET_OIDC_DISCORD_CLIENT_ID", "abc"),
            ("CABINET_OIDC_DISCORD_CLIENT_SECRET", "xyz"),
            ("CABINET_OIDC_DISCORD_ADMIN_GROUP", "admins"),
        ]));
        let [forum, github] = providers.as_slice() else {
            panic!("two providers: {providers:?}");
        };
        assert_eq!(github.settings.kind, Kind::Github);
        assert_eq!(github.settings.name, "GitHub");
        assert_eq!(github.settings.scopes, "read:user user:email");
        assert_eq!(github.settings.issuer, "");
        assert_eq!(github.settings.preset.as_deref(), Some("github"));
        assert_eq!(forum.settings.kind, Kind::OAuth2);
        assert_eq!(forum.settings.id_field.as_deref(), Some("user.id"));
        assert!(forum.fields.contains(&"kind"));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("doesn't use groups"));
    }

    #[test]
    fn reads_presets() {
        let (providers, problems) = parse(&vars(&[
            ("CABINET_OIDC_G_TYPE", "google"),
            ("CABINET_OIDC_G_CLIENT_ID", "abc"),
            ("CABINET_OIDC_G_CLIENT_SECRET", "xyz"),
            ("CABINET_OIDC_KC_TYPE", "keycloak"),
            ("CABINET_OIDC_KC_CLIENT_ID", "abc"),
            ("CABINET_OIDC_KC_CLIENT_SECRET", "xyz"),
            ("CABINET_OIDC_NOPE_TYPE", "myspace"),
            ("CABINET_OIDC_NOPE_CLIENT_ID", "abc"),
            ("CABINET_OIDC_NOPE_CLIENT_SECRET", "xyz"),
        ]));
        let [google] = providers.as_slice() else {
            panic!("one provider: {providers:?}");
        };
        assert_eq!(google.settings.name, "Google");
        assert_eq!(google.settings.issuer, "https://accounts.google.com");
        assert_eq!(google.settings.scopes, "openid profile email");
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(
            problems
                .iter()
                .any(|p| p.contains("KC_*: ISSUER is missing"))
        );
        assert!(problems.iter().any(|p| p.contains("TYPE must be")));
    }

    #[test]
    fn password_switch() {
        assert_eq!(password_sign_in(&vars(&[])), Ok(None));
        assert_eq!(
            password_sign_in(&vars(&[("CABINET_PASSWORD_SIGN_IN", "false")])),
            Ok(Some(false))
        );
        assert!(password_sign_in(&vars(&[("CABINET_PASSWORD_SIGN_IN", "nah")])).is_err());
        assert_eq!(
            guest_access(&vars(&[("CABINET_GUEST_ACCESS", "on")])),
            Ok(Some(true))
        );
        assert_eq!(guest_access(&vars(&[])), Ok(None));
    }
}

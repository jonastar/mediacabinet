# Sign in with another account (OIDC)

Nov 2, 2026 · Jonas

People sign in with an account they already have elsewhere (Authentik, Authelia, Keycloak, Pocket ID, Google, …) instead of a Media Cabinet password. Any number of providers, each a handful of fields and one choice that matters: whether new people wait for approval. Everything else has a default that works. Providers can be set up in the UI or from environment variables. Status: built (`crates/core/src/oidc.rs`, `crates/core/src/sign_in/`, `crates/server/src/api/sign_in.rs`, migration 0015).

## Setting up a provider

Manage → **Users** (its own tab, with a count of people waiting) lists users and, under **Sign-in**, the providers, with "Add provider". The basic form is four fields and one toggle:

| Field | Notes |
| --- | --- |
| Name | Shown on the button ("Sign in with Authentik"). Also gives the provider its **key** (`authentik`), used in its redirect URL and env vars; the key can't change once someone has signed in with it. |
| Issuer URL | e.g. `https://auth.example.com/application/o/cabinet/`. Everything else comes from its discovery document. |
| Client ID, client secret | From the provider. |
| **New people** | *Wait for approval* (default) or *Can watch right away*. |

The form shows the provider's **redirect URL** to paste into the provider, built from the request's host and `X-Forwarded-Proto`/`X-Forwarded-Host` (`https://media.example.com/api/auth/oidc/authentik/callback`). "Save" fetches the discovery document first, so a wrong issuer fails there, not at someone's first sign-in.

### Testing a provider

The form's **Test** section tries the settings as they are in the form, saved or not, turned on or not, and from the environment or not:

- **Check settings** asks the provider without anyone signing in. It finds the issuer (and corrects it to the one the provider names), compares the scopes with the provider's `scopes_supported`, and says where groups will come from, using its `groups` scope or `claims_supported`. It also tells whether the client ID and secret are right by trading a made-up code at the token endpoint: RFC 6749 §5.2 has the provider authenticate the client first, so `invalid_client` means wrong credentials and `invalid_grant` means only the code was wrong. Providers that answer otherwise get "couldn't tell".
- **Try signing in** opens the provider in a new window and signs in there, as whoever the admin is at the provider. Nothing is saved and nobody is signed in. The report shows what came back: the subject, name, email and whether it's verified, groups, granted scopes and every claim's name. It also says what a real sign-in would do: sign in as an existing user, ask "Is this you?", make a new user (pending or not, admin or not) or turn them away. Problems are spelled out, e.g. the provider refusing a scope (`invalid_scope`), no groups claim when groups are used, or an unverified email.

The window's last page hands the report's token to the form over a `BroadcastChannel`, which works even when the provider's `Cross-Origin-Opener-Policy` cuts the window off from the page that opened it, and then closes. If nothing answers, it shows the report on Manage → Users instead (`?sign_in_test=`). A redirect URI the provider doesn't know never comes back at all; the provider shows its own error, so the form says to add the redirect URL.

### Advanced (collapsed by default)

Per provider:

- **Groups claim**: defaults to `groups`.
- **Admin group**: members are admins. Empty: roles are managed in Media Cabinet.
- **Allowed groups**: only members may sign in at all, and they skip approval. Empty: anyone the provider lets through.
- **Scopes**: default `openid profile email` (plus `groups` when a groups setting is filled and the provider advertises it). Check settings shows exactly what's asked for.
- **Redirect URL override**: for setups where the forwarded headers are wrong.
- **Enabled**: off hides the button and refuses sign-ins without losing anyone's links.

Global, at the top of the Sign-in tab:

- **Show password sign-in on the login page**: on by default. Off hides the password form on the login page behind a small "Use a password" link, so an admin can't lock themself out. If it's off and there's exactly one enabled provider, the login page goes straight to it.

### Picking the provider

The form starts with one question, **Provider**, a list of the ones people use:

- **Self-hosted:** Authentik, Authelia, Keycloak, Pocket ID, Kanidm, Zitadel.
- **Online accounts:** Google, Microsoft, GitLab, GitHub, Discord, Apple.
- **Other:** Other OpenID Connect ("most sign-in services; try this first") and Other OAuth2 ("only for providers without OpenID Connect").

Nobody has to know what OIDC or OAuth2 is. Each entry asks only for what that provider needs, in its own terms. Keycloak asks for its address and realm, Authentik for its address and the application slug, Microsoft for the tenant ID; Google and Apple ask for no address at all. The issuer is built from those parts. When a saved issuer doesn't fit them, it's shown as a plain URL instead.

Each entry also brings its defaults: name, scopes, groups claim (Zitadel's roles), and no group settings where the provider has no groups (Google, Apple, GitHub, Discord). Above the fields, **How to set up {provider}** has the redirect URL with a Copy button, a few numbered steps at the provider's end, and a link to its documentation. Authelia gets its configuration snippet and Kanidm its commands, both with the redirect URL filled in.

The list, fields and steps live in the web app (`web/src/pages/manage/providerPresets.ts`). The server keeps a short table of the same keys (`sign_in/presets.rs`) for what it fills in itself: the type, a fixed issuer (Google, Apple) or default one (gitlab.com), default scopes, and whether groups are allowed. The choice is stored as the provider's `preset`, so editing it later shows the same steps. Like the type, it's fixed once the provider is added.

Behind the list there are four ways of signing in:

- **OpenID Connect**: every self-hosted entry, Google, Microsoft, GitLab, Apple and Other OpenID Connect.
- **GitHub** and **Discord**: built-in endpoints and scopes (`read:user user:email`, `identify email`), and no groups. GitHub's email is the primary one from `/user/emails`, and only if GitHub marks it verified. Discord's email counts as verified when its `verified` flag says so.
- **Other OAuth2**: for anything else with a user endpoint. You enter the authorization, token and user URLs. Under Advanced, **User fields** are dotted paths into the user endpoint's JSON: ID (default `id`), name (default `preferred_username`, `login` or `username`), email (default `email`) and verified. Without a verified field, emails count as unverified and never match existing users. The groups field is a dotted path too. Scopes default to none, and then no `scope` is sent.

### Sign in with Apple

Apple is OpenID Connect with two differences:

- **The client secret is signed.** It's an ES256 JWT made from the private key (the `.p8` file, stored as the client secret), the Team ID and the Key ID. Its claims are `iss` = Team ID, `aud` = `https://appleid.apple.com`, and `sub` = the Services ID, which is the client ID. A new one is signed for each token request and lasts five minutes, so it never needs renewing. `ring`, already in the build through rustls, does the signing.
- **Apple posts people back.** When asked for the email or name, Apple requires `response_mode=form_post`, so the callback also takes a POST. A cross-site POST arrives without the `SameSite=Lax` sign-in cookie. So the POST answers with a small page that carries on to the GET callback from this site, and there the cookie comes along. The `user` field Apple posts on someone's first sign-in holds their name. It's kept with the started sign-in (never put in the URL) and names the new user.

Apple needs a real domain over HTTPS for its return URL; localhost and IP addresses are refused. Its emails may be private relay addresses.

Plain OAuth2 has no ID token. Who someone is comes from the user endpoint, which Media Cabinet fetches itself over TLS with the access token. The OIDC path also relies on that TLS connection rather than on signatures (see Implementation). `state` and PKCE still protect the round trip; there's no nonce. The answer is translated into OIDC claims (`sub`, `preferred_username`, `email`, `email_verified`), so linking, approval, groups and the Test buttons work the same for every type. Clients authenticate with HTTP Basic, which RFC 6749 requires servers to support; GitHub gets its credentials in the form instead.

The Test buttons work for these types too. Check can't compare scopes because there's no discovery document, but it still checks the client ID and secret. GitHub answers token errors with HTTP 200 and `incorrect_client_credentials`/`bad_verification_code`, and both are understood.

## Configuring from the environment

For people who keep their setup in a compose file. Same prefix as every other setting:

```sh
CABINET_OIDC_AUTHENTIK_ISSUER=https://auth.example.com/application/o/cabinet/
CABINET_OIDC_AUTHENTIK_CLIENT_ID=media-cabinet
CABINET_OIDC_AUTHENTIK_CLIENT_SECRET_FILE=/run/secrets/authentik
CABINET_OIDC_AUTHENTIK_ADMIN_GROUP=media-admins
CABINET_PASSWORD_SIGN_IN=false
```

- `CABINET_OIDC_{KEY}_{SETTING}`. Settings are a fixed list (`ISSUER`, `CLIENT_ID`, `CLIENT_SECRET`, `NAME`, `NEW_USERS`, `GROUPS_CLAIM`, `ADMIN_GROUP`, `ALLOWED_GROUPS`, `SCOPES`, `REDIRECT_URL`, `ENABLED`, `TEAM_ID` and `KEY_ID` for Apple, and `TYPE` (`oidc`, `oauth2`, or any entry in the list, e.g. `keycloak`, `google`, `apple`, `github`) with `AUTHORIZE_URL`, `TOKEN_URL`, `USERINFO_URL`, `ID_FIELD`, `NAME_FIELD`, `EMAIL_FIELD`, `VERIFIED_FIELD` for the others), matched from the end, so the key can contain underscores: `CABINET_OIDC_POCKET_ID_ISSUER` is key `pocket-id`. Any `_SECRET` also accepts `_SECRET_FILE` for Docker secrets.
- `ISSUER` and `CLIENT_ID` make a provider; anything missing takes the same default as the UI. `NAME` defaults to the key in title case. `NEW_USERS` is `pending` (default) or `active`. `ALLOWED_GROUPS` is comma-separated.
- A provider with a missing or invalid setting is logged at startup and left out; the server still starts.

**The environment wins, until you take over.** At startup each env provider is written to the database (it needs a row for logins to point at) and marked as coming from the environment. In the UI it shows read-only, each field labelled with its variable, and one button: **Edit here instead**. That copies the current values and from then on the database is used and the env vars for that provider are ignored, with a startup warning that names them so it's clear why a changed variable did nothing. **Go back to the environment** undoes it and reapplies the variables.

This is per provider, not per field: mixing the two sources field by field means nobody can tell where a value comes from. Bootstrap-only (env used once, then forgotten) was the other option, but it silently ignores later changes to the compose file, which is the main reason people use env vars.

If an env provider's variables disappear, the provider is disabled, not deleted, so people's links survive a typo in the compose file. It can be deleted from the UI.

## Signing in

Authorization code flow with PKCE (S256), `state` and `nonce`. The login page has a button per enabled provider that goes to `GET /api/auth/oidc/{key}/start`; the provider returns to `GET /api/auth/oidc/{key}/callback`. `state`, `nonce` and the PKCE verifier sit in a short-lived (10 minute) HttpOnly cookie, so nothing is stored for half-finished sign-ins. The callback is per provider so a response can't be replayed against a different provider (mix-up attacks).

An external account is identified by **provider + `sub`**, never by email or username: those can change or be reused at the provider. On the callback, one of three things happens:

1. **Known account** (provider + `sub` is linked to a user): signed in. Same session cookie as a password login.
2. **Email matches an existing user**, the provider says `email_verified`, and that user isn't linked to this provider yet: an "Is this you?" screen showing the account name. Entering that account's password links the two and signs in. "No, create a new account" goes to 3. Never linked without the password: an email match alone would let anyone who controls an email address at the provider take over the account. If the matched user is linked to other providers, the screen also offers "Prove it with {other provider}": signing in there as that user links the waiting account too.
3. **Otherwise**: a new user.
   - Name from `preferred_username`, else `name`, else the email's local part; `-2`, `-3`, … if taken. They can rename themself in Account.
   - Email from the `email` claim if verified.
   - No password. They can set one later in Account if they want both ways in.
   - Role: from the provider's admin group if one is set, else a user.
   - Status: *active* if the provider's new people "Can watch right away", if it has allowed groups (being in one is the approval), or if they're an admin by group; otherwise *pending*.
   - Every library (there's no library access model yet).

Errors at any step (provider down, denied consent, `state` mismatch, not in an allowed group) land on the login page with a one-line reason; the detail goes to the log and, for refusals, to the activity log.

### Linking later

Account → **Sign-in** lists each provider with "Link" or "Unlink". Link goes through the same flow while signed in and links the result to the current user (refused if that external account is already linked to someone else). Unlink is allowed if the user still has another way in (a password or another link). Admins see linked accounts per user in Manage → Users and can unlink them.

This is also how existing users get linked when their Media Cabinet account has no email to match: they sign in with their password once and link.

## Pending, approved, disabled

A user gets a **status**: `active`, `pending` or `disabled`.

- **Pending**: has a session, but the API refuses everything except `/api/auth/*` and `GET /api/me`. The web UI shows a "Waiting for an admin to let you in" screen with a sign-out button and checks again on focus.
- Admins see pending users at the top of Manage → Users with **Let in** and **Decline**, a count on the Users tab, and an admin activity entry ("sam signed in with Authentik for the first time and is waiting to be let in").
- **Decline** sets `disabled` instead of deleting, so the same person signing in again gets "Your access was declined" rather than a fresh pending request. A disabled user can't sign in by any method and their sessions are dropped. Delete is still there for removing someone completely.
- Disabling is useful on its own (it's what an admin wants instead of deleting someone's watch history), so it's a normal action on any non-admin user, not only for declined requests.

The check goes in `require_auth`, in one place like the rest of the policy: non-active users are refused before `needs_admin` is consulted.

## Roles from groups

Only for a provider with an admin group set; otherwise Media Cabinet owns roles and the provider is never asked.

- The groups claim is read from the ID token, falling back to userinfo.
- Applied on **every** sign-in with that provider (not on password sign-ins), both ways: added to the group → admin next sign-in; removed → user next sign-in. The provider is the source of truth, so the admin toggle for that user in Manage → Users is disabled with a title ("Managed by Authentik: admin group `media-admins`").
- A user linked to two providers that both map roles gets the role from whichever they signed in with last. Rare enough not to design around; the title says which provider set it.
- The last-admin rule still holds: if a sign-in would demote the last admin, they stay admin and a warning goes to the activity log.
- Setting an admin group affects admins already linked to the provider too: someone outside the group stops being an admin at their next sign-in with it.
- Users without a link to a role-mapping provider are unaffected.

## Data

Migrations `0015_external_sign_in.sql`, `0016_oauth2_sign_in.sql` and `0017_sign_in_presets.sql`:

- `app_user`: `email TEXT` (unique when set, case-insensitive), `status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'pending', 'disabled'))`.
- `auth_provider`: id, `key` (unique), `source` (`ui` or `env`), `overridden` (env provider edited here instead), name, issuer, client id, client secret, new-user status, groups claim, admin group, allowed groups (JSON array), scopes, redirect override, enabled. The secret is stored as-is, like webhook secrets: the database also holds every session.
- `auth_provider` (0016): `kind` (`oidc`, `github`, `discord`, `oauth2`; the issuer is empty for all but `oidc`), and for `oauth2` the three URLs and four user fields.
- `auth_provider` (0017): `preset` (the entry picked in the form), and Apple's `team_id` and `key_id`. Apple is kind `oidc` with preset `apple` rather than a kind of its own, because the kind's CHECK can't change without rebuilding the table.
- `user_login`: `user_id`, `provider_id`, `subject`, `email` (as last seen), `linked_at`, `last_used_at`; unique on `(provider_id, subject)`.
- Password sign-in on/off: a single-row `auth_setting` table, overridden by `CABINET_PASSWORD_SIGN_IN` when set (shown read-only in the UI like env providers).

Deleting a provider deletes its `user_login` rows; users left with no way in are disabled until an admin gives them a password (the admin is warned with a count before confirming).

## API

Public (`/api/auth/` needs no session):

- `GET /auth/status` gains `providers: [{ key, name }]` and `password_sign_in` for the login page, and the user's `status`.
- `GET /auth/oidc/{key}/start?link=&then_confirm=` → redirect to the provider. `link=true` links to the signed-in user; `then_confirm=true` also completes a waiting "Is this you?" if the sign-in turns out to be that user.
- `GET /auth/oidc/{key}/callback` → redirect to `/`, `/?sign_in=confirm` ("Is this you?", with a cookie naming the waiting account), `/account?linked={key}`, or the same pages with `?sign_in_error=…`.
- `GET /auth/oidc/confirm` (what the screen shows), `POST /auth/oidc/confirm` `{ password }` (case 2), `POST /auth/oidc/confirm/decline` (case 3).

Signed in:

- `PATCH /me` takes `email`, and a `password` without `current_password` when there's no password yet.
- `GET /me/sign-in` (password or not, linked accounts, providers to link), `DELETE /me/logins/{id}`.

Admins:

- `GET /users` adds `has_password`, `logins` and `role_from` (the provider deciding their role). `PATCH /users/{id}` takes `email` and refuses `is_admin` when a provider decides it.
- `PUT /users/{id}/status` `{ status }`: let in or turn back on (`active`), decline or turn off (`disabled`). `DELETE /users/{id}/logins/{login}`.
- `GET /auth-providers` (providers, the password switch, and the redirect URL base), `POST /auth-providers`, `PUT`/`DELETE /auth-providers/{key}`, `POST`/`DELETE /auth-providers/{key}/override`, `POST /auth-providers/test` (fetches discovery; returns the issuer as the provider names it). `PUT /auth-settings` `{ password_sign_in }`.
- Testing takes `{ existing, provider }`: the settings as in the form, and the key of the provider being edited, for its saved secret. `POST /auth-providers/check` returns notes (`good`, `info`, `warning`, `problem`), the issuer and the scopes asked for. `POST /auth-providers/try` returns the URL to open and sets the sign-in cookie; the callback then ends on a page that hands over a token, and `GET /auth-providers/tests/{token}` returns the report for ten minutes.

Activity: `login` says how (`via`); new kinds `user_status`, `login_linked` and `login_unlinked`; `user_created` and `user_changed` say when a provider did it; `login_failed` says why a provider sign-in was refused; provider changes are `config_changed` entries.

## Implementation

Written by hand in `crates/core/src/oidc.rs` with what's already in the tree: `reqwest`, `serde_json`, `sha2`, `getrandom`, plus `base64` (already a transitive dependency). No OIDC or JWT crate.

That's possible because of OIDC Core §3.1.3.7: an ID token received **directly from the token endpoint over TLS**, by a confidential client authenticating with its secret, may be trusted on the strength of the TLS connection instead of its signature. That is the only way we receive ID tokens, so there is no JWKS fetching and no RSA or EC verification. What we do check:

- The token endpoint is HTTPS (see below), and we authenticate with `client_secret_basic`, or `client_secret_post` if that's all discovery lists.
- The discovery document's `issuer` equals the configured issuer exactly, and so does the ID token's `iss`.
- `aud` contains our client ID; if there are several audiences, `azp` is our client ID.
- `exp` is in the future and `iat` isn't, with a minute of clock skew.
- `nonce` equals the one we sent.
- `state` equals the cookie's, compared in constant time.

Rules that keep this sound:

- **HTTPS issuers only.** Without TLS, skipping the signature check would let anyone on the network forge an ID token. `http://` is refused unless the host is `localhost`/loopback (for development). People with an internal provider on plain HTTP put it behind their reverse proxy, which they need for the redirect anyway.
- **Never accept an ID token from anywhere else**: not from the browser, a URL fragment, or the implicit or hybrid flow. Only `response_type=code`. If a flow that hands us tokens through the browser is ever added (native clients), it needs real signature verification first.
- The `userinfo` call (for groups or a missing email) uses the access token from the same exchange; its `sub` must equal the ID token's.

Other details:

- Discovery is cached in memory per provider for an hour and fetched again after a failed sign-in.
- Started sign-ins and waiting "Is this you?" accounts live in memory (at most 1,000 of each, ten minutes each). The `state` is also a cookie scoped to `/api/auth/oidc`, so a callback only completes in the browser that started it.
- Only the claims are kept; access and refresh tokens are discarded. Media Cabinet sessions are its own and don't end when the provider's does.
- Tests run against a small fake provider built from the axum test server already in `cabinet-core`'s dev-dependencies: discovery, authorize redirect, token endpoint, userinfo, and the bad cases (wrong `iss`, `aud`, `nonce`, expired token, mismatched userinfo `sub`).

## Not in v1

- Groups from GitHub (org teams) or Discord (servers).
- Signature verification (JWKS). Not needed for the code flow; needed before any flow that receives tokens through the browser.
- Back-channel or front-channel logout; signing out of Media Cabinet doesn't sign you out of the provider.
- Library access from groups: waits for the library permission model.
- Sign-in for native/TV clients (device code flow or quick connect), with the Jellyfin-compatible API.

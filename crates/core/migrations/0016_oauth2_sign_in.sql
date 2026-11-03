-- Signing in with plain OAuth2: GitHub, Discord, or any provider with a user
-- endpoint. Design: docs/sign-in.md. Only OIDC providers have an issuer; the
-- others keep it empty.

ALTER TABLE auth_provider ADD COLUMN kind TEXT NOT NULL DEFAULT 'oidc'
    CHECK (kind IN ('oidc', 'github', 'discord', 'oauth2'));

-- 'oauth2' only: where to send people, trade the code, and ask who they are.
ALTER TABLE auth_provider ADD COLUMN authorize_url TEXT;
ALTER TABLE auth_provider ADD COLUMN token_url TEXT;
ALTER TABLE auth_provider ADD COLUMN userinfo_url TEXT;
-- 'oauth2' only: fields of the user endpoint's answer, as dotted paths.
-- Unset: `id`, the usual name fields, `email`, and emails count as unverified.
ALTER TABLE auth_provider ADD COLUMN id_field TEXT;
ALTER TABLE auth_provider ADD COLUMN name_field TEXT;
ALTER TABLE auth_provider ADD COLUMN email_field TEXT;
ALTER TABLE auth_provider ADD COLUMN verified_field TEXT;

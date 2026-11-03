-- Providers picked by name in the form (e.g. 'keycloak', 'apple'), and the
-- Team ID and Key ID that sign Sign in with Apple's client secret; its
-- private key is kept as the client secret. Design: docs/sign-in.md.

ALTER TABLE auth_provider ADD COLUMN preset TEXT;
ALTER TABLE auth_provider ADD COLUMN team_id TEXT;
ALTER TABLE auth_provider ADD COLUMN key_id TEXT;

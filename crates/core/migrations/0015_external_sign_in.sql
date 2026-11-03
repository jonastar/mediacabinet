-- Signing in with another account (OIDC). Design: docs/sign-in.md.

ALTER TABLE app_user ADD COLUMN email TEXT;
-- Pending: signed in with a provider, waiting for an admin. Disabled: declined
-- or turned off by an admin; can't sign in at all.
ALTER TABLE app_user ADD COLUMN status TEXT NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'pending', 'disabled'));

CREATE UNIQUE INDEX app_user_email ON app_user (email COLLATE NOCASE) WHERE email IS NOT NULL;

CREATE TABLE auth_provider (
    id             INTEGER PRIMARY KEY,
    -- In the redirect URL and env var names; fixed once someone has signed in.
    key            TEXT NOT NULL UNIQUE,
    -- 'env': written from CABINET_OIDC_* at startup, unless overridden.
    source         TEXT NOT NULL DEFAULT 'ui' CHECK (source IN ('ui', 'env')),
    overridden     INTEGER NOT NULL DEFAULT 0 CHECK (overridden IN (0, 1)),
    name           TEXT NOT NULL,
    issuer         TEXT NOT NULL,
    client_id      TEXT NOT NULL,
    client_secret  TEXT NOT NULL,
    new_users      TEXT NOT NULL DEFAULT 'pending' CHECK (new_users IN ('pending', 'active')),
    groups_claim   TEXT NOT NULL DEFAULT 'groups',
    admin_group    TEXT,
    -- JSON array of group names; empty: anyone the provider lets through.
    allowed_groups TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(allowed_groups)),
    scopes         TEXT NOT NULL DEFAULT 'openid profile email',
    redirect_url   TEXT,
    enabled        INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at     INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- An account at a provider, linked to a user.
CREATE TABLE user_login (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    provider_id  INTEGER NOT NULL REFERENCES auth_provider (id) ON DELETE CASCADE,
    -- The provider's `sub`: the only stable identifier.
    subject      TEXT NOT NULL,
    -- As last seen, for showing which account it is.
    email        TEXT,
    linked_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    last_used_at INTEGER,
    UNIQUE (provider_id, subject)
) STRICT;

CREATE INDEX user_login_user ON user_login (user_id);

CREATE TABLE auth_setting (
    id               INTEGER PRIMARY KEY CHECK (id = 1),
    password_sign_in INTEGER NOT NULL DEFAULT 1 CHECK (password_sign_in IN (0, 1))
) STRICT;

INSERT INTO auth_setting (id) VALUES (1);

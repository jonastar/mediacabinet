-- Logins: passwords on users, and sessions.

-- Argon2 PHC string; null until the account is set up.
ALTER TABLE app_user ADD COLUMN password_hash TEXT;

CREATE TABLE session (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    -- SHA-256 of the cookie's token; the token itself is never stored.
    token_hash   TEXT NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at   INTEGER NOT NULL
) STRICT;

CREATE INDEX session_user ON session (user_id);

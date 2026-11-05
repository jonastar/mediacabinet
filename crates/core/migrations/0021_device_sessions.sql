-- Signing in on TVs and apps: sessions with a device name, and short-lived
-- tokens for media requests that can't send a header.

-- Set on sessions for devices, which last longer and are listed in Account.
ALTER TABLE session ADD COLUMN device_name TEXT;

CREATE TABLE media_token (
    id         INTEGER PRIMARY KEY,
    -- Ends with its session, so signing out a device ends its media tokens too.
    session_id INTEGER NOT NULL REFERENCES session (id) ON DELETE CASCADE,
    -- SHA-256 of the token; the token itself is never stored.
    token_hash TEXT NOT NULL UNIQUE,
    expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX media_token_session ON media_token (session_id);

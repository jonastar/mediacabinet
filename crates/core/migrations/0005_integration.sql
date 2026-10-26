-- Integration: API keys for external tools, and webhook delivery state.

-- Keys are shown once when created; only a SHA-256 hash is kept.
CREATE TABLE api_key (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL,
    -- First characters of the key, to tell keys apart in the UI.
    prefix       TEXT NOT NULL,
    key_hash     TEXT NOT NULL UNIQUE,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch()),
    last_used_at INTEGER
) STRICT;

ALTER TABLE webhook ADD COLUMN name TEXT NOT NULL DEFAULT '';
ALTER TABLE webhook ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1));
-- Consecutive failed deliveries; retries back off exponentially.
ALTER TABLE webhook ADD COLUMN failures INTEGER NOT NULL DEFAULT 0;
ALTER TABLE webhook ADD COLUMN next_attempt_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE webhook ADD COLUMN last_error TEXT;
ALTER TABLE webhook ADD COLUMN last_delivered_at INTEGER;

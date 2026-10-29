-- The activity log (docs/activity.md): a readable history for people, each entry
-- with an audience. Play sessions feed what people watched.

CREATE TABLE activity (
    id              INTEGER PRIMARY KEY,
    -- `items_added`, `watched`, `login`, …; checked in code, so a new kind needs no
    -- table rebuild.
    kind            TEXT NOT NULL,
    at              INTEGER NOT NULL,
    -- Who did it, when a user acted. Kept when the user is removed.
    actor_user_id   INTEGER REFERENCES app_user (id) ON DELETE SET NULL,
    -- Who it's about. Removing the user removes what's about them.
    subject_user_id INTEGER REFERENCES app_user (id) ON DELETE CASCADE,
    -- Entries about library content; viewers see them only for libraries they can access.
    library_id      INTEGER REFERENCES library (id) ON DELETE CASCADE,
    item_id         INTEGER REFERENCES item (id) ON DELETE CASCADE,
    audience        TEXT NOT NULL CHECK (audience IN ('everyone', 'admins', 'subject')),
    payload         TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload)),
    -- The outbox events an entry was made from (JSON array).
    event_ids       TEXT CHECK (event_ids IS NULL OR json_valid(event_ids)),
    CHECK (audience <> 'subject' OR subject_user_id IS NOT NULL)
) STRICT;

CREATE INDEX activity_at ON activity (at, id);
CREATE INDEX activity_item ON activity (item_id, kind, at) WHERE item_id IS NOT NULL;
CREATE INDEX activity_subject ON activity (subject_user_id, kind) WHERE subject_user_id IS NOT NULL;
CREATE INDEX activity_kind ON activity (kind, at);

-- Where consumers of the event outbox are up to.
CREATE TABLE event_consumer (
    name   TEXT PRIMARY KEY,
    cursor INTEGER NOT NULL DEFAULT 0
) STRICT;

-- From the first progress report for an item until no report for a while, or the end.
CREATE TABLE play_session (
    id             INTEGER PRIMARY KEY,
    user_id        INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    item_id        INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    started_at     INTEGER NOT NULL,
    last_report_at INTEGER NOT NULL,
    ended_at       INTEGER,
    position_start REAL NOT NULL,
    position_end   REAL NOT NULL,
    -- Reached the played threshold.
    finished       INTEGER NOT NULL DEFAULT 0 CHECK (finished IN (0, 1)),
    -- The user's incognito setting when the session started.
    incognito      INTEGER NOT NULL CHECK (incognito IN (0, 1)),
    CHECK (finished = 0 OR ended_at IS NOT NULL)
) STRICT;

CREATE UNIQUE INDEX play_session_open ON play_session (user_id, item_id) WHERE ended_at IS NULL;
CREATE INDEX play_session_last ON play_session (user_id, item_id, ended_at);

-- Hides the user's watching from other users (not from admins).
ALTER TABLE app_user ADD COLUMN incognito INTEGER NOT NULL DEFAULT 0 CHECK (incognito IN (0, 1));

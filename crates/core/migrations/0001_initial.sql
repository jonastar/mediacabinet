-- Initial schema. Follows the data model in media-cabinet-handoff/docs/spec.md.
--
-- Conventions:
--   * STRICT tables. Booleans are INTEGER 0/1, JSON is TEXT checked with json_valid.
--   * Timestamps are INTEGER Unix seconds, UTC. Compare against unixepoch().
--   * Derived state (availability, wanted, gaps, next up) is never stored here.
--   * User data is protected with ON DELETE RESTRICT; only pure child rows cascade.

-- ── Storage ─────────────────────────────────────────────────────────────

CREATE TABLE library (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('movies', 'tv'))
) STRICT;

CREATE TABLE library_root (
    id               INTEGER PRIMARY KEY,
    library_id       INTEGER NOT NULL REFERENCES library (id) ON DELETE RESTRICT,
    path             TEXT NOT NULL UNIQUE,
    write_policy     TEXT NOT NULL DEFAULT 'adaptive'
                     CHECK (write_policy IN ('managed', 'adaptive', 'read_only')),
    layout_template  TEXT NOT NULL,
    is_import_target INTEGER NOT NULL DEFAULT 0 CHECK (is_import_target IN (0, 1)),
    -- A read-only root can never receive imports.
    CHECK (NOT (is_import_target AND write_policy = 'read_only'))
) STRICT;

-- At most one import target per library.
CREATE UNIQUE INDEX library_root_one_import_target
    ON library_root (library_id) WHERE is_import_target;

CREATE TABLE media_file (
    id            INTEGER PRIMARY KEY,
    root_id       INTEGER NOT NULL REFERENCES library_root (id) ON DELETE RESTRICT,
    relative_path TEXT NOT NULL,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    device_id     INTEGER,
    inode         INTEGER,
    content_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'present'
                  CHECK (status IN ('present', 'missing', 'removed', 'ignored')),
    first_seen    INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen     INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

-- Removed files keep their rows (and history), so a new file may reuse the path.
CREATE UNIQUE INDEX media_file_path
    ON media_file (root_id, relative_path) WHERE status <> 'removed';
CREATE INDEX media_file_inode ON media_file (device_id, inode);
CREATE INDEX media_file_hash ON media_file (content_hash) WHERE content_hash IS NOT NULL;

CREATE TABLE media_stream (
    file_id      INTEGER NOT NULL REFERENCES media_file (id) ON DELETE CASCADE,
    stream_index INTEGER NOT NULL,
    kind         TEXT NOT NULL CHECK (kind IN ('video', 'audio', 'subtitle')),
    codec        TEXT,
    language     TEXT,
    channels     INTEGER,
    width        INTEGER,
    height       INTEGER,
    hdr_format   TEXT,
    bitrate      INTEGER,
    is_default   INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    is_forced    INTEGER NOT NULL DEFAULT 0 CHECK (is_forced IN (0, 1)),
    PRIMARY KEY (file_id, stream_index)
) STRICT;

CREATE TABLE sidecar (
    id       INTEGER PRIMARY KEY,
    file_id  INTEGER NOT NULL REFERENCES media_file (id) ON DELETE CASCADE,
    -- Absolute: next to the video, or in the cache directory for read-only roots.
    path     TEXT NOT NULL UNIQUE,
    kind     TEXT NOT NULL CHECK (kind IN ('subtitle', 'nfo', 'artwork')),
    language TEXT
) STRICT;

CREATE INDEX sidecar_file ON sidecar (file_id);

-- ── World ───────────────────────────────────────────────────────────────

CREATE TABLE item (
    id               INTEGER PRIMARY KEY,
    parent_id        INTEGER REFERENCES item (id) ON DELETE RESTRICT,
    kind             TEXT NOT NULL CHECK (kind IN ('movie', 'series', 'episode')),
    title            TEXT NOT NULL,
    sort_title       TEXT,
    original_title   TEXT,
    year             INTEGER,
    overview         TEXT,
    runtime_seconds  INTEGER,
    -- Episodes: air time. Movies: release. airs_tz is the network's IANA zone.
    airs_at          INTEGER,
    airs_tz          TEXT,
    series_status    TEXT CHECK (series_status IN ('continuing', 'ended', 'cancelled')),
    -- Series and movies: the provider whose item set is canonical.
    primary_provider TEXT,
    -- JSON array of field names that metadata refresh must never overwrite.
    metadata_locked  TEXT NOT NULL DEFAULT '[]'
                     CHECK (json_valid(metadata_locked) AND json_type(metadata_locked) = 'array'),
    -- Set when the primary provider stops listing the item but it has links or watch state.
    removed_upstream_at INTEGER,
    created_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    CHECK ((kind = 'episode') = (parent_id IS NOT NULL)),
    CHECK (kind = 'series' OR series_status IS NULL),
    CHECK (kind <> 'episode' OR primary_provider IS NULL)
) STRICT;

CREATE INDEX item_parent ON item (parent_id);
CREATE INDEX item_airs_at ON item (airs_at) WHERE airs_at IS NOT NULL;

CREATE TRIGGER item_parent_is_series_insert
BEFORE INSERT ON item
WHEN NEW.parent_id IS NOT NULL
    AND (SELECT kind FROM item WHERE id = NEW.parent_id) IS NOT 'series'
BEGIN
    SELECT RAISE(ABORT, 'episode parent must be a series');
END;

CREATE TRIGGER item_parent_is_series_update
BEFORE UPDATE OF parent_id, kind ON item
WHEN NEW.parent_id IS NOT NULL
    AND (SELECT kind FROM item WHERE id = NEW.parent_id) IS NOT 'series'
BEGIN
    SELECT RAISE(ABORT, 'episode parent must be a series');
END;

CREATE TABLE external_id (
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    provider   TEXT NOT NULL,
    value      TEXT NOT NULL,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    PRIMARY KEY (item_id, provider, value)
) STRICT;

-- Only primary-provider IDs are unique. Secondary IDs may cover several items.
CREATE UNIQUE INDEX external_id_primary_unique
    ON external_id (provider, value) WHERE is_primary;
CREATE UNIQUE INDEX external_id_one_primary_per_item
    ON external_id (item_id) WHERE is_primary;
CREATE INDEX external_id_lookup ON external_id (provider, value);

CREATE TABLE item_tag (
    item_id INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    tag     TEXT NOT NULL,
    source  TEXT NOT NULL,
    PRIMARY KEY (item_id, tag, source)
) STRICT;

CREATE INDEX item_tag_tag ON item_tag (tag);

-- ── Structure ───────────────────────────────────────────────────────────

CREATE TABLE ordering (
    id         INTEGER PRIMARY KEY,
    series_id  INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('aired', 'dvd', 'absolute', 'story', 'custom')),
    source     TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    UNIQUE (series_id, name)
) STRICT;

-- At most one default per series; the import guarantees at least one.
CREATE UNIQUE INDEX ordering_one_default ON ordering (series_id) WHERE is_default;

CREATE TRIGGER ordering_series_is_series
BEFORE INSERT ON ordering
WHEN (SELECT kind FROM item WHERE id = NEW.series_id) IS NOT 'series'
BEGIN
    SELECT RAISE(ABORT, 'ordering must belong to a series');
END;

CREATE TABLE ordering_season (
    id          INTEGER PRIMARY KEY,
    ordering_id INTEGER NOT NULL REFERENCES ordering (id) ON DELETE CASCADE,
    number      INTEGER NOT NULL,
    title       TEXT,
    overview    TEXT,
    poster      TEXT,
    UNIQUE (ordering_id, number),
    UNIQUE (id, ordering_id)
) STRICT;

-- An item may appear more than once in an ordering, and two items may share a
-- number: split and merged episodes map differently across orderings.
CREATE TABLE ordering_entry (
    ordering_id        INTEGER NOT NULL REFERENCES ordering (id) ON DELETE CASCADE,
    position           INTEGER NOT NULL,
    item_id            INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    -- Null for flat orderings (absolute). Must belong to the same ordering.
    ordering_season_id INTEGER,
    episode_number     INTEGER,
    PRIMARY KEY (ordering_id, position),
    FOREIGN KEY (ordering_season_id, ordering_id)
        REFERENCES ordering_season (id, ordering_id) ON DELETE CASCADE
) STRICT;

CREATE INDEX ordering_entry_item ON ordering_entry (item_id);
CREATE INDEX ordering_entry_number
    ON ordering_entry (ordering_id, ordering_season_id, episode_number);

-- ── Bridge ──────────────────────────────────────────────────────────────

CREATE TABLE series_location (
    series_item_id INTEGER NOT NULL REFERENCES item (id) ON DELETE RESTRICT,
    root_id        INTEGER NOT NULL REFERENCES library_root (id) ON DELETE RESTRICT,
    -- Relative to the root. Resolved once and never recomputed from metadata.
    path           TEXT NOT NULL,
    is_primary     INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    PRIMARY KEY (series_item_id, root_id, path)
) STRICT;

CREATE UNIQUE INDEX series_location_one_primary
    ON series_location (series_item_id) WHERE is_primary;
CREATE UNIQUE INDEX series_location_folder ON series_location (root_id, path);

CREATE TRIGGER series_location_is_series
BEFORE INSERT ON series_location
WHEN (SELECT kind FROM item WHERE id = NEW.series_item_id) IS NOT 'series'
BEGIN
    SELECT RAISE(ABORT, 'series location must belong to a series');
END;

-- ── Links and review ────────────────────────────────────────────────────

CREATE TABLE file_link (
    file_id    INTEGER NOT NULL REFERENCES media_file (id) ON DELETE RESTRICT,
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE RESTRICT,
    edition    TEXT,
    part       INTEGER,
    confidence REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    source     TEXT NOT NULL CHECK (source IN ('path_id', 'nfo', 'parser', 'ai', 'user')),
    pinned     INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (file_id, item_id)
) STRICT;

CREATE INDEX file_link_item ON file_link (item_id);

-- The review queue. Two kinds of proposal share it:
--   * file → item: subject is a file, target is an existing item.
--   * folder → series: subject is a series folder found by scanning, target is a
--     provider series that has not been imported yet (nothing is imported on a guess).
CREATE TABLE match_candidate (
    id             INTEGER PRIMARY KEY,
    file_id        INTEGER REFERENCES media_file (id) ON DELETE CASCADE,
    root_id        INTEGER REFERENCES library_root (id) ON DELETE CASCADE,
    folder_path    TEXT,
    item_id        INTEGER REFERENCES item (id) ON DELETE CASCADE,
    provider       TEXT,
    provider_value TEXT,
    provider_title TEXT,
    confidence     REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    source         TEXT NOT NULL CHECK (source IN ('path_id', 'nfo', 'parser', 'ai', 'search')),
    reason         TEXT NOT NULL,
    state          TEXT NOT NULL DEFAULT 'pending'
                   CHECK (state IN ('pending', 'accepted', 'rejected')),
    created_at     INTEGER NOT NULL DEFAULT (unixepoch()),
    decided_at     INTEGER,
    CHECK (
        (file_id IS NOT NULL AND root_id IS NULL AND folder_path IS NULL)
        OR (file_id IS NULL AND root_id IS NOT NULL AND folder_path IS NOT NULL)
    ),
    CHECK (
        (item_id IS NOT NULL AND provider IS NULL AND provider_value IS NULL)
        OR (item_id IS NULL AND provider IS NOT NULL AND provider_value IS NOT NULL)
    ),
    CHECK ((state = 'pending') = (decided_at IS NULL))
) STRICT;

CREATE INDEX match_candidate_pending ON match_candidate (created_at) WHERE state = 'pending';
CREATE INDEX match_candidate_file ON match_candidate (file_id) WHERE file_id IS NOT NULL;

-- ── Wanted and status ───────────────────────────────────────────────────

CREATE TABLE quality_profile (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE,
    -- Allowed formats and cutoff.
    rules TEXT NOT NULL CHECK (json_valid(rules))
) STRICT;

CREATE TABLE monitor (
    item_id            INTEGER PRIMARY KEY REFERENCES item (id) ON DELETE CASCADE,
    -- Null inherits from the parent.
    monitored          INTEGER CHECK (monitored IN (0, 1)),
    quality_profile_id INTEGER REFERENCES quality_profile (id) ON DELETE SET NULL
) STRICT;

-- Written by external tools. The system displays it and never acts on it.
CREATE TABLE item_status (
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    source     TEXT NOT NULL,
    status     TEXT NOT NULL,
    detail     TEXT CHECK (detail IS NULL OR json_valid(detail)),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER NOT NULL,
    PRIMARY KEY (item_id, source)
) STRICT;

-- ── Users and playback ──────────────────────────────────────────────────

-- "user" is reserved in Postgres, hence app_user.
CREATE TABLE app_user (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    is_admin   INTEGER NOT NULL DEFAULT 0 CHECK (is_admin IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE user_item_state (
    user_id          INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    item_id          INTEGER NOT NULL REFERENCES item (id) ON DELETE RESTRICT,
    position_seconds REAL NOT NULL DEFAULT 0,
    played           INTEGER NOT NULL DEFAULT 0 CHECK (played IN (0, 1)),
    play_count       INTEGER NOT NULL DEFAULT 0,
    last_played_at   INTEGER,
    PRIMARY KEY (user_id, item_id)
) STRICT;

CREATE TABLE playlist (
    id               INTEGER PRIMARY KEY,
    -- Null for shared playlists.
    owner_user_id    INTEGER REFERENCES app_user (id) ON DELETE CASCADE,
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL CHECK (kind IN ('manual', 'smart')),
    base_ordering_id INTEGER REFERENCES ordering (id) ON DELETE CASCADE,
    rules            TEXT CHECK (rules IS NULL OR json_valid(rules)),
    CHECK (kind = 'smart' OR (base_ordering_id IS NULL AND rules IS NULL)),
    CHECK (kind = 'manual' OR base_ordering_id IS NOT NULL)
) STRICT;

CREATE TABLE playlist_entry (
    playlist_id INTEGER NOT NULL REFERENCES playlist (id) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    item_id     INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    PRIMARY KEY (playlist_id, position)
) STRICT;

CREATE INDEX playlist_entry_item ON playlist_entry (item_id);

-- Drives "next up": exactly one of ordering or playlist.
CREATE TABLE user_series_pref (
    user_id     INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    series_id   INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    ordering_id INTEGER REFERENCES ordering (id) ON DELETE CASCADE,
    playlist_id INTEGER REFERENCES playlist (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, series_id),
    CHECK ((ordering_id IS NULL) <> (playlist_id IS NULL))
) STRICT;

-- ── Integration ─────────────────────────────────────────────────────────

-- Outbox, written in the same transaction as the change. AUTOINCREMENT keeps ids
-- strictly increasing and never reused, so consumers can resume from a cursor.
CREATE TABLE event (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    type       TEXT NOT NULL,
    payload    TEXT NOT NULL CHECK (json_valid(payload)),
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

CREATE TABLE webhook (
    id            INTEGER PRIMARY KEY,
    url           TEXT NOT NULL,
    -- JSON array of event types; empty means all.
    event_types   TEXT NOT NULL DEFAULT '[]'
                  CHECK (json_valid(event_types) AND json_type(event_types) = 'array'),
    secret        TEXT,
    -- Last event delivered successfully.
    cursor        INTEGER NOT NULL DEFAULT 0,
    created_at    INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;

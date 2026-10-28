-- no-transaction
-- Typed references and owners, enforced by foreign keys instead of triggers.
--
--   * `series` holds what only a series has. A reference that must point at a series
--     (an episode's parent, orderings, folders, next-up choices) points here.
--   * Every item has a library, episodes included, and the rows that belong to a library
--     carry its id: media files, links, series folders, external IDs and review proposals.
--     Composite foreign keys keep each copy equal to its owner's.
--
-- Tables are rebuilt with foreign keys off, because dropping `item` with them on would
-- run its cascades. The rebuild is one transaction and checks every key before it commits.

PRAGMA foreign_keys = OFF;

BEGIN;

DROP VIEW item_wanted;
DROP VIEW item_state;

DROP TRIGGER item_parent_is_series_insert;
DROP TRIGGER item_parent_is_series_update;
DROP TRIGGER ordering_series_is_series;
DROP TRIGGER series_location_is_series;
DROP TRIGGER series_location_in_library_insert;
DROP TRIGGER series_location_in_library_update;
DROP TRIGGER file_link_in_library;
DROP TRIGGER external_id_primary_unique_in_library;

-- ── item and series ─────────────────────────────────────────────────────

CREATE TABLE item_new (
    id               INTEGER PRIMARY KEY,
    -- Episodes are in their series' library.
    library_id       INTEGER NOT NULL REFERENCES library (id) ON DELETE RESTRICT,
    parent_id        INTEGER REFERENCES series (item_id) ON DELETE RESTRICT,
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
    -- Series and movies: the provider whose item set is canonical.
    primary_provider TEXT,
    -- JSON array of field names that metadata refresh must never overwrite.
    metadata_locked  TEXT NOT NULL DEFAULT '[]'
                     CHECK (json_valid(metadata_locked) AND json_type(metadata_locked) = 'array'),
    -- Set when the primary provider stops listing the item but it has links or watch state.
    removed_upstream_at INTEGER,
    -- Series and movies: when metadata was last fetched, and the last failure.
    refreshed_at     INTEGER,
    refresh_error    TEXT,
    created_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at       INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (id, kind),
    UNIQUE (id, library_id),
    UNIQUE (id, library_id, kind),
    FOREIGN KEY (parent_id, library_id) REFERENCES item (id, library_id) ON UPDATE CASCADE,
    CHECK ((kind = 'episode') = (parent_id IS NOT NULL)),
    CHECK (kind <> 'episode' OR primary_provider IS NULL)
) STRICT;

INSERT INTO item_new (id, library_id, parent_id, kind, title, sort_title, original_title, year,
                      overview, runtime_seconds, airs_at, airs_tz, primary_provider,
                      metadata_locked, removed_upstream_at, refreshed_at, refresh_error,
                      created_at, updated_at)
SELECT i.id, coalesce(i.library_id, p.library_id), i.parent_id, i.kind, i.title, i.sort_title,
       i.original_title, i.year, i.overview, i.runtime_seconds, i.airs_at, i.airs_tz,
       i.primary_provider, i.metadata_locked, i.removed_upstream_at, i.refreshed_at,
       i.refresh_error, i.created_at, i.updated_at
FROM item i
LEFT JOIN item p ON p.id = i.parent_id;

CREATE TABLE series (
    item_id            INTEGER PRIMARY KEY,
    kind               TEXT NOT NULL DEFAULT 'series' CHECK (kind = 'series'),
    status             TEXT CHECK (status IN ('continuing', 'ended', 'cancelled')),
    -- The ordering filenames are numbered in, when it isn't the default (e.g. absolute
    -- numbering for anime releases). Null means try the default first.
    match_ordering_id  INTEGER REFERENCES ordering (id) ON DELETE SET NULL,
    -- Limits wanted to the episodes of one playlist (e.g. no filler).
    wanted_playlist_id INTEGER REFERENCES playlist (id) ON DELETE SET NULL,
    FOREIGN KEY (item_id, kind) REFERENCES item (id, kind) ON DELETE CASCADE
) STRICT;

INSERT INTO series (item_id, status, match_ordering_id, wanted_playlist_id)
SELECT id, series_status, match_ordering_id, wanted_playlist_id FROM item WHERE kind = 'series';

DROP TABLE item;
ALTER TABLE item_new RENAME TO item;

CREATE INDEX item_parent ON item (parent_id);
CREATE INDEX item_airs_at ON item (airs_at) WHERE airs_at IS NOT NULL;
CREATE INDEX item_library ON item (library_id);

-- ── Files ───────────────────────────────────────────────────────────────

CREATE UNIQUE INDEX library_root_library ON library_root (id, library_id);

CREATE TABLE media_file_new (
    id            INTEGER PRIMARY KEY,
    root_id       INTEGER NOT NULL,
    library_id    INTEGER NOT NULL,
    relative_path TEXT NOT NULL,
    size          INTEGER NOT NULL,
    mtime         INTEGER NOT NULL,
    device_id     INTEGER,
    inode         INTEGER,
    content_hash  TEXT,
    status        TEXT NOT NULL DEFAULT 'present'
                  CHECK (status IN ('present', 'missing', 'removed', 'ignored')),
    -- From ffprobe. probed_at is null until probed, and reset when content changes.
    container     TEXT,
    duration_seconds REAL,
    probed_at     INTEGER,
    probe_error   TEXT,
    first_seen    INTEGER NOT NULL DEFAULT (unixepoch()),
    last_seen     INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (id, root_id),
    UNIQUE (id, library_id),
    FOREIGN KEY (root_id, library_id) REFERENCES library_root (id, library_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO media_file_new (id, root_id, library_id, relative_path, size, mtime, device_id, inode,
                            content_hash, status, container, duration_seconds, probed_at,
                            probe_error, first_seen, last_seen)
SELECT f.id, f.root_id, r.library_id, f.relative_path, f.size, f.mtime, f.device_id, f.inode,
       f.content_hash, f.status, f.container, f.duration_seconds, f.probed_at, f.probe_error,
       f.first_seen, f.last_seen
FROM media_file f
JOIN library_root r ON r.id = f.root_id;

DROP TABLE media_file;
ALTER TABLE media_file_new RENAME TO media_file;

-- Only present files hold their path. Missing and removed rows keep theirs as
-- history, so a file moved or copied onto that path can take it over.
CREATE UNIQUE INDEX media_file_path
    ON media_file (root_id, relative_path) WHERE status = 'present';
CREATE INDEX media_file_root_path ON media_file (root_id, relative_path);
CREATE INDEX media_file_inode ON media_file (device_id, inode);
CREATE INDEX media_file_hash ON media_file (content_hash) WHERE content_hash IS NOT NULL;

-- A sidecar's root is its video's root.
CREATE TABLE sidecar_new (
    id            INTEGER PRIMARY KEY,
    file_id       INTEGER NOT NULL,
    root_id       INTEGER NOT NULL REFERENCES library_root (id) ON DELETE CASCADE,
    -- 'root': next to the video, relative to the root. 'cache': in the cache
    -- directory, for read-only roots.
    location      TEXT NOT NULL DEFAULT 'root' CHECK (location IN ('root', 'cache')),
    relative_path TEXT NOT NULL,
    kind          TEXT NOT NULL CHECK (kind IN ('subtitle', 'nfo', 'artwork')),
    language      TEXT,
    is_forced     INTEGER NOT NULL DEFAULT 0 CHECK (is_forced IN (0, 1)),
    is_hearing_impaired INTEGER NOT NULL DEFAULT 0 CHECK (is_hearing_impaired IN (0, 1)),
    UNIQUE (root_id, location, relative_path),
    FOREIGN KEY (file_id, root_id) REFERENCES media_file (id, root_id) ON DELETE CASCADE
) STRICT;

INSERT INTO sidecar_new SELECT * FROM sidecar;
DROP TABLE sidecar;
ALTER TABLE sidecar_new RENAME TO sidecar;

CREATE INDEX sidecar_file ON sidecar (file_id);

-- ── Links and review ────────────────────────────────────────────────────

-- A file links only to items in its own library.
CREATE TABLE file_link_new (
    file_id    INTEGER NOT NULL,
    item_id    INTEGER NOT NULL,
    library_id INTEGER NOT NULL,
    edition    TEXT,
    part       INTEGER,
    confidence REAL NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    source     TEXT NOT NULL CHECK (source IN ('path_id', 'nfo', 'parser', 'ai', 'user')),
    pinned     INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0, 1)),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (file_id, item_id),
    FOREIGN KEY (file_id, library_id) REFERENCES media_file (id, library_id) ON DELETE RESTRICT,
    FOREIGN KEY (item_id, library_id) REFERENCES item (id, library_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO file_link_new (file_id, item_id, library_id, edition, part, confidence, source,
                           pinned, created_at)
SELECT l.file_id, l.item_id, f.library_id, l.edition, l.part, l.confidence, l.source, l.pinned,
       l.created_at
FROM file_link l
JOIN media_file f ON f.id = l.file_id;

DROP TABLE file_link;
ALTER TABLE file_link_new RENAME TO file_link;

CREATE INDEX file_link_item ON file_link (item_id);

-- The review queue: the subject (a file, or a series folder) and an item target are in
-- the proposal's library.
CREATE TABLE match_candidate_new (
    id             INTEGER PRIMARY KEY,
    library_id     INTEGER NOT NULL REFERENCES library (id) ON DELETE CASCADE,
    file_id        INTEGER,
    root_id        INTEGER,
    folder_path    TEXT,
    item_id        INTEGER,
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
    FOREIGN KEY (file_id, library_id) REFERENCES media_file (id, library_id) ON DELETE CASCADE,
    FOREIGN KEY (root_id, library_id) REFERENCES library_root (id, library_id) ON DELETE CASCADE,
    FOREIGN KEY (item_id, library_id) REFERENCES item (id, library_id) ON DELETE CASCADE,
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

-- A proposal to link a file to an item in another library could never be accepted.
INSERT INTO match_candidate_new (id, library_id, file_id, root_id, folder_path, item_id, provider,
                                 provider_value, provider_title, confidence, source, reason,
                                 state, created_at, decided_at)
SELECT c.id, coalesce(f.library_id, r.library_id), c.file_id, c.root_id, c.folder_path, c.item_id,
       c.provider, c.provider_value, c.provider_title, c.confidence, c.source, c.reason, c.state,
       c.created_at, c.decided_at
FROM match_candidate c
LEFT JOIN media_file f ON f.id = c.file_id
LEFT JOIN library_root r ON r.id = c.root_id
LEFT JOIN item i ON i.id = c.item_id
WHERE i.id IS NULL OR i.library_id = coalesce(f.library_id, r.library_id);

DROP TABLE match_candidate;
ALTER TABLE match_candidate_new RENAME TO match_candidate;

CREATE INDEX match_candidate_pending ON match_candidate (created_at) WHERE state = 'pending';
CREATE INDEX match_candidate_file ON match_candidate (file_id) WHERE file_id IS NOT NULL;

-- ── Series-owned rows ───────────────────────────────────────────────────

CREATE TABLE external_id_new (
    item_id    INTEGER NOT NULL,
    library_id INTEGER NOT NULL,
    kind       TEXT NOT NULL,
    provider   TEXT NOT NULL,
    value      TEXT NOT NULL,
    is_primary INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    PRIMARY KEY (item_id, provider, value),
    FOREIGN KEY (item_id, library_id, kind) REFERENCES item (id, library_id, kind)
        ON DELETE CASCADE ON UPDATE CASCADE
) STRICT;

INSERT INTO external_id_new (item_id, library_id, kind, provider, value, is_primary)
SELECT x.item_id, i.library_id, i.kind, x.provider, x.value, x.is_primary
FROM external_id x
JOIN item i ON i.id = x.item_id;

DROP TABLE external_id;
ALTER TABLE external_id_new RENAME TO external_id;

-- A primary ID names one item per library and kind (a series and an episode may share a
-- number). Secondary IDs may cover several items.
CREATE UNIQUE INDEX external_id_primary_unique
    ON external_id (library_id, kind, provider, value) WHERE is_primary;
CREATE UNIQUE INDEX external_id_one_primary_per_item
    ON external_id (item_id) WHERE is_primary;
CREATE INDEX external_id_lookup ON external_id (provider, value);

CREATE TABLE ordering_new (
    id         INTEGER PRIMARY KEY,
    series_id  INTEGER NOT NULL REFERENCES series (item_id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('aired', 'dvd', 'absolute', 'story', 'custom')),
    source     TEXT NOT NULL,
    is_default INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    UNIQUE (series_id, name)
) STRICT;

INSERT INTO ordering_new SELECT * FROM ordering;
DROP TABLE ordering;
ALTER TABLE ordering_new RENAME TO ordering;

-- At most one default per series; the import guarantees at least one.
CREATE UNIQUE INDEX ordering_one_default ON ordering (series_id) WHERE is_default;

-- A series' folders are in roots of its own library.
CREATE TABLE series_location_new (
    series_item_id INTEGER NOT NULL REFERENCES series (item_id) ON DELETE RESTRICT,
    root_id        INTEGER NOT NULL,
    library_id     INTEGER NOT NULL,
    -- Relative to the root. Resolved once and never recomputed from metadata.
    path           TEXT NOT NULL,
    is_primary     INTEGER NOT NULL DEFAULT 0 CHECK (is_primary IN (0, 1)),
    PRIMARY KEY (series_item_id, root_id, path),
    FOREIGN KEY (root_id, library_id) REFERENCES library_root (id, library_id) ON DELETE RESTRICT,
    FOREIGN KEY (series_item_id, library_id) REFERENCES item (id, library_id) ON DELETE RESTRICT
) STRICT;

INSERT INTO series_location_new (series_item_id, root_id, library_id, path, is_primary)
SELECT sl.series_item_id, sl.root_id, r.library_id, sl.path, sl.is_primary
FROM series_location sl
JOIN library_root r ON r.id = sl.root_id;

DROP TABLE series_location;
ALTER TABLE series_location_new RENAME TO series_location;

CREATE UNIQUE INDEX series_location_one_primary
    ON series_location (series_item_id) WHERE is_primary;
CREATE UNIQUE INDEX series_location_folder ON series_location (root_id, path);

-- Drives "next up": exactly one of ordering or playlist.
CREATE TABLE user_series_pref_new (
    user_id     INTEGER NOT NULL REFERENCES app_user (id) ON DELETE CASCADE,
    series_id   INTEGER NOT NULL REFERENCES series (item_id) ON DELETE CASCADE,
    ordering_id INTEGER REFERENCES ordering (id) ON DELETE CASCADE,
    playlist_id INTEGER REFERENCES playlist (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, series_id),
    CHECK ((ordering_id IS NULL) <> (playlist_id IS NULL))
) STRICT;

INSERT INTO user_series_pref_new SELECT * FROM user_series_pref;
DROP TABLE user_series_pref;
ALTER TABLE user_series_pref_new RENAME TO user_series_pref;

-- ── Derived views ───────────────────────────────────────────────────────

-- Per episode and movie: availability, resolved monitoring and quality.
--   availability: available | missing | not_acquired | upcoming | unknown
--   monitored: the item's own setting, else its series', else not monitored
--   best_height: the tallest video stream among present linked files
CREATE VIEW item_state AS
WITH files AS (
    SELECT l.item_id,
           sum(f.status = 'present') AS present,
           sum(f.status = 'missing') AS missing,
           max(CASE WHEN f.status = 'present' THEN (
               SELECT max(s.height) FROM media_stream s WHERE s.file_id = f.id AND s.kind = 'video'
           ) END) AS best_height
    FROM file_link l JOIN media_file f ON f.id = l.file_id
    GROUP BY l.item_id
)
SELECT i.id AS item_id,
       i.kind,
       i.parent_id,
       CASE
           WHEN coalesce(fi.present, 0) > 0 THEN 'available'
           WHEN coalesce(fi.missing, 0) > 0 THEN 'missing'
           WHEN i.airs_at IS NULL THEN 'unknown'
           WHEN i.airs_at > unixepoch() THEN 'upcoming'
           ELSE 'not_acquired'
       END AS availability,
       coalesce(m.monitored, pm.monitored, 0) AS monitored,
       coalesce(m.quality_profile_id, pm.quality_profile_id) AS quality_profile_id,
       fi.best_height
FROM item i
LEFT JOIN files fi ON fi.item_id = i.id
LEFT JOIN monitor m ON m.item_id = i.id
LEFT JOIN monitor pm ON pm.item_id = i.parent_id
WHERE i.kind IN ('episode', 'movie');

-- Items that are wanted, and why:
--   missing / not_acquired: monitored and not available
--   upgrade: available, but below the quality profile's cutoff
-- A series with a wanted playlist only wants that playlist's episodes.
CREATE VIEW item_wanted AS
SELECT st.item_id,
       st.kind,
       st.parent_id,
       st.availability,
       st.quality_profile_id,
       st.best_height,
       q.cutoff_height,
       CASE WHEN st.availability = 'available' THEN 'upgrade' ELSE st.availability END AS reason
FROM item_state st
LEFT JOIN (
    SELECT id,
           CASE json_extract(rules, '$.cutoff')
               WHEN '2160p' THEN 2160
               WHEN '1440p' THEN 1440
               WHEN '1080p' THEN 1080
               WHEN '720p' THEN 720
               WHEN '576p' THEN 576
               WHEN '480p' THEN 480
           END AS cutoff_height
    FROM quality_profile
) q ON q.id = st.quality_profile_id
LEFT JOIN series s ON s.item_id = st.parent_id
LEFT JOIN playlist wp ON wp.id = s.wanted_playlist_id
WHERE st.monitored
  AND (
      st.availability IN ('missing', 'not_acquired')
      -- Small allowance: 1080p releases are often 1040 or 800 lines tall (wide aspect ratios).
      OR (st.availability = 'available' AND q.cutoff_height IS NOT NULL
          AND coalesce(st.best_height, 0) < q.cutoff_height * 0.7)
  )
  AND (
      wp.id IS NULL
      OR (wp.kind = 'manual' AND EXISTS (
          SELECT 1 FROM playlist_entry pe WHERE pe.playlist_id = wp.id AND pe.item_id = st.item_id))
      OR (wp.kind = 'smart'
          AND EXISTS (SELECT 1 FROM ordering_entry oe
                      WHERE oe.ordering_id = wp.base_ordering_id AND oe.item_id = st.item_id)
          AND NOT EXISTS (SELECT 1 FROM item_tag t, json_each(wp.rules, '$.exclude_tags') x
                          WHERE t.item_id = st.item_id AND t.tag = x.value))
  );

-- ── Check and commit ────────────────────────────────────────────────────

-- Fails the migration (and rolls it back) if any row breaks a foreign key, e.g. a file
-- linked to an item in another library before links were checked on every write.
CREATE TEMP TABLE migration_check (
    foreign_key_violations INTEGER CHECK (foreign_key_violations = 0)
);
INSERT INTO migration_check SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE migration_check;

COMMIT;

PRAGMA foreign_keys = ON;

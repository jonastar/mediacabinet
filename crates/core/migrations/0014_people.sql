-- People and credits, companies, ratings, and a few more details from providers.
-- All of it is replaced on each refresh; nothing here is edited by hand.

-- Someone credited on a series, episode or movie. Shared by every library.
CREATE TABLE person (
    id           INTEGER PRIMARY KEY,
    provider     TEXT NOT NULL,
    provider_id  TEXT NOT NULL,
    name         TEXT NOT NULL,
    -- The provider's image path, like item_artwork.path.
    photo        TEXT,
    -- Acting, Directing, Writing...
    known_for    TEXT,
    -- Fetched when the person's page is first opened.
    biography    TEXT,
    birthday     TEXT,
    deathday     TEXT,
    birthplace   TEXT,
    details_fetched_at INTEGER,
    UNIQUE (provider, provider_id)
) STRICT;

--   cast: a series' regulars or a movie's cast
--   guest: an episode's guest stars
--   crew: directors, writers, creators...
CREATE TABLE credit (
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('cast', 'guest', 'crew')),
    position   INTEGER NOT NULL,
    person_id  INTEGER NOT NULL REFERENCES person (id) ON DELETE RESTRICT,
    -- Cast and guests: the character. Crew: the job.
    role       TEXT,
    -- Crew only.
    department TEXT,
    -- A series' cast: how many episodes they're in.
    episodes   INTEGER,
    PRIMARY KEY (item_id, kind, position)
) STRICT;

CREATE INDEX credit_person ON credit (person_id);

-- Networks a series airs on, and studios that made it.
CREATE TABLE item_company (
    item_id     INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    role        TEXT NOT NULL CHECK (role IN ('network', 'studio')),
    position    INTEGER NOT NULL,
    name        TEXT NOT NULL,
    provider    TEXT NOT NULL,
    provider_id TEXT,
    PRIMARY KEY (item_id, role, position)
) STRICT;

-- Scores from rating sites: tmdb, imdb, rotten_tomatoes, metacritic.
CREATE TABLE item_rating (
    item_id    INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    source     TEXT NOT NULL,
    value      REAL NOT NULL,
    -- What a perfect score is: 10 or 100.
    scale      REAL NOT NULL CHECK (scale > 0),
    votes      INTEGER,
    fetched_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (item_id, source)
) STRICT;

ALTER TABLE item ADD COLUMN tagline TEXT;
-- The age rating in the configured region, e.g. PG-13 or TV-MA.
ALTER TABLE item ADD COLUMN content_rating TEXT;
-- Series and movies: when ratings were last asked of OMDb, found or not.
ALTER TABLE item ADD COLUMN ratings_checked_at INTEGER;

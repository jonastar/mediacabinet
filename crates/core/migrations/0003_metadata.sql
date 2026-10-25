-- Metadata import: artwork, refresh state, and the ordering used for file matching.

-- Artwork from the primary provider. `path` is the provider's image path; URLs
-- are built when serving, so image sizes can change without a refresh.
CREATE TABLE item_artwork (
    item_id  INTEGER NOT NULL REFERENCES item (id) ON DELETE CASCADE,
    kind     TEXT NOT NULL CHECK (kind IN ('poster', 'backdrop', 'still')),
    provider TEXT NOT NULL,
    path     TEXT NOT NULL,
    PRIMARY KEY (item_id, kind)
) STRICT;

-- Series and movies: when metadata was last fetched, and the last failure.
ALTER TABLE item ADD COLUMN refreshed_at INTEGER;
ALTER TABLE item ADD COLUMN refresh_error TEXT;

-- Series: the ordering filenames are numbered in, when it isn't the default
-- (e.g. absolute numbering for anime releases). Null means try the default first.
ALTER TABLE item ADD COLUMN match_ordering_id INTEGER REFERENCES ordering (id) ON DELETE SET NULL;

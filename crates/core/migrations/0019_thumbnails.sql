-- Frames extracted from video files, kept as `thumbnails/{file_id}.webp` in the
-- data directory. A row records what's on disk, as a scan records what's in a
-- root: the file's size and mtime when it was tried show when it has changed.

CREATE TABLE file_thumbnail (
    file_id INTEGER PRIMARY KEY REFERENCES media_file (id) ON DELETE CASCADE,
    size    INTEGER NOT NULL,
    mtime   INTEGER NOT NULL,
    made_at INTEGER NOT NULL DEFAULT (unixepoch()),
    -- Why no frame could be extracted. Scans leave these alone until the file
    -- changes; asking by hand tries them again.
    error   TEXT
) STRICT;

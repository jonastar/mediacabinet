-- Inboxes: a folder per library whose videos are imported on their own
-- (docs/library-management.md).

-- Absolute; outside every root. NULL: no inbox.
ALTER TABLE library ADD COLUMN inbox_path TEXT;

-- Videos an inbox has dealt with, so none is imported twice. One row per path:
-- a file replaced at the same path (another inode, size or mtime) is looked at
-- again. Rows go once their file has left the inbox.
CREATE TABLE inbox_file (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES library (id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    device_id  INTEGER,
    inode      INTEGER,
    size       INTEGER NOT NULL,
    mtime      INTEGER NOT NULL,
    -- `imported`, or `attention` when a person has to decide.
    state      TEXT NOT NULL CHECK (state IN ('imported', 'attention')),
    -- Why it needs attention.
    reason     TEXT,
    -- The library file it became, while that's known.
    file_id    INTEGER REFERENCES media_file (id) ON DELETE SET NULL,
    seen_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (library_id, path)
) STRICT;

-- Folders excluded from matching: their files are ignored, and so are files that
-- appear in them later. Removing the row brings them back.
CREATE TABLE ignored_folder (
    id          INTEGER PRIMARY KEY,
    root_id     INTEGER NOT NULL REFERENCES library_root(id) ON DELETE CASCADE,
    folder_path TEXT NOT NULL,
    ignored_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    UNIQUE (root_id, folder_path)
);

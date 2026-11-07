-- Provider artwork the provider said it doesn't have. What has been downloaded is
-- on disk, so isn't recorded; this keeps background runs from asking again for
-- what would only fail. Asking by hand tries again.

CREATE TABLE artwork_unavailable (
    provider TEXT NOT NULL,
    kind     TEXT NOT NULL,
    path     TEXT NOT NULL,
    tried_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (provider, kind, path)
) STRICT, WITHOUT ROWID;

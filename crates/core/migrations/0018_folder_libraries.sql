-- no-transaction
-- A third kind of library: 'folders' shows its folders as they are, without
-- identifying anything or fetching metadata.
--
-- SQLite can't change a CHECK in place, so the table is rebuilt. Foreign keys are off
-- because dropping `library` with them on would run its cascades. The rebuild is one
-- transaction and checks every key before it commits.

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE library_new (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL CHECK (kind IN ('movies', 'tv', 'folders'))
) STRICT;

INSERT INTO library_new (id, name, kind) SELECT id, name, kind FROM library;

DROP TABLE library;

ALTER TABLE library_new RENAME TO library;

CREATE TEMP TABLE migration_check (
    foreign_key_violations INTEGER CHECK (foreign_key_violations = 0)
);
INSERT INTO migration_check SELECT count(*) FROM pragma_foreign_key_check;
DROP TABLE migration_check;

COMMIT;

PRAGMA foreign_keys = ON;

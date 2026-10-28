-- Background tasks (docs/tasks.md). Scans become the first kind; their history
-- moves over from scan_run with the same ids.

CREATE TABLE task (
    id           INTEGER PRIMARY KEY,
    -- `scan`, …; checked in code, so a new kind needs no table rebuild.
    kind         TEXT NOT NULL,
    -- The scope: at most one is set; none means everything.
    library_id   INTEGER REFERENCES library (id) ON DELETE CASCADE,
    root_id      INTEGER REFERENCES library_root (id) ON DELETE CASCADE,
    item_id      INTEGER REFERENCES item (id) ON DELETE CASCADE,
    file_id      INTEGER REFERENCES media_file (id) ON DELETE CASCADE,
    -- What asked for it: `added`, `manual`, `scheduled`, `watch`, ….
    trigger      TEXT NOT NULL,
    state        TEXT NOT NULL DEFAULT 'queued'
                 CHECK (state IN ('queued', 'running', 'ok', 'failed', 'cancelled')),
    requested_at INTEGER NOT NULL DEFAULT (unixepoch()),
    started_at   INTEGER,
    finished_at  INTEGER,
    -- What it did, per kind: for scans {"scan": …, "probe": …, "match": …}.
    outcome      TEXT CHECK (outcome IS NULL OR json_valid(outcome)),
    error        TEXT,
    CHECK ((library_id IS NOT NULL) + (root_id IS NOT NULL) + (item_id IS NOT NULL)
           + (file_id IS NOT NULL) <= 1),
    CHECK ((state = 'queued') = (started_at IS NULL) OR state = 'cancelled'),
    CHECK ((state IN ('queued', 'running')) = (finished_at IS NULL))
) STRICT;

CREATE INDEX task_open ON task (state) WHERE state IN ('queued', 'running');
CREATE INDEX task_root ON task (root_id, kind, id) WHERE root_id IS NOT NULL;
CREATE INDEX task_library ON task (library_id, kind, id) WHERE library_id IS NOT NULL;
CREATE INDEX task_item ON task (item_id, kind, id) WHERE item_id IS NOT NULL;
CREATE INDEX task_file ON task (file_id, kind, id) WHERE file_id IS NOT NULL;

INSERT INTO task (id, kind, root_id, trigger, state, requested_at, started_at, finished_at,
                  outcome, error)
SELECT id, 'scan', root_id, trigger, state, started_at, started_at, finished_at, report, error
FROM scan_run;

DROP TABLE scan_run;

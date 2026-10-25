-- Scan history per root, and when a root became unreachable.

-- Set when a scan finds the root unreachable or unmounted; cleared by the next
-- successful scan.
ALTER TABLE library_root ADD COLUMN offline_since INTEGER;

CREATE TABLE scan_run (
    id          INTEGER PRIMARY KEY,
    root_id     INTEGER NOT NULL REFERENCES library_root (id) ON DELETE CASCADE,
    -- What started it.
    trigger     TEXT NOT NULL CHECK (trigger IN ('added', 'manual', 'scheduled', 'watch')),
    state       TEXT NOT NULL DEFAULT 'running' CHECK (state IN ('running', 'ok', 'failed')),
    started_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    finished_at INTEGER,
    -- {"scan": ScanReport, "probe": ProbeReport} when it ran.
    report      TEXT CHECK (report IS NULL OR json_valid(report)),
    error       TEXT,
    CHECK ((state = 'running') = (finished_at IS NULL))
) STRICT;

CREATE INDEX scan_run_root ON scan_run (root_id, started_at DESC);

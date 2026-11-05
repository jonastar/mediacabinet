-- What a task logged while it ran (docs/tasks.md), capped per task.

CREATE TABLE task_log (
    id      INTEGER PRIMARY KEY,
    task_id INTEGER NOT NULL REFERENCES task (id) ON DELETE CASCADE,
    at      INTEGER NOT NULL,
    level   TEXT NOT NULL CHECK (level IN ('info', 'warn', 'error')),
    message TEXT NOT NULL,
    fields  TEXT CHECK (fields IS NULL OR json_valid(fields))
) STRICT;

CREATE INDEX task_log_task ON task_log (task_id, id);

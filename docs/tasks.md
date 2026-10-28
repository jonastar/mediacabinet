# Tasks

Oct 28, 2026 · Jonas

Background work the server does on its own or on request: scans, metadata refresh, thumbnails, trickplay, and later AI subtitle generation and sync. One system, one history, one progress UI. Status: design, not built.

## What a task is

- A task has a **kind** (`scan`, `metadata_refresh`, `thumbnails`, `trickplay`, …) and a **scope**: a library, a root, an item or a file. Every kind accepts every scope that makes sense for it; "regenerate this episode's thumbnail" is the thumbnails task with an item scope.
- A task's work list is **computed from current state when it runs**, not stored: the thumbnails task over a library means "files in this library without a thumbnail". No per-item subtasks or rows.
- Tasks are **idempotent**. Running one twice does nothing the second time, so resuming after a restart is running it again. There are no checkpoints.
- Kinds with nothing to compute from (forced regeneration) keep a `force` flag in the task's parameters and an optional cursor (last file id done), so a restart doesn't start over.

## Requests and coalescing

- A request whose scope is already covered by a queued or running task of the same kind is merged into it (a library task covers an item in it).
- A request for a scope a running task doesn't cover is queued. Same idea as the scan service's re-run today: at most one queued follow-up per kind and scope.
- Each task records its **trigger**: `manual`, `scheduled`, `watch`, `added`, `import`, `webhook`. Manual requests go ahead of scheduled ones in the queue.

## Running

Two lanes, no general worker pool:

| Lane | Kinds | Concurrency |
| --- | --- | --- |
| Light | scan, metadata refresh | As today: one scan per root; metadata limited by the provider's rate limit |
| Heavy | thumbnails, trickplay, AI subtitles | One task at a time (a semaphore). A setting if someone wants more. |

- A long heavy task must never block a scan or a metadata refresh; that's the only reason for the lanes.
- **Playback comes first.** Between items, the heavy lane waits while any transcode is running. Checked per item, so a task pauses within one item's work.
- AI subtitle work shares the heavy lane for now, GPU or not. Split it out when there's a reason to.
- Tasks can be **cancelled**. Cancellation is checked between items; the item in progress finishes or is abandoned, never left half-written (write outputs to a temporary name, then rename).

## Restart

- On startup, tasks left `running` go back to `queued`, not `failed`: being idempotent, they pick up where the state says they are. Queued tasks run too; a second queued one for the same kind and scope is folded into the first.
- Scheduled tasks (periodic scans, metadata refresh) are requested by a scheduler as now; the request coalesces with anything already queued.

## Progress, outcomes and logs

- **Progress** lives in memory while running, as `ScanProgress` does now: a phase, `done` and `total` (total unknown while walking). Persisted on state changes, not every item. Served over the API and pushed on the event stream.
- **Outcomes** are structured and persisted with the task: counts (done, skipped, failed) plus a list of failed items with their path and error, capped (first 200). This is what the task inspector shows first. The scan's `ScanReport` becomes the scan kind's outcome.
- **Logs**: each task runs inside a `tracing` span carrying `task_id`; a tracing layer copies events in that span to the task's log. Capped per task (e.g. 500 lines, warnings and errors kept over info when trimming) so a sweep that warns per file can't fill the database. Logs are for diagnosing; outcomes are for the UI.

## Data

- The `task` table (migration 0010) replaced `scan_run`, keeping its ids: `kind`, the scope as one of `library_id`, `root_id`, `item_id`, `file_id` (at most one set, none meaning everything; each a foreign key, so a task goes with its scope), `trigger`, `state` (`queued`, `running`, `ok`, `failed`, `cancelled`), `requested_at`, `started_at`, `finished_at`, `outcome` (JSON), `error`. Kinds and triggers are checked in code, so adding one needs no table rebuild.
- Still to add when a kind needs them: `params` (JSON, e.g. `force`) and `requested_by` (user).
- `task_log`: `task_id`, `at`, `level`, `message`, `fields` (JSON). Deleted with the task.
- Old finished tasks are pruned after a retention period (30 days, keeping the last run per kind and scope regardless).
- Generated files (thumbnails, trickplay) live in the data directory keyed by file id; whether one exists is checked on disk or from a column on the file, never a "done" flag on the task.

## Migration from what exists

1. Done: the queue, coalescing, progress and recovery moved from `ScanService` into the task runner (`crates/core/src/tasks.rs`); scans are the first kind. `ScanService` keeps the scan's work, its schedule and watching.
2. Metadata refresh (`metadata::refresh::spawn_refresher`) becomes a scheduled kind.
3. Thumbnails, then trickplay, as the first heavy kinds.

## API and UI

- `GET /tasks` (filter by state, kind, scope), `GET /tasks/{id}` with outcome and log, `POST /tasks` to request, `POST /tasks/{id}/cancel`. All admin-only.
- A Tasks screen in Manage mode: running and queued with progress, history below. The task inspector shows parameters, outcome, failures and log.
- Library, series and item pages show tasks scoped to them and offer the relevant requests ("Generate thumbnails", "Rescan").
- Finished and failed tasks appear in the activity log for admins (see `activity.md`).

## Open

- Retention period and log cap values.
- Whether scheduled heavy tasks (thumbnails for new files) run right after a scan adds files or on their own schedule. Leaning: a scan that adds files requests them, scoped to its root.

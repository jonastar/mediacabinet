# Tasks

Oct 28, 2026 · Jonas

Background work the server does on its own or on request: scans, metadata refresh, thumbnails, artwork downloads, trickplay, and later AI subtitle generation and sync. One system, one history, one progress UI. Status: partly built; the roadmap's Phase 8 lists what's left.

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
| Light | scan, metadata refresh, artwork | As today: one scan per root; metadata limited by the provider's rate limit |
| Heavy | thumbnails, trickplay, AI subtitles | One task at a time (a semaphore). A setting if someone wants more. |

- A long heavy task must never block a scan or a metadata refresh; that's the only reason for the lanes.
- **Playback comes first.** Between items, a heavy task waits while any HLS session (remux or transcode) is open. Checked per item, so a task pauses within one item's work. Direct play doesn't count: it costs the server next to nothing.
- A heavy task stays `queued` while it waits for the lane, and can be cancelled meanwhile.
- AI subtitle work shares the heavy lane for now, GPU or not. Split it out when there's a reason to.
- Tasks can be **cancelled**. Cancellation is checked between items; the item in progress finishes or is abandoned, never left half-written (write outputs to a temporary name, then rename). A running scan can't be stopped, as it has no items to stop between; its queued follow-up can.

## Restart

- On startup, tasks left `running` go back to `queued`, not `failed`: being idempotent, they pick up where the state says they are. Queued tasks run too; a second queued one for the same kind and scope is folded into the first.
- Scheduled tasks (periodic scans, metadata refresh) are requested by a scheduler as now; the request coalesces with anything already queued.

## Progress, outcomes and logs

- **Progress** lives in memory while running, as `ScanProgress` does now: a phase, `done` and `total` (total unknown while walking). Persisted on state changes, not every item. Served over the API and pushed on the event stream.
- **Outcomes** are structured and persisted with the task: counts (done, skipped, failed) plus a list of failed items with their path and error, capped (first 200). This is what the task inspector shows first. The scan's `ScanReport` becomes the scan kind's outcome.
- **Logs**: each task runs inside a `tracing` span carrying `task_id`; a tracing layer copies events in that span to the task's log. Capped per task (e.g. 500 lines, warnings and errors kept over info when trimming) so a sweep that warns per file can't fill the database. Logs are for diagnosing; outcomes are for the UI. The server's log filter (`RUST_LOG`) applies, and work moved off the task with `spawn_blocking` or `tokio::spawn` logs outside the span unless it's instrumented.

## Data

- The `task` table (migration 0010) replaced `scan_run`, keeping its ids: `kind`, the scope as one of `library_id`, `root_id`, `item_id`, `file_id` (at most one set, none meaning everything; each a foreign key, so a task goes with its scope), `trigger`, `state` (`queued`, `running`, `ok`, `failed`, `cancelled`), `requested_at`, `started_at`, `finished_at`, `outcome` (JSON), `error`. Kinds and triggers are checked in code, so adding one needs no table rebuild.
- Still to add when a kind needs them: `params` (JSON, e.g. `force`) and `requested_by` (user).
- `task_log` (migration 0020): `task_id`, `at`, `level`, `message`, `fields` (JSON). Deleted with the task, and after 30 days once it has finished.
- Still to do: old finished tasks are pruned after a retention period (30 days, keeping the last run per kind and scope regardless).
- Generated files (thumbnails, trickplay) live in the data directory keyed by file id; whether one exists is checked on disk or from a table keyed by file, never a "done" flag on the task.

## Thumbnails

- The `thumbnails` kind runs on a library. Its work list is present video files linked to a movie (folder videos are movies) or an episode with no provider art, and no thumbnail of the file as it is now: `file_thumbnail` (migration 0019) records each file's size and mtime when it was tried.
- The frame comes from 25% in; if that's nearly black, 40%, 60% and 10%, keeping the brightest. One WebP at most 640 wide, tone-mapped if HDR, at `thumbnails/{file_id}.webp` in the data directory.
- Thumbnails are a fallback, not artwork: `item_artwork` holds provider images, and `artwork` on an item lists `thumbnail` as well when it has one (`GET /artwork/{id}/thumbnail`).
- An episode has one picture, so clients don't choose: `artwork` lists `still` if it has either sort, and `GET /artwork/{id}/still` serves the provider's if there is one, otherwise the frame. A frame made before the provider had a still is simply no longer served.
- A scan asks for a run, on its root's library, when that would find anything. A file that fails is listed in the outcome and remembered: scans skip it until the file changes, and asking by hand tries it again. Failing to start ffmpeg ends the run without marking anything.

## Artwork

- Provider images are kept in `artwork/{provider}/{size}/` in the data directory. Items only store the provider's path, so the `artwork` task downloads them ahead of time, and a client asking first downloads that one image. It's why the library keeps working when TMDB doesn't.
- The `artwork` kind runs on a library. Its work list is the `item_artwork` rows with nothing on disk at the current size, so changing a size downloads that kind again. People's photos aren't included: there are many, on pages rarely visited, so they still download when first asked for.
- A scan or metadata refresh asks for a run when that would find anything, and an hourly sweep asks too, which covers the scheduled refresh (it doesn't run as a task yet) and picks up after an outage.
- It's in the light lane, because downloading doesn't compete with playback for the CPU and shouldn't wait hours behind thumbnails.
- An image the provider says it hasn't got is listed in the outcome and remembered in `artwork_unavailable` (migration 0022); a refresh by hand tries it again. An outage stops the run without marking anything, and only counts as failed if someone asked for that run, so an hourly sweep doesn't report it every hour.
- Still to do: removing images nothing refers to any more, such as old sizes or artwork a provider has replaced.

## Migration from what exists

1. Done: the queue, coalescing, progress and recovery moved from `ScanService` into the task runner (`crates/core/src/tasks.rs`); scans are the first kind. `ScanService` keeps the scan's work, its schedule and watching.
2. Partly done: `metadata_refresh` is a kind with a library scope, requested by hand (`POST /libraries/{id}/refresh`). It refreshes every series and movie in the library not refreshed since the request, so a restart carries on. Still to do: the schedule (`metadata::refresh::spawn_refresher`) requesting it instead of refreshing on its own.
3. Done: thumbnails, the first heavy kind. Trickplay is next.

## API and UI

- `GET /tasks` (filter by kind and result), `GET /tasks/{id}` with outcome and log, `POST /tasks/{id}/cancel`. All admin-only. Requests live with what they act on: `POST /roots/{id}/scan`, `POST /libraries/{id}/refresh`, `POST /libraries/{id}/thumbnails`.
- A Tasks tab in the Manage menu: running and queued with progress, history below. The task inspector shows parameters, outcome, failures and log.
- Library, series and item pages show tasks scoped to them and offer the relevant requests ("Generate thumbnails", "Rescan").
- Finished and failed tasks appear in the activity log for admins (see `activity.md`).

## Open

- Retention period and log cap values.
- Removing thumbnails of files that are gone: a removed root's rows go, but its images stay on disk until something sweeps them.

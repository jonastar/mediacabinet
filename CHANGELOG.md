# Changelog

Changes that matter to people running Media Cabinet or using its API: features,
fixes, changed behaviour, migrations and anything to do when upgrading. Newest
first. How versions and releases work is in `docs/releasing.md`.

Keep entries brief: one line each, saying what changed for the user, not how.
Details belong in the commit, docs or API reference.

## 1.0.0 (unreleased)

The first release: a self-hosted library manager and streaming server for movies
and TV, with its own web UI. See `docs/roadmap.md` for what v1 covers.

- Scans interrupted by a restart run again on startup instead of being marked failed.
- Migration 0010 moves scan history into the new task table; `scan.finished` events carry `task_id` instead of `scan_run_id`.
- Migration 0009 rebuilds the item, file and link tables (backed up first); the wanted list's library filter now uses each item's own library.
- Episode inspector in Manage mode: links (unlink, set edition), numbers in each ordering, history.

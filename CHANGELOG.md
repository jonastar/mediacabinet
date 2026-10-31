# Changelog

Changes that matter to people running Media Cabinet or using its API: features,
fixes, changed behaviour, migrations and anything to do when upgrading. Newest
first. How versions and releases work is in `docs/releasing.md`.

Keep entries brief: one line each, saying what changed for the user, not how.
Details belong in the commit, docs or API reference.

## 1.0.0 (unreleased)

The first release: a self-hosted library manager and streaming server for movies
and TV, with its own web UI. See `docs/roadmap.md` for what v1 covers.

- Redesigned Home: a greeting header, Up next beside This week, and Recently added as posters.
- Episodes split over two files ("The Last Day (1)" and "(2)", or "Part 1" and "Part 2") link to the one episode as parts, and an episode number past the end of a season is no longer read as an earlier absolute number.
- Migration 0012 adds the part to review candidates.
- Media Cabinet branding: new logo and wordmark, teal accent colour, favicons, home-screen icons and an animated loading screen.
- Activity log: library additions, what people watched (with an incognito setting), and for admins sign-ins, account and settings changes and task results; on an Activity screen, Home and the inspector.
- Migration 0011 adds the activity log, play sessions and the incognito setting; `activity.added` events announce new entries.
- Scans interrupted by a restart run again on startup instead of being marked failed.
- Migration 0010 moves scan history into the new task table; `scan.finished` events carry `task_id` instead of `scan_run_id`.
- Migration 0009 rebuilds the item, file and link tables (backed up first); the wanted list's library filter now uses each item's own library.
- Episode inspector in Manage mode: links (unlink, set edition), numbers in each ordering, history.

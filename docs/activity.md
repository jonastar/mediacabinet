# Activity

Oct 28, 2026 · Jonas

A readable history of what happened: library additions, what people watched, user and configuration changes, tasks, and later comments, ratings and reviews. Each entry has an audience; viewers only see what they're allowed to. Status: built (`crates/core/src/activity/`, migration 0011).

## Activity and events

The event outbox (`events.rs`) and activity are separate systems:

- **Events** are for machines: fine-grained (one `file.added` per file), no actor, no visibility, consumed from a cursor by webhooks, the SSE stream and the wanted syncer.
- **Activity** is for people: grouped ("5 new episodes of X"), attributed to a user when one acted, filtered per viewer.

Activity entries made from events store the ids of the events they came from, so the two can be joined, or merged into one system later if that turns out to be simpler.

## Where entries come from

- **Library changes**: a consumer reads the event outbox from its own cursor (like the wanted syncer) and writes activity, grouping events of one scan or import for the same series or movie into one entry.
- **Direct writes** for things that aren't events: logins, user and configuration changes, playback, task results, and later social features.

## Audience

Every entry has an audience, set when it's written:

| Audience | Who sees it |
| --- | --- |
| `everyone` | All users who can access the entry's library |
| `admins` | Admins only |
| `subject` | The user it's about, and admins |

- The filter is applied in one query function, taking the `Viewer`. The endpoint is an ordinary read, not in `ADMIN_READS`.
- Entries about library content carry their `library_id`. Users only see entries for libraries they can access. Per-user library access comes after v1 (per-user library scoping in the roadmap); until then everyone can access every library. The filter joins on the library from the start, so when access control lands, activity follows it with a change in one place, and a child account won't see entries for adult titles.

## Playback

- Playback is recorded as **play sessions**: a session starts with the first progress report for an item and ends after no report for a timeout (e.g. 10 minutes) or when the item is finished. `watch::report_progress` is where they start and extend.
- An activity entry is written when a session ends, only if it lasted at least a minimum (e.g. 5 minutes) or reached the end. Short sessions (sampling, skipping around) produce nothing.
- Users see each other's watching by default.
- **Incognito** is a per-user setting. Sessions record whether it was on when they happened; their entries get the `subject` audience instead of `everyone`. Turning incognito off later doesn't expose earlier sessions, and turning it on doesn't hide earlier ones.
- The setting says plainly that incognito hides watching from other users, not from admins.
- Watch state (`user_item_state`) is unaffected: incognito only changes who sees activity.

## Kinds

| Kind | Audience | Notes |
| --- | --- | --- |
| Items added (series, season's episodes, movie) | everyone | Grouped per series or movie per scan or import |
| Watched / stopped watching | everyone, or subject when incognito | From play sessions |
| User created, removed, changed (renamed, password reset, made admin) | admins | |
| Login, failed login | admins | Whether to store IP addresses is open |
| Configuration changed | admins | Which fields changed; never the values of secrets (API keys, the TMDB key, webhook secrets, passwords) |
| Task failed or finished | admins | Links to the task inspector (`tasks.md`) |
| Comments, ratings, reviews | everyone | Later |

## Data

- `activity`: `kind`, `at`, `actor_user_id` (nullable), `subject_user_id` (nullable), `library_id` (nullable), `item_id` (nullable), `audience`, `payload` (JSON), `event_ids` (JSON, nullable).
- `play_session`: `user_id`, `item_id`, `started_at`, `last_report_at`, `ended_at`, `position_start`, `position_end`, `incognito`.
- Deleting a user deletes the activity they are the subject of; entries where they were only the actor keep the entry with the actor removed.

## Retention and privacy

- Users can clear their own watch activity.
- Old entries are pruned per kind: watch activity and logins after a retention period (e.g. 90 days), library additions and admin entries kept longer.

## Inspector

The item inspector shows both, as two tabs: **Activity** (readable, filtered by the same rules) and **Events** (the raw outbox for the item, in Manage mode only, for debugging matching and webhooks).

## API and UI

- `GET /activity` with a cursor and filters (kind, library, item, user), filtered per viewer. `DELETE /activity/mine/watch` to clear one's own watch activity (`is_own_change`).
- An Activity screen; a short feed on Home ("Recently added", "Friends are watching").
- Live updates through the existing event stream.

## As built

- **Additions** count an episode or movie when it gets its first file link ever (the first `link.created` for it), so upgrades and second versions aren't news. Additions to a series or movie within 30 minutes of its latest entry grow that entry; `new_series` says none of the series' episodes was linked before. The consumer's cursor is in `event_consumer`; on first run it reads the whole outbox, so existing libraries get their history.
- **Sessions** end after 10 minutes without a report, or at the played threshold (90%); reports through the credits belong to the finished session. Entries need 5 minutes or reaching the end. Quiet sessions are ended by a background worker every minute.
- **Task results**: failures, tasks that finished with a problem, and manual runs. Routine scheduled and watch scans that went fine aren't logged.
- **Configuration**: libraries, roots, API keys and webhooks, with the names of the fields that changed. User changes cover renames, password resets and the admin flag (`user_changed`).
- **Retention**: watching and logins 90 days, everything else a year; pruned hourly.
- **Live updates**: writing or growing an entry emits `activity.added` with the entry's id.
- Changes made with an API key have no actor. Watch mode's Activity screen shows additions and watching only, also for admins.

## Open

- Storing IP addresses for logins (not stored for now).

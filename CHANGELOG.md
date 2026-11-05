# Changelog

Changes that matter to people running Media Cabinet or using its API: features,
fixes, changed behaviour, migrations and anything to do when upgrading. Newest
first. How versions and releases work is in `docs/releasing.md`.

Keep entries brief: one line each, saying what changed for the user, not how.
Details belong in the commit, docs or API reference.

## 1.0.0 (unreleased)

The first release: a self-hosted library manager and streaming server for movies
and TV, with its own web UI. See `docs/roadmap.md` for what v1 covers.

- TVs and apps can sign in with a code approved at `/pair`, for any kind of account, or with a password (`POST /api/auth/pair`, `POST /api/auth/device`); they stay signed in for a year from last use, and Account lists them with sign out (`GET /api/me/devices`).
- The API accepts `Authorization: Bearer` device tokens, `?media_token=` for artwork, streams and subtitles (`POST /api/me/media-token`), and cross-site requests from packaged TV apps (`CABINET_CORS_ORIGINS` adds origins).
- `GET /api/server-info` gives the server's name (`CABINET_SERVER_NAME`), version and minimum app version.
- Clients can report `hdr_formats` and `max_audio_channels` when asking to play; reasons for converting now say "this player".
- If a video won't play, the player tries again in a simpler form instead of stopping, and remembers what this browser couldn't play; the stats panel shows it and can forget it.
- Migration 0021 adds device names to sessions, and media tokens.
- Movies and folder videos with no artwork show a frame from the video, made after a scan or with Generate thumbnails on the library page (`POST /api/libraries/{id}/thumbnails`); it pauses while a converted stream is playing.
- A Tasks tab in Manage shows what's running and what ran, with progress, failures and logs for each task, and can stop one (`GET /api/tasks`, `GET /api/tasks/{id}`, `POST /api/tasks/{id}/cancel`).
- Migrations 0019 and 0020 add thumbnails and task logs.
- New Folders library type: browse and play videos in your folders as they are, with no metadata fetched and nothing on disk changed.
- Converted streams start sooner: the first seconds are sent in 1-second segments, HDR is scaled down before it's tone-mapped, and the player loads its streaming code while the server starts.
- The quality menu shows the resolution of Original.
- The player has a stats panel (press `i`, or Playback stats in the tracks menu): how long each step of starting took, download rate, bitrate, buffer, dropped frames, stalls and, when converting, what it's converted to, ffmpeg's speed and how far ahead it is; admins also see the ffmpeg command. `GET /play/{id}` reports a stream's progress.
- Sign in with Authentik, Keycloak, Pocket ID or any OpenID Connect provider, set up in Manage → Users or with `CABINET_OIDC_*`; new people wait for an admin to let them in unless the provider is set to let them straight in.
- Sign in with GitHub, Discord or any plain OAuth2 provider with a user endpoint, besides OpenID Connect.
- Adding a sign-in provider starts by picking it from a list (Authentik, Keycloak, Google, Microsoft, Apple…), with setup steps for each; Sign in with Apple is supported.
- Migration 0017 records which provider was picked, and Apple's Team ID and Key ID.
- Migration 0016 adds a type to sign-in providers, with endpoints and user fields for plain OAuth2.
- Sign-in providers can be tested before saving: Check settings flags unknown scopes and a wrong client secret, and Try signing in shows what the provider sends back and what would happen.
- Users have an email address, can link and unlink accounts at sign-in providers from Account, and can be turned off instead of removed (`PUT /api/users/{id}/status`).
- Users moved from Manage → Settings to their own Manage → Users tab.
- Migration 0015 adds sign-in providers, linked accounts, and users' email and status.
- A library's metadata can be fetched again in one go from its Metadata panel (`POST /api/libraries/{id}/refresh`), running as a task with progress.
- Cast and crew on movie, series and episode pages (guest stars, directors, writers, creators), with a page per person listing everything they're in across the library.
- Series show their network; movies and series show studios, a tagline and the age rating (`CABINET_TMDB_REGION` picks the country).
- Ratings from TMDB, plus IMDb, Rotten Tomatoes and Metacritic with an OMDb key (`CABINET_OMDB_KEY`); `GET /api/items/{id}/about` and `GET /api/people/{id}` return the new details.
- Migration 0014 adds people, credits, companies and ratings.
- Files in the episode inspector and a movie's Details view can show their probed streams and sidecars; `GET /api/files/{id}/probe` returns them (admin only).
- A series folder whose name has a wrong provider ID (such as `[tmdb-…]`) can be renamed to the series' own from its Location panel; read-only roots ask for confirmation first.
- The review queue can ignore a whole folder (its files now and later), with a list of ignored folders to bring back; clicking an open series again folds it.
- Migration 0013 adds ignored folders.
- A Manage menu for admins, with tabs for an overview, Wanted, the review queue, libraries and settings, replaces the Watch / Manage switch; media pages get a Details toggle instead.
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

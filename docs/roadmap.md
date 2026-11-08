# Media Cabinet — Roadmap to v1

v1 is a self-hosted library manager and streaming server for movies and TV, used through its own web UI. It tracks what you have and want, works with today's \*arrs, and plays your library in the browser.

The web UI is built early and grows with the backend: Phase 2 lays the foundation against seeded data, and every later phase ships the screens for what it adds. Backend phases follow the build order in the handoff README.

Tick items as they land. Open questions from the spec are listed in the phase that needs the answer.

## Phase 0 — Foundations

- [x] Choose the stack (`docs/decisions.md`)
- [x] Scaffold the Cargo workspace, server, web app and Docker image
- [x] Enforce code style (rustfmt, oxfmt, oxlint, pre-commit hook)
- [x] Initial schema migration with constraint tests
- [x] Backup before migrating an existing database
- [x] Fixture library generator (`scripts/make-fixtures.sh`)
- [ ] CI: format, lint, test and build on every push (needs a remote)
- [ ] Choose a licence

## Phase 1 — Scanning core

- [x] Discover: walk roots, skip junk, record files
- [x] File identity across renames, swaps and cross-device moves (inode, path, content hash)
- [x] Missing and restored files; unreachable, empty and unreadable roots change nothing
- [x] Probe: ffprobe streams into `media_stream` for new and changed files
- [x] Sidecars: associate subtitles, `.nfo` and artwork with their videos

## Phase 2 — Web UI foundation

Real API, seeded data. Seeing and testing the design early.

- [x] Dev seed command: the mockups' sample series, episodes, orderings, files and watch state
- [x] Read API for the UI: series list, series detail with episodes per ordering, home rails
- [x] Simple availability and next up in the read API (replaced by Phase 5's derived views)
- [x] Routing and data fetching conventions; API types generated from OpenAPI
- [x] App shell: sidebar, Media Cabinet branding, a Manage menu for admins with a tab per section, and a remembered Edit mode switch on media pages
- [x] Shared components: segmented controls, state badges, cards, rails, posters, progress bars
- [x] Series page: hero (Watch and Manage), ordering switch, wording per mode
- [x] Series page, Manage: counts per state, technical lines, monitor checkboxes, gaps
- [x] Home: hero, keep watching, coming this week, TV shows; needs attention is the Manage overview
- [x] Responsive layout down to phone width; focus states

## Phase 3 — Library management

- [x] Library and root management API (create, edit, remove; write policy; import target)
- [x] Layout templates: parse and validate, and propose one from existing folders
- [x] Group: find each file's series folder from the template; grouping levels as hints
- [x] Scan orchestration: one scan per root at a time, scheduled scans, scan on demand
- [x] Filesystem watching (notify) to rescan changed paths quickly
- [x] Large-library test: scan time and memory for ~50k files (`docs/benchmarks.md`)
- [x] UI: Libraries and roots (write policy, coloured template, offline state, "where would a new file go")
- [x] UI: scan progress and results
- [x] Folder libraries: browse folders as they are, no metadata (`docs/folder-libraries.md`)

## Phase 4 — Metadata and matching

- [x] Provider client: TMDB (series, seasons, episodes, episode groups, movies)
- [x] Import a series: items for every episode, orderings (aired default, DVD, absolute), external IDs
- [x] Import a movie
- [x] Cross-reference secondary provider IDs where they map cleanly
- [x] Refresh: match by primary ID, reconcile by position, title and date, never duplicate
- [x] Refresh: new episodes, provider-deleted episodes flagged (`removed_upstream_at`), locked fields kept
- [x] Refresh scheduling: ended and cancelled series less often
- [x] Offline behaviour: cached metadata, refresh retries when providers are unreachable
- [x] Item tags from external sources (filler lists), set through `PUT /items/{id}/tags`
- [x] Identify series: explicit ID in path, `.nfo`, provider search with year hints
- [x] Folder → series proposals in the review queue when unsure
- [x] Filename parser (S/E, `1x01`, absolute, multi-episode, year, edition, part), test-first on a name corpus
- [x] Resolve parsed numbers against orderings (root or series setting, default, then others)
- [x] Sanity checks: runtime vs expected, episode title in the filename
- [x] Accept above threshold as `file_link`, otherwise `match_candidate`
- [x] Movies: editions, multi-part files, versions (4K and 1080p of one film)
- [x] Extras and junk: samples, trailers, featurettes ignored or linked as extras
- [x] Pinned links and user decisions survive every rescan
- [x] Review queue actions: accept, accept and pin, reject, pick another candidate, ignore file or folder
- [x] Decide: match confidence thresholds global, per root or per source
- [x] UI: Review queue (path segments by template token, fact tiles, candidates with confidence)
- [x] UI: Series Manage panels for metadata and orderings (edit details, lock fields, change match)
- [x] UI: Movies library and movie page
- [x] UI: real artwork (hero backdrop with scrim, posters, thumbnails)
- [x] UI: search

## Phase 5 — Derived state

- [x] Availability per item (available, missing, not acquired, upcoming, unknown)
- [x] Monitor inheritance (series → episode) and quality profiles
- [x] Wanted: monitored and not available, or below cutoff; `item.wanted` and `item.unwanted` events
- [x] Gaps within an ordering or playlist
- [x] Next airing, in the user's time zone
- [x] Custom status from external tools, with expiry and stale display
- [x] Decide: own quality profiles fully, or share with the \*arrs
- [x] Decide: can wanted follow a playlist (monitor only no-filler episodes)
- [x] UI: switch the Series and Home pages to the derived views
- [x] UI: Series Manage panel for monitoring (quality profile, wanted follows)
- [x] UI: Wanted and Calendar screens

## Phase 6 — Integration API and events

- [x] Item addressing: `item:<id>` and provider position (`tvdb:123`, ordering, S02E05)
- [x] `GET /wanted`, `GET /items/{ref}`, `GET /series/{ref}/gaps`, `GET /calendar`
- [x] `PUT /items/{ref}/status/{source}`
- [x] `POST /series` (add by provider ID) and the movie equivalent
- [x] `GET /events` stream (SSE) from a cursor
- [x] Webhooks: subscribe, deliver with retries, resume from cursor, signed payloads
- [x] \*arr compatibility: `/wanted` in the custom import list format
- [x] \*arr compatibility: accept Connect webhooks and rescan affected paths
- [x] `POST /imports`: match, place by write policy, link
- [x] Placement: resolve `series_location` once; adaptive and managed policies; read-only roots never written
- [x] Managed roots: rename and move existing files to the template
- [x] Move a series between roots or folders (explicit action)
- [x] Fix a wrong provider ID in a series folder's name (read-only roots only when confirmed)
- [x] API keys for external tools
- [x] Decide: write `.nfo` or embedded tags back out (default or opt-in): not in v1; opt-in later
- [x] UI: Series Manage panel for location (move series)
- [x] UI: live updates from the event stream
- [x] UI: Settings for API keys and webhooks

## Phase 7 — Users and playback

- [x] Local users and login (no external account), admin role
- [x] Watch state: position, played, play count; mark watched or unwatched
- [x] Per-user ordering or playlist for each series; next up
- [x] Playlists: manual and smart (no filler)
- [x] Direct play over HTTP range requests
- [x] Client capability detection (codecs, HDR, containers) to choose direct play or transcode
- [x] Transcoding to HLS with ffmpeg, including HDR tone mapping
- [x] Version picker, audio and subtitle tracks (embedded and sidecar)
- [x] Hardware acceleration (VAAPI, NVENC, QSV) where available: encoding only, opt-in, with a self-test that falls back to software. Only the fallback is verified so far; no working GPU to test on yet
- [x] Decide: remote access via the user's reverse proxy or VPN, or an optional relay (decided: reverse proxy or VPN; see decisions)
- [x] UI: login and user management
- [x] UI: Player (version and track picker, direct play or transcode hint, resume)
- [x] UI: Player stats (download rate, bitrate, buffer, frames, stalls; ffmpeg speed and command from `GET /play/{id}`)
- [x] UI: playlists, mark watched

## Phase 8 — Tasks and activity

Designs: `docs/tasks.md`, `docs/activity.md`.

- [x] Task runner extracted from the scan service; scans become the first task kind (`scan_run` → `task`)
- [x] Metadata refresh of a whole library on request, as a task (`metadata_refresh`, library scope)
- [ ] Metadata refresh as a scheduled task
- [x] Task outcomes and task-scoped logs (tracing span per task)
- [x] Heavy lane: one task at a time, waits while transcodes run
- [x] Thumbnails task
- [ ] Trickplay task
- [ ] Storage settings: artwork quality and generated thumbnails, each can be turned off (default: high quality, thumbnails on; `docs/metadata-storage.md`)
- [x] UI: Tasks tab in Manage and task inspector
- [x] Activity: table, per-viewer audience filter, library additions from the event outbox
- [x] Play sessions and watch activity; incognito setting
- [x] Activity for users, logins, configuration changes and task results (admins)
- [x] UI: Activity screen, Home feed, inspector Activity and Events tabs

## Sign-in with other accounts

Design: `docs/sign-in.md`.

- [x] OIDC code flow with PKCE, written by hand; HTTPS issuers only
- [x] Any number of providers, from the UI or `CABINET_OIDC_*`; env providers read-only until taken over
- [x] New people pending until an admin lets them in, or let straight in; allowed groups and an admin group
- [x] "Is this you?" for a verified email that matches an account; linking and unlinking from Account
- [x] Users turned off instead of removed; Manage → Users tab with a waiting count
- [x] Checking and trying a provider's settings before saving
- [x] GitHub, Discord and other plain OAuth2 providers
- [x] A list of known providers with setup steps for each, Sign in with Apple included
- [ ] Library access from groups (waits for per-user library scoping)

## Device players

Samsung and LG TVs, Android, Android TV, iOS and Apple TV, for browsing and watching. Designs: `docs/clients.md`, `docs/design-language.md`.

### Pre-work

- [x] Decide: the apps follow v1 as 1.x; Android might be ready for 1.0, but 1.0 doesn't wait for it
- [x] Decide: Samsung and LG apps are packaged, not loaded from the server
- [x] Decide: one small Preact app for all Samsung and LG TVs, which is also the reference for Android TV and tvOS
- [x] Folders with READMEs: `players/` (with `players/web`, the shared TV app), `design/`, `web/src/core/`
- [ ] Choose a licence with the App Store in mind (same item as Phase 0)
- [x] Device tokens: `Authorization: Bearer`, device names, a year from last use
- [x] Pairing: start and poll from the app, approve on the web at `/pair`, for passwords and every provider
- [x] UI: signed-in devices in Account, with sign out
- [x] Capabilities: HDR formats and audio channels; remuxing honours `audio_codecs`
- [x] Play reasons worded for any client, not just browsers
- [x] `GET /api/server-info`: name, version, minimum app version
- [x] Rule for API changes: additive, removals wait for a minimum version
- [ ] Generate Swift and Kotlin API clients from the OpenAPI spec
- [ ] Git remote and macOS CI: build, sign, upload to TestFlight
- [ ] Spike on a rented Mac: AVPlayer plays direct, remuxed and transcoded streams with the cookie, on an iPhone and an Apple TV
- [ ] Spike: Media3 plays the same streams on an Android phone and TV
- [ ] Register as a seller with Samsung and LG, and find out which TVs a new app can reach; this sets the store floor
- [ ] Spike: an ES5 build with `@vitejs/plugin-legacy` plays a stream using Preact, the playback session and hls.js, on the oldest TV available (goal: Samsung 2017, LG 2016); sets how far back the app runs. The app for it is built (`players/web`, "About this TV" shows what the TV reports); running it on TVs remains
- [ ] Spike: a packaged app on a Samsung and an LG TV reaches a plain HTTP server and plays direct, remuxed and transcoded streams, with HDR on newer models; compare `<video>` and AVPlay on Samsung; note what `Origin` each sends, and narrow the CORS list to that
- [ ] Read Samsung's and LG's store requirements, including apps that need a person's own server
- [x] CORS on `/api` without credentials, for packaged TV apps
- [x] Media tokens: short-lived, read-only, media paths only
- [x] Fall back when playback fails: ask again without what failed, down to H.264 and AAC, and remember it per device
- [x] Design tokens: `design/tokens.json`, generated CSS, Swift and Kotlin (`docs/design-language.md`)

### Samsung and LG TVs

- [ ] Web: a configurable server address behind one base URL
- [x] `players/web` project: ES5 build, polyfills, fixed-value CSS, `@core` alias, a check that fails on CSS older TVs can't use, and lint and format in the pre-commit hook
- [ ] Web: `src/core/` with the API client, playback session, fallback, capabilities and wording; a lint rule keeps React and CSS out
- [ ] Try hosting the TV build at `/tv` on the server, for TV browsers with no install
- [x] Spatial focus navigation and remote keys, and LG's Magic Remote pointer
- [ ] TV: connect to a server, pairing by code or QR code
- [ ] TV: home, shows, movies, series and episode pages, search
- [x] TV: player with versions, tracks, subtitles, resume, next episode
- [ ] `players/webos`: manifest, icons, platform adapter (capabilities, HDR, keys), packaging
- [ ] `players/tizen`: manifest, icons, platform adapter (capabilities, HDR, keys), packaging

### Android and Android TV

- [ ] Project in `players/android`: API client, data layer, playback logic
- [ ] Connect to a server, including plain HTTP on a home network
- [ ] Sign in: password, pairing through the browser
- [ ] Phone: home, shows, movies, series and episode pages, search
- [ ] Phone: player with custom controls, versions, tracks, subtitles, resume, next episode
- [ ] Recover from ended sessions and lost connections
- [ ] TV: pairing by code or QR code
- [ ] TV: the same screens with D-pad focus and scrubbing

### iOS and Apple TV

- [ ] Project in `players/apple`: API client, data layer, playback logic
- [ ] Connect to a server: home network, Tailscale, HTTPS
- [ ] Sign in: password, pairing through the browser
- [ ] iPhone and iPad: home, shows, movies, series and episode pages, search
- [ ] iPhone and iPad: player with custom controls, versions, tracks, subtitles drawn by the app, resume, next episode
- [ ] AirPlay, picture in picture, background audio
- [ ] Recover from ended sessions and lost connections
- [ ] Apple TV: pairing by code or QR code
- [ ] Apple TV: the same screens with focus and Siri Remote scrubbing

### Testing and stores

- [ ] Playback test on real devices: codecs, HDR, surround, subtitles, long pauses, lost connections
- [ ] Remote access test through a reverse proxy
- [ ] Public demo server with openly licensed films, for store review
- [ ] App icons, screenshots and store listings (`docs/brand/`)
- [ ] Google Play: closed test with 12 testers for 14 days, then production
- [ ] App Store: TestFlight beta, then review for iOS and tvOS
- [ ] Samsung and LG: seller registration, then store review

## Phase 9 — Release

- [ ] Keyboard navigation and accessibility pass across all screens
- [x] Episode inspector (every link, number in each ordering, event history)
- [x] Stream inspector: a file's probed streams and sidecars (`GET /files/{id}/probe`)
- [ ] Published Docker image and release binaries
- [ ] Smaller image (static ffmpeg)
- [ ] Upgrade test: migrate a large database from an older version, with backup
- [ ] Install, configuration and \*arr setup docs
- [ ] API reference from the OpenAPI spec

## Later stage — more metadata providers

TMDB is v1's only provider. These plug in behind the same `Provider` interface.

- [ ] TVDB client (season types as orderings; needs a subscriber or project key)
- [ ] AniDB client (its own episode numbering as an ordering)
- [ ] Choosing the primary provider per library or series

## Later stage — people and richer metadata

Design: `docs/people.md`.

- [x] People and credits from TMDB: cast, guest stars, key crew, creators
- [x] Networks, studios, tagline, age rating per region
- [x] Ratings: TMDB, and IMDb, Rotten Tomatoes and Metacritic from OMDb
- [x] UI: cast rails, crew and companies on media pages; a page per person
- [ ] Search people by name
- [ ] Choose artwork in Edit mode: pick a poster or backdrop from all of TMDB's images for a movie or series, kept across refreshes like a locked field (TMDB's top backdrop for many movies is the poster's key art laid out wide)

## After v1

- Jellyfin-compatible API so existing apps work
- Optional AI matching (embeddings for fuzzy titles, a local LLM as tie-breaker)
- Light theme
- Music and books
- Localized titles, collections, per-user library scoping
- A "work" identity built from external IDs, shared by the copies of a title in different libraries: watch history synced across libraries, and a stable key for third-party integrations. Watch history stays on `item_id` until then.

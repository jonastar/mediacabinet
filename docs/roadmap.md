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
- [x] App shell: sidebar, Media Cabinet branding, app-wide Watch / Manage mode for admins (remembered)
- [x] Shared components: segmented controls, state badges, cards, rails, posters, progress bars
- [x] Series page: hero (Watch and Manage), ordering switch, list and grid (remembered), wording per mode
- [x] Series page, Manage: counts per state, technical lines, monitor checkboxes, gaps
- [x] Home: hero, keep watching, coming this week, TV shows; Manage adds needs attention
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

## Phase 4 — Metadata and matching

- [ ] Provider client: TMDB (series, seasons, episodes, episode groups, movies)
- [ ] Provider client: TVDB (season types as orderings)
- [ ] Import a series: items for every episode, orderings (aired default, DVD, absolute), external IDs
- [ ] Import a movie
- [ ] Cross-reference secondary provider IDs where they map cleanly
- [ ] Refresh: match by primary ID, reconcile by position, title and date, never duplicate
- [ ] Refresh: new episodes, provider-deleted episodes flagged (`removed_upstream_at`), locked fields kept
- [ ] Refresh scheduling: ended and cancelled series less often
- [ ] Offline behaviour: cached metadata, refresh retries when providers are unreachable
- [ ] Item tags from external sources (filler lists)
- [ ] AniDB support (decide: v1 or later)
- [ ] Identify series: explicit ID in path, `.nfo`, provider search with year hints
- [ ] Folder → series proposals in the review queue when unsure
- [ ] Filename parser (S/E, `1x01`, absolute, multi-episode, year, edition, part), test-first on a name corpus
- [ ] Resolve parsed numbers against orderings (root or series setting, default, then others)
- [ ] Sanity checks: runtime vs expected, episode title in the filename
- [ ] Accept above threshold as `file_link`, otherwise `match_candidate`
- [ ] Movies: editions, multi-part files, versions (4K and 1080p of one film)
- [ ] Extras and junk: samples, trailers, featurettes ignored or linked as extras
- [ ] Pinned links and user decisions survive every rescan
- [ ] Review queue actions: accept, accept and pin, reject, pick another candidate, ignore file
- [ ] Decide: match confidence thresholds global, per root or per source
- [ ] UI: Review queue (path segments by template token, fact tiles, candidates with confidence)
- [ ] UI: Series Manage panels for metadata and orderings (edit details, lock fields, change match)
- [ ] UI: Movies library and movie page
- [ ] UI: real artwork (hero backdrop with scrim, posters, thumbnails)
- [ ] UI: search

## Phase 5 — Derived state

- [ ] Availability per item (available, missing, not acquired, upcoming, unknown)
- [ ] Monitor inheritance (series → episode) and quality profiles
- [ ] Wanted: monitored and not available, or below cutoff; `item.wanted` and `item.unwanted` events
- [ ] Gaps within an ordering or playlist
- [ ] Next airing, in the user's time zone
- [ ] Custom status from external tools, with expiry and stale display
- [ ] Decide: own quality profiles fully, or share with the \*arrs
- [ ] Decide: can wanted follow a playlist (monitor only no-filler episodes)
- [ ] UI: switch the Series and Home pages to the derived views
- [ ] UI: Series Manage panel for monitoring (quality profile, wanted follows)
- [ ] UI: Wanted and Calendar screens

## Phase 6 — Integration API and events

- [ ] Item addressing: `item:<id>` and provider position (`tvdb:123`, ordering, S02E05)
- [ ] `GET /wanted`, `GET /items/{ref}`, `GET /series/{ref}/gaps`, `GET /calendar`
- [ ] `PUT /items/{ref}/status/{source}`
- [ ] `POST /series` (add by provider ID) and the movie equivalent
- [ ] `GET /events` stream (SSE) from a cursor
- [ ] Webhooks: subscribe, deliver with retries, resume from cursor, signed payloads
- [ ] \*arr compatibility: `/wanted` in the custom import list format
- [ ] \*arr compatibility: accept Connect webhooks and rescan affected paths
- [ ] `POST /imports`: match, place by write policy, link
- [ ] Placement: resolve `series_location` once; adaptive and managed policies; read-only roots never written
- [ ] Managed roots: rename and move existing files to the template
- [ ] Move a series between roots or folders (explicit action)
- [ ] API keys for external tools
- [ ] Decide: write `.nfo` or embedded tags back out (default or opt-in)
- [ ] UI: Series Manage panel for location (move series)
- [ ] UI: live updates from the event stream
- [ ] UI: Settings for API keys and webhooks

## Phase 7 — Users and playback

- [ ] Local users and login (no external account), admin role
- [ ] Watch state: position, played, play count; mark watched or unwatched
- [ ] Per-user ordering or playlist for each series; next up
- [ ] Playlists: manual and smart (no filler)
- [ ] Direct play over HTTP range requests
- [ ] Client capability detection (codecs, HDR, containers) to choose direct play or transcode
- [ ] Transcoding to HLS with ffmpeg, including HDR tone mapping
- [ ] Version picker, audio and subtitle tracks (embedded and sidecar)
- [ ] Hardware acceleration (VAAPI, NVENC, QSV) where available
- [ ] Decide: remote access via the user's reverse proxy or VPN, or an optional relay
- [ ] UI: login and user management
- [ ] UI: Player (version and track picker, direct play or transcode hint, resume)
- [ ] UI: playlists, mark watched

## Phase 8 — Release

- [ ] Keyboard navigation and accessibility pass across all screens
- [ ] Episode inspector (every link, number in each ordering, event history)
- [ ] Published Docker image and release binaries
- [ ] Smaller image (static ffmpeg)
- [ ] Upgrade test: migrate a large database from an older version, with backup
- [ ] Install, configuration and \*arr setup docs
- [ ] API reference from the OpenAPI spec

## After v1

- Native and TV clients; Jellyfin-compatible API so existing apps work
- Optional AI matching (embeddings for fuzzy titles, a local LLM as tie-breaker)
- Light theme
- Music and books
- Localized titles, people and credits, collections, per-user library scoping

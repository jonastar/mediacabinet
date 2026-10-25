# Media Cabinet — Roadmap to v1

v1 is a self-hosted library manager and streaming server for movies and TV, used through its own web UI. It tracks what you have and want, works with today's \*arrs, and plays your library in the browser.

Phases follow the build order in the handoff README: each one builds on the one before. Later phases can start early where they don't depend on unfinished work (for example, UI shells against stub data).

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

## Phase 1 — Library and scanning

- [x] Discover: walk roots, skip junk, record files
- [x] File identity across renames, swaps and cross-device moves (inode, path, content hash)
- [x] Missing and restored files; unreachable, empty and unreadable roots change nothing
- [ ] Probe: ffprobe streams into `media_stream` for new and changed files
- [ ] Sidecars: associate subtitles, `.nfo` and artwork with their videos
- [ ] Library and root management (create, edit, remove; write policy; import target)
- [ ] Layout templates: parse and validate, and propose one from existing folders
- [ ] Group: find each file's series folder from the template; grouping levels as hints
- [ ] Scan orchestration: one scan per root at a time, scheduled scans, scan on demand
- [ ] Filesystem watching (notify) to rescan changed paths quickly
- [ ] Large-library test: scan time and memory for ~50k files

## Phase 2 — Metadata

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

## Phase 3 — Matching and review

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

## Phase 4 — Derived state

- [ ] Availability per item (available, missing, not acquired, upcoming, unknown)
- [ ] Monitor inheritance (series → episode) and quality profiles
- [ ] Wanted: monitored and not available, or below cutoff; `item.wanted` and `item.unwanted` events
- [ ] Gaps within an ordering or playlist
- [ ] Next airing, in the user's time zone
- [ ] Custom status from external tools, with expiry and stale display
- [ ] Decide: own quality profiles fully, or share with the \*arrs
- [ ] Decide: can wanted follow a playlist (monitor only no-filler episodes)

## Phase 5 — Integration API and events

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

## Phase 6 — Users and playback

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

## Phase 7 — Web UI

- [ ] App shell: sidebar, Media Cabinet branding, app-wide Watch / Manage mode for admins
- [ ] Shared components: segmented controls, state badges, cards, rails, posters
- [ ] Series page: hero (Watch and Manage), orderings, list and grid, monitor checkboxes, gaps
- [ ] Series page, Manage panels: monitoring, metadata, orderings, location
- [ ] Home: hero, needs attention, keep watching, coming this week, TV shows
- [ ] Movies: library and movie page
- [ ] Search
- [ ] Review queue
- [ ] Libraries and roots (write policy, template, offline state, "where would a new file go")
- [ ] Wanted and Calendar screens
- [ ] Player: version and track picker, direct play or transcode hint, resume
- [ ] Real artwork: hero backdrop with scrim, posters, thumbnails
- [ ] Settings: users, providers, API keys, webhooks
- [ ] Focus states and keyboard navigation
- [ ] Episode inspector (every link, number in each ordering, event history)

## Phase 8 — Release

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

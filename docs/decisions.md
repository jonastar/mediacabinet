# Media Cabinet — Tech decisions

Oct 25, 2026 · Jonas

Settles the stack and the build-blocking open questions in `media-cabinet-handoff/docs/spec.md`.

## Decided

| Area | Decision |
| --- | --- |
| Backend language | Rust |
| Database | SQLite only for v1, WAL mode. Keep SQL portable so Postgres can be added later. |
| Frontend | React + TypeScript, no meta-framework. Vite as the build tool. |
| API contract | OpenAPI. The frontend's TypeScript types are generated from it. |
| Streaming | In v1, with our own player. Web player first. |
| Deployment | Both: a single binary with the UI embedded, and a Docker image that adds ffmpeg. |

## Data model

- Seasons are not items; they belong to an ordering (`ordering_season`). Season-level actions (monitor a season, mark it watched) are bulk actions on that season's episodes in a given ordering. Accepted trade-off: nothing can be stored on a season across orderings, and a rule like "monitor future episodes of Season 3" isn't supported. Add a season-scoped rule on `ordering_season` if that's ever needed.

## Metadata and matching

- TMDB is v1's only metadata provider (`CABINET_TMDB_KEY`, read access token preferred). TVDB and AniDB are a separate stage after v1, behind the same `Provider` interface. An offline catalog (`CABINET_METADATA_CATALOG`) stands in for tests and trying the app without a key.
- Providers that only know an air date (TMDB) store `airs_at` as the end of that day in UTC with `airs_tz` null, so nothing counts as aired early. The UI shows a date without a time when `airs_tz` is null.
- When a provider offers no absolute ordering, one is derived from aired order (source `computed`), because anime releases are numbered absolutely.
- Match confidence threshold: one global value, 0.85 (`CABINET_MATCH_THRESHOLD`). Per-root or per-source thresholds can come later if needed. Ambiguous search results (two candidates within 0.1) are capped below the threshold so they always go to review.
- Samples, trailers and other extras are marked `ignored` by the matcher; they aren't linked as extras yet.
- Matching uses the provider's search API, not TMDB's daily ID exports: the exports hold only the original-language title and popularity (no year, no translations), so they can't identify most folders. They may later serve as an offline ID check or popularity tie-breaker.
- Write transactions use `BEGIN IMMEDIATE` (`db::begin_write`) so concurrent scans wait for the lock instead of failing.

## Wanted and quality

- Availability, resolved monitoring and wanted are SQL views (`item_state`, `item_wanted`, migration 0004): one definition for every screen and the API. Gaps and up next depend on an ordering and stay in Rust (`catalog::rules`).
- Quality profiles are ours but deliberately small: a resolution cutoff (`{"cutoff": "1080p"}`). Available files below it are wanted as upgrades; heights are compared with a 0.7 allowance so wide films (1920×800) count as 1080p. The \*arrs keep their own detailed profiles for choosing releases; `/wanted` reports our cutoff so they can map it.
- Wanted can follow a playlist (e.g. no filler): a series' `wanted_playlist_id` limits wanted to that playlist's episodes. Monitoring stays per item.
- `item.wanted` / `item.unwanted` events come from diffing `item_wanted` against `wanted_snapshot`, right after changes that affect it and every minute for changes time makes (episodes airing).

## Writing to disk

- Imports hard-link by default (download clients keep seeding), falling back to a copy across filesystems; copy and move are options. Sidecars with the same name travel along.
- Nothing is overwritten: a taken destination stops the operation. Across filesystems, moves copy, check the size, then remove the original.
- Renaming files on managed roots (Organize) and moving a series are explicit actions with a dry run, never automatic on scan.
- Replacing an older version on import only happens when asked (`replace`); the old file is marked `removed`, not `missing`.
- Media Cabinet doesn't write `.nfo` files or embedded tags back out in v1. Writing them, opt-in, can come later.
- Integration endpoints meant for tools (*arr lists and webhooks, imports) require an API key now. The rest require a session or key once logins exist (Phase 7).

## Backend stack

| Concern | Choice | Notes |
| --- | --- | --- |
| Async runtime | tokio | |
| HTTP | axum + tower-http | `ServeFile` handles range requests for direct play. SSE for `GET /events`. |
| Database access | sqlx (SQLite) | Raw SQL checked at compile time; derived views stay as real SQL. |
| Migrations | sqlx migrations, embedded in the binary | Copy the database file before each migration run (spec: backup before migrating). |
| OpenAPI | utoipa (code-first) | Spec served at `/api/openapi.json`; `openapi-typescript` generates frontend types. |
| Media probing | ffprobe / ffmpeg as subprocesses (`tokio::process`) | No FFmpeg bindings. |
| Transcoding | ffmpeg → HLS when the client can't direct-play | |
| File identity | device + inode, then a partial content hash (blake3 over head, tail and size) | Full hash only when needed. |
| Filesystem watching | notify | Scheduled full scans stay the source of truth. |
| Filename parsing | Own parser module, built test-first against a corpus of real filenames | Look at existing Rust crates first. The layout template supplies structure, so the parser only extracts S/E, absolute number, year, edition. |
| Logging | tracing | |
| Errors | thiserror in library code, anyhow at the edges | |
| Static UI | Built frontend embedded with rust-embed | Ships as one binary. |

## Frontend stack

- React + TypeScript, built with Vite.
- Plain CSS; the design tokens from `design.md` become CSS variables on `:root`.
- Native `<video>` for direct play; hls.js for HLS wherever Media Source exists (Chrome claims native HLS but can't play these streams), native HLS only without it.
- Keep the number of dependencies small. Add data fetching or routing libraries only when they're needed.

## Users and playback

- **Signing in with other accounts.** OpenID Connect providers, any number, from the UI or `CABINET_OIDC_*` (design: `docs/sign-in.md`). Written by hand, without an OIDC or JWT crate: ID tokens are only taken from the token endpoint over TLS, which OIDC Core §3.1.3.7 accepts in place of a signature check, so there are no signing keys to handle. Issuers must be HTTPS (or loopback). New people wait for an admin unless the provider lets them straight in.
- **Accounts.** Local accounts, Argon2id passwords, and accounts at sign-in providers linked to them. The first run asks for the admin's name and password. Sessions are an HttpOnly, SameSite=Lax cookie holding a random token; only its SHA-256 is stored, and a session lasts 30 days from its last use. A new password signs the user out everywhere else.
- **Who may do what.** Every `/api` request needs a session or an API key, apart from health, the login endpoints and the OpenAPI document. Changes need an admin unless they are the user's own: their account, watch state, series views, playlists and playback. A few reads also need an admin: users, API keys, webhooks, the review queue and browsing folders. The rule is one deny-by-default function (`crates/server/src/api/auth.rs`), so a new endpoint is admin-only until it's listed as a user's own. API keys act as an admin, and per-user state they touch belongs to the first admin.
- **Watch state.** Past 90% of the duration counts as played: the play count goes up once and the resume point clears. Under 30 seconds isn't kept as a resume point.
- **Playback.** The browser reports which codecs, containers and HDR it supports. The server then picks one of three ways to play a file. It plays directly (range requests) when everything fits. It repackages to HLS when only the container or audio doesn't fit. Otherwise it converts the video to H.264 in HLS: for an unsupported codec, a lower quality, HDR on an SDR screen (tone-mapped), or picture subtitles (burned in). Text subtitles are always WebVTT for the player to draw. An HLS session can start partway into the file, and seeking past what's converted starts a new one there. ffmpeg is paused when it's three minutes ahead of the player, and a session nobody uses for a minute is ended.
- **Hardware encoding.** Hardware encoding (`CABINET_HWACCEL`: VAAPI, NVENC or QSV) is opt-in and only covers the encode. Decoding and filters stay in software, so everything ffmpeg can read still works. At startup a short test encode checks the hardware, and the server falls back to software if it fails. H.264 is the only output for now. A browser without H.264 can only play files directly.
- **Remote access.** Use your own reverse proxy (Caddy, nginx, Traefik) or a VPN (WireGuard, Tailscale). There is no relay service in v1: it would need infrastructure someone has to run, and it would route everyone's media through it. Behind HTTPS the session cookie is marked Secure (from `X-Forwarded-Proto`).

## Still open

- Licence.
- The rest of the spec's open questions (quality profile ownership, confidence thresholds, wanted following a playlist, writing `.nfo` back out). None of them block the schema and scanner work.

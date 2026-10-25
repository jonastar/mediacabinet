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

- TMDB is the first metadata provider (`CABINET_TMDB_KEY`). TVDB and AniDB plug in behind the same `Provider` later. An offline catalog (`CABINET_METADATA_CATALOG`) stands in for tests and trying the app without a key.
- Providers that only know an air date (TMDB) store `airs_at` as the end of that day in UTC with `airs_tz` null, so nothing counts as aired early. The UI shows a date without a time when `airs_tz` is null.
- When a provider offers no absolute ordering, one is derived from aired order (source `computed`), because anime releases are numbered absolutely.
- Match confidence threshold: one global value, 0.85 (`CABINET_MATCH_THRESHOLD`). Per-root or per-source thresholds can come later if needed. Ambiguous search results (two candidates within 0.1) are capped below the threshold so they always go to review.
- Samples, trailers and other extras are marked `ignored` by the matcher; they aren't linked as extras yet.
- Write transactions use `BEGIN IMMEDIATE` (`db::begin_write`) so concurrent scans wait for the lock instead of failing.

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
- Native `<video>` for direct play; hls.js where native HLS is not available.
- Keep the number of dependencies small. Add data fetching or routing libraries only when they're needed.

## Still open

- Licence.
- The rest of the spec's open questions (remote access, quality profile ownership, confidence thresholds, wanted following a playlist, writing `.nfo` back out). None of them block the schema and scanner work.

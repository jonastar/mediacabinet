# Media Cabinet — Spec

Oct 25, 2026 · Jonas

## Overview

A self-hosted system that is the single source of truth for a media library: what exists, what you have, what you want, and where every file lives. It also streams that library to users.

Today that knowledge is split. Sonarr and Radarr track what you want and have; Plex and Jellyfin track what is in the library. They sync through filesystem scans and webhooks, and drift apart. This system owns one model, and other tools plug into it.

**Goals**

- One library model for movies and TV: items, files, orderings, availability and wanted state.
- Accept existing libraries as they are, without forcing a rename.
- Know where new files belong, so imports, subtitles and post-processing have a home.
- Rich APIs and events so acquisition tools (the \*arrs) integrate deeply.
- Stream to users, with watch state that follows their chosen ordering or playlist.
- Work fully offline. No account, no telemetry by default.

**Non-goals**

- No downloading, indexer or torrent/usenet functionality. Acquisition stays in external tools.
- Music and books are out of scope for v1. They get their own models later.
- Native client apps are not a v1 deliverable (see Lessons and open questions).

## Design principles

1. **Strict model, flexible filesystem.** The internal model never depends on where a file sits. Folder layout is a matching hint and, optionally, a placement rule.
2. **Items come from metadata, not files.** Every episode a provider knows about exists as an item, owned or not. Files attach to items.
3. **Structure lives in orderings.** Seasons and episode numbers belong to an ordering (aired, DVD, absolute…), not to the item.
4. **Derive state, don't store it.** Availability, wanted, gaps and next airing are computed from links, file status and dates. No flags to drift.
5. **Never destroy data because a file went away.** Missing files keep their links, watch history and pinned matches.
6. **User decisions win.** Pinned matches and locked metadata survive every rescan and refresh.
7. **Explain every match.** Each link records its source and confidence. Uncertain matches go to review, never silently in.
8. **Events out, APIs in.** Every state change emits an event; external tools act through the API, never the database.

## Core concepts

The model has two sides: the world (what exists, from metadata) and storage (what is on disk). Links join them.

| Term | Meaning |
| --- | --- |
| Library | A collection of one media kind (movies or TV). |
| Library root | A folder in a library, with a write policy and a layout template. |
| Item | A movie, series or episode. Identity and metadata only, no position. |
| Media file | A file on disk, identified by inode or content hash rather than path. |
| Link | Ties a file to an item, with edition, confidence and source. Many-to-many. |
| Ordering | A way of numbering a series' episodes: aired, DVD, absolute, story, custom. Owns seasons. |
| Playlist | A curated or filtered sequence of items, which may skip items or span series. |
| Series location | The folder a series lives in on a root. Resolved once and stored. |
| Availability | Derived per item: available, missing, not acquired, upcoming, unknown. |
| Wanted | Derived: monitored and not available, or below the quality cutoff. |
| Custom status | Free-text state written by an external tool, with an expiry. |

```
  STORAGE (what is on disk)        BRIDGE                    WORLD (from metadata)
  ┌──────────────────┐      ┌──────────────────┐      ┌──────────────────────┐
  │ Library root     │──────│ Series location  │──────│ Series (item)        │
  │ policy, template │      │ folder, once     │      │ primary provider     │
  └────────┬─────────┘      └──────────────────┘      └──────────┬───────────┘
           │                                                      │
  ┌────────┴─────────┐      ┌──────────────────┐      ┌──────────┴───────────┐
  │ Media file       │══════│ File link        │══════│ Episode / movie      │
  │ inode / hash     │      │ edition, conf,   │      │ (item) identity,     │
  └────────┬─────────┘      │ source, pin      │      │ metadata             │
           │                └──────────────────┘      └──────────┬───────────┘
  ┌────────┴─────────┐      ┌──────────────────┐      ┌──────────┴───────────┐
  │ Streams,sidecars │      │ Match candidate  │      │ Ordering             │
  │ ffprobe, folder  │      │ review queue     │      │ seasons + positions  │
  └──────────────────┘      └──────────────────┘      └──────────────────────┘
```

Storage never references items directly. Every file reaches an item through a link; match candidates are review-queue proposals that become links once accepted.

## Data model: storage

Storage tables describe what is on disk. A file's identity survives renames and moves, so its links and history do too.

```
library
  id, name
  kind                          -- movies | tv

library_root
  id, library_id, path
  write_policy                  -- managed | adaptive | read_only
  layout_template               -- "{year}/{genre}/{series}/Season {season:00}/{filename}"
  is_import_target              -- where new series land by default

media_file
  id, root_id, relative_path
  size, mtime
  device_id, inode              -- fast identity
  content_hash                  -- fallback identity, survives moves
  status                        -- present | missing | removed | ignored
  first_seen, last_seen

media_stream                    -- from ffprobe
  id, file_id, index
  type                          -- video | audio | subtitle
  codec, language, channels
  width, height, hdr_format, bitrate
  is_default, is_forced

sidecar
  id, file_id
  path                          -- next to the video, or in the cache dir for read_only roots
  kind                          -- subtitle | nfo | artwork
  language

series_location
  series_item_id, root_id
  path                          -- resolved once, never recomputed
  is_primary                    -- where new files for this series go
```

**File status**

- `present`: seen on the last scan.
- `missing`: was present, now not found. Links and history are kept.
- `removed`: deleted on purpose through the app. The item falls back to not acquired, with no alert.
- `ignored`: the user excluded it from matching (samples, trailers, junk).

## Data model: world and structure

Items hold identity and metadata only. Seasons and episode numbers live in orderings, because DVD and absolute orders split and merge seasons differently from aired order.

```
item
  id
  parent_id                     -- episode → series; null for movies and series
  kind                          -- movie | series | episode
  title, sort_title, original_title
  year, overview, runtime
  airs_at, airs_tz              -- episodes; release for movies
  series_status                 -- continuing | ended | cancelled (series only)
  primary_provider              -- series/movie: whose episode list is canonical
  metadata_locked

external_id
  item_id, provider, value
  is_primary                    -- from the primary provider
  UNIQUE(item_id, provider, value)
  -- partial unique index on (provider, value) WHERE is_primary

item_tag
  item_id, tag, source          -- ("filler", "animefillerlist")

ordering
  id, series_id, name
  kind                          -- aired | dvd | absolute | story | custom
  source                        -- tmdb, tvdb, user…
  is_default                    -- exactly one per series

ordering_season
  id, ordering_id, number
  title, overview, poster

ordering_entry
  ordering_id, item_id
  ordering_season_id            -- null for flat orderings
  episode_number
  position                      -- overall sort key
```

**Rules**

- Every series has exactly one default ordering, normally aired. API calls without an ordering use it.
- An item can be absent from an ordering. Specials often are not in absolute order.
- Only primary-provider IDs must be unique. A secondary provider's ID may cover two items (a merged double episode) or none.
- `airs_at` is a timestamp with the network's time zone, so a US Sunday 21:00 shows as Monday in Oslo.
- A read-only cache of the default ordering's season and episode number on `item` is allowed for convenience. The ordering tables stay the source of truth.

## Data model: links, wanted, playback and integration

`file_link` is many-to-many: one file can cover several episodes, and one item can have several files (versions and editions). There is no separate version entity.

```
── Links ──────────────────────────────────────────

file_link
  file_id, item_id
  edition                       -- null | "Director's Cut" | "Extended"…
  part                          -- multi-file movies
  confidence                    -- 0–1
  source                        -- path_id | nfo | parser | ai | user
  pinned                        -- never changed by rescans

match_candidate                 -- the review queue
  id, file_id, item_id
  confidence, source, reason
  state                         -- pending | accepted | rejected

── Wanted and status ──────────────────────────────

quality_profile
  id, name, rules               -- json: allowed formats, cutoff

monitor
  item_id
  monitored                     -- null = inherit from parent
  quality_profile_id            -- null = inherit

item_status
  item_id, source
  status, detail                -- "downloading", {"progress": 0.4}
  updated_at, expires_at
  PRIMARY KEY (item_id, source)

── Playback ───────────────────────────────────────

user
  id, name, …

user_item_state
  user_id, item_id
  position, played, play_count, last_played_at

playlist
  id, owner_user_id             -- null = shared
  name
  kind                          -- manual | smart
  base_ordering_id, rules       -- smart only

playlist_entry                  -- manual only
  playlist_id, item_id, position

user_series_pref
  user_id, series_id
  ordering_id | playlist_id     -- exactly one; drives "next up"

── Integration ────────────────────────────────────

event                           -- outbox
  id, type, payload, created_at
```

**Derived views (never stored)**

| View | Computed from |
| --- | --- |
| availability | file links, file status, `airs_at` |
| wanted | resolved monitor inheritance, availability, quality profile cutoff |
| gaps | unavailable entries before an available one, within an ordering or playlist |
| next airing | earliest upcoming episode by `airs_at` |
| next up (per user) | the user's ordering or playlist, plus `user_item_state` |

## Library layout and write policies

Every root has a layout template, including any grouping levels above the series folder. The template makes matching deterministic and tells the system where new files go.

```
{series}/Season {season:00}/{filename}                  ← default
{year}/{genre}/{series}/Season {season:00}/{filename}   ← grouped by year and genre
```

**Template tokens**

- Grouping tokens (`{year}`, `{genre}`, `{letter}`, free text) sit above `{series}`. They are hints for matching, never items in the model.
- `{series}` marks the series folder. `{season}` and `{filename}` are optional below it.
- When a library is added, the scanner proposes a template from the existing folders for the user to confirm.

**Write policies**

| Policy | Existing files | New files |
| --- | --- | --- |
| Managed | Renamed and moved to match the template | Placed by the template |
| Adaptive (default) | Never moved | Next to their siblings if a season folder exists, otherwise in the series' primary location by the template |
| Read-only | Never touched | Never written; imports go to another root |

**Placement rules**

1. A series' folder is resolved once, when it first gets one, and stored in `series_location`. It is never recomputed from metadata, so a genre change on TMDB never moves files.
2. Moving a series is an explicit user action.
3. A series may have locations on several roots. Exactly one is primary per series.
4. Sidecars (subtitles, `.nfo`, post-processing output) go next to their video file. On read-only roots they go to a cache directory.

## Scanning and matching

Matching is deterministic first. AI is an optional tie-breaker, never the matcher. Anything uncertain goes to the review queue.

A series enters the system one of two ways:

- **Added on purpose** (user or an \*arr): import the series from its provider. No files yet.
- **Discovered by scanning**: files come first, and the series must be identified before anything is imported.

**Scan stages**

1. **Discover.** Walk each root, create or update `media_file` rows. Recognize moved files by inode, then content hash, so links survive.
2. **Probe.** Read streams with ffprobe; associate sidecars in the same folder.
3. **Group.** Use the root's layout template to find each file's series folder. Grouping folders above it are hints only.
4. **Identify the series.** In order: an explicit ID in the path (`{tmdb-1399}`), an `.nfo`, then a provider search using the folder name plus year hints. A low-confidence result goes to review; nothing is imported on a guess.
5. **Import** the series' items and orderings from its primary provider (see Metadata import).
6. **Match files to items.** Layered, stopping at the first confident answer:
   1. Explicit IDs in the path, `.nfo` or container tags.
   2. Filename parsing (guessit, anitopy) for title, season, episode, absolute number, edition.
   3. Resolve parsed numbers against an ordering: the root or series setting first, the default ordering next, others if numbers don't fit (an episode beyond the season's length suggests absolute).
   4. Sanity checks: runtime from ffprobe against the expected runtime, episode title in the filename.
7. **Accept or queue.** Above the threshold, write a `file_link`. Below it, write `match_candidate` rows for review.

**Rules**

- Pinned links are never changed by a scan.
- Two files claiming the same item become two links (versions), not a conflict.
- A whole-library rescan never runs while a root is unreachable. An unreachable root marks nothing missing.

**Optional AI**

- Embedding models for fuzzy title matching (localized, romanized anime titles, typos).
- A small local LLM as a tie-breaker: given the path and a short candidate list, it picks one or answers none, with grammar-constrained output. Expect 3–8B parameters for reliable results; 1B is a floor.
- AI links carry `source = ai` and a capped confidence, so most go to review.
- The system works fully without AI and without a GPU.

## Metadata import and refresh

Each series or movie has one primary provider that defines its item set. Other providers are cross-references.

**Import**

1. Create the series item and an episode item for every episode the primary provider lists, owned or not.
2. Create the provider's orderings: aired as default, plus DVD, absolute and others where offered (TVDB season types, TMDB episode groups).
3. Attach external IDs from secondary providers where they map cleanly. Where they don't, leave them out or map one ID to several items.

**Refresh**

1. Match existing items by primary external ID.
2. If an ID no longer resolves, fall back to position in the default ordering plus title and air date, and reconcile. Never create a duplicate item, or watch history and links are orphaned.
3. New episodes become new items; their availability starts as upcoming or not acquired.
4. Episodes the provider deleted are flagged for review, not deleted, if they have links or watch state.
5. `metadata_locked` fields are never overwritten.
6. Ended and cancelled series refresh less often.

**Provider notes**

| Provider | Episode identity | Orderings |
| --- | --- | --- |
| TMDB | Episode IDs; API addressed by season and episode number | Episode groups referencing episode IDs |
| TVDB | Episode IDs shared across orderings | Season types: aired, DVD, absolute, alternate, regional |
| AniDB | Episode IDs; own numbering with special, credit and trailer types | Its own numbering as an ordering |
| IMDb, Trakt | Episode IDs; Trakt cross-references TMDB, TVDB, IMDb | None of their own |

Split and merged episodes (a double pilot that is one episode in aired order and two in DVD order) are expected. The primary provider decides what an item is; other orderings and providers map onto it.

## Availability, wanted and status

Availability describes the world; wanted describes intent. They are separate, so users can browse a whole series without everything unowned showing as a problem.

**Availability (per item, derived)**

| State | Rule |
| --- | --- |
| Available | At least one linked file is `present` |
| Missing | Had a linked file; none present, and at least one is `missing` |
| Not acquired | No present or missing files, and it has aired |
| Upcoming | `airs_at` is in the future |
| Unknown | No air date (announced, TBA) |

"Missing" always means the system lost something. A file the user deleted through the app is `removed`, and the item becomes not acquired.

**Wanted**

An item is wanted when it is monitored (after inheritance from series to episode) and either not available or below its quality profile's cutoff. When an item becomes or stops being wanted, an event is emitted.

**Gaps and next airing**

- A gap is an unavailable entry that comes before an available one in an ordering or playlist. A filler episode is not a gap in a no-filler playlist.
- Next airing is the earliest upcoming episode by `airs_at`, shown in the user's time zone.

**Custom status**

- Written by external tools per item and source, so several tools can report at once.
- Free text, with documented well-known values the UI styles: `searching`, `queued`, `downloading`, `importing`, `failed`.
- `detail` carries JSON such as progress and ETA.
- Every status has `expires_at`. Past it, the UI shows it as stale or hides it, so a crashed downloader can't leave "downloading" forever.
- The system displays status but never acts on it.

## Integration API and events

External tools read the wanted list, report status, and hand off files through the API. They never touch the database. Every change is published as an event.

**Addressing items**

Any endpoint taking an item accepts either form:

- Internal: `item:<id>`
- Provider position: `tvdb:12345` + ordering + season and episode (or absolute number), for example `(tvdb:12345, aired, S02E05)`. Resolved through the ordering tables.

This lets the \*arrs integrate by position, as they work today, without positions leaking into item identity.

**Endpoints (first draft)**

| Endpoint | Purpose |
| --- | --- |
| `GET /wanted` | Wanted items with quality targets; filterable by library, series, ordering |
| `GET /items/{ref}` | Item with availability, links, files, orderings |
| `GET /series/{ref}/gaps` | Gaps within an ordering or playlist |
| `GET /calendar` | Upcoming and recent episodes by `airs_at` |
| `PUT /items/{ref}/status/{source}` | Set custom status, detail and expiry |
| `POST /imports` | Hand off a file: the system matches, places by write policy, links |
| `POST /series` | Add a series by provider ID; imports items and orderings |
| `GET /events` | Event stream (SSE or long-poll) from a cursor |
| `POST /webhooks` | Subscribe a URL to event types |

**Events**

`item.wanted`, `item.unwanted`, `item.added`, `file.added`, `file.upgraded`, `file.missing`, `file.removed`, `link.created`, `link.changed`, `status.changed`, `series.refreshed`, `match.needs_review`.

Events are written to the `event` outbox in the same transaction as the change, then delivered. Consumers resume from a cursor, so nothing is lost while they are down.

**Working with today's \*arrs, unchanged**

- Expose `/wanted` in the \*arrs' custom import list JSON format, so Sonarr and Radarr poll it.
- Accept their Connect webhooks (import, upgrade, delete) to rescan the affected paths immediately.
- Deeper handoff through `POST /imports` comes later, when the \*arrs or plugins support it.

## Lessons from Plex and Jellyfin, and open questions

Plex's problems are mostly about its business; Jellyfin's are mostly about its engine. This design targets the engine problems directly and avoids the business ones by being self-hosted.

| Pain today | Where | This design's answer |
| --- | --- | --- |
| Strict naming; unrecognized shows; renaming whole collections | Both, worst in Jellyfin | Layout templates with grouping levels, adaptive policy, layered matching |
| Hard to fix bad matches; fixes reverted | Both | Review queue, pinned links, locked metadata |
| Category folders unsupported for TV | Plex | Grouping tokens in the template |
| No concept of unowned episodes | Plex | Items from metadata; availability states |
| Irreversible, slow database migrations | Jellyfin 10.11 | Versioned migrations, automatic backup before each, large-library tests |
| Transcoding failures (HDR especially) | Jellyfin | Known client capabilities feed quality profiles and warnings |
| Cloud outages block local use | Plex | Fully offline; cloud features optional |
| Own library behind promoted content | Plex | The user's library is the home screen |
| Telemetry, account required, price hikes | Plex | No account, no telemetry by default |
| No relay or NAT traversal; weak clients | Jellyfin | Open question below |

**Out of scope for v1**

- Music and books (separate models later).
- Localized titles, people and credits, collections, artwork storage, per-user library scoping. Each bolts on without changing the core.

**Open questions**

- [ ] Database: SQLite, Postgres, or both? Affects JSON columns, partial indexes and views.
- [ ] Streaming in v1, or library and wanted first with Jellyfin serving playback?
- [ ] Clients: own web player first, then a Jellyfin-compatible API so existing apps work?
- [ ] Remote access: rely on the user's reverse proxy or VPN, or build optional relay?
- [ ] Quality profiles: own them fully, or share responsibility with the \*arrs?
- [ ] Match confidence thresholds: global, per root, or per source?
- [ ] Should wanted be able to follow a playlist (monitor only no-filler episodes)?
- [ ] Write `.nfo` sidecars or embedded tags back out for portability, and by default or opt-in?

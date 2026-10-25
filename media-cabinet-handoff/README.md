# Media Cabinet — handoff

Media Cabinet is a self-hosted media library manager and streaming server. It combines what the *arr tools track (what you have and want) with what Plex or Jellyfin do (library and playback), in one model. It does **not** download anything; acquisition tools integrate through its API and events.

## What's in here

| Path | What it is |
| --- | --- |
| `docs/spec.md` | Product and technical spec: principles, full data model, layout templates and write policies, scanning and matching pipeline, metadata import, availability/wanted rules, integration API and events, open questions |
| `docs/design.md` | Web UI handoff: design tokens, global patterns (Watch/Manage mode, wording per state), screen-by-screen behaviour, known design gaps |
| `design/screens/HybridHome.dc.html`, `HybridSeries.dc.html` | **The chosen design.** Reference mockups for Home and Series |
| `design/screens/Review.dc.html`, `Roots.dc.html` | Match review queue and library roots screens (same design language, older branding) |
| `design/screens/*` (others) | Earlier explorations, reference only |
| `design/canvas.json` | Layout of the original design canvas (titles of each screen) |

## Suggested starting order

1. Read `docs/spec.md`, especially "Design principles", the data model sections and "Availability, wanted and status".
2. Settle the open questions at the end of the spec that block the build, especially the database choice (SQLite, Postgres, or both).
3. Build the core in this order: schema and migrations → scanner (discover, probe, file identity) → metadata import with orderings → layered matcher plus review queue → derived views (availability, wanted, gaps) → API and event outbox.
4. Then the web UI, starting with the Series page (`docs/design.md` and `HybridSeries.dc.html`).

## Notes for implementation

- The mockups' `<script type="text/x-dc">` blocks contain the sample data and UI logic, such as how ordering changes labels, gaps and next up. Use them as a reference for component state.
- The `.dc.html` files need a canvas runtime that isn't included, so open them as source, not in a browser.
- Use "Media Cabinet" as the product name everywhere. Some older screens still say "Shelf".

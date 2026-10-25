# Media Cabinet — Web UI design handoff

The chosen direction is **"Media Cabinet"**: the original dark layout (sidebar, amber accent, Manrope) combined with the cinematic hero and large cards from the Lean-back exploration.

Reference mockups: `design/screens/HybridHome.dc.html` and `design/screens/HybridSeries.dc.html`. The other screens are earlier explorations, kept for reference only (see the table at the end).

## Design tokens

### Colour (dark theme, the only theme so far)

| Token | Value | Use |
| --- | --- | --- |
| `bg` | `#0F1115` | Page background |
| `bg-sidebar` | `#13161C` | Sidebar |
| `surface` | `#171A21` | Cards, panels |
| `surface-raised` | `#1F232C` | Active nav item, fact tiles |
| `border` | `#262A33` | Card borders, dividers |
| `border-control` | `#2E333D` | Inputs, secondary buttons |
| `text` | `#E8E6E1` | Primary text |
| `text-secondary` | `#B5BAC3` | Nav items, secondary labels |
| `text-muted` | `#9AA0AA` | Meta lines, captions (passes 4.5:1 on `bg` and `surface`) |
| `accent` | `#F2B544` | Primary buttons, selected pills, progress, Up next |
| `accent-ink` | `#1A1406` | Text on `accent` |
| `accent-soft-text` | `#F5D08A` | Accent-tinted labels on dark tints |
| `state-available` | `#4CC38A` (text on tint: `#6FD6A3`) | Available |
| `state-missing` | `#F07A5A` (text on tint: `#F59C84`) | Missing / file offline |
| `state-upcoming` | `#6EA8FE` (text on tint: `#8DBBFF`) | Upcoming, downloading, external status |
| `state-neutral` | `#B5BAC3` on `#22262E` | Not acquired, unknown |

Status badges use a 14–18% alpha tint of the state colour as the background, with the lighter "text on tint" colour for the label.

The hero background is a per-series colour (placeholder `#3A1C20`). With real artwork it must become a backdrop image with a dark scrim, so the title and buttons keep 4.5:1 contrast.

### Typography

- **UI:** Manrope 400–800. Titles use 800 with tight tracking (`-0.02em` to `-0.035em`).
- **Mono:** JetBrains Mono, for paths, IDs, templates and technical file info (Manage mode only).
- Scale: hero title 76px (Watch) or 52px (Manage), page H1 30px, section H2 22–24px, body 15–16px, meta 13–13.5px, eyebrow labels 12–13px uppercase with `0.08em` tracking.

### Shape and spacing

- Radius: 10px controls, 14px cards, 999px pills and primary buttons.
- Spacing steps of 4. Common values: 8, 12, 16, 20, 24, 28, 32, 40.
- Minimum touch target: 44px. Pills inside segmented controls are 40px.
- Layout: sidebar `flex: 1 1 220px`, content `flex: 999 1 560px` in a wrapping row, so the sidebar stacks above the content on narrow screens.

## Global patterns

- **Watch / Manage mode.** Watch is for viewing; Manage is for editing. In the mockups the switch sits in each hero. **In the real app it should be one app-wide setting** in the user menu, shown only to users with admin rights, and remembered.
  - Manage adds Wanted, Review queue and Libraries & roots to the sidebar.
  - Manage replaces friendly wording with system states, and shows technical details (file, codec, match source and confidence).
  - Manage shows a "Managing · changes apply for everyone" label.
- **Segmented controls** (mode, ordering, layout) use `aria-pressed` buttons. The selected option is filled; the others are transparent.
- **Wording per mode:**

| State | Watch mode label | Manage mode label |
| --- | --- | --- |
| available, unwatched | Ready | available |
| available, watched | Watched | available |
| next per user ordering | Up next | (no badge) |
| missing | File offline | missing |
| not acquired | Not in library | not acquired |
| not acquired + external status | Downloading 40% | not acquired (status shown in tech line) |
| upcoming | Airs Fri 30 Oct | upcoming |
| unknown | Date TBA | unknown |

- **Gaps** are shown only in Manage mode.

## Screens

### Home (`HybridHome.dc.html`)

1. Hero: search field, mode switch, "Continue · <ordering>" eyebrow, show title, episode and time left, Resume (primary) and Episodes buttons.
2. Manage only: "Needs attention" cards for matches to review, files missing (with the offline root named), wanted, and gaps.
3. Rails that scroll sideways: Keep watching (16:9 cards with a progress bar), Coming this week (day/time tile), TV Shows (2:3 posters with optional badge; Manage adds an availability bar).

### Series (`HybridSeries.dc.html`)

1. Hero, Watch mode: title, meta line with next airing, overview, "Resume S01E06" (computed from the user's ordering), Watch from start.
2. Hero, Manage mode: shorter hero, "Managing" label, counts per state (available, missing, not acquired, wanted), Edit details, Rescan files, Refresh metadata.
3. Manage only: four panels.
   - **Monitoring:** monitor new episodes, quality profile, and whether wanted follows all episodes or a playlist.
   - **Metadata:** primary provider and ID, other linked providers, lock my edits, change match.
   - **Orderings:** list with default marker, which one is used for file matching, new playlist.
   - **Location:** primary path and policy, other locations (with offline state), move series.
4. Episodes toolbar: ordering switch (Aired / Absolute / No filler) and a List/Grid switch. Remember the layout per user.
5. **List view:** a season header (aired order only), then one row per episode with a 16:9 thumbnail (number plus progress bar), title, Filler tag, meta line and state badge. Manage adds a monitor checkbox, a technical line and a "more actions" button.
6. **Grid view:** 16:9 cards with a badge, large episode number and progress bar. The Up next card gets an accent outline. Manage adds a "Monitored" checkbox and a technical line.

Behaviour shown by the mockup's sample data (see `renderVals()` in the file):

- Changing the ordering renumbers episodes: `S01E03` in aired order, `03` in absolute order, `E03` in the no-filler playlist.
- No filler removes filler episodes. That removes their gaps and changes Up next (from E06, a filler, to E07).
- Unticking an episode's monitor checkbox lowers the wanted count right away.

### Earlier screens still useful for the same design language

| File | What to take from it |
| --- | --- |
| `Review.dc.html` | Match review queue: path segments coloured by template token, fact tiles (value and source), explanation callout, candidate radio list with confidence bars, Accept and pin |
| `Roots.dc.html` | Library roots: per-root write policy switch, coloured layout template, offline state, "where would a new file go" preview |
| `Series.dc.html` | First version of the interactive ordering/gap logic |
| `Main.dc.html` | First home layout (no hero) |

These use the old placeholder name "Shelf" and an older logo. Restyle them to the Media Cabinet sidebar and logo when you build them.

### Explorations, reference only

`Editorial*` (light, serif), `Console*` (dense, table-first, with an event stream panel), `Leanback*` (TV, 10-foot). `ConsoleSeries` has a useful idea for an advanced Manage view: an episode inspector showing every link, the episode's number in each ordering, and its event history.

## Known gaps and next design work

- Playback flow: version picker (4K or 1080p), audio and subtitle tracks, and a "plays directly / needs transcoding on this device" hint before starting.
- Real artwork: a backdrop with a scrim in the hero, and checking that the amber accent works over real posters.
- A hero that collapses when you scroll (on desktop the episodes start too far down the page).
- Focus states and remote/keyboard navigation for TV browsers.
- Wanted and Calendar screens; Movies equivalents of the Series page.
- A light theme (tokens above are dark only).

## About the `.dc.html` files

The mockups are single-file HTML templates for a design canvas tool. They are **not runnable on their own**: the `support.js` runtime they reference is not included. Read them as precise visual specs:

- All styles are inline, so exact sizes, colours and spacing are in the markup.
- `{{name}}` is a value from the `renderVals()` method in the `<script type="text/x-dc">` block at the bottom.
- `<sc-for list="{{items}}" as="x">` repeats, and `<sc-if value="{{flag}}">` shows its children conditionally.
- `renderVals()` holds the sample data and the UI logic (mode, layout, ordering, gaps, next up), which is a good reference for the real component state.
- Show titles and numbers are invented sample data.

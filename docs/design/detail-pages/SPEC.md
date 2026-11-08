# Media Cabinet — detail pages handoff

Final direction for the Series, Episode and Movie detail pages: one shared header across all three, with a dedicated editor view for admins.

## What's in this folder

| File | What it is |
|---|---|
| `pages/series.html` | Series page, viewer mode. Interactive: Select mode, season tabs incl. "All seasons", collapsible seasons, live next-episode countdown. |
| `pages/series-editor.html` | Series page with Edit mode on (admin/editor). Dense episode table with bulk actions. |
| `pages/episode.html` | Episode page. |
| `pages/movie.html` | Movie page. |
| `pages/dc-lite.js` | Tiny renderer that makes the mockups run standalone. Prototype aid only; don't ship it. |
| `index.html` | Links to the four pages. |

Open `index.html` in a browser. No build step, no server needed. Fonts load from Google Fonts (Manrope); everything else is local.

Each page's data and state logic lives in the `class Component` script at the bottom of its HTML file, which is the quickest place to read exact behaviour.

---

## Design tokens

### Color

| Token | Value | Use |
|---|---|---|
| `bg` | `#12161c` | Page background |
| `surface` | `#1a1f27` | Cards, facts card, inputs |
| `surface-raised` | `#1f252e` | Highlighted card (Continue watching, selected version) |
| `surface-chip` | `#222833` | Neutral chips, selection bar |
| `border` | `#2b323d` | Card and input borders |
| `border-strong` | `#3a424e` | Secondary button outlines |
| `divider` | `#232a34` | Row and section dividers |
| `text` | `#eef1f4` | Primary text |
| `text-2` | `#c9d0d8` | Body copy, secondary text |
| `text-muted` | `#a3adb9` | Meta, labels |
| `text-faint` | `#7d8794` | Placeholder labels only |
| `accent` | `#34bfae` | Primary buttons, progress, up-next, selection |
| `on-accent` | `#06201d` | Text and icons on accent |
| `accent-wash` | `rgba(52,191,174,.08)` + 1px inset `rgba(52,191,174,.45)` | Up-next row / selected row |
| `info` | `#9cc4ff` on `rgba(106,168,255,.14)` | Upcoming episode, Offline badge |
| `warning` | `#f0a54a` on `rgba(240,165,74,.12)` | Missing file (editor) |

Backdrop scrim: `linear-gradient(180deg, rgba(18,22,28,.4) 0%, rgba(18,22,28,.8) 55%, #12161c 100%)` over a 440px backdrop band.

### Type (Manrope)

| Role | Size / weight / tracking |
|---|---|
| Page title (h1) | 72px / 800 / -0.03em, line-height .98 |
| Section heading (h2) | 22–24px / 700 |
| Card title | 18px / 700 |
| Body / synopsis | 16px / 400, line-height 1.6 |
| Details line | 15px / 400 |
| Meta, table cells | 14px |
| Eyebrow labels | 12px / 700 / 0.08em, uppercase |
| Chips | 12–13px / 700 |

Numbers that change (countdowns, sizes, episode codes in tables) use `font-variant-numeric: tabular-nums`.

### Shape and spacing

- Radii: pills and buttons 999px; cards, poster, facts card 14px; episode stills 8–10px; inputs 10px.
- Container: max-width 1280px, 40px side padding.
- Vertical rhythm: 56px between page sections; 48px between the header's two columns; 18px between items in the header's right column.
- Buttons: primary 54px tall; secondary 44–54px; table/toolbar 34–40px. Touch targets never below 44px outside dense editor tables.

---

## The shared header

Every detail page uses the same two-column header over a backdrop band.

```
[← Back]                                                    [Edit mode ○]

┌─────────────┐   Title (h1)
│             │   Year · Genre · Network · Status  [Rating] 7.5 TMDB
│  Artwork    │   Tagline (italic)
│             │   Synopsis
│             │   [Primary action] [Secondary] [+]
└─────────────┘
┌─────────────┐   Featured block (h2 + card)  ← the only part that differs per page
│ Facts card  │
└─────────────┘
```

- **Left column** (`flex: none`): artwork + facts card. Width 280px for posters (2:3, 280×420), 360px for episode stills (16:9).
- **Right column** (`flex: 999 1 520px`): wraps under the left column on narrow screens.
- **Top bar**: back link (pill, translucent surface) on the left, **Edit mode** switch on the right. The switch is a real `role="switch"` button.

### Per page

| | Artwork | Facts card | Primary actions | Featured block |
|---|---|---|---|---|
| Series (viewer) | Poster | Network, First aired, Status, Next episode (when airing), Seasons, In library | Resume SxxExx · Watch from start · + | Continue watching card, plus Next episode card when airing |
| Series (editor) | Poster with "Change artwork" | Matched to, Library, Path, Added, Locked fields | Edit metadata · Lock fields (top bar: Refresh metadata, Fix match) | Library health: Episodes, Missing, Unmatched, On disk |
| Episode | Still with play button | Aired, Runtime, Aired / DVD / Absolute order, File | Play (again) · Mark (un)watched | Up next: Previous / Next episode cards |
| Movie | Poster | Director, Writer, Studio, Released, Added | Split Play button (caret picks version) · Mark watched · + | Versions: selectable cards with tech chips and size |

---

## Below the header

**Series, viewer**: Episodes section (season tabs, order picker, Select button, episode rows), then Cast (2:3 tiles).

**Series, editor**: Episodes as a table: checkbox, #, Title, Aired, Runtime, File, Size, Status. Toolbar with season picker, Aired/DVD/Absolute segmented control, filter input, bulk action bar.

**Episode**: "More from Season N" strip (current episode outlined in accent), then Main cast and "Also in this episode".

**Movie**: Cast grid (2:3 tiles).

---

## States and behaviours

### Edit mode
- One switch in the top bar, same position on every page.
- On the Series page it swaps the whole page to the editor layout (table instead of rows, admin facts, Library health instead of Continue watching).
- Episode and Movie editor variants weren't drawn; the same pattern applies (swap facts card and featured block for admin content).

### Episode rows (viewer)
- Still 192×108 with progress bar along the bottom when partly watched.
- Status chip on the right: **Watched** (neutral fill), **Up next** (accent fill), **Ready** (outline).
- The up-next row gets the accent wash.
- **Offline** badge (info color, download icon) next to the status for episodes synced for offline.

### Season tabs
- Pills: **All seasons**, Season 1…N, Specials. Selected pill is light fill, dark text.
- **All seasons** shows every season as a collapsible group (chevron button with `aria-expanded`). By default only the season containing the up-next episode is open.
- A single season tab shows that season expanded.
- Group header: season name, progress bar, "x of y watched", and "Mark season watched" (hidden while selecting).

### Select mode (viewer)
- Entered with the **Select** button next to the Order picker. The button is replaced by the selection bar.
- Checkboxes appear on every row and on every season header. The whole row is a `<label>`, so clicking anywhere toggles it.
- A season header checkbox selects or clears that whole season, even when collapsed.
- Selected rows get the accent wash (up-next highlight is suppressed while selecting so selection reads clearly).
- Selection bar: count · Select all (everything currently visible) · Clear | Mark watched · Mark unwatched · Download for offline · Remove download · Add to playlist · **Done** (exits and clears selection).
- Viewer actions are personal only (watched state, offline sync, playlists). Library-wide actions stay in editor mode.

### Currently airing series
- Status reads "Returning series"; facts card gains a "Next episode" row.
- **Next episode card** under Continue watching: calendar icon, eyebrow "Next episode · SxxEyy", countdown as the headline, then title · air date · network, and a "Download when available" switch.
- Countdown wording:
  - ≥ 1 day: "in 1 day and 2 hours"
  - < 1 day: "in 3 hours and 12 minutes"
  - < 1 hour: "in 12 minutes"
  - reached: "Airing now"
  - Short form for badges: "in 1d 2h", "in 3h 12m", "in 12m", "Now".
- Refreshes every 30 seconds.
- The latest season's header shows a "Next in 1d 2h" badge (visible even when collapsed) and "· 1 upcoming" in its count.
- The upcoming episode is the last row of its season: dashed outline, calendar placeholder instead of a still, "Airs <date>", countdown chip, no checkbox.
- Dates use the `nb-NO` locale to match the existing app (e.g. "søn. 8. nov., 15:19").

### Movie versions
- Versions are radio cards; the selected one has an accent border and raised surface.
- The caret half of the Play button opens the same choice as a menu, showing the current version's short label (e.g. "4K DV").

---

## Responsive

- Header columns wrap: artwork + facts stack above the text on narrow widths.
- Episode rows wrap the still above the text.
- Selection bar and toolbars wrap; the editor table scrolls horizontally inside its own box (`min-width: 900px`).
- Grids use `repeat(auto-fill, minmax(...))`, so tile counts adapt.

---

## Placeholder and sample data

Not real data. Replace with your API values:

- All artwork (backdrops, posters, stills) and cast photos; cast shows initials, like the app's existing "MS" fallback.
- Anything in `[brackets]`.
- Season 1 episode titles after S01E02 were filled in from memory for layout; check order against your data. Seasons 2+ are placeholders.
- The airing state is hypothetical (the show has ended); the next air time is set to ~26 hours after page load.
- File details ("1080p · H.264"), the second movie version, and editor stats are sample values.

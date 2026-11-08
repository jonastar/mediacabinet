# Detail pages

The series, movie and episode pages share one header: `web/src/pages/detail/`. The design and its mockups are in [`design/detail-pages/`](design/detail-pages/SPEC.md); open its `index.html`. `dc-lite.js` only runs the mockups and isn't part of the app.

## Where the app differs from the design

**Left out on purpose:** offline sync (the Offline badge, Download for offline, Remove download, Download when available). It needs a milestone of its own.

**Disabled until built:** Add to collection, Change artwork, Fix match on movies, and Refresh metadata in the episode table, since refreshes work on a whole series.

**Different on purpose:**

- Pages run up to 1920px wide rather than 1280px, so large screens use their width.
- There are no season tabs: seasons are collapsible groups, which still work with many seasons.
- Continue watching sits between the previous and next episodes, and cards are only as wide as their content.
- The viewer's episode list keeps a List / Grid switch, a watched button on each episode and the episode's number on its thumbnail.
- Select stays in the toolbar while selecting, lit, and clicking it again leaves Select mode, as Done does.
- The episode page's back link is a full breadcrumb (`Library / Series / Season N`); the season link opens that season's group on the series page and scrolls to it (`?season=N`).
- Colours are the app's tokens rather than the design's near-identical values, so these pages match the rest of the app and the brand.
- Dates, numbers and language names are English like the wording (`core/locale.ts`), not a fixed `nb-NO`: the browser's English locale if it has one, otherwise `en-GB`.
- Edit mode uses the Manage wording from `lib/wording.ts` for status chips.
- "Unmatched" in Library health is "Not acquired", and "Wanted" is added: the API has no per-series count of unmatched files.
- "Added" isn't in the facts cards: the API doesn't expose it.
- Add to playlist starts a new playlist from the selection, as playlists have no add-to-existing action yet.
- The editor table has a Monitored column, and edit mode keeps the Monitoring, Metadata, Orderings and Location panels and Rescan files, none of which the design draws.
- "Full cast & crew" opens in place, as there's no page for it.
- A date with no known air time counts down in whole days.

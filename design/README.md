# Design tokens

The design values shared by the web UI and every player. Not started: `web/src/index.css` holds today's values. How they're used is in [`docs/design-language.md`](../docs/design-language.md). The logo, icons and animations are in [`docs/brand/`](../docs/brand/).

## Planned

`tokens.json` is the one source, and a small script generates:

- `web/src/tokens.css`: CSS variables for the web UI
- fixed values for the Samsung and LG screens, because older TVs have no CSS variables
- a Swift file for `players/apple`
- a Kotlin file for `players/android`

The generated files are committed, and CI checks they're current.

Only values used on more than one platform go here: colour, state colours, radii, spacing, type sizes and durations. Values only the web needs, such as the layout template colours, stay in `index.css`.

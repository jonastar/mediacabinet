# Design tokens

The design values shared by the web UI and every player. How they're used is in [`docs/design-language.md`](../docs/design-language.md). The logo, icons and animations are in [`docs/brand/`](../docs/brand/).

`tokens.json` is the one source. After changing it, run `pnpm tokens` in `web/`, which runs `scripts/generate-tokens.mjs` to write:

- `web/src/tokens.css`: CSS variables for the web UI
- `players/web/src/tokens.ts`: fixed values for the Samsung and LG screens, because older TVs have no CSS variables
- `players/apple/Core/Sources/Core/Tokens.swift`
- `players/android/core/src/main/kotlin/app/mediacabinet/core/Tokens.kt`

The generated files are committed, and the pre-commit hook checks they're current (CI should too, once there is one).

Only values used on more than one platform go here: colour, state colours, radii, spacing, type sizes and durations. Values only the web needs, such as the layout template colours, stay in `web/src/index.css`.

The `web` type sizes are the lower end where `docs/design-language.md` gives a range. The web's CSS doesn't use the type, spacing or duration variables yet.

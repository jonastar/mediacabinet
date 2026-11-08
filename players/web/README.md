# TV web app

The app for Samsung and LG TVs, on Preact. [`../tizen`](../tizen/) and [`../webos`](../webos/) wrap its build for each platform. The plan is in [`docs/clients.md`](../../docs/clients.md), and the look and focus rules in [`docs/design-language.md`](../../docs/design-language.md).

There's one app for every model, kept small so it also starts quickly on budget new TVs. Newer TVs get HDR, better codecs and 4K through capabilities and the server, not a different UI.

## Status

A first version, for finding out how far back it runs: connect to a server, pair with a code, pick something from Home (keep watching, shows, movies; not folder libraries), go through a show by season, and watch it, with resume, seeking, fallback, next episode, and version, quality, audio and subtitle pickers. Back returns to where you were, each rail remembers its last item, and LG's Magic Remote pointer works everywhere. "About this TV" shows what the TV reports, and anything that goes wrong shows on screen, since a TV has no console.

Not yet: search, a QR code, a page for movies (choosing one plays it), and episodes that can't be played yet on a show's page. The subtitle and audio chosen aren't carried over to the next episode, as on the web.

## Working on it

```sh
pnpm install           # in web/ too: the core's types come from there
pnpm dev               # a desktop browser, with arrow keys for a remote; Escape is Back
pnpm build             # writes dist/, and fails if it won't run on old TVs
```

To reach a server from `pnpm dev`, start it with `CABINET_CORS_ORIGINS=http://localhost:5174`. A built app opened as a file doesn't need that, because it's the same as a packaged one.

To put it on a TV, see [`../webos`](../webos/) and [`../tizen`](../tizen/).

The server also hosts the build at `/tv-mode/`, where it's same-origin: it skips connecting and pairing, signs in with the web UI's cookie, and zooms the 1920×1080 layout to fit the window. A debug server reads `dist/` from disk, so run `pnpm build` and reload. The Docker image builds it beside the web UI.

## Setup

This is its own project, separate from the web UI in [`web/`](../../web/):

- `package.json`: Preact, hls.js and the legacy build plugin. The web UI stays on React.
- `vite.config.ts`: ES5 with polyfills, written to `dist/`, with tokens written into the CSS as fixed values (`token(color.accent)`).
- `scripts/check-build.mjs`: reads what the build wrote, and fails on JavaScript that isn't ES5, CSS older TVs can't use, and `crossorigin` scripts. A passing Vite build proves none of that.
- `tsconfig.json`: an `@core` alias to [`web/src/core/`](../../web/src/core/), the only code shared with the web UI.
- `src/`: screens, focus handling and the platform adapter (`platform.ts`). `api.ts` checks every call's path, method, body and result against the schema generated from the server's OpenAPI document.

It uses the same lint and format rules as `web/`, and the pre-commit hook runs them.

## The reference for TV

Android TV and tvOS follow this app for layout, sizes and focus behaviour, so keep it runnable in a desktop browser, where arrow keys stand in for a remote. When building a screen settles something, write the rule in `docs/design-language.md` and put the value in `design/tokens.json`, so other platforms don't have to read this CSS. The CSS limits below are for old TVs, and other platforms don't share them.

## Rules

This code may have to run on much older browsers than the web UI, possibly down to Chromium 38. A passing build doesn't show that it does: Vite leaves CSS alone, and without the legacy plugin keeps JavaScript those TVs can't parse.

**CSS:**

- Nothing from `web/`: all CSS lives here.
- No `var()`: tokens are fixed values.
- No grid and no `gap`: flexbox with margins.
- No `aspect-ratio`: fixed sizes.
- No `:has()`, `:focus-visible` or `color-mix`. The focus module sets a class.
- No `#RRGGBBAA` or `rgb(0 0 0 / 50%)`: use `rgba()` with commas.
- No `clamp()`, `min()`, `max()` or `inset`.
- Static Manrope files, not the variable font.

**JavaScript:**

- The build goes to ES5 with polyfills, loaded from the entry point.
- No regex lookbehind, `BigInt` or `Proxy`, which can't be compiled or polyfilled.
- Don't assume `video.play()` returns a promise: before Chromium 50 it returns nothing, so `.play().catch()` crashes.
- core-js doesn't cover browser APIs. Check before using anything new (`fetch`, `AbortController`, `ResizeObserver`, `URLSearchParams`).
- No `EventSource`: it can't send the bearer token. Refetch when a screen is shown again.

**TV behaviour:**

- Something is always focused, and nothing depends on hover. With a pointer, hovering moves focus, so anything a pointer needs beyond that (rail arrows, the timeline) is extra, and never in the way of the remote.
- Give a control `data-key` if focus should find it again, and put rails in `data-rail`; `focus.ts` explains them.
- A screen with a text field keeps it in the top half (`.above-keyboard`), clear of the on-screen keyboard.
- Requests go through the configurable server address, never a relative `/api` URL.
- Memory is short: render only what's near the screen, and request images at display size.

Anything specific to one platform (remote keys, HDR checks, AVPlay) goes behind the adapter, with its implementation in `../tizen` or `../webos`.

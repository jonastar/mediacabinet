# TV web app

The app for Samsung and LG TVs, on Preact. [`../tizen`](../tizen/) and [`../webos`](../webos/) wrap its build for each platform. Not started. The plan is in [`docs/clients.md`](../../docs/clients.md), and the look and focus rules in [`docs/design-language.md`](../../docs/design-language.md).

There's one app for every model, kept small so it also starts quickly on budget new TVs. Newer TVs get HDR, better codecs and 4K through capabilities and the server, not a different UI.

## Planned setup

This is its own project, separate from the web UI in [`web/`](../../web/):

- `package.json`: Preact, hls.js and the legacy build plugin. The web UI stays on React.
- `vite.config.ts`: ES5 with polyfills, written to `dist/`, with a check that fails on CSS older TVs can't use.
- `tsconfig.json`: an `@core` alias to [`web/src/core/`](../../web/src/core/), the only code shared with the web UI.
- `src/`: screens, focus handling, its own small hooks over the core, and the platform adapter interface.

It uses the same lint and format rules as `web/`. The pre-commit hook has to run them here too once there's code.

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

- Something is always focused, and nothing depends on hover.
- Requests go through the configurable server address, never a relative `/api` URL.
- Memory is short: render only what's near the screen, and request images at display size.

Anything specific to one platform (remote keys, HDR checks, AVPlay) goes behind the adapter, with its implementation in `../tizen` or `../webos`.

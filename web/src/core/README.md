# Core

Plain TypeScript shared by the web UI and the TV app in `players/web`. Not started: this code is still in `api/`, `lib/`, `pages/PlayerPage.tsx` and `pages/player/useSource.ts`. The plan is in `docs/clients.md`.

## Planned contents

- API client, with a configurable server address
- Playback session: plans, fallback, resume, seeking past what's converted, progress reporting
- Capabilities, with platform overrides for TVs
- Wording and formatting

## Rules

- No JSX, no CSS, and no imports from React or Preact. A lint rule enforces this. The web UI and the TV app each wrap the core in their own hooks.
- No runtime dependencies. An import from here resolves against `web/node_modules`, even when `players/web` is building. So each app passes in what the core needs, such as hls.js.
- No polyfills. Those load from the TV app's entry point, so the web UI isn't affected.
- It has to run on old TVs too, so avoid what can't be compiled or polyfilled: regex lookbehind, `BigInt` and `Proxy`. Don't assume `video.play()` returns a promise.
- Browser APIs that core-js doesn't cover (`fetch`, `AbortController`, `ResizeObserver`, `URLSearchParams`) need a polyfill in the TV build or a way around them.

Swift and Kotlin can't use this code, but it's the reference for how playback should behave in those apps.

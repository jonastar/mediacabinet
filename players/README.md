# Players

The apps people watch on. They browse and play; managing a library stays in the web UI. The TV app in [`web/`](web/) has a first version, and the rest aren't started. The plan is in [`docs/clients.md`](../docs/clients.md) and the checklist in [`docs/roadmap.md`](../docs/roadmap.md).

| Folder | Targets | UI | Player |
| --- | --- | --- | --- |
| [`android/`](android/) | Android, Android TV | Kotlin, Compose | Media3 |
| [`apple/`](apple/) | iOS, iPadOS, tvOS | SwiftUI | AVPlayer |
| [`tizen/`](tizen/) | Samsung TVs | Preact, from [`web/`](web/) | `<video>`, hls.js |
| [`webos/`](webos/) | LG TVs | Preact, from [`web/`](web/) | `<video>`, hls.js |
| [`web/`](web/) | The TV app that `tizen/` and `webos/` wrap | Preact | `<video>`, hls.js |

There's a folder per build, not per target. Android's phone and TV apps share one Gradle build, and iOS and tvOS share one Xcode project. `tizen/` and `webos/` hold only each platform's build scripts, adapter and docs.

`players/web` is the TV app, not the web UI, which is in [`web/`](../web/) at the top of the repository.

## What every player does the same

- **Sign-in:** device tokens from the pairing flow, sent as `Authorization: Bearer`. Never an API key, because those act as an admin. When `GET /api/server-info` says `guest_access`, offer "Continue as guest" (`POST /api/auth/device/guest`, `docs/guests.md`).
- **Playback:** send capabilities to `POST /items/{id}/play`, and play the plan it returns. If playback fails to start, ask again without what failed, and remember that for the device.
- **Timeline:** custom controls, using `offset_seconds` and `duration_seconds`, because an HLS session can start partway into a file.
- **API changes:** additive. Old versions stay in use for months, and removals wait for a declared minimum version (`GET /api/server-info`).
- **Look and behaviour:** [`docs/design-language.md`](../docs/design-language.md), with values from [`design/`](../design/) and Watch mode wording from `web/src/lib/wording.ts`. For TV layouts and focus, [`players/web/`](web/) is the working reference, and runs in a desktop browser.

## Testing

Emulators help with layout and focus, but don't show HDR, surround or hardware decoding problems. Playback gets tested on real devices, using the list in `docs/clients.md`.

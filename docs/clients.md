# Device players

Nov 4, 2026 · Jonas (updated Nov 5: Samsung and LG TVs, `players/`, design language)

Apps for Android, iOS, Android TV, Apple TV, Samsung TVs (Tizen) and LG TVs (webOS). Status: plan, not built. The checklist is in `docs/roadmap.md`, the shared look and behaviour in `docs/design-language.md`.

## Scope

The apps are for browsing and watching: connect to a server, sign in, home, shows, movies, series and episode pages, search, and a player with versions, tracks, resume and next episode.

Not in the first version: the Manage menu (admins use the web UI), offline downloads, Chromecast, and TV home screen rows. AirPlay comes with AVPlayer, so it only needs testing.

## Stack

| Platforms | UI | Player |
| --- | --- | --- |
| iOS, tvOS | SwiftUI | AVPlayer |
| Android, Android TV | Kotlin, Compose (Compose for TV on TV) | Media3 |
| Samsung (Tizen), LG (webOS) | React, TV screens from `web/` | `<video>` and hls.js |

- **Native on Android and Apple, web on Samsung and LG.** The player is the part that has to be right, and each platform's own player gets hardware decoding, HDR and surround without extra work. tvOS has no web view at all. Samsung and LG TV apps are web apps anyway, so they reuse the web code.
- **No bundled mpv or VLC.** The server already remuxes or transcodes what a client can't play, so the apps stay small. An app reports its capabilities and falls back when they turn out wrong (see Capabilities).
- **Phone and TV share everything except screens.** Each platform family has one API client, playback logic and data layer, with separate UI for phone and TV.
- **Same repository**, so an API change and the apps that use it land together.
- **Jellyfin-compatible API stays a later option.** Seasons aren't items here, and per-user orderings and playlists don't map onto Jellyfin's model, so those apps would hide them.

## Repository layout

```
design/            tokens.json, the design values every player shares
players/
  android/         one Gradle build: core, phone, tv
  apple/           one Xcode project: Core package, iOS and tvOS targets
  tizen/           config.xml, icons, platform adapter, packaging
  webos/           appinfo.json, icons, platform adapter, packaging
web/
  src/tv/          TV screens and focus handling, used by tizen and webos
  tv.html          second entry point; `pnpm build:tv` writes web/dist-tv
```

- **A folder per build, not per target.** Six targets come from four builds. Gradle and Xcode each want one root to share code between phone and TV, and iOS and tvOS ship under one app record.
- **TV web code stays in `web/`.** It imports `api/`, `lib/` and the player logic directly, and keeps the same lint, format and type checks. `players/tizen` and `players/webos` stay thin.

## Order

1. **Server pre-work and design tokens**, which everything needs.
2. **LG and Samsung.** They reuse the most code and need no Mac, and they're the first test of the focus model and the fallback logic that the other TV apps then follow.
3. **Android and Android TV.** No Mac needed.
4. **iOS and Apple TV.** Needs a Mac and the most outside setup.

## Samsung and LG TVs

### Packaged, not loaded from the server

The app carries its own copy of the TV screens and calls the server the person connects to. The alternative is a thin shell that opens the UI from the server, which would be same-origin and always match the server's version, with none of the server work below.

Packaged is the plan because:

- One guide says hosted Tizen apps need a partner-level certificate. That's a single third-party source, so check it with Samsung.
- The official Jellyfin Tizen client is reported to be in Samsung's store, and its build steps suggest the web client is packaged inside it. If so, this route is known to work there. Confirm by reading its repository.
- It works like the native apps: bearer tokens, additive API changes, and a connect screen that still appears when the server is down.

The cost is that UI fixes reach people through store updates, not server updates.

### What packaging needs from the server and web code

- **Bearer tokens, not cookies.** A packaged app is cross-site to the server. `SameSite=Lax` cookies aren't sent, and `SameSite=None` requires `Secure`, which rules out plain HTTP on a home network. TVs use the device tokens below.
- **CORS on `/api`**, without credentials. Find out what `Origin` each platform sends from a packaged app, then allow only what's needed. No cookies are involved, so this doesn't expose web sessions.
- **A configurable server address.** `api/client.ts`, `lib/live.ts`, artwork and play plan URLs are all relative. They go through one base URL, which stays empty on the web.
- **Short-lived media tokens.** `<img>`, CSS backgrounds and `<video src>` can't send headers. `POST /api/me/media-token` returns a token that lasts a day, passed as `?media_token=`. It only works for GETs on artwork, `/api/files/*/stream`, `/api/play/*` and `/api/sidecars/*`. The device token itself never goes in a URL: it lasts a year, and URLs end up in proxy logs. hls.js sends the bearer header for playlists and segments through `xhrSetup`, and fetches subtitles the same way.
- **No live updates at first.** `EventSource` can't send headers either. TV screens refetch when they're shown again.

### TV screens

The current UI can't simply be reused: it's built around a sidebar and the pointer, with 43 `:hover` rules against 4 `:focus` rules. `web/src/tv/` gets its own screens and shares what isn't visual: `api/`, `useApi`, `lib/wording.ts`, `lib/format.ts` and the player logic.

- **Focus:** a small spatial navigation module of our own, following the rules in `docs/design-language.md`. Use a library if that turns out harder than it looks.
- **Player:** `PlayerPage.tsx` (607 lines) splits into session logic that both UIs share and separate controls.
- **Remote:** Back is key code 461 on webOS and 10009 on Tizen. Tizen media keys have to be registered through `tizen.tvinputdevice`. The platform adapter maps these to common actions.
- **Trying it:** `pnpm dev` serves `tv.html`, and a desktop browser's arrow keys stand in for a remote.
- **Memory:** TVs have little to spare, so rails render only what's near the screen, images are requested at display size, and hls.js gets a buffer limit.

### Which TVs

Models from 2021 onward, the same floor as Jellyfin's Tizen client.

| Model year | Samsung | LG |
| --- | --- | --- |
| 2021 | Tizen 6.0, Chromium 76 | webOS 6, Chromium 79 |
| 2022 | Tizen 6.5, Chromium 85 | webOS 22, Chromium 87 |
| 2023 | Tizen 7.0, Chromium 94 | webOS 23, Chromium 94 |
| 2024 | Tizen 8.0, Chromium 108 | webOS 24, Chromium 108 |

LG's table gives platform release years, and at least one report describes a 2021 model on an older webOS, so the app checks what it can do, not the model year.

What this floor rules out:

- **CSS.** The web UI uses `color-mix` (16 places) and `:has()` (3), and `aspect-ratio` (8) and flexbox `gap`, none of which Chromium 76 has. TV screens avoid them, and any CSS shared with TV has to as well.
- **JavaScript.** The TV build sets an old `build.target`. That lowers syntax but adds no polyfills, so newer methods such as `replaceAll` need checking. Confirm in the spike that React 19 and hls.js run on Chromium 76.

### Playback

- **HDR detection fails on most of these TVs.** `caps.ts` uses `matchMedia('(dynamic-range: high)')`, which arrived in Chromium 98, so every TV before 2024 would report no HDR and get tone-mapped video. The platform adapter asks the TV's own API instead.
- **`caps.ts` tests everything through Media Source**, while direct play uses a plain `<video>`, which may play more on a TV (MKV, for one). The adapter separates the two answers.
- **`<video>` or AVPlay on Samsung.** Start with `<video>` and hls.js, the code that already exists. The evidence is mixed: one forum report has 4K H.264 failing through Media Source but 4K HEVC working on 2021 and 2022 models, one guide says most big streaming apps use AVPlay, and Dolby says a Media Source player gets at HDR more easily. The spike settles it. AVPlay goes behind the adapter if it's needed. I found nothing reliable on webOS, so it gets the same test.

### Tools

Both platforms have command line tools that run on Linux, and both TVs have a developer mode for installing test builds. Samsung's dev builds are signed with a certificate tied to the TV. Emulators help with layout and focus, but not with playback.

Sources: [Samsung web engines](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html), [LG web engines](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine), [hosted Tizen apps](https://promwad.com/company/news/how-develop-publish-tizen-apps-smart-tv-our-guide-javascript-engineers), [Jellyfin on Samsung](https://www.notebookcheck.com/Jellyfin-erhaelt-offizielle-App-fuer-Samsung-Smart-TVs.1219790.0.html), [4K on Tizen without AVPlay](https://forum.developer.samsung.com/t/how-to-play-4k-video-without-avplay/23505).

## Pre-work on the server

The playback API already suits device players: `POST /items/{id}/play` takes a client's capabilities and returns a plan. These are the gaps.

### Device sign-in

- **Tokens are sessions.** `identify` in `api/auth.rs` also accepts `Authorization: Bearer <token>`, using the same session table with a device name added. `Viewer` and the rest of the auth rules then work unchanged. API keys can't do this job, because they act as an admin.
- **Sessions last longer on devices.** The web's 30 days from last use would sign out a TV that sits unused for a month, so device sessions last a year from last use.
- **Devices can be seen and revoked.** Account lists them and can sign each one out. A new password signs out devices too, as it does other sessions.
- **One pairing flow for every sign-in type:**
  1. The app calls `POST /api/auth/pair` with its device name, and gets a short code and a secret. Both expire after ten minutes and are kept in memory, like started sign-ins.
  2. The TV shows the code and a URL or QR code. A phone opens the system browser at that URL directly.
  3. The person signs in on the web as usual (password or any provider) and approves the named device at `/pair`. Approving goes through `POST /api/me/pair`, because `/api/auth/` is public and `/api/me/` needs a signed-in user.
  4. The app polls with its secret and receives its token.
- **Phones can also use a password** directly, which avoids a browser round trip for local accounts.
- **Providers stay in the browser.** The app never sees an ID token, so the signature verification that `docs/sign-in.md` requires for browser-delivered tokens isn't needed.
- **Pairing endpoints are rate limited**, since the code is short and starting a pairing is public.

### Authenticating media requests

Players fetch streams themselves, so the token has to reach them.

- Media3 takes custom headers, so it can send the bearer token.
- AVPlayer's header option isn't public API. It can use cookies instead (`AVURLAssetHTTPCookiesKey`), set to `mc_session` with the same token. This needs no server change. Confirm it in the playback spike.
- Samsung and LG send the bearer token through hls.js and use media tokens for direct play and images (see above).

### Capabilities

Devices sometimes claim to play things they can't, and a dead-end error is the usual result. So capabilities are only a first guess, and every player recovers the same way:

1. Ask for a plan with the reported capabilities.
2. If playback fails to start (an error, or no frames within a few seconds), ask again without the codec or container that failed, at the same position.
3. Repeat until the plan is H.264 and AAC in HLS, the safest thing the server can send. Only then show an error.
4. Remember what failed on this device, so the next file skips the failed attempt. The player's stats say what happened.

The play API already takes capabilities on each call, so this needs no server change.

- `ClientCaps.hdr` is a yes or no. Clients need to report formats (`hdr10`, `hlg`, `dolby_vision`) and maximum audio channels. Keep `hdr` working for the web.
- Check that remuxing honours `audio_codecs`. `HLS_AUDIO` includes Opus and FLAC, which AVPlayer may not play in HLS.
- Check Dolby Vision files on real devices, particularly profiles without an HDR10 base layer.
- Reasons say "This browser can't play…". Change them to wording that fits any client.

### Timeline and sessions

- **Custom player controls on every platform.** HLS uses a growing `event` playlist that can start partway into a file, so a system player would show the wrong position and duration. The apps use `offset_seconds` and `duration_seconds`, and start a new session when seeking past what's converted, as the web player does.
- **A full-length playlist is a later option.** It would allow system controls on tvOS, but it's real server work, so it waits until custom controls prove inadequate.
- **Long pauses.** A session is ended after 60 seconds without requests. Apps restart at the same position when they resume. Check that the failure they see is clean.

### Subtitles

- **Android:** Media3 can load the existing WebVTT URLs.
- **Apple:** AVPlayer can't load separate subtitle files with an HLS stream, so the app draws them itself from the same URLs. Listing them in the HLS playlist would be the alternative if drawing them proves poor.
- **Picture subtitles:** these keep being burned in, which needs no client work.

### Staying compatible

Old app versions will keep calling the API, sometimes for months.

- **API changes are additive.** Removing or changing a field used by an app waits for a declared minimum version.
- **`GET /api/server-info` (public).** Returns the server's name, version and the minimum app version it supports. Apps use it on the connect screen, and it can tell people when to update.
- **Generated API clients.** Swift and Kotlin types come from `/api/openapi.json`, as the web's do.

### Reaching a server

- **iOS and tvOS:** App Transport Security blocks plain HTTP by default. `NSAllowsLocalNetworking` covers home network addresses. Test Tailscale addresses and names, and tell people to use HTTPS otherwise.
- **Android:** cleartext traffic has to be allowed in the network security config.
- **Samsung and LG:** check that a packaged app can reach plain HTTP addresses on both platforms, and what each manifest has to declare.
- Discovering servers on the local network can wait. The first version asks for an address.

### Demo server for store review

Reviewers need something to log in to. Run a small public instance with a non-admin account and openly licensed films (Big Buck Bunny, Sintel, Tears of Steel). It also doubles as a stable test target.

## Building for Apple without owning a Mac

iOS and tvOS apps can only be built with Xcode on macOS. Nothing else in this project needs a Mac.

- **macOS in a VM on Linux: no.** Projects like OSX-KVM make it possible, but Apple's licence only allows macOS on Apple-branded hardware. It's also a poor fit technically: macOS 26 is the last release for Intel Macs, so a virtual Intel Mac eventually can't run the Xcode that store uploads require. This isn't worth risking on the developer account the apps ship from.
- **CI runners.** GitHub Actions macOS runners build and sign the app and upload it to TestFlight, which installs it on a real iPhone or Apple TV. This covers release builds and most testing, but there's no simulator and each try takes a while. It needs a Git remote, which also unblocks the CI item in Phase 0.
- **Rented Mac.** A real Mac in a data centre, used over remote desktop, with Xcode and the simulator. Rent one monthly for the Apple phases only. Physical devices still go through TestFlight.
- **Used Mac mini.** Any Apple Silicon model will do, kept on a shelf and reached over SSH and screen sharing. It pays for itself in a few months compared to renting.

**The plan:** Samsung, LG and Android come first and need no Mac. Rent a Mac for a day or two for the playback spike, then monthly during the Apple phases, and use CI for release builds. Buy a used Mac mini only if the rental keeps getting renewed.

A Mac doesn't replace the rest: an Apple Developer membership (about $99 a year), an iPhone and an Apple TV are needed either way.

## Effort

Judgement calls, not measurements. The right-hand column assumes the pace of this repository so far, and is the less certain of the two.

| Work | Solo developer | With AI |
| --- | --- | --- |
| Pre-work | 2–3 wk | 1 wk |
| Design tokens and language | 1 wk | 1–2 days |
| Samsung and LG | 5–8 wk | 1–2 wk |
| Android and Android TV | 6–10 wk | 1–2 wk |
| iOS and tvOS | 6–10 wk | 1–2 wk |
| Device testing and stores | 4–6 wk | 2–3 wk |
| Total | 6–9 months | 7–11 weeks |

## Stores

- **Samsung and LG:** each has its own seller registration and app review. Their requirements haven't been checked yet, including whether either has a problem with an app that only works with a person's own server. Read both before building the TV screens. Review uses the demo server.

- **Google Play:** a one-time fee of about $25. New personal accounts must run a closed test with at least 12 testers, opted in for 14 days straight, before applying for production. Organisation accounts are generally exempt, but need a D-U-N-S number. TV apps are reviewed separately for TV requirements. Start finding testers early.
- **App Store:** review uses the demo server. iOS and tvOS can ship under one app record.
- **Licence:** choose it before publishing. GPL-family licences are known to conflict with App Store terms, so this choice limits where the apps can ship.

## Testing

Emulators don't show HDR, surround or hardware decoding problems, so playback gets tested on real devices:

- **Devices:** an iPhone, an Apple TV 4K, an Android phone, an Android TV device, a Samsung TV and an LG TV, and an HDR TV with a surround receiver or soundbar if possible. Include a 2021 model for Samsung or LG, since that's the oldest engine supported.
- **Files:** H.264, HEVC, AV1 and VP9; HDR10, HLG and Dolby Vision; AAC, AC-3, E-AC-3, TrueHD, DTS, Opus and FLAC; text and picture subtitles.
- **Situations:** resume, seeking past what's converted, a long pause, switching tracks, losing the network, remote access through a reverse proxy.

`fixtures` and `scripts/make-fixtures.sh` are the starting point for the test files.

## Open questions

- Do the apps ship with v1 or follow it as 1.x? The spec says native apps aren't a v1 deliverable, which points to 1.x.
- Which Samsung and LG TVs are available to test on? This could change the order or the 2021 floor.

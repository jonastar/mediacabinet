# Device players

Nov 4, 2026 · Jonas (updated Nov 5: Samsung and LG TVs, `players/`, design language)

Apps for Android, iOS, Android TV, Apple TV, Samsung TVs (Tizen) and LG TVs (webOS). Status: the server pre-work below is built, the apps aren't. The checklist is in `docs/roadmap.md`, the shared look and behaviour in `docs/design-language.md`.

## Scope

The apps are for browsing and watching: connect to a server, sign in, home, shows, movies, series and episode pages, search, and a player with versions, tracks, resume and next episode.

Not in the first version: the Manage menu (admins use the web UI), offline downloads, Chromecast, and TV home screen rows. AirPlay comes with AVPlayer, so it only needs testing.

## Stack

| Platforms | UI | Player |
| --- | --- | --- |
| iOS, tvOS | SwiftUI | AVPlayer |
| Android, Android TV | Kotlin, Compose (Compose for TV on TV) | Media3 |
| Samsung (Tizen), LG (webOS) | Preact, TV screens from `web/` | `<video>` and hls.js |

- **Native on Android and Apple, web on Samsung and LG.** The player is the part that has to be right, and each platform's own player gets hardware decoding, HDR and surround without extra work. tvOS has no web view at all. Samsung and LG TV apps are web apps anyway, so they reuse the web code.
- **No bundled mpv or VLC.** The server already remuxes or transcodes what a client can't play, so the apps stay small. An app reports its capabilities and falls back when they turn out wrong (see Capabilities).
- **Phone and TV share everything except screens.** Each platform family has one API client, playback logic and data layer, with separate UI for phone and TV.
- **Same repository**, so an API change and the apps that use it land together.
- **Jellyfin-compatible API stays a later option.** Seasons aren't items here, and per-user orderings and playlists don't map onto Jellyfin's model, so those apps would hide them.

## Repository layout

```
design/            tokens.json, the design values every player shares
players/
  README.md        what's shared and how the folders fit together
  android/         one Gradle build: core, phone, tv
  apple/           one Xcode project: Core package, iOS and tvOS targets
  web/             the TV app: Preact screens and focus handling, built for tizen and webos
  tizen/           config.xml, icons, platform adapter, build scripts, docs
  webos/           appinfo.json, icons, platform adapter, build scripts, docs
web/
  src/core/        plain TypeScript shared by the web UI and the TV app
```

- **A folder per build, not per target.** Six targets come from four builds. Gradle and Xcode each want one root to share code between phone and TV, and iOS and tvOS ship under one app record.
- **The TV app is its own project in `players/web`.** It has its own dependencies and build settings, so Preact, the legacy plugin and polyfills can't reach the web UI, and web UI CSS can't slip into the TV app. It shares only `web/src/core/`, through an alias, and uses the same lint and format rules. The Docker build builds it beside the web UI, since the server hosts it at `/tv-mode/` (below). `players/tizen` and `players/webos` stay thin.
- **The core has no framework or CSS.** It holds the API client, playback session, fallback, capabilities, wording and formatting. A lint rule stops it importing React or CSS, and polyfills load only from the TV app's entry point, so the web UI isn't affected. It has no runtime dependencies either, because its imports would resolve against `web/node_modules` even in a TV build, so each app passes in hls.js. Swift and Kotlin can't use the code, but it's the reference for how playback should behave.

## Order

The apps follow v1 as 1.x. Android might be ready for 1.0, but 1.0 doesn't wait for it.

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
- **CORS on `/api`**, without credentials. No cookies are involved, so this doesn't expose web sessions. Allowed for now: `null` and `file://`, what packaged apps are expected to send, plus any in `CABINET_CORS_ORIGINS`. The TV spike finds out what each platform really sends, and the list narrows to that. Sign-in endpoints are opt-in (only status, logout and pairing): above all, CORS must not reach `/api/auth/setup`, or any page someone visits could claim a new server on their network.
- **A configurable server address.** `api/client.ts`, `lib/live.ts`, artwork and play plan URLs are all relative. They go through one base URL, which stays empty on the web.
- **Short-lived media tokens.** `<img>`, CSS backgrounds and `<video src>` can't send headers. `POST /api/me/media-token` returns a token that lasts a day, passed as `?media_token=`. It only works for GETs on artwork, people's photos, streams, HLS files and subtitles, and ends with its session. The device token itself never goes in a URL: it lasts a year, and URLs end up in proxy logs. hls.js sends the bearer header for playlists and segments through `xhrSetup`, and fetches subtitles the same way.
- **No live updates at first.** `EventSource` can't send headers either. TV screens refetch when they're shown again.

### TV screens

The current UI can't simply be reused: it's built around a sidebar and the pointer, with 43 `:hover` rules against 4 `:focus` rules. `players/web` gets its own screens and shares what isn't visual through `web/src/core/`.

- **One small app for every TV, on Preact.** There's no separate minimal player: two TV interfaces would be more to maintain, mainly for a shrinking set of old TVs. Preact is about 4 KB gzipped against about 55 KB for React, which helps startup and memory on budget new TVs as well as old ones. Newer TVs still get HDR, better codecs and 4K, because those come from capabilities and the server. It has its own small hooks over the core, so nothing needs `preact/compat`. Plain TypeScript with no framework was the alternative, but rails, focus and lists would soon need a homemade framework.
- **Reference for other TV apps.** This app is what Android TV and tvOS follow for layout and focus (see `docs/design-language.md`).

- **Focus:** a small spatial navigation module of our own, following the rules in `docs/design-language.md`. Use a library if that turns out harder than it looks.
- **Player:** the playback session is in `src/core/session.ts`, and each UI has its own controls. The TV app uses it; `PlayerPage.tsx` shares the parts below it (`stream.ts`, `watchdog.ts`, `fallback.ts`, `caps.ts`) and still has to move onto it. `hls.light` works for direct, remuxed and seeking so far, since the server sends one rendition and the player draws subtitles itself.
- **Remote:** Back is key code 461 on webOS and 10009 on Tizen. Tizen media keys have to be registered through `tizen.tvinputdevice`. The platform adapter maps these to common actions.
- **Trying it:** `pnpm dev` in `players/web`, and a desktop browser's arrow keys stand in for a remote.
- **Memory:** TVs have little to spare, so rails render only what's near the screen, images are requested at display size, and hls.js gets a buffer limit.

### Which TVs

As many as possible, but two separate limits apply: what the stores let a new app reach, and what the app can run on.

**The stores come first, and neither publishes a list that could be found:**

- **Samsung: probably 2021 onward.** Jellyfin's store app launched in February 2026 for 2021 and newer only. SamMobile says Samsung "appears to have disabled" publishing new apps for 2020 and older, without citing a source. Samsung's docs only say developers choose from a list of model groups, so a seller account will show the real list.
- **LG: unknown.** One vendor page says webOS 4 and up (2018 onward), and one Jellyfin guide says webOS 3 "may not" be supported. Both are weak sources. Ask through Seller Lounge 1:1 Q&A.

**The app aims lower than that where it costs little.** The technical goal is Samsung from 2017 and LG from 2016, the oldest Chromium-based TVs from each.

- **Older Samsung TVs** that the store can't reach would sideload a `.wgt`, as older TVs do for Jellyfin today. It's a bonus, not a promise.
- **The LG answer decides the real floor:** Chromium 53 if the store stops at webOS 4, Chromium 38 if it reaches webOS 3.
- **WebKit models and older** aren't planned. They'd need older flexbox and possibly older-style HLS from the server.
- **The server also hosts the build at `/tv-mode/`** (`/tv` is the web UI's shows), and the web UI links to it as TV mode. Any TV can open it in its built-in browser with no install, and any other window gets the 1920×1080 layout zoomed to fit. Being same-origin, it skips connecting and pairing: the web UI's session cookie signs requests in, and a signed-out visit goes to the web UI to sign in. TV browsers can be limited for video and may not capture Back, so it's an extra, not the main route.

| Model year | Samsung | LG |
| --- | --- | --- |
| 2014 | | webOS 1, WebKit |
| 2015 | Tizen 2.3, WebKit | webOS 2, WebKit |
| 2016 | Tizen 2.4, WebKit | webOS 3, Chromium 38 |
| 2017 | Tizen 3.0, Chromium 47 | webOS 3, Chromium 38 |
| 2018 | Tizen 4.0, Chromium 56 | webOS 4, Chromium 53 |
| 2019 | Tizen 5.0, Chromium 63 | webOS 4, Chromium 53 |
| 2020 | Tizen 5.5, Chromium 69 | webOS 5, Chromium 68 |
| 2021 | Tizen 6.0, Chromium 76 | webOS 6, Chromium 79 |
| 2022 | Tizen 6.5, Chromium 85 | webOS 22, Chromium 87 |
| 2023 | Tizen 7.0, Chromium 94 | webOS 23, Chromium 94 |
| 2024 | Tizen 8.0, Chromium 108 | webOS 24, Chromium 108 |

LG's table gives platform release years, and at least one report describes a 2021 model on an older webOS, so the app checks what it can do, not the model year. Engines don't change with firmware updates, according to one third-party guide.

**One build for every TV:** ES5 with polyfills. Newer TVs run the same bundle, slightly larger than they need, and there's only one thing to test and ship.

**JavaScript: setting a target isn't enough.** A trial build of the current web app on Vite 8.3 showed the points below. Building `players/web` found more, and `scripts/check-build.mjs` there now fails the build on all of them:

- The plugin minifies legacy chunks with Oxc for ES2015, which brings back template literals, so the build uses Terser.
- Its polyfills chunk skips Babel, so the bundler's helpers stay as arrow functions, and the config lowers them.
- `crossorigin` on script tags blocks them from `file://`, where a packaged app runs, so every TV would show a blank screen. The config strips it.
- In legacy-only mode the CSS is carried inside the JavaScript, and `cssCodeSplit: false` drops it altogether. It stays inside, and the check reads it from there.
- `build.target: 'es5'` is refused, because Rolldown only goes down to ES2015.
- `chrome47` and `chrome38` build without complaint, but the output keeps classes and destructuring that those engines can't parse. The `chrome47` bundle is the same as the `es2015` one, so a passing build proves nothing.
- `@vitejs/plugin-legacy` is the route to ES5: it has a release for Vite 8, and uses Babel, core-js and SystemJS. Set it to legacy chunks only. It's untested here, and is the spike's first step.
- core-js doesn't cover browser APIs. `fetch` (missing on Chromium 38), `AbortController`, `URLSearchParams` and `ResizeObserver` need their own polyfills or a way around them.
- `tsconfig`'s target doesn't matter, since `tsc` only checks types here (`noEmit`).
- A few things can't be compiled or polyfilled: regex lookbehind, `BigInt` and `Proxy`. `video.play()` only returns a promise from Chromium 50, so `.play().catch()` crashes before that.

**CSS: the target does nothing.** The build leaves CSS as written, and the web UI's built CSS has 684 `var()` uses, 25 grids and 224 `gap`s. These limits set how the TV screens are written:

| Feature | Needs | So on TV |
| --- | --- | --- |
| `var()` | Chromium 49 | Tokens become fixed values in `tv.css` |
| Grid | Chromium 57 | Flexbox with margins |
| Flexbox `gap` | Chromium 84 | Margins |
| `aspect-ratio` | Chromium 88 | Fixed sizes |
| Variable fonts | Chromium 62 | Static Manrope files |
| `#RRGGBBAA`, `rgb(0 0 0 / 50%)` | Chromium 62, 65 | `rgba()` with commas |
| `:focus-visible` | Chromium 86 | A class set by the focus module |
| `:has()`, `color-mix` | Chromium 105, 111 | Not used |

The TV screens are new, so this costs little if it's followed from the first line, and a lot if it's fixed later. Nothing from the web UI's CSS is imported. `players/web/README.md` has the rules, and a check in the TV build should fail on these features.

**What could still move the floor:**

- **Preact** should run on these engines, which is one reason to use it over React 19, whose requirements aren't documented anywhere I could find. Confirm it in the spike.
- **hls.js** expects `Map`, `Set`, `Promise` and similar. Its fMP4 HLS, the only kind the server makes, may not work on the oldest Media Source versions. One write-up calls Chromium 47's a 2015-era implementation.
- **HTTPS:** old TVs may not trust current certificates or support what proxies require. Plain HTTP on a home network isn't affected.
- **Performance and memory** on the oldest hardware. This one only shows up on a real TV.

Older TVs get a simpler result, not a broken one. The server converts what they can't play, usually to H.264 in SDR, and animations can fall back to the static mark.

### Playback

- **HDR detection fails on most of these TVs.** `caps.ts` uses `matchMedia('(dynamic-range: high)')`, which arrived in Chromium 98, so every TV before 2024 would report no HDR and get tone-mapped video. The platform adapter asks the TV's own API instead.
- **`caps.ts` tests everything through Media Source**, while direct play uses a plain `<video>`, which may play more on a TV (MKV, for one). The adapter separates the two answers.
- **`<video>` or AVPlay on Samsung.** Start with `<video>` and hls.js, the code that already exists. The evidence is mixed: one forum report has 4K H.264 failing through Media Source but 4K HEVC working on 2021 and 2022 models, one guide says most big streaming apps use AVPlay, and Dolby says a Media Source player gets at HDR more easily. The spike settles it. AVPlay goes behind the adapter if it's needed. I found nothing reliable on webOS, so it gets the same test.

### Tools

Both platforms have command line tools that run on Linux, and both TVs have a developer mode for installing test builds. Samsung's dev builds are signed with a certificate tied to the TV. Emulators help with layout and focus, but not with playback.

Sources: [Samsung web engines](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html), [LG web engines](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine), [hosted Tizen apps](https://promwad.com/company/news/how-develop-publish-tizen-apps-smart-tv-our-guide-javascript-engineers), [Jellyfin on Samsung](https://www.notebookcheck.com/Jellyfin-erhaelt-offizielle-App-fuer-Samsung-Smart-TVs.1219790.0.html), [4K on Tizen without AVPlay](https://forum.developer.samsung.com/t/how-to-play-4k-video-without-avplay/23505), [SamMobile on publishing for older TVs](https://www.sammobile.com/news/jellyfin-app-samsung-tv-launched-plex-alternative/), [Samsung Seller Office](https://developer.samsung.com/tv-seller-office/guides/applications/distributing-application.html), [Android Authority on Jellyfin's cutoff](https://www.androidauthority.com/jellyfin-samsung-tvs-3637724/).

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
- **Phones can also use a password** directly through `POST /api/auth/device`, which avoids a browser round trip for local accounts.
- **Providers stay in the browser.** The app never sees an ID token, so the signature verification that `docs/sign-in.md` requires for browser-delivered tokens isn't needed.
- **Pairing endpoints are rate limited**, since the code is short and starting a pairing is public: 30 starts a minute overall, and 10 wrong codes per user in ten minutes. Polling isn't limited, because the secret can't be guessed.
- **A QR code or link carries the code** (`path` in the response, `/pair?code=…`), and it survives a round trip to a sign-in provider.

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

The play API already takes capabilities on each call, so this needs no server change. The steps are in `web/src/core/fallback.ts`, and the web player uses them, cheapest first: the container, then the audio, then HDR, then the video codec. Two details matter for other players:

- **Only a step that worked is remembered.** A broken file fails every step, so it teaches nothing about the device. Neither does a failure partway through a file. H.264 and AAC can be ruled out for one file, which forces 10-bit H.264 through the encoder, but are never remembered.
- **"No frames" means data is there and nothing plays:** not paused, at least 2 seconds buffered and nothing played for 8 seconds. A slow network or a blocked autoplay isn't a failure.

Capabilities a client reports:

- `hdr_formats` lists formats (`hdr10`, `hlg`, `dolby_vision`), and others are tone-mapped. It wins over `hdr`, the older yes or no the web still sends.
- `max_audio_channels` converts audio with more channels to stereo AAC. Without a limit, surround is copied if `audio_codecs` lists its codec.
- Remuxing honours `audio_codecs`, so a client just leaves out what it can't play in HLS, as AVPlayer may with Opus and FLAC.
- Still to do: check Dolby Vision files on real devices, particularly profiles without an HDR10 base layer. The scan records `dolby_vision` without a profile, so a client that lists it claims all of them.

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

- **API changes are additive.** Removing or changing a field used by an app waits for a declared minimum version (`docs/releasing.md`).
- **`GET /api/server-info` (public).** Returns the server's name (`CABINET_SERVER_NAME`), version and the minimum app version it supports. Apps use it on the connect screen, and it can tell people when to update.
- **Generated API clients.** Swift and Kotlin types come from `/api/openapi.json`, as the web's do.

### Reaching a server

- **iOS and tvOS:** App Transport Security blocks plain HTTP by default. `NSAllowsLocalNetworking` covers home network addresses. Test Tailscale addresses and names, and tell people to use HTTPS otherwise.
- **Android:** cleartext traffic has to be allowed in the network security config.
- **Samsung and LG:** check that a packaged app can reach plain HTTP addresses on both platforms, and what each manifest has to declare.
- Discovering servers on the local network can wait. The first version asks for an address.

### Demo server for store review

Reviewers need something to log in to. Run a small public instance with openly licensed films (Big Buck Bunny, Sintel, Tears of Steel) and guest access on, so apps offer "Continue as guest" (`docs/guests.md`); a non-admin account as well, for stores that want a username and password. It also doubles as a stable test target and as the website's demo.

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
| Samsung and LG, back to 2016–17 | 6–10 wk | 2–3 wk |
| Android and Android TV | 6–10 wk | 1–2 wk |
| iOS and tvOS | 6–10 wk | 1–2 wk |
| Device testing and stores | 4–6 wk | 2–3 wk |
| Total | 6–9 months | 7–11 weeks |

## Stores

- **Samsung and LG:** each has its own seller registration and app review. Register with both early, because that's the way to learn which TVs a new app can reach. Also check whether either has a problem with an app that only works with a person's own server. Review uses the demo server.

- **Google Play:** a one-time fee of about $25. New personal accounts must run a closed test with at least 12 testers, opted in for 14 days straight, before applying for production. Organisation accounts are generally exempt, but need a D-U-N-S number. TV apps are reviewed separately for TV requirements. Start finding testers early.
- **App Store:** review uses the demo server. iOS and tvOS can ship under one app record.
- **Licence:** choose it before publishing. GPL-family licences are known to conflict with App Store terms, so this choice limits where the apps can ship.

## Testing

Emulators don't show HDR, surround or hardware decoding problems, so playback gets tested on real devices:

- **Devices:** an iPhone, an Apple TV 4K, an Android phone, an Android TV device, a Samsung TV and an LG TV, and an HDR TV with a surround receiver or soundbar if possible. Samsung and LG need several ages: the oldest model we claim to support (2017 Samsung, 2016–17 LG), one from around 2020, and a current one. Used older TVs are cheap, and no claim about a model year is made without one.
- **Files:** H.264, HEVC, AV1 and VP9; HDR10, HLG and Dolby Vision; AAC, AC-3, E-AC-3, TrueHD, DTS, Opus and FLAC; text and picture subtitles.
- **Situations:** resume, seeking past what's converted, a long pause, switching tracks, losing the network, remote access through a reverse proxy.

`fixtures` and `scripts/make-fixtures.sh` are the starting point for the test files.

## Open questions

- Why does playback restart in a loop in Samsung's TV Simulator, after a first frame and a little audio? There are no logs to go on. The app's own endless retry was fixed and it still happens, so the simulator's known trouble with video on Windows is the likely cause. Check on an emulator or a real TV.
- Does `players/tizen/config.xml` work? Nothing has installed it yet, since the Simulator ignores it, and the `.wgt` has to be signed on a computer with a desktop session first. The app ID is a placeholder.
- Does video show on an old LG TV? On the webOS 3.0 emulator the app starts, pairs and browses, and streams load with the clock moving, but there's no picture or sound. LG doesn't list HLS on emulators before 5.0, so only a real TV can settle it.
- Should a stream that plays with no picture count as failed? The watchdog only checks that time moves, so a TV doing what that emulator did would show black with no error. Checking `videoWidth` shortly after starting would catch it.
- Which Samsung and LG TVs are available to test on? This could change the order or the floor.
- Which TVs can a new app reach in each store? Seller accounts answer this, and it sets the floor.
- How far back does the app actually run? `players/web` is ready to answer this: connect, press play and watch a stream, built to ES5, on the oldest TV available. Its output parses as ES5 and it runs from `file://` on a current engine, but it hasn't run on an old one yet.

# Android and Android TV

Kotlin, Compose (Compose for TV on TV) and Media3. See [`docs/clients.md`](../../docs/clients.md). This may be ready for 1.0, but 1.0 doesn't wait for it.

**Status:** the phone app connects, signs in (password, browser, guest), shows Home and series pages, and plays with custom controls. It builds and its core is tested, and `LiveServerTest` has played plans from a real library (direct, and transcoded to HLS) with the token, but the app hasn't run on a device yet. The TV app isn't started.

## Layout

One Gradle build:

- `core`: the API client, account and HTTP client, capabilities, the fallback rules, the playback session, and wording. Used by both apps.
  - `api/Models.kt` is generated from `web/src/api/openapi.json` by `scripts/generate-api-clients.mjs`, which `pnpm api:types` in `web/` runs. Don't edit it. Calls are written by hand in `api/Api.kt`, like `players/web/src/api.ts`.
  - `Tokens.kt` is generated from `design/tokens.json` (`pnpm tokens`).
  - `playback/`, `Labels.kt` and `Wording.kt` are ports of `web/src/core/` and `web/src/lib/wording.ts`, which stay the reference. Keep them in step; the unit tests repeat the web's cases.
- `phone`: phone and tablet screens.
- `tv`: Android TV screens with D-pad focus. Not there yet.

## Building

Needs JDK 17 or later and the Android SDK (`sdk.dir` in `local.properties`, which isn't committed). No Android Studio needed.

```sh
./gradlew :phone:assembleDebug      # phone/build/outputs/apk/debug/phone-debug.apk
./gradlew :core:testDebugUnitTest   # the core's unit tests
./gradlew :phone:lintDebug
```

### Release signing

A release build (`./gradlew :phone:assembleRelease :phone:bundleRelease`) is signed when `CABINET_KEYSTORE` points at a keystore, with `CABINET_KEYSTORE_PASSWORD`, `CABINET_KEY_ALIAS` and, if it differs from the keystore's, `CABINET_KEY_PASSWORD`. Without them it's unsigned. Make the keystore once, and keep it and its password safe: every update has to be signed with the same key.

```sh
keytool -genkeypair -v -keystore release.jks -alias media-cabinet -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 release.jks   # for the ANDROID_KEYSTORE_BASE64 secret
```

`.github/workflows/release-android.yml` takes them from the repository secrets `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`.

Google Play re-signs apps with its own key unless it's given yours. When setting up Play App Signing, give it this key, so the APKs on GitHub and the app from Play are signed the same way and update each other.

`LiveServerTest` decodes a real server's responses with the generated models, when pointed at one:

```sh
CABINET_TEST_SERVER=http://127.0.0.1:8420 CABINET_TEST_NAME=… CABINET_TEST_PASSWORD=… \
  ./gradlew :core:testDebugUnitTest --tests '*LiveServerTest'
```

## Running on a device

Over Wi-Fi, from the machine that builds:

1. On the phone, turn on Developer options, then **Wireless debugging**. Choose **Pair device with pairing code**, and run `adb pair <ip>:<port> <code>` with what it shows.
2. `adb connect <ip>:<port>`, with the address on the Wireless debugging screen (not the pairing one).
3. `./gradlew :phone:installDebug`, then open Media Cabinet.

An Android TV device works the same way, or with **Network debugging** and `adb connect <ip>:5555` on older ones. `scrcpy` shows the device's screen on a computer. The emulator needs KVM, so it doesn't run on a machine without it.

## Things to know

- **Streams:** Media3 fetches through the same OkHttp client as the API, so the bearer token goes with every request to the signed-in server, and only to it. Images use it too (Coil).
- **HLS sessions look live to Media3,** since the playlist grows as ffmpeg works. The session plays them from the start at normal speed, and uses the plan's `offset_seconds` for the timeline.
- **Failures:** a stream Media3 can't decode often plays on without its picture or sound, so the session treats an unsupported video or audio track as a failure and falls back, as well as errors and the no-picture watchdog.
- **Plain HTTP and own certificates:** both are allowed in the network security config, for servers on a home network or behind a private certificate authority.
- **Subtitles:** drawn by the app from the server's WebVTT, like the TV app. The server shifts cues by the plan's `offset_seconds`, so they're timed against the stream, not the file.
- **Sizes:** Android TV lays out at 960×540 dp, so TV values in `docs/design-language.md` are halved here.
- **TV screens:** follow the layouts and focus behaviour of [`players/web/`](../web/), and use what Compose for TV does well on top.
- **One app ID:** phone and TV both use `tv.mediacabinet`. Google Play takes a separate build for its Android TV track.
- **Manrope** is bundled in `core` (`res/font/manrope.ttf`, the variable font), under the SIL Open Font License in `core/fonts-OFL.txt`.
- **Google Play:** new personal accounts need a closed test with 12 testers for 14 days before production, and TV apps are reviewed separately. Start finding testers early.

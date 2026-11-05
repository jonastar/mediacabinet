# Android and Android TV

Kotlin, Compose (Compose for TV on TV) and Media3. Not started. See [`docs/clients.md`](../../docs/clients.md). This may be ready for 1.0, but 1.0 doesn't wait for it.

## Planned layout

One Gradle build:

- `core`: API client generated from `/api/openapi.json`, data layer, playback logic
- `phone`: phone and tablet screens
- `tv`: Android TV screens with D-pad focus

## Things to know

- **Streams:** Media3 takes custom headers, so it sends the bearer token directly.
- **Plain HTTP:** cleartext traffic has to be allowed in the network security config, for servers on a home network.
- **Subtitles:** Media3 can load the server's WebVTT URLs.
- **Sizes:** Android TV lays out at 960×540 dp, so TV values in `docs/design-language.md` are halved here.
- **TV screens:** follow the layouts and focus behaviour of [`players/web/`](../web/), and use what Compose for TV does well on top.
- **Google Play:** new personal accounts need a closed test with 12 testers for 14 days before production, and TV apps are reviewed separately. Start finding testers early.

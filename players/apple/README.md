# iOS, iPadOS and Apple TV

SwiftUI and AVPlayer. Not started. See [`docs/clients.md`](../../docs/clients.md).

## Planned layout

One Xcode project:

- `Core`: a Swift package with the API client generated from `/api/openapi.json`, data layer and playback logic
- an iOS target for iPhone and iPad
- a tvOS target with focus and Siri Remote scrubbing

## Things to know

- **A Mac is required.** Xcode only runs on macOS. `docs/clients.md` covers renting one, CI builds through TestFlight, and why macOS in a VM isn't an option.
- **Streams:** AVPlayer's header option isn't public API, so the token goes in the `mc_session` cookie (`AVURLAssetHTTPCookiesKey`). This needs confirming in the playback spike.
- **Subtitles:** AVPlayer can't load separate subtitle files with an HLS stream, so the app draws them itself from the WebVTT URLs.
- **Plain HTTP:** App Transport Security blocks it by default. `NSAllowsLocalNetworking` covers home network addresses. Tailscale addresses and names need testing.
- **TV screens:** follow the layouts and focus behaviour of [`players/web/`](../web/), with tvOS's own focus effect and blur on top.
- **Licence:** MIT, chosen because GPL-family licences conflict with App Store terms.
- **Store:** iOS and tvOS can ship under one app record. Review needs the demo server.

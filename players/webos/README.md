# LG TVs (webOS)

A packaged web app. The app itself is in [`players/web/`](../web/). This folder holds LG's build scripts, adapter and docs. Not started. See [`docs/clients.md`](../../docs/clients.md).

## Planned contents

- `appinfo.json`: the app's manifest
- icons from `docs/brand/`
- the platform adapter: capabilities, HDR, remote keys
- a build script that wraps `players/web/dist` into an `.ipk`

## Things to know

- **Which TVs:** unknown until LG answers which versions its store reaches for a new app (Seller Lounge 1:1 Q&A). That makes the floor Chromium 53 if it stops at webOS 4, or Chromium 38 if it reaches webOS 3. The app aims to run back to 2016. 2014 and 2015 models use WebKit and aren't planned.
- **Remote:** Back is key code 461.
- **HDR:** `matchMedia('(dynamic-range: high)')` needs Chromium 98, so the adapter asks the TV's own API.
- **Player:** starts with `<video>` and hls.js. Nothing reliable was found on HDR and Dolby Vision limits here, so the spike tests them.
- **Model years:** LG's table gives platform release years, and models don't always match them. Check what the TV can do, not its year.
- **Network:** check that a packaged app can reach plain HTTP servers.
- **Store:** requirements haven't been read yet. Register as a seller before the TV screens get built.

# LG TVs (webOS)

A packaged web app. The app itself is in [`players/web/`](../web/). This folder holds LG's manifest and packaging. See [`docs/clients.md`](../../docs/clients.md).

## Putting it on a TV

Works in the webOS 6.0 Simulator, which skips packaging. Not tried on a TV yet, so the `.ipk` and manifest may need changes. You need the Developer Mode app on the TV. LG's CLI is a local dependency here, so `pnpm install` in this folder sets it up.

```sh
pnpm install
./package.sh                                            # writes build/*.ipk from players/web
pnpm exec ares-setup-device                             # once, to add the TV as "tv"
pnpm exec ares-novacom --device tv --getkey             # once, with the passphrase the TV shows
pnpm exec ares-install --device tv build/*.ipk
pnpm exec ares-launch --device tv app.mediacabinet.tv
```

These need to run on a machine that can reach the TV over the network. If that isn't this one, copy the `.ipk` and run the install there with LG's CLI.

## Still to come

- the platform adapter: capabilities, HDR. Today's is generic, in `players/web/src/platform.ts`
- proper icon sizes for the store

## Things to know

- **Which TVs:** unknown until LG answers which versions its store reaches for a new app (Seller Lounge 1:1 Q&A). That makes the floor Chromium 53 if it stops at webOS 4, or Chromium 38 if it reaches webOS 3. The app aims to run back to 2016. 2014 and 2015 models use WebKit and aren't planned.
- **Remote:** Back is key code 461.
- **HDR:** `matchMedia('(dynamic-range: high)')` needs Chromium 98, so the adapter asks the TV's own API.
- **Player:** starts with `<video>` and hls.js. Nothing reliable was found on HDR and Dolby Vision limits here, so the spike tests them.
- **Model years:** LG's table gives platform release years, and models don't always match them. Check what the TV can do, not its year.
- **Network:** check that a packaged app can reach plain HTTP servers.
- **Store:** requirements haven't been read yet. Register as a seller before the TV screens get built.

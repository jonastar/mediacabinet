# Samsung TVs (Tizen)

A packaged web app. The app itself is in [`players/web/`](../web/). This folder holds Samsung's manifest and packaging. See [`docs/clients.md`](../../docs/clients.md).

## Putting it on a TV

Not tried on a TV yet, so the manifest may need changes. You need [Tizen Studio's CLI](https://developer.samsung.com/smarttv/develop/getting-started/setting-up-sdk/installing-tv-sdk.html), a signing profile tied to the TV, and Developer Mode on the TV.

```sh
./package.sh                                  # writes build/*.wgt from players/web
sdb connect <the TV's address>
tizen install -n build/MediaCabinet.wgt -t <the TV's name in "sdb devices">
```

## Still to come

- the platform adapter: capabilities, HDR. Today's is generic, in `players/web/src/platform.ts`
- proper icon sizes for the store

## Things to know

- **Which TVs:** the store probably only reaches 2021 onward (Tizen 6.0, Chromium 76), which a seller account will confirm. The app aims to run back to 2017 (Tizen 3.0, Chromium 47), with older models sideloading the `.wgt` as a bonus. 2015 and 2016 models use WebKit and aren't planned.
- **Remote:** Back is key code 10009. Media keys have to be registered through `tizen.tvinputdevice`.
- **HDR:** `matchMedia('(dynamic-range: high)')` needs Chromium 98, so the adapter asks the TV's own API.
- **Player:** starts with `<video>` and hls.js. AVPlay goes behind the adapter if the spike shows it's needed, for example for 4K H.264.
- **Signing:** builds are signed, and test builds use a certificate tied to the TV.
- **Network:** check that a packaged app can reach plain HTTP servers, and what `config.xml` has to declare.
- **Store:** requirements haven't been read yet. Register as a seller before the TV screens get built.

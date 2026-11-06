# Samsung TVs (Tizen)

A packaged web app. The app itself is in [`players/web/`](../web/). This folder holds Samsung's manifest and packaging. See [`docs/clients.md`](../../docs/clients.md).

## Putting it on a TV

The UI, pairing and navigation work in Samsung's TV Simulator, which ignores the manifest and signing. Not tried on an emulator or a TV yet, so the manifest may need changes.

```sh
./package.sh     # writes build/app/ and build/MediaCabinet-unsigned.wgt from players/web
```

**TV Simulator:** open `build/app/`.

**Emulator or real TV:** the package has to be signed, and that needs Samsung's tools on your own computer, because `tz` couldn't read a profile's password back over SSH on Linux (probably no keyring there). In the Tizen extension or Tizen Studio there, create a signing profile in Certificate Manager (a real TV needs a Samsung certificate tied to it, with Developer Mode on), then:

```sh
tz pack -t wgt -b MediaCabinet-unsigned.wgt -s <profile> -o MediaCabinet.wgt
tz install -p MediaCabinet.wgt -t <device>
```

Samsung's emulator needs hardware virtualisation. On AMD processors, that means TV image 8.0.1 or later with Windows Hypervisor Platform turned on. It's untested here so far.

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

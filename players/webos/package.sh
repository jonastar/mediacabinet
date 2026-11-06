#!/bin/sh
# Builds the TV app and wraps it into an .ipk in build/.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/../..
(cd "$root/players/web" && pnpm build)
rm -rf "$here/build"
mkdir -p "$here/build/app"
cp -R "$root/players/web/dist/." "$here/build/app/"
cp "$here/appinfo.json" "$here/build/app/"
cp "$root/docs/brand/icons/android-maskable-192.png" "$here/build/app/icon.png"
ares-package --outdir "$here/build" "$here/build/app"

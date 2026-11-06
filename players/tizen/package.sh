#!/bin/sh
# Builds the TV app and wraps it into a signed .wgt in build/. Give a signing
# profile's name, or it uses the active one.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/../..
(cd "$root/players/web" && pnpm build)
rm -rf "$here/build"
mkdir -p "$here/build/app"
cp -R "$root/players/web/dist/." "$here/build/app/"
cp "$here/config.xml" "$here/build/app/"
cp "$root/docs/brand/icons/android-maskable-512.png" "$here/build/app/icon.png"
if [ -n "$1" ]; then
    tizen package -t wgt -s "$1" -o "$here/build" -- "$here/build/app"
else
    tizen package -t wgt -o "$here/build" -- "$here/build/app"
fi

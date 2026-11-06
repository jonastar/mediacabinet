#!/bin/sh
# Builds the TV app for Samsung TVs:
#
#   build/app/                       the app with its manifest, for the TV Simulator
#   build/MediaCabinet-unsigned.wgt  the same as a package; it needs signing to install
#   build/MediaCabinet.wgt           with a profile: ./package.sh <profile> [profiles.xml]
#
# Signing uses tz, Samsung's CLI, from the Tizen extension or TIZEN_CORE=/path/to/tz.
# Over SSH on Linux it couldn't read the profile's password back, probably for want
# of a keyring: see README.md for signing on your own computer.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/../..

(cd "$root/players/web" && pnpm build)
rm -rf "$here/build"
mkdir -p "$here/build/app"
cp -R "$root/players/web/dist/." "$here/build/app/"
cp "$here/config.xml" "$here/build/app/"
cp "$root/docs/brand/icons/android-maskable-512.png" "$here/build/app/icon.png"
(cd "$here/build/app" && zip -q -r ../MediaCabinet-unsigned.wgt .)
echo "Wrote $here/build/MediaCabinet-unsigned.wgt"

if [ -n "$1" ]; then
    tz=${TIZEN_CORE:-$(command -v tz || echo "$HOME/.tizen-extension-platform/server/sdktools/data/tools/tizen-core/tz")}
    "$tz" pack -t wgt -b "$here/build/MediaCabinet-unsigned.wgt" -s "$1" ${2:+-p "$2"} -o "$here/build/MediaCabinet.wgt"
    echo "Wrote $here/build/MediaCabinet.wgt"
fi

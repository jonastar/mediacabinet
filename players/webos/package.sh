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
cp "$here/images/icon-80.png" "$here/build/app/icon.png"
cp "$here/images/icon-130.png" "$here/build/app/largeIcon.png"
cp "$here/images/splash-1920x1080.png" "$here/build/app/splash.png"
# The build is minified already, and checked as ES5 in that form.
(cd "$here" && pnpm exec ares-package --no-minify --outdir build build/app)

#!/bin/sh
# Builds the TV app and wraps it into an .ipk in build/.
set -e
here=$(cd "$(dirname "$0")" && pwd)
root=$here/../..
version=$(cd "$root" && scripts/set-version.sh webos)
rm -rf "$here/build"
(cd "$root/players/web" && VITE_APP_PLATFORM=webos VITE_APP_VERSION=$version TV_OUT_DIR=$here/build/app pnpm build)
cp "$here/appinfo.json" "$here/build/app/"
cp "$here/images/icon-80.png" "$here/build/app/icon.png"
cp "$here/images/icon-130.png" "$here/build/app/largeIcon.png"
cp "$here/images/splash-1920x1080.png" "$here/build/app/splash.png"
# The build is minified already, and checked as ES5 in that form.
(cd "$here" && pnpm exec ares-package --no-minify --outdir build build/app)

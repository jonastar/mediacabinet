#!/usr/bin/env bash
# Puts the pinned jellyfin-ffmpeg build's ffmpeg and ffprobe for a platform in a
# folder, with its licence. The server finds them next to its own executable.
# Used by .github/workflows/release-server.yml for the archives and the image.
#
#   scripts/fetch-ffmpeg.sh <platform> <dir>
#
# <platform> is a release archive's: x86_64-linux, aarch64-linux, x86_64-macos,
# aarch64-macos or x86_64-windows.
#
# To update ffmpeg, set VERSION to a release of
# https://github.com/jellyfin/jellyfin-ffmpeg/releases and each platform's
# SHA-256 to its "portable" asset's (shown on the release page).
set -euo pipefail

VERSION=8.1.3-2

if [ $# -ne 2 ]; then
  echo "usage: $0 <platform> <dir>" >&2
  exit 2
fi
platform=$1
dir=$2

case "$platform" in
  x86_64-linux)
    asset=linux64-gpl.tar.xz
    sha256=1df84d04c4322902dba81c290508cdf3d89524d210b9fe03154bea6c4e3bfbb6
    ;;
  aarch64-linux)
    asset=linuxarm64-gpl.tar.xz
    sha256=6522585818059cc886363fb1dee86ac9b8579fec3b914337fb92974ef9eb9884
    ;;
  x86_64-macos)
    asset=mac64-gpl.tar.xz
    sha256=d394180f1155f84857544a48a99869f5741ccc50c5a1f73d2797429a2b326448
    ;;
  aarch64-macos)
    asset=macarm64-gpl.tar.xz
    sha256=d898cc819e7c4e9219d6410ddbdaada0838f14bec6ff89e283639915394cb43f
    ;;
  x86_64-windows)
    asset=win64-clang-gpl.zip
    sha256=b65dd5011a80b5abec08bd559489cf188217238c8e38b5bf553504c974f5694d
    ;;
  *)
    echo "unknown platform: $platform" >&2
    exit 2
    ;;
esac

repo=https://github.com/jellyfin/jellyfin-ffmpeg
file=jellyfin-ffmpeg_${VERSION}_portable_$asset
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

curl -fsSL --retry 3 -o "$work/$file" "$repo/releases/download/v$VERSION/$file"
if command -v sha256sum > /dev/null; then
  actual=$(sha256sum "$work/$file" | cut -d' ' -f1)
else
  actual=$(shasum -a 256 "$work/$file" | cut -d' ' -f1)
fi
if [ "$actual" != "$sha256" ]; then
  echo "$file: SHA-256 is $actual, expected $sha256" >&2
  exit 1
fi

mkdir -p "$work/out" "$dir"
case "$file" in
  *.zip)
    if command -v unzip > /dev/null; then
      unzip -q "$work/$file" -d "$work/out"
    else
      7z x -bso0 -o"$work/out" "$work/$file"
    fi
    ;;
  *)
    tar xJf "$work/$file" -C "$work/out"
    ;;
esac

exe=
if [ "$platform" = x86_64-windows ]; then
  exe=.exe
fi
for program in ffmpeg ffprobe; do
  cp "$work/out/$program$exe" "$dir/"
  chmod +x "$dir/$program$exe"
done

curl -fsSL --retry 3 -o "$dir/FFMPEG-COPYING.GPLv3.txt" \
  "https://raw.githubusercontent.com/jellyfin/jellyfin-ffmpeg/v$VERSION/COPYING.GPLv3"
cat > "$dir/FFMPEG.md" << EOF
# ffmpeg

\`ffmpeg\` and \`ffprobe\` here are jellyfin-ffmpeg $VERSION ($file),
unchanged, from $repo/releases/tag/v$VERSION.

They're licensed under the GNU General Public License version 3, in
FFMPEG-COPYING.GPLv3.txt. Their source is at $repo/tree/v$VERSION.
Media Cabinet runs them as separate programs; its own licence is in LICENSE.

To use another ffmpeg, set \`CABINET_FFMPEG\` and \`CABINET_FFPROBE\`, or delete
these two and put yours on the PATH.
EOF

#!/usr/bin/env bash
# Generates a small fake media library for manual and integration testing.
#
#   scripts/make-fixtures.sh [--force] [OUT_DIR]     (default: fixtures/media)
#
# Every clip is ~2 s and unique (its audio tone differs), so content hashes never
# collide. Titles are invented. Each root exercises a case from the spec:
#
#   tv/            default template {series}/Season {season:00}/{filename}
#     Night Harbor (2019)       clean SxxEyy names, a gap (S01E04 absent), a
#                               multi-episode file (S02E02E03), scene-style names,
#                               specials, sidecar subtitles, tvshow.nfo, a sample
#     Glass Orchard {tmdb-99001} explicit ID in the folder, 1x01 naming, mp4 with
#                               embedded mov_text subtitles
#     Unmatched Show            no year, no ID: needs a provider search
#   tv-grouped/    {year}/{genre}/{series}/Season {season:00}/{filename}
#   anime/         absolute numbering, release-group names, jpn + eng audio,
#                  ass subtitles, an episode number past a season's length
#   movies/        nfo with an ID, an edition (Director's Cut), 4K HDR10 and
#                  1080p versions, a two-part movie (cd1/cd2), a trailer, and a
#                  loose scene-named file
#   junk           .part download, zero-byte file, Thumbs.db, .DS_Store, .txt

set -euo pipefail

force=0
if [[ "${1:-}" == "--force" ]]; then
    force=1
    shift
fi
root=$(cd "$(dirname "$0")/.." && pwd)
out=${1:-$root/fixtures/media}

if [[ -d "$out" && $force -eq 0 ]]; then
    echo "$out exists; pass --force to regenerate" >&2
    exit 1
fi
if [[ -d "$out" ]]; then
    rm -r -- "$out"
fi
mkdir -p "$out"

tmp=$(mktemp -d)
trap 'rm -r -- "$tmp"' EXIT

seed=0
ff() {
    ffmpeg -nostdin -hide_banner -loglevel error -y "$@"
}

# Sets $hz to the next unique tone, so no two files have the same content.
# (Not a $(...) function: a subshell would never advance the counter.)
next_tone() {
    seed=$((seed + 1))
    hz=$((220 + seed * 11))
}

srt() {
    printf '1\n00:00:00,000 --> 00:00:01,500\n%s\n' "$1" > "$2"
}

# H.264 + AAC stereo (eng). Container from the extension.
basic() {
    local path=$out/$1
    mkdir -p "$(dirname "$path")"
    next_tone
    ff -f lavfi -i "testsrc2=size=320x180:rate=24:duration=2" \
        -f lavfi -i "sine=frequency=$hz:duration=2" \
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -ac 2 -b:a 64k \
        -metadata:s:a:0 language=eng "$path"
}

# H.264 + AC3 5.1 (eng) + AAC (jpn) + subtitles eng (default) and eng (forced).
# Subtitle codec: srt in mkv by default, ass when given.
multi() {
    local path=$out/$1 subcodec=${2:-srt}
    mkdir -p "$(dirname "$path")"
    srt "Dialogue" "$tmp/full.srt"
    srt "Sign" "$tmp/forced.srt"
    next_tone
    local eng=$hz
    next_tone
    ff -f lavfi -i "testsrc2=size=640x360:rate=24:duration=2" \
        -f lavfi -i "sine=frequency=$eng:duration=2" \
        -f lavfi -i "sine=frequency=$hz:duration=2" \
        -i "$tmp/full.srt" -i "$tmp/forced.srt" \
        -map 0:v -map 1:a -map 2:a -map 3:s -map 4:s \
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
        -c:a:0 ac3 -ac:a:0 6 -b:a:0 192k -c:a:1 aac -ac:a:1 2 -b:a:1 64k \
        -c:s "$subcodec" \
        -metadata:s:a:0 language=eng -metadata:s:a:1 language=jpn \
        -metadata:s:s:0 language=eng -metadata:s:s:1 language=eng \
        -disposition:a:0 default -disposition:a:1 0 \
        -disposition:s:0 default -disposition:s:1 forced \
        "$path"
}

# MP4: H.264 + AAC + embedded mov_text subtitles.
mp4() {
    local path=$out/$1
    mkdir -p "$(dirname "$path")"
    srt "Dialogue" "$tmp/mp4.srt"
    next_tone
    ff -f lavfi -i "testsrc2=size=320x180:rate=24:duration=2" \
        -f lavfi -i "sine=frequency=$hz:duration=2" -i "$tmp/mp4.srt" \
        -map 0:v -map 1:a -map 2:s \
        -c:v libx264 -preset ultrafast -pix_fmt yuv420p -c:a aac -ac 2 -b:a 64k -c:s mov_text \
        -metadata:s:a:0 language=eng -metadata:s:s:0 language=eng "$path"
}

# 2160p HEVC 10-bit with HDR10 metadata + E-AC3 5.1. One second to keep it fast.
hdr() {
    local path=$out/$1
    mkdir -p "$(dirname "$path")"
    next_tone
    ff -f lavfi -i "testsrc2=size=3840x2160:rate=24:duration=1" \
        -f lavfi -i "sine=frequency=$hz:duration=1" \
        -c:v libx265 -preset ultrafast -pix_fmt yuv420p10le \
        -color_primaries bt2020 -color_trc smpte2084 -colorspace bt2020nc \
        -x265-params "log-level=error:hdr10=1:colorprim=bt2020:transfer=smpte2084:colormatrix=bt2020nc:master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,1):max-cll=1000,400" \
        -c:a eac3 -ac 6 -b:a 192k -metadata:s:a:0 language=eng "$path"
}

text() {
    mkdir -p "$(dirname "$out/$1")"
    printf '%s' "$2" > "$out/$1"
}

# ── tv/ ───────────────────────────────────────────────────────────────────

nh="tv/Night Harbor (2019)"
text "$nh/tvshow.nfo" '<?xml version="1.0" encoding="UTF-8"?>
<tvshow>
  <title>Night Harbor</title>
  <year>2019</year>
  <uniqueid type="tmdb" default="true">99010</uniqueid>
  <uniqueid type="tvdb">399010</uniqueid>
</tvshow>
'
basic "$nh/Season 01/Night Harbor - S01E01 - Pilot.mkv"
srt "Pilot" "$out/$nh/Season 01/Night Harbor - S01E01 - Pilot.en.srt"
srt "Pilote" "$out/$nh/Season 01/Night Harbor - S01E01 - Pilot.fr.forced.srt"
basic "$nh/Season 01/Night Harbor - S01E02 - The Tide.mkv"
basic "$nh/Season 01/Night Harbor - S01E03 - Lighthouse.mkv"
# S01E04 deliberately absent: a gap.
basic "$nh/Season 01/Night Harbor - S01E05 - Undertow.mkv"
basic "$nh/Season 01/Night Harbor - S01E06 - Fog.mkv"
basic "$nh/Season 01/sample-night.harbor.s01e02.mkv"
basic "$nh/Season 02/Night.Harbor.S02E01.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv"
basic "$nh/Season 02/Night.Harbor.S02E02E03.1080p.WEB-DL.DDP5.1.H.264-GRP.mkv"
basic "$nh/Specials/Night Harbor - S00E01 - Behind the Scenes.mkv"

go="tv/Glass Orchard {tmdb-99001}"
mp4 "$go/Season 1/Glass Orchard 1x01.mp4"
mp4 "$go/Season 1/Glass Orchard 1x02.mp4"
mp4 "$go/Season 1/Glass Orchard 1x03 - Grafting.mp4"

basic "tv/Unmatched Show/Season 01/Unmatched Show S01E01.mkv"

# ── tv-grouped/ ───────────────────────────────────────────────────────────

basic "tv-grouped/2021/Drama/Lantern Street/Season 01/Lantern.Street.S01E01.720p.mkv"
basic "tv-grouped/2021/Drama/Lantern Street/Season 01/Lantern.Street.S01E02.720p.mkv"
basic "tv-grouped/2008/Comedy/Paper Moons (2008)/Season 03/Paper Moons - S03E10.mkv"
basic "tv-grouped/2008/Comedy/Paper Moons (2008)/Season 03/Paper Moons - S03E11.mkv"

# ── anime/ ────────────────────────────────────────────────────────────────

sb="anime/Starfall Brigade"
multi "$sb/[Kumo] Starfall Brigade - 01 [1080p][A1B2C3D4].mkv" ass
multi "$sb/[Kumo] Starfall Brigade - 02 [1080p][B2C3D4E5].mkv" ass
multi "$sb/[Kumo] Starfall Brigade - 03 [1080p][C3D4E5F6].mkv" ass
# Past a 12-episode first season: only makes sense in absolute order.
multi "$sb/[Kumo] Starfall Brigade - 14 [1080p][D4E5F6A7].mkv" ass
multi "$sb/[Other-Group] Starfall Brigade - 15v2 (BD 1080p HEVC).mkv"

# ── movies/ ───────────────────────────────────────────────────────────────

qm="movies/The Quiet Meridian (2014)"
text "$qm/The Quiet Meridian (2014).nfo" '<?xml version="1.0" encoding="UTF-8"?>
<movie>
  <title>The Quiet Meridian</title>
  <year>2014</year>
  <uniqueid type="tmdb" default="true">99020</uniqueid>
  <uniqueid type="imdb">tt9902000</uniqueid>
</movie>
'
multi "$qm/The Quiet Meridian (2014).mkv"
basic "$qm/The Quiet Meridian (2014) - Director's Cut.mkv"
basic "$qm/The Quiet Meridian (2014)-trailer.mp4"
srt "Meridian" "$out/$qm/The Quiet Meridian (2014).en.srt"

hc="movies/Halfway Coast (2001)"
hdr "$hc/Halfway Coast (2001) - 2160p HDR10.mkv"
basic "$hc/Halfway Coast (2001) - 1080p.mkv"

lw="movies/Long Winter Road (1987)"
basic "$lw/Long Winter Road (1987) - cd1.mkv"
basic "$lw/Long Winter Road (1987) - cd2.mkv"

basic "movies/some.random.movie.2010.720p.BluRay.x264-NOGRP.mkv"

# ── junk ──────────────────────────────────────────────────────────────────

# An interrupted download: a real mkv cut in half.
part="$nh/Season 02/Night.Harbor.S02E04.1080p.WEB-DL.mkv"
basic "$part"
head -c $(($(stat -c %s "$out/$part") / 2)) "$out/$part" > "$out/$part.part"
rm -- "$out/$part"
text "$nh/Season 02/empty.mkv" ''
text "$nh/Season 01/Thumbs.db" 'junk'
text "movies/.DS_Store" 'junk'
text "movies/README.txt" 'not media'

count=$(find "$out" -type f | wc -l)
size=$(du -sh "$out" | cut -f1)
echo "Wrote $count files ($size) to $out"

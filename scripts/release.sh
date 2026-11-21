#!/usr/bin/env bash
# Releases an app (see docs/releasing.md): a commit that drops -dev and dates the
# changelog, tagged <app>/vYYYY.N.P, then a commit starting the next -dev version.
# Nothing is pushed.
#
#   scripts/release.sh server               release what's being worked on
#   scripts/release.sh server --patch 2026.2
#                                           start release/server/2026.2 from
#                                           server/v2026.2.0, for patches
#
# On a release/<app>/YYYY.N branch it releases the patch being worked on;
# anywhere else, this year's next release: 2026.3.0 after 2026.2.0, or 2027.1.0
# as the first of a new year.
set -euo pipefail

usage="usage: scripts/release.sh server|android|apple|tizen|webos [--patch YYYY.N]"
app=${1:?$usage}
root=$(git rev-parse --show-toplevel)
cd "$root"

case $app in
server)
    changelog=CHANGELOG.md
    ;;
android | apple | tizen | webos)
    changelog=players/$app/CHANGELOG.md
    ;;
*)
    echo "$usage" >&2
    exit 1
    ;;
esac

if ! git diff --quiet HEAD; then
    echo "release: commit or stash your changes first" >&2
    exit 1
fi

# Adds an "(unreleased)" section for a version above the newest one.
start_section() {
    sed -i "0,/^## / s//## $1 (unreleased)\n\n## /" "$changelog"
}

if [[ ${2:-} == --patch ]]; then
    line=${3:?$usage}
    git switch -c "release/$app/$line" "$app/v$line.0"
    scripts/set-version.sh "$app" "$line.1-dev"
    start_section "$line.1"
    git commit -qam "Start $app $line.1-dev"
    echo "On release/$app/$line. Cherry-pick fixes from main with git cherry-pick -x."
    exit 0
fi

current=$(scripts/set-version.sh "$app")
branch=$(git branch --show-current)
if [[ $branch == release/$app/* ]]; then
    release=${current%-dev}
    IFS=. read -r year minor patch <<<"$release"
    next=$year.$minor.$((patch + 1))-dev
else
    year=$(date +%Y)
    last=$(git tag -l "$app/v$year.*" | sed "s|^$app/v$year\.\([0-9]*\)\..*|\1|" | sort -n | tail -1)
    minor=$((${last:-0} + 1))
    release=$year.$minor.0
    next=$year.$((minor + 1)).0-dev
fi

if git rev-parse -q --verify "refs/tags/$app/v$release" >/dev/null; then
    echo "release: $app/v$release is tagged already" >&2
    exit 1
fi
if ! grep -q '^## .* (unreleased)$' "$changelog"; then
    echo "release: $changelog has no \"(unreleased)\" section" >&2
    exit 1
fi

scripts/set-version.sh "$app" "$release"
# The heading said what main was working toward; the release may be a new year's.
sed -i "0,/^## .* (unreleased)$/ s//## $release ($(date +%F))/" "$changelog"
git commit -qam "Release $app $release"
git tag -a "$app/v$release" -m "Media Cabinet $app $release"

scripts/set-version.sh "$app" "$next"
start_section "${next%-dev}"
git commit -qam "Start $app $next"

echo "Tagged $app/v$release. Push with: git push origin $branch $app/v$release"

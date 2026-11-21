#!/usr/bin/env bash
# Shows or sets an app's version in every file that carries it (see docs/releasing.md).
#
#   scripts/set-version.sh server                 shows it
#   scripts/set-version.sh server 2026.2.0        a release
#   scripts/set-version.sh server 2026.3.0-dev    work toward the next one
#
# Apps: server (with the web UI), android, apple, tizen, webos.
set -euo pipefail

usage="usage: scripts/set-version.sh server|android|apple|tizen|webos [VERSION]"
app=${1:?$usage}
version=${2:-}
root=$(git rev-parse --show-toplevel)

cargo_toml=$root/Cargo.toml
web_package=$root/web/package.json
gradle_properties=$root/players/android/gradle.properties
xcconfig=$root/players/apple/Version.xcconfig
tizen_version=$root/players/tizen/VERSION
tizen_config=$root/players/tizen/config.xml
webos_package=$root/players/webos/package.json
webos_appinfo=$root/players/webos/appinfo.json

# Sets the first "version" in a JSON file.
set_json() {
    sed -i "0,/\"version\": \".*\"/ s//\"version\": \"$2\"/" "$1"
}

show() {
    case $app in
    server)
        sed -n '/^\[workspace.package\]/,/^\[/ s/^version = "\(.*\)"/\1/p' "$cargo_toml"
        ;;
    android)
        sed -n 's/^cabinetVersion=//p' "$gradle_properties"
        ;;
    apple)
        sed -n 's/^CABINET_VERSION = //p' "$xcconfig"
        ;;
    tizen)
        cat "$tizen_version"
        ;;
    webos)
        grep -m1 '"version"' "$webos_package" | sed 's/.*"version": "\([^"]*\)".*/\1/'
        ;;
    *)
        echo "$usage" >&2
        exit 1
        ;;
    esac
}

if [[ -z $version ]]; then
    show
    exit 0
fi
if ! [[ $version =~ ^[0-9]{4}\.[0-9]+\.[0-9]+(-dev)?$ ]]; then
    echo "set-version: expected YYYY.N.P or YYYY.N.P-dev, got '$version'" >&2
    exit 1
fi
# Stores and app manifests that take only numbers get the version without -dev.
plain=${version%-dev}
IFS=. read -r year minor patch <<<"$plain"

case $app in
server)
    sed -i "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$version\"/" "$cargo_toml"
    set_json "$web_package" "$version"
    cargo update --manifest-path "$cargo_toml" --workspace --quiet
    ;;
android)
    # Bounded by the versionCode formula in players/android/phone/build.gradle.kts.
    if ((minor > 99 || patch > 99)); then
        echo "set-version: Android's versionCode has room for 99 releases a year and 99 patches" >&2
        exit 1
    fi
    sed -i "s/^cabinetVersion=.*/cabinetVersion=$version/" "$gradle_properties"
    ;;
apple)
    sed -i -e "s/^CABINET_VERSION = .*/CABINET_VERSION = $version/" \
        -e "s/^MARKETING_VERSION = .*/MARKETING_VERSION = $plain/" "$xcconfig"
    ;;
tizen)
    # config.xml takes {0-255}.{0-255}.{0-65535}, so the year goes in as two digits.
    if ((minor > 255 || patch > 65535)); then
        echo "set-version: Tizen's config.xml has room for 255 releases a year" >&2
        exit 1
    fi
    echo "$version" >"$tizen_version"
    sed -i "s/\(<widget [^>]* version=\"\)[^\"]*\"/\1$((year % 100)).$minor.$patch\"/" "$tizen_config"
    ;;
webos)
    set_json "$webos_package" "$version"
    set_json "$webos_appinfo" "$plain"
    ;;
*)
    echo "$usage" >&2
    exit 1
    ;;
esac

if [[ $(show) != "$version" ]]; then
    echo "set-version: $app's version didn't change to $version" >&2
    exit 1
fi
echo "$app $version"

#!/usr/bin/env bash
# Sets the version of the server, core crate and web app (see docs/releasing.md).
#
#   scripts/set-version.sh 1.0.0        a release
#   scripts/set-version.sh 2.0.0-dev    work toward the next one
set -euo pipefail

version=${1:?usage: scripts/set-version.sh VERSION}
if ! [[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-dev)?$ ]]; then
    echo "set-version: expected X.Y.Z or X.Y.Z-dev, got '$version'" >&2
    exit 1
fi
root=$(git rev-parse --show-toplevel)

sed -i "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$version\"/" "$root/Cargo.toml"
sed -i "0,/\"version\": \".*\"/ s//\"version\": \"$version\"/" "$root/web/package.json"
cargo update --manifest-path "$root/Cargo.toml" --workspace --quiet

grep -q "^version = \"$version\"" "$root/Cargo.toml"
grep -q "\"version\": \"$version\"" "$root/web/package.json"
echo "version $version"

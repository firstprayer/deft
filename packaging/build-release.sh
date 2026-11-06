#!/bin/sh
# Build one release archive into dist/. Usage: packaging/build-release.sh [target]
# PROFILE=release gives a faster build than the default dist profile.
set -eu

cd "$(dirname "$0")/.."
target="${1:-$(rustc -vV | sed -n 's/^host: //p')}"
profile="${PROFILE:-dist}"

cargo build -p deftcli --locked --profile "$profile" --target "$target"

binary="target/$target/$profile/deft"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT
mkdir "$stage/deft-$target"
cp "$binary" LICENSE.md README.md THIRD-PARTY-NOTICES.md "$stage/deft-$target/"

mkdir -p dist
archive="deft-$target.tar.gz"
tar -czf "dist/$archive" -C "$stage" "deft-$target"
(cd dist && shasum -a 256 "$archive" > "$archive.sha256")
echo "ok dist/$archive"
cat "dist/$archive.sha256"

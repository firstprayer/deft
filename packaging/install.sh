#!/bin/sh
# Installs deft from a GitHub release.
#
#   DEFT_VERSION      a tag such as v0.1.0; default latest
#   DEFT_INSTALL_DIR  where the binary goes; default ~/.local/bin
#   DEFT_BASE_URL     where archives live; default the GitHub release
#   DEFT_NO_SKILL     set to skip installing the agent skill
set -eu

fail() { echo "error: $1" >&2; exit 1; }

case "$(uname -s)-$(uname -m)" in
    Darwin-arm64) target=aarch64-apple-darwin ;;
    Darwin-x86_64) target=x86_64-apple-darwin ;;
    Linux-x86_64) target=x86_64-unknown-linux-musl ;;
    Linux-aarch64) target=aarch64-unknown-linux-musl ;;
    *) fail "no build for $(uname -s) $(uname -m); build from source: https://github.com/firstprayer/deft/blob/main/docs/INSTALL.md" ;;
esac

version="${DEFT_VERSION:-latest}"
if [ "$version" = latest ]; then
    release=https://github.com/firstprayer/deft/releases/latest/download
else
    release="https://github.com/firstprayer/deft/releases/download/$version"
fi
base="${DEFT_BASE_URL:-$release}"
dir="${DEFT_INSTALL_DIR:-$HOME/.local/bin}"
archive="deft-$target.tar.gz"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

curl -fsSL "$base/$archive" -o "$work/$archive" || fail "could not download $base/$archive"
curl -fsSL "$base/$archive.sha256" -o "$work/$archive.sha256" || fail "could not download the checksum"

if command -v sha256sum >/dev/null 2>&1; then
    (cd "$work" && sha256sum -c "$archive.sha256" >/dev/null) || fail "checksum mismatch"
else
    (cd "$work" && shasum -a 256 -c "$archive.sha256" >/dev/null) || fail "checksum mismatch"
fi

tar -xzf "$work/$archive" -C "$work"
mkdir -p "$dir"
install -m 755 "$work/deft-$target/deft" "$dir/deft"
echo "ok installed $("$dir/deft" version) to $dir/deft"

case ":$PATH:" in
    *":$dir:"*) ;;
    *) echo "note: add $dir to your PATH" ;;
esac

# Teach the agents already on this machine. Never creates an agent's folder.
if [ -z "${DEFT_NO_SKILL:-}" ]; then
    found=
    for agent in claude codex; do
        if [ -d "$HOME/.$agent" ]; then
            found=1
            "$dir/deft" skill install --agent "$agent" || echo "note: could not install the $agent skill"
        fi
    done
    [ -n "$found" ] || echo "note: no agent found; run deft skill install --agent claude|codex later"
fi

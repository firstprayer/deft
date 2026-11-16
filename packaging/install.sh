#!/bin/sh
# Installs deft from a GitHub release.
#
#   DEFT_VERSION      a tag such as v0.1.0; default latest
#   DEFT_INSTALL_DIR  where the binary goes; default ~/.local/bin
#   DEFT_BASE_URL     where archives live; default the GitHub release
#   DEFT_NO_SKILL     set to skip installing the agent skill
#   DEFT_NO_MODIFY_PATH  set to leave shell startup files alone
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

# ~/.local/bin is not on the PATH of a new Mac, so a new shell would not find deft.
# This follows uv's installer, with the same env files, so the two share one line:
# $dir/env (and env.fish) put the folder on PATH once, and the startup files of every
# shell source it, whichever shell is in use today. Agents' shells read them too.
# Unlike uv it does this even when the folder is on PATH already: on Fedora that comes
# from .bashrc alone, and zsh would not find deft.
case "$dir" in
    "$HOME"/*) shown="\$HOME/${dir#"$HOME"/}" ;;
    *) shown="$dir" ;;
esac
# Writes an env file, or reuses one another installer wrote for this folder. Never overwrites.
write_env() {
    if [ -f "$1" ]; then grep -qF "\"$shown\"" "$1"; return; fi
    if [ "$2" = sh ]; then
        cat > "$1" <<EOF || return 1
#!/bin/sh
# add binaries to PATH if they aren't added yet
# affix colons on either side of \$PATH to simplify matching
case ":\${PATH}:" in
    *:"$shown":*)
        ;;
    *)
        # Prepending path in case a system-installed binary needs to be overridden
        export PATH="$shown:\$PATH"
        ;;
esac
EOF
    else
        cat > "$1" <<EOF || return 1
if not contains "$shown" \$PATH
    # Prepending path in case a system-installed binary needs to be overridden
    set -x PATH "$shown" \$PATH
end
EOF
    fi
}
# Appends a line to a startup file once, creating the file if needed.
add_line() {
    file="$1" line="$2" other="$3"
    if [ -f "$file" ] && { grep -qF "$line" "$file" || grep -qF "$other" "$file"; }; then return 0; fi
    mkdir -p "$(dirname "$file")" &&
        printf '\n# Added by the deft installer\n%s\n' "$line" >> "$file" &&
        added="$added $file" || echo "note: could not write $file"
}
if [ -n "${DEFT_NO_MODIFY_PATH:-}" ]; then
    case ":$PATH:" in
        *":$dir:"*) ;;
        *) echo "note: add $dir to your PATH" ;;
    esac
else
    added=
    if write_env "$dir/env" sh; then
        sh_line=". \"$shown/env\"" sh_other="source \"$shown/env\""
        now=". \"$dir/env\""
    else
        sh_line="export PATH=\"$shown:\$PATH\"" sh_other="$sh_line"
        now="export PATH=\"$dir:\$PATH\""
    fi
    if write_env "$dir/env.fish" fish; then
        fish_line="source \"$shown/env.fish\""
    else
        fish_line="fish_add_path \"$shown\""
    fi
    # sh and bash login shells: .profile always; bash reads .bash_profile or .bash_login
    # instead when one exists, and .bashrc in other interactive shells.
    add_line "$HOME/.profile" "$sh_line" "$sh_other"
    for rc in .bashrc .bash_profile .bash_login; do
        [ ! -f "$HOME/$rc" ] || add_line "$HOME/$rc" "$sh_line" "$sh_other"
    done
    # Interactive bash that is not a login shell reads only .bashrc.
    [ -f "$HOME/.bashrc" ] || ! command -v bash >/dev/null 2>&1 || add_line "$HOME/.bashrc" "$sh_line" "$sh_other"
    # zsh: .zshenv, which every zsh reads, interactive or not (uv writes .zshrc), and
    # .zprofile, as some systems' /etc/zprofile resets PATH after .zshenv (Alpine).
    add_line "${ZDOTDIR:-$HOME}/.zshenv" "$sh_line" "$sh_other"
    add_line "${ZDOTDIR:-$HOME}/.zprofile" "$sh_line" "$sh_other"
    add_line "$HOME/.config/fish/conf.d/deft.env.fish" "$fish_line" "$fish_line"
    # GitHub Actions: later steps of the job see the folder.
    [ -z "${GITHUB_PATH:-}" ] || echo "$dir" >> "$GITHUB_PATH"
    [ -z "$added" ] || echo "ok added $dir to your PATH in$added"
    case ":$PATH:" in
        *":$dir:"*) ;;
        *) echo "note: open a new terminal, or run: $now" ;;
    esac
fi

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

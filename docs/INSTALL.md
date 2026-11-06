# Installing deft

Nothing has been released. Today the only route is to build from source.

## What works today

You need [rustup](https://rustup.rs). `rust-toolchain.toml` pins the compiler, and rustup fetches it.

```bash
git clone https://github.com/firstprayer/deft
cd deft
cargo install --path crates/deft-cli --locked
deft version
deft doctor
```

The binary goes in `~/.cargo/bin`. It is about 50 MB. The build compiles Typst and an AVIF encoder, so it is slow and needs several GB of free disk.

macOS is the only platform regularly built. Linux and Windows are untested.

## Planned, not available

| Route | Status |
|---|---|
| Release binaries on GitHub | Planned |
| Install script | Planned; draft in `packaging/install.sh` |
| Homebrew tap | Planned; draft in `packaging/homebrew/deft.rb` |
| `cargo install deftcli` | Planned; needs the workspace crates published |
| npm, PyPI, winget, Scoop | Considered |

`homebrew/core` is not an option, because it accepts only open source licenses.

## Engines

Deft is one binary, but some groups call external programs. `deft doctor` shows what is available.

| Group | Needs | How to get it |
|---|---|---|
| `video`, `audio`, and commands that read or write them | ffmpeg | Your package manager, such as `brew install ffmpeg` |
| `youtube` | yt-dlp | `deft engine install yt-dlp`, which verifies a checksum |
| `pdf`, `image`, `sheet`, `md`, `archive` | Nothing | Built in |

`deft engine install` only supports yt-dlp. ffmpeg must be installed by hand.

## Agent setup

```bash
deft skill install --agent all
```

This writes a skill for Claude Code and Codex in your home folder. See [USAGE.md](USAGE.md#agent-setup).

The planned install script will do this for you, for each agent folder it finds. `DEFT_NO_SKILL=1` will skip it. Homebrew will not, so run the command above after `brew install`.

## Update and remove

To update, pull the repository and run the install command again, then run `deft skill install --agent all`.

```bash
cargo uninstall deftcli
rm -rf ~/.claude/skills/deft ~/.codex/skills/deft
```

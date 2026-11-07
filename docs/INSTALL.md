# Installing deft

## Install script

On macOS or Linux:

```bash
curl -fsSL https://deftcli.com/install.sh | sh
deft version
deft doctor
```

The script downloads the release for your system, checks its SHA-256 and puts `deft` in `~/.local/bin`. It then installs the skill for each agent folder it finds. The binary is about 45 MB.

| Variable | Effect |
|---|---|
| `DEFT_VERSION` | A tag such as `v0.1.0`. The default is the latest release |
| `DEFT_INSTALL_DIR` | Where the binary goes |
| `DEFT_NO_SKILL` | Set it to skip the skill |

## Homebrew

```bash
brew install firstprayer/tap/deft
deft skill install --agent all
```

`homebrew/core` is not an option, because it accepts only open source licenses.

## From source

You need [rustup](https://rustup.rs). `rust-toolchain.toml` pins the compiler, and rustup fetches it. This route also works on Windows, which has no installer yet.

```bash
git clone https://github.com/firstprayer/deft
cd deft
cargo install --path crates/deft-cli --locked
```

The build compiles Typst and an AVIF encoder, so it is slow and needs several GB of free disk.

## Engines

Deft is one binary, but some groups call external programs. `deft doctor` shows what is available.

| Group | Needs | How to get it |
|---|---|---|
| `video`, and commands that read or write it | ffmpeg | Your package manager, such as `brew install ffmpeg` |
| `pdf`, `image`, `sheet`, `md` | Nothing | Built in |

Deft does not install these for you; install ffmpeg by hand.

## Agent setup

```bash
deft skill install --agent all
```

This writes a skill for Claude Code and Codex in your home folder. See [USAGE.md](USAGE.md#agent-setup).

The install script does this for you, for each agent folder it finds. `DEFT_NO_SKILL=1` skips it. Homebrew does not, so run the command above after `brew install`.

## Update and remove

To update, run the install script or `brew upgrade deft` again.

```bash
rm ~/.local/bin/deft
rm -rf ~/.claude/skills/deft ~/.agents/skills/deft
```

Use `brew uninstall deft` if you installed with Homebrew, or `cargo uninstall deftcli` if you built from source.

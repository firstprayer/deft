# Contributing

Thank you for looking. Bug reports, failing files and new benchmark tasks help the most, and they need no agreement.

## Ways to help

| Help | How |
|---|---|
| Report a bug | Open a bug issue. Include the command, the full output and `deft version` |
| Report a job deft cannot do | Open a job request. It can become a benchmark case marked as a gap |
| Say a help text confused you or an agent | Open an issue with the text. Help is the product |
| Send code | Open an issue first, then see the contributor agreement below |

## Contributor agreement

Deft is source-available under PolyForm Shield, not open source, and the author keeps the rights needed to license it. Code from outside therefore needs a signed contributor license agreement before it can be merged.

The agreement is not published yet. Until it is, code contributions cannot be accepted, so please open an issue and describe the change.

## Development

```bash
cargo xtask setup          # once per clone: enables the git hooks
cargo xtask check          # the gate: fmt, clippy, unit, e2e, eval lint, generated files
```

`cargo xtask check` is the gate, and the pre-push hook runs it. It needs Ghostscript, ffmpeg, Pillow and openpyxl.

| Read | For |
|---|---|
| [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) | Layout, rules and checks |
| [docs/STYLE.md](docs/STYLE.md) | Naming, help text and code style |
| [tests/README.md](tests/README.md) | Unit, e2e and agent eval tests |
| [evals/README.md](evals/README.md) | Adding a benchmark case |

## Adding a command

A command is one file, `crates/deft-ops/src/ops/<group>/<command>.rs`, exporting `pub static OP: Op`. `build.rs` finds it. Every option and choice value needs an e2e case in `tests/e2e/cases/`, or the build fails. Then run `cargo xtask site` to refresh the command reference and the skill.

## Rules that surprise people

- All project files are in English, unless they are deliberate localisation.
- Pin new dependencies to releases at least two weeks old.
- The same option name must mean the same thing in every command.
- Do not edit `docs/COMMANDS.md` or `skills/deft/SKILL.md`. They are generated.

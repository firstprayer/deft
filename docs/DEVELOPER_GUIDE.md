# Developer guide

Deft is a single-binary CLI that replaces the scripts an agent would otherwise write for file jobs. This guide covers how the code is laid out, the rules it follows and how to check a change.

## Layout

- `crates/deft-core`: formats, option value syntax, error codes, `Op`/`Group` specs, `Job`/`OpResult`.
- `crates/deft-tools-*` (data, doc, file, image, pdf, sheet, text): capability libraries. Pure functions, no CLI knowledge, no printing.
- `crates/deft-engines`: locating and running ffmpeg and other programs. Never through a shell.
- `crates/deft-ops`: `src/ops/<group>/<command>.rs`, one operation per file, auto-registered by `build.rs`. `runtime.rs` runs them; `registry.rs` resolves paths, aliases, search and conversion chains.
- `crates/deft-cli`: `deft` binary: argument parsing, three-layer help, rendering.
- `crates/deft-testkit`: fixtures, visual cases. `xtask`: developer tools.

## Where things are registered

There is no central list. Each operation is one file, `crates/deft-ops/src/ops/<group>/<command>.rs`, exporting `pub static OP: Op`. That file holds the command name (`id`, `mounts`), its options (`params`) and the function it runs (`run`). `crates/deft-ops/build.rs` scans the folders and generates the registry. `cargo xtask commands` prints the full map.

## Rules

- A group is only a logical index. Put a command where an agent would look first.
- Adding a command or group means adding files only. See [CONTRIBUTING.md](../CONTRIBUTING.md).
- An operation has one implementation. To reach it from another group, add a mount; do not copy it.
- The same option name must mean the same thing everywhere (`specs_are_well_formed` enforces this).
- Help text is the product. Keep summaries short, stay inside the token budgets, review snapshot diffs with `cargo insta review`.
- Never overwrite inputs. Deft never touches the network.
- Pin new dependencies to releases at least two weeks old.

## Checks

`cargo xtask check` is the gate, and the pre-push hook runs it. `.github/workflows/build.yml` builds and runs unit tests on macOS, Linux and Windows. Use standard GitHub runners only; larger runners are billed.

```bash
cargo xtask check               # fmt, clippy, unit, e2e, image comparison, generated files, eval lint
cargo test-unit                 # 1. unit tests
cargo test-e2e                  # 2. e2e tests: fixed commands against ground truth (see tests/README.md)
cargo xtask fmt                 # format everything; plain cargo fmt skips operation files
cargo clippy --workspace --all-targets
cargo xtask eval lint           # 3. agent eval files are consistent (see evals/README.md)
cargo xtask eval run --arm reference   # eval tasks are solvable; this is not a correctness test
cargo xtask review              # builds review/index.html for human visual review
cargo xtask bench               # performance budgets on the release binary
cargo xtask site                # regenerate docs/COMMANDS.md and skills/deft/SKILL.md
cargo xtask notices             # regenerate THIRD-PARTY-NOTICES.md after changing dependencies
cargo xtask catalog --open      # searchable page of every group, command and option
```

## Developer tools

Tools for people working on deft live in `xtask/`, not in the `deft` binary. Add one as a module there and a line in `xtask/src/main.rs`.

`catalog/index.html` is built from the compiled registry and `tests/e2e/cases/`. It is never committed and never edited: change the source and it follows. The git hooks rebuild it after each commit, pull and checkout. To show a new kind of information, change `xtask/src/catalog.rs` and `xtask/src/catalog.html`.

```bash
cargo xtask setup               # once per clone: enables the git hooks
cargo xtask eval fetch          # once per clone: downloads the real test files, about 24 MB
```

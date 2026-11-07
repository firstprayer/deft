# Style guide

Formatting and lint rules are enforced by tools, so there is nothing to memorise. The rest of this guide covers what tools cannot check.

## Tools

| Tool | Command | Enforced by |
|---|---|---|
| rustfmt, default settings | `cargo xtask fmt` (`cargo fmt` cannot reach operation files) | pre-commit and pre-push hooks |
| clippy, no warnings allowed | `cargo clippy --workspace --all-targets -- -D warnings` | pre-push hook |
| EditorConfig, for every other file type | your editor | `.editorconfig` |

`rust-toolchain.toml` pins the compiler, so a formatter or lint result is the same on every machine. Run `cargo xtask setup` once per clone to enable the hook.

## Language

All project files are in English: code, comments, docs, help text, commit messages. Other languages appear only in deliberate internationalisation content, such as CJK test data.

## Naming

| Thing | Rule | Example |
|---|---|---|
| Tool crates | `deft-tools-<area>` | `deft-tools-pdf` |
| Operation file | `crates/deft-ops/src/ops/<group>/<command>.rs` | `pdf/to_images.rs` |
| Operation id | `<group>.<command>`, matching the path | `pdf.to-images` |
| Commands | Reuse a verb from the design doc; `to-<format>` and `from-<format>` for conversions | `compress`, `to-pdf` |
| Options | Long, kebab-case; one meaning everywhere | `--target-size` |
| Error codes | `SCREAMING_SNAKE_CASE`; add, never rename | `INPUT_MISMATCH` |

## Code

- Tool crates are pure: they do not print, read the environment, or know about the CLI.
- Operation files stay thin: declare the spec, read options, call a tool crate.
- External programs go through `deft-engines`, never through a shell.
- Comment why, not what. Skip comments that repeat the code.
- Return a `DeftError` with a stable code. Add a `hint` when there is a concrete fix.

## Help text

- Summaries: at most 64 characters, start with a capital, no trailing period.
- Every option has help text.
- Every command has at least one example that really runs.
- Write for an agent that has never seen Deft: say what goes in and what comes out.

## Tests

See `tests/README.md`.

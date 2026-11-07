# Tests

Deft has three separate test sets. The first two prove the program is correct.
The third measures how much it helps an agent; it is not a correctness test.

| Set | What it proves | Where | Run |
|---|---|---|---|
| 1. Unit | A function or a spec is right, in-process | `crates/*/src/*_tests.rs`, `tests/unit/` | `cargo test-unit` |
| 2. E2E | A fixed `deft` command writes the right output | `tests/e2e/` | `cargo test-e2e` |
| 3. Agent eval | Agents use fewer tokens and less time with deft | `evals/` | `cargo xtask eval run` |

Both correctness sets must pass before every commit. Neither needs an agent or
the network.

## 1. Unit tests

Each library has a `<module>_tests.rs` file next to the module it tests.
`tests/unit/` holds checks that span the registry:

- `specs.rs`: every operation file is well formed, its examples parse, help
  stays within its token budget, schemas are valid.
- `help.rs`: snapshots of the top-level and group help.
- `properties.rs`: property tests of the option syntax.

## 2. E2E tests

`tests/e2e/cases/<group>/<command>.toml` lists fixed commands and the ground
truth for each. One case is one test:

```toml
[[case]]
name = "fill stretches to the exact size"
run = "deft image resize photo-1.jpg --size 300x100 --fit fill -o out.png"
expect = [
  { type = "image", file = "out.png", width = 300, height = 100, format = "PNG" },
  { type = "image_expected", file = "out.png", from = "photo-1.jpg", op = "resize:300,100" },
]
```

| Key | Meaning |
|---|---|
| `run` | One command, or a list run in order |
| `expect` | Checks on the outputs; `crates/deft-testkit/src/judge.rs` lists them all |
| `error` | The last command must fail with this code |
| `prints` | The last command must print this text |
| `data` | Real files from `evals/data`, by id. Shown as ignored until `cargo xtask eval fetch` has run |

Ground truth comes from tools other than deft: Pillow computes the expected
image, Ghostscript renders and reads PDFs, ffprobe and ffmpeg inspect media,
openpyxl reads workbooks. Text formats compare byte for byte.

Run one command's cases with `cargo test -p deftcli --test e2e_cases image/resize`.

Set `DEFT_E2E_KEEP=<folder>` to keep each case's files and what it printed.

Every sandbox starts with the fixtures from `deft_testkit::write_fixtures`.

`tests/e2e/coverage.rs` fails the build when a command, an option or a choice
value has no case. Adding an option therefore means adding its case.

Other e2e files:

- `behavior.rs`: rules shared by all commands, such as `-o`, batches and exit codes.
- `examples.rs`: every example in every command's help runs as written.
- `visual.rs`: rendered pages compared with `tests/e2e/golden/`; review changes
  with `cargo xtask review`.

Needs Ghostscript, ffmpeg, Pillow, openpyxl, PyYAML, toml, lxml and
BeautifulSoup. Commands that call tesseract need it
too. Cases that need ffmpeg are skipped when it is missing.

## 3. Agent eval

See `evals/README.md`. It benchmarks tokens and time. A failing eval case means
the agent did not finish a task, which may be a help or discoverability problem.
A bug it finds gets a case in set 1 or 2 before it is fixed.

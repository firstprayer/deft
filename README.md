# deft

**One command instead of a throwaway script.**

[deftcli.com](https://deftcli.com)

When an AI agent has a file job, it writes a script, debugs it, checks the output and throws the script away. Deft is one binary that turns those jobs into single commands: PDF, images, Word and PowerPoint files, spreadsheets, Markdown, HTML, e-books, JSON, YAML and more. The agent spends fewer tokens and less time, and fails less often.

```console
$ deft pdf compress photos.pdf
ok pdf.compress photos.pdf -> photos.compressed.pdf  704KB -> 272KB (-61%)  0.10s
```

## Measured

The same agent, with and without deft:

| 147 tasks | With deft | Without |
|---|---:|---:|
| Tasks solved | **147** | 140 |
| Failed commands | **18** | 212 |
| Tokens | **1,940,543** (24% fewer) | 2,555,050 |
| Time | **3,032 s** (56% less) | 6,929 s |

**Method.** 147 everyday file jobs written the way a person asks, 59 of them multi-step. Claude Code with Opus 5.5 at medium effort, in its sandbox, one attempt per task. The "with" arm has deft installed with its skill; the "without" arm uses what the machine has, which included ffmpeg, Ghostscript, Pillow and pandas. Tokens are uncached input plus output. Results are judged with tools other than deft. Every task and run, with its trace, is at [deftcli.com/evals](https://deftcli.com/evals/).

## Quick start

On macOS or Linux:

```bash
curl -fsSL https://deftcli.com/install.sh | sh
deft doctor                         # shows what is available
```

The script checks the download, then sets up the skill for Claude Code and Codex. Reading HEIC and AVIF needs ffmpeg, and OCR needs tesseract. See [docs/INSTALL.md](docs/INSTALL.md) for Homebrew and other routes.

## Examples

```bash
deft pdf compress report.pdf --target-size 2MB
deft pdf watermark contract.pdf --text CONFIDENTIAL
deft image resize photos/*.jpg --size 1200x -o web/
deft md to-pdf notes.md
deft convert notes.md -o notes.pdf
deft sheet pivot sales.xlsx --rows city --columns year --values amount -o totals.csv
deft yaml convert config.yaml --to json -o config.json
deft pdf fill form.pdf --data values.json --flatten -o filled.pdf
```

## How an agent uses it

- **Find a command.** `deft` lists groups, `deft pdf` lists commands, `deft pdf compress --help` shows options, and `deft find "shrink pdf"` searches. Each layer has a token budget that tests enforce.
- **Read one line.** A result starts with `ok`, `partial` or `error`. Add `--json` for structure.
- **Fix it in one turn.** Errors carry a stable code and a `hint:` with the fix.
- **Stay safe.** Inputs are never overwritten and writes are atomic. Deft reads and writes local files and collects no usage data; it goes online only to download pictures a document links to, when it converts that document to PDF, Word or EPUB.

## Engines

Reading HEIC and AVIF uses ffmpeg. OCR uses tesseract. `deft doctor` shows what is available.

## Documentation

| Document | Contents |
|---|---|
| [docs/INSTALL.md](docs/INSTALL.md) | What works today, what is planned, engines |
| [docs/USAGE.md](docs/USAGE.md) | Syntax, inputs and outputs, results, errors, agent setup |
| [docs/COMMANDS.md](docs/COMMANDS.md) | Every command, generated from the source |
| [deftcli.com/evals](https://deftcli.com/evals/) | The agent benchmark |
| [docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) | Layout, rules and checks for working on deft |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to help, and the contributor agreement |
| [CHANGELOG.md](CHANGELOG.md) | Release notes |
| [SECURITY.md](SECURITY.md) | Reporting a vulnerability |

## License

Deft is source-available under the [PolyForm Shield License 1.0.0](LICENSE.md). You may use, change and share it for almost any purpose, including at work, but you may not use it to provide a product that competes with Deft. For other terms, contact the author.

This is not an OSI open source license. Third-party components keep their own licenses; see [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

Contributions need a contributor agreement first, so please open an issue before sending code.

# deft

**One command instead of a throwaway script.**

[deftcli.com](https://deftcli.com)

When an AI agent has a file job, it writes a script, debugs it, checks the output and throws the script away. Deft is one binary that turns those jobs into single commands: PDF, images, video, audio, spreadsheets, Word files, Markdown and more. The agent spends fewer tokens and less time, and fails less often.

```console
$ deft pdf compress photos.pdf
ok pdf.compress photos.pdf -> photos.compressed.pdf  704KB -> 272KB (-61%)  0.10s
```

## Measured

The same agent, with and without deft:

| 85 supported tasks | With deft | Without |
|---|---:|---:|
| Tasks solved | **85** | 78 |
| Failed commands | **3** | 70 |
| Tokens | **745,522** (27% fewer) | 1,024,324 |
| Time | **1,096 s** (57% less) | 2,545 s |

**Method.** 100 tasks written the way a person asks, of which 85 are ones deft supports; 9 are known gaps and 6 need the network and were not run. Codex with gpt-6-luna at medium effort, in its sandbox with no network. The latest attempt counts: 22 of 188 task and arm pairs were rerun, and 8 of those went from fail to pass, 4 in each arm. The "with" arm adds deft and its skill text. The "without" arm uses what the machine has, which included ffmpeg, Ghostscript, Pillow and pandas. Tokens are uncached input plus output. Results are judged with tools other than deft.

**Limits.** One agent; Claude Code has not been measured. Deft was improved against these same tasks, so this is a development benchmark, not a held-out test. On 31 of the 85 tasks deft used more tokens, because the agent reads help first: a plain `tar` still wins on a trivial job. The savings come from jobs where an agent would otherwise write code, such as PDF, images, Markdown and sheets. Cases, results and the harness are in [evals/](evals/README.md).

## Quick start

On macOS or Linux:

```bash
curl -fsSL https://deftcli.com/install.sh | sh
deft doctor                         # shows what is available
```

The script checks the download, then sets up the skill for Claude Code and Codex. Video and audio commands need ffmpeg. See [docs/INSTALL.md](docs/INSTALL.md) for Homebrew and other routes.

## Examples

```bash
deft pdf compress report.pdf --target-size 2MB
deft pdf watermark contract.pdf --text CONFIDENTIAL
deft image resize photos/*.jpg --size 1200x -o web/
deft video to-gif clip.mp4 --from 0:10 --to-time 0:14 -o clip.gif
deft video add-subtitles talk.mp4 --subtitles talk.srt -o subtitled.mp4
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
- **Stay safe.** Inputs are never overwritten and writes are atomic. Everything runs locally: deft never touches the network and collects no usage data.

## Engines

`video` and `audio` use ffmpeg, and so does reading HEIC and AVIF. OCR uses tesseract. `deft doctor` shows what is available.

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

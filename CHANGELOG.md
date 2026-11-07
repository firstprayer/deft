# Changelog

Deft follows semantic versioning. Before 1.0, a minor version may change commands or options.

## Unreleased

### Added

- `contact` group: VCF address books to and from CSV.
- `calendar` group: ICS calendars to and from CSV.
- `gps` group: GPX and KML conversion, distance, time and climb.
- `pdf extract-sheets`: each table in a PDF as its own CSV, or as a sheet in one XLSX.
- `video frames --interval` and `--from`.
- `deft find` says when no command does a job.

### Changed

- `video stabilize` uses two passes where ffmpeg has vidstab, which removes far more shake.
- `video compress --target-size` uses two passes and the whole size budget.
- YAML output quotes strings such as `yes`, `on` and dates, which older parsers misread.

### Removed

- The `archive` and `subtitle` groups, and `audio waveform`.
- The `youtube` group, `deft engine install` and `update`, `--offline` and `--install-engines`. Deft no longer touches the network.
- Commands an agent does in one ordinary line: `file hash`, `base64`, `split`, `join`, `rename` and `organize`; `video crop`, `resize`, `rotate`, `mute`, `fade`, `repeat`, `reverse`, `stack`, `thumbnail`, `extract-audio` and `extract-subtitles`; `audio chunk`, `compress`, `fade`, `merge`, `reverse`, `speed`, `strip` and `volume`; `image diff`, `invert` and `sharpen`; `pdf grayscale` and `repair`; `data query`; `sheet transpose`; `email info`, `contact info` and `calendar info`.
- `sheet filter`, `sort`, `select`, `group`, `dedupe` and `join`: `sheet query` does all of them in SQL.
- Commands whose output was not good enough: `pdf to-docx`, `to-md`, `to-sheet` and `to-pdfa`; `slides to-pdf`; `md to-pptx`; `audio mix`.

### Fixed

- `sheet query` runs on SQLite compiled into deft, so it no longer needs the `sqlite3` program. Numbers keep every digit, identifiers such as 007 stay text, empty cells are NULL, every sheet of a workbook is a table, repeated headers work and `--sheet` is new. A SQLite file is opened read-only.
- Report commands write their result as JSON with `-o name.json` instead of ignoring `-o`.
- Text options keep their spaces, so `doc replace --find ' '` works.
- `sheet merge` lines columns up by header. `sheet extract --range` counts from A1 when a sheet starts later. Comma decimals in semicolon files are numbers, and ids such as 12E3 stay text.
- UTF-16 text is no longer taken for MP3. `deft info` identifies locked PDFs and directories.
- `deft convert` writes every page into a directory. `data format` keeps YAML documents apart and warns about comments.
- `video from-images` keeps every picture. Pictures of different sizes in one format could be dropped, so a slideshow came out too short.
- `video chunk` cuts equal parts. A part could run on to the next cut, giving 6, 3 and 1 seconds instead of 3, 3, 3 and 1.

## 0.1.0

First version. See [docs/COMMANDS.md](docs/COMMANDS.md) for the full command list.

### Added

- Command groups: `pdf`, `image`, `video`, `audio`, `doc`, `slides`, `data`, `sheet`, `md`, `html`, `epub`, `email`, `youtube`, `file`, `qr`.
- Top-level commands: `convert`, `info`, `find`, `schema`, `engine`, `skill`, `doctor`.
- One-line results starting with `ok`, `partial` or `error`, and `--json` for structure.
- Stable error codes, with a `hint:` when there is a concrete fix.
- Three layers of help with token budgets enforced by tests, and `deft find` to search by intent.
- A generated skill for Claude Code and Codex: `deft skill install --agent all`.
- External plugins: a `deft-<group>` program on the PATH acts as a group.
- Watermarks, page numbers and subtitles in any script, including Chinese.

### Known limits

- `video` and `audio` need ffmpeg, which must be installed by hand. `youtube` needs yt-dlp.
- WebP is written losslessly only.
- PDF rendering uses pure-Rust hayro; quality on complex PDFs still needs checking.
- Releases cover macOS and Linux. Windows builds and passes unit tests, but has no installer yet.
- Full end-to-end tests run on macOS only.

### Measured

On the 85 supported tasks of a 100-task benchmark, with Codex (gpt-6-luna, medium effort), counting the latest attempt: 85 solved with deft and 78 without, 27% fewer tokens, 57% less time, 3 failed commands against 70. Method and limits are in [the README](README.md#measured).

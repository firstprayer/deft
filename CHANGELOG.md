# Changelog

Deft follows semantic versioning. Before 1.0, a minor version may change commands or options.

## Unreleased

### Added

- `gps` group: GPX and KML conversion, distance, time and climb.
- `pdf extract-sheets`: each table in a PDF as its own CSV, or as a sheet in one XLSX.
- `video frames --interval` and `--from`.
- `deft find` says when no command does a job.
- `html to-docx --main` and `html to-pdf --main`.
- `calendar to-csv` adds `status` and `except` columns, and `calendar from-csv` reads them.

### Changed

- `video stabilize` uses two passes where ffmpeg has vidstab, which removes far more shake.
- `video compress --target-size` uses two passes and the whole size budget.
- YAML output quotes strings such as `yes`, `on` and dates, which older parsers misread.
- Word readers keep footnotes, endnotes, text boxes, nested tables, content controls and list levels, and `doc replace` reaches notes.
- `doc to-html` embeds pictures. Pictures in PDFs keep their natural size.
- `calendar to-csv` shows the last day of an all-day event, not the day after it.
- `gps info` and `gps convert` treat track segments separately, and `climb_m` ignores rises under 2 m.

### Removed

- The `archive` and `subtitle` groups, and `audio waveform`.
- `html screenshot` and `html to-pdf --browser`. Deft no longer uses Chrome.
- The `youtube` group, `deft engine install` and `update`, `--offline` and `--install-engines`. Deft no longer touches the network.
- Commands an agent does in one ordinary line: `file hash`, `base64`, `split`, `join`, `rename` and `organize`; `video crop`, `resize`, `rotate`, `mute`, `fade`, `repeat`, `reverse`, `stack`, `thumbnail`, `extract-audio` and `extract-subtitles`; `audio chunk`, `compress`, `fade`, `merge`, `reverse`, `speed`, `strip` and `volume`; `image diff`, `invert` and `sharpen`; `pdf grayscale` and `repair`; `video split` (`ffmpeg -ss … -to …`); `data query`; `sheet transpose`; `audio tag`; `email info`.
- `sheet query`, `filter`, `sort`, `select`, `group`, `dedupe` and `join`: an agent does these in one line with sqlite3, DuckDB or pandas. Deft no longer includes SQLite.
- The `email` and `file` groups.
- Commands whose output was not good enough: `pdf to-docx`, `to-md`, `to-sheet` and `to-pdfa`; `slides to-pdf`; `md to-pptx`; `audio mix`.

### Fixed

- `pdf redact` removes the text and form values from the file, not just from the page, and `pdf strip` removes the XMP packet.
- `pdf encrypt` output opens in Apple's PDF readers. `pdf to-text` and `search` read fonts that came out as garbage before, and ligatures.
- `pdf merge` keeps every form and bookmark. `extract-sheets` uses real glyph widths.
- HEIC photos from phones come out whole and upright. `image strip` removes everything after a JPEG's end. `image info` finds GPS in every format. Display P3 colour survives.
- `image compress` never grows a file and fails when it misses a target. `image trim --speck` ignores dust.
- `video from-images`, `merge`, `watermark`, `add-subtitles` and `info` handle mixed sizes, silent clips and phone rotation. `audio convert`, `split` and `normalize` work with cover art.
- Word readers keep footnotes, text boxes and nested tables. EPUB keeps every chapter and refuses DRM. HTML keeps the headline and declared charset.
- Text options keep their spaces. `-o` works on report commands.
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

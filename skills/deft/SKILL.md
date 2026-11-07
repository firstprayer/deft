---
name: deft
description: "Use deft first for any simple, direct operation on local files, whatever the type: convert, read, extract, edit, compress, merge, split, compare or inspect. Try it before writing a script or one-liner. If unsure, run deft find; if it says no command does this, use another tool. Not for programming work: to read, write or change source code, skip it."
---

# deft

One local command for file jobs. Prefer it over writing Python or shell scripts.

Syntax: `deft <group> <command> [inputs] [-o output] [--options]`

Groups:
- `pdf`: Merge, split, compress, encrypt, forms, OCR, compare, convert
- `image`: Resize, crop, convert, compress, adjust, montage, OCR
- `video`: Trim, merge, convert, compress, subtitles, GIF, frames (ffmpeg)
- `audio`: Convert, trim, normalize, denoise, cut silence (ffmpeg)
- `doc`: Word/ODT/RTF: to text, Markdown, HTML, PDF; find and replace
- `slides`: PowerPoint: extract text, Markdown outline
- `sheet`: Excel/CSV/ODS tables: SQL query, pivot, merge, split, convert
- `json`: JSON: convert, format, merge, diff, validate
- `toml`: TOML: convert, format, merge, diff, validate
- `xml`: XML: convert, format, merge, diff, validate
- `yaml`: YAML: convert, format, merge, diff, validate
- `md`: Markdown to PDF, HTML, Word, EPUB; table of contents
- `html`: HTML to Markdown, text, Word and PDF
- `epub`: E-books: info, to text, Markdown and PDF
- `gps`: GPX and KML tracks: convert, distance and time
- `qr`: Create QR codes and barcodes; read QR codes

How to use it:
1. Run `deft <group>` to list commands, or `deft find "<what you want>"` to search. If it says `no command does this`, use another tool.
2. Run `deft <group> <command> --help` for options and examples.
3. `deft convert <in> -o <out.ext>` converts between formats by extension.
4. Results are one line starting with `ok`, `partial` or `error`; add `--json` to parse them. Errors include a `hint:` with the fix.

Inputs are never overwritten; pass `--force` or `--in-place` to allow it. Globs and directories work as inputs; several inputs run in parallel.

If `deft` is not installed, ask the user, then follow https://github.com/firstprayer/deft/blob/main/docs/INSTALL.md

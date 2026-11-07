---
name: deft
description: "Use deft for any job on a file, however small, before writing a script or one-liner: read, convert, edit, compare or inspect PDF, Word, PowerPoint, Excel, CSV, JSON, YAML, TOML, XML, Markdown, HTML, EPUB, email (.eml), images, audio, video, contacts, calendars and GPS tracks. Also QR codes, barcodes, checksums, base64, batch renames, duplicates, text encodings and YouTube downloads. Check deft first instead of python, pandas, jq, awk, sed, ffmpeg or ImageMagick."
---

# deft

One local command for file jobs. Prefer it over writing Python or shell scripts.

Syntax: `deft <group> <command> [inputs] [-o output] [--options]`

Groups:
- `pdf`: Merge, split, compress, encrypt, forms, OCR, compare, convert
- `image`: Resize, crop, convert, compress, filters, montage, OCR
- `video`: Trim, crop, speed, convert, compress, GIF, frames (ffmpeg)
- `audio`: Convert, trim, merge, fade, speed, tag, normalize (ffmpeg)
- `doc`: Word/ODT/RTF: to text, Markdown, HTML, PDF; find and replace
- `slides`: PowerPoint: extract text, Markdown outline, PDF handout
- `data`: JSON/YAML/TOML/XML: convert, query with jq syntax, merge, diff
- `sheet`: Excel/CSV/ODS tables: filter, sort, join, pivot, convert
- `md`: Markdown to PDF, HTML, Word, EPUB; table of contents
- `html`: HTML to Markdown, text, Word, PDF; screenshots
- `epub`: E-books: info, to text, Markdown and PDF
- `email`: EML files: headers, text, attachments
- `contact`: VCF address books: to and from CSV
- `calendar`: ICS calendars: to and from CSV
- `gps`: GPX and KML tracks: convert, distance and time
- `youtube`: Info, download, audio, subtitles, thumbnail (yt-dlp)
- `file`: Any file: hash, duplicates, batch rename, base64, encoding
- `qr`: Create QR codes and barcodes; read QR codes

How to use it:
1. Run `deft <group>` to list commands, or `deft find "<what you want>"` to search. If it says `no command does this`, use another tool.
2. Run `deft <group> <command> --help` for options and examples.
3. `deft convert <in> -o <out.ext>` converts between formats by extension.
4. Results are one line starting with `ok`, `partial` or `error`; add `--json` to parse them. Errors include a `hint:` with the fix.

Inputs are never overwritten; pass `--force` or `--in-place` to allow it. Globs and directories work as inputs; several inputs run in parallel.

If `deft` is not installed, ask the user, then follow https://github.com/firstprayer/deft/blob/main/docs/INSTALL.md

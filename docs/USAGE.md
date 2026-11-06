# Using deft

For the full command list, see [COMMANDS.md](COMMANDS.md). To install, see [INSTALL.md](INSTALL.md).

## Syntax

```
deft <group> <command> [inputs...] [-o output] [--option value ...]
```

A group is an index: by file type (`pdf`), by source (`youtube`) or by task (`archive`). `deft convert`, `deft info` and `deft find` work on any file.

## Finding a command

Help comes in layers, so an agent reads only what it needs.

```bash
deft                        # groups and top-level commands
deft pdf                    # commands in a group
deft pdf compress --help    # options and examples for one command
deft find "shrink pdf"      # search by what you want to do
deft help common            # options every command takes
deft help values            # how to write pages, times, sizes and colours
deft schema pdf compress    # JSON Schema of a command's options
```

Common wrong guesses work. `combine` resolves to `merge`, `shrink` to `compress`, and `img` to `image`.

## Inputs

| Form | Meaning |
|---|---|
| `a.pdf b.pdf` | Several files |
| `photos/*.jpg` | A wildcard, which deft expands itself |
| `photos/` | Matching files in a folder; add `-r` to go into subfolders |
| `-` | Standard input; if the format cannot be detected, add `--stdin-format <fmt>` |
| `https://...` | Only for commands that take a web address |

Deft reads the file's header to learn its type. The extension is only a hint.

## Outputs

| Case | What happens |
|---|---|
| No `-o` | Written next to the input, such as `report.compressed.pdf` |
| `-o file` | Written to that file. The extension must match the content |
| `-o dir/` | Written into that folder with the original names |
| `-o -` | Written to standard output |
| Output already exists | Fails with `OUTPUT_EXISTS`, unless you pass `--force` |
| `--in-place` | Replaces the input, atomically. This is the only way to change an input |

## Values

| Kind | Examples | Used by |
|---|---|---|
| Pages | `1-3,5,8-`, `last`, `-2`, `odd`, `even`, `all` | `--pages` |
| Time | `90`, `1:30`, `01:02:03.5`, `1m30s`, `500ms` | `--from`, `--to-time`, `--at` |
| Size | `1920x1080`, `800x`, `x600`, `50%` | `--size` |
| Length | `10mm`, `1cm`, `0.5in`, `36pt`, `20px` | `--margin`, `--font-size` |
| File size | `500KB`, `1.5MB`, `2GB` (1KB = 1024 bytes) | `--target-size` |
| Colour | `#ff0000`, `#f00`, `#ff000080`, `red` | `--color` |
| Position | `top-left`, `top`, `center`, `bottom-right` and the rest of a 3x3 grid | `--position` |

`deft help values` has the current list.

## Results

The output is one line per result, and the first word tells you how it went.

<!-- OK -->
```console
$ deft pdf compress photos.pdf
ok pdf.compress photos.pdf -> photos.compressed.pdf  704KB -> 272KB (-61%)  0.10s
```
<!-- /OK -->

Add `--json` for structure, or set `DEFT_JSON=1`.

<!-- JSON -->
```json
{
  "v": 1,
  "ok": true,
  "op": "pdf.compress",
  "items": [
    {
      "ok": true,
      "inputs": [
        "photos.pdf"
      ],
      "outputs": [
        {
          "path": "small.pdf",
          "format": "pdf",
          "bytes": 278842
        }
      ],
      "facts": {
        "bytes_before": 720475,
        "bytes_after": 278842,
        "ratio": 0.387,
        "images_recompressed": 1
      },
      "warnings": []
    }
  ],
  "elapsed_ms": 67
}
```
<!-- /JSON -->

## Errors

An error has a stable code, and a hint when there is a concrete fix.

<!-- ERROR -->
```console
$ deft pdf compress locked.pdf
error INPUT_ENCRYPTED locked.pdf: the PDF is password-protected
hint: add --password <password>

$ deft pdf compres photos.pdf
error UNKNOWN_COMMAND deft pdf has no command "compres"
hint: did you mean: deft pdf compress | deft audio compress | deft image compress
```
<!-- /ERROR -->

| Exit code | Meaning |
|---|---|
| 0 | All succeeded |
| 1 | Processing failed |
| 2 | Usage error: the option or command does not exist |
| 3 | Input problem: not found, corrupt, wrong type, encrypted |
| 4 | Missing engine |
| 5 | Partial success in a batch |
| 130 | Interrupted |

## Batches

Several inputs run in parallel, one job per CPU core; set the count with `-j`. Files that fail do not stop the others, and the exit code is then 5.

## Safety

- `--dry-run` shows what would happen and writes nothing.
- `--offline` or `DEFT_OFFLINE=1` refuses all network access.
- Commands marked `[net]` in help use the network. Nothing else does, apart from `deft engine install`.
- Deft collects no usage data and keeps no logs of your runs.

## Agent setup

```bash
deft skill install --agent all              # writes ~/.claude/skills/deft and ~/.agents/skills/deft
deft skill install --agent claude --scope project   # into this project instead
deft skill print                            # print the skill text
```

The skill text is generated from the command registry, so reinstall it after updating deft. For an agent without skills, put the output of `deft skill print` in its instructions file. That is what the benchmark does with `AGENTS.md`.

## Engines

Video and audio commands run ffmpeg; `youtube` commands run yt-dlp. Neither ships inside deft.

```bash
deft doctor                     # what is available
deft engine list
deft engine install yt-dlp      # fetches yt-dlp and checks its checksum
deft engine run ffmpeg -- -i a.mp4 -c copy b.mkv    # anything deft has no command for
```

ffmpeg must be installed by hand. See [INSTALL.md](INSTALL.md).

## Plugins

A program named `deft-<group>` on the PATH acts as a group. Plugin commands do not appear in `deft find` yet.

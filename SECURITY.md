# Security

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's **Report a vulnerability** button on the repository's Security tab.

Include the deft version, the command, and a file that reproduces the problem if you can share one.

There is one maintainer, so there is no guaranteed response time. You will get an honest answer about whether and when a fix will ship.

## Supported versions

Deft has not had a release. After 0.1.0, only the latest release gets security fixes.

## What is in scope

Deft reads files that an agent or a person may not trust, so these matter most:

- A crafted file that causes code execution, a crash that loses data, or a write outside the output path.
- A command that overwrites its input or touches the network when it is not marked `[net]`.
- A way around the checksum check in `deft engine install`.
- Arguments reaching a shell. External programs must never run through one.

Bugs in ffmpeg and yt-dlp belong with those projects, unless deft makes them worse.

## How deft limits risk

- Files are processed locally. Deft collects no usage data and keeps no logs of your runs.
- Only `network: true` operations and `deft engine install` may touch the network; `--offline` refuses them.
- Inputs are not overwritten, and writes go to a temporary file before an atomic rename.
- External programs run without a shell, and `deft engine install` verifies checksums.

Fuzz testing has not been done yet.

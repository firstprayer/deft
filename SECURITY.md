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
- A command that overwrites its input, or goes online for anything but the pictures a document links to.
- Arguments reaching a shell. External programs must never run through one.

Bugs in ffmpeg belong with those projects, unless deft makes them worse.

## How deft limits risk

- Files are processed locally. Deft collects no usage data and keeps no logs of your runs.
- Deft goes online only to download pictures that a document being converted links to: plain GET requests over http or https, without cookies or credentials, with a time and size limit for each picture and for the whole run. Only picture data is kept.
- Inputs are not overwritten, and writes go to a temporary file before an atomic rename.
- External programs run without a shell.

Fuzz testing has not been done yet.

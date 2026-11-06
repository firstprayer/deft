# Release packaging (draft)

Nothing here has been published, and no release exists. `build-release.sh` and `install.sh` have been run on macOS arm64 from local files, including a refused tampered archive. No other platform, and nothing against a real GitHub release, has been tested.

`.github/workflows/release.yml` builds the archives with the scripts here.

| File | What it is |
|---|---|
| `build-release.sh` | Builds one target and writes `deft-<target>.tar.gz` and its `.sha256` to `dist/` |
| `install.sh` | Draft for `curl <url> \| sh`: downloads an archive, verifies its checksum, installs the binary, then the skill for each agent folder it finds. `DEFT_NO_SKILL=1` skips the skill |
| `homebrew/deft.rb` | Draft formula for a `firstprayer/homebrew-tap` repository |

## Cutting a release

Run from the repository root.

```bash
cargo xtask check                        # the gate
cargo xtask notices                      # refresh third-party notices
git tag v0.1.0
git push origin v0.1.0
```

The tag runs `.github/workflows/release.yml`. It builds macOS and Linux archives, runs each packaged binary and leaves a draft release with the archives and `install.sh`. Run the workflow by hand to build without a release.

Test each archive on a clean machine, then publish the draft. Fill the version and checksums into `homebrew/deft.rb`, and push that to the tap repository.

## Why not `homebrew/core`

Formulae there must be open source under a DFSG-compatible license. PolyForm Shield is not, so deft stays in a third-party tap.

## Archive contents

`deft`, `LICENSE.md`, `README.md`, `THIRD-PARTY-NOTICES.md`. The license requires the terms and the `Required Notice` line to travel with copies, and third-party notices must ship with binaries.

## Not drafted

- macOS signing and notarising; needs an Apple Developer account.
- Windows signing, npm, PyPI, winget, Scoop.
- crates.io. `cargo install deftcli` needs every workspace crate published first, and whether crates.io accepts the `LicenseRef-` license expression is untested.

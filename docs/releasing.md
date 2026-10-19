# Releasing NoteCraft

NoteCraft is laid out like the Craft apps (WordCraft, PdfCraft) so a release could move into their
pipeline later with little change. It is not part of ArtCraft, and nothing here uses an ArtCraft
name or mark.

## Versions

- One source: `[workspace.package] version` in the root `Cargo.toml`. Every crate inherits it.
- Plain semantic versioning: `MAJOR.MINOR.PATCH`, with an optional `-rc.1` style tag. The Craft
  apps do the same, and they are all in the 0.x range today.
- Git tags and release names are `v<version>`, for example `v0.1.0`.
- `cargo xtask version` prints it. `cargo xtask version set 0.1.0` changes it and refreshes
  `Cargo.lock`.
- Every 0.x and every `-pre` version is marked a pre-release on GitHub.
- An MSI version is numeric, so `0.1.0-rc.1` and `0.1.0-rc.2` both become `0.1.0` there. The
  installer allows same-version upgrades so release candidates replace each other.

## Making a release

1. On `main`, with the gate green, set the version: `cargo xtask version set 0.1.0`. Commit it.
2. Start the build, either way:
   - Actions, Release, Run workflow (the version box overrides `Cargo.toml` for a test run), or
   - merge `main` into the `release` branch and push.
3. The run builds on a Windows runner and drafts a GitHub Release `NoteCraft v<version>`. Nothing
   is published. Try the zip on a clean machine, then publish the draft by hand.

| File | What it is |
|---|---|
| `notecraft-<version>-windows-x64.msi` | Per-machine installer: Program Files, Start menu shortcut |
| `notecraft-<version>-windows-x64-portable.zip` | `notecraft.exe` and `notecraft-cli.exe`, no install |
| `SHA256SUMS.txt` | SHA-256 of the two files above |

Each file also gets a build provenance attestation. Check a download with
`gh attestation verify <file> --repo AlienTechSoftware/notecraft`.

Windows x64 is the only target. macOS and Linux are untested, so they are not built.

## Signing

`packaging/windows/sign.ps1` signs with `WINDOWS_CERTIFICATE` (a base64 PFX) or with Azure Trusted
Signing (`AZURE_*` secrets), and leaves the files unsigned with a warning when neither is set.
No signing secret exists yet, so releases are unsigned and Windows SmartScreen shows "Windows
protected your PC" until a file builds reputation. The draft's notes say so. Keep any secret in
the `release` environment of the repository, never in a file.

## Before the first public release

- Decide on signing (SignPath Foundation is free for open source; check its conditions).
- The MSI `UpgradeCode` in `packaging/windows/notecraft.wxs` is fixed once the first MSI is public.
  Changing it later stops new versions from replacing old ones.
- The exe has the window icon at run time but no embedded file icon yet, and no version resource.
- Smoke test the artifact, not the build: it starts with no console window, a fresh profile gets
  the Welcome notebook in `Documents\NoteCraft`, a page can be opened, edited and saved, and a bad
  notebooks folder shows an error box. Install, upgrade and uninstall must leave the notebooks
  alone.

## Building locally

```text
cargo xtask version
pwsh packaging/windows/package.ps1 -ZipOnly     # portable zip and checksums, no WiX needed
pwsh packaging/windows/package.ps1              # also the MSI (needs WiX v5, see the script)
```

Output goes to `dist/release`. The script checks that the app is a Windows GUI program and the
command line tool a console one, and that both are x64, before it packs anything.

# Releasing NoteCraft

NoteCraft is laid out like the Craft apps (WordCraft, PdfCraft) so a release could move into their
pipeline later with little change. It is not part of ArtCraft, and nothing here uses an ArtCraft
name or mark.

## Nightly releases

The `Nightly release` workflow (`.github/workflows/nightly-release.yml`) makes a release every day
at 07:00 UTC, which is midnight Mountain Standard Time. While daylight time is in force it runs at
01:00 Mountain time, because GitHub's schedule has no time zones. No one has to do anything.

One run:

1. Stops, and releases nothing, when no commit reached `main` since the last release.
2. Stops with an error when CI on `main`'s newest commit is not green.
3. Takes the next patch number above every published release (0.0.9 becomes 0.0.10), runs
   `cargo xtask version set`, rebuilds the Welcome notebook for that number, and commits both on
   top of `main`. Only the tag `v<version>` is pushed. `main` is never pushed to, so the version in
   `main`'s `Cargo.toml` stays where it is, and the released numbers live in the tags.
4. Builds and tests the tagged commit on Linux.
5. Runs the Packaging workflow on the tagged commit: Windows x64 (MSI and portable zip), Linux
   x86_64 and aarch64 (tar.gz, AppImage, deb and rpm), and macOS Apple Silicon and Intel (DMG and
   command line zip).
6. Runs the Website pipeline on the tagged commit and deploys it to notecraft.cloud, including
   `/demo/`. The built web app is kept as `notecraft-web-<version>.zip`.
7. Publishes GitHub Release `v<version>` as a pre-release, not a draft, with every file above and
   one `SHA256SUMS.txt`. The notes are a fixed alpha notice plus GitHub's list of merged pull
   requests.

### Run a release by hand

- On GitHub: Actions, Nightly release, Run workflow, branch `main`, Run workflow.
- From a terminal: `gh workflow run nightly-release.yml --repo AlienTechSoftware/notecraft --ref main`.
- When nothing has merged since the last release the run stops early. Tick **force** (or add
  `-f force=true`) to release anyway, for example after a failed run.
- A failed run leaves a tag without a release. The next run reuses that number and replaces the tag.
- To update only the website, run Actions, Deploy website. It builds `main` and needs no release.

### What it needs

- The `AZURE_STATIC_WEB_APPS_API_TOKEN` repository secret, the one Deploy website already uses.
- Settings, Actions, General, Workflow permissions must let `GITHUB_TOKEN` write contents. The
  workflow asks only for what each job needs, but an organisation or repository setting of
  "read only" caps it and the tag push fails.
- No tag ruleset may block `github-actions[bot]` from creating `v*` tags.
- Nothing is signed. When signing secrets are added (see Signing below), the nightly needs a change
  to use them.

## Versions

- One source: `[workspace.package] version` in the root `Cargo.toml`. Every crate inherits it.
- **Nightly releases bump the patch number by themselves, once a day, in the tag only.** The
  maintainers approved that on 2026-11-12. A minor or major number, or any change to `main`'s
  version line, still needs the owner's approval in words, each time. The first release was
  `v0.0.1`. The owner approved 0.0.2 on 2026-10-19 for the title band, 0.0.3 on 2026-10-21, 0.0.6
  on 2026-10-25, 0.0.7 on 2026-10-29, 0.0.8 on 2026-11-02, and 0.0.9 on 2026-11-06.
- A nightly patch number can carry features, not only fixes. The patch rule below is for releases
  made by hand.
- Plain semantic versioning: `MAJOR.MINOR.PATCH`, with an optional `-rc.1` style tag. The Craft
  apps do the same, and they are all in the 0.x range today.
- **Patch** (0.1.0 to 0.1.1): bug fixes only, or a rebuild after a packaging or workflow fix. No new
  features, and never a change to the notebook folder format. The Craft apps' patch tags (WordCraft
  0.6.1, PdfCraft 0.7.1, CADCraft 0.6.2) were rebuilds after a release workflow fix.
- **Minor** (0.1.x to 0.2.0): new features or changed behaviour. In 0.x, anything that might break
  something goes here too. The patch number goes back to 0.
- **Pre-release:** `0.2.0-rc.1`. Source of these rules: the Craft apps' tags and release guides.
  Their central release playbook was not available, so the rules above are the standard meaning of
  semantic versioning plus what the tags show.
- Git tags and release names are `v<version>`, for example `v0.1.0`.
- `cargo xtask version` prints it. `cargo xtask version set 0.1.0` changes it and refreshes
  `Cargo.lock`.
- Every 0.x and every `-pre` version is marked a pre-release on GitHub.
- The maturity label is **alpha**, said in the README and in each release's notes. The sibling
  Craft apps (WordCraft, PdfCraft, GridCraft) call themselves alpha in their READMEs and release
  plain `v0.x` tags; none of them uses `-alpha` or `-beta` in a version, and neither do we. A
  `-rc.N` tag is for a build to test before a number is final. Beta is a word for later, when
  the owner says so, and it changes the README, not the version.
- An MSI version is numeric, so `0.1.0-rc.1` and `0.1.0-rc.2` both become `0.1.0` there. The
  installer allows same-version upgrades so release candidates replace each other.

## Making a release by hand

The nightly does all of this. These steps are for a release with a number the nightly would not
pick, such as a minor version.

1. On `main`, with the gate green, keep the version as it is. Change it with `cargo xtask version set <x.y.z>` only
   when the owner has approved that exact number. Commit it.
2. Run Actions, Packaging, Run workflow on the approved commit. This manually builds and verifies
   Linux, Windows, and macOS artifacts and uploads them without publishing anything. Record its
   source commit and the version from `[workspace.package]`.
3. Run Actions, Release, Run workflow from `main` at that same commit and enter that Packaging run
   ID. The workflow rejects runs from another branch, commit, or workflow, and derives the release
   version from the checked-out workspace. If `main` advanced, rerun Packaging for the new commit.
   The workflow drafts `NoteCraft v<version>`; it does not publish it.
4. Build Android separately through the approved ArtCraft Android builder. Record the NoteCraft
   source commit and package version, verify the APK package metadata and signing certificate match
   the intended release, then install it on a clean emulator/device. Attach it only after those
   checks pass. Android is not built by this repository's Packaging workflow today.
5. For the website, wait for the `Deploy website` workflow on the same `main` commit to succeed.
   It publishes the browser app and the separately version-checked Welcome notebook archive from
   `docs/demo/DemoNotebook`. Its production check verifies both the served WASM and notebook
   payload. Open the demo in a clean browser session and confirm it opens the official Welcome
   notebook. If a production run is cancelled or fails, rerun it before signoff.
6. Review the draft's complete asset list and rendered notes, then test each platform package on a
   clean target. Publish the draft only after the owner approves.

The website, desktop release, and Android package are separate pipelines. Each bundles both
generated Welcome formats. Desktop NoteCraft runs the first-use wizard once after installation or
a preferences reset and creates the managed Welcome notebook only when the user chooses Yes. For
0.0.9 the wizard is verified on Windows only; the website and Android flows are deferred for it. The website uses a static archive transport. The generic engine sample stays
separate. Record and verify the source revision and version for every artifact.

### Notes and the draft

- The workflow writes its own fixed text into the draft, not the notes in
  `docs/releases/<version>/RELEASE_NOTES.md`. After the draft exists, replace its body with those
  notes: `gh release edit v<version> --draft --notes-file <file>`.
- Release notes are body content below the GitHub release title: do not add a duplicate H1 title
  such as `# NoteCraft <version>` to `RELEASE_NOTES.md`.
- Relative images are unreliable on a release page. Use absolute release-asset URLs for the
  artwork and animated GIF, upload both assets, and verify that both the image and GIF render
  before publishing. The release is not complete if either asset is only present in the checkout.
- Where `pwsh` is not installed, run the packaging script with
  `powershell -NoProfile -ExecutionPolicy Bypass -File packaging/windows/package.ps1`.

### Clean-machine check

Before publishing, install the MSI on a machine that has never run NoteCraft. Follow the
`packaging/installation artifacts.md` contract for the expected user notebook location. Launch
NoteCraft and verify the first-use wizard appears: welcome page with the banner, logging, page format,
create the demo, then thank you. Choose Yes on the demo page and verify it creates
`welcome-to-notecraft` in the chosen format before opening it. Place a sentinel user notebook beside
it and verify it is unchanged. Restart and confirm the wizard does not appear again. Reset the
preferences and confirm it appears again. Confirm an upgrade does not show it.

| File | What it is |
|---|---|
| `notecraft-<version>-windows-x64.msi` | Per-machine installer: Program Files, Start menu shortcut |
| `notecraft-<version>-windows-x64-portable.zip` | App and CLI with the paired-format Welcome source |
| `SHA256SUMS.txt` | SHA-256 of the two files above |

Each file also gets a build provenance attestation. Check a download with
`gh attestation verify <file> --repo AlienTechSoftware/notecraft`.

The manual Packaging workflow builds Windows x64, Linux x64, and macOS Apple Silicon/Intel
artifacts. They are not official public releases until the separate Release workflow is run and
the draft is reviewed. A maintainer must complete native Intel and clean-machine testing,
Developer ID signing and notarization before public macOS distribution.

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
- The exes carry the app icon and version information (build.rs, winresource), and the packaging script checks both.
- Smoke test the artifact: it starts with no console window, asks before refreshing the managed
  Welcome notebook, leaves sibling notebooks unchanged, and reports an invalid notebooks folder.
  Install, upgrade and uninstall must preserve other user notebook data. Use the canonical
  `packaging/installation artifacts.md` contract for the full checks.

## Building locally

```text
cargo xtask version
pwsh packaging/windows/package.ps1 -ZipOnly     # portable zip and checksums, no WiX needed
pwsh packaging/windows/package.ps1              # also the MSI (needs WiX v5, see the script)
```

WiX 5 installs with `dotnet tool install --global wix --version 5.0.2`, which needs .NET SDK 6 or newer. With only SDK 5 on the machine, download the `wix` package from NuGet and run its `wix.dll` with the .NET 8 runtime (`DOTNET_ROLL_FORWARD=Major`) instead.

Output goes to `dist/release`. The script checks that the app is a Windows GUI program and the
command line tool a console one, and that both are x64, before it packs anything.

# NoteCraft macOS packaging guide

This directory contains the NoteCraft macOS app, DMG and CLI archive scripts. The app bundle
includes both generated Welcome notebook formats as the source for the first-use setup. For 0.0.9
the setup is verified on Windows only; the macOS check is deferred. It replaces only `welcome-to-notecraft`; sibling notebooks
remain unchanged. Packages remain test artifacts until a maintainer completes signing,
notarization and clean-machine checks.

## Build and verify test packages

Use macOS with Xcode Command Line Tools, the pinned Rust toolchain in `rust-toolchain.toml`,
`jq` and `shasum`. No Homebrew package is needed by the installed app.

```sh
rustup install
packaging/OSX/package.sh --arch aarch64
packaging/OSX/verify.sh aarch64
packaging/OSX/test-stale.sh aarch64
bash packaging/OSX/test-provenance.sh
```

The DMG contains `NoteCraft.app`, an Applications link and a read-me file at its root. Drag the
app to Applications and open it. On first launch of each version, NoteCraft asks whether to add
the bundled Welcome notebook to the user's notebook folder.

The default `--arch universal` builds both Apple targets and combines the GUI and CLI with
`lipo`. `--arch x86_64` produces Intel-only packages. The DMG and separate CLI zip are written to
`dist/macos/` unless `--out DIR` or `DIST` selects another directory. `CARGO_TARGET_DIR` selects
the Cargo build directory. The scripts leave build outputs in place and remove their own
temporary staging directories. Use a separate output directory when retaining an earlier DMG.

`--no-build` requires a clean Git working tree and a previous clean `package.sh` build for every
requested target. Each record binds the Cargo version, target, executable hashes and full Git
revision to the source state. Both universal slices must match the current source identity.
Old records without source provenance must be rebuilt. The negative tests cover version/hash
changes, same-version revision changes, dirty sources and mismatched universal slice provenance.

Normal builds support uncommitted work. The source snapshot includes staged/unstaged Git changes
and the contents of untracked, non-ignored files. It is checked before and after the build and
packaging; inputs must not change during the command. Dirty builds get a `-dirty` suffix in
`NoteCraftBuildCommit` and cannot be reused with `--no-build`, even if unchanged. Submodules and
Git entries hidden by assume-unchanged or skip-worktree are rejected. Build inputs must belong
to the Git source tree; ignored output directories are not source inputs. Custom target and
output directories should be ignored or outside the source tree.

The finished DMG and CLI archive carry version, post-signing binary hashes and source identity,
checked by `verify.sh` against the current checkout. Bundle build numbers use the full Git commit
count; the pull-request workflow fetches complete history for both architectures, and verification
requires the bundle number to equal that count. On a host that cannot execute a requested
architecture, successful Cargo builds and matching records establish source provenance;
runtime behavior still needs a native host check.

The app bundle contains its executables, icon, both generated Welcome notebook formats, project
licences, attribution and third-party notices. The DMG verifier compares the bundled notebooks
with their official generated source and checks signatures, architecture slices and source hashes.
These package checks do not replace a clean macOS installation test; see [the macOS installation gaps](issues.md) and the canonical
`packaging/installation artifacts.md` contract.
The standalone CLI zip can be extracted without installing the app. Both packages are
signed ad-hoc by default for local testing. A downloaded ad-hoc package can be rejected by
Gatekeeper; do not change system security settings to test it.

For a manual Finder check, use a disposable notebook directory containing a sentinel user notebook
and an older `welcome-to-notecraft`. Open the DMG, drag `NoteCraft.app` into Applications, then
launch it. Choose Yes and confirm the Welcome notebook has both formats and the current content,
the old Welcome files are gone, and the sentinel notebook is unchanged. Relaunch to confirm the
same version does not ask again. Remove only the test app after the check; confirm app removal
leaves the notebook root in place. Repeat on Intel before release.

The preferred direction is to follow the packaging tools and libraries already used by the
ArtCraft applications, especially the sibling MailCraft macOS package. Do not introduce a new
installer framework, package manager, signing service, or app-bundle convention without first
checking the existing ArtCraft implementation and getting approval for the divergence.

## Preferred ArtCraft shape

The reference implementation is:

- `../../../storytold/3p/mailcraft/packaging/macos/package.sh`
- `../../../storytold/3p/mailcraft/packaging/macos/Info.plist.in`
- `../../../storytold/3p/mailcraft/packaging/macos/entitlements.plist`

That package uses the native Apple/Rust toolchain:

1. Build `aarch64-apple-darwin` and `x86_64-apple-darwin` release binaries with Cargo.
2. Set and document the minimum macOS deployment target.
3. Combine each executable's architecture slices with `lipo`.
4. Assemble a normal `NoteCraft.app` bundle with `Contents/MacOS`, `Contents/Resources`,
   `Info.plist`, the `.icns` icon, and required notices/licences.
5. Sign the executable and app bundle with `codesign`, using a Developer ID Application
   identity, a secure timestamp, and the hardened runtime.
6. Create a drag-to-Applications DMG with `hdiutil`.
7. Sign the distributable DMG when credentials are available.
8. Submit with Apple's `xcrun notarytool`, wait for the result, and staple the ticket with
   `xcrun stapler` before calling the artifact distributable.

Ad-hoc signing is useful for local smoke tests only. It is not a substitute for a signed and
notarized release artifact.

## Tool and library preferences

Prefer these components unless an approved design says otherwise:

| Need | Preferred tool | Reason |
|---|---|---|
| Rust binaries | `cargo`, `rustup`, Rust Apple Darwin targets | Matches the Rust application and ArtCraft build conventions. |
| Universal binaries | Apple `lipo` | The sibling ArtCraft package already uses it and it makes architecture inspection explicit. |
| App metadata | `Info.plist` generated from a checked-in template and `plutil` | Native macOS metadata with a lintable, reviewable source. |
| App icon | `.icns` produced by the repository's approved asset workflow | Keeps icon licensing and attribution in the repository. |
| Code signing | Apple `codesign` | Required for Developer ID distribution and verification. |
| Disk image | Apple `hdiutil` | Produces the conventional drag-to-Applications DMG used by the sibling package. |
| Notarization | `xcrun notarytool` and `xcrun stapler` | Current Apple command-line workflow; do not use the retired `altool` path. |
| CI host | Pinned GitHub-hosted macOS runners selected by the pull-request packaging workflow | Configured to build and verify review artifacts on Apple Silicon and Intel. |

Homebrew may be documented for developer setup when an ArtCraft project already uses it, but it
must not become a runtime dependency of the NoteCraft app or a hidden requirement of the release
artifact. Prefer tools supplied by macOS/Xcode and Rustup for the packaging path.

## Version and identity rules

- Read the version from `[workspace.package]` in the root `Cargo.toml`; do not duplicate a current
  release version in this guide or in packaging scripts.
- Keep `CFBundleShortVersionString` and the artifact filename tied to that source.
- Use a monotonically increasing numeric `CFBundleVersion` for the bundle build number, such as
  the CI build number or a documented repository revision count.
- Keep `com.alientechsoftware.notecraft` as the stable bundle identifier, matching the native
  viewport app ID. Confirm it with the maintainer before the first signed release.
- Keep the minimum macOS version explicit in the build environment, Rust target configuration,
  and `Info.plist`; test on that minimum before release.

## Signing and secret handling

The release workflow must keep signing material out of the repository and logs. Follow the sibling
ArtCraft pattern of importing a certificate into a temporary keychain, signing with the hardened
runtime, verifying with `codesign --verify --strict`, and deleting the temporary keychain on exit.

The release agent should define the exact CI secret names and credential format before adding the
workflow. At minimum, distinguish:

- Developer ID Application certificate and password;
- Apple team identifier;
- notarization account or App Store Connect API credentials;
- app-specific password or equivalent notary credential, if that workflow is chosen.

Never print certificate material, passwords, keychain passwords, or notarization credentials.

## Required validation before signoff

- Build both Apple architectures from the same commit and confirm each binary with `lipo -info`.
- Check the app bundle with `plutil -lint`.
- Verify the executable, CLI, app bundle, and DMG signatures with `codesign`/`spctl`.
- Inspect the DMG contents and confirm the app, Applications link, icon, notices, licences, and
  attribution files are present.
- Run the app from the DMG on Apple Silicon and Intel, or document the missing hardware test.
- Test first launch from a clean macOS user/profile outside the repository.
- Confirm the local notebook, privacy flow, logs, and file associations behave as intended.
- Submit the exact release DMG to Apple notarization, inspect the notary log, staple the ticket,
  and verify the stapled artifact offline where possible.
- Compare the packaged executable with the build output and record SHA-256 checksums.
- Record commands, runner image, Xcode/Rust versions, artifact paths, signatures, notarization
  result, manual checks, and known gaps in the release signoff record.

Do not claim macOS packaging is complete merely because a cross-compiled binary exists. A signed,
notarized, installed, and manually exercised artifact is the release outcome.

## Sources and local precedents

- ArtCraft MailCraft implementation: `../../../storytold/3p/mailcraft/packaging/macos/`.
- ArtCraft CADCraft release documentation: `../../../storytold/cadcraft/README.md`.
- [Rust Apple Darwin platform support](https://doc.rust-lang.org/rustc/platform-support/apple-darwin.html).
- [Apple: notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
- [Apple: resolving common notarization issues](https://developer.apple.com/documentation/security/resolving-common-notarization-issues).
- [Apple: hardened runtime](https://developer.apple.com/documentation/security/hardened-runtime).
- [GitHub: choosing a runner for a job](https://docs.github.com/en/actions/how-tos/write-workflows/choose-where-workflows-run/choose-the-runner-for-a-job).

The GitHub Actions workflow creates review artifacts only. The existing release workflow remains
Windows-only; no macOS release is published by this package script or pull-request workflow.

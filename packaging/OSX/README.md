# NoteCraft macOS packaging guide

This directory is reserved for the NoteCraft macOS installer and distribution work.

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
| CI host | A pinned GitHub-hosted macOS runner selected by the release workflow | Provides Xcode command-line tools and avoids pretending Linux can perform the final Apple signing step. |

Homebrew may be documented for developer setup when an ArtCraft project already uses it, but it
must not become a runtime dependency of the NoteCraft app or a hidden requirement of the release
artifact. Prefer tools supplied by macOS/Xcode and Rustup for the packaging path.

## Version and identity rules

- Read the version from `[workspace.package]` in the root `Cargo.toml`; do not duplicate a current
  release version in this guide or in packaging scripts.
- Keep `CFBundleShortVersionString` and the artifact filename tied to that source.
- Use a monotonically increasing numeric `CFBundleVersion` for the bundle build number, such as
  the CI build number or a documented repository revision count.
- Choose a stable reverse-DNS bundle identifier under the NoteCraft project namespace and keep it
  stable across upgrades. Record the decision before publishing the first signed build.
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

The next implementation agent should read this guide, inspect the sibling ArtCraft package again, and
write the macOS-specific acceptance criteria before adding scripts or workflow jobs.

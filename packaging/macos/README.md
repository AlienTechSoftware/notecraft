# macOS packages

`package.sh` builds a native app, a drag-to-Applications DMG, and a separate CLI zip. It uses
macOS tools included with Xcode Command Line Tools, system `jq` and `shasum`, and the Rust version in `rust-toolchain.toml`.
No Homebrew package or Apple Developer account is needed for local builds.

## Build

On an Apple Silicon Mac, from the repository root:

```sh
rustup install
packaging/macos/package.sh --arch aarch64
packaging/macos/verify.sh aarch64
```

The default architecture is `aarch64`. Outputs go to `dist/macos/`:

```text
notecraft-<version>-macos-aarch64.dmg
notecraft-cli-<version>-macos-aarch64.zip
```

`--skip-build` packages binaries already built at
`${CARGO_TARGET_DIR:-target}/aarch64-apple-darwin/release/`. First run `package.sh` without
`--skip-build` for each target. That run records the Cargo version, target and SHA-256 of both
executables next to them. A later `--skip-build` fails if the record is missing or either binary
has changed. Native binaries are also checked with `--version`; for a non-native target, the
build record binds its version to the successful Cargo build. Each finished package carries a
version and binary hash record, which `verify.sh` checks even when it cannot execute the binary.
The GUI record is alongside the signed app inside the DMG, since signing the app changes its
executable after its bundle resources are sealed.
Set `DIST` and `CARGO_TARGET_DIR` to choose other output locations. The version comes from
`[workspace.package]` in `Cargo.toml`, and the build commit comes from Git. The bundle carries
`NoteCraft.icns` derived from the attributed PNG in `assets/brand/`, both project licenses,
`NOTICE`, `ATTRIBUTION.md`, and the additional notices in `licenses/`.

The GUI does not require the CLI. Its demo and UI assets are embedded in the executable. The
package does not claim file type associations: open notebooks using NoteCraft's own interface.

## Install and use

```sh
open dist/macos/notecraft-<version>-macos-aarch64.dmg
```

Drag `NoteCraft.app` to `Applications`, eject the image, and open NoteCraft from Finder. Choose
or create a notebook folder in the app. For a controlled test, use a new disposable folder; do
not use notebooks you care about to evaluate an alpha build.

The CLI is separate. Extract the zip, move `notecraft-cli` to a directory of your choice, and
run `notecraft-cli --version`. It need not be installed for the app to work.

The default package is signed ad-hoc for local testing. Ad-hoc signing is not trusted public
distribution and is not notarized. A downloaded copy may be blocked by Gatekeeper. Do not
disable Gatekeeper or change system security settings. A public release needs a Developer ID
Application signature and Apple notarization, followed by validation on another Mac.

## Architectures and signing

`--arch x86_64` builds Intel binaries; `--arch universal` builds both targets and combines each
binary with `lipo`. Install the target first if it is absent:

```sh
rustup target add x86_64-apple-darwin
packaging/macos/package.sh --arch universal
packaging/macos/verify.sh universal
```

Each input and packaged binary is checked with `lipo`. If an Intel-only binary cannot run on an
Apple Silicon host, the script skips that smoke test; the CI workflow checks Intel execution on
an Intel runner. Universal runtime support still needs validation on both host types before
publishing. The Apple Silicon package does not depend on those checks.

Set `MACOS_SIGN_IDENTITY` to a Developer ID Application identity to sign with the hardened
runtime and a secure timestamp. Signing credentials are never stored in this repository. The
script does not submit packages for notarization; a maintainer must notarize and staple the app
and DMG, then check them on a clean Mac before public distribution.

## Troubleshooting

- `missing executable`: build the requested Rust target or omit `--skip-build`.
- `wrong architecture`: remove the stale binary from the requested target directory and rebuild.
- `iconutil` failure: check that Xcode Command Line Tools are installed and that the source PNG
  exists at `assets/brand/notecraft-512.png`.
- `hdiutil` mount failure during verification: eject other NoteCraft test images and rerun
  `verify.sh`. The script never installs files into `/Applications`.
- `codesign --verify` failure: inspect the exact failing bundle path. Do not alter a signed app
  after packaging it.
- First launch fails: inspect the app's local `notecraft.log` next to its settings file and
  confirm that the chosen notebook directory is writable. `NOTECRAFT_SETTINGS` can point to
  an isolated settings file for testing.

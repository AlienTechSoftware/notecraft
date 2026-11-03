# Demo notebook installation design

Status: cross-platform packaging design and release requirement.

## Purpose

Every desktop release must include the official NoteCraft demo notebook. A clean install must
make that notebook available to the user, while an existing user's edits must not be silently
overwritten.

This document applies to Windows, Linux, and macOS installers and portable artifacts. Platform
specific gaps are tracked in `packaging/OSX/issues.md` and in the relevant release signoff record.

## Source of truth

The authored demo content is under `docs/demo/scripts/content`.

The generated, checked-in release notebook is:

- `docs/demo/DemoNotebook/html`
- `docs/demo/DemoNotebook/cmgfm`

The two generated formats must contain the same notebook structure, pages, languages,
translations, metadata, links, assets, and feature examples. Only the serialized page format and
required format declaration may differ.

`my-notebooks/welcome-notebook` is disposable test data. It is never the source for a package.
Refresh it from the official generated notebook when a test needs a local copy:

```text
cargo xtask demo-copy html --replace
cargo xtask demo-copy cmgfm --replace
```

Before packaging, regenerate and check the official notebook:

```text
cargo xtask demo
cargo xtask demo-check
cargo test -p notecraft-formats --test format_parity
```

## Required release behavior

Every desktop artifact must carry the generated demo payload, including both `html` and `cmgfm`
formats. The package build must fail when either format is missing, empty, or structurally
incomplete.

The payload is required in all of these artifact families:

- Windows MSI and portable archive.
- Linux tar archive, AppImage, deb, and rpm when produced.
- macOS app bundle and DMG, with any future pkg or setup helper following the same rule.

The package copy is a release payload and an inspection/audit aid. It is not a second authored
source and must not be edited by hand.

## Installation and first launch contract

The intended product behavior is:

1. Setup installs the application and carries the official demo payload.
2. First launch creates or locates the Welcome notebook in the user's configured notebooks root.
3. The app opens the Welcome notebook after a clean install.
4. Later launches do not replace user changes.
5. Users can edit, move, sync, or delete the notebook like any other notebook.
6. A deliberate restore or repair action may replace it, but that action must be explicit and
   documented.

Until the install-if-absent behavior is implemented, the release record must clearly state that
startup deployment still uses the embedded fallback and must not claim that setup installed the
user notebook. The embedded copy is a fallback, not the long-term installation source.

## Platform rules

Windows installers and portable archives must stage the demo in their package payload and verify
the staged HTML entry page before producing the artifact.

Linux packages must stage the demo in the common filesystem tree before creating tar, AppImage,
deb, or rpm outputs. Each output must be inspected from the final artifact, not only from the
temporary staging directory.

macOS app bundles and DMGs must include the demo under app resources. A drag-to-Applications DMG
cannot silently write to `~/Documents/NoteCraft`; installing the user notebook requires either a
signed installer with an approved per-user setup strategy or a separately consented setup helper.
See `packaging/OSX/issues.md`.

## Verification requirements

The release signoff must include evidence for each artifact family produced:

- The source was regenerated and passed demo parity checks.
- Both generated formats and their language folders are present in the final artifact.
- A clean profile or VM launches the app and opens the Welcome notebook.
- A user edit survives normal restart.
- Upgrade behavior is tested and does not overwrite the edited notebook.
- A deliberate restore path, if implemented, is tested separately.
- Package contents, version, checksums, licenses, notices, and attribution are recorded.
- No personal notebooks or machine-specific paths are included in the artifact.

The packaging scripts should enforce the cheap content checks. Installer smoke tests must verify
the installed result on the target platform. A green package build alone is not sufficient release
evidence.

## Future implementation work

- Change `notebook.welcome` and startup deployment from replace-on-every-start to install-if-absent
  plus an explicit restore operation.
- Define the setup payload contract used by all three desktop platforms.
- Remove the full embedded duplicate, or keep only a small documented repair fallback.
- Add clean-install, restart, upgrade, and restore regression tests.
- Add macOS Apple Silicon and Intel install/launch coverage.
- Keep the generated HTML and CMGFM notebooks in parity for every demo change.

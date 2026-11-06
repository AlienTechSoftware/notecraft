# macOS installation issues

This file records macOS-specific gaps related to demo notebook installation. The cross-platform
policy lives in the canonical `packaging/installation artifacts.md` contract;
this file records only macOS implementation gaps.

## Current limitations

1. The current artifact is a drag-to-Applications DMG. Copying an app from a DMG cannot reliably
   perform a per-user write to `~/Documents/NoteCraft`.
2. `packaging/OSX/package.sh` stages `docs/demo/DemoNotebook` under
   `NoteCraft.app/Contents/Resources/demo`, but that only proves the payload is inside the app.
   It does not prove that setup installed the Welcome notebook into the user's notebooks root.
3. The app still has a runtime embedded-notebook deployment path. Its replacement and upgrade
   behavior must be changed and tested before macOS can claim install-if-absent semantics.
4. There is no approved signed `.pkg` or separately consented first-run setup helper for the
   per-user notebook installation step.
5. Automated clean-profile install and first-launch coverage is not yet available for both Apple
   Silicon and Intel macOS.
6. The package script supports ad-hoc signing for local smoke tests, but an ad-hoc app is not a
   distributable release. Developer ID signing, notarization, stapling, and offline verification
   still require release credentials and a macOS release runner.

## Required before macOS release signoff

- Decide whether per-user installation uses a signed pkg or an explicit setup helper.
- Define consent, failure recovery, uninstall, and multi-user behavior for that setup path.
- Verify that the final app bundle and DMG contain both generated demo formats.
- Test clean first launch, restart, upgrade, edit persistence, and explicit restore on Apple
  Silicon and Intel.
- Record signing, notarization, stapling, checksums, runner versions, and manual results in the
  release signoff record.

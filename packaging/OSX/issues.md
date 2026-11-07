# macOS installation issues

This file records macOS-specific gaps related to User Assistance notebook installation. The
cross-platform policy lives in the canonical `packaging/installation artifacts.md` contract.

## Current limitations

1. The DMG now keeps `NoteCraft.app`, the separate setup app and optional `Welcome Notebook/`
   payload as sibling items. The setup app uses explicit consent and install-if-absent behavior.
   Native macOS package execution has not yet been run from this Windows development host.
2. Clean-profile install, repeat setup, notebook edit/deletion, app upgrade and uninstall behavior
   still need manual verification on Apple Silicon and Intel.
3. The package script supports ad-hoc signing for local smoke tests, but an ad-hoc app is not a
   distributable release. Developer ID signing, notarization, stapling and offline verification
   still require release credentials and a macOS runner.

## Required before macOS release signoff

- Verify the setup app's consent, failure recovery and existing-notebook preservation on macOS.
- Verify that the app bundle contains no notebook and the optional DMG sibling contains both
  generated formats; verify a second DMG with the optional payload omitted.
- Test clean first launch, restart, upgrade, edit persistence, explicit restore and deletion on
  Apple Silicon and Intel.
- Record signing, notarization, stapling, checksums, runner versions and manual results in the
  release signoff record.

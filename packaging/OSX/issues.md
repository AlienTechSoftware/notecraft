# macOS installation issues

This file records macOS-specific gaps related to User Assistance notebook installation. The
cross-platform policy lives in the canonical `packaging/installation artifacts.md` contract.

## Current limitations

1. The app bundle carries both generated formats. The first-use setup is deferred on macOS for 0.0.9
   by the owner's Windows-first decision (2026-11-08). Native macOS package execution has not yet been
   run from this Windows development host.
2. Clean-profile launch, replacement of an older Welcome notebook, sibling notebook preservation,
   app upgrade and uninstall behavior still need manual verification on Apple Silicon and Intel.
3. The package script supports ad-hoc signing for local smoke tests, but an ad-hoc app is not a
   distributable release. Developer ID signing, notarization, stapling and offline verification
   still require release credentials and a macOS runner.

## Required before macOS release signoff

- Verify that the app bundle contains both generated formats, Yes replaces the Welcome copy, No
  preserves it, and neither choice modifies a sibling notebook.
- Test clean first launch, restart, upgrade, Welcome replacement and uninstall on
  Apple Silicon and Intel.
- Record signing, notarization, stapling, checksums, runner versions and manual results in the
  release signoff record.

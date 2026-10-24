---
name: nc-reelase-signoff
description: Run NoteCraft's complete pre-release signoff, including version, demo parity, tests, lint, packaging, installers, assets, and release evidence.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-ua, nc-prod, nc-hl-design]
---

# nc-reelase-signoff

Use this skill before declaring a NoteCraft release ready. The name preserves the repository's
requested `reelase` spelling for discoverability.

## Signoff gates

1. Confirm the approved version in `[workspace.package]`; do not bump it without explicit approval.
2. Check branch, status, diff, tags, and release notes. No unrelated or generated scratch files.
3. Regenerate the demo from `docs/demo/scripts/content` and refresh the scratch copy.
4. Run `cargo xtask demo-check`, format parity tests, and visual HTML/CMGFM comparison.
5. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, and `cargo xtask ci` when available.
6. Run layers, assets, docs, text, and relevant UI/integration tests.
7. Build the debug, release, and production/install artifacts appropriate to the release.
8. Verify installer contents, bundled bridge/discovery behavior, version/About text, icons,
   screenshots, licenses, and attribution. Test installation in a clean VM when the release changes
   packaging or external bridges.
9. Confirm no automatic telemetry upload, local privacy wording, and release defaults.
10. Record exact commands, outputs, artifact paths, commit, tag candidate, known gaps, and manual
    checks. Do not push, publish, tag, or release without explicit owner approval.

## Stop conditions

Stop and report instead of guessing when a gate fails, the version is ambiguous, a branch contains
unrelated work, an installer cannot be tested, or generated demo parity is not proven.

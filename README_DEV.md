# NoteCraft developer guide

This is the short entry point for contributors. The detailed, authoritative CI/CD
rules live in [`SKILLS/nc-devx/SKILL.md`](SKILLS/nc-devx/SKILL.md). Read that file
when changing build scripts, workflows, packaging, or release behavior.

## First day

1. Install the pinned Rust toolchain and platform dependencies with the setup script
   for your operating system.
2. Enable the repository hooks:

   ```bash
   git config core.hooksPath .githooks
   ```

3. Work on a branch, not `main`.
4. Run the smallest useful check while working, then run the full local gate before
   asking for review:

   ```bash
   cargo xtask ci
   ```

## The four local quality levels

The levels are cumulative:

| Level | Purpose | Main command or trigger |
| --- | --- | --- |
| L0 | Fast feedback while changing code | `cargo fmt`, focused `cargo clippy`, focused `cargo test` |
| L1 | Commit safety | Pre-commit hook: branch, PII, format, layers, assets, docs, and text checks |
| L2 | Push safety | Pre-push hook: branch freshness, PII, and full `cargo xtask ci` |
| L3 | Merge safety | GitHub PR CI: quality checks plus build/test on Linux, Windows, and macOS |

L4 is the release process rather than an ordinary development gate: packaging,
installation validation, release notes, and publication approval.

## GitHub workflow lanes

The workflow lanes are deliberately separate:

1. **Quality** - `.github/workflows/ci.yml` runs format, clippy, static checks,
   demo checks, tests, and visible but non-blocking coverage.
2. **Platform build/test** - the same CI workflow builds and tests the workspace on
   Linux, Windows, and macOS. It does not create installers.
3. **Packaging** - `.github/workflows/packaging.yml` is manual-only. It builds and
   verifies the Linux, Windows, and macOS artifacts and uploads them as artifacts.
4. **Installation/deployment** - `.github/workflows/release.yml` is manual-only.
   It takes the successful Packaging run ID, downloads those exact artifacts, and
   creates a draft release. It does not rebuild them.

Normal PRs do not package, publish, sign, or notarize anything. We use standard
GitHub-hosted runners and local/VM validation because the project has no budget for
paid runners or signing services. Do not add a paid service or weaken a required
gate without owner approval.

## Before asking for review

```bash
cargo fmt --all -- --check
cargo xtask ci
git diff --check
```

For a release, follow [`docs/releasing.md`](docs/releasing.md): run Packaging first,
verify the artifacts, then run Release with that Packaging run ID. A green source
build does not prove that an installer places files correctly, and a packaging run
does not replace the cross-platform CI gate.

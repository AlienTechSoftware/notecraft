---
name: nc-reelase-signoff
description: Run NoteCraft's complete pre-release signoff, including the OneNote feature and serialization matrix, version, demo parity, tests, lint, packaging, installers, assets, and release evidence.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-ua, nc-prod, nc-hl-design]
---

# nc-reelase-signoff

## Job to be done

Verify NoteCraft release readiness with complete version, demo, test, packaging, installer, asset, and evidence checks.

Use this skill before declaring a NoteCraft release ready. The name preserves the repository's
requested `reelase` spelling for discoverability.

## Signoff gates

1. Confirm the approved version in `[workspace.package]`; do not bump it without explicit approval.
2. Check branch, status, diff, tags, and release notes. No unrelated or generated scratch files.
3. Update the authored demo under `docs/demo/scripts/content`, regenerate both formats into
   `docs/demo/DemoNotebook`, and refresh the ignored developer copy from the generated output.
4. Run `cargo xtask demo-check`, format parity tests, and visual HTML/CMGFM comparison.
   Use [`references/onenote-feature-serialization-matrix.xlsx`](references/onenote-feature-serialization-matrix.xlsx)
   to review OneNote feature coverage, serialization layers, and candidate test scenarios. Treat
   fixture mappings inferred from names as leads only; verify fixture contents before counting them
   as coverage. Update parse and round-trip status only from observed results.
5. Run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
   `cargo test --workspace`, and `cargo xtask ci` when available.
6. Run layers, assets, docs, text, and relevant UI/integration tests.
7. Build the debug, release, and production/install artifacts appropriate to the release.
8. Verify installer contents, bundled bridge/discovery behavior, version/About text, icons,
   screenshots, licenses, and attribution. Test installation in a clean VM, device, or browser when
   the release changes packaging or external bridges. Apply `packaging/installation artifacts.md`
   and the platform matrix below. Verify each artifact carries both generated formats and all 13
   demo locales. The first-use wizard offers the managed Welcome notebook once, after installation or a
   preferences reset; installation copies none. A sentinel sibling notebook must survive it. For
   0.0.9 the wizard is signed off on Windows only.
   Verify upgrade and uninstall behavior for each platform, including Android's app-specific data
   behavior. Record platform limits rather than treating a package build as an install test.
9. Confirm no automatic telemetry upload, local privacy wording, and release defaults.
10. Record exact commands, outputs, artifact paths, commit, tag candidate, known gaps, and manual
    checks. Do not push, publish, tag, or release without explicit owner approval.
11. For a GitHub draft, check the actual draft body and asset list after the final edit. Confirm
    every embedded image URL returns HTTP 200 and renders in the draft preview; an uploaded GIF
    asset alone does not make it appear inline. Resolve repository-relative links to stable URLs
    that exist at the referenced ref. Re-read the draft URL after editing because an untagged draft
    URL can change.
12. Check every shipped platform, including external Android build workflows. Verify the build
    source commit and embedded/package version match the approved release. Do not attach an APK
    from an older version or describe a platform as included when its verified artifact is absent.
13. Verify the hosted demo after deployment, not just the checked-in notebook: the latest
    `Deploy website` run must be successful for the release source commit, and its live-version
    check must match `[workspace.package] version` for both the WASM and the separate Welcome
    notebook archive. Test the demo in a clean browser session and confirm it opens the official
    `Welcome to NoteCraft` notebook, not the engine's generic sample. If deployment was cancelled,
    rerun it from `main` and verify the live assets before signoff.
14. Keep deployment provenance aligned. Run desktop Packaging and Release from the same `main`
    commit; confirm the Release workflow accepted that exact Packaging run. For Android, record
    the external builder's source commit, APK package version, signing certificate identity, and
    clean-install result. Android is a separate manual build today. Do not present the release as
    platform-complete until every advertised platform has a verified artifact or its absence is
    clearly called out.
15. Confirm every shipped User Assistance payload contains both generated formats and all 13
    languages. Both formats must pass parity checks before packaging.

## Platform signoff matrix

Copy this matrix into the release record and replace each `NOT RUN` with a status and evidence
reference. Keep one row per shipped target. Split a row when architectures or delivery channels
have separate artifacts or install results. Status and evidence rules are in
[`references/templates.md`](references/templates.md).

| Platform | Refactor demo | Build | Lint | Test | Gates | Install | Install test | Uploaded | Ready |
|---|---|---|---|---|---|---|---|---|---|
| web-WASM | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Windows | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| macOS | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Linux | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Android | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |

## Release checklist

Use the same checklist in `references/templates.md` for each release. A checked item records
workflow progress only; the evidence reference supports the status.

| Done | Check | Evidence / status |
|---|---|---|
| [ ] | Approved version is consistent in Cargo metadata, app metadata, artifacts, and release draft. | NOT RUN |
| [ ] | Demo scripts and authored content are updated; HTML and CMGFM outputs are regenerated and parity-checked. | NOT RUN |
| [ ] | Both generated formats and all 13 locales are present in `docs/demo/DemoNotebook`; the developer copy is refreshed from that source. | NOT RUN |
| [ ] | Formatting, lint, tests, static gates, and required CI pass on the release source commit. | NOT RUN |
| [ ] | Coverage is run and its result is recorded, including warning-only status. | NOT RUN |
| [ ] | Each platform row has build, install, clean-install test, artifact, and provenance evidence. | NOT RUN |
| [ ] | `docs/releases/<version>/RELEASE_NOTES.md` matches verified release scope. | NOT RUN |
| [ ] | `build_git.py` is reviewed and its generated change list is reconciled with merged PRs. | NOT RUN |
| [ ] | The version-specific GIF generator (`docs/releases/<version>/<version>.py`) and source frames are updated; the GIF is regenerated and verified in the draft preview. | NOT RUN |
| [ ] | Draft links, images, artifact list, hashes or attestations, known gaps, and recovery notes are reviewed. | NOT RUN |

## Release notes assets

- [`docs/releases/template/RELEASE_NOTES.md`](../../docs/releases/template/RELEASE_NOTES.md) is the
  starting skeleton for each version's release notes. Copy it into the versioned release folder
  and replace the placeholders with approved, verified content.
- [`docs/releases/template/build_git.py`](../../docs/releases/template/build_git.py) generates a
  first-parent Git change list for review. Copy it into the versioned release folder and run it
  with the previous release tag. The generated list is source material, not finished release copy;
  reconcile it with merged PRs and the approved release scope.

## Stop conditions

Stop and report instead of guessing when a gate fails, the version is ambiguous, a branch contains
unrelated work, an installer cannot be tested, or generated demo parity is not proven.

## References

- [`references/0.0.6-lessons.md`](references/0.0.6-lessons.md): what went wrong in 0.0.6, the checks that
  would have caught it, and a checklist for the next sign-off. Read it before starting a release.
- [`references/templates.md`](references/templates.md): release matrix, status definitions, evidence
  record, and reusable checklist.

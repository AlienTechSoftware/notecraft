# NoteCraft v0.0.9 Readiness

## Release identity

| Field | Value |
|---|---|
| Release version | 0.0.9 |
| Release source commit | `171e2d5328a2cb5ce0df165ff0f6a8b896741eae` |
| Release branch or tag candidate | `release/0.0.9`; no tag candidate set |
| Previous release tag | Not verified in this signoff run |
| Approved release scope reference | `docs/releases/0.0.9/RELEASE_NOTES.md` |
| Signoff owner | NoteCraft owner |

This is a local readiness snapshot, not release approval. The local WASM and archive were built from
the source tree immediately before it was committed as `171e2d5`; rebuild them from that exact commit
in CI to establish published artifact provenance. The working tree is clean.

## Platform signoff matrix

| Platform | Refactor demo | Build | Lint | Test | Gates | Install | Install test | Uploaded | Ready |
|---|---|---|---|---|---|---|---|---|---|
| web-WASM | PASS | PASS | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |
| Windows x64 MSI | PASS | PASS | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |
| Windows x64 portable | PASS | PASS | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |
| macOS | PASS | BLOCKED | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |
| Linux x64 tar.gz | PASS | PASS | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |
| Android APK | PASS | BLOCKED | PASS | PASS | PASS | BLOCKED | BLOCKED | NOT RUN | BLOCKED |

### Evidence and blockers

| Platform | Evidence |
|---|---|
| Shared demo | `cargo xtask demo html`, `cargo xtask demo cmgfm`, and `cargo xtask demo both` completed. `cargo xtask demo-check` reports 662 generated files current. `cargo test -p notecraft-formats --test format_parity` passed (2 tests). Both formats contain 13 locales. |
| Shared code gates | `cargo xtask ci` passed after the generator refactor. It includes format, clippy, workspace tests, layer, asset, docs, text and demo checks. `python .github/web-build/test_bundle_demo.py` passed (2 tests). `git diff --check` passed. |
| web-WASM | Fresh local Trunk and pinned Binaryen builds completed. `website/demo/notecraft-web-aaa2af09d8cd8925_bg.wasm` SHA-256: `672C72D2C52E973C00C5D9CB5B7E832EDB73BF3AE324723E1CF43E98F30DD286`; it embeds the `0.0.9` marker. On 2026-11-07, `https://notecraft.cloud/demo/` returned HTTP 200 and its referenced WASM returned HTTP 200 with the `0.0.9` marker, but `https://notecraft.cloud/demo/welcome-notebook.zip` returned HTTP 404. The release notebook archive is built locally at `website/demo/welcome-notebook.zip` (662 files: 331 per format; SHA-256 `ED977B51991E3C1FA5C4B0FFCECA9778A96E2D7370731BD10E6A1D85C2E577FD`). The live archive is absent, so first-load provisioning cannot succeed on the hosted site. Deployment and clean-browser install test remain blocked. |
| Windows x64 MSI | `dist/release/notecraft-0.0.9-windows-x64.msi` SHA-256: `1C3C280AA43189A00A6A5E66DA3B02242D2CCEF32FE58FAEA94AFA112C42AA21`. Package is unsigned. Clean VM install test and upload not run. |
| Windows x64 portable | `dist/release/notecraft-0.0.9-windows-x64-portable.zip` SHA-256: `1C0D7891542888CE310AD1A41C603433092FBD310071676C2E88E916258DD2D6`. Clean VM launch and upload not run. |
| Linux x64 tar.gz | `dist/linux/notecraft-0.0.9-linux-x86_64.tar.gz` SHA-256: `CBC30F4C443AF714ECC24EB164170CFF38659E075F0BB9046B884756E1529A85`. Package smoke test passed; install on a clean Linux target and upload not run. |
| macOS | Build requires a macOS runner. No current-candidate package or install result. |
| Android APK | The Android builder is external to this repository and has not been rebuilt from this candidate. No current-candidate APK or install result. |

`PASS` on shared code checks records local command results; it does not establish release readiness.
Every platform remains `BLOCKED` until its artifact provenance, installation, clean-target test,
upload and deployment requirements are satisfied for one immutable source commit.

## Release checklist

| Done | Check | Evidence / status |
|---|---|---|
| [x] | Approved version is consistent in workspace metadata and local artifacts. | PASS: version 0.0.9. Release draft consistency not rechecked in this run. |
| [x] | Demo scripts and authored content are updated; HTML and CMGFM outputs are regenerated and parity-checked. | PASS: `cargo xtask demo both`, `cargo xtask demo-check`, and `cargo test -p notecraft-formats --test format_parity`. |
| [x] | Both generated formats and all 13 locales are present; developer copy is refreshed. | PASS: selected-format and both-format commands completed; `my-notebooks/welcome-notebook` refreshed from each output. |
| [x] | Formatting, lint, tests, static gates, and CI pass on the local candidate tree. | PASS: `cargo xtask ci`; pre-commit L1 hooks; `python .github/web-build/test_bundle_demo.py`; `git diff --check`. Candidate is committed as `171e2d5`. |
| [ ] | Coverage is run and its result is recorded. | NOT RUN. |
| [ ] | Each platform has build, install, clean-install test, artifact, and provenance evidence. | BLOCKED: see platform matrix. |
| [ ] | Release notes match verified scope. | NOT RUN: notes exist but have not been reconciled against merged release scope in this signoff. |
| [ ] | `build_git.py` change list is reconciled with merged PRs. | NOT RUN. |
| [ ] | Release GIF is regenerated and verified in draft preview. | NOT RUN. |
| [ ] | Draft links, images, artifact list, hashes, known gaps, and recovery notes are reviewed. | NOT RUN. |

## Next actions

1. Rebuild platform artifacts from `171e2d5328a2cb5ce0df165ff0f6a8b896741eae`; build macOS on a macOS runner and Android
   through its external builder.
2. Install and launch each candidate on clean targets and verify demo provisioning and data
   preservation.
3. Upload the exact artifacts, deploy the website from the same source commit, and verify the
   hosted demo and release draft.
4. Run coverage and complete the remaining release checklist with immutable evidence links.

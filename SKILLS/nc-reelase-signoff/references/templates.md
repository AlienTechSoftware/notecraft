# Release Signoff Templates

Copy the platform matrix and checklist into the release record for each version. Keep this file as
the canonical definition of status and evidence. The same matrix and checklist appear in
`../SKILL.md` so the required gates are visible during signoff.

## Release identity

Record these values before testing. Every result and artifact must point to the same candidate
unless the release record explains an approved exception.

| Field | Value |
|---|---|
| Release version | NOT SET |
| Release source commit | NOT SET |
| Release branch or tag candidate | NOT SET |
| Previous release tag | NOT SET |
| Approved release scope reference | NOT SET |
| Signoff owner | NOT SET |

## Platform signoff matrix

Keep one row per shipped target. Split a row when architectures, package types, or delivery
channels have separate artifacts or install results. Current desktop packaging targets include
Windows x64, Linux x64, and macOS Apple Silicon and Intel. Android uses its external builder; list
each shipped APK variant. The web row covers the WASM build and hosted demo deployment.

| Platform | Refactor demo | Build | Lint | Test | Gates | Install | Install test | Uploaded | Ready |
|---|---|---|---|---|---|---|---|---|---|
| web-WASM | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Windows | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| macOS | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Linux | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |
| Android | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN | NOT RUN |

### Column meanings

- **Refactor demo:** authored scripts/content and generated HTML and CMGFM outputs match the
  candidate. Both formats pass parity and the package includes the expected 13 demo locales. Link
  the same shared demo evidence in every platform row.
- **Build:** the platform package was produced from the release source commit and contains the
  expected app version. Record package name and SHA-256.
- **Lint:** required formatting and static checks passed for the candidate commit. This is usually
  shared CI evidence; repeat its run link in each row.
- **Test:** automated tests required for the platform or package passed. Name omitted suites and
  explain why they do not apply.
- **Gates:** all required blocking CI, packaging, provenance, and approval gates passed. Record
  workflow run links and source commit. Coverage is required to be run and recorded, but its
  current CI status is warning-only.
- **Install:** the candidate artifact was installed or deployed to its intended target. For web,
  this means the candidate deployment completed and the live assets were checked.
- **Install test:** a clean target launch passed. Verify first-start notebook provisioning, normal
  app behavior, and data preservation. A build or package-content inspection alone is not an
  install test.
- **Uploaded:** the exact candidate artifact is present in the intended workflow artifact store or
  release draft. Record the URL and hash. Uploading an artifact does not mean it is public or ready.
- **Ready:** derived status. Mark `PASS` only when every required column in that row is `PASS`,
  evidence refers to the same source commit and version, and no blocking release issue remains.

### Status values

Use only these values in matrix cells:

| Value | Meaning |
|---|---|
| `NOT RUN` | No result has been recorded. This is the initial value. |
| `PASS` | The named check ran against the candidate and its evidence is linked. |
| `FAIL` | A check ran and the candidate did not meet it. Record the failure and follow-up. |
| `BLOCKED` | The check could not run. Name the blocker, owner, and next action. |
| `N/A` | The target or check is explicitly outside the approved release scope. Record who approved the exclusion and why. |

Never use a blank cell, a checkbox, a green icon, a package filename, or an agent statement as
evidence of a pass. A manual check records the target, steps, observed result, tester, and date.

## Evidence record

Keep evidence close to the release record or link to immutable CI runs and artifacts. Capture at
least:

| Evidence field | Record |
|---|---|
| Check and status | Name the exact gate and use one status value above. |
| Source identity | Full commit SHA, branch or tag, and `[workspace.package]` version. |
| Command or workflow | Exact local command or immutable CI workflow/run URL. |
| Target | OS version, architecture, device/emulator, or browser and deployment URL. |
| Artifact | Exact filename or deployed asset URL, SHA-256, and provenance attestation when available. |
| Result | Relevant tool output or concise observed behavior. Link full logs where possible. |
| Failure follow-up | Blocker/issue, owner, next action, and recheck result. |

Evidence from another source commit or version does not verify this candidate. Re-run the affected
check after any change to its source artifact.

## Release checklist

The checkbox tracks workflow progress; fill the evidence/status column with the actual result. Do
not check an item while its required evidence is missing.

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

### Demo provisioning acceptance

Test the installed application on each shipped platform, including a fresh profile and a profile
with user data:

1. On first start, the app installs the managed Welcome notebook only if its folder is absent.
2. When that folder already exists, including when it contains user edits, app start and upgrade do
   not overwrite it.
3. A sentinel sibling notebook remains unchanged.
4. The installed Welcome notebook includes both generated formats and all 13 locales.
5. Record delete/recreate behavior, normal uninstall behavior, and any platform storage limits.

The web target uses the equivalent checks for the hosted build and notebook payload in a clean
browser session. Record any browser storage or static-download behavior separately from desktop or
mobile installation.

## Recommended validation commands

Run the repository gates on the exact candidate commit. Use the project's current release and CI
guides if a command changes.

```text
cargo xtask demo
cargo xtask demo-check
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo xtask layers
cargo xtask assets
cargo xtask docs
cargo xtask text
cargo xtask coverage
cargo xtask ci
```

The coverage CI job is warning-only; its result still belongs in the evidence record. Do not replace
a failed required gate with a manual assertion.

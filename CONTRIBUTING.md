# Contributing to NoteCraft

Thanks for taking the time to improve NoteCraft. This guide starts with the smallest useful local loop and follows a change through tests, review, CI, and release. For project rules and the full skill inventory, see [`AGENTS.md`](AGENTS.md). For setup detail and quality gate definitions, see [`README_DEV.md`](README_DEV.md) and [`SKILLS/nc-devx/SKILL.md`](SKILLS/nc-devx/SKILL.md).

## 1. Get the project running

NoteCraft is a Rust desktop application. Install Rust with rustup and the platform build tools listed by the setup guide. From the repository root:

```bash
# Linux
bash SKILLS/nc-devx/scripts/setup.sh

# macOS
bash SKILLS/nc-devx/scripts/setup.mac.sh

# Windows PowerShell
.\SKILLS\nc-devx\scripts\setup.ps1
```

The setup scripts check or install the pinned toolchain and platform dependencies, enable the Git hooks, and build the development helper. To check a machine without changing it, use `-CheckOnly` on Windows or `CHECK_ONLY=1` with the Unix setup script. See [`README_DEV.md`](README_DEV.md) for the macOS setup variant and manual setup.

Build and launch the app with the bundled sample notebooks:

```bash
cargo run -p notecraft
```

The command-line app is `notecraft-cli`. Run `cargo run -p notecraft-cli -- --help` to see its commands.

## 2. Find the right project guidance

Start from the skill inventory in [`AGENTS.md`](AGENTS.md). A typical change uses `nc-devx` for the workflow, `nc-hl-design` when shaping a feature, the narrowest relevant implementation skill, and `nc-code-rust` for Rust implementation practices. UI work may also need `nc-uiux-testing`; page storage work may need `nc-dom-ser`. Skills state what they own and which neighboring skills handle related concerns.

Before changing behavior, read the relevant architecture or feature documents and tests. Keep the product's plain-file storage, cross-platform behavior, and clean-room constraints in view. For user-facing behavior, write down observable acceptance criteria and add a regression or feature test before or alongside the implementation.

## 3. Make a focused change and get fast feedback

Use a task branch rather than working on `main`. Choose a descriptive prefix such as `feat/`, `fix/`, `docs/`, or `test/`. Keep each change focused and preserve unrelated edits.

While iterating, format and check the affected crate or test:

```bash
cargo fmt --all
cargo check -p <crate>
cargo test -p <crate>
```

For behavior changes, run the focused regression test first. For a UI change, follow [`docs/testing.md`](docs/testing.md) and the `nc-uiux-testing` skill. Do not commit screenshot goldens; the UI harness captures and compares them during the test.

Common project checks include:

```bash
cargo xtask i18n-audit  # translation catalogs and hard-coded UI string coverage
cargo xtask demo-check   # generated demo and HTML/CMGFM consistency
cargo xtask docs         # documentation links
cargo xtask text         # text policy checks
```

Use `cargo xtask demo` only after changing authored demo content under `docs/demo/scripts/content`; generated notebooks under `docs/demo/DemoNotebook` are outputs and should not be hand-edited.

## 4. Run the full local gate

Before requesting review, run the same full quality gate used by the local pre-push hook:

```bash
cargo xtask ci
git diff --check
```

`cargo xtask ci` covers formatting, clippy, workspace tests, and repository checks for layers, assets, documentation, text, and demo parity. Enable hooks once per clone if setup did not do it:

```bash
git config core.hooksPath .githooks
```

The pre-commit gate checks branch naming, staged content, PII, formatting, crate layers, assets, docs, and text. The pre-push gate checks that the branch is current with `origin/main`, scans PII, and runs the full CI gate. Fix a failing check and rerun it; do not bypass hooks.

## 5. Commit and open a pull request

Use a short conventional commit subject, for example:

```text
fix(editor): preserve selection after undo
```

Push your task branch to your fork or to a branch on the official repository, then open a pull request against `main`. Include:

- The behavior or problem being addressed and why.
- The relevant issue or design decision, if one exists.
- The exact focused and full checks run, with results.
- Screenshots or recordings for visible UI changes, and any platform-specific limits.

Keep the pull request narrow and reviewable. Respond to review by updating the branch and rerunning affected checks. The required CI workflow runs formatting, clippy, static checks, demo checks, and builds/tests on Linux, Windows, and macOS. Coverage is reported as a warning and does not block a merge. A maintainer merges after required checks and review are satisfied.

The usual contributor path is:

```mermaid
flowchart LR
    A[Task branch] --> B[Focused checks]
    B --> C[cargo xtask ci]
    C --> D[Push branch]
    D --> E[Open pull request]
    E --> F[GitHub CI]
    F --> G{Checks and review pass?}
    G -- No --> H[Fix and push update]
    H --> F
    G -- Yes --> I[Maintainer merges]
```

**In short:** validate locally, open a PR, fix any CI or review findings, then a maintainer merges.

## 6. Understand CI and delivery

These are the GitHub Actions workflows contributors will see. A green source check does not mean a package or release exists:

1. **CI** (`.github/workflows/ci.yml`) runs on PRs and pushes to `main`: formatting, clippy, static checks, demo checks, and Linux/Windows/macOS build and tests. Coverage is visible and nonblocking.
2. **Deploy website** (`.github/workflows/deploy-website.yml`) runs for website changes on `main`, creates previews for website changes in PRs, and can be run manually to redeploy the current `main` tip.
3. **Packaging** (`.github/workflows/packaging.yml`) is started manually from GitHub Actions. It builds and verifies platform artifacts, then uploads them. It does not publish a release.
4. **Release** (`.github/workflows/release.yml`) is started manually with the ID of a successful Packaging run. It drafts a release using those exact artifacts; it does not rebuild them.
5. **Publication** happens after a maintainer reviews the draft, installation evidence, and release notes. See [`docs/releasing.md`](docs/releasing.md).

```mermaid
flowchart TD
    PR[Pull request] --> CI[CI: checks and platform tests]
    PR -->|website files changed| Preview[Deploy website: preview]
    Main[Push to main] --> CI
    Main -->|website files changed| Site[Deploy website: production]
    ManualSite[Manual Deploy website run] --> Site
    Actions[Manual Packaging run] --> Artifacts[Verified artifacts]
    Artifacts -->|run ID| Release[Manual Release run]
    Release --> Draft[Draft release]
    Draft --> Review[Maintainer review and publish]
```

**In short:** CI checks code, website actions deploy previews or the site, Packaging creates artifacts, and Release drafts from a selected Packaging run.

Contributors should not start packaging or release workflows for an ordinary pull request. Packaging, signing, deployment, and publication are separate from code review and require maintainer coordination.

## 7. Where to ask and what to include

Use the project's GitHub issues for bugs and feature discussion. Before reporting a bug, check whether it is already tracked. Include the NoteCraft version, operating system, reproducible steps, expected and actual behavior, and relevant sanitized logs. Never attach private notebook contents, credentials, personal paths, or unreviewed diagnostic logs.

For OneNote format research, use public specifications and document sources in the research journal. NoteCraft is a clean-room project: do not use Microsoft source code, decompiled binaries, or copied proprietary assets.

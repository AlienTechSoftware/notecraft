---
name: nc-devx
description: 'NoteCraft developer experience. One-time setup, development workflow, common tasks (lint, test, run app, git hooks), and platform-specific scripts. Fast feedback loop for local work.'
license: MIT
metadata:
  version: "0.0.20261101"
  related_skills: [code-assistant, nc-arch, nc-code-rust]
---

# nc-devx - NoteCraft Developer Experience

## Job to be done

Verify NoteCraft developer workflows with fast feedback and intact quality gates across supported environments.

Developer setup, workflow, and automation for local NoteCraft work. The newcomer
summary is [`README_DEV.md`](../../README_DEV.md); this skill is the detailed and
authoritative reference for the gates and CI/CD lanes.

---

## One-Time Setup

### Prerequisites

- **Rust:** [rustup.rs](https://rustup.rs), with the version pinned in `rust-toolchain.toml`
- **Platform tools:** See platform-specific scripts below
- **Git:** For cloning and version control

### Quick Setup (Recommended)

Each setup script is also the environment check. It checks every tool NoteCraft needs, installs
only what is missing, and ends with "All green." or a list of what is not ready (exit status 1).
On a machine that is already set up it changes nothing and reports all OK. Run the one for your
platform from the repository root:

**Windows (PowerShell 5+):**
```powershell
.\SKILLS\nc-devx\scripts\setup.ps1              # check, and install what is missing
.\SKILLS\nc-devx\scripts\setup.ps1 -CheckOnly   # check only, change nothing
.\SKILLS\nc-devx\scripts\setup.ps1 -Packaging   # also .NET SDK and WiX, for the MSI
```
Installing the C++ build tools or the .NET SDK asks for administrator rights (a UAC prompt).

**Linux (bash/zsh):**
```bash
bash SKILLS/nc-devx/scripts/setup.sh
```

For Ubuntu on WSL2, see [`references/wsl.md`](references/wsl.md) for package
installation, build/test commands, and WSLg UI diagnostics.

### Linux release packages

Build a portable Linux tarball from the repository root:

```bash
packaging/linux/package.sh
```

The output is written under `dist/linux/`. Use `--skip-build` to package an
existing release binary, or request optional formats explicitly:

```bash
packaging/linux/package.sh --formats tar
packaging/linux/package.sh --formats appimage
packaging/linux/package.sh --formats deb,rpm
```

AppImage requires `appimagetool`; Debian and RPM packages require `nfpm`.
The script does not install or download those tools. Validate the portable
packaging path with:

```bash
packaging/linux/test.sh
```

**macOS (bash/zsh):**
```bash
bash SKILLS/nc-devx/scripts/setup.mac.sh
```

These scripts:
1. Check, and install if missing, the platform tools: the MSVC C++ build tools on Windows
   (winget), the Xcode Command Line Tools on macOS, a compiler, pkg-config and the window system
   libraries on Linux (apt, dnf or pacman, with sudo)
2. Check, and install if missing, rustup and the pinned Rust with clippy and rustfmt
3. Turn the git hooks on (`core.hooksPath = .githooks`)
4. Build xtask (development helper tool)

### Android development

Android APK builds use the Linux Android builder. On Windows, run it in WSL2 with Ubuntu 24.04.
The Android bootstrap is separate from the desktop setup because it installs the Android SDK and
NDK and asks you to review their licenses.

**Windows PowerShell:**
```powershell
.\SKILLS\nc-devx\scripts\setup.android.ps1 -CheckOnly  # inspect; no changes
.\SKILLS\nc-devx\scripts\setup.android.ps1              # install missing tools in WSL2
```

**Linux or WSL2:**
```bash
bash SKILLS/nc-devx/scripts/setup.android.sh --check-only # inspect; no changes
bash SKILLS/nc-devx/scripts/setup.android.sh              # install missing tools
```

The bootstrap installs the Rust Android target for the exact channel in `rust-toolchain.toml`,
`cargo-ndk`, JDK 21, Android command-line tools, platform-tools (`adb`), API 35, Build Tools
35.0.0, and NDK 27.2.12479018. Set `ARTCRAFT_TARGET_SDK`, `ARTCRAFT_BUILD_TOOLS_VERSION`, or
`ARTCRAFT_NDK_VERSION` to match a different builder configuration. Android SDK packages and
licenses are installed under `$HOME/Android/Sdk` by default. In WSL, keep this SDK on the Linux
filesystem; the setup script refuses an SDK path under `/mnt/` because the mounted Windows drive
can reject Android SDK directory moves.

For the builder checkout, APK build commands, device installation, and log capture, see
[`references/android.md`](references/android.md). The standard `setup.sh` remains the prerequisite
for Linux desktop development and its native build packages.

**Options:**
```bash
# Windows: -SkipRust, -SkipBuild, -Packaging, -CheckOnly
.\SKILLS\nc-devx\scripts\setup.ps1 -SkipRust

# Linux: SKIP_RUST=1, SKIP_DEPS=1, SKIP_BUILD=1, CHECK_ONLY=1
CHECK_ONLY=1 bash SKILLS/nc-devx/scripts/setup.sh

# macOS: SKIP_RUST=1, SKIP_XCODE=1, SKIP_BUILD=1, CHECK_ONLY=1
CHECK_ONLY=1 bash SKILLS/nc-devx/scripts/setup.mac.sh
```

### Manual Setup

If you prefer to set up step-by-step:

```bash
# 1. Enable git hooks (once per clone)
git config core.hooksPath .githooks

# 2. Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"  # On Linux/macOS

# 3. Install the Rust pinned in rust-toolchain.toml
rustup toolchain install 1.99.1 --profile minimal --component clippy --component rustfmt

# 4. Build the dev helper tool
cargo build -p xtask

# 5. Verify the setup
cargo build -p notecraft
cargo test --workspace --lib
```

---

## Development Workflow

### Running the App

**Quick start (debug build, sample notebooks):**
```bash
cargo run -p notecraft
```

**Release build (optimized):**
```bash
cargo run --release -p notecraft
```

**With custom notebook folder:**
```bash
cargo run -p notecraft -- --notebooks /path/to/notebooks
```

**With bundled demo (after copy):**
```bash
cargo xtask demo-copy html
cargo run -p notecraft -- --notebooks ./my-notebooks --expand-all
```

### OCR test and label lab

`tools/ocr-lab` is a developer tool for OCR. It is never shipped and is not a default workspace
member, so a bare `cargo build` and the release do not build it. `cargo xtask` gates and the CI use
`--workspace`, so they do build and test it. It runs the product's own OCR code
(`notecraft-ocr`), so what it shows is what the app would read.

- **Start it:** `run.cmd --ocr-lab`. Extra flags pass through: `--folder <dir>` for the test
  pictures, `--models <dir>` for the model folder. Without flags it reopens the last folder and
  models. `cargo run -p notecraft-ocr-lab` does the same without the script.
- **Headless:** `--check --folder <dir> --models <dir>` reads every picture, prints one line each
  and exits 1 if any picture fails. Use it in scripts; it opens no window.
- **Models:** the header light is green when both ONNX files are present and match their checksum,
  amber when one is missing, red when one is wrong, grey when no folder is set. Hover it for the
  reason. **Download models** fetches the two public `ocrs` files (about 12 MB) into
  `tools/ocr-lab/local/models`, checks them, then loads them. Nothing downloads until someone
  presses the button. The weights are CC BY-SA 4.0 (see the OCR skill).
- **Cases:** a folder of pictures, grouped in the navigator by kind (the first folder) and language
  (folder names). Labels are drawn in the window and saved beside each picture as
  `<picture>.label.json`. The result is saved as `<picture>.ocr.json` (the one OCR file, see nc-info-ocr), and a changed picture ignores
  its old run. Each case folder keeps a `SOURCES.txt` with where each picture came from and its
  licence.
- **Local, never committed:** `tools/ocr-lab/local/` holds the settings, the log, the reports and
  the downloaded models. It is git-ignored.
- **Checks:** `cargo test -p notecraft-ocr-lab` covers scoring, label files, the grouping rules,
  the model check and the saved-run round trip.
- **Full guide:** `tools/ocr-lab/README.md` (window layout, labels, picture sliders, runs, where
  files live, checks). The OCR design and engine facts are in the `nc-info-ocr` skill.
- **Open feature requests (label `OCR`):** #489 insert and draw your own test pictures under each
  group; #490 show loaded model name, size, capabilities and source in the details pane.

### Local diagnostics paths

`cargo run -p notecraft` from a development checkout normally uses
`local/settings.toml`. The app log is `local/notecraft.log` (and may rotate to
`local/notecraft.log.old`); telemetry session files are under
`local/telemetry/<session-id>/`. The Options page's Folder value points to the
telemetry directory, not to `notecraft.log`.

An installed Windows build normally uses `%APPDATA%\\NoteCraft\\settings.toml`,
with its app log and telemetry directory beside that settings file. These are
defaults, not profile guarantees: `NOTECRAFT_SETTINGS` overrides the settings
path, and a release binary launched from a checkout may use `local/settings.toml`.
When diagnosing, copy the path shown by the running app or verify its resolved
settings path before searching. See
[`nc-runtime-diagnostics`](../nc-runtime-diagnostics/SKILL.md) for the full
precedence and platform paths.

### Running Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p notecraft-formats

# With output
cargo test --workspace -- --nocapture

# Run a specific test
cargo test test_icon_round_trip -- --exact
```

### Linting and Formatting

**Format check (pre-commit runs this):**
```bash
cargo fmt --all -- --check
```

**Auto-format:**
```bash
cargo fmt --all
```

**Clippy lint (catches style issues):**
```bash
cargo clippy --workspace
```

**Full pre-commit gate (before you commit):**
```bash
cargo xtask ci
```

### Common Commands

| Task | Command |
|------|---------|
| Build all | `cargo build --workspace` |
| Build release | `cargo build --release` |
| Run full CI gate | `cargo xtask ci` |
| Regenerate demo notebook | `cargo xtask demo` |
| Check demo parity | `cargo xtask demo-check` |
| Rebuild docs/images | `cargo xtask screenshots` (capture mechanics: [`nc-uiux-testing`](../nc-uiux-testing/SKILL.md)) |
| Check text style | `cargo xtask text` |
| Verify layers | `cargo xtask layers` |
| Check assets/ATTRIBUTION.md | `cargo xtask assets` |

---

## Quality Levels

Each level includes the ones before it. L1 runs automatically from the commit hooks once
`git config core.hooksPath .githooks` is set (the setup script does it). Run `cargo xtask
pre-push` manually before pushing; its fast hook checks that validation is still current. GitHub
PR CI runs L3; L4 is the release signoff and delivery process.

| Level | When | What runs | Where | Time |
|---|---|---|---|---|
| **L0** | After each task | `cargo fmt`, then clippy and the tests of the crates you changed: `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo test -p <crate>` | by hand | seconds |
| **L1** | Before a commit | `cargo xtask check-branch`: not on `main`; a branch name starting `feat/ fix/ docs/ test/ chore/ refactor/ perf/ ci/ build/ release/` (a warning otherwise); nothing from `local/`, `target/`, `my-notebooks/` or a `.log` staged; no staged file over 10 MB; a warning when behind `origin/main`. Then PII, fmt, layers, assets, docs links and text. The message: `type(scope): what`, at most 72 characters | `.githooks/pre-commit`, `.githooks/commit-msg` | under 5 s |
| **L2** | Before a push | Run `cargo xtask pre-push` before `git push`: fetch current `main`, check PII against `origin/main`, run full `cargo xtask ci`, then record the result for the clean commit and base. The fast `.githooks/pre-push` hook fetches again and verifies the record still matches; it refuses stale or missing validation. Merge commits are allowed. | `cargo xtask pre-push`, `.githooks/pre-push` | Full gate varies by machine/cache; hook is seconds |
| **L3** | Pull request | Code quality and cross-platform source validation: split fmt, clippy, static checks, demo checks, tests, and non-blocking coverage, plus build/test jobs for Linux, Windows, and macOS. No installers or publishing. | `.github/workflows/ci.yml`, `.github/pull_request_template.md` | CI |
| **L4** | Release | Release signoff, packaging, installer validation, release notes, and deployment approval. Packaging and deployment are separate workflow concerns; neither is a shortcut around L3. | `.github/workflows/packaging.yml`, `.github/workflows/release.yml`, `docs/releasing.md` | release/manual |

### CI lanes: quality, build, packaging, deployment

The four quality levels above are developer gates. The CI delivery lanes are a separate model that
prevents a slow or privileged packaging step from hiding a source-quality or cross-platform build
failure. New contributors should know which lane they are changing:

**Budget constraint:** NoteCraft currently has no budget for CI, signing, notarization, or larger
hosted runners. Prefer GitHub-hosted free-tier capacity and existing local/VM validation. Keep full
packaging manual or release-only, do not select paid/larger runners, and do not introduce paid
services or signing infrastructure without explicit owner approval.

| Lane | Purpose | Trigger | Produces | Merge/deployment authority |
|---|---|---|---|---|
| **1. Code quality** | fmt, clippy, static checks, docs/assets/demo checks, and coverage | Every pull request and main push as configured | Check results and diagnostics | Required quality checks; coverage remains warning-only |
| **2. Platform build/test** | Prove the source builds and applicable tests pass on Linux, Windows, and macOS | Every pull request | Per-platform logs and test results | Required for merge; no installers |
| **3. Packaging** | Build and inspect MSI/ZIP, Linux packages, and macOS artifacts | Manual `Packaging` workflow only | Versioned, checksumed workflow artifacts | Never publishes by itself |
| **4. Installation/deployment** | Install/uninstall, first launch, notebook placement, upgrade/data persistence, then publish | Manual `Release` workflow after selecting a successful Packaging run | Installation evidence and an approved release | Only this lane may deploy or publish |

Rules:

- Lanes 1 and 2 must stay fast, visible, and safe to run on untrusted pull requests.
- Lane 3 is isolated because packaging tools, larger artifacts, and platform-specific setup are
  expensive and more privileged than ordinary CI.
- Lane 4 is isolated because it changes external state or publishes artifacts; it requires explicit
  release authorization and must not be inferred from a green build.
- A packaging success does not replace the three-platform build/test matrix. A green build does not
  prove that an installer places files correctly.
- Record runner label, Rust/toolchain version, artifact name, architecture, demo payload, notices,
  and known limitations in packaging results.
- The `Packaging` workflow must finish successfully before `Release` is run. Pass its run ID to
  `Release`; the release workflow downloads those exact artifacts instead of rebuilding them.

### Linear history is a policy choice

The L2 push gate now permits merge commits. A linear history can keep branch logs simple and can
help automated changelog generation and bisecting, but it is not a correctness or release-safety
requirement. A merge commit can preserve
the true integration point, the source branch provenance, and the conflict resolution that was
reviewed.

Consider allowing merge commits on `release/*` or other integration branches when preserving that
provenance is more valuable than a straight-line log. If the rule is relaxed, keep the substantive
gates unchanged: PII scanning, formatting, clippy, the complete test suite, static checks, demo
parity, and release signoff. Do not bypass hooks or weaken those gates merely to preserve a merge
commit.

Coverage is not in a hook: it rebuilds the workspace with instrumentation, which takes
minutes. Run `cargo xtask coverage` on demand before opening a pull request that adds code or
tests; CI runs it on pull requests and after pushes to `main`, but it is non-blocking because it
repeats the full instrumented build.

### CI resource and duplication policy

The GitHub workflow keeps the expensive checks separate so failures identify the responsible
phase. Rust jobs set `CARGO_BUILD_JOBS=2` and reduce dev/test debug information to lower linker
memory and disk pressure. Heavy jobs print CPU count, memory, and disk capacity before building.
The test job may retry only errors that identify runner infrastructure pressure (`No space left
on device`, linker I/O failure, `Bus error`, or signal 7); ordinary compiler and test failures are
never retried.

Coverage remains visible on pull requests so developers can think about coverage before merge;
its warning-only result must not block merging. Do not select a larger runner unless the
repository's GitHub account explicitly provides that runner label; record the measured resource
need and the cost/availability decision in the PR.

### Developer workflow timings

`cargo xtask ci` and `cargo xtask test` record validation phase durations locally under
`target/devx/timings.jsonl`. The platform build/test workflow also records its build and test
steps in the GitHub Actions summary. Local history is not uploaded. CI compares only its own
platform and Rust toolchain history, restored from the main-branch Actions cache; pull request
runs do not update that history. See [`docs/devx-performance.md`](../../docs/devx-performance.md)
for the comparison policy and command details.

Never skip a level with `--no-verify` for your own work (AGENTS.md). If a hook stops you on
work that is not yours, such as another session's half-written file, say so in the report.

---

## Test Coverage

The baseline is [`docs/coverage.json`](../../docs/coverage.json). `cargo xtask coverage`
compares a new run with it and warns when a figure falls by more than 0.5 points; it never
fails. CI does the same on every pull request, as a job that cannot block a merge. Raise the
baseline with `cargo xtask coverage --update` in the change that raised coverage. Details:
[`docs/testing.md`](../../docs/testing.md#coverage).

---

## Personally Identifiable Information (PII) Scanner

NoteCraft checks for sensitive data before commits and pushes to prevent accidental leaks.

### What's Checked
- Owner name and email patterns
- Notebook file extensions (should not be in repo)
- SSN-like patterns (###-##-####)
- Custom patterns per project

### Manual Scan
```bash
# Scan current staged files
bash SKILLS/nc-devx/scripts/check-pii.sh

# Scan all files
bash SKILLS/nc-devx/scripts/check-pii.sh --all
```

### Fixing PII Issues

If the scanner blocks a commit or push:

1. **Identify the issue:**
   ```bash
   # The hook will show which files contain what patterns
   # Edit those files and remove the sensitive data
   ```

2. **Common fixes:**
   - Remove owner name/email from commits: Use `git commit --author` or `git config user.name`
   - Remove `.one` files: `git rm --cached notebook.one`
   - Remove personal data from comments: Edit the file and remove it
   - Update email references: Replace with generic placeholder

3. **Re-stage and commit:**
   ```bash
   git add <file>
   git commit  # Re-run pre-commit hook
   ```

4. **If it's in commit history:**
   ```bash
   # Amend the previous commit if not yet pushed
   git commit --amend
   
   # Or rebase to fix multiple commits
   git rebase -i HEAD~N  # Replace N with number of commits to go back
   ```

### Customize PII Patterns

Edit `SKILLS/nc-devx/scripts/check-pii.sh` to add project-specific patterns:

```bash
PII_PATTERNS=(
    "person@domain"            # Actual email to block (uncomment and update)
    "sensitive-word"           # Add your patterns here
)
```

### Cleaning PII from Git History

If PII was already committed (e.g., in commit author fields), the scanner will catch it in *files* but not in git metadata. To clean git history:

**Option 1: Rewrite history (careful - affects all contributors)**
```bash
# For local-only branches (not yet pushed):
git rebase -i HEAD~N  # Go back N commits, manually edit or squash

# For published branches, use git filter-repo:
# Install: pip install git-filter-repo
git filter-repo --replace-text patterns.txt  # See git-filter-repo docs
```

**Option 2: Accept and prevent future**
- Leave historical commits as-is (no public harm once committed)
- Prevent new commits with the scanner
- Update `.gitconfig` to use a safe author email

**Recommended:**
```bash
# Set per-project safe author
git config user.email "dev@company.com"
git config user.name "Developer"

# Verify
git config user.email
```

---

## Git LFS (large sample files)

Two OneNote sample files are stored in Git LFS, and nothing else is:

- `crates/import/tests/fixtures/3p/onenote.rs/samples/Large Desktop.one`
- `crates/import/tests/fixtures/3p/onenote.rs/samples/Large OneDrive.one`

The rule is in `.gitattributes` (`Large*.one filter=lfs ...`).

### Why these files use LFS

The two files are 113 MB and 161 MB, and GitHub rejects plain git objects that large. LFS keeps the
real bytes on GitHub's LFS server and leaves a 134-byte pointer in git. Without LFS the files could
not be committed at all. They are OneNote samples from the onenote.rs corpus, used only by manual
import tests. No build, test or CI run reads them, so a normal clone skips them on purpose.

### One-time setup

1. Install Git LFS: `winget install GitHub.GitLFS` on Windows, `brew install git-lfs` on macOS, or
   your package manager on Linux. Check it with `git lfs version`.
2. Run `git lfs install` once per machine. It adds the LFS filters to your global git config.

### Daily use

- A normal clone or pull gives you the two files as pointers. That is expected. The import tests
  do not need them.
- To get the real files for a manual import test, run:

  ```
  git lfs pull --include="crates/import/tests/fixtures/3p/onenote.rs/samples/Large*.one" --exclude=""
  ```

  The empty `--exclude` is required, because `.lfsconfig` skips these files by default.
- To put the real files back as pointers in your working tree, run
  `git checkout -- <file>` for each of the two files. This does not change the repository.
- Before you commit a change to one of these files, run `git status`. Git LFS should show the file
  as modified only when its content really changed. If it shows an unexpected change, stop and check.

### Pushing with LFS

- When a push includes LFS objects that the remote does not have, Git LFS uploads them first. If
  the local objects are missing, the push fails with `missing or corrupt local objects`. This
  happened to the nightly on 20 November 2026.
- To upload the two samples yourself, you need a copy of the full file in the working tree and write
  access to the repository. Stage the file with `git add` so Git LFS stores it locally, then run
  `git lfs push --all origin`, then unstage with `git restore --staged`. Do not commit the file.
- Only a full-size file can be uploaded. If the local file does not match the pointer's hash, Git
  LFS refuses it. Check the hash against `SHA256SUMS.txt` in the corpus folder first.

### CI and the nightly

- CI does not fetch LFS objects. Nothing on `main` sets `lfs` on a checkout. The nightly's push of
  its version bump is the one place that needs them. Its fix is pending in PR #425, which either
  fetches the objects or uses the push flag `lfs.allowincompletepush`. Read that PR's decision
  before you change the nightly's checkout.
- Any new workflow that checks out the repository and pushes must handle LFS the same way.
- `cargo xtask pre-push` does not check LFS. If a push is refused for LFS reasons, run
  `git lfs fetch --all` and `git lfs pull`, then push again.

### Troubleshooting

- **The two files are only 134-byte text files.** Run the `git lfs pull` command above.
- **`git lfs` is not found.** Install Git LFS, then run `git lfs install` again.
- **Push refused: `missing or corrupt local objects`.** Run `git lfs fetch --all`, then
  `git lfs pull`. If the file was never on this machine in full, get it from a copy of the full
  file, then push again.
- **Upload refused or auth fails.** You need write access to the repository. Do not put
  credentials in chat or in the repository.
- An LFS object that was never pushed cannot be fetched from the remote. If it is lost locally
  before its first push, nothing on the remote can restore it: recover it from another copy of the
  working tree, or re-add the file.

## Git Hooks

Hooks are in `.githooks/` and run automatically after `git config core.hooksPath .githooks`.

### Pre-commit Hook (runs every commit)
**File:** `.githooks/pre-commit`  
**Speed:** ~2 seconds  
**Checks:**
- `cargo fmt --check` (code style)
- `cargo xtask layers` (crate boundaries)
- `cargo xtask assets` (ATTRIBUTION.md)
- `cargo xtask docs` (link validity)
- `cargo xtask text` (plain ASCII, no fancy quotes)

**If it blocks:**
- Fix the issue and re-stage the files
- Or use `--no-verify` as a last resort (and fix the underlying issue)

### Push Validation
Run `cargo xtask pre-push` before `git push`. It checks branch freshness, scans for PII, and runs
the full `cargo xtask ci` while no remote transport is open. On success, it records the current
commit and `origin/main` in Git metadata. The `.githooks/pre-push` hook fetches `origin/main` again
and checks that the working tree is clean and both hashes still match. Missing or stale validation
refuses the push with the command to rerun. If `origin/main` advances during CI, rerun the command.

**If it blocks:**
- Fix the issue locally and rerun `cargo xtask pre-push` before pushing
- Then push

---

## Troubleshooting

### "cargo not found"
- Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- Source your shell: `source ~/.cargo/env` (on Unix-like systems)

### "xtask not found"
- Run: `cargo build -p xtask`
- Xtask is a dev helper built on first use

### Pre-commit hook blocks on unrelated files
- Run `cargo xtask text` to see which files have issues
- Fix them, or use `git commit --no-verify` if they're pre-existing

### Slow build times
- Debug builds are optimized for speed; release builds take longer
- Use `cargo check` to verify without building binaries
- Use `cargo build -j 1` if you're out of memory

### Sub-repository not found
- Run the platform setup script again, or
- Manually update: `git submodule update --init --recursive`

---

## Platform-Specific Notes

### Windows
- **Setup script:** `SKILLS/nc-devx/scripts/setup.ps1`
- **Android setup:** `SKILLS/nc-devx/scripts/setup.android.ps1` (runs in WSL2)
- **Required tools:** Visual Studio Build Tools or MSVC, WiX (for installer builds)
- **Shell:** PowerShell recommended; Git Bash also works
- **Git hooks:** Use `CRLF` line endings (git config auto-handles)

### Linux
- **Setup script:** `SKILLS/nc-devx/scripts/setup.sh`
- **Android setup:** `bash SKILLS/nc-devx/scripts/setup.android.sh`
- **Required tools:** gcc/clang, pkg-config
- **Shell:** bash/zsh
- **Notes:** Some features (e.g., native dialogs) use X11 or Wayland

### macOS
- **Setup script:** `SKILLS/nc-devx/scripts/setup.mac.sh`
- **Required tools:** Xcode Command Line Tools, cargo-watch (optional)
- **Shell:** zsh (default in modern macOS) or bash
- **Notes:** Use `mac` in platform filenames (not `osx`)

---

## Quick Feedback Loop

### TDD workflow for UI and behavior changes

From version 0.0.7 onward, use this order for new UI behavior and user-facing
features:

1. Record the requirement and observable acceptance criteria.
2. Add focused tests for the criteria before changing production code.
3. Run the new tests and confirm they fail for the expected missing behavior.
4. Implement the behavior, keeping the change scoped to the contract.
5. Run the focused tests, then the changed-crate checks and the applicable
   quality level.

Use the real `egui_kittest` harness for pointer, keyboard, accessibility, and
other user-visible UI behavior. Use lower-level tests for pure geometry or model
rules, but do not use them as a substitute when the interaction itself is part
of the requirement. Every fixed, reproducible bug should gain a regression test.

For rapid iteration during development:

```bash
# Terminal 1: Watch and rebuild on changes
cargo watch -x 'check -p notecraft'

# Terminal 2: Run the app
cargo run -p notecraft

# Terminal 3: Run tests as you change them
cargo watch -x 'test -p notecraft-formats'
```

Or use your IDE's built-in build/test runner if available.

---

## Submitting Work

Before submitting a PR:

1. Verify git hooks are enabled: `git config core.hooksPath` should show `.githooks`
2. Fix any failures and commit with a clear message
3. Run `cargo xtask pre-push` after committing; it runs the full gate on the exact clean commit
4. Push and open the PR; the pre-push hook checks that validation is still current

See [AGENTS.md](../../AGENTS.md) for submission guidelines.

---

## References

- [AGENTS.md](../../AGENTS.md) - Rules, commands, and gates
- [Rust coding standards](nc-code-rust) - Code style, testing, error handling
- [Architecture](nc-arch) - System design and crate organization
- [Gaps and roadmap](../../docs/gaps.md) - Known limitations and future work

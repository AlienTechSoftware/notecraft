# NoteCraft developer guide

This is the short entry point for contributors. The detailed, authoritative CI/CD
rules live in [`SKILLS/nc-devx/SKILL.md`](SKILLS/nc-devx/SKILL.md). Read that file
when changing build scripts, workflows, packaging, or release behavior.

New contributors can follow the step-by-step [`CONTRIBUTING.md`](CONTRIBUTING.md).

Android APK development is an optional Linux/WSL2 lane. Bootstrap its Rust target, JDK, Android
SDK/NDK, and ADB with [`setup.android.sh`](SKILLS/nc-devx/scripts/setup.android.sh); Windows users
can launch it through [`setup.android.ps1`](SKILLS/nc-devx/scripts/setup.android.ps1). See the
[`nc-devx Android guide`](SKILLS/nc-devx/references/android.md) for APK and device checks.

## Fork and clone

```bash
# Fork AlienTechSoftware/notecraft on GitHub first, then clone your fork:
git clone https://github.com/<your-username>/notecraft.git
cd notecraft
git remote add upstream https://github.com/AlienTechSoftware/notecraft.git
git fetch upstream
```

Contributors open pull requests from their fork against `main` in this repository.
Work on a branch, not `main` (see "First day" below).

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

## Start a coding agent session

Claude Code loads `CLAUDE.md` automatically, and Codex reads `AGENTS.md`
automatically, when started in the repository folder. Both files point at the
rules in `AGENTS.md`. Start the agent in the repo folder (`claude` or `codex`),
then use this as the first prompt, with the task filled in:

```text
Read AGENTS.md, README_DEV.md, and CONTRIBUTING.md first, and follow them.
Then work on this task: [describe the task and the issue number, if any].

Plan before implementing: share scope, acceptance criteria, likely files,
risks, and verification steps, and wait for my CHECKPOINT nod before writing code.
Work on branch [type/name]. Never use --no-verify, never push, merge, or
publish without my explicit go. Before claiming completion, run cargo xtask ci
and the focused tests for the change, and report the exact commands and results.
```

## OCR (optional)

OCR is not needed to build or run the app. To work on it:

- `cargo xtask ocr-check` runs the OCR lab's tests, and the headless read of the smoke pictures
  when the models are set (`NOTECRAFT_OCR_MODELS`, the folder with the two ONNX files).
- `.\SKILLS\nc-devx\scripts\setup.ps1 -OcrModels` fetches the models into
  `tools\ocr-lab\local\models` and checks them. They are about 12 MB and their weights are
  CC BY-SA 4.0, so the fetch is opt-in.
- `run.cmd --ocr-lab` opens the OCR test and label lab, with its own guide and screenshots in
  [`tools/ocr-lab/README.md`](tools/ocr-lab/README.md). Its header light shows whether the models
  are present and intact. Its settings, log, reports and downloaded models stay under
  `tools\ocr-lab\local\`, which is never committed. Saved runs (`<picture>.ocr-run.json`) and labels
  (`<picture>.label.json`) are written beside each picture; the fixtures folder ignores them in git.

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

# Agents contract: NoteCraft

Rules for AI agents working in this folder. The project-wide contract is one level up
(`p-NoteCraft/AGENTS.md`); this file adds what is specific to the code.

## Shared agent workflow

- **Understand first.** Read the host contract and this file, inspect status and
  branch, then read relevant code, design docs, tests and existing work notes.
  State the issue and likely cause before changing code.
- **Plan before implementation.** Share scope, acceptance criteria, likely
  files, risks and verification. For substantial work, include the estimate
  required by the host guide and wait for the owner's CHECKPOINT nod.
- **Track work.** NoteCraft planning, progress, decisions, handoffs and
  remaining work belong in the private host repo's `work/` folder. Add to
  existing notes without replacing another session's work. Keep public repo
  issues and pull requests clear enough for external review.
- **Managed checkout worktrees are prohibited for automated agents.** Do not
  create, use, move, lock or remove linked Git worktrees in these managed
  checkouts, and do not invoke a mode that creates one silently. Work in the
  normal repository folders. This rule applies to automated agents in the
  managed workspace only. External contributors may use their own forks,
  clones, branches and worktrees.
- **Branches and user work.** Follow the branch the owner names. Otherwise use
  one branch for one feature and keep one active branch at a time. Check status
  before switching. If Git reports a branch checked out elsewhere, stop,
  inspect and preserve that work, and report the path. Never force a checkout,
  delete a folder, reset, clean or stash unrelated changes. Stage only files
  changed for this task.
- **Keep repositories distinct.** Planning and discussion belong in the
  private host; NoteCraft implementation and its design docs belong here.
  Other repositories in the workspace remain separate projects.
- **Quality and delivery.** Run relevant repository checks before committing
  or claiming completion. Fix failures or report them clearly. Commit
  substantial tested work in coherent chunks when authorized. Do not push,
  merge, publish or release without explicit authorization.
- **External contribution path.** Contributors may use their own fork or clone
  and submit a pull request to this repository. Pull requests are reviewed
  against the relevant design docs and tests; run `cargo xtask ci` and focused
  checks for changed behavior before requesting review.
- **Completion report.** State the branch, files changed, checks and results,
  commits, and remaining work. Do not report unverified work as complete.

## What this is

NoteCraft is a pure-Rust, clean-room note taking app. Notebooks, section groups,
sections and pages are plain folders. See [`docs/architecture.md`](docs/architecture.md).

## Harmony, reuse and platforms

- Prioritize compatibility with established interaction patterns and
  conventions in related suite apps where they fit NoteCraft's requirements.
  Inspect available references and shared libraries before implementing a new
  capability. Prefer reusable shared code over one-off duplication when the
  dependency and repository boundaries allow it. Keep app-specific behavior at
  clear edges.
- Keep shared contracts and behavior platform-neutral while using useful native
  platform support through isolated implementations. Name platform-specific
  files `<feature>.<platform>.<extension>`: `.win.*` for Windows, `.mac.*` for
  macOS and `.lnx.*` for Linux. Rust must select these files explicitly through
  `cfg`-gated module wiring; the suffix alone does not select a file. Preserve a
  shared implementation or fallback for platforms without a specialized one.

## Bundled notebook and browser playground

- The Welcome notebook's authored content lives in `docs/demo/scripts/content`.
  `cargo xtask demo` generates `docs/demo/DemoNotebook`, and
  `crates/engine/build.rs` embeds that generated folder into the app. Edit the
  authored content, regenerate the notebook, and use `cargo xtask demo-check`
  to confirm the checked-in bundle matches its source. Inspect Git status first
  and preserve unrelated edits to the source or generated notebook.
- **HARD RULE: `docs/demo/DemoNotebook/html` and
  `docs/demo/DemoNotebook/cmgfm` MUST remain identical in notebook structure,
  pages, languages, translations, metadata, links, assets and feature examples.
  The only permitted difference is the serialized page file format and its
  required format declaration.** At the same viewport, zoom, theme and app
  settings, both notebooks MUST render nearly pixel-for-pixel alike. Every
  demo or rendering change must pass
  `cargo xtask demo-check` and
  `cargo test -p notecraft-formats --test format_parity`, plus a visual
  side-by-side check at matching settings. The automated parity test compares
  page metadata, shared files and semantic page blocks; it does not compare
  rendered pixels, so record visual verification separately.
- The owner's OpenPlayground fork is checked out at `../openplayground` from
  this repository. Use it to inspect browser deployment and integration.
  `tools.yaml` currently identifies NoteCraft as slug `onenote`, builds from
  `AlienTechSoftware/notecraft` on the head channel, watches
  `docs/demo/DemoNotebook` among its rebuild paths, and applies
  `patches/onenote.yaml` for the web build. Regenerate the notebook after
  content changes so the watched output is current. For browser-only behavior,
  inspect the patch rules and coordinate changes in the OpenPlayground repo
  with the upstream NoteCraft source they target. Check status in both repos
  before editing; never push or publish without an explicit GO.

## Commands

```bash
cargo xtask ci              # the full gate
cargo test --workspace
cargo xtask screenshots     # rebuild docs/images
cargo xtask demo            # rebuild docs/demo/DemoNotebook from docs/demo/scripts/content

git config core.hooksPath .githooks   # once per clone: turn the hooks on
```

## Rules

- **Layers.** A crate depends only on lower layers. `cargo xtask layers` enforces it. Only
  `ui-egui` and the apps may use egui, eframe, winit or rfd.
- **Never crash.** No `unwrap`, `expect`, `panic`, `todo`, `unimplemented` or `unreachable`
  outside tests; the lints deny them. No `unsafe`.
- **Everything is a command.** A new user action is a `CommandSpec` in the engine, with a test.
  Refresh `docs/commands.md` with `UPDATE_DOCS=1 cargo test -p notecraft-engine commands_doc`.
- **UI tests** follow the method in [`docs/testing.md`](docs/testing.md): paired comparisons
  and the control channel. Do not commit golden images.
- **Themes.** The UI uses theme tokens only. Contrast tests must pass for all 14 themes.
- **Assets.** Every file in `assets/` or `docs/images/` needs a row in `ATTRIBUTION.md`.
- **Text style.** Plain ASCII, no em dashes, short sentences. `cargo xtask text` checks it.
- **Honesty.** Mark what is verified and what is assumed. Do not claim coverage, platform
  support or licences that were not checked. Keep `docs/gaps.md` true.
- **Personal data.** No owner name, email or real notebook names in tracked files. Never copy
  the owner's `.one` files into the repository.
- **Clean room.** Use only Microsoft's published specifications for the `.one` format.
- **Versions.** The version is 0.0.2, and it changes only when the owner says so, in words, for that change. No agent bumps it, runs `cargo xtask version set`, tags a release or starts a release with a version override on its own judgement, and an earlier approval does not cover the next bump. Semantic versioning, `MAJOR.MINOR.PATCH`, as the sibling Craft apps do. Still 0.x. A patch (0.1.0 to 0.1.1) is bug fixes only, or a rebuild after a packaging or workflow fix, and never changes the folder format. A minor (0.1.x to 0.2.0) adds features or changes behaviour, and resets the patch to 0. A pre-release adds a tag, `0.2.0-rc.1`. The version lives only in `[workspace.package]`; change it with `cargo xtask version set`. Tags are `v<version>`. Details in [`docs/releasing.md`](docs/releasing.md).
- **Page model.** Read [`docs/page-model.md`](docs/page-model.md) before changing how a page is stored, edited on screen or shown as code. The page file names everything the page owns, and what the window shows and what the folder holds must never disagree. A new kind of page content is named in the page file, has a reader and writer in `notecraft_formats`, appears in the code view's list and has a round-trip test.
- **Page representation changes.** Apply [`SKILLS/nc-dom-ser/SKILL.md`](SKILLS/nc-dom-ser/SKILL.md) for changes crossing the page model, HTML/CMGFM, rendering, storage, or editor. `cargo xtask ci` must keep the cross-format and whole-demo parity checks green.
- **One feature at a time.** One feature, one branch. Do not start a second branch, or work on a second feature, unless the owner asks for more than one. A request that arrives while a branch is open is answered or written down and waits; it does not start another branch. Land the branch before the next one begins: rebase it onto `origin/main`, run `cargo xtask ci`, check out `main`, pull it, and fast-forward the branch in, so the history stays a straight line. The owner says when to push. Delete the branch once it is merged.
- **Git.** Do not commit, push or publish without explicit permission.
- **Hooks.** [`.githooks/pre-commit`](.githooks/pre-commit) runs the cheap gates (fmt, layers,
  assets, docs, text), about two seconds. [`.githooks/pre-push`](.githooks/pre-push) runs the
  whole of `cargo xtask ci`, a few minutes, so a red tree is caught before CI sees it. Git does
  not pick up a tracked hooks folder by itself, so each clone needs the `core.hooksPath` line
  above once. They check the working tree, not the index, so another session's half-written file
  will stop your commit; `--no-verify` is the way through that, and it is not a thing to reach
  for otherwise.
- **Installs and downloads** (for example `cargo-llvm-cov`) need explicit permission.

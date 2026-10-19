# Agents contract: NoteCraft

Rules for AI agents working in this folder. The project-wide contract is one level up
(`p-NoteCraft/AGENTS.md`); this file adds what is specific to the code.

## What this is

NoteCraft is a pure-Rust, clean-room note taking app. Notebooks, section groups,
sections and pages are plain folders. See [`docs/architecture.md`](docs/architecture.md).

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
- **Git.** Do not commit, push or publish without explicit permission.
- **Hooks.** [`.githooks/pre-commit`](.githooks/pre-commit) runs the cheap gates (fmt, layers,
  assets, docs, text), about two seconds. [`.githooks/pre-push`](.githooks/pre-push) runs the
  whole of `cargo xtask ci`, a few minutes, so a red tree is caught before CI sees it. Git does
  not pick up a tracked hooks folder by itself, so each clone needs the `core.hooksPath` line
  above once. They check the working tree, not the index, so another session's half-written file
  will stop your commit; `--no-verify` is the way through that, and it is not a thing to reach
  for otherwise.
- **Installs and downloads** (for example `cargo-llvm-cov`) need explicit permission.

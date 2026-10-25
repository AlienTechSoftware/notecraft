---
name: nc-arch-x-plat
description: NoteCraft's cross-platform design rule. Branch the code so every platform uses what it does best; never reduce the app to the features all platforms share. Covers platform boundaries, per-platform backends, missing-feature handling, and how each backend is tested.
license: MIT
metadata:
  version: "0.0.20261025"
  related_skills: [nc-arch, nc-code-rust, nc-uiux-controls]
---

# nc-arch-x-plat - Cross-platform by union

Use this skill whenever code touches something the operating system provides: the clipboard,
files and the shell, file dialogs, opening files and folders, icons, printing, fonts, windows,
notifications, paths, or installers.

## The philosophy: union, not intersection

- **Union.** NoteCraft gives each platform the best behaviour that platform offers. A feature that
  Windows does well is built for Windows, even if macOS or Linux do it differently or not yet.
- **Not intersection.** We do not shrink the app to the subset that every platform supports. That
  would make the Windows app worse than it could be, and it hides what each platform can do.
- **Shared meaning, native means.** The user-visible behaviour is defined once, in the shared core.
  How it is achieved is native to each platform. Both the Windows Shell icon for an attachment and
  a macOS or Linux equivalent answer the same question, "what does this file look like?"

## Rules

1. **The core is platform-free.** Engine, store, model and formats do not name an operating system.
   They call a boundary: a small trait or function that describes what the app needs.
2. **Each platform is a branch behind the boundary.** Platform code lives in files named
   `<feature>.<platform>.<extension>` (`.win.*`, `.mac.*`, `.lnx.*`) and is selected explicitly
   with `cfg`-gated module wiring. The suffix alone selects nothing.
   Existing examples:
   - `crates/store/src/assets.win.rs` extracts a Windows Shell icon; `assets.rs` picks it on Windows
     and uses an SVG fallback elsewhere.
   - `open_folder` and `open_file_for_viewer` in `crates/ui-egui/src/app.rs` choose `explorer`, `open` or
     `xdg-open` per platform.
   - The clipboard in `crates/ui-egui/src/clipboard.rs` is the boundary for copy and paste.
3. **Use the native capability.** A platform branch uses the richest API its platform provides. Do
   not write the Windows branch to match the macOS branch, or the reverse. A feature that one
   platform does better gets the better version there.
4. **A fallback is a last resort and it is visible.** A shared fallback is allowed only for a
   platform with no specialised branch yet. It must not silently drop a feature: the UI says what
   is not built on that platform, for example "not built on macOS yet" (the File screen already
   does this for online notebooks).
5. **Missing is a decision, not a default.** When a platform cannot do something, record it in
   `docs/gaps.md` and in the platform's entry in the feature-parity table, with the reason.
6. **Errors come back to the user.** A platform call that fails shows its error in the window. A
   release build on Windows has no console, so `eprintln!` alone is not an error report.

## Testing each platform's branch

- Each backend has its own tests, compiled only for its platform (`#[cfg(...)]`).
- Tests use real data where they can (see the no-mocks rule in `AGENTS.md`). A platform boundary
  may be replaced by a test double, one per backend, starting with Windows.
- Every platform branch that ships is built and checked in CI on its own runner. Today only Windows
  x64 is built; macOS and Linux are untested and are not claimed as supported.
- A new platform backend is not finished until its boundary tests pass on that platform.

## Checklist for a change that touches the OS

1. Name the boundary. Is it a new one, or does an existing one cover it?
2. Write the shared behaviour first, in the core, with real test data.
3. Write each platform branch with the richest native API it has.
4. For a platform without a branch, show what is not built, and add it to `docs/gaps.md`.
5. Test each branch that exists on its own platform, and say which platforms were not run.
6. Update the feature-parity table and the relevant `SKILLS/*` notes.

---
name: nc-arch-x-plat
description: NoteCraft's cross-platform design rule. Branch the code so every platform uses what it does best; never reduce the app to the features all platforms share. Covers platform boundaries, per-platform backends, missing-feature handling, and how each backend is tested.
license: MIT
metadata:
  version: "0.0.20261025"
  related_skills: [nc-arch, nc-code-rust, nc-uiux-controls]
---

# nc-arch-x-plat - Cross-platform by union

## Job to be done

Design platform boundaries that preserve each platform's useful capabilities without leaking platform details into shared contracts.

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
   `<feature>.<platform>.<extension>`: `.win.*` for Windows, `.mac.*` for macOS, `.lnx.*` for
   Linux. Code shared between desktop and Android uses `<feature>.android.<extension>`; code shared
   with iOS uses `<feature>.ios.<extension>`. Code used only on Android lives in `android/` with no
   suffix, and iOS-only code lives in `ios/` the same way. `.droid.*` is retired. Rust selects each
   file explicitly with `cfg`-gated module wiring. The suffix alone selects nothing. For example, a
   module can expose
   a shared contract and select the native implementation like this:

   ```rust
   #[cfg(target_os = "windows")]
   #[path = "clipboard.win.rs"]
   mod platform;

   #[cfg(target_os = "macos")]
   #[path = "clipboard.mac.rs"]
   mod platform;

   #[cfg(target_os = "linux")]
   #[path = "clipboard.lnx.rs"]
   mod platform;

   #[cfg(target_os = "android")]
   #[path = "clipboard.android.rs"]
   mod platform;

   #[cfg(target_os = "ios")]
   #[path = "clipboard.ios.rs"]
   mod platform;
   ```
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

### Path-test portability gotcha

Shared path tests must be platform-neutral even when they describe a Windows, Linux or macOS
user-facing location. Do not compare a joined path with a hard-coded string containing the
separators of one operating system. That makes a valid implementation fail on another runner
(for example, expecting `C:\\Users\\Ada\\Documents\\NoteCraft` while Linux correctly produces
`C:\\Users\\Ada/Documents/NoteCraft` for a Windows-looking fixture).

- Build expected paths with `PathBuf::from(...).join(...)` or the equivalent platform-aware API.
- Keep the semantic assertion identical on every platform: the root is the current user's
  `Documents/NoteCraft` folder.
- Use platform-looking fixture roots only to test parsing or policy; never use their spelling as a
  cross-platform separator expectation.
- Run the focused test on at least Windows and Linux before claiming the path change is green.
  A failure in one shared test can make both the normal test gate and coverage gate red.

This is especially important for profile-relative notebook paths, installer payload paths and
temporary test roots. The implementation may accept an explicit platform override, but the
normal default and its tests must remain portable.

## Checklist for a change that touches the OS

1. Name the boundary. Is it a new one, or does an existing one cover it?
2. Write the shared behaviour first, in the core, with real test data.
3. Write each platform branch with the richest native API it has.
4. For a platform without a branch, show what is not built, and add it to `docs/gaps.md`.
5. Test each branch that exists on its own platform, and say which platforms were not run.
6. For path changes, check both separator behavior and the semantic location on Windows and Linux;
   do not rely on a Windows-only local pass.
7. Update the feature-parity table and the relevant `SKILLS/*` notes.

# NoteCraft 0.0.8 P2 handoff

## Handoff for the next model

### Repository and branch

- Repository: `C:\dev\p-NoteCraft\.repos\o-NoteCraft_2`
- Current branch: `0.0.8_p2s`
- Baseline: `e051df177611e12973e0bd33c6077e409bf392b9`
- Image resize remains excluded because Agent 1 owns that work elsewhere.

### Current worktree changes

The current checkout contains these task changes. Preserve them unless the owner
explicitly asks for a reset:

- `.gitignore`: ignores two malformed local directory names.
- `crates/ui-egui/src/pageedit.rs`: inline images are represented as one
  logical caret unit and unmodified Left/Right movement is intercepted before
  `egui::TextEdit` can select the image replacement character.
- This handoff document.

Do not amend or create commits without owner authorization. Do not push,
merge, delete, reset, clean, or stash unrelated work.

### Latest verified checks

The latest completed checks passed:

- `cargo fmt --all -- --check`
- `cargo clippy -p notecraft-ui-egui --all-targets -- -D warnings`
- `cargo build --release`
- `cargo test -p notecraft-ui-egui --lib pageedit::tests::inline_image`
- `cargo test -p notecraft-ui-egui --test rtl_quality`
- `cargo test -p notecraft-ui-egui --test ui -- --test-threads=1`
- `git diff --check`

The full UI suite result was 123 passed and 0 failed. This is automated
verification only. The owner must still manually verify the visual caret and
selection behavior with `.\run.cmd`.

### Active inline-image behavior

**Detailed issue:** Inline image caret behavior is tracked in the current implementation and regression tests; a standalone issue file is not present in this checkout.

An inline image is an atomic object in the text flow:

- Text before and after the image remains in the same owning paragraph/block.
- The image occupies one logical caret unit, regardless of alt-text length.
- Right Arrow from immediately before the image must move directly to the
  position immediately after it.
- Left Arrow from immediately after the image must move directly to the
  position immediately before it.
- An unmodified arrow must not visibly select the image or make the caret
  disappear.
- Shift+Arrow may intentionally select the image.
- Home, End, PageUp, PageDown, typing, deletion, save, reload, LTR, and RTL
  behavior still require explicit manual and regression coverage.

The current implementation uses `U+FFFC` internally. The original image
Markdown/HTML and alt text must remain intact when the block is written back.

### Manual retest requested by the owner

Run:

```powershell
.\run.cmd
```

Use an inline image with text on both sides. Test:

1. Click immediately before the image.
2. Press Right Arrow once. The caret must appear immediately after the image.
3. Press Left Arrow once. The caret must return immediately before the image.
4. Repeat with Shift+Arrow to verify intentional selection.
5. Type before and after the image.
6. Repeat in an RTL paragraph.
7. Test Home, End, PageUp, PageDown, Delete, and Backspace.

If the caret disappears or the image becomes selected during an unmodified
arrow move, capture the exact key sequence and a screenshot before changing
the implementation. Check the runtime log for the block, image source, caret
offset, selection range, and focus state. Do not infer the cause from a
standalone image block: standalone `Block::Image` objects are a separate
interaction path from inline image spans.

### Malformed-folder incident

The following folder is a real directory in the checkout, not merely a display
artifact:

`C:\dev\p-NoteCraft\.repos\o-NoteCraft_2\Cdevp-NoteCraft.reposo-NoteCraft_2my-notebooks`

It is ignored by the repository rule:

`/.gitignore:18:/Cdevp-NoteCraft.reposo-NoteCraft_2my-notebooks/`

The folder was observed with directory attributes and a creation timestamp
from October 29, 2026. Its contents and creator are not yet established.
Do not delete it automatically. Inspect its contents, active processes, test
fixtures, and command history first. Remove it only after the owner confirms
that it is disposable. Search path-construction code for Windows paths that
are being passed as a literal directory name or incorrectly escaped string.

### Handoff stop conditions

Stop and ask the owner if:

- the manual inline-image test still loses the caret;
- the malformed folder contains files not clearly disposable;
- the intended branch is checked out elsewhere;
- a test or lint gate fails;
- a change would affect image resize, which belongs to Agent 1.

## Issue inventory

Baseline: `e051df177611e12973e0bd33c6077e409bf392b9`

This file captures the 0.0.8 work identified in the Agent 2 audit. Image
resize is intentionally excluded because Agent 1 owns that work in another
folder.

Priority meanings:

- **P0**: release blocker or core workflow failure.
- **P1**: high-value release work or release-confidence gap.
- **P2**: important follow-up that should not displace P0/P1 work.

## Recommended execution order

1. Infinite-canvas drawing outside text blocks.
2. First-launch privacy and startup flow.
3. User-facing error sanitization.
4. Block creation, hit testing, and click-to-caret.
5. Confirm the duplicate-notebook report was only a local setup issue.
6. Clipboard interoperability.
7. OneNote import scope and highest-value missing features.
8. Linux validation and Windows installer validation.
9. AMD graphics crash investigation.
10. Performance baseline measurements.
11. Decide whether RTL UI mirroring belongs in 0.0.8.
12. Investigate and clean malformed folder names shown in the attached image.

## P0: Core workflow and trust

### P0-1: Infinite-canvas drawing outside text blocks

**Status:** Investigated; fix not implemented  
**Reference:** [issue_infinte_canvas.md](./issue_infinte_canvas.md)  
**Related issue:** GitHub #22

With Draw selected, ink should begin and continue anywhere on the page,
including empty space beside, above, or below text blocks. The current ink
interaction is bounded by the calculated page body and can lose pointer input
outside the text layout.

Acceptance:

- An empty page accepts a stroke without creating a text block.
- Drawing works beside, above, and below text blocks.
- A stroke remains continuous across former text-block boundaries.
- Write mode still edits text and does not steal the click.
- Scrolling and zooming preserve coordinates.
- Ink saves and reloads through `ink.svg`.
- Both HTML and CMGFM pages are covered.

### P0-2: First-launch privacy and startup flow

**Status:** Appears implemented in the current baseline; verify before closing  
**References:**

- [issue_first_launch_dialog.md](./done/issue_first_launch_dialog.md)
- [issue_missing_privacy_dialog.md](./done/issue_missing_privacy_dialog.md)
- [`apps/notecraft/src/main.rs`](../../../apps/notecraft/src/main.rs)
- [`crates/ui-egui/src/app.rs`](../../../crates/ui-egui/src/app.rs)

The Welcome notebook auto-deployment code is removed from startup, but the
first-use flow needs one clear product decision. Decide whether the initial
dialog is a logging-level disclosure, a telemetry consent dialog, or two
separate dialogs.

Acceptance:

- A fresh settings directory shows the intended dialog exactly once.
- Dismissal and the user's choice persist.
- Settings or Privacy allows the choice to be changed later.
- `--first-use` reliably resets the flow for testing.
- No Welcome notebook deployment occurs implicitly at startup.
- No telemetry or network behavior occurs before consent.
- Existing settings do not unexpectedly show the dialog again.

The current code checks `privacy_welcome_seen`, supports the `--first-use`
reset flag, and has UI tests for first-use behavior. Treat this as a
verification task unless a fresh-settings manual run reproduces the missing
dialog.

### P0-3: Sanitize user-facing errors

**Status:** Needs a focused audit; the original Welcome-deployment example is
obsolete in this baseline  
**Reference:** [issue_user_facing_errors.md](./issue_user_facing_errors.md)

Technical paths, raw OS codes, and implementation terms must not be shown in
normal user-facing messages. Technical details belong in logs. The original
Welcome-notebook deployment example is no longer the active startup path, but
the same pattern may still exist in clipboard, file, import, and save errors.

Audit at least:

- [`apps/notecraft/src/main.rs`](../../../apps/notecraft/src/main.rs)
- [`crates/ui-egui/src/app.rs`](../../../crates/ui-egui/src/app.rs)
- [`crates/ui-egui/src/backstage.rs`](../../../crates/ui-egui/src/backstage.rs)
- [`crates/store/`](../../../crates/store/)

Acceptance:

- No raw OS error codes in normal dialogs or alerts.
- No VirtualBox, UNC, or full local paths in user messages.
- Messages explain the impact and give a next step.
- Technical context is logged with the relevant operation.
- Recoverable errors offer retry, continue, or an appropriate settings action.

## P1: High-value product issues

### P1-1: Block creation, hit testing, and click-to-caret

**Status:** Specified; not implemented  
**Reference:** [issue_block_hit_test.md](./issue_block_hit_test.md)

New blocks should have a useful initial width and clicking inside a block
should activate it and place the caret near the clicked line and character.

Current behavior is split into two intentional paths:

- Clicking open paper creates a new block at the pointer position.
- Clicking inside an existing block should go to that block's editor and must
  not create another block.

The current `start_a_box` width is not 500-600 points. It derives the width
from remaining page room and caps it at approximately 360 points. The issue is
therefore both an initial-size issue and a hit-testing/caret-placement issue;
it is not a request to stop creating blocks when the user clicks genuinely
open paper.

Acceptance:

- New blocks are at least 500 points wide where the page permits.
- A new page is immediately ready for typing.
- One click activates the intended block.
- Clicking text places the caret on the clicked line.
- Clicking above or below text places the caret at the nearest sensible end.
- Tests cover multiple blocks and switching between them.

### P1-2: Duplicate notebook in the sidebar

**Status:** Likely local setup issue; verify and close if not reproducible  
**Reference:** [issue_duplicate_notebook.md](./issue_duplicate_notebook.md)

The Welcome notebook was observed twice in the notebook tree. Determine
whether the cause is duplicate filesystem data, duplicate insertion during
refresh, stale tree state, rendering duplication, or Welcome notebook
creation behavior.

The current checkout contains a normal `my-notebooks/welcome-to-notecraft`
directory and separate malformed path-derived directories. First isolate those
local setup artifacts and retry with a fresh notebooks folder. Do not change
tree-loading code unless the duplicate reproduces with one physical notebook
directory.

Acceptance:

- One physical notebook produces one tree node.
- Repeated refresh is idempotent.
- Creating the Welcome notebook through the File menu is idempotent.
- Deleting and recreating a notebook does not leave a stale duplicate.
- A regression test reproduces the original failure before the fix.

### P1-3: Clipboard interoperability

**Status:** Plain text works; rich clipboard support is incomplete  
**References:** [`ROADMAP.md`](../../../ROADMAP.md), [`docs/gaps.md`](../../gaps.md)

Implement or explicitly scope:

- bitmap, DIB, and PNG paste;
- HTML clipboard input;
- formatted table paste;
- rich text/RTF output;
- NoteCraft-specific clipboard formats where useful.

Acceptance should use real clipboard test data and the repository's clipboard
test exemption. Do not silently downgrade a rich paste to plain text without
preserving the existing user-visible behavior.

### P1-4: OneNote import completeness and scope decision

**Status:** Required product area; incomplete  
**References:** [`ROADMAP.md`](../../../ROADMAP.md), [`docs/gaps.md`](../../gaps.md)

The importer has been exercised against 443 real backup files, but missing
areas include:

- ink;
- tags;
- page dates;
- `.onetoc2` section order and notebook colors;
- `.onepkg` packages;
- password-protected sections;
- a window import command;
- complete preservation of underline and equation structures.

The parser-choice analysis in the baseline also leaves the implementation
decision pending. Select a concrete 0.0.8 scope and document what is deferred.

## P1: Release confidence

### P1-5: Validate Linux and macOS support claims

**Status:** Unverified release claim  
**References:** [`docs/gaps.md`](../../gaps.md), [`ROADMAP.md`](../../../ROADMAP.md)

The application has been built and run on Windows, but Linux and macOS
behavior is not established.

At minimum validate Linux native build and runtime on the supported test
environment, including X11 or Wayland as applicable, file dialogs, clipboard,
AccessKit/AT-SPI behavior, and packaging. Validate macOS separately before
claiming support.

### P1-6: Investigate the AMD graphics crash

**Status:** Unreproduced; release risk remains  
**Reference:** [`docs/gaps.md`](../../gaps.md)

A release build reportedly crashed repeatedly on one Windows machine with AMD
graphics. Test explicit wgpu backends, capture the backend and driver in the
diagnostic log, and determine whether a product default or documented
workaround is required.

### P1-7: Clean-machine installer validation

**Status:** Package built; installation lifecycle not validated  
**Reference:** [`docs/gaps.md`](../../gaps.md)

Test a clean Windows machine or VM for:

- installation;
- Start menu shortcut;
- finish-page launch option;
- same-version upgrade;
- uninstall;
- preserving notebooks during uninstall;
- SmartScreen messaging;
- the GitHub release workflow.

This is a release-gate verification task rather than an immediate code issue.

### P1-8: Large notebook and attachment load testing

**Status:** Caps exist; measured behavior is missing  
**Reference:** [`docs/gaps.md`](../../gaps.md)

Create representative real fixtures and measure startup, page open, editing,
save, search, memory, and frame responsiveness for large notebooks, many
containers, images, ink, attachments, and diagrams.

## P2: Important follow-up

### P2-1: Decide the scope of RTL UI mirroring

**Status:** Text support is partial; full chrome mirroring is planned  
**Reference:** [enhancement_bidi_uiux_alignment.md](./enhancement_bidi_uiux_alignment.md)

Remaining areas include mirrored application chrome, advanced RTL selection,
run isolates, Latin font consistency, and digit options. Full UI mirroring is
a substantial cross-cutting change. Explicitly decide whether it belongs in
0.0.8 or is a separate release/branch.

### P2-2: Page editing structural limitations

**Status:** Known product limitation  
**Reference:** [`docs/gaps.md`](../../gaps.md)

The editor still has limitations around cross-block selection, editing tables,
pictures, and horizontal rules, and preserving freeform arrangement when page
text is written without containers. Prioritize only after the block hit-testing
work unless the release scope changes.

### P2-3: Performance baseline measurements

**Status:** Plan exists; measurements do not  
**Reference:** [rc_perf.md](./rc_perf.md)

Start with measurable baselines for startup, frame time, page parse/load/save,
search, memory, and large demo pages. Defer a full flame-chart/Gantt/waterfall
analysis product unless the baseline identifies a need.

### P2-4: Malformed folder names in the repository checkout

**Status:** Observed in the attached screenshot; cause not yet diagnosed

The screenshot shows malformed-looking folder names in the repository root,
including entries resembling:

- `Zdevp-NoteCraftwork`
- `Cdevp-NoteCraft.reposo-NoteCraft_2my-notebooks`

The current checkout inspection confirms these are actual directory names, not
only a screenshot rendering artifact. They appear to be path strings that were
created as folder names, likely by a script or tool that mishandled Windows
path escaping or a copied workspace path.

This must be investigated before deletion:

1. Confirm whether the directories are tracked, ignored, or untracked.
2. Inspect their contents and timestamps.
3. Search scripts, test output, and recent command history for the creator.
4. Check whether any active process or test depends on them.
5. Repair the path handling at the source.
6. Remove only the specifically confirmed stray directories with owner
   approval, never with a broad recursive cleanup.

Acceptance:

- No malformed path-derived directories are created by normal tests, builds,
  screenshots, or profiling commands.
- Existing intended paths use normal Windows separators and remain readable.
- A regression check catches path strings being used as directory names.
- Cleanup is verified with `git status`, directory listing, and the relevant
  test/build command.

This item is intentionally documented rather than deleted automatically:
the directories may contain useful diagnostics or work from another process.

### P2-5: Font, links, Mermaid, container, drawing, and sync gaps

The following remain documented follow-up work:

- bundled bold and italic fonts;
- links inside tables and mixed-direction lines;
- Mermaid source-file rendering, zoom, pan, and richer syntax;
- moving blocks between containers and picture-only containers;
- automatic shapes, stencils, ink replay, ink-to-text, and ink-to-math;
- sync conflict merge and resolution UI;
- page history for attached assets and drawings.

These are not individually recommended as 0.0.8 blockers unless the owner
changes the release scope.

## Explicitly excluded

- Image frame and resize behavior, including issue #18, is excluded from this
  file because Agent 1 owns that work in another folder.

## Verification expectations

For implemented issues, run the smallest focused checks first, then the
repository gate before release:

```text
cargo fmt --check
cargo test -p notecraft-ui-egui --test ui
cargo test --workspace
cargo xtask ci
```

Add platform-specific, installer, demo, parity, or visual checks whenever the
issue touches those surfaces. Do not claim Windows/Linux/macOS support,
installer readiness, or visual correctness without running the corresponding
check.



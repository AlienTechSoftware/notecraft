# Workspace 2: Infinite canvas

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_2`
**Updated:** 2026-11-05
**Status:** Implementation complete for this workstream; ready to test.

## Goal

Deliver the v0.0.9 infinite-canvas work. The release README describes the target as reliable drawing outside text blocks and across empty page space.

## Verified current behavior

The repository has page-wide ink already. `crates/ui-egui/src/app.rs` lays out an ink body after the page blocks, reserves `max(1100pt, content height + 240pt)`, and calls `inkpad::show`. The page lives in a vertical `ScrollArea`; page colour and rule lines are painted behind it. `crates/ui-egui/src/inkpad.rs` stores strokes in page coordinates in `ink.svg`. The Draw tool's ink response therefore has a finite page surface that grows with content, rather than a truly unbounded canvas. Ink and background scrolling behavior is implied by this shared page layout; it has not been visually exercised in this investigation.

The older `docs/releases/0.0.8/issue_infinte_canvas.md` describes the original bug as empty-space ink hit testing and gives criteria for drawing outside text blocks, keeping Write behavior, and saving/reloading both HTML and CMGFM. The current `app.rs` includes a finite sheet reservation intended to cover empty page space, but no UI test specifically covering these cases was found by source search. Runtime reproduction remains unverified.

The private backlog's owner-priority section says to preserve continuous scrolling/backgrounds, pen/touch/keyboard switching, drawing performance, and related settings/theme controls. It explicitly says to confirm whether the desired behavior is seamless backgrounds as the page grows or a different infinite-canvas behavior. The backlog also says the page already scrolls vertically, grows with content, and creates a box when open paper is clicked; that click-to-create behavior is not missing work. `work/requests/pen-and-drawing.md` records that pressure is surfaced through winit on Windows; it does not establish device classification or automatic switching requirements for touch and keyboard.

## Clarified scope and proposed acceptance criteria

Owner direction (2026-11-05): include both continuous page behavior and an unbounded canvas interaction; the unbounded interaction is the priority. Existing zoom-out reduces page content only (50% to 250%) and does not navigate into open space around the finite page surface. Defer automatic pen/touch/keyboard switching. Keep manual Draw/Write selection.

Proposed observable criteria:

1. In Draw mode, a stroke can start and continue in empty canvas space beyond the current page area in all directions, including while the view is navigated; no text box or paragraph is created by drawing.
2. Existing text and box content retain stable positions relative to page-coordinate ink while zooming and navigating.
3. Page background/rules continue seamlessly as the page grows and through navigation, without a visible edge at the former reserved sheet boundary.
4. In Write mode, text editing and click-to-create-box behavior remain unchanged; canvas navigation uses a separately defined gesture and does not steal ordinary text clicks.
5. Ink remains editable and round-trips through `ink.svg` for HTML and CMGFM pages; zoom/pan do not alter stored stroke coordinates.
6. UI regression tests cover Draw versus Write event ownership, navigation, former sheet boundaries, and save/reload for both page formats. Record a repeatable large-stroke interaction/render workload and verify it has no severe responsiveness regression; set a numeric budget after measuring the baseline.
7. Do not add automatic input switching. State actual platform support based on available UI builds/tests; do not infer device support from Windows pressure data.

Navigation gesture is still an implementation choice. Proposed baseline: middle-button drag pans the view; wheel scrolling remains vertical, with Shift+wheel horizontal. This preserves ordinary primary-button text/ink input and avoids relying on automatic device classification. Confirm during implementation design if existing egui behavior makes this gesture conflict.

## Implementation checkpoint proposal

Test-first work in `crates/ui-egui/tests/ui.rs` (and focused UI helpers if needed), then implementation primarily in `crates/ui-egui/src/app.rs` and `crates/ui-egui/src/inkpad.rs`; add a small platform-neutral viewport type only if the current app state cannot own the transform cleanly. No page format change is expected. Run focused UI tests, crate tests/clippy/fmt, then `cargo xtask ci` and relevant visual paired checks for HTML/CMGFM.

Risk: egui scroll-area event ownership and the current text layout couple page coordinates to a finite body rectangle; pan/zoom must preserve caret, selection, ink hit testing, and persistence without intercepting normal editing. Estimate: 50-90k implementation tokens; owner review 45-75 minutes including build and UI smoke review. Biggest unknown: whether current UI automation can deterministically exercise 2D pan gestures and compare persisted ink across both formats.

The detailed private backlog is `work/notecraft-improvements.md`, especially its owner-priority and open-work sections. This summary is included here so the checkout can coordinate from Git without depending on the private host.

## Acceptance criteria

Acceptance criteria 1, 2, 4, 5 and the UI portions of 6 are covered by the focused UI test for HTML and CMGFM. The ink crate regression test covers negative and beyond-sheet coordinates through save and reload. Automatic input switching remains deferred as requested. Criteria around continuous backgrounds through arbitrary navigation and a measured large-stroke responsiveness budget remain open for owner smoke review; no performance claim is made.

## Coordination

- At assignment, `_2` was clean on `main`. Inspect local `next` and remote state before creating a dedicated v0.0.9 feature branch.
- Keep canvas implementation in `_2`; do not force-switch branches or mix unrelated changes.
- Coordinate shared UI automation requirements with `_3` and check for regressions against `_1` localization.
- Do not merge or push without explicit authorization.
- This checkout has branch `feat/v0.0.9-infinite-canvas` based on `origin/next` (local `next` did not exist). `origin/next` and `fork/next` both pointed to `568b810` at investigation time. The branch was clean when created; the handoff note is now an untracked file on this branch.

## Progress

- 2026-11-04: Workstream assigned. No files or code were changed in `_2` by the coordination setup.
- 2026-11-05: Inspected current page/ink layout and the private drawing backlog. Confirmed finite sheet reservation, vertical scrolling, page-colour/rule painting and page-coordinate `ink.svg`. On 2026-11-05, the owner clarified that both continuous page behavior and an unbounded canvas are wanted, with the latter prioritized; input switching is deferred. Created `feat/v0.0.9-infinite-canvas` from `origin/next` and received the required CHECKPOINT nod.
- 2026-11-05: Added a failing-first integration test, then implemented 2D canvas navigation (middle-button drag, vertical wheel, Shift+wheel horizontal), continuous viewport background/rules, page-space ink hit testing and SVG viewBox growth for negative/outside-page ink. Added save/reload regression coverage. Verified HTML and CMGFM side-by-side at matching viewport/settings with test captures; visible ink remains in open space for both.
- Checks: `cargo fmt --all -- --check` PASS; `cargo test -p notecraft-ink` PASS (37 tests); `cargo test -p notecraft-ui-egui --test ui` PASS (123 tests); `cargo clippy -p notecraft-ink -p notecraft-ui-egui --all-targets -- -D warnings` PASS; `cargo xtask ci` PASS (workspace suite and all repository gates); `git diff --check` PASS.
- Remaining owner smoke review: manually exercise large strokes and background continuity while panning/scrolling in the Windows app; large-stroke responsiveness has not been benchmarked. No commit or push made.
- 2026-11-05 follow-up: owner clarified that ink must render above text and drawing over text must not move the caret or alter page words. Added `drawing_over_text_keeps_ink_above_the_page_without_editing_words`; fixed Draw-mode pointer handling to preserve the caret and selection while the foreground ink layer owns the gesture. `cargo test -p notecraft-ui-egui --test ui` passes (124 tests), `cargo xtask ci` passes, and `cargo build -p notecraft` passes. Relaunched the rebuilt app against a fresh temporary notebook folder containing one Welcome notebook.

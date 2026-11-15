# Vertical scroll bar dead under the Select tool

Status: fixed on `fix/select-scrollbar`, uncommitted, `cargo xtask ci` green
Date: 2026-11-14

## Issue

With the Select tool active, the page's vertical scroll bar did not respond to
mouse input. It worked normally with the Typing tool.

## Cause

In every non-Write pointer mode the page renders `containers::show_ink_view`
followed by `inkpad::show`, which registered a page-wide
`ui.interact(body, layer, Sense::click_and_drag())` on an egui `Order::Middle`
layer. That response covered the floating vertical scroll bar strip at the
right edge of the ScrollArea viewport, so egui hit testing gave the ink layer
pointer ownership over the bar. Typing mode returns before that interaction is
created, which is why the bar kept working there.

## Fix

`inkpad::show` now takes a separate `hit` rectangle: `body` still defines the
paint and coordinate space, while `hit` is the only region that may take the
pointer. `NoteCraftApp` remembers the page ScrollArea's vertical bar strip from
the previous frame (`page_scroll_strip`, computed from the same
`bar_width`/`bar_outer_margin`/clip math egui uses for its bar interact rect)
and shrinks `hit` to end at the strip's left edge. Ink painting, selection and
coordinate conversion are unchanged.

## Verification

- `cargo test -p notecraft-ui-egui --test ui the_page_scroll_bar_works_while_the_select_tool_is_active`
  failed before the fix (Select left the page origin unchanged) and passes after.
- `cargo test -p notecraft-ui-egui`: 269 passed.
- `cargo clippy -p notecraft-ui-egui --all-targets -- -D warnings`: clean.
- `cargo fmt --all -- --check`: clean.
- `cargo xtask ci`: all checks passed.

Files: `crates/ui-egui/src/app.rs`, `crates/ui-egui/src/inkpad.rs`,
`crates/ui-egui/tests/ui.rs`.

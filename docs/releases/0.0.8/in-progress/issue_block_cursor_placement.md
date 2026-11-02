# 0.0.8 issue: clicking a block must place the caret

## Status

**Status:** Planned. The owner reported that clicking inside a text block can
select the whole block instead of placing the caret in the text.

**Priority:** P0 interaction issue.

## User-visible requirement

A block has two different interaction zones:

- Clicking the thick top border or header selects the whole container/block for
  moving, resizing, or block actions.
- Clicking inside the body enters text editing and places the caret at the text
  position under the pointer. It must not select the whole block.

For a click in blank body space below the current text, the caret should be
placed at the end of that block rather than starting a different block.

The date paragraph remains content at the top of a new page; it follows the
same body-click rule when it is editable.

## Acceptance criteria

- A body click gives the owning text block focus.
- The caret lands at the nearest valid text offset for the pointer position.
- Clicking below the final line in the same block places the caret at the
  block's end.
- The container is not selected by a body click.
- Clicking the thick top border/header still selects the container.
- Dragging the header still moves the container.
- Existing image resize, attachment double-click, text selection, RTL caret,
  and block deletion behavior remains unchanged.
- HTML and CMGFM pages behave the same way.

## Diagnosis plan

1. Use the session log to correlate pointer coordinates, container frame,
   block rectangle, caret block, caret offset, and selection state.
2. Trace the competing `egui` responses in `containers.rs` and `pageedit.rs`:
   the container frame/header response must not consume clicks that belong to
   the block body.
3. Confirm the body hit-test maps the pointer to the existing block's text
   layout, including the end-of-block fallback.
4. Add regression tests before changing behavior:
   body click places the caret, blank-body click goes to the block end, and
   header click selects the container.

## Likely implementation

Keep the header interaction rectangle separate from the content rectangle.
Route body clicks through the existing text hit-testing and caret-placement
path, and reserve container selection for the header/thick top border. Avoid
adding serialized placeholder paragraphs solely to support the interaction.

## Verification

- Focused UI tests for body clicks, end-of-block clicks, and header selection.
- Existing image resize and attachment double-click regression tests.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo xtask ci`.

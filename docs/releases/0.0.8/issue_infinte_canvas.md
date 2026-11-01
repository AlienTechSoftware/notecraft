# Infinite canvas drawing outside text blocks: investigation

- Date: 2026-10-31
- Status: INVESTIGATED; fix not implemented
- Issue: https://github.com/AlienTechSoftware/notecraft/issues/22
- Release target: 0.0.8

## Summary

Issue #22 reports that the Draw tool does not create ink when the pointer is in an empty
area of an infinite-canvas page outside a text block. Drawing is expected to work anywhere
on the page, independently of text containers.

This belongs in 0.0.8 because it blocks the defining freeform-canvas workflow. The bug is
in UI hit testing and page geometry, not in the ink file format: ink is already stored as
`ink.svg` beside the page file and can represent coordinates outside the text.

## What the current code does

The relevant path is:

1. `crates/ui-egui/src/app.rs` lays out the editable page in the `page` closure.
2. `containers::show(...)` lays out and interacts with text blocks.
3. The app then computes a `body` rectangle from the page's top coordinate and current UI
   bounds, reserves vertical space, and calls `inkpad::show(...)` after the text blocks.
4. `crates/ui-egui/src/inkpad.rs::show` paints existing strokes with a layer painter and
   installs `ui.interact(body, ui.id().with("ink-layer"), Sense::click_and_drag())` when
   the active pointer is Draw, Erase, Pick, Shape, or Lasso.

The ink implementation explicitly intends to accept strokes outside words. Its pointer
coordinate is converted from the `body` origin, and the saved drawing has its own page
coordinates. The storage model therefore does not require a text block to exist at the
stroke location.

## Likely failure mechanism

The interaction surface is only the `body` rectangle created by the page layout. It is not
an independent infinite-canvas surface. Its width and position come from the current egui
container (`ui.max_rect()`), while its height is synthesized from the text content plus a
fixed amount of extra room. This creates several ways for an open canvas area to be outside
the actual hit target:

- open space beside the text column can be outside the `Frame`/column UI rectangle;
- space below the reserved area can be outside `body` until the layout has been extended;
- text-container widgets and their hit testing are created before the ink interaction and
  can consume pointer input where their rectangles overlap the intended canvas surface;
- the `body.contains(pointer)` filter silently discards any pointer position outside the
  computed rectangle, even though the painter itself is clipped to that same rectangle.

The current implementation therefore has one large page-layer interaction rectangle, but
it is still bounded by the layout geometry of the text editor. The bug should be reproduced
with an empty page and with a page containing a text block, testing beside, below, and far
from the block. The first failing case will identify whether the defect is horizontal page
geometry, insufficient vertical reservation, or egui event ownership.

## Investigation plan for the implementation

Instrument `inkpad::show` and the page closure at debug level with:

- the screen-space `body` rectangle;
- `pointer_interact_pos()` and whether it is inside `body`;
- the active pointer mode and `resp.hovered()/contains_pointer()/is_pointer_button_down_on()`;
- the text-container response that is under the same pointer, when any.

Then replay the issue in these positions:

1. directly beside a text block;
2. below the last text block but inside the visible page;
3. well below the last text block after scrolling;
4. on an empty page with no text block at all.

The fix should make the canvas hit target cover the complete scrollable page surface and
should keep text editing and ink mutually exclusive for a Draw-tool press. Do not solve this
by adding dummy text blocks: that would make the canvas depend on editor content and would
create unwanted persisted paragraphs.

## 0.0.8 acceptance criteria

1. With Draw selected, a stroke can begin and continue in empty space beside, below, and
   above text blocks.
2. An empty page accepts a stroke without first creating a text block.
3. A stroke remains continuous when it crosses the former text-block boundary.
4. With the Write tool selected, clicking empty page space keeps the existing text-editing
   behavior.
5. Existing text blocks remain editable; Draw input does not move the caret or create a
   paragraph.
6. Coordinates remain stable while scrolling and zooming, and strokes save/reload through
   the existing `ink.svg` path.
7. The behavior is verified for both HTML and CMGFM pages because the ink layer is shared by
   both formats.

## Scope and risk

This is a UI interaction and geometry fix. No page-format change is expected. The main risk
is making the canvas widget steal clicks needed by text editing, so the implementation must
preserve the existing pointer-mode split and test both Draw and Write paths.

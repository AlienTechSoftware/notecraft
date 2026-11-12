---
name: nc-uiux-keys
description: Keyboard and pointer navigation into and out of page objects (code blocks, math, tables, images, diagrams) and between page blocks: arrows, Page Up and Down, Home, End, and a single click to edit.
license: MIT
metadata:
  version: "0.0.20261111"
  related_skills: [nc-uiux-controls, nc-uiux-bidi, nc-uiux-z-order, nc-uiux-testing]
---

# nc-uiux-keys

## Job to be done

Make moving through a page feel the same whether the caret is in text or in an object. The owner's
standard (2026-11-11): navigation into and out of a block is smooth with the cursor and the
navigation keys.

## Contract

Two layers. Keep them separate. The generic layer is the default. A block kind may override it
only with an owner rule written under its own heading below.

### Generic caret movement (unrestricted text)

Paragraphs, headings, quotes and list items. These are plain text: the caret moves character by
character and line by line across blocks in page order.

- Left and Right move one character; at a block edge they move into the neighbouring block.
- Up and Down move one visual line at a time, keeping the column. Shorter lines clamp to their end.
- Home and End go to the start and end of the current visual line.
- Ctrl+Home and Ctrl+End go to the start and end of the page.
- Page Up and Page Down move by a screen and never skip a block.
- Enter splits the block at the caret. Backspace at the start joins it with the block above.
- Shift with any of these extends the selection across blocks.
- Escape leaves editing without changing words.

### Code block

Owner rules (2026-11-11). A code block is visual styling: a frame and background drawn around
page text.

- Click on a code block's text places the caret in it. A click does not select the block as an
  object.
- Inside the block, Up and Down move one line at a time, keeping the column, as in generic text.
- Up from the block's first line, and Down from its last line, leave the block for the neighbouring
  block. The block then shows in view mode, with its line numbers.
- Down from the block above lands in the first line of the block. Up from the block below lands in
  its last line, at the end of that line.
- Enter inside the block adds a line. Enter on an empty line below the block leaves it as a new
  paragraph.
- Numbered code keeps its numbers and bar while the caret is inside; they are not part of the text
  the caret moves through.

Known open points (2026-11-11): Up and Down inside a block did not move the caret line by line in
the running app, and Down into a block landed at its end instead of its start.

### Other special blocks

Math blocks, tables, images, diagrams and boxes have their own rules. Do not assume the code block
rules apply to them. Each needs owner rules recorded here before its caret behaviour changes.

## Design rules

- The keys are the same in read and edit views. The view should not change size when the caret
  arrives: draw the object with the same frame in both states (see `preview.rs` and `pageedit.rs`).
- Object clicks, link clicks and text clicks must not fight. A click on a link follows the link; a
  click on an object's body edits it; a click beside an object puts the caret in the neighbour.
- Do not add a keyboard mode switch. One keyboard model for every block.

## Known gap (2026-11-11)

Code blocks are treated as objects (`crates/ui-egui/src/preview.rs`, `object_kind`, which returns
`code`). Object clicks record a selection, and editing a code block needs a double-click. The fix
belongs in the object click path, so a single click on a code object moves the caret into it.

## Checks

- A UI test for each rule above, using the real window and key events (`crates/ui-egui/tests/ui.rs`),
  with a failing test written before the fix.
- Run the same test in the HTML and CMGFM pages.
- Visual check: caret arrival must not change the block's size.

## Not in this skill

- Text entry in right-to-left blocks: `nc-uiux-bidi`.
- Pointer ownership when surfaces overlap: `nc-uiux-z-order`.
- Buttons and menus: `nc-uiux-controls`.

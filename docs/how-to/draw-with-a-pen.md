# Draw with a pen

NoteCraft takes ink on the page itself, anywhere on it, over the words and between them.
Open the **Draw** tab and pick a pen. That is all: there is no sheet to insert first.

## Write on it

Pick a pen from the rack in **Drawing Tools**, then write anywhere on the page.

- A **pen** swells as you press. At a light touch it is a quarter of its width.
- A **pencil** varies less, the way a pencil does.
- A **fountain pen** answers pressure more than a pen: a hairline at a touch, broad when
  pressed.
- A **brush pen** has the widest range of all, nearly nothing until you lean on it.
- A **highlighter** is see-through and does not thin at all, because a chisel tip lays down
  its whole width however you hold it.

**Eraser** rubs out any stroke the pointer passes over. **Undo stroke** takes the last one
back, and **Redo stroke** puts it back.

The Draw tab also provides tools for drawing straight lines and clean shapes:

- **Ruler** lays a straightedge on the page. Drag to move it, or drag its handle or press `[`
  and `]` (hold Shift for 15 degrees) to turn it. A stroke started just outside a long edge
  is guided along that edge so it stays straight. The ruler is a stencil and is not saved to
  the file.
- **Shapes** offers regular shapes (lines, arrows, rectangles, rounded rectangles, squares,
  ellipses, circles, diamonds, triangles and stars) drawn by dragging between two corners.
- **Automatic Shapes**, when turned on, recognises a hand-drawn line, rectangle, ellipse or
  triangle and redraws it as a clean shape when you lift the pen, keeping your chosen tool,
  colour and thickness. Recognition is strict so handwriting stays as drawn, and one Undo
  restores what the hand drew.
- **Ink Replay** redraws the page's strokes in the order they were made at an even speed,
  with controls for play, pause (Space), restart, stepping one stroke back or forward, and
  speed (0.5x to 4x). It does not alter the drawing or undo history.

## Picking ink up

Ink is its own thing on the page, not part of any paragraph, so it is picked up on its own.

- **Select** then click a stroke to hold it. Drag inside the box to move it, drag a corner
  to stretch it, and press Delete to throw it away. Clicking bare page lets go.
- **Lasso Select** then draw round ink to pick it up. A closed loop catches strokes whose
  points sit inside it; an open C-shaped bracket catches strokes that are enclosed or have
  at least 65% of their readings and centre enclosed.
- **Type** puts the pointer back to the words and the caret.

Moving a paragraph does not move ink that happens to sit near it, and moving ink does not
disturb the words under it. They are two layers on one sheet.

## Arranging the rack

The rack is yours to arrange, and it is remembered between runs.

- **Add** puts another tool on the end: a pen, pencil, fountain pen, brush pen or
  highlighter.
- **Right-click a tool** for Modify Tool, Move Left, Move Right, Add Another and Delete.
- **The chevron** under the tool in your hand opens the tool editor: the stroke it makes,
  its thickness as dots, and a grid of colours. The Eyedropper picks a colour from existing
  ink on the page.
- Each rack pen displays a preview of its stroke underneath it.
- **Alt+1 to Alt+9** pick up the pen at that position on the rack from anywhere; **Alt+0**
  puts the pen down and returns to typing.
- The last tool cannot be deleted, so there is always something to draw with.

## Sensitivity

The wedge beside the rack sets how readily the pen answers pressure. It is not thickness:
thickness is how wide a tool is at a full press, and sensitivity is how hard you have to
press to get there. Turned up, a light hand already lays down a broad line; turned down, it
takes a firm press. The middle leaves the readings exactly as the pen gave them.

Your pen's own readings are kept whatever this is set to, so a stroke drawn at one setting
can be re-weighted later rather than having to be drawn again.

Ink is written on a background thread a quarter of a second after the last change once the
pen is lifted, so drawing never waits for the disk. It is finished before any action that
needs it on disk, such as switching pages or undoing.

## Where the ink lives

In `ink.svg`, in the page's own folder, beside `index.md` or `index.html`. A page nobody has
drawn on has no such file, and a page rubbed clean loses it again. The ink travels, syncs
and backs up with the page because it is another file in the same place.

A page with ink names it, at the end of its words:

```markdown
![Ink](ink.svg)
```

That line is how the page's source and the page itself stay in step, and how another
program opening the folder finds your handwriting at all. NoteCraft does not draw it as a
picture in the middle of the words; it draws the ink as a layer over the whole page, so
the line takes no room here. The line appears the first time you draw and goes when the
page is rubbed clean.

## Pressure

If you draw with a pen that reports pressure, the stroke swells and thins as you press.
A mouse draws an even line, which is the right answer for a mouse.

Pressure is read on Windows, and on Linux under Wayland when using pen tablets. On macOS and
Linux under X11 the windowing layer does not report tablet pressure events, so a pen there
draws like a mouse; see [gaps](../gaps.md). To see what your pen is reporting:

```sh
cargo run -p notecraft --example pen_check
```

## What is in the file

Two things, in one file:

- **The drawing**, as filled outlines. SVG has no variable-width stroke, so every editor
  draws a pressure stroke as a filled shape, and so do we. Any browser opens the file and
  shows your handwriting.
- **The pen's own readings**, in a namespace of ours (`nc:dabs`), on the same shapes. Other
  programs ignore them. NoteCraft reads them back, which is what lets it rub out one stroke
  of an old drawing, or redraw it at another size, long after it was made.

**Save picture** writes the ink again next to it, as `ink-picture.svg`, with the readings
left out. Use it when you are sending the drawing somewhere else and want nothing but the
picture. In the browser, it also starts a download so you can retrieve the SVG outside
the in-memory notebook. Exporting again uses a new filename and keeps earlier pictures.

## What is not here yet

Turning handwriting into text (Ink to Text) and formulas into math (Ink to Math) are not
built yet; the Draw tab buttons are placeholders. The raw readings kept in every `ink.svg`
are what will make handwriting recognition possible later without redrawing anything.

Other stencils such as a protractor, replaying only a selected part of the ink, tilt and
eraser-end stylus detection, and pressure on macOS and X11 remain unbuilt.

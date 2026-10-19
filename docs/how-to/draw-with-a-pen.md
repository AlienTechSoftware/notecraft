# Draw with a pen

NoteCraft takes ink on a sheet you put into a page. Open the **Draw** tab.

## Put a sheet on the page

**Draw**, then **Insert a drawing**. A sheet appears under the line the caret is in, and the
pen is already in your hand. The page now holds an ordinary picture:

```markdown
![Drawing](assets/drawing-1.svg)
```

That is all the page knows about it. The drawing itself is a file in the page's own folder,
so it travels with the page, syncs with it and opens in any browser.

## Write on it

Pick a pen from the rack in **Drawing Tools**: four pens, a pencil, two highlighters. The
slider beside them sets how wide it is. Then draw on the sheet.

- A **pen** swells as you press. At a light touch it is a quarter of its width.
- A **pencil** varies less, the way a pencil does.
- A **highlighter** is see-through and does not thin at all, because a chisel tip lays down
  its whole width however you hold it.

**Eraser** rubs out any stroke the pointer passes over. **Undo the last stroke** takes the
last one back. **Write** puts the caret back, so clicking the page writes words again
instead of drawing.

Each stroke is written to the file the moment you lift the pen. Ink does not wait for the
page to save.

## Pressure

If you draw with a pen that reports pressure, the stroke swells and thins as you press.
A mouse draws an even line, which is the right answer for a mouse.

Pressure is read on Windows today. On macOS and Linux the window layer does not hand it to
us, so a pen there draws like a mouse; see [gaps](../gaps.md). To see what your pen is
reporting:

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

**Save as a plain picture** writes the drawing again next to the first, as
`drawing-1.plain.svg`, with the readings left out. Use it when you are sending the drawing
somewhere else and want nothing but the picture.

## What is not here yet

Drawing anywhere on the page rather than on a sheet, selecting and moving ink, shapes, the
ruler, ink replay, and turning handwriting into text. The Draw tab shows those and says
they are not built. The readings kept in every file are what will make handwriting
recognition possible later without redrawing anything.

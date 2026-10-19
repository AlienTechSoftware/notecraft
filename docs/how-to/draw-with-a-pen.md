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
back. **Select** stops drawing: the pointer picks text and puts the caret back.

## Arranging the rack

The rack is yours to arrange, and it is remembered between runs.

- **Add** puts another tool on the end: a pen, pencil, fountain pen, brush pen or
  highlighter.
- **Right-click a tool** for Modify Tool, Move Left, Move Right, Add Another and Delete.
- **The chevron** under the tool in your hand opens the same editor: the stroke it makes,
  its thickness as dots, and a grid of colours.
- The last tool cannot be deleted, so there is always something to draw with.

## Sensitivity

The wedge beside the rack sets how readily the pen answers pressure. It is not thickness:
thickness is how wide a tool is at a full press, and sensitivity is how hard you have to
press to get there. Turned up, a light hand already lays down a broad line; turned down, it
takes a firm press. The middle leaves the readings exactly as the pen gave them.

Your pen's own readings are kept whatever this is set to, so a stroke drawn at one setting
can be re-weighted later rather than having to be drawn again.

Each stroke is written the moment you lift the pen. Ink does not wait for the page to save.

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

**Save picture** writes the ink again next to it, as `ink-picture.svg`, with the readings
left out. Use it when you are sending the drawing somewhere else and want nothing but the
picture.

## What is not here yet

Selecting and moving ink, shapes, the ruler, ink replay, and turning handwriting into
text. The pen rack cannot be added to or rearranged yet, and a tool's colour and thickness
cannot be changed from its own popup the way OneNote's can. The Draw tab shows those and says
they are not built. The readings kept in every file are what will make handwriting
recognition possible later without redrawing anything.

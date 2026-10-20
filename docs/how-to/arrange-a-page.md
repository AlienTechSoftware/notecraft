# Arrange a page in boxes

A page is a sheet, not a single column. The paragraphs on it live in **containers**: boxes you
can put where you like, side by side or one under another, the way an outline works in OneNote.

## Moving and resizing

Point at a box and its edges appear:

- The **bar along the top** moves it. Drag it anywhere on the page.
- The **right edge** resizes it. A box has a floor of 120 points, so it cannot be dragged away
  to nothing.

Both are saved into the page as soon as you let go.

## Start a new box

Click open paper, anywhere that is not in a box: under the last words, or beside and between the
boxes that are there. A new empty box starts where you clicked, with the caret in it, and you can write.
A click on a box's own words is a click in that box and starts nothing. A click that only lets go of a
selected box, or closes its menu, starts nothing either.

## Selecting and deleting a box

Click the **bar along the top**, or **right-click anywhere in the box**, and the whole box is
selected: everything in it is shaded at once, and the caret goes away because the box, not a word
in it, is what you picked. Then:

- **Delete** (the key, or the item on the right-click menu) takes the box away together with every
  block in it. It is saved like any other edit.
- **Cut** and **Copy** on that menu put the box's content on the clipboard as Markdown. Cut also
  takes the box away.
- Below those, the same menu carries the page's own entries (Paste, Table, Link, Equation, Date and
  time, Picture from a file, Attach a file), so a right-click on a box's words still reaches them.
- **Escape**, or a click outside the box, lets go of the selection without changing anything.

If the caret is in a box with some words selected, a right-click still belongs to those words: the
page's own menu cuts and copies them.

## Where the arrangement is kept

In the page's own text, as an element around each box's blocks: a `div` with
`class="nc-box"` and its place in `data-x`, `data-y` and `data-width`. The source pane shows
them, a browser shows an HTML page in the same boxes, and the format is written out in
[the folder format](../folder-format.md), section 3.4.

The blocks inside are ordinary HTML or Markdown, in reading order, so the page is the same
document in any other editor, in a diff, or in a search.

A page nobody arranged has no boxes in its text. Opening it, or writing on it, does not add any:
the first box that is moved, resized or added does.

Pages from an earlier version kept the arrangement on one line of metadata instead
(`containers: ...` in the front matter, or a `notecraft:containers` meta tag). They still open in
their boxes, and are rewritten in the new form the next time the boxes change.

If the page is edited somewhere else so that a box's words no longer add up, NoteCraft reads what
is there: words outside every box join the box before them, and nothing is dropped.

A script can do the same thing:

```sh
notecraft-cli run page.containers path=work/notes
notecraft-cli run page.containers path=work/notes boxes="0,0,320,2; 360,30,300,3"
notecraft-cli run page.containers path=work/notes boxes=""     # back to one column
```

## What is not built yet

Dragging a paragraph from one box to another, and a picture in a box of its own. [docs/gaps.md](../gaps.md) keeps that list.

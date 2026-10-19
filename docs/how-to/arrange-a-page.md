# Arrange a page in boxes

A page is a sheet, not a single column. The paragraphs on it live in **containers**: boxes you
can put where you like, side by side or one under another, the way an outline works in OneNote.

## Moving and resizing

Point at a box and its edges appear:

- The **bar along the top** moves it. Drag it anywhere on the page.
- The **right edge** resizes it. A box has a floor of 120 points, so it cannot be dragged away
  to nothing.

Both are saved into the page as soon as you let go.

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

In the page's own file, as one line of metadata: `containers: 0,0,320,2; 360,30,300,3` in the
front matter of a Markdown page, or `<meta name="notecraft:containers" content="...">` in an
HTML one. Each box is `x,y,width,blocks`: where it sits, how wide it is, and how many of the
page's blocks belong to it, in order.

The paragraphs themselves are **not** moved around in the file. They stay in reading order, so
the page is the same document in any other editor, in a diff, or in a search. The boxes are a
layer over that order.

That has one honest consequence. If the page is edited somewhere else and gains or loses a
paragraph, the counts no longer match what is there. NoteCraft notices and puts everything
back into one box rather than guessing and losing a paragraph. Arrange it again and it is
saved again.

A script can do the same thing:

```sh
notecraft-cli run page.containers path=work/notes
notecraft-cli run page.containers path=work/notes boxes="0,0,320,2; 360,30,300,3"
notecraft-cli run page.containers path=work/notes boxes=""     # back to one column
```

## What is not built yet

Clicking an empty part of the page to start a new box, dragging a paragraph from one box to
another, and a picture in a box of its own. [docs/gaps.md](../gaps.md) keeps that list.

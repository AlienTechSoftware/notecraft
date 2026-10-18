# Arrange a page in boxes

A page is a sheet, not a single column. The paragraphs on it live in **containers**: boxes you
can put where you like, side by side or one under another, the way an outline works in OneNote.

## Moving and resizing

Point at a box and its edges appear:

- The **bar along the top** moves it. Drag it anywhere on the page.
- The **right edge** resizes it. A box has a floor of 120 points, so it cannot be dragged away
  to nothing.

Both are saved into the page as soon as you let go.

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

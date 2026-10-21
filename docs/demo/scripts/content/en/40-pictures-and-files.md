---
title: Pictures and files
---

# Pictures and files

Drag a file from your file manager onto this page. NoteCraft asks what to do with it:

- **Insert the picture**: a copy goes into this page's `assets` folder and the page shows it.
- **Attach the file**: the same copy, shown as a link.
- **Link to the file where it is**: nothing is copied, and the link breaks if the file moves.

**Insert**, then **Picture** or **File** does the same and tells you to drop the file.

## Where it goes

```text
pictures-and-files/
  index.md
  assets/
    diagram.png
    budget.xlsx
```

The picture travels with the page: move the folder, and the picture moves with it. Names are
made safe for every operating system, two files never share a name, and a file over 512 MB is
refused.

## Pasting

A table copied from another note taking app or a spreadsheet pastes as a table. To paste a
picture, click in the page and press **Ctrl+V**. NoteCraft saves a copy in the page's `assets`
folder and inserts it in the note.

## Printouts

Some apps can render a PDF or a document as pictures on your page. NoteCraft cannot do that
yet; a PDF you attach is kept as a file, and the dialog says so.

Next: [Two page formats](../page-formats/).

---
id: 21b2be1e3e8b89723162
title: "Look inside a page"
order: 75
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Many note taking apps keep a whole notebook in one binary file, and a text editor shows noise. A
NoteCraft page is a folder of plain files, and the code view shows you that folder, file by file.
Try it now: press the `< >` button beside this page.

## What you will see

On the left, **Files in this page**: the page's own file, `index.md`, first, then anything else in
the folder. A file that nothing on the page uses is marked *not used by the page*. On the right is
the file you picked. This is the page you are reading, so the viewer shows its own text.

The page file shows its **head** in grey: the page's id, title, order and dates. You cannot type in
it, so a hand edit cannot break the page's id. Below it is the **body**, which you can edit. Press
the same button again to close the view, and the page is drawn again from what is in the body.

| File | What the viewer shows | Can you type in it? |
|---|---|---|
| The page file, `index.md` or `index.html` | The head, and the body exactly as saved | The body |
| A drawing, `ink.svg` | The SVG text, with the pen's own readings | Yes, with care |
| A picture | The picture | No |
| Other text | Its text | No |
| Anything else | Its name, size and type | No |

A file over 256 KB shows its first part and says so.

## What it shows you

- A page arranged in boxes has a `div` with `class="nc-box"` around each box.
- A drawing is a file, `ink.svg`, and the page's text names it with one line.
- Tags and links are plain text.

Nothing is hidden: if the page shows it, a file in this folder holds it.

Next: [Right to left](../right-to-left/).

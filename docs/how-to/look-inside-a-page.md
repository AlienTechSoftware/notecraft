# Look inside a page

A OneNote section is one binary file. Open it in a text editor and you see noise. A NoteCraft page
is a folder of plain files, and the code view shows you that folder, file by file, so you can see
for yourself what your notes are made of.

![The code view: the files of one page down the left, and the page file with its head and its body on the right](../images/page-source.png)

## Open it and close it

Press the **`< >`** button beside the page. The page steps aside, the files of the page take its
place, and a **Source** tab appears on the ribbon. Press the button again to go back.

While the code view is open the page is not drawn. There is only one copy of the page you can
change at a time, so what you see is what is in the file. When you close the view, NoteCraft reads
the text again, draws the page, and live editing is back.

## What is on the screen

**Files in this page**, on the left, lists everything the page owns:

- the page file, `index.md` or `index.html`, always first, and picked when the view opens;
- `ink.svg`, if you have drawn on the page;
- the pictures and files in `assets`;
- any other file in the page's folder.

The sizes are the sizes on disk. A file that nothing on the page names is marked **not used by the
page**. It is kept, and never drawn.

The viewer, on the right, shows the file you pick:

| File | What you see | Can you type in it? |
|---|---|---|
| The page file | Its **head** in grey: the id, title, order and dates. Below it the **body**: the words, exactly as saved | The body, yes. The head is shown but cannot be typed in, so a hand edit cannot break the page's id |
| A drawing, `ink.svg` | The SVG text. Each stroke is a path, with the pen's own readings beside it | Yes, with care: the pen's readings are in the file, and an edit that breaks them cannot be undone by the pen |
| A picture | The picture | No |
| Other text (`.txt`, `.json`, `.toml` and so on) | Its text | No |
| Anything else | Its name, size and type | No |

A file over 256 KB shows its first part and says so.

## What you can learn from it

This is the head of a Markdown page, as the view shows it:

```text
---
id: 18dff7129d1f602c000c
title: "Format ideas"
order: 10
created: 2026-10-19T15:13:56Z
updated: 2026-10-19T15:13:56Z
tags: []
---
```

- **The head is the page's own record:** its id, title, order, the times it was created and last
  changed, and the settings that travel with it, such as a direction you chose for the page.
- **Boxes are elements.** A page arranged in boxes has a `div` with `class="nc-box"` around each
  box's blocks, with its place in `data-x`, `data-y` and `data-width`. See
  [Arrange a page in boxes](arrange-a-page.md).
- **A drawing is a file.** The page's own text has one line that names `ink.svg`, and the strokes
  are in that file. See [Draw with a pen](draw-with-a-pen.md).
- **Tags and links are plain text** in the body.

## Fixing a page by hand

Because the files are plain, the view is also how you debug a page. Change the body, and the page
is drawn from what you wrote when you close the view. If the text holds markup the editor cannot
model, the page is shown but not written on, and says so. The code view is where you fix it.

## Not built yet

- Drawing the strokes of `ink.svg` in the viewer. It shows the SVG text.
- Deleting a file that the page does not use, from this view. Delete it in your file manager: a
  page is a folder.
- Opening a file with the system from the viewer.

These are listed in [gaps.md](../gaps.md). The design, and how the code view relates to the live
view and the files, is in [the page model](../page-model.md); the files themselves are described in
[the folder format](../folder-format.md).

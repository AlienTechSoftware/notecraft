---
id: b347df57703a4d984908
title: "Two page formats"
order: 50
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

This page is an **HTML** file. The other pages in this section are **Markdown** files. Both are edited the same way, and you cannot tell them apart on the page; the format shows at the top right, beside whether the page is saved.

| Format | File | Good for |
| --- | --- | --- |
| HTML, the built-in default | index.html | Rich layout, and pages that came from somewhere else |
| CommonMark + GFM | index.md | Clean diffs and merges, and reading the file by itself |

## Where the choice is made

1. The format you pick when you create the page.
2. The nearest format set on a section, group or notebook above it.
3. The global setting, the **Default format** box in the top bar.
4. The built-in default, HTML.

A setting changes new pages only. The pages you already have keep their format, which is visible from the file name, so no setting is ever needed to read a page.

## Look at this page's source

Click the `< >` button beside the page. The head of the file carries the page's identity and dates as `<meta name="notecraft:...">` tags; a Markdown page carries the same in its front matter. NoteCraft edits only what is inside `<body>`.

Next: [Sync and backup](../sync-and-backup/).

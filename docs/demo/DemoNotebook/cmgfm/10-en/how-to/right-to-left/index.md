---
id: d2036763a6dcb9ec037e
title: "Right to left"
order: 80
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

Arabic, Hebrew, Persian and Urdu read from the right. NoteCraft draws them that way: the letters
join, a line of them hangs from the right edge, and a line that mixes the two directions is laid
out by the Unicode algorithm, so a price keeps its number the right way round:

السعر 123 ريال

The fonts that make this possible ship with the app, so the line above looks the same on a
machine with no Arabic font installed. Hebrew and Hindi have their own bundled fonts too:

שלום עולם

नमस्ते दुनिया

## Three directions, three controls

| Control | Where | What it sets |
|---|---|---|
| **Page** | **Home**, then **Direction** | Which way the whole page reads. A line with no letters of its own follows it |
| **Paragraph** | The alignment menu in **Basic Text** | Which way the paragraph the caret is in reads |
| **Run** | **More...** in the Direction group | Which way the selection reads, or what you type next |

Page direction is NoteCraft's own; the apps people come from have no such setting, and a
notebook written in Arabic wants one.

## Where the setting is kept

In the text itself. A page direction is `dir: rtl` in the front matter of a Markdown page or a
meta tag in an HTML page. A paragraph direction is one invisible character at its start, the
Unicode right-to-left or left-to-right mark. Both survive any other editor.

The Arabic, Persian, Urdu and Hebrew groups of this notebook are written right to left
throughout. Open one from [the start page](../../../00-start/welcome/) and compare.

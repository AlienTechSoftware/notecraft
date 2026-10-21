# Write right to left

Arabic, Hebrew, Persian, Urdu and the other right-to-left scripts draw in NoteCraft: the
letters join, a line of them reads from the right, and a line that mixes the two directions
is laid out by the Unicode algorithm, so `السعر 123 ريال` keeps its number the right way
round. The caret in such a line is NoteCraft's own: a click lands on the character under the
pointer, and the arrow keys move the way the eye does, not the way the bytes run. The fonts
that make that possible, Noto Naskh Arabic and Noto Sans Hebrew, ship with the app, so a
notebook looks the same on a machine that has no such font installed. (Noto Sans Devanagari
ships too, for Hindi; it reads left to right.)

```text
ملاحظات
مرحبا بالعالم. هذا اختبار للكتابة من اليمين إلى اليسار.
```

## Three directions, three controls

They are three different things, and Word and OneNote keep them apart too:

| Control | Where it is | What it sets | Shortcut |
|---|---|---|---|
| **Page** | **Home**, then **Direction** | Which way the whole page reads. A paragraph with no letters of its own, such as an empty one or `123`, follows it | |
| **Paragraph** | The alignment menu in **Basic Text**, where OneNote keeps it | Which way the paragraph the caret is in reads, whatever the page says | Ctrl+Shift+R, Ctrl+Shift+L |
| **Run** | **More...** in the Direction group | Which way the selection reads, or what you type next if nothing is selected | |

Page direction is NoteCraft's own: OneNote has no such setting, and a notebook written in
Arabic wants one. **More...** opens a compact menu with **Left-to-Right** and
**Right-to-Left** choices for a selected or newly typed run. Windows keeps Alt+Shift for
switching the active input language.

**Auto** on the Page row, and **Direction from the words** in the menu, take the setting off
again and let the words decide: the first letter of the paragraph picks the direction, which
is what the Unicode algorithm does. The two buttons are icons, lines of text with an arrow
under them pointing the way the text runs.

## Several paragraphs at once

A direction belongs to a paragraph, not to a block or an outline. Asking for one with several
paragraphs selected puts it on every paragraph in the selection, which is the same thing a
reader sees and keeps one place to store it. (Selecting across paragraphs is not built yet,
so today the control acts on the paragraph the caret is in; the cascade is already there for
when it is.)

## Where the setting is kept

Nowhere special, which is the point. It is in the text itself:

- A **page** direction is a line in the page's own file: `dir: rtl` in the front matter of a
  Markdown page, `<meta name="notecraft:dir" content="rtl">` in an HTML one.
- A **paragraph** direction is one invisible character at its start, the Unicode
  right-to-left or left-to-right mark (U+200F, U+200E).
- A **run** direction is a pair of invisible characters around it, the Unicode isolates
  (U+2067 or U+2066, then U+2069).

So the direction survives a copy, a sync, and an edit in any other editor, and every program
that knows the Unicode algorithm reads it the same way. `notecraft-cli run page.direction
path=work/notes dir=rtl` does the page one from a script.

## What does not work yet

**A selection across two paragraphs.** Inside one paragraph you can drag, hold Shift with
the arrows, or double click a word, in Arabic as in English. A selection that runs from one
paragraph into the next is not built for any language yet.

**A mirrored window.** The tree stays on the left and the ribbon reads left to right,
whatever the page says. An Arabic interface is its own piece of work.

**The direction of a run.** The Run buttons in **More...** write the right Unicode isolate
into the file, so the text is correct and other editors honour it, but NoteCraft does not yet
take it into account when it draws the line.

[docs/gaps.md](../gaps.md) keeps that list honest, and [ROADMAP.md](../../ROADMAP.md) has the
order of the work.

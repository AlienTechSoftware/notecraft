# The page, its files, the live view and the code view

Read this before you change how a page is stored, edited on screen, or shown as code. It is the
shared picture, so every contributor and every agent works from the same one. Where the code does
not yet match it, that is a gap and belongs in [gaps.md](gaps.md); do not quietly work around it.

Status: written 2026-10-19 from decisions by the owner. Each item says whether it is built.

## 1. Words used here

| Word | Meaning |
|---|---|
| **Page** | One folder. A page is the last level of the notebook tree: a notebook holds sections, a section holds pages, and a page can hold child pages. |
| **Page file** | The page's own text: `index.html` or `index.md`. |
| **Head** | The metadata of the page file: the `<head>` of an HTML page, the front matter of a Markdown one. Title, id, order, times, direction. |
| **Body** | The rest of the page file: the words and structure. This is what a person writes. |
| **Block** | One paragraph, heading, list item, table, picture line and so on. The model the editor works on. |
| **Box** | A container on the page: a place, a width and the blocks it holds. Written as an element in the body ([folder-format.md](folder-format.md) section 3.4). |
| **Ink file** | `ink.svg`: the pen's strokes for the page. |
| **Attachment** | A file added to the page, kept in the page's `assets/` folder: a picture, a document. |
| **Live view** | The page drawn the way it will be read, edited in place. |
| **Code view** | The page's files as they are, with a list and a viewer. Opened by the Source button. |

## 2. The rules

1. **The folder is the page.** Everything the page owns is a file directly in its folder, or in
   its `assets/` folder. That list is flat. Child pages are other pages; they are not in this
   page's list.
2. **The page file is the truth for the page's words and structure.** The order of the blocks,
   the boxes, and the links to every other file are in it. If the live view shows something, the
   page file names it. No layer exists that only the window knows about.
3. **Other files are named by the page file.** A picture or an attachment is a link. The pen's
   drawing is a line that names `ink.svg`. A file in the folder that the page file does not name
   is an **orphan**: it is listed as one, it is never drawn, and it is deleted only by the owner.
   **Ink is the one exception**: an ink file is drawn whether or not the page names it, and the
   page is made to name it (section 5.4). Hiding strokes a person drew because a line was lost
   would be the worse answer.
4. **Metadata lives in the head.** Title, id, order, times and direction. The title is in
   the head alone: a new page does not repeat it in its body, and a page that does - an
   older one, or one written by hand - has that heading left out of the live view, because
   the title area above the page is already showing it. The file is not changed; the code
   view shows it as it is. The one exception today
   is the width of table columns, which is on a metadata line; see [gaps.md](gaps.md).
5. **What the window shows and what the folder holds never disagree.** Open a page, change
   nothing, save it: the folder is the same. Close the window and read the folder: it says what
   the window showed. Each kind of content has a test for this.
6. **A new kind of page content** must (a) be named in the page file, (b) have its reader and
   writer in `notecraft_formats`, so the store, the engine and the window share one, (c) appear
   in the code view's list, and (d) have a round-trip test. Content the page file does not name
   is not finished.

## 3. The live view

- It draws the blocks and boxes that were read from the page file (`notecraft_formats::boxes`).
- An edit changes the blocks. The body is then written again from the blocks and boxes, and saved
  to the page file by the autosave.
- Undo and Redo go through the page's history: whole states of the page (body, blocks, boxes and
  the caret), not a list of edits. Ink has its own undo, because a drawing is its own file.
- Ink is drawn as a layer over the page. Its strokes are in ink files, one or many, and every one
  of them is drawn (decided by the owner, 2026-10-19). The page file names them. Section 5 has the
  detail.
- The live view can only be used when the body survives the trip through blocks and back. If it
  would lose markup, the page is shown read-only and the code view is how it is fixed.

## 4. The code view

Status: built on 2026-10-19, except the two lines marked below. How to use it, for a reader:
[how-to/look-inside-a-page.md](how-to/look-inside-a-page.md). Questions that are still open are
in section 8.

- **A flat list of the page's files**, with a viewer beside it. The page file is first and is
  selected when the view opens.
- **The page file is shown whole.** The head is shown above the body, read-only, so the title,
  the id and every metadata line can be seen. Only the body can be edited. A hand edit cannot
  break the page's id.
- **Files in our own formats can be edited.** HTML and Markdown are the page's native formats, and
  a person debugging or fixing a bug needs to change them (decided by the owner, 2026-10-19). The
  page file is one. Every other file is read-only: ink files, pictures and attachments. A hand
  edit to an ink file would put it out of step with the pen's own readings in it.
- **The viewer follows the kind of file.**
  - HTML and Markdown: shown as text, and editable.
  - Other text: shown as text, read-only.
  - Pictures: shown as pictures.
  - Ink files: shown as SVG text, which **can be typed in** (owner, 2026-10-19, changing the line
    above that every file but the page's own is read-only: a person debugging ink needs it, and the
    view warns that the pen's readings are in that file). Not built: drawing the strokes in the
    viewer with NoteCraft's own painter.
  - Anything else: name, size and type, and a button to open it with the system.
- **Orphans are marked** "not used by the page". Deleting one is on the page itself today, under
  the list of files; not built in this view.
- **A big file shows its first part**, with a note, so the view stays quick. The first 256 KB.

### While the code view is open

The live view is off. The page is not drawn and cannot be edited on the page. The body text in the
code view is the one editable copy, so two editable copies of the page never exist at once.

### When the code view is closed

1. The body is read again: blocks and boxes come from the text.
2. The live view is drawn from them.
3. Live editing is turned back on.

If the text no longer survives the trip through blocks, the live view stays read-only and says why,
with a way back to the code view.

## 5. Ink

What a pen leaves on a page. This section is as complete as the code and the owner's decisions allow.
It says what is built, what is decided but not built, and what is open.

### 5.1 What is built

- **One ink file per page: `ink.svg`**, in the page's folder, beside the page file. It is created the
  first time the page is drawn on. When the last stroke is rubbed out the file is deleted, so a page
  with no ink has no ink file.
- **It is ordinary SVG.** A browser or an editor draws it. Each stroke is one `<path>`. The path's
  `d` is a **filled outline** of the stroke, because SVG has no variable-width stroke: the outline
  is what any other program draws.
- **The pen's own readings are kept beside the outline**, in NoteCraft's namespace
  (`xmlns:nc="https://notecraft.org/ns/ink"`) on the same path: `nc:tool` (`pen`, `pencil` or
  `highlighter`), `nc:width`, `nc:gain` when it is not 1, and `nc:dabs`, a list of `x,y,pressure`
  readings. Other programs ignore an attribute in a namespace they do not know. The readings are
  what lets a stroke be rubbed out in part or smoothed later, and an outline alone cannot do that.
  So every stroke is stored twice: the outline to be seen, the readings to be edited. The weight is
  not shared evenly between the two, which is worth knowing before anybody tries to make ink
  smaller: in the demo notebook's drawing, measured on 2026-10-19, two strokes came to 31 KB, of
  which the outlines were 30.8 KB and the readings 0.6 KB. The outline is finely sampled along the
  curve, two points a sample, each with two decimals. Rounding the outline and sampling it by how
  much the curve actually bends is where the size is, not the readings.
- **Coordinates** are the page body's own, measured from its top left corner, so strokes stay beside
  the words they were written against when the window is resized. The `viewBox` (820 by 1100) is
  only what another program uses to frame the picture; the pen may write past it.
- **The pen writes anywhere on the page**, over the words and between them, not inside a box. Ink is
  a layer over the whole sheet, and highlighter strokes are drawn under the words.
- **Tools:** pen (opaque, the width follows the pressure), pencil (opaque, thin), highlighter
  (see-through and flat), an eraser, and selecting strokes to move, stretch or delete them.
- **Saving:** the ink file is written after each change: a stroke finished, erased, moved or
  deleted.
- **The page names its ink** with one image line (shown below). It is added the first time the page
  is drawn on and taken out when the ink is gone. NoteCraft does not draw that line as a picture; it draws the layer. The
  line is there so the page's words and what is on it agree, and so another program that opens the
  page can see the handwriting.

  ```text
  Markdown   ![Ink](ink.svg)
  HTML       <img src="ink.svg" alt="Ink">
  ```

- **A plain copy:** the Draw tab's Save Picture writes `ink-picture.svg` beside the ink, without the
  readings, to send somewhere else. It is derived from the ink and can be written again, so the code
  view lists it as a derived file and not as an orphan.
- **Older pages** put a drawing in as a picture, `assets/drawing-1.svg`. On opening, those strokes
  are moved onto `ink.svg` and the picture line is taken out. The old file is left where it was,
  because it is the owner's, so it shows up as an orphan until the owner deletes it.
- **Undo for ink** is Undo Stroke on the Draw tab. It takes the last stroke back and has no redo. The
  page's own undo does not include ink, because the drawing is its own file.

### 5.2 Known faults

1. ~~**The page can stop naming its ink.**~~ Fixed on 2026-10-19: the folder decides. The page's
   text is made to name every ink file beside it and no ink file that is gone, when the page opens
   and again after every change to it, so deleting the box that held the line repairs it at once
   rather than leaving the page silent about a drawing that is still there. An SVG counts as ink
   because it carries the ink namespace, not because of its name, so a picture of the owner's own
   is never adopted. `notecraft_store::page::ink_files` lists them; `NoteCraftApp::name_ink` keeps
   the text in step.
2. **Translucent strokes show dark spots at bends.** A stroke is drawn as a strip of triangles that
   each carry the stroke's opacity. Where the strip folds, two triangles overlap and blend twice.
   The highlighter shows it; the opaque pen does not.
3. **Ink has no redo.**
4. **An old `assets/drawing-1.svg` stays behind** as an orphan, by design (see above).

### 5.3 Decided, not built

- **Every ink file is drawn in the live view, one or many** (owner, 2026-10-19). Today a page has
  exactly one.
- **Ink is a file in the page's folder that the page file links to**, in both formats. This is what
  the code does now; the owner asked why not link in both formats and the answer is that it is the
  design (see 5.5).
- **Ink files are read-only in the code view** and are shown as drawn strokes (section 4).

### 5.4 Proposed, for the owner to confirm

- **What an ink file is.** An SVG in the page's folder whose root carries the ink namespace. The
  namespace, not the name, says it is ink, so an ordinary SVG picture is never mistaken for it.
- **Names.** `ink.svg` stays the first. More are `ink-2.svg`, `ink-3.svg` and so on. The page file
  names each with its own image line, in the order they are drawn: the first is the bottom layer.
- **Where a new stroke goes.** Into the ink file that is active, which is the first unless the
  person chose another. The code does not have a choice yet, and the reason for having more than
  one file (separate layers, shapes, strokes that arrived from another device) is not settled.
- ~~**The folder and the page file are kept in step.**~~ Built on 2026-10-19, and it is the repair
  for fault 1. An ink file in the folder that the page does not name is adopted: the page gets the
  line for it. A line that names an ink file that is gone is dropped. Nothing drawn is ever
  unnamed, and nothing named is ever missing. What is still proposed is the rest of many ink
  files: the names `ink-2.svg` and so on, which file a new stroke goes into, and why a page would
  have more than one. The adopting works for any number of them today.
- **Each ink file in the code view** is drawn as strokes first, with its SVG text one click away,
  and shows how many strokes it holds.
- **The fix for fault 2** is to draw a translucent stroke as one shape that does not overlap itself,
  so each pixel is blended once.

### 5.5 Where ink is stored: the open question

The page file could hold only a link, as now. It could hold the strokes inline as an `<svg>` element.
Or a setting could choose, as the owner suggested. What each costs:

| | Link to a file (now) | Inline in the page text | A setting for either |
|---|---|---|---|
| Page text | stays small, one line | carries every stroke | depends on the setting |
| Autosave | rewrites a small file on a text edit, and the ink file only when the pen moves | rewrites the whole file, ink included, on every text edit | as the choice |
| Page history | each step holds the page text only | each step holds the ink too; with up to 200 steps and a 16 MB limit it would run out on a drawn page | as the choice |
| Sync | text and ink are separate files, so an edit to each can merge | one file, so both edits are one conflict | as the choice |
| Source view | the line, plus the file in the list | a very large block of numbers in the text | as the choice |
| A single file that carries its drawing | no | yes | only in inline mode |
| Markdown | works | a raw `<svg>` of that size is poor Markdown, and the reader needs a way to hold it | worse: two code paths |
| What it costs to build | nothing: it is the current design | a reader and writer, and the history to take care of | both, plus converting existing pages when the setting changes |

The recommendation is to keep the link as the one storage form, fix faults 1 and 2, and add a
"save the page as one file" export that inlines the strokes at that moment, which gives portability
without a second way of storing ink. A setting can come later if a need turns up that the export does
not cover. A setting that changes what is written to disk is a larger decision than a display setting.

## 6. Going between them

| Action | Body text | Blocks and boxes | Other files |
|---|---|---|---|
| Edit in the live view | rebuilt from the blocks and boxes | changed | unchanged |
| Draw with the pen | gets the line naming `ink.svg` if it had none | unchanged | `ink.svg` rewritten |
| Add a picture or a file | gets its link | gains the line | the file is copied into `assets/` |
| Delete a box | loses the box and its blocks | box and blocks removed | none touched; the ink and the files stay, and the page must still name the ink |
| Open the code view | unchanged | not used while it is open | listed, none changed |
| Edit the body in the code view | what the person typed | read again on close | unchanged |
| Close the code view | kept | rebuilt from the text | unchanged |
| Undo or Redo | put back from the history | put back with it | not touched |

The page file always names the ink while there is ink. A delete or an undo must not leave ink the
text does not name.

## 7. Decisions

| Decision | When | Built |
|---|---|---|
| Boxes are `div` elements in the text of both formats, with their place in `data-` attributes | 2026-10-19 | yes |
| The page title is a title area inside the page, like OneNote's, stored as the page's title metadata | 2026-10-19 | yes |
| The code view lists the page's files and has a viewer, and closing it renders the page and turns live editing on | 2026-10-19 | yes |
| A drawing's SVG can be typed in in the code view | 2026-10-19 | yes |
| Every ink SVG on a page is drawn in the live view, one or many | 2026-10-19 | no: one file today |
| Files in our own formats (HTML, Markdown) are editable in the code view, to debug and fix bugs; every other file is read-only | 2026-10-19 | no |
| Ink is a file the page links to, not inline text; a setting is being weighed | current behaviour | yes, with the faults in 5.2 |
| The folder decides what ink a page has: the page's text is made to name every ink file beside it, and none that is gone | 2026-10-19 | yes |

Known faults are listed in 5.2.

## 8. Open questions

Answered on 2026-10-19: ink files are drawn in the live view, one or many, and the code view draws
them as strokes; files in our own formats are editable in the code view and every other file is
read-only.

1. **History and the code view.** Proposed: a visit to the code view is one step in the page's
   history, taken when it is closed after a change, so Undo takes back the whole visit. The
   alternative is a step for every edit made in the code view.
2. **Ink storage.** Link only, a setting, or inline only. Section 5.5 has the table and the
   recommendation.
3. **Many ink files.** Why a page would have more than one, how the person picks the active one, and
   whether the names in 5.4 are right.
4. ~~**Adopting an unnamed ink file**~~ Answered by building it on 2026-10-19: yes, the page is
   changed to name an ink file it did not, because the alternative is strokes that are drawn and
   unnamed. Say so if you would rather it were the other way.
5. **Ink undo.** Whether Undo Stroke should have a redo, and whether ink should join the page's own
   history.
6. **Whether the width of table columns** moves from a metadata line to the markup of the table.
7. **An Options window** for settings that today are spread around.

## 9. Where it lives in the code

| Part | Where |
|---|---|
| Reading and writing a body with boxes | `crates/formats/src/boxes.rs` |
| Page file, head and body, boxes, direction | `crates/store/src/page.rs` |
| Pictures and attachments | `crates/store/src/assets.rs` |
| Blocks drawn and edited | `crates/ui-egui/src/pageedit.rs`, `containers.rs`, `preview.rs` |
| Ink | `crates/ink`, `crates/ui-egui/src/inkpad.rs` |
| The editor state and the source pane | `crates/ui-egui/src/app.rs` |
| Undo and Redo of the page | `crates/ui-egui/src/history.rs` |
| The code view | `crates/ui-egui/src/codeview.rs` |
| Which files of a page are ink | `crates/store/src/page.rs` (`ink_files`), `crates/ink/src/svg.rs` (`is_ink`) |

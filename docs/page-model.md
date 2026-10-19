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
4. **Metadata lives in the head.** Title, id, order, times and direction. The one exception today
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
- Ink is drawn as a layer over the page. Its strokes are in `ink.svg`; the page file names it.
- The live view can only be used when the body survives the trip through blocks and back. If it
  would lose markup, the page is shown read-only and the code view is how it is fixed.

## 4. The code view

Status: agreed in principle by the owner; not built. Questions that are still open are in
section 7.

- **A flat list of the page's files**, with a viewer beside it. The page file is first and is
  selected when the view opens.
- **The page file is shown whole.** The head is shown above the body, read-only, so the title,
  the id and every metadata line can be seen. Only the body can be edited. A hand edit cannot
  break the page's id.
- **Other files are read-only.** A hand edit to `ink.svg` would put it out of step with the pen's
  own data in it.
- **The viewer follows the kind of file.**
  - Text and markup: shown as text.
  - Pictures: shown as pictures.
  - `ink.svg`: drawn as strokes by NoteCraft's own painter, with its text one click away.
  - Anything else: name, size and type, and a button to open it with the system.
- **Orphans are marked** "not used by the page", and can be deleted from the list.
- **A big file shows its first part**, with a note, so the view stays quick.

### While the code view is open

The live view is off. The page is not drawn and cannot be edited on the page. The body text in the
code view is the one editable copy, so two editable copies of the page never exist at once.

### When the code view is closed

1. The body is read again: blocks and boxes come from the text.
2. The live view is drawn from them.
3. Live editing is turned back on.

If the text no longer survives the trip through blocks, the live view stays read-only and says why,
with a way back to the code view.

## 5. Going between them

| Action | Body text | Blocks and boxes | Other files |
|---|---|---|---|
| Edit in the live view | rebuilt from the blocks and boxes | changed | unchanged |
| Draw with the pen | gets the line naming `ink.svg` if it had none | unchanged | `ink.svg` rewritten |
| Add a picture or a file | gets its link | gains the line | the file is copied into `assets/` |
| Delete a box | loses the box and its blocks | box and blocks removed | none touched; the ink and the files stay |
| Open the code view | unchanged | not used while it is open | listed, none changed |
| Edit the body in the code view | what the person typed | read again on close | unchanged |
| Close the code view | kept | rebuilt from the text | unchanged |
| Undo or Redo | put back from the history | put back with it | not touched |

The page file always names the ink while there is ink. A delete or an undo must not leave ink the
text does not name.

## 6. Decisions

| Decision | When | Built |
|---|---|---|
| Boxes are `div` elements in the text of both formats, with their place in `data-` attributes | 2026-10-19 | yes |
| The page title is a title area inside the page, like OneNote's, stored as the page's title metadata | 2026-10-19 | no |
| The code view lists the page's files and has a viewer, and closing it renders the page and turns live editing on | 2026-10-19 | no |
| Ink is a file the page names, not inline text | current behaviour; the owner is weighing a setting | yes, with a known fault below |

Known fault: when the box that holds the line naming the ink is deleted, the page file stops naming
the ink while the ink is still drawn. It must be named again whenever it is missing.

## 7. Open questions

1. Does `ink.svg` open as drawn strokes first, with its text one click away?
2. Are non-page files read-only, or can they be edited too?
3. Is a visit to the code view one step in the page's history, or is every edit in it a step?
4. Where ink is stored: only as a linked file, a setting that also allows inline, or inline only.
   Inline puts the pen's data in the text, so every autosave and every history step carries it.
   A "save the page as one file" export would give portability without a second storage form.
5. Whether the width of table columns moves from a metadata line to the markup of the table.
6. An Options window for settings that today are spread around.

## 8. Where it lives in the code

| Part | Where |
|---|---|
| Reading and writing a body with boxes | `crates/formats/src/boxes.rs` |
| Page file, head and body, boxes, direction | `crates/store/src/page.rs` |
| Pictures and attachments | `crates/store/src/assets.rs` |
| Blocks drawn and edited | `crates/ui-egui/src/pageedit.rs`, `containers.rs`, `preview.rs` |
| Ink | `crates/ink`, `crates/ui-egui/src/inkpad.rs` |
| The editor state and the source pane | `crates/ui-egui/src/app.rs` |
| Undo and Redo of the page | `crates/ui-egui/src/history.rs` |

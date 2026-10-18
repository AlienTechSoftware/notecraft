# MS-ONE: concrete notes (paraphrased)

Source: [MS-ONE] OneNote File Format, v20221115 (revision 3.4), PDF, 108 pages.
Read: 2026-10-17. Text was extracted locally from the PDF; section numbers are
the spec's. Paraphrased. Constants (JCIDs, property IDs, GUIDs, numeric ranges)
are recorded as data. Not read in detail: the worked example (section 3.1),
the paragraph-style and formatting sections (2.2.43 to 2.2.45), and the
per-property tables in 2.3.

Companion: `ms-onestore-notes.md` describes the container. This file says what
the objects in it mean. Same licence notice applies (Microsoft permits copies to
develop implementations; read the patent notice before shipping).

## Big picture

- MS-ONE is a persistence format for notes as hierarchical sets of sections and
  pages, stored in a revision store file.
- Everything is properties, property sets and file data objects (section 1.3.3).
  A property set is one object. Each object has a JCID that says what it is.
- The file's own types and properties are identified by numbers (JCID,
  PropertyID). The spec uses names for them. Any reader must carry these
  tables.
- Rule for readers (2.1.12, 2.1.13): ignore any property, property set or JCID
  you do not know, and write it back unchanged. This is how newer and older
  OneNote versions coexist.
- Schema versions are `SchemaRevisionInOrderToRead` and `...ToWrite` (2.3.69,
  2.3.97). A reader that meets other schemas must leave them alone.

## Hierarchy as stored

| Level | Where | JCID |
|---|---|---|
| Notebook | A `.onetoc2` file plus the folder of `.one` files | `0x00020001` (TOC container) |
| Section | One `.one` file, its root object space | `jcidSectionNode` `0x00060007` |
| Page series | Child of the section; one per top-level page and its subpages | `jcidPageSeriesNode` `0x00060008` |
| Page | Its own object space (PageObjectSpace) | `jcidPageNode` `0x0006000B` |
| Outline | On a page | `jcidOutlineNode` `0x0006000C` |
| Outline element | In an outline | `jcidOutlineElementNode` `0x0006000D` |
| Content | Rich text, image, table, embedded file | see below |

- **Notebook (1.3.4, 2.2.14, 2.2.15):** the `.onetoc2` file lists the notebook's
  children: sections and subfolders of the notebook's folder. Each section entry
  has the section's file identity GUID, an ordering ID, a file name and a color.
  The notebook has a color and an `EnableHistory` flag. Section groups are
  therefore real subfolders of the notebook folder.
- **Section order and names** live in the notebook's `.onetoc2`, not in the
  `.one` files. The section itself stores its pages and the order of pages.
- **Section object space (2.1.14):** the root of a `.one` file. Default content
  root is the section node. Metadata root is `jcidSectionMetaData` (`0x00020031`).
- **Page object space (2.1.10):** default content root is a page manifest node
  (`0x00060037`); metadata root is `jcidPageMetaData` (`0x00020030`).

## Subpages and the 3-level limit

- Pages in a section are not nested. A section holds a flat ordered list of
  pages (the "page series"). Subpages are encoded by a number, `PageLevel`
  (2.3.74), stored in each page's metadata.
- `PageLevel` is a signed integer that MUST be from 1 through 3. 1 means no
  indent. The first page of a series MUST be 1. This is the source of OneNote's
  limit of three page levels: it is a rule of the file format, not only of the
  user interface.
- Consequence for NoteCraft: folder nesting has no such limit, but an importer
  must flatten OneNote's flat list plus levels into a folder tree. A page at
  level 2 or 3 becomes a child of the nearest earlier page with a lower level.
  This mapping is my reading; it is not stated in the spec.

## Page

- Fields on a page (2.2.19): last modified time, child elements (outlines,
  images, embedded files), a structure child (the title), orientation, page
  width and height, four margins, margin origin, a cached title string, author,
  a flag for right-to-left layout, read-only, deletable, conflict-page flag, and
  the audio and video recordings on the page.
- Sizes are in half-inch increments (width, height, margins).
- Pages have a creation timestamp on the page series (2.2.18) and a last
  modified time on the page.
- A **title** (2.1.16) is a title node plus one or two special outlines flagged
  as title text, title date and title time. The title string is also cached in
  page metadata.
- Conflict pages and version history pages exist as additional page object
  spaces (2.1.2, 2.1.17, 2.1.18). Version history is stored per page.

## Outlines and outline elements

- An **outline** (1.3.2.1) is the container for text, lists, tables and images.
  It can sit anywhere on the page and can overlap other outlines. This is the
  "free-form canvas": position comes from offset properties on the outline
  (`OffsetFromParentHoriz`, `OffsetFromParentVert`, measured from the page
  origin) and size from layout max width and height.
- An outline has an indent structure (`EnforceOutlineStructure`,
  `OutlineElementChildLevel`, `RgOutlineIndentDistance`), alignment, and a
  minimum width. The indent here is for the bullets and text inside an outline,
  separate from page levels.
- An **outline element** (2.2.21) is one paragraph-level row. It holds child
  elements (nested outline elements, for indentation), content children (the
  actual text, image, table or file), a list-nodes child (bullet or number),
  authors (original and most recent) and a creation time.
- An **outline group** (2.2.22) adds an extra indent level where the first
  element of an outline is indented more than a later one.

## Text

- Text lives in a rich text node (`jcidRichTextOENode`, `0x0006000E`) as a
  Unicode string (`RichEditTextUnicode`, no line feed characters) or an
  extended-ASCII string. Unicode wins if both exist.
- **Text runs:** a paragraph is split into runs. `TextRunIndex` lists the
  character positions where each run ends (number of runs = positions + 1).
  `TextRunFormatting` lists one formatting object per run. Character positions
  follow the same rules as in the Word binary format.
- **Run formatting** is a separate object per run (paragraph style object for
  text). Properties include bold, italic, underline (with an underline type),
  strikethrough, superscript, subscript, font, font size, colors, highlight,
  language, hidden, hyperlink and hyperlink display text, and a math flag
  (`MathFormatting`, a boolean "this run is a mathematical expression").
- **Paragraph style** covers alignment, space before and after, line spacing and
  a style ID. Text language is an LCID.
- **Hyperlinks:** a run can be flagged as a hyperlink, and a second flag marks
  the run that is the display text of a link. Where a text run's target URL is
  stored was not found in the parts read. (`WzHyperlinkUrl` is documented for
  images.)
- **Lists:** a number-list node (2.2.25) per list item. If its format string
  contains the character U+FFFD it is a numbered list; otherwise bulleted. It
  holds the bullet font and size, color, restart number and language.
- Math: only a boolean flag on a text run is documented. No equation structure
  was found in the parts read.

## Tables

- Table node (`0x00060022`): row and column counts, column widths, locked
  columns, borders visible, alignment, tags.
- Row node (`0x00060023`) holds cell nodes (`0x00060024`). A cell contains
  outline elements. A cell's indent level is fixed (outline structure enforced,
  child level 1).
- Cell shading color exists (2.2.97).

## Images

- Image node (`0x00060011`): picture container (reference to binary data), max
  width and height, "size set by user" flag, language ID for OCR, alt text, file
  name, hyperlink, displayed page number, text for the image (OCR or caption),
  upload state, picture width and height, tags.
- An image directly on a page (not inside an outline element) has an explicit
  offset from the page origin and may be flagged as a background image.
- **Picture data** is a file data object (JCID `0x00080039`). Allowed formats:
  DIB, EMF, JPEG, PNG, TIFF, WMF.
- `ImageUploadState` marks incomplete uploads. The picture container must be
  present only when the upload is complete.

## Embedded files, audio and video

- Embedded file node (`0x00060035`): an icon (picture container), the file's
  name and source path, a text representation, language, size and position, and
  the file data reference.
- File data object (JCID `0x00080036`) holds the bytes. It can live inline or in
  an external `onefiles` folder (per MS-ONESTORE).
- Audio and video are embedded files with an extra recording GUID, duration in
  milliseconds and a media type: 1 = audio (wma, mp3, wav), 2 = video (wmv, avi,
  mpg). The page also lists all recording GUIDs.

## Ink

- **MS-ONE does not describe ink.** A search of the full extracted text found no
  ink, stroke, handwriting or drawing structure. The only "Pen" hit is an icon
  name in the note tag table.
- So the stroke data, shapes and ink-to-text results of OneNote pages are not
  documented in either specification read so far. Whether such data is stored
  inside the revision store as unknown property sets (which the spec says to
  preserve) or elsewhere, is not answered by these documents. Needs a test file
  with ink and a look at how existing parsers treat it.

## Tags

- **Note tag (2.1.9):** a label with an optional icon attached to a paragraph,
  image, table or file. Two kinds: normal tags and task tags.
- Tag definitions are shared objects (`jcidNoteTagSharedDefinitionContainer`,
  `0x00120043`) and each tagged object has a state (`NoteTagStates`).
- **Identifiers:** `ActionItemType` is 0 to 99 for normal tags. For task tags it
  is 100 to 105: due today, tomorrow, this week, next week, no due date, custom
  date.
- **Icons:** `NoteTagShape` selects the icon. Values 1 to 12 are checkable check
  boxes in color and shape variants. Values 13 and up are non-checkable icons
  (star, flag, question mark, priority, contact, calendar, time-sensitive,
  light bulb, pushpin, home, comment, smiley and more). Task tags use shapes 89
  to 93.
- **State:** a checkable tag is complete or not. Completion time is stored. A
  non-checkable tag stores completed = created time. Tags can have font and
  highlight colors (normal tags only). Task tags have a due date.

## Other

- **Authors:** original and most recent author objects.
- **Timestamps:** creation (a Time32 and a longer timestamp) and last modified.
- **Conflict objects:** objects produced when two edits collide are flagged
  (`IsConflictObjectForRender` and `...ForSelection`) and pages with conflicts
  are tracked under the page manifest.
- **Right-to-left:** flags on pages, outline elements and reading order.
- **Read-only, deletable, cannot-be-selected, cannot-be-moved** flags exist on
  several node types.

## What this means for NoteCraft (proposals, not decisions)

| OneNote concept | Plain-file mapping idea |
|---|---|
| Notebook, section group | Folder |
| Section | Folder |
| Page and subpages | Folder with page content and child folders; page level from the flat list becomes folder nesting |
| Outline, outline element | Blocks inside the page; position metadata only for free-form pages |
| Rich text runs | Markdown inline formatting, or HTML spans |
| Table | Markdown table, or HTML if cells hold block content |
| Image, audio, video, file | Separate files in the page's folder, linked by relative path |
| Note tag | Inline tag syntax or front matter (check boxes become task list items) |
| Ink | Not documented in the specs. Import needs more research. |
| Math | A flag on a run only. Import likely loses the equation structure. |
| Version history, conflict pages | Git or file history; conflict pages are usually dropped |

## Open questions

- Ink: where is it stored and how is it encoded?
- Equations: structure beyond the boolean flag.
- Password-protected sections: encryption algorithm (MS-ONESTORE encryption key
  node is known, details unread).
- Exact mapping of `PageLevel` to a tree when levels skip (1, 3).
- Whether other parsers (`onenote_parser`, `one2html`) already cover these.

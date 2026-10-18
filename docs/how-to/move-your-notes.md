# Move your notes in

Short version: **not yet**. The reader exists, the importer does not. This page says exactly
how far it has got, so you know what to expect and what to keep.

NoteCraft reads `.one` files, the section files the Office note taking app writes on Windows.
That is the only import format so far.

## What NoteCraft can read today

`crates/onenote` reads a `.one` section file:

- The container: the header, the file node lists, the transaction log, object spaces, objects
  and their property sets, and the bytes of pictures and attachments kept inside the file
  ([MS-ONESTORE]).
- The objects: the section, its pages with their titles and page levels, outlines and outline
  elements, text with its runs and their bold, italic, underline, strikethrough and link marks,
  bullets and numbering, tables, pictures and embedded files ([MS-ONE]).

What it cannot read: password-protected sections (it says so rather than guessing), ink, and
anything Microsoft does not document. Notebook structure (`.onetoc2`) is not read yet, so
section order and colours would have to be rebuilt from folder and file names.

## What is still missing

Turning that into folders: a notebook per backup folder, a section per `.one` file, pages and
sub-pages from the page levels, text into HTML or Markdown, pictures and attachments into each
page's `assets` folder. That work is next on the [roadmap](../../ROADMAP.md).

## Where those files are

The desktop app keeps backups of every section here:

```text
%LOCALAPPDATA%\Microsoft\OneNote\16.0\Backup\
```

Those are ordinary `.one` files and are the best source: no sign-in, no network, no API. The
`cache` folder beside it is a different, undocumented format, and notebooks that live only in
OneDrive may not appear in `Backup` at all. Microsoft's Graph API can reach those, but it needs
a signed-in app, which NoteCraft does not have.

Keep your backups until the importer lands and you have checked its work. Nothing in NoteCraft
writes to them.

## Export, the other direction

Exporting a NoteCraft notebook back to `.one` is not planned. Your notes are already plain
files: copy the folder.

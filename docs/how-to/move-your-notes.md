# Move your notes in

NoteCraft imports the section files (`.one`) the Office note taking app writes on Windows. It
reads them, never changes them, and writes a tree of plain folders. Import only: exporting back
to `.one` is not planned, and your notes are already plain files you can copy.

## Quick start

```text
notecraft-cli import onenote --dry-run          show what would be imported
notecraft-cli import onenote                    import everything it finds
notecraft-cli import onenote D:\my\backups --to D:\NoteCraft --format cmgfm
```

With no source, on Windows it looks in the desktop app's backup folder:

```text
%LOCALAPPDATA%\Microsoft\OneNote\16.0\Backup\
```

Anywhere else, give it a folder of `.one` files. It searches all the way down. One `.one` file
works too.

## What you get

- The first folder under the source becomes a **notebook**. Folders below it become **section
  groups**. Each `.one` file becomes a **section**.
- Each page becomes a **page folder**. A sub-page becomes a folder inside its page, so a
  sub-page of a sub-page of a sub-page is three folders deep. The file format stops at three
  levels; NoteCraft does not stop there, it just has nothing deeper to import.
- Text keeps bold, italic, strikethrough and links. Bullet and numbered lists keep their depth.
  Tables become tables. Pictures and attached files are stored in the page's `assets` folder and
  linked by a relative path.
- A page whose text mostly reads right to left is marked as a right to left page. Names and text
  in Arabic, Hebrew, Persian and Urdu come through as they are.
- `--format html` (the default) or `--format cmgfm` chooses what every page is written as.

## Options

| Option | What it does |
|---|---|
| `--to FOLDER` | The notebooks folder to write into. Default: `--notebooks`, then the usual one. |
| `--format html\|cmgfm` | The page format to write. Default `html`. |
| `--all-versions` | The backup folder keeps many dated copies of each section. By default only the newest is imported. This adds the older ones, named with their date. |
| `--existing skip\|keep\|replace` | When a section is already in the destination. `skip` (default) leaves it, so running twice changes nothing. `keep` imports beside it. `replace` deletes it and imports again. |
| `--only WORD` | Only notebooks, groups or sections whose name contains the word. Repeat it for more. |
| `--dry-run` | Plan and report, write nothing. |

The command ends with a report: how many sections, pages, sub-pages, pictures, files and tables
were imported, what was left out and why, and what could not be carried over.

## What does not come across

- **Password protected sections.** They are encrypted. They are skipped and counted.
- **Ink.** Handwriting is not read yet.
- **Underline.** Neither page format has an underline mark here. The words stay.
- **Equations.** The file marks a run as math but keeps no structure, so it arrives as text.
- **Pictures that were never stored.** Some pictures were only ever in the cloud, and the backup
  does not hold their bytes. They are counted in the report, not invented.
- **Windows metafile and TIFF pictures.** A browser cannot show them. They are kept as files.
- **Section order and colours.** They live in the notebook's table of contents (`.onetoc2`),
  which the backups do not contain and the importer does not read yet. Sections come in by name.
- **Page layout.** A page is a free canvas of boxes. Boxes come in as blocks in reading order.
- **Tags, creation dates and authors.** Not read yet.

## Keep your backups

Nothing in NoteCraft writes to them. Keep them until you have looked through the result.

The `cache` folder beside `Backup` is a different, undocumented format and is not read. A
notebook that lives only in the cloud may not appear in `Backup` at all.

## How it works, for the curious

`crates/onenote` reads a section file into pages (see [architecture](../architecture.md)).
`crates/import` finds the files, picks the newest copy of each section, turns each page into the
same blocks the editor uses, and asks the store to make the folders. The notes on what real files
look like, and where they differ from the published description, are in
[`docs/research/onenote-file-format/real-files.md`](../research/onenote-file-format/real-files.md).

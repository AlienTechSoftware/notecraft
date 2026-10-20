# What real desktop backups look like

Date: 2026-10-19
Status: DRAFT. Observations from running the reader over a private set of 443 real section
files (about 2.6 GB, written by a desktop app over about fourteen years). Structure and counts
only. No names, text or identifiers from those files are recorded here, and the files stay off
the repository.

Where this differs from the paraphrased specification in the other notes, this page says so.
The reader follows what real files do, and each point below has a test or a counted result.

## Method

`crates/onenote/examples/survey.rs` reads every `.one` file under a folder and prints only
counts: how many files read, pages, blocks, pictures, errors by kind. Run it with
`cargo run --release -p notecraft-onenote --example survey -- <folder>`.

## Results

| Step | Files read | Pages found |
|---|---|---|
| Reader as first written, from the specification alone | 313 of 443 | 13 |
| After the fixes below | 442 of 443 | 9,306 |

The one file left is empty (0 bytes). Of the files that read, 46 give no pages: 23 are password
protected, and the rest are sections with no pages in them.

## Where real files differ from the specification, and what the reader does

1. **A page's content is in role 4 revisions.** The specification makes the revision with role 1
   the one an application opens. In real files, role 1 holds a small version history node
   (object type 0x3C) and the page's own objects are declared in role 4 revisions, one per edit.
   The reader takes objects from every revision in order, the last declaration winning, and takes
   roots from role 1 only. Skipping role 4 lost most of the content.
2. **A page manifest is not always the content root.** Because of point 1, the root object of a
   page's space is the version history node. The reader finds the manifest (type 0x37) by kind.
3. **Compact ids are scoped to the object group that declared them.** Each group has its own
   global id table, and groups reuse small indexes for different GUIDs. One merged table per
   space sent references to the wrong objects, and most pages came out empty. The reader now
   rewrites every reference in a property set, as it reads it, to an index that is unique in the
   space.
4. **The inserted file footer is counted from the start of the structure.** The specification
   pads the structure to a multiple of 8 bytes. The structure can start at an odd offset in the
   file, so the footer is at the start plus the padded length, not at the next multiple of 8 in
   the file. Reading it the other way failed 105 files.
5. **File node list fragments can end without a terminator.** A fragment that is full has no
   terminator node, and its footer still names the next fragment. Stopping there lost pictures
   and attachments in many files (about 2,100 pictures in this set) and also pages.
6. **Padding at the end of a fragment is not always zero.** Fewer than 8 bytes of anything left
   after the last node is padding, not a node.
7. **One-byte text is its own property.** `TextExtendedAscii` holds one byte per character.
   Reading it as UTF-16 turns English into unrelated characters. The reader decodes it as
   Windows-1252.
8. **Some backups do not declare the section node.** The page series are there. The reader
   takes them in creation order and flags the section as recovered.
9. **Object limits.** With every revision read, an 85 MB section holds about a million objects.
   The object limit is 4 million.
10. **A property set that cannot be read costs one object**, not the file. The count is kept on
    the object space.
11. **Hyperlinks can be field codes in the text.** The marker U+FDDF, the word `HYPERLINK`, and
    an address in quotes, with the linked words in the run after it. This is not in the notes
    on the object model. The importer turns it into a link on those words.
12. **Some file references are `<invfdo>`.** The bytes of that picture or file are not in the
    file. They are reported as missing.

## Not checked

- What role 4 revisions mean exactly, and whether the last one is always the current page.
- Ink, tags, page dates and authors.
- The notebook table of contents (`.onetoc2`), which backups do not contain.
- Files from other versions of the desktop app, and `.onepkg` packages.

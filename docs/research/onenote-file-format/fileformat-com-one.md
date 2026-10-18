# docs.fileformat.com: .ONE file format (summary)

Source: https://docs.fileformat.com/note-taking/one/
Read: 2026-10-17. Paraphrased by the assistant. A third-party page; it points to
Microsoft's MS-ONESTORE as its own source. Not checked against the spec.

## What it is

- `.one` files are OneNote section files.
- A page can hold text, handwriting, and objects pasted from other apps: images,
  drawings, audio and video clips.
- Elements sit at free positions on the page, not in a fixed flow.

## Content model

- **Notebook:** a set of section files in one folder. Notebook properties hold
  settings such as section order and color.
- **Section:** the top-level container. It holds pages and metadata (name, list
  of pages, their order).
- **Page:** holds text, lists, tables, titles, images and note tags. It can be
  named and can have sub-pages.
- **Property set:** a group of properties that describes one kind of content.
- **File data object:** a binary blob for a picture, embedded file, or audio or
  video.

## Physical layout (revision store)

- The file starts with a fixed header. The rest is divided into blocks, and a
  field says where each block is and how big it is.
- A block that nothing reachable points to is ignored.
- Structures are byte-aligned. Integers are signed unless stated. Fields are
  little-endian unless stated.
- Header contents: a file-type GUID (separates `.one` from `.onetoc2`), a file
  identity GUID, a fixed format GUID, version codes (0x2A for `.one`, 0x1B for
  `.onetoc2`), references to the free chunk list, transaction log, root of the
  file node list and the hashed chunk list, an ancestor GUID that links to the
  table-of-contents file, a CRC of the file name, the expected file length,
  version GUIDs, and build numbers that readers should mostly ignore.

## Related format

- `.onetoc2`: the notebook's table-of-contents file.

## Not covered by the page

- Compression or encryption.
- How to open or convert a `.one` file.
- A version history of OneNote itself.

## Relevance to NoteCraft

- Confirms the structure NoteCraft wants to flatten: notebook, section, page,
  sub-page, plus binary attachments stored as separate objects.
- A section file is one container for many pages. NoteCraft's folder-per-node
  layout is the opposite, so an importer must unpack a section into folders.
- The page offers no byte-level detail. An importer needs MS-ONESTORE and MS-ONE.

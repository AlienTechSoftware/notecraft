# MS-ONESTORE: concrete notes (paraphrased)

Source: [MS-ONESTORE] OneNote Revision Store File Format, v20250520
(revision 13.3, released 2025-05-20), PDF, 101 pages. Read: 2026-10-17. Text was
extracted locally from the PDF; section numbers below are the spec's. Notes are
paraphrased. Facts such as GUIDs, magic numbers and field sizes are quoted as
data. Not yet read: sections 2.5 (per-node layouts), 2.7 and 2.8 (the
FSSHTTP-based encodings) and 3.x (the worked examples).

Licensing: the spec's own notice says Microsoft copyrights the document, and
that copies may be made to develop implementations. The notice also says
Microsoft patents might cover implementations and that the Open Specifications
Promise may apply. Check the promise text before shipping.

## Answer to the compression question

- The spec describes no compression of data. The word "compressed" appears
  only for file node references: a pointer or length stored in 2 or 1 bytes is
  multiplied by 8 to get the real value (section 2.4.3). That is size packing,
  not LZ.
- "LZ" does not occur in the spec text.
- Integrity uses CRCs (section 2.1.2). Encryption exists as an optional feature
  of an object space (see Encryption).

## Big picture (section 1.3, 2.1)

A `.one` or `.onetoc2` file is a revision store: a small database in one file.

- **Object space:** a set of objects that can be revised independently. Every
  file has exactly one root object space. A section file's pages live in further
  object spaces, each referenced from the root list.
- **Object:** identified by an ExtendedGUID (a GUID plus a 32-bit number). It
  holds a property set, or a file data object. Declaring the same identity again
  revises it.
- **Revision:** an immutable snapshot of an object space: chosen root objects
  and the objects reachable from them. A revision manifest can say "same as
  revision X, except these objects".
- **Revision manifest list:** all revisions of one object space, labelled with
  revision roles and contexts.
- **Object group:** a list of objects, each with an identity and, optionally, a
  property set.
- **Global identification table:** maps compact indexes to GUIDs, so objects can
  be referenced with 4-byte CompactIDs instead of full ExtendedGUIDs.
- **Reference counts:** every object carries one. Objects not reachable from a
  root object are garbage.

## Physical layout (section 2.3)

- Everything is byte-aligned and little-endian. Only blocks reachable from the
  header or from another reachable block count. Unreachable bytes are ignored,
  which is why a file can hold stale data.
- **Header:** the first 1024 bytes. Field order and offsets, derived from the
  spec's sizes:

| Offset | Size | Field | Notes |
|---|---|---|---|
| 0 | 16 | guidFileType | `.one` = {7B5C52E4-D88C-4DA7-AEB1-5378D02996D3}; `.onetoc2` = {43FF2FA1-EFD9-4C76-9EE2-10EA5722765F} |
| 16 | 16 | guidFile | identity of this file |
| 32 | 16 | guidLegacyFileVersion | must be zero |
| 48 | 16 | guidFileFormat | {109ADD3F-911B-49F5-A5D0-1791EDC8AED8} |
| 64, 68, 72, 76 | 4 each | ffvLastCodeThatWroteToThisFile and three more | 0x2A for `.one`, 0x1B for `.onetoc2` |
| 80 | 8 | fcrLegacyFreeChunkList | zero |
| 88 | 8 | fcrLegacyTransactionLog | nil |
| 96 | 4 | cTransactionsInLog | number of committed transactions, never zero |
| 100 to 127 | 28 | legacy and flag fields | ignored |
| 128 | 16 | guidAncestor | the table-of-contents file's guidFile: same folder for `.one`, parent folder for `.onetoc2`; zero = none |
| 144 | 4 | crcName | CRC of the file name |
| 148 | 12 | fcrHashedChunkList | stp 8 bytes, cb 4 bytes |
| 160 | 12 | fcrTransactionLog | |
| 172 | 12 | fcrFileNodeListRoot | |
| 184 | 12 | fcrFreeChunkList | |
| 196 | 8 | cbExpectedFileLength | file size |
| 204 | 8 | cbFreeSpaceInFreeChunkList | |
| 212 | 16 | guidFileVersion | changes with each commit |
| 228 | 8 | nFileVersionGeneration | |
| 236 | 16 | guidDenyReadFileVersion | |
| 252 to 295 | 44 | debug fields, build numbers | ignored |
| 296 | 728 | rgbReserved | zero |

- **File chunk reference:** a pointer (`stp`, byte offset from file start) and a
  size (`cb`). Variants by field width: 32/32, 64/64, 64/32 (header). "Nil" has
  all stp bits set and cb 0; "zero" has both zero.
- **Free chunk list:** unused blocks, as linked fragments with a CRC.
- **Transaction log:** records how many nodes of each file node list are valid.
  A write appends nodes, appends log entries, then bumps `cTransactionsInLog` in
  the header. Anything past the committed count is an unfinished write.
- **Hashed chunk list:** read-only objects that several revisions can share.

## File node lists (section 2.4)

- Every list is stored as one or more **fragments**. A fragment starts with a
  16-byte header: magic `0xA4567AB1F5F7F4C4` (8 bytes), list ID (4 bytes, at
  least 0x10), fragment sequence (4 bytes). It ends with a 12-byte reference to
  the next fragment and an 8-byte footer `0x8BC215C38233BA4B`.
- A **file node** starts with a 32-bit word: FileNodeID (10 bits), Size (13
  bits), StpFormat (2), CbFormat (2), BaseType (4), one reserved bit that is
  always 1.
  - BaseType 0: no reference. 1: references data. 2: references another file
    node list.
  - StpFormat: 8-byte, 4-byte, 2-byte compressed, 4-byte compressed. CbFormat:
    4-byte, 8-byte, 1-byte compressed, 2-byte compressed. "Compressed" means
    multiply by 8.
- A list ends at a chunk terminator node (0x0FF), when fewer than 4 bytes
  remain, or when the transaction log's node count is reached.
- Node types seen in `.one` files (IDs in hex): 0x004 ObjectSpaceManifestRoot,
  0x008 object space list reference, 0x00C list start, 0x010 revision manifest
  list reference, 0x014 list start, 0x01E and 0x01F revision manifest start,
  0x01C revision manifest end, 0x022 global ID table start, 0x024 entry, 0x028
  end, 0x0A4 and 0x0A5 object declarations (normal and large ref count), 0x0C4
  and 0x0C5 read-only object declarations, 0x072 and 0x073 file data
  declarations, 0x0B0 object group list reference, 0x084 dependency overrides,
  0x05A root object reference, 0x07C encryption key, 0x0FF chunk terminator.
  The `.onetoc2` file uses a partly different set (0x01B, 0x021, 0x025, 0x026,
  0x02D, 0x02E, 0x041, 0x042, 0x059).

## Objects, property sets and files (section 2.6)

- **JCID** (4 bytes): object type. Low 2 bytes `index` says what kind of object
  it is (meaning is defined in MS-ONE). Flags: IsBinary, IsPropertySet,
  IsGraphNode (ignored), IsFileData, IsReadOnly.
- **Property set:** a 2-byte property count, then an array of 4-byte PropertyIDs,
  then a data stream. A PropertyID has a 26-bit id and a 5-bit type, plus a
  boolean bit. Types: no data, bool, 1, 2, 4 or 8 bytes of data, length-prefixed
  data (under 0x40000000), object ID and array of object IDs, object space ID(s),
  context ID(s), array of property values, nested property set.
  What each property id means is defined in MS-ONE, not here.
- **Object references:** an object's property set can point to other objects in
  the same object space, to other object spaces and to contexts. Object
  references must not form a cycle.
- **File data object:** the content of an inserted file (image, audio, video,
  attachment). Layout: header GUID {BDE316E7-2665-4511-A4C4-8D4D0B7A9EAC}, an
  8-byte length, 4 unused bytes, 8 reserved bytes, the data padded to an 8-byte
  boundary, footer GUID {71FBA722-0F79-4A0B-BB13-899256426B24}. The glossary adds
  that a file data object can be stored inside the file, or outside it in a
  `onefiles` folder.

## Encryption

An object space can be encrypted. Then the second node of each revision manifest
is a 0x07C node holding the key data. A password-protected section is the user
case. The details of the algorithm are not in the parts read so far.

## Alternative packaging (sections 2.7 and 2.8)

The same revision store can travel over the FSSHTTP protocol (OneNote over
OneDrive and SharePoint) in a different, packaged encoding. This is the form the
Rust crate `onenote_parser` handles. Desktop files, like the owner's backup, are
the on-disk revision store described above.

## What an importer needs beyond this spec

MS-ONE, to know what each JCID index and property id means: section, page, outline,
text run, table, image, ink, tags, and so on. This spec only describes the
container.

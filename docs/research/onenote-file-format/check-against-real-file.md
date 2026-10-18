# Check of the spec against a real `.one` file

Date: 2026-10-17
Status: DRAFT. Structure only. No page content, names or identifiers from the
file are recorded here.

## Method

A throwaway Python script (kept outside the repo, in the session scratchpad)
read the owner's OneNote desktop backup of a section file, read-only. It parsed
the header and walked the file node lists following the layout in
`ms-onestore-notes.md`. The file is 10,912 bytes and holds almost no content.

## Results

| Check | Spec says | File shows |
|---|---|---|
| File type GUID at offset 0 | {7B5C52E4-...} for `.one` | match |
| Format GUID at offset 48 | {109ADD3F-911B-49F5-A5D0-1791EDC8AED8} | match |
| Legacy GUID at offset 32 | zero | zero (the zero block in the early hex dump) |
| Version codes | 0x2A | all four are 0x2A |
| Expected file length | equals file size | 10,912 = file size |
| Reserved block (728 bytes) | zero | zero |
| Transactions in log | not zero | 14 |
| Root file node list | at stp 1024, magic `A4567AB1F5F7F4C4` | match, list ID 0x10, footer ok |
| Free chunk list | optional | nil, so none |
| Hashed chunk list | optional | present, 1024 bytes at offset 7976 |
| All other list fragments | magic and footer | all valid |

Every structure checked matched the spec. The walk found 2 object spaces listed
under the root, each with one revision manifest (type 0x01E) and one object
group. Together they hold 5 global ID table entries, 17 normal object
declarations (0x0A4), 5 read-only declarations (0x0C4) and 5 root object
references. What each object space is (for example, a page) was not decoded.

## The "LZ" bytes: resolved

- The five `4C 5A` ("LZ") hits are at offsets 7338, 9794, 9968, 10044 and 10072.
- Each one is the last two bytes of the 16-byte GUID
  {7111497F-1B6B-4209-9491-C98B04CF4C5A}, stored in little-endian order as
  `7f 49 11 71 6b 1b 09 42 94 91 c9 8b 04 cf 4c 5a`. That GUID occurs 5 times in
  the file, at offsets 7324, 9780, 9954, 10030 and 10058. Every "LZ" is inside
  one of those occurrences.
- MS-ONE section 2.1.18 names this GUID as part of the context ExtendedGUID of
  the version history object space. In three of the five occurrences the 4 bytes
  after it are 1, 2 and 3, which fits the second half of an ExtendedGUID.
- So "LZ" is a coincidence in a constant GUID, not a compression marker.
  Verified by a byte search of the whole file.

## Not checked

- Property set contents and what the objects mean (needs MS-ONE).
- The transaction log entries and the CRCs.
- Any file with content, images, ink, sub-pages, attachments or a password.
- Whether NoteCraft can write this format. Out of scope.

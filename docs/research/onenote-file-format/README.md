# OneNote file format references

Date: 2026-10-17
Status: DRAFT. Notes only. MS-ONESTORE and MS-ONE have been read in part.
MS-ONESTORE was checked against one real file.

Clean-room rule: this folder holds paraphrased notes and links to public
documentation. No Microsoft code, no decompiled binaries, no copied
specification text. Facts such as GUIDs, magic numbers and field sizes are
recorded as data.

## Files

| File | What it is |
|---|---|
| `ms-onestore-notes.md` | Concrete notes on the `.one` container: header, file node lists, objects, property sets, file data, encryption. |
| `ms-one-notes.md` | Concrete notes on what the objects mean: notebook, section, page, subpage levels, outline, text, tables, images, embedded files, audio and video, tags, and the ink gap. Includes a mapping to plain files. |
| `check-against-real-file.md` | The spec checked against a real OneNote desktop backup. Structure only. |
| `wikipedia-onenote-format.md` | Wikipedia's "File format" section, a correction, and a trace of its references 21 to 26. |
| `import-sources.md` | Where users can import OneNote data from: local backups, cache, OneDrive, Graph API, `.onepkg`. Includes counts from the owner's machine (no names). |
| `fileformat-com-one.md` | Summary of the docs.fileformat.com page. A secondary source. |
| `README.md` | This index. |

## Sources

| Source | Kind | Read? |
|---|---|---|
| [MS-ONESTORE v20250520 PDF](https://officeprotocoldocs-f5hpbjgea6b8gneq.b02.azurefd.net/files/MS-ONESTORE/%5bMS-ONESTORE%5d.pdf) (redirects to officeprotocoldoc.z19.web.core.windows.net) | Microsoft primary spec | Yes: front matter, sections 1.x, 2.1 to 2.4, 2.6 (part), header and node-list parts. Not yet: 2.5, 2.7, 2.8, 3.x |
| https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-onestore/ | MS-ONESTORE landing page, version list, licensing notice | Yes |
| https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-one/73d22548-a613-4350-8c23-07d15576be50 | MS-ONE landing page (v3.4, 2022-11-15) | Yes |
| [MS-ONE v20221115 PDF](https://officeprotocoldoc.z19.web.core.windows.net/files/MS-ONE/%5bMS-ONE%5d.pdf) | Microsoft primary spec, 108 pages | Yes: overview, concepts, page, outline, text, table, image, embedded file, tags. Not yet: the worked example (3.1), paragraph styles, most of the section 2.3 property tables |
| https://en.wikipedia.org/wiki/Microsoft_OneNote | Secondary | Yes, see `wikipedia-onenote-format.md` |
| https://docs.fileformat.com/note-taking/one/ | Secondary | Yes, see `fileformat-com-one.md` |
| https://docs.rs/onenote_parser | Rust reader | No |
| https://lib.rs/crates/one2html | `.one` to HTML converter | No |
| https://advisories.gitlab.com/pkg/cargo/onenote_parser/ | Advisory for `onenote_parser` | No |
| Wikipedia refs 22, 23, 24, 26 (old MS links) | Primary, old | No |

## Key findings so far

- `.one` is a section file. A notebook is a folder plus a `.onetoc2` table of
  contents. Wikipedia's wording that `.one` can be a notebook is wrong.
- The container is a revision store: objects with property sets, grouped in
  object spaces, with immutable revisions, a transaction log and reference
  counts. It is a small database in a file, not a document.
- No data compression is described. "Compressed" in the spec means pointers
  packed in fewer bytes and multiplied by 8. The owner's "LZ" bytes are the end
  of a constant GUID named in MS-ONE 2.1.18 (verified in the file).
- OneNote's 3-level subpage limit is a rule of the file format: subpages are a
  flat list with a `PageLevel` number that MUST be 1 to 3 (MS-ONE 2.3.74).
- Section groups are real subfolders of the notebook folder. Section order and
  names live in the notebook's `.onetoc2`.
- A page is a free-form canvas: outlines and images have offsets from the page
  origin and can overlap.
- Ink is not described in MS-ONE or MS-ONESTORE (no stroke or handwriting
  structure found). Equations are only a flag on a text run.
- Optional encryption exists per object space (password-protected sections).
- Inserted files can be inside the file or in an external `onefiles` folder.
- The spec checks out against a real desktop backup: every header field and list
  fragment checked matched.
- The container does not say what a page, outline or text run is. That is MS-ONE.

## Open questions

- Where is ink stored and how is it encoded? Needs a test file with ink.
- Read MS-ONE section 3.1 (worked example) and the paragraph-style sections.
- Read MS-ONESTORE 2.5 (node layouts) and 3.x (worked examples) before
  writing any parser.
- Encryption algorithm details, if password-protected sections must import.
- A test set with real content: text, images, ink, sub-pages, attachments, a
  password-protected section.
- Whether `onenote_parser` reads desktop-app files (older docs say it does not).
- Read the patent notice and the Open Specifications Promise before shipping.

## Rules for this folder

- Add a source row for every document used, with the date read.
- Paraphrase. Quote at most a short phrase, and name the source.
- Do not store the owner's real `.one` files here. They contain personal data.

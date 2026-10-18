# Where a user's OneNote data can be imported from

Date: 2026-10-17
Status: DRAFT. Based on the owner's statements, one directory listing of the
owner's machine (counts and sizes only) and web searches. Secondary sources are
labelled. No notebook names or content are recorded here, because they are
personal data.

## Paths

| # | Source | Status | Notes |
|---|---|---|---|
| 1 | Local OneNote backups: `%LOCALAPPDATA%\Microsoft\OneNote\16.0\Backup\` | Primary path for version 1 | Plain `.one` revision store files, which MS-ONESTORE describes. One file was parsed and matched the spec. |
| 2 | OneNote cache: `...\16.0\cache\` | Unknown | Thousands of `.bin` files. One secondary source says the format is proprietary and undocumented. Not investigated. |
| 3 | Notebooks stored by OneDrive | Not usable as files | Secondary sources say the OneDrive desktop client does not sync notebook files. It leaves a web-link stub (`.url`) in the synced folder. OneNote syncs the notebook with its own logic. |
| 4 | Cloud access through Microsoft Graph | Later, needs a sign-in app | Secondary sources say app-only access to the OneNote Graph APIs was retired on 2025-03-31. Delegated (signed-in user) access remains. Page content format and fidelity (ink, layout) not checked. |
| 5 | `.onepkg` export from OneNote | Possible | Wikipedia says it is a CAB archive of `.one` files. Rust CAB support not checked. |
| 6 | Files downloaded from OneDrive (FSSHTTP packaging) | Possible | The format the Rust crate `onenote_parser` is built for. Not checked whether it matches the cache `.bin` files. |

## What the owner's machine shows (structure only)

- Folder: `C:\Users\<user>\AppData\Local\Microsoft\OneNote\16.0`. Subfolders
  `Backup`, `cache`, `FullTextSearchIndex`, `MasterIndex`, `Memory`,
  `ServerListings`, `AccessibilityCheckerIndex`.
- 439 `.one` files totaling about 2.6 GB, all under `Backup`.
- About 18,700 `.bin` files totaling about 2 GB. These are cache files.
- 60 `.db` files (about 177 MB) with 58 empty `.db-journal` files, and one
  `.onesrvcache` file. Likely search and index data; not inspected.
- No `.onetoc2` files at all.
- `Backup` is organised by notebook or year folder, with section group folders
  below them, and section files named like `<Section> (On <date>).one`. The same
  section appears many times with different dates.
- Largest single `.one` files are about 160 MB, 100 MB and 90 MB.

## Consequences for an importer

- **The owner has a large private test corpus** (439 files). It must stay on the
  owner's machine. Do not copy files into the repo or paste their content. Test
  output must be structural only unless the owner agrees otherwise.
- **Many dated copies of each section.** The importer must choose the newest
  copy per section, or offer to import several as history.
- **No `.onetoc2` in backups.** Section order, section colors and notebook
  metadata are not available from the backup files. The importer would rebuild
  order from folder and file names and mark the notebook as reconstructed.
  Section groups are real subfolders, so that part of the structure survives.
- **Large files.** Files of 100 MB and more need streaming reads and a memory
  budget, not whole-file loads.
- **Cache `.bin` files are an unknown.** Importing from the cache would reach
  notebooks that were never backed up, but needs separate research.
- **Cloud import** needs a registered app, sign-in, privacy handling and a
  decision on token storage. It belongs in a later phase.

## Open

- What are the cache `.bin` files, and do they follow MS-ONESTORE?
- Does a notebook that lives only in OneDrive appear in `Backup` at all?
- What does the Graph API return for ink, layout and attachments?
- Does `onenote_parser` read desktop revision-store files? Unconfirmed.
- Retention: how far back do backups go and how are old ones pruned?

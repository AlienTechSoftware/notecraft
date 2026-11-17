# Portable export and sharing

File > Export and File > Share use the same review screen. Home > Send Page opens it
with the current page selected. No account is required.

1. Choose the current page, its section or its notebook. A page includes its subpages.
2. Choose an existing destination folder outside the source notebook. Download-enabled
   browser hosts use the browser's download destination instead.
3. Select **Review export**. Check the file list, page text preview, extra linked content
   and external links. Preparing or reviewing an export does not write or send anything.
4. Select **Write reviewed export**. Desktop output goes into a new export folder.
   A browser host receives a ZIP containing the same files. Unzip it before opening it.
5. Share the entire exported folder with the application or transfer tool you choose.
   A local file path does not grant anyone remote access.

The export keeps original HTML and CMGFM page files, their metadata, arrangement, rich
text, ink and attachments. Mixed-format notebooks stay mixed. It does not convert content
or offer OneNote, Office or PDF output. Printing is a separate feature. Save a Copy gives
copied nodes new identities; export preserves the identities in the reviewed snapshot.

A page or section export retains its notebook ancestry so the result can be opened as a
notebook. Linked local pages and files are included transitively inside that notebook;
the review lists dependencies added outside the selected scope. NoteCraft page links are
rewritten to relative links. Links to missing files or outside that notebook stop preparation
with an error. Choose a wider scope or repair the link before trying again. External web
links are retained and disclosed. Their content is never downloaded.

The open editor contributes its unsaved body text and changed live drawing. The review captures
file bytes, so later edits cannot silently change an already reviewed export. Review again to refresh it. Cancel
discards the review. Destination failures retain it for retry. Existing output is never
overwritten; repeated writes get separate export folders. Failed writes remove their staging
folder. If cleanup fails, the error identifies the remaining folder.

Only content is exported. The `.notecraft` records containing saved versions, recycle bins
and templates are excluded, as are Git, Mercurial and Subversion records. Symbolic links,
outside references, more than 4096 files, more than 256 MiB of content, and folders nested
more than 64 levels are refused. A page must
also fit the normal page-open size limit.

Native system share sheets and automatic mail attachments are not integrated on Windows,
macOS, Linux, Android or iOS yet. The visible fallback is to save the portable folder and
share it manually. Download-enabled browser hosts use the existing explicit file-delivery
facility. Android folder picking and access to other apps' storage retain the limitations
listed in [gaps.md](gaps.md). Native handoff availability never prevents local export.

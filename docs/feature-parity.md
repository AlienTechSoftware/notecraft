# Feature parity

Last reviewed: 2026-11-23.

What NoteCraft does today against the feature list of the desktop note taking app most
people are moving from, called **the other app** below. That list is kept out of this
repository; it was written from general knowledge and web searches, not from running the two
side by side. Status words:
**Done** (built and tested), **Partial** (some of it), **Planned** (decided, not built),
**Later** (wanted, not scheduled), **Skip** (not wanted).

| Area | Feature | Status | Notes |
|---|---|---|---|
| Structure | Notebook, section group, section, page as folders | Done | `folder-format.md`. |
| Structure | Unlimited subpage depth | Done | Tested to 20 levels; the tree shows 6 in the demo. The other app stops at 3. |
| Structure | Multi-level collapse and expand | Done | Per-row chevrons, Expand all, Collapse all. |
| Structure | A new notebook starts usable | Done | Like the other app: one section, "New Section 1", holding one empty page. `notebook.create empty=true` skips it. |
| Structure | Colours per notebook or section | Done | Nine colours on notebooks, section groups and sections, saved in `_node.toml`, shown on the icon and the row edge. |
| Structure | Templates | Partial | Insert > Page Templates: built-in templates across categories (Academic, Blank, Business, Planners), and any page saved as a template; a click adds a page to the section. No preview images, and no default template for new pages. |
| Authoring | Rich text | Partial | Both formats edit block by block with a real caret, falling back to read-only for unrepresentable markup. The Home ribbon provides font family selection from installed faces, font sizing, font colour, text highlights, formatting marks (bold, italic, underline, strikethrough, sub/superscript), paragraph alignment (left, center, right), and paragraph spacing. Partial selections across lines or markup and OS font registration remain limited. |
| Authoring | Tables, lists, to-do, quotes, code | Done | The ribbon inserts them, the page draws them, to-do boxes tick where they stand, and lists scale markers with effective body font size. Tables include a dedicated Table ribbon tab with cell editing, row/col management, shading, cell alignment, cell merge/split, sorting, and optional GridCraft spreadsheet integration. |
| Authoring | Right-to-left text | Partial | Arabic, Hebrew, Persian and Urdu draw with proper word placement, line and run directions. Arrow navigation follows visual direction through mixed bidi text, caret avoids vowels, and block-spanning selections work. Window mirroring and Arabic collation sorting are not built. |
| Authoring | Tags | Partial | Twelve tags stored as plain text marks. Home > Find Tags opens a Tags Summary pane across open notebooks with jump-to-tag clicks. Grouping/sorting options, search within summary, and generating a dedicated tag summary page are not built. |
| Authoring | Equations | Done | KaTeX-style TeX drawn natively in real maths fonts, with a gallery on the Insert tab. A formula the engine cannot take stays visible as text. `how-to/write-equations.md`. |
| Authoring | Diagrams (Mermaid) | Partial | Drawn in the page (flowcharts, sequence, class, state, ER, Gantt, mind maps, pie, bar and line, quadrant), in both page formats; a click opens the source, and the code view shows it as text. `how-to/draw-diagrams.md`. |
| Authoring | Free-form canvas boxes | Partial | Pages support multiple free-form text containers with draggable top handles, right-edge resizing, and click-to-place on blank paper. Layout metadata is stored in HTML head or CMGFM front matter. Moving blocks between containers, exact container height detection, and picture containers are not built. |
| Authoring | Ink | Partial | Draw tab with five nibs, customizable pen rack (thickness dots, colour grid, eyedropper, stroke previews, Alt+1..9), pressure sensitivity, eraser, undo/redo, ruler stencil, regular shapes, automatic shape recognition, ink replay, and select/lasso (including open C-bracket). Ink is saved to `ink.svg` with `nc:dabs` readings, written off the main thread. Ink to Text/Math and macOS/X11 pressure are not built. |
| Embedding | Images, files, video | Partial | Dropped files can be copied into the page folder, linked, or inserted as PDF printouts (native Rust rasterization; LibreOffice for Office formats). Includes image right-click "Save picture as...", Insert > Online Video links, and a 12-sticker gallery. Video embedding/playback, screen clipping, audio/video recording, and transcription are not built. |
| Embedding | Live Office objects | Skip | Not portable. |
| Retrieval | Full-text search | Done | Across all notebooks, title and body. |
| Retrieval | Links between pages | Partial | Clicking a link opens the named page, section, external file, or web URL. Copy link to page generates `notecraft:/book/section/page` links. Links are followed by path; links by ID, links inside table cells, and links inside mixed bidi lines are not built. |
| Retrieval | Versions, recycle bin | Partial | History tab provides Page Versions (stores and restores snapshots of a page's file together with its ink and attachments, issue #553; versions taken before that are marked "words only") and Notebook Recycle Bin (restores or permanently removes deleted pages, sections, and section groups). Deleting entire notebooks is not supported in the bin. |
| Retrieval | OCR, audio search | Partial | Printed text in pictures and printout pages is read on this computer with the `ocrs` models (setting `ocr_models`), found by search, and copied with the picture menu's Copy Text from Picture, for one picture or one or every printout page, shown for review first. Handwriting recognition and audio search are not built. Raw stroke data (`nc:dabs`) in ink files is preserved for future handwriting recognition. |
| Sync | Cloud sync and backup | Done by design | Plain files in a synced folder (OneDrive, Dropbox, Nextcloud, Syncthing). File changes debounce and defer during active ink strokes. Real-time conflict merging is not built. |
| Sync | Section passwords | Later | |
| Collaboration | Sharing | Partial | Shared folder sync; live co-authoring is Later. File > Share, File > Export, and Home > Send Page provide reviewed original-format folder exports (or browser ZIP downloads). History tab includes author tracking and Find by Author across open notebooks. Native OS share sheets are not integrated. |
| Output | Export | Partial | File > Export produces reviewed original-format folder packages (downloaded as ZIP in the browser). Images offer "Save picture as...", drawings offer "Save picture" SVG export, tables export to CSV for GridCraft, and pages offer background printout rendering. File > Print and Ctrl+P print the page or its section, with a preview, paper size, orientation and margins, and save it as a PDF (pictures of the sheets, 200 dpi) or send it to a printer (CUPS on macOS and Linux; the PDF viewer on Windows). A text-based PDF writer and OneNote file export are not built. |
| Output | Import | Partial | `notecraft-cli import onenote` and in-app import read `.one` sections, `.onetoc2` color metadata, and `.onepkg` packages into notebook folders. Still missing: importing ink strokes, tags, and password-protected sections. |
| Input | Local dictation | Partial | Optional `voice` build, reviewed text insert, Undo and save. See [dictation](how-to/dictate.md) for format and hardware acceptance limits. |
| Output | Add-ins, AI | Later | |
| Look | Themes | Done | 14 themes, contrast checked. `themes.md`. |
| Look | Ruled and grid page backgrounds | Done | Format Background on Draw or View provides none, lines, or squares with selectable rule line colours and page tint colours, stored per page in metadata (`notecraft:paper`, `notecraft:page-color`) and contrast-checked across themes. |
| Look | Office-style ribbon | Partial | Home, Insert, Draw, History and View tabs with Office-style groups, Styles and Tags galleries, contextual Table and Source tabs, section tabs layout, and File screen. Immersive Reader and Dock to Desktop are available. Review tab and general Help tab remain unbuilt. |
| Files | See what a page is made of | Done | The code view lists every file of a page and shows what is in each. The other app keeps a section in one binary file that a text editor cannot read. `how-to/look-inside-a-page.md`. |
| Look | Platforms | Partial | Desktop builds (Windows MSI/zip, Linux deb/rpm/AppImage/tar.gz, macOS DMG/zip) and browser WASM demo are actively built and verified in CI. Android arm64 APK builds with a phone layout, but lacks native file pickers, system clipboard, and on-device signoff. |

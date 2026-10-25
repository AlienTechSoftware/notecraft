# Feature parity

Last reviewed: 2026-10-17.

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
| Structure | Templates | Partial | Page templates for both formats; no template picker. |
| Authoring | Rich text | Partial | Both formats are written on in their finished look, block by block, with a real caret. A page whose markup cannot be written back exactly opens read-only rather than risking it. No font, size, colour or alignment controls yet. |
| Authoring | Tables, lists, to-do, quotes, code | Done | The ribbon inserts them, the page draws them, and a to-do box ticks where it stands. |
| Authoring | Right-to-left text | Partial | Arabic and Hebrew draw, and the page, the line and the run each take a direction, written into the text so it travels. A line that mixes the two directions is still drawn in one order, and the window is not mirrored. |
| Authoring | Tags | Partial | Twelve tags, stored as plain text in the page and drawn as marks. No tag summary page. |
| Authoring | Equations | Done | KaTeX-style TeX drawn natively in real maths fonts, with a gallery on the Insert tab. A formula the engine cannot take stays visible as text. `how-to/write-equations.md`. |
| Authoring | Diagrams (Mermaid) | Partial | Drawn in the page (flowcharts, sequence, class, state, ER, Gantt, mind maps, pie, bar and line, quadrant), in both page formats; a click opens the source, and the code view shows it as text. `how-to/draw-diagrams.md`. |
| Authoring | Free-form canvas boxes | Later | Not representable in Markdown. |
| Authoring | Ink | Later | Possible as SVG strokes per page. |
| Embedding | Images, files, video | Partial | A dropped file can be copied into the page folder, linked where it is, or rendered as a printout. Windows releases bundle Poppler; LibreOffice is used for Office conversion. |
| Embedding | Live Office objects | Skip | Not portable. |
| Retrieval | Full-text search | Done | Across all notebooks, title and body. |
| Retrieval | Links between pages | Partial | Stored as relative links; shown, not clickable yet. |
| Retrieval | Versions, recycle bin | Later | Left to file history of the synced folder. |
| Retrieval | OCR, audio search | Later | |
| Sync | Cloud sync and backup | Done by design | Plain files in a synced folder. Verified once on OneDrive in the prototype; not re-verified for this build. |
| Sync | Section passwords | Later | |
| Collaboration | Sharing | By design | A shared folder. Live co-authoring is Later. |
| Output | Export | Later | Export scope is undecided. |
| Output | Import | In progress | Required, import only (owner's decision, 2026-10-17). `notecraft-cli import onenote` reads `.one` files and writes notebook folders in either page format. Not done: ink, tags, `.onetoc2`, the window. |
| Output | Add-ins, AI | Later | |
| Look | Themes | Done | 14 themes, contrast checked. `themes.md`. |
| Look | Ruled and grid page backgrounds | Done | View, Rule Lines: none, lines or squares behind the page, in colours each theme checks for contrast. Remembered between runs. |
| Look | Office-style ribbon | Partial | Home, Insert and View with groups, a Styles gallery, a Tags gallery, a contextual Source tab and a File screen. Draw, History, Review and Help are not built, and say so. |
| Files | See what a page is made of | Done | The code view lists every file of a page and shows what is in each. The other app keeps a section in one binary file that a text editor cannot read. `how-to/look-inside-a-page.md`. |
| Look | Platforms | Partial | Windows built and tested; macOS and Linux not built. |

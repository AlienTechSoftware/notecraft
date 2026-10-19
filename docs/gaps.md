# Known gaps and risks

Last reviewed: 2026-10-17. An honest list of what is missing or unproven. Remove an item when
it is fixed and tested, not when it is promised.

## Product

- **Import is not finished.** It is a requirement, and the scope is import only
  (owner's decision, 2026-10-17). `crates/onenote` reads a `.one` section file: the container
  and the objects in it, down to pages, outlines, text runs, pictures and attachments, tested
  against synthetic files. What is missing is the other half, writing that out as notebook
  folders, so nothing can be imported yet. Notebook structure (`.onetoc2`) is not read, and
  neither are password-protected sections or ink.
- **Mermaid diagrams are not drawn.** They are kept as source and shown as source. Mermaid is
  JavaScript; options are a pure-Rust renderer for a subset, or an optional external tool.
- **The page editor has edges.** Both formats are written on in their finished look, block by
  block, and written back to the page's own format. But a page whose markup the writer cannot
  reproduce exactly is opened read-only rather than risking it, and a table, a picture or a
  horizontal rule cannot be edited in place: only the blocks around it.
- **No bold or italic font.** egui bundles none; the preview uses colour and slant for emphasis.
  A bundled font family is the fix and needs a licence check.
- **Links follow by relative path only.** A click on a link opens the page or folder it
  names, a file in the program the system has for it, or a web address in the browser; the
  target is a path as written, so a renamed page breaks the links to it. Links by id are not
  defined (`folder-format.md` section 10). Not clickable yet: a link inside a table cell, and
  one in a line that mixes left-to-right and right-to-left text.
- **One weight per bundled script.** Arabic, Hebrew, Devanagari, Chinese, Japanese and
  Korean draw from bundled Noto faces (the CJK one is 16 MB, most of the app's size), but in
  one weight each and with Simplified Chinese forms for characters the CJK scripts share; a
  Japanese or Korean reader may see a few unfamiliar shapes. Scripts outside these six still
  draw as boxes.
- **The Welcome notebook speaks English only.** `docs/demo/DemoNotebook`, built by
  `cargo xtask demo` and carried inside the app, appears in an empty notebooks folder on the
  first start and comes back from the File screen. Twelve more languages are planned, four
  of them right to left; the start page lists only what is built.
- **No icons of the user's own** on a notebook or section. Colours are there (nine of them,
  on notebooks, section groups and sections); a chosen icon or picture is not.
- **Rule lines are a window setting, not a page setting.** View, Rule Lines draws none, lines
  or squares behind every page, and the choice is remembered; the page itself carries no
  setting, so it does not travel with the notebook.
- **The ribbon is a first pass.** Home, Insert and View with Office-style groups, a Styles
  gallery, a Tags gallery and a File screen. Missing: fonts and sizes, text colour and
  highlight, alignment, and the Draw, History, Review and Help tabs. Controls that are not
  built say so rather than doing nothing.
- **Right-to-left text works; the window around it does not know yet.** Arabic and the other
  right-to-left scripts draw, joined and in the right order; a page, a paragraph or a run can
  be given a direction; a right-to-left line hangs from the right edge with its bullet, box
  and tag mark on that side; and a line that mixes the two directions is laid out by the
  Unicode algorithm, so `السعر 123 ريال` keeps the number as `123`. What is missing: the
  window is not mirrored (the tree stays on the left whatever the page says), a selection
  cannot be dragged inside a right-to-left line yet (the caret moves, the selection does not),
  and the isolates the Run buttons write are kept in the file but not yet honoured when the
  line is drawn.
- **The caret in a right-to-left line is NoteCraft's own, and it is new.** egui cannot place
  one there (`crates/ui-egui/tests/arabic_layout.rs` measures why: it counts glyphs where it
  means characters), so a line with Arabic in it is drawn by `crates/ui-egui/src/bidi.rs` and
  written in with a caret of ours: parley says which character a click landed on and which
  way a step moves, egui still draws. Selecting works there too, by dragging, with Shift and
  the arrows, and a double click takes the word under the pointer. What it does not do yet:
  a selection that spans two paragraphs, which no block can do, and Ctrl with the arrows to
  step by word.
- **The caret is egui's, not ours.** Each block is an egui text box, so there is no caret of
  our own across blocks: no selection spanning two blocks, and no caret movement by visible
  step in mixed text. The sibling WordCraft owns its whole text pipeline (shaping, line boxes,
  hit testing) and gets this right; matching a mature editor means a text crate of our own
  over parley or the same house stack. Decision pending.
- **A dropped file can be copied in or linked.** A printout (a document rendered to pictures,
  as other apps insert one) is not built: it needs a renderer for PDF and Office files.
- **Pasting reads only the flavour egui hands over, which is text.** An app that copies a
  table puts several on the clipboard at once: HTML (the richest, with the cells, the marks
  inside them and even `direction:ltr`), plain text with a tab between cells, and pictures
  (Bitmap, DIB, PNG). NoteCraft understands both text flavours: tab-separated lines come
  back as a table, and if the HTML reaches it as text, header and all, it is parsed the way
  a page is. What it cannot do is ask the clipboard for the HTML or the picture flavour,
  because egui exposes text only; that needs a clipboard crate (`clipboard-rs` reads HTML
  and pictures on all three systems, `arboard` pictures alone), which is a decision for the
  owner.
- **Containers are a first pass.** A page can hold several boxes, each with its own place,
  width and blocks; a box is moved by the bar along its top and resized by its right edge,
  and the arrangement is kept in the page's own file. What is missing: making a new box by
  clicking an empty part of the page, moving blocks from one box to another, boxes that know
  how tall they are (the room under them is guessed), and a picture as a box of its own. The
  blocks themselves stay in reading order in the file, so a page read anywhere else is
  unchanged; if the counts and the blocks ever disagree, everything goes back into one box
  rather than losing a word.
- **Pen and drawing.** The pen writes anywhere on the page, as OneNote's does. The ink of a
  page is `ink.svg` in the page's own folder; a page never drawn on has no such file, and a
  page rubbed clean loses it. It is SVG that also carries the pen's own readings in our
  namespace, so NoteCraft can change a stroke later while anything else sees the picture.
  Built: the Draw tab; five nibs (pen, pencil, fountain pen, brush pen, highlighter); a
  rack the owner arranges, with Add, a right-click menu, a Modify Tool editor for colour
  and thickness, and the whole rack remembered between runs; a pressure sensitivity wedge;
  an eraser; undo; Save picture; and picking ink up with Select or a lasso to move, stretch
  or delete it. **Not built:** shapes and automatic shapes; the ruler and other stencils;
  ink replay; ink to text and ink to math; laser pointer; More Colours and the eyedropper
  in the tool editor. Pressure is real and measured on Windows (hardest 0.930 over 1223
  readings, 2026-10-18); on macOS, Linux and Wayland winit drops the tablet fields, so a
  pen there draws at full pressure like a mouse, and tilt and the eraser end are on no
  platform. `cargo run -p notecraft --example pen_check` shows what a pen reports.
- **Tables are edited cell by cell**: click a cell to write in it, Tab walks along the row
  and on to the next and grows a row off the end, Escape leaves. While the caret is in a
  table the ribbon offers a Table tab, as Office does for the thing in hand: rows and
  columns can be added either side of the caret and taken out, and the heading row and the
  last column stay. A drag across cells picks the block between them, which Delete empties
  and the tab's Select Table, Select Rows and Select Columns pick the same way; deleting
  rows or columns with a block picked takes all of them. A table is drawn with lines round
  its cells, which the tab's Hide Borders turns off and on; that choice is the app's, not
  each table's, because CommonMark has nowhere to keep it. A column is a fixed width and
  stays that width when the box around it is resized, the way OneNote's does; a border
  dragged sideways changes the column to its left, and the page keeps the widths beside
  the arrangement of its boxes (`notecraft:columns`), because neither CommonMark nor an
  HTML table that must match it has anywhere to put them. The Insert
  tab's Table button opens OneNote's size picker, where the pointer runs over a grid and
  names the size it would make. Not built, and greyed where a OneNote user looks for them:
  merging and splitting cells, shading, alignment, sorting; dragging a row border to change
  its height; and the Insert Table dialog.
- **A spreadsheet goes to GridCraft**, the open spreadsheet from the same workshop, where
  OneNote converts a table to Excel. The Table tab writes the table out as a CSV file
  beside the page and opens it; the Insert tab starts an empty one and links it from the
  page. Both are greyed when GridCraft is not installed, which is looked for on the PATH
  and in the folder its installer uses.
- **Pasting** puts a picture on the page as a file beside it, which egui alone cannot do:
  the clipboard is read directly, because egui hands a window text only. The page also has
  its own menu under the right button, with paste, cut, copy and the things the Insert tab
  can put there.
- **The Insert tab** carries OneNote's groups, but most of what they offer is not built.
  Working: tables, a file attachment, a picture, a link, the three time stamps, an
  equation, and NoteCraft's own Mermaid diagram. Not built, and greyed on the tab: insert
  space, file printout, spreadsheet, screen clipping, online video, transcribe, record
  video, page templates, symbol and stickers.
- **Templates picker, version history and recycle bin** are not built. (Tags are: to-do boxes
  and the other familiar marks, stored as plain text in the page.)
- **Sync conflicts** are only detected by name by the checker; there is no merge or resolve UI.

## Engineering

- **Windows only so far.** The code avoids OS features, but nothing has been built or run on
  macOS or Linux. Treat cross-platform as an unproven claim.
- **Coverage is 87.8% of lines** (measured 2026-10-17); window start-up and `app.rs` are the weakest.
- **Control key file permissions on Windows** rely on the user folder's access rules.
- **Large pages and attachments** are capped but not load tested.
- **Crashes on one Windows PC are unexplained.** A release build was reported to crash several
  times on a Windows machine with AMD graphics. It was not reproduced: about fifteen runs under
  a debugger (empty folders, a restored session, real use) ended in clean exits, and no panic was
  logged. One lead, assumed and not confirmed: NoteCraft takes the graphics library's default
  backends (Vulkan, DirectX 12 and OpenGL), and WordCraft's README documents that also creating
  an OpenGL instance can crash AMD's driver before the window appears. WordCraft's answer is
  DirectX 12 only by default; NoteCraft has not made that change. To rule it in or out, set
  `WGPU_BACKEND` to `dx12` or `vulkan` before starting (the library reads it; checked in its
  source, not tested here). `notecraft.log` in `%APPDATA%\NoteCraft` records a panic with its
  backtrace and the last step reached; see [the README](../README.md#logs).
- **The installer is built but not yet installed, upgraded or removed on a clean machine.** The
  MSI's contents and properties were checked by unpacking it. The Start menu shortcut, the
  finish-page launch box and an uninstall that leaves notebooks alone are unchecked. Builds are
  unsigned, so Windows SmartScreen warns. The release workflow has not run on GitHub.

## Legal and provenance

- **The koala artwork** was generated by the owner with Windows Copilot, a licensed copy run
  locally on the developer machine. The owner states its terms allow using and distributing
  the output; that was not independently checked. It is outside the project's licence. See
  `ATTRIBUTION.md`.
- **The licence is MIT OR Apache-2.0** for the code (the siblings' choice, taken 2026-10-17).
  The artwork above is outside it, and no audit of the dependencies' licences has been run.
- **Clean room:** the `.one` format work uses only Microsoft's published specifications and
  files the owner owns. No Microsoft code or text is copied.

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
- **The Welcome notebook is in thirteen languages, and twelve are machine translated.**
  `docs/demo/DemoNotebook`, built by `cargo xtask demo` and carried inside the app, appears in an
  empty notebooks folder on the first start and comes back from the File screen. English is the
  source. The other twelve (four of them right to left) were translated by an AI model, and each
  translated page says so at its foot and asks for corrections. Every language has the same pages in
  the same order, including the one on the code view.
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
  window is not mirrored (the tree stays on the left whatever the page says), and the
  isolates the Run buttons write are kept in the file but not yet honoured when the line is
  drawn. Selecting inside a right-to-left line does work; see the caret entry below.
- **A right-to-left page title is drawn, not typed into, until it is clicked.** The title
  is an egui text box, which lays the letters of an Arabic word out the right way and then
  puts the next word to the right of it, so every Arabic page showed its own name with the
  words backwards. A right-to-left title is drawn by the bidi painter now and becomes a
  text box when it is clicked. **Not built:** the words are still backwards while the title
  is being typed in, which needs the page's own caret in the title box.
- **The demo has a page with right-to-left in every element** - العربية في كل عنصر, under
  the Arabic How to - which is the corpus to look at after any change to text drawing.
  **Known from it, and not fixed:** a numbered item puts its number on the left of a
  right-to-left line (a bullet and a task box go to the right, as they should), and a line
  that starts with a Latin tag word such as `TODO:` is read as left to right and flips.
- **The caret keeps off the vowels.** A letter with a haraka on it is two shaped clusters,
  so the arrow keys used to stop inside it and a click could land there: a caret in a place
  the eye cannot see, and a Backspace that takes the vowel off a letter that looks
  untouched. `bidi::step` and `bidi::hit` carry on past a mark now, for Arabic, Hebrew,
  Syriac, Thaana, Thai, Cyrillic and the Latin combining accents. A general answer is
  Unicode grapheme breaking, which would want a crate of its own.
  `crates/ui-egui/tests/rtl_quality.rs` measures this and five other things a writer of
  Arabic would notice. **Not built:** a triple click that takes the line, and sorting names
  by Arabic collation rather than by code point.
- **Each word of a right-to-left line is placed by NoteCraft.** egui lays the letters of an
  Arabic or Hebrew word out the right way, but puts the next word to the right of the one
  before it, so a line of them came out with its words backwards; the owner read one on
  2026-10-19 and said so. `bidi.rs` draws each word on its own and places them from the
  right edge of the run leftwards, with the gaps measured rather than left in a galley.
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
- **A box's right-click menu is short.** It has Cut, Copy and Delete, then the page's entries. OneNote's also has
  Paste Options, Copy Link to Paragraph, Select, Order, Move and Resize, and a small formatting
  toolbar above it; none of those are here yet, and no entry is shown that does nothing. Move and
  resize are the bar and the right edge. A box is deleted without asking, as OneNote does, and
  Ctrl+Z brings it back: the page's history holds whole states of the page, boxes included.
- **Table column widths are still on a metadata line.** The boxes of a page are elements in its text,
  but the widths a table's columns were dragged to are kept in the head (`notecraft:columns`) or the
  front matter (`columns:`), so the same disconnect remains for them: another program does not see
  them. The code view does show them, because it shows the page file's head.
- **A drawing in the code view** is drawn, not shown as a wall of numbers: NoteCraft's own
  painter draws the strokes to fit the room, says how many there are, and Show the code puts
  the SVG one click away.
- **The code view** is the page's folder as it is: every file of the page down the left, the one
  that is picked on the right. The page file is picked when the view opens, and the whole of
  it can be typed in, head included - the owner asked for that on 2026-10-19, against the earlier
  line that only the body could be edited; the page is read again when the view closes, so a
  change to the title or any other metadata line takes effect then. A drawing's SVG can be typed in too - the owner asked for that on 2026-10-19, against the earlier line
  that every file but the page's own is read-only; the view says what the risk is, because the pen's
  readings are in that file. Pictures are shown as pictures, other text is read-only, and a file
  nothing on the page names is marked as not used. The live page is not drawn while the view is
  open, so there is only ever one editable copy of the page on screen. **Not built:** drawing an ink
  file as strokes in the viewer (it shows the SVG as text), and opening a file with the system.
- **Writing a page's text without boxes makes it one column.** The boxes are part of the text, so
  `page.save` with a plain body drops them. An agent that wants to keep an arrangement writes the box
  elements, or calls `page.containers` afterwards.
- **The Quick Access Toolbar is part of OneNote's.** It has Back, Forward, Undo and Redo, a menu to
  choose which show, to put it under the ribbon or to hide it, and the choices are kept. OneNote's
  menu also lists Print, Print Preview, Touch/Mouse Mode and More Commands: none exist here, so they
  are not listed. Back and Forward are also Alt+Left and Alt+Right and the mouse's side buttons. Undo
  covers the page's words and boxes (a typing burst is one step); ink keeps its own undo and has no
  redo, so Redo never brings a stroke back. In a text box Ctrl+Z is still the text box's own. When the
  strip is hidden a small arrow stays, so it can be brought back: OneNote hides it completely.
- **The mini toolbar is partial.** `crates/ui-egui/src/floating.rs` draws it, as data, so any menu can
  float one. Today it floats over the page's text menu with Bold, Italic, Strikethrough, Code, lists,
  indents, Quote, the To Do and Important tags and Clear Formatting. OneNote's also has a font and a
  size box, font colour, the highlighter, Styles and Format Painter: the editor has none of them, so
  they are not drawn. It does not appear on a box's menu (a selected box has no words for Bold to act
  on), beside a text selection, or on tables and pictures yet.
- **Menu entries.** Rows carry OneNote's pictures. Still missing from a box's menu: Paste Options with
  its pictures, Copy Link to Paragraph, Select, Order, Move and Resize.
- **Containers are a first pass.** A page can hold several boxes, each with its own place,
  width and blocks; a box is moved by the bar along its top and resized by its right edge,
  and the arrangement is kept in the page's own file. What is missing: making a new box by
  clicking an empty part of the page, moving blocks from one box to another, boxes that know
  how tall they are (the room under them is guessed), and a picture as a box of its own. The
  blocks themselves stay in reading order in the file, so a page read anywhere else is
  unchanged; if the counts and the blocks ever disagree, everything goes back into one box
  rather than losing a word.
- **Paper and page colour are the page's own.** Draw, then Format Background, gives the
  open page its rule lines or squares and a pale colour behind the words, and both are
  written into that page's head (`notecraft:paper`, `notecraft:page-color`), so a page
  keeps them and another page is not touched. The window's own rule-lines setting stays as
  what a page with none of its own is drawn on. **Not built:** a rule line colour, and
  "always create pages with rule lines".
- **Copy link to page** is on a page's menu in the tree, as it is in OneNote. The link
  names the page from the notebooks folder down (`notecraft:/book/section/page`), because
  a relative link would depend on the page it is pasted into, which nobody knows when the
  link is copied. It is copied as a Markdown link with the page's name, so pasting it into
  a page gives words to click, and clicking opens the page. A link of this kind cannot
  climb out of the notebooks folder, and a link to a page that has gone says so.
- **One title, not two.** A page carries its title in its head, and the window draws it in
  the title area above the words. A new page's body no longer repeats it. A page from
  before this, or one written by hand, may still open with that title as a heading; the
  live view leaves that heading out rather than showing the name twice, and the file is
  left exactly as it is, so the code view still shows it and another program still sees a
  page with a heading. Renaming moves both, as it did before.
- **The demo notebook** ships pictures and handwriting: every language's welcome page opens with
  the NoteCraft banner in that language, and the page about pictures and files carries a real
  `ink.svg`, drawn by the demo builder with the same crate the pen uses. The banner's words are in
  the picture only where the script needs no shaping; Arabic, Persian, Urdu, Hindi and Hebrew get
  the wordmark, and every language carries the line as words under it, which reads correctly and
  can be searched. Baking those five into the picture needs a text shaping library, which is not
  installed here. Three pages show paper and page colour: the one about writing is college
  ruled, the one about tags is squared and warm, the one about making it yours is teal.
- **Pen and drawing.** The pen writes anywhere on the page, as OneNote's does. The ink of a
  page is `ink.svg` in the page's own folder; a page never drawn on has no such file, and a
  page rubbed clean loses it. The folder is what decides: the page's own text names every
  ink file beside it and names none that is gone, checked when the page opens and after
  every change to it, so a drawing cannot be left unnamed by deleting the line that named
  it. An SVG counts as ink because it carries NoteCraft's namespace, not because of its
  name, so a picture of your own in the page's folder is never adopted as a drawing. The folder is what decides: the page's own text names every
  ink file beside it and names none that is gone, checked when the page opens and after
  every change to it, so a drawing cannot be left unnamed by deleting the line that named
  it. An SVG counts as ink because it carries NoteCraft's namespace, not because of its
  name, so a picture of your own in the page's folder is never adopted as a drawing. It is SVG that also carries the pen's own readings in our
  namespace, so NoteCraft can change a stroke later while anything else sees the picture.
  Built: the Draw tab; five nibs (pen, pencil, fountain pen, brush pen, highlighter); a
  rack the owner arranges, with Add, a right-click menu, a Modify Tool editor for colour
  and thickness, and the whole rack remembered between runs; a pressure sensitivity wedge;
  an eraser; undo; Save picture; picking ink up with Select or a lasso to move, stretch
  or delete it; and ten regular shapes - line, arrow, rectangle, rounded rectangle, square,
  ellipse, circle, diamond, triangle and star - drawn by dragging out the two corners they
  sit between. A drawn shape is ink like any other stroke, so the eraser, Select and Undo
  Stroke all work on it; it is not kept as a shape that can be turned back into a perfect
  circle later, and the file does not say which shape it was.
  **Not built:** automatic shapes; the ruler and other stencils;
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
  merging and splitting cells, shading, alignment, sorting; and the Insert Table dialog.
  A row has no height of its own on purpose: OneNote sizes a row by the text in it and
  offers no handle for one, which Microsoft's own help for OneNote for the web says in so
  many words, so a row here grows and shrinks with its words too. Word's Table Properties
  can set a height; if NoteCraft ever does, it should be Word's "at least" rule, where
  content taller than the height still pushes the row down rather than being cut off.
- **A spreadsheet goes to GridCraft**, the open spreadsheet from the same workshop, where
  OneNote converts a table to Excel. The Table tab writes the table out as a CSV file
  beside the page and opens it; the Insert tab starts an empty one and links it from the
  page. Both are greyed when GridCraft is not installed, which is looked for on the PATH
  and in the folder its installer uses (`%ProgramFiles%\GridCraft` on Windows). Checked
  against GridCraft 0.8.0 on 2026-10-19: a table with a comma and a quote in it opens in
  the right cells. The table goes out and comes back: Read back
  from it, beside Convert, puts what the spreadsheet holds into the table it came from,
  quotes and commas undone. Nothing watches the file, because a page would be a strange
  thing to write in if its tables changed under the writer's hands; the way back is a
  button, as the way out is.
- **Undo** takes back changes to the page: Ctrl+Z, Ctrl+Y or Ctrl+Shift+Z, and the two
  buttons at the left of the Home tab. A step is the whole page as it stood - its text,
  its blocks, the boxes they sit in and where the caret was - so anything that changes the
  page can be taken back, including a row deleted from a table or a box deleted whole. A
  run of letters into one block folds into one step, as it does in Word. The history is
  the open page's: it starts when the page opens and goes when another is opened, and at
  most 200 steps or 16 MB are kept. Ink is not in it; a stroke is taken back with Undo
  Stroke on the Draw tab, because a drawing is its own file beside the page.
- **A file in a page's folder** is listed under the page, and each row has a Delete that
  asks before it removes the file from disk. Undo does not bring a file back: the page's
  history holds the page, not the files beside it. A line that linked to the file is left
  as it was written, and shows as a link to something no longer there.
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

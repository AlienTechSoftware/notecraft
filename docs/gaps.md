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
- **Links are not clickable** in the preview.
- **No icons of the user's own** on a notebook or section. Colours are there (nine of them,
  on notebooks, section groups and sections); a chosen icon or picture is not.
- **Rule lines are a window setting, not a page setting.** View, Rule Lines draws none, lines
  or squares behind every page, and the choice is remembered; the page itself carries no
  setting, so it does not travel with the notebook.
- **The ribbon is a first pass.** Home, Insert and View with Office-style groups, a Styles
  gallery, a Tags gallery and a File screen. Missing: fonts and sizes, text colour and
  highlight, alignment, and the Draw, History, Review and Help tabs. Controls that are not
  built say so rather than doing nothing.
- **The caret is egui's, not ours.** Each block is an egui text box, so there is no caret of
  our own across blocks: no selection spanning two blocks, no bidi caret movement, and
  right-to-left text is laid out by egui, which does not run the bidi algorithm. The sibling
  WordCraft owns its whole text pipeline (shaping, line boxes, hit testing) and gets this
  right; matching a mature editor here means doing the same, or reusing PdfCraft's `bidi.rs` for a
  partial fix. Decision pending.
- **A dropped file can be copied in or linked.** A printout (a document rendered to pictures,
  as other apps insert one) is not built: it needs a renderer for PDF and Office files.
- **Templates picker, version history and recycle bin** are not built. (Tags are: to-do boxes
  and the other familiar marks, stored as plain text in the page.)
- **Sync conflicts** are only detected by name by the checker; there is no merge or resolve UI.

## Engineering

- **Windows only so far.** The code avoids OS features, but nothing has been built or run on
  macOS or Linux. Treat cross-platform as an unproven claim.
- **Coverage is 87.8% of lines** (measured 2026-10-17); window start-up and `app.rs` are the weakest.
- **Control key file permissions on Windows** rely on the user folder's access rules.
- **Large pages and attachments** are capped but not load tested.

## Legal and provenance

- **The koala artwork** was generated with an AI tool. Its licence terms have not been
  verified, so it is outside the project's licence. Treat it as a blocker before any public
  release that ships it. See `ATTRIBUTION.md`.
- **The licence is MIT OR Apache-2.0** for the code (the siblings' choice, taken 2026-10-17).
  The artwork above is outside it, and no audit of the dependencies' licences has been run.
- **Clean room:** the `.one` format work uses only Microsoft's published specifications and
  files the owner owns. No Microsoft code or text is copied.

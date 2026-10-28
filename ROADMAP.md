# Roadmap

Last reviewed: 2026-10-17. Status of the proof of concept, measured where a number is given.

## State today

- 292 tests pass across the workspace (`cargo test --workspace`): model 23, formats 38, math 22,
  store 41, engine 17, `.one` reader 29, MCP 15, command line 6, window app 3, UI library 62,
  UI behaviour and pixel tests 23, icon pair tests 4, xtask 9. Counts come from the last full
  run.
- Line coverage, measured 2026-10-17 with `cargo llvm-cov --workspace`: 87.8% of lines (5748 lines, 703 missed), 87.3% of regions. Lowest: window app main 29%, xtask 58%, `ui-egui` app.rs 70%. That measurement predates the MCP crate and has not been repeated.
- Built and run on Windows only.

## Done

- Folder format for notebooks, section groups, sections and pages, any depth. A new notebook
  starts with one section and one page, as people expect, and a new page carries its date.
- Two page formats (HTML default, CMGFM) with a settings cascade.
- Command registry shared by the window, the command line, the control channel and the MCP
  server.
- Native egui app: tree with coloured notebook and section icons in the familiar style, page with
  formatted writing (Markdown), equations, attachments, search, autosave, full page view.
- **The page editor**: a real caret on the formatted page, for HTML pages as well as Markdown
  ones, written back to the page's own format, with a round-trip gate that makes a page
  read-only rather than lose its markup.
- **The ribbon**: Home, Insert and View tabs with Office-style groups, the Styles and Tags
  galleries, a contextual Source tab, and the File screen. Controls that are not built say so.
- Tags: to-do boxes and the other familiar marks, stored as plain text.
- Colours per notebook, section group and section, stored in `_node.toml`.
- 14 themes with contrast checks and a sun or moon switcher.
- Control channel, headless screenshot runner, paired-comparison UI tests.
- **MCP server** (`notecraft-cli mcp`), headless or against a running window. See
  [`docs/mcp.md`](docs/mcp.md).
- The `.one` reader: the container (header, file node lists, object spaces, objects,
  property sets, inserted files) and the object model ([MS-ONE]: sections, pages, outlines,
  text runs, pictures, attachments), read only, with synthetic-file tests.
- Containers: a page is a sheet of boxes, each with its place, width and blocks, moved by
  the bar on top and resized by the right edge, kept in the page's own file.
- Arabic and right-to-left: Noto Naskh Arabic bundled, so the scripts draw at all; a
  direction for the page, for a paragraph and for a run, each written into the text itself as
  the Unicode mark or isolate, so it travels with the file; right-to-left lines drawn from the
  right edge, bullets, boxes and tag marks included; a line that mixes the two directions laid
  out by the Unicode algorithm (`crates/text`, over parley), with a caret of NoteCraft's own
  in it, because egui cannot place one in a shaped script.
- Gates: `cargo xtask ci`.

## Next (in this order)

1. **Import, the rest**: the importer runs (`notecraft-cli import onenote`) and has been tried on
   a real set of 443 backup files. Still to do: ink, tags and page dates, the notebook table of
   contents (`.onetoc2`) for section order and colours, `.onepkg` packages, an import command in
   the window, and password protected sections.
2. **Pasting what other apps really put on the clipboard**: a picture (Bitmap, DIB or PNG,
   which needs a clipboard crate such as `arboard`) and the HTML flavour, which keeps the
   formatting inside a pasted table. The plain-text flavour already comes back as a table.
3. The rest of the ribbon: Draw, History, Review and Help, fonts, size, colour and alignment.
4. **Containers, the rest of them**: a new box where you click, dragging blocks between
   boxes, a box that measures its own height, and a picture in a box of its own. Moving and
   resizing are done (see Done above).
5. Mermaid rendering (choose an approach first).
6. Clickable links, bold and italic fonts, page templates.
7. **Right-to-left, the rest of it**: selecting inside such a line (dragging, Shift with the
   arrows, a word on a double click), honouring the run isolates when drawing, a Latin face
   bundled so parley and egui shape the Latin with the same font, and a mirrored window for
   an interface in Arabic. Then the advanced options Word has, such as which digits to show.
8. Build and run on macOS and Linux; re-measure coverage.

## Open decisions for the owner

| Decision | Why it matters |
|---|---|
| Where and when to push | Nothing is committed or pushed yet. |
| Commit attribution lines | Two sources disagree. Resolve before the first commit. |

Settled: `.one` support is import only for now, and the code is licensed MIT OR Apache-2.0,
the same as the sibling Craft apps (owner, 2026-10-17).

See [`docs/gaps.md`](docs/gaps.md) for the full list of gaps and
[`docs/feature-parity.md`](docs/feature-parity.md) for feature parity.

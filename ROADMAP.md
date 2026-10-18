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
- Gates: `cargo xtask ci`.

## Next (in this order)

1. **Import, part three**: turn what the reader returns into notebook folders, with
   pictures and attachments, then try it on real backup files. The reader is done; this is the
   writing half.
2. The rest of the ribbon: Draw, History, Review and Help, fonts, size, colour and alignment.
3. Mermaid rendering (choose an approach first).
4. Clickable links, bold and italic fonts, page templates.
5. Right-to-left text and a caret that handles it. Two ways were weighed: a third-party text
   layer (parley) or NoteCraft's own shaping, as WordCraft does it.
6. Build and run on macOS and Linux; re-measure coverage.

## Open decisions for the owner

| Decision | Why it matters |
|---|---|
| Koala artwork licence | Unverified. Replace or confirm before release. |
| Where and when to push | Nothing is committed or pushed yet. |
| Commit attribution lines | Two sources disagree. Resolve before the first commit. |

Settled: `.one` support is import only for now, and the code is licensed MIT OR Apache-2.0,
the same as the sibling Craft apps (owner, 2026-10-17).

See [`docs/gaps.md`](docs/gaps.md) for the full list of gaps and
[`docs/feature-parity.md`](docs/feature-parity.md) for feature parity.

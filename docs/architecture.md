# NoteCraft architecture

Last reviewed: 2026-10-17. How the code is built today. Rules for working in the code are
in [`AGENTS.md`](../AGENTS.md). The on-disk format is in [`folder-format.md`](folder-format.md).

NoteCraft follows the layout of the other Craft apps (WordCraft, PdfCraft and their
siblings): a Cargo workspace of small crates in layers, one command registry that every front
end shares, a headless way to look at the UI, and gates that run with one command.

## Layers

`cargo xtask layers` enforces this table: a crate may only depend on crates in a lower layer,
and nothing below L6 may depend on egui, eframe, winit or rfd.

| Layer | Crate | Job |
|---|---|---|
| L0 | `model` | The shared vocabulary: node kinds, page formats, name and folder rules, ids, time, a tiny `key = "value"` reader. No file system, no dependencies. |
| L1 | `formats` | Text handling for HTML and CommonMark + GFM: front matter, the body of an HTML page, page templates, HTML to Markdown-like text for previews, and a Markdown parser into a block model. |
| L1 | `math` | A TeX subset parsed into a tree and laid out into drawing items (text, lines, paths) given a way to measure text. No UI dependency. |
| L1 | `onenote` | Reads `.one` section files: the revision store container, its objects and their property sets. Read only, no file system of its own. |
| L2 | `store` | Notebooks as plain folders: the tree, creating and deleting nodes, page content, attachments, settings, search, and the folder checker. |
| L4 | `engine` | `Session` and the **command registry**. Every user-visible action is a `CommandSpec`. |
| L5 | `mcp` | NoteCraft as an MCP server: the command registry over the Model Context Protocol, headless or against a running window. |
| L6 | `ui-egui` | The egui front end: tree with code-drawn icons, the page and how it is written on (`pagetext`), themes, math drawing. Swappable. |
| apps | `apps/notecraft`, `apps/notecraft-cli` | The desktop window (eframe) and the command line. |
| tools | `xtask` | `cargo xtask ci` and friends. |

```text
model  <--  formats  <--  store  <--  engine  <--  ui-egui  <--  apps/notecraft
  ^  ^                      ^           ^
  |  +----- math            |           +-----  mcp  <---------  apps/notecraft-cli
  +------------------------ +
```

(`math` is used by `ui-egui`; `formats` and `store` use `model`; `engine` uses all three
lower crates. The picture is a summary; `cargo xtask layers` is the rule.)

## Everything is a command

A user-visible action is a `CommandSpec` in `crates/engine/src/commands.rs`: an id such as
`node.create`, a label, a documented parameter list, a flag that says whether it changes the
tree, and a `run` function. The window, `notecraft-cli run`, the control channel and the MCP
server all dispatch by id. Programmatic calls never open dialogs. The list is in
[`commands.md`](commands.md), and a test keeps that file in step with the registry.

`Session::run` also catches a panic that escapes a command and turns it into
`EngineError::Panicked`, so a bug cannot take the app down. It is a safety net: the command
code itself has no `unwrap`, `expect` or `panic` outside tests.

UI-only state (which rows are expanded, the open page text, the theme) lives in the UI crate.
When something there should be scriptable, it becomes an engine command.

## Data flow

1. **Open a page.** The UI asks for `page.load`. `store::page::load_page` reads `index.html`
   or `index.md`, returning the editable part: the inside of the HTML body, or the Markdown
   after the front matter.
2. **Edit.** The text lives in the UI's editor. Each change re-parses a preview:
   `preview::blocks_for` turns HTML into Markdown-like text (`formats::html::to_markdown`) and
   parses it, or parses Markdown directly, into the same `Block` model.
3. **Draw.** `preview::show` draws blocks with egui. Inline and display math go through
   `math::layout`, measured with the UI's fonts and painted with the painter.
4. **Save.** About 0.9 seconds after the last keystroke, and on Ctrl+S, the UI sends
   `page.save`. `store::page::save_body` keeps the page's metadata, refreshes `updated`, and
   writes through a temporary file and a rename.
5. **Tree changes** (`node.create`, `node.delete`, `node.title`, `notebook.create`) make the
   session reload the tree from disk, so the UI always shows what is on disk.

## Themes

14 themes: light and dark, each with seven accents (default, red, green, yellow, blue, gray,
purple). `ui-egui/src/theme.rs` builds a `Tokens` set per theme, and every colour the UI uses
comes from it. The switcher is in the top bar and also offers to follow the system's light or
dark setting. The choice is saved in the settings file. Every text and accent colour is moved
until it meets its contrast target when the tokens are built, and a test checks all 14 themes.
See [`themes.md`](themes.md).

## Control channel

`notecraft --control PORT` opens a loopback JSON-lines channel that can run any engine command
and drive the window (click, type, theme, screenshot). The same methods run against an
off-screen window in `ui_shot` and in the tests. See [`control-protocol.md`](control-protocol.md).

## Writing on the page

The page area is the page, not a code box. `ui-egui/src/pagetext.rs` lays the text out the
way it will be read: headings in heading type, bold, links, quotes and code in their own
styles, and the Markdown marks themselves dimmed so the words come first. It is a pure
function from text to styled ranges, so it is tested without a window.

The source is shown only when the user asks for it with the Code button, side by side with
the page. An HTML page cannot be written on as text (its markup is tags, not marks in the
words), so it is shown the way it reads until the block editor is built. See
[`gaps.md`](gaps.md).

## Reading `.one` files

`crates/onenote` reads a `.one` section file into objects and property sets:

1. The header (1024 bytes) names the file type and points at the root file node list.
2. The file node lists are walked, honouring the transaction log, so an unfinished write is
   ignored.
3. Each object space collects its objects; a later declaration of the same object wins, which
   is how a revision store states the current version.
4. An object's data is a property set ([MS-ONESTORE] 2.6) or a reference to an inserted file.

What the objects mean ([MS-ONE]: sections, pages, outlines, text runs) is the next layer and
is not built yet. The crate never writes and has no UI dependency, so the importer can be
driven from the command line as easily as from the window.

## Equations and diagrams

- **Equations** are a TeX subset (`crates/math`): fractions, roots, superscripts and
  subscripts, sums, products and integrals with limits, scaled brackets, Greek letters,
  common symbols and text. The layout is pure and tested without a window. Anything the parser
  does not know stays visible as text.
- **Diagrams** (Mermaid) are stored as source, in a fenced block or `<pre class="diagram">`,
  and shown as source. Rendering is not built. Mermaid is JavaScript; a pure-Rust renderer or
  another approach is an open question in [`gaps.md`](gaps.md).

## Testing and gates

See [`testing.md`](testing.md). In short: unit tests in every crate, a hostile-input test that
runs every command with junk parameters, headless UI tests that drive the window through the
control channel, paired pixel comparisons (no committed golden pictures), and
`cargo xtask ci` which runs fmt, clippy with warnings denied, all tests, the layer check, the
asset attribution check, the documentation link check and the text style check.

## Prototypes

`prototypes/notebook-poc` is the first proof of concept: a standard-library-only command line
and a small web app, plus the cloud-sync stress test that was run against a real OneDrive
folder (see its README). It is not part of the workspace and is kept as a reference. The
folder rules it proved are now in `crates/store`.

## Reference material

`docs/research/` holds the notes this design rests on: Microsoft's published `.one` file
format specifications, paraphrased. They are notes, not Microsoft text. The feature list this
project was designed against is kept out of this repository.

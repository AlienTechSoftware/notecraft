# Testing NoteCraft

Last reviewed: 2026-10-17.

Quality is not negotiable here, so the project is tested on several levels. The UI testing
follows the method of the other Craft apps: no committed golden pictures, paired comparisons
rendered by the same renderer on the same machine, and a control channel that drives the real
window. Run everything the way the gate does:

```bash
cargo xtask ci
```

That runs, in order: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, the layer check, the asset attribution check, the documentation
link check, and the text style check. Each can be run alone: `cargo xtask fmt|clippy|test|layers|assets|docs|text`.

## What is tested, and how

| Level | Where | What it proves |
|---|---|---|
| Unit tests | in each crate (`#[cfg(test)]`) | Pure logic: names, slugs, Markdown and HTML conversion, TeX parsing and layout, settings, ids, time, themes, the control transport. |
| Store tests | `crates/store` | The folder format on a real temporary folder: creating nodes, the format cascade, deep nesting, unusual titles, both formats, attachments, search, the checker. |
| Engine tests | `crates/engine` | Every command through the registry, a full workflow with commands only, path confinement, and `hostile_params_never_panic`. |
| Command line tests | `apps/notecraft-cli` | A whole session through the command line, and error paths. |
| MCP tests | `crates/mcp` | The protocol lifecycle over a pipe, the tool list and its schemas, a notebook made, written, searched and checked through the tools, and the errors a tool reports instead of failing. |
| UI behaviour tests | `crates/ui-egui/tests/ui.rs` | The real window, driven headlessly through the control methods: open a page, toolbar buttons, dialogs, default format, theme, restart with the saved theme, a real TCP control connection, accessibility labels. |
| Pixel tests | same file | Rendered pixels have the theme's colours for all 14 themes; display math leaves ink where it should. |
| Icon tests | `crates/ui-egui/tests/icons_visual.rs` | Each code-drawn icon is rendered twice by the same renderer and compared; the pairs must match exactly, and negative controls prove the comparison can fail. |
| Repository gates | `xtask` | Layering, asset attribution, links in documents, no byte order marks, plain punctuation. |

## Why no golden pictures

A stored reference image breaks when a GPU, a driver or a font changes, and then people
regenerate it without looking. Instead, a test renders the thing under test and a reference
built in a different way in the same run, and compares them. Anything that differs by one pixel
fails, and a negative control (a deliberately wrong input) must fail too, so a broken
comparison cannot pass silently. Nothing is committed that depends on one machine.

## Looking at the window

Tests are not a substitute for looking.

- **Review captures.** Set `NOTECRAFT_UI_VISUAL_DIR` to a folder and run
  `cargo test -p notecraft-ui-egui --test ui`. The tests write PNGs there. Open them.
- **Scripted shots.** `ui_shot` renders any state off screen from a JSON-lines script, see
  [`control-protocol.md`](control-protocol.md). `cargo xtask screenshots` rebuilds the pictures
  used in the README from `scripts/readme-screens.jsonl`.
- **The running app.** `runpoc.cmd demo` opens the app on sample notebooks. With `--control 0`
  a tool can call `ui.screenshot` on the real window.

The tests render with egui's bundled fonts, so they do not depend on installed fonts.

## Hostile input

Code that reads files, parameters or user text is written to survive junk:

- Markdown, HTML and TeX parsers are run on empty text, unclosed markers, deeply nested
  markers, control characters and very long input.
- `hostile_params_never_panic` runs every engine command with empty, huge, traversal (`..`)
  and absolute-path parameters, and checks that nothing panics and nothing outside the
  notebooks folder is touched.
- The control channel refuses a wrong key, junk and over-long lines.
- Page and attachment sizes are capped; recursion depth is capped.

## Coverage

Line coverage needs `cargo-llvm-cov`, which is not installed by default (installing it needs
the maintainer's permission):

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
cargo xtask coverage
```

Last measurement (2026-10-17): 87.8% of lines, 87.3% of regions, workspace-wide. The weakest files are `apps/notecraft/src/main.rs` (29%, the window start-up), `xtask` (58%) and `crates/ui-egui/src/app.rs` (70%). Re-measure before quoting the number later.

## What is not tested yet

- Keyboard editing inside the text area (that behaviour is egui's).
- The OS file-drop event itself (the attach command is tested).
- macOS and Linux: the code avoids OS features, but it has only been built and run on Windows.
- Real cloud sync, beyond the one OneDrive test recorded in the prototype's README.
- Long documents and very large attachments.

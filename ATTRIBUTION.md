# Attribution

Every file under `assets/` and `docs/images/` has a row here. `cargo xtask assets` fails if one
is missing. Last reviewed: 2026-10-17.

## Brand artwork

| File | What | Origin | Licence status |
|---|---|---|---|
| `assets/brand/koala-tile.png` | The koala logo on a rounded purple tile (source) | Generated with an AI image tool at the owner's request, then cropped by the owner's instruction | **Unverified.** The tool's terms for commercial or open use were not checked. Blocker for a public release. |
| `assets/brand/notecraft-512.png` | Derived from the source: resized, transparent corners | Derived work of the row above | Same as the source |
| `assets/brand/notecraft-256.png` | Same, 256 px | Derived | Same as the source |
| `assets/brand/notecraft-128.png` | Same, 128 px | Derived | Same as the source |
| `assets/brand/notecraft-64.png` | Same, 64 px | Derived | Same as the source |
| `assets/brand/notecraft-32.png` | Same, 32 px | Derived | Same as the source |

## Documentation pictures

These are screenshots of NoteCraft itself, made by `cargo xtask screenshots` from sample
notebooks. They contain the koala artwork above and the app's own text. They hold no personal
data.

| File | What |
|---|---|
| `docs/images/hero.png` | The window as the README opens: tree, ribbon and a page |
| `docs/images/ribbon-home.png` | The Home tab of the ribbon |
| `docs/images/ribbon-insert.png` | The Insert tab of the ribbon |
| `docs/images/tags.png` | A page with to-do boxes and tags |
| `docs/images/file-screen.png` | The File screen with the notebook covers |
| `docs/images/markdown-page.png` | A Markdown page with the preview |
| `docs/images/html-page.png` | An HTML page with the preview |
| `docs/images/equations.png` | Inline and display equations |
| `docs/images/dark-green.png` | The dark green theme |
| `docs/images/theme-switcher.png` | The theme menu open |

## Code and fonts

- The icons in the tree and toolbar are drawn in code (`crates/ui-egui/src/icons.rs`). No icon
  files are used.
- The UI uses the fonts bundled with egui. They carry their own licences, which ship with the
  egui crates. Their terms were not re-read for this file.
- Dependencies are listed in `Cargo.lock`. A licence audit of them (for example with
  `cargo deny`) has not been run.
- NoteCraft's own code is dual-licensed under MIT or Apache-2.0, at your option: `LICENSE-MIT`,
  `LICENSE-APACHE`, `NOTICE`. The rows above are not covered by it where they say so.
- The `.one` file format notes in `docs/research/` paraphrase Microsoft's published
  specifications. No Microsoft text or code is included.

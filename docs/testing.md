# Testing NoteCraft

Last reviewed: 2026-11-12.

Quality is not negotiable here, so the project is tested on several levels. The UI testing
follows the method of the other Craft apps: no committed golden pictures, paired comparisons
rendered by the same renderer on the same machine, and a control channel that drives the real
window. Run everything the way the gate does:

```bash
cargo xtask ci
```

That runs, in order: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, the layer check, the asset attribution check, the documentation
link check, the text style check, and the check that the Welcome notebook in
`docs/demo/DemoNotebook` is current with its content. The default `cargo xtask demo` generates
both formats. Use `cargo xtask demo html` or `cargo xtask demo cmgfm` to generate one format,
or `cargo xtask demo both` to request both explicitly. Each can be run alone:
`cargo xtask fmt|clippy|test|layers|assets|docs|text|demo-check`.

## What is tested, and how

| Level | Where | What it proves |
|---|---|---|
| Unit tests | in each crate (`#[cfg(test)]`) | Pure logic: names, slugs, Markdown and HTML conversion, TeX parsing and layout, settings, ids, time, themes, the control transport. |
| Page representation parity | `crates/formats/src/render.rs`, `crates/formats/src/boxes.rs`, `crates/formats/tests/format_parity.rs` | Block and inline-markup round trips, box layout round trips, and semantic equality for every page in the complete HTML and CMGFM demo notebooks. This is a required gate against the two serializations drifting apart. |
| Store tests | `crates/store` | The folder format on a real temporary folder: creating nodes, the format cascade, deep nesting, unusual titles, both formats, attachments, search, the checker. |
| Engine tests | `crates/engine` | Every command through the registry, a full workflow with commands only, path confinement, and `hostile_params_never_panic`. |
| Command line tests | `apps/notecraft-cli` | A whole session through the command line, and error paths. |
| Install provisioning tests | `apps/notecraft-setup` | Creates or replaces only the managed Welcome notebook, keeps sibling user notebooks unchanged, rejects incomplete payloads before replacing an existing copy, and refreshes a deleted copy on the next launch. |
| Install payload checks | `packaging/*`, `.github/web-build`, Android builder contract tests | Windows, Linux, macOS, Android and browser payloads include both generated formats and all 13 language folders; release page version and archive checks guard the website bundle. |
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

The canonical UI automation and screenshot workflow is [`nc-uiux-testing`](../SKILLS/nc-uiux-testing/SKILL.md).
This document keeps the repository testing overview; read the skill for capture modes,
visual evidence rules, and platform automation boundaries.

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

Coverage needs `cargo-llvm-cov`, which is not installed by default (installing it needs
the maintainer's permission):

```bash
rustup component add llvm-tools
cargo install cargo-llvm-cov --locked
cargo xtask coverage            # measure, and compare with docs/coverage.json
cargo xtask coverage --update   # measure, and make this run the new baseline
```

The baseline is [`coverage.json`](coverage.json), one entry per platform: the total line,
function and region coverage, line coverage per crate, and when and with what it was
measured. It covers the workspace without `xtask`. A run is compared with its own platform
only, because code for one platform counts only where it is built. `--update` writes the
current platform and keeps the others. `badges` holds the README figures, with "not measured"
for a platform that has no baseline yet. Linux is measured in CI; Windows on a Windows machine.

The comparison warns and never fails. A figure that falls by more than 0.5 points is printed
as a warning, and the task still succeeds. On a pull request the `coverage` job in
[`ci.yml`](../.github/workflows/ci.yml) runs it, shows each drop as an annotation and puts the
table on the run's summary page; the job is allowed to fail, so it never blocks a merge. It is
not part of `cargo xtask ci` or the pre-push hook, because a coverage run rebuilds everything.

When a change raises coverage, run `--update` in the same pull request, so the new figure
lands with the code that earned it and git keeps the history.

## Continuous integration

[`ci.yml`](../.github/workflows/ci.yml) runs the same gates on every pull request and on main:
fmt, clippy, the static checks, coverage, and a build and test on Linux, Windows and macOS.

- **Prose-only pull requests skip the Rust jobs.** When every changed path is top-level
  Markdown, `SKILLS/`, `work/`, or `docs/` other than the files tests read (`docs/demo/`,
  `docs/commands.md`, `docs/coverage.json`), the build, test, clippy and coverage jobs are
  skipped. fmt and the static checks (links, text style, PII) still run. Pushes to main always
  run everything. The `CI` status job fails if the change filter itself fails, so a skip can
  never pass by accident.
- **UI tests wait by the clock, not by counting sleeps.** `settle` in
  `crates/ui-egui/tests/ui.rs` gives background work half a second measured with a clock. The
  hosted macOS runners oversleep about five times, so counting twenty-five 20 ms sleeps made the
  UI suite take 14 minutes there instead of 3.
- **UI tests share one offscreen GPU device per test binary** (`tests/support/gpu.rs`), as the
  icon tests do. Each window still gets its own renderer and textures.
- **Independent matrix scenarios are separate tests.** The formatting-shortcut matrix keeps
  all 24 theme, format, style and modifier combinations, each with its own real temporary
  notebook, settings and harness. The test runner can schedule them across workers, and a
  failure names its exact combination. The scenario assertions and background waits stay
  the same. The existing `formatting_shortcuts_match_ribbon` filter still runs the full matrix.

## Clipboard in tests

No test reads or writes the system clipboard. The app takes its clipboard as a boundary (see
[ADR 0001](adr/0001-clipboard-boundary.md)), and the UI tests give it an in-memory clipboard from
`crates/ui-egui/tests/support/clipboard.rs`. That double is the one mock the rules allow (see
`AGENTS.md`). Copies are read from it, and the UI suite runs in parallel.

## What is not tested yet

- Keyboard editing inside the text area (that behaviour is egui's).
- The OS file-drop event itself (the attach command is tested).
- macOS and Linux: the code avoids OS features, but it has only been built and run on Windows.
- Real cloud sync, beyond the one OneDrive test recorded in the prototype's README.
- Long documents and very large attachments.

For focused serialization and editor checks, see [Literal text regression checks](testing-literal-text.md).

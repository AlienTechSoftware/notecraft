# Notebook fixtures

`docs/demo/DemoNotebook/html/` and `docs/demo/DemoNotebook/cmgfm/` are complete copies of the
Welcome demo notebook. They contain the same pages, languages, links, diagrams, images, and
ink, serialized entirely as HTML or CMGFM. The app embeds the full HTML notebook; the CMGFM
notebook stays in the repository for manual comparison and regression checks.

`test-notebook` is a small, quick fixture for checking the two renderers side by side. Open the
repository's `tests` folder as the notebooks folder, then open both pages under
**Format parity test** > **Format examples**.

The automated comparison checks all pages in both full demo notebooks:

```text
cargo test -p notecraft-formats --test format_parity
cargo run -p notecraft-cli -- --notebooks tests check test-notebook
```

The built-in Welcome notebook opens on its HTML start page.

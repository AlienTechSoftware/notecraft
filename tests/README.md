# Notebook fixtures

`docs/demo/DemoNotebook/html/` and `docs/demo/DemoNotebook/cmgfm/` are complete copies of the
Welcome demo notebook. They contain the same pages, languages, links, diagrams, images, and
ink, serialized entirely as HTML or CMGFM. The app embeds the full HTML notebook; the CMGFM
notebook stays in the repository for manual comparison and regression checks.

These are two file-format encodings of one product demo, not separate samples. Keep their
content, page metadata, features, links, assets, and layout settings in sync. With the same
viewport, zoom, theme, and app settings, their rendered pages must be nearly pixel-for-pixel
alike. A visual difference is a parity bug unless it comes only from platform text
rasterization.

`test-notebook` is a small, quick fixture for checking the two renderers side by side. Open the
repository's `tests` folder as the notebooks folder, then open both pages under
**Format parity test** > **Format examples**.

The automated comparison checks all pages in both full demo notebooks for matching paths,
metadata, shared files, and semantic page blocks. It does not compare rendered pixels, so
review both versions side by side at the same settings after any rendering or demo-content
change. Run the automated comparison with:

```text
cargo test -p notecraft-formats --test format_parity
cargo run -p notecraft-cli -- --notebooks tests check test-notebook
```

The built-in Welcome notebook opens on its HTML start page.

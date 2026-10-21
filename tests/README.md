# Notebook fixtures

`test-notebook` is a small notebook for checking HTML and CMGFM rendering side by side. The
`HTML` and `CMGFM` folders keep each format's sample page and assets together. The pages have
the same content and differ only in their page format and identifying title. Open the
repository's `tests` folder as the notebooks folder, then open both pages under
**Format parity test** > **Format examples**.

The automated comparison reads the same fixture:

```text
cargo test -p notecraft-formats --test format_parity
cargo run -p notecraft-cli -- --notebooks tests check test-notebook
```

The built-in Welcome notebook remains the user-facing demo and opens on its HTML start page.

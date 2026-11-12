# Literal text and editor regression checks

The page writers quote literal text only where the supported Markdown syntax would otherwise change it. The live editor retains literal and inline-image identity separately, so storage escapes do not become new characters when a line takes focus. New Markdown typed during an edit still formats normally.

## Automated checks

Use the repository toolchain pin, including rustfmt and clippy.

```sh
cargo test -p notecraft-formats --locked
cargo test -p notecraft-ui-egui --lib --locked
cargo clippy -p notecraft-formats -p notecraft-ui-egui --all-targets -- -D warnings
cargo xtask ci
```

The format suite covers literal delimiters, backslashes before letters and punctuation, styled boundaries, table pipes, native block prefixes and box boundaries. The demo parity suite reads all bundled pages and checks three successive saves. Editor tests cover actual text widgets, re-entry, exact replacement ranges, Unicode selections, inline images, table cells, split/join, cross-block deletion, tags, block kinds, Clear Formatting and clipboard fragments. Clipboard fragment tests inspect the queued product output and do not read or write the system clipboard.

When comparing branches, use separate Cargo target directories. A shared target can reuse a same-name workspace crate from another checkout based on source timestamps. If seeding cached third-party artifacts, remove all workspace-owned package artifacts before testing the new checkout.

## Manual acceptance

1. Create a disposable HTML page. In its code view, put this paragraph inside the body:

```html
<p>Cost $5 or $6, *literal*, [x](y), C:\Users\reader</p>
```

2. Return to the page, click that line, append words, leave it and click it again. The literal punctuation and path stay unchanged; persistence backslashes must not appear in the working text.
3. Select and replace a literal run, then type a new bold or italic run. The replacement uses its new characters, while untouched literal text keeps its identity. Check a mixed Unicode line and a line using the font controls.
4. Split and join the line, switch its block kind or tag, and clear formatting. Copy and Cut selections within and across paragraphs. The selected words must reach the destination, including their literal punctuation.
5. Put literal delimiters in a table cell. Edit it, clear its formatting, move to another cell and return. Its text must remain present.
6. Save, close and reopen three times. Repeat with a CMGFM page and with text inside a positioned box. Confirm the same text and layout on every cycle.

This change does not extend the existing inline-code grammar to code spans containing embedded backticks. That independent parser limitation remains.

# Code blocks and numbered code

Status: built 2026-10-22. Folds into [page-model.md](page-model.md) when agreed.

A code block is `Block::Code { lang, text }`. `lang` is the block's own info string, so the
formats carry it as they carry any language.

| Style (Styles gallery, by its label) | `lang` | CMGFM | HTML |
|---|---|---|---|
| Code | `rust`, or empty | ` ```rust ` | `<pre class="rust">` |
| 123|code | `123\|rust`, or `123\|` for no language | ` ```123\|rust ` | `<pre class="123\|rust">` |

- `123|` is the one name for "number the lines". `notecraft_formats::markdown::{NUMBERED, code_lang}` split a
  `lang` into whether it is numbered and the language, so no other code spells the prefix.
- A page read by something that knows nothing of the prefix shows the code under an odd language name and
  loses nothing.
- The numbers are for reading. A numbered block shows them on the page and none while the caret is in it,
  as a diagram shows its picture on the page and its source while it is written in. Both views use the same
  box, size and language label (`preview.rs`, `pageedit.rs`), so the only change on entering is the numbers.
- Changing between the two styles keeps the words and the language; it does not re-read the words as
  Markdown. The same style again turns the block back into ordinary text.
- Not decided: syntax colouring of the language in the page, and wrapping long lines beside the numbers.

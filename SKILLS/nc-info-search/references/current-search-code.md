# Current search code (checked 2026-11-22)

Source: `crates/store/src/search.rs`, read in this check.

## What it does

1. `search(root, needle, limit)` calls `search_in(root, &[], needle, limit)`.
2. `search_in` lowercases the needle, collapses its whitespace, and returns nothing for an empty
   needle or a zero limit.
3. It collects every page folder under each notebook (`child_dirs` from the notebooks folder,
   plus the notebooks opened from elsewhere). `collect_pages` recurses through child folders and
   keeps each folder that has a page file.
4. It removes duplicates by canonical path, so a page reached through a link is searched once.
5. For each page it loads the page (`load_page`) and builds its plain text:
   - HTML pages: `html::strip_tags` on the body.
   - CommonMark pages: the body is read into blocks, rendered to HTML, then stripped.
   - Whitespace is collapsed.
6. The searched text is the node title followed by the plain text, lowercased.
7. A hit is the first position where the whole lowercased needle occurs as a substring. The
   snippet is 30 characters before and 70 after, whitespace collapsed.
8. Iteration stops when `limit` hits are found.

## What it does not do

- No index. Every query loads every page file.
- It does not search picture alt text. The alt text sits in an attribute, and `strip_tags` drops
  attributes.
- It does not search OCR text (`<picture>.txt`, or the proposed `<picture>.ocr.json`) or attachments.
- No ranking. Hits come back in folder-walk order, not by relevance.
- No stemming, no tokenising, no per-word matching. A phrase must occur exactly, ignoring case.
- Non-ASCII case folding uses `to_lowercase` on `char`s; tests check `CAF\u{c9}` against `café`.

## Tests that pin the behaviour

`crates/store/src/search.rs` tests:
- `finds_text_in_both_formats_and_in_titles`: tags between words are ignored; case-insensitive;
  non-ASCII; the title is searched.
- Further tests cover linked notebooks and limits (see the file).

## Why this matters for the design

Every hybrid design choice starts from these limits: no index, no ranking, no alt or OCR text.
The keyword index must read the same page content and must also read the new companion files.

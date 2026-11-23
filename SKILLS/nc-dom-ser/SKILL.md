---
name: nc-dom-ser
description: Serialize and deserialize NoteCraft HTML and CMGFM pages while preserving semantic, structural, and visual parity.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-code-rust, nc-prod, nc-arch]
---

# nc-dom-ser - NoteCraft Format Serialization

## Job to be done

Serialize and deserialize NoteCraft page content while preserving semantic, structural, and cross-format parity.

## Core Requirement (Hard Rule)

Both HTML and CommonMark formats MUST remain:
- **Identical** in notebook structure and content
- **Pixel-near-identical** in visual rendering at 1x zoom, light theme
- Only difference: file format (`.html` vs `index.md`)

---

## Model and view (block is the object, the control is its view)

- **The block is the model.** A block (`Block` in `crates/formats/src/markdown.rs`) holds
  everything the page stores about it: its kind, its text and any data the user chose for it.
  Anything the files must keep belongs on the block and is written to both HTML and CMGFM.
- **Controls are views of the block.** The `+` button, its popup, hover and focus are drawn from
  the block and hold only view state (for example, which popup is open). They never hold data
  that a file should keep.
- **Data the user picks is model data.** An emoji chosen on a block is stored on that block,
  round-trips through both formats, and has a round-trip test. A store kept by the view, keyed
  by page path and block position, is not allowed: it loses data when blocks move and is lost
  on restart.
- **One view entry point per block.** Every path that draws a block (page editor, boxes,
  read-only views) calls one function that draws the block frame and its header control as
  children of the frame. No call site draws block chrome itself.

---

## Supported Block Types (Parity Required)

Every block type in HTML MUST have a CommonMark equivalent:

| Block | HTML | CommonMark | Status |
|-------|------|-----------|--------|
| Heading | `<h1>-<h6>` | `# ... ######` | ✓ |
| Paragraph | `<p>` | Blank lines | ✓ |
| Code | `<pre><code>` | ` ``` lang ` | ✓ |
| Table | `<table>` | Markdown table | ✓ |
| Image | `<img src="...">` | `![alt](src)` | ✓ |
| List (bullet) | `<ul><li>` | `- item` | ✓ |
| List (ordered) | `<ol><li>` | `1. item` | ✓ |
| Quote | `<blockquote>` | `> text` | ✓ |
| Math inline | `<span class="math">` | `$...$` | ✓ |
| Math block | `<div class="math">` | `$$...$$` | ✓ |
| Rule | `<hr>` | `---` | ✓ |
| Strikethrough | `<s>` | `~~text~~` | ✓ |

**Rule:** Never add a block type to one format only.

---

## Inline Formatting (Must Match)

| Format | HTML | CommonMark |
|--------|------|-----------|
| **Bold** | `<strong>` | `**text**` |
| *Italic* | `<em>` | `*text*` |
| `Code` | `<code>` | `` `text` `` |
| [Link](url) | `<a href="...">` | `[text](...)` |
| ![Image](url) | `<img>` | `![alt](src)` |

---

## Asset Paths (Relative & Identical)

```
Page.html
index.md              (Same page, different format)
Page_assets/
  ├── image.png
  ├── code.txt
```

**Both formats reference same file:**
- HTML: `<img src="Page_assets/image.png">`
- MD: `![](Page_assets/image.png)`

---

## Page Properties (Head Metadata)

A page property is an attribute of the page object (`store::page::Page`), not of a block. It is
written in the page's head, never in the body, and it is read in the same read as the body.

| Property (object field) | HTML head (`<meta>`) | CMGFM front matter | Value | Absent means |
|---|---|---|---|---|
| `justify` | `notecraft:justify` | `justify` | list of block positions | nothing justified |
| `columns` | `notecraft:columns` | `columns` | table widths | default widths |
| `read_only` | `notecraft:read-only` | `read-only` | `true` | editable |

Rules for every page property:

1. **Default is absent.** A page that never set a property has no line for it. Setting the default
   removes the line, so an unlocked page is the same file it was before it was locked.
2. **Only one value is written.** Read-only writes `true`. Anything else reads as the default.
3. **A body save keeps it.** Saving the words must not drop a property (`save_body` preserves the
   head). Each property has a test that saves a body and reads the property back.
4. **Both formats, both ways.** A property is written and read in HTML and in CMGFM, with a
   round-trip test for each format, and `crates/formats/tests/format_parity.rs` lists its key so the
   two editions cannot drift.
5. **Engine command.** The window changes a property only through an engine command (for
   `read_only`, `page.read_only`), with a test.

## Round-Trip Testing (Required)

Every format must serialize losslessly:

```rust
#[test]
fn html_round_trip() {
  let page = parse_html("<p>Test</p>")?;
  let html = serialize_html(&page)?;
  let page2 = parse_html(&html)?;
  assert_eq!(page.blocks, page2.blocks);
}

#[test]
fn md_round_trip() {
  let page = parse_markdown("# Title")?;
  let md = serialize_markdown(&page)?;
  let page2 = parse_markdown(&md)?;
  assert_eq!(page.blocks, page2.blocks);
}
```

---

## Demo Notebook Sync

**Location:** `docs/demo/DemoNotebook/{html,cmgfm}/`

**Never edit one without the other.** When updating content:

1. Edit source in `docs/demo/scripts/content/`
2. Generate both: `cargo xtask demo-copy html` + `cargo xtask demo-copy cmgfm`
3. Verify parity (same files, visual diff)
4. Commit both together

---

## Checklist Before Merging

- [ ] Block type in both HTML and CommonMark
- [ ] Round-trip tests pass (both formats)
- [ ] Cross-format test passes (HTML → MD → HTML)
- [ ] Demo notebooks in sync and visually identical
- [ ] Asset paths relative and identical
- [ ] No format-only features (unless documented)

---

**Last updated:** 2026-11-22 | **Blocking requirement:** Yes

## Reference: WordCraft and NoteCraft document models

See `references/wordcraft_x_notecraft_dom.md` for the comparison of the two document models and what it means for caret, runs and inline objects.

## Reference: object types

See `references/object-types.md` for the object identifiers, Rust names, WordCraft and OneNote equivalents, and the room for future object kinds.

See `references/naming-convention.md` for the rules that the identifiers, Rust names, HTML classes and data attributes follow.

RFC 0001, the proposed NoteCraft document model with its integration appendix: `references/rfc-0001-notecraft-dom.md`.

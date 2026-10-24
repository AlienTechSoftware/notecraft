---
name: nc-dom-ser
description: 'NoteCraft DOM/Format Serialization: HTML and CommonMark dual-format support with semantic parity. Enforces hard requirement: both formats identical in structure and visual rendering.'
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-code-rust, nc-prod, nc-arch]
---

# nc-dom-ser - NoteCraft Format Serialization

## Core Requirement (Hard Rule)

Both HTML and CommonMark formats MUST remain:
- **Identical** in notebook structure and content
- **Pixel-near-identical** in visual rendering at 1x zoom, light theme
- Only difference: file format (`.html` vs `index.md`)

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

**Last updated:** 2026-10-24 | **Blocking requirement:** Yes

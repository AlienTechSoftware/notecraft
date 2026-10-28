# NoteCraft Format Fidelity Matrix

**Complete reference for all supported content types, objects, and their representations in HTML and CommonMark/GFM formats.**

## Quick Reference: Change Impact Guide

Use this matrix to decide which boundaries and checks a change needs. It is a routing aid, not a replacement for the normative page model.

| Change affects | Inspect | Minimum proof |
|---|---|---|
| Block or span meaning | `crates/formats/src/markdown.rs`, parser, renderers | model round trip in both formats |
| HTML syntax | HTML reader and `render.rs` | HTML parse/write plus format parity |
| CMGFM syntax | Markdown parser and writer | CMGFM parse/write plus format parity |
| Containers or placement | `crates/model`, `formats::boxes`, page editor | layout round trip and page test |
| Page head or metadata | `crates/store/src/page.rs`, folder format | save/reload with unrelated head preserved |
| Attachments or ink | page body links, assets, `ink.svg`, code view | file-list agreement and reload |
| Editor behavior | `crates/ui-egui`, engine commands | integrated UI/control test and reload |
| Code view | source pane and file listing | visible file ownership and edit round trip |
| Demo content | authored content and generated demo | `demo-check`, semantic parity, visual side-by-side |

---

## Block-Level Content Types

### Text Blocks

| Type | HTML | CommonMark | Status | Notes |
|------|------|-----------|--------|-------|
| **Headings** |
| Heading 1 | `<h1>text</h1>` | `# text` | ✓ Full parity | Six levels (h1-h6) supported |
| Heading 2 | `<h2>text</h2>` | `## text` | ✓ Full parity | |
| Heading 3 | `<h3>text</h3>` | `### text` | ✓ Full parity | |
| Heading 4 | `<h4>text</h4>` | `#### text` | ✓ Full parity | |
| Heading 5 | `<h5>text</h5>` | `##### text` | ✓ Full parity | |
| Heading 6 | `<h6>text</h6>` | `###### text` | ✓ Full parity | |
| **Body Text** |
| Paragraph | `<p>text</p>` | Line with blank line separator | ✓ Full parity | Standard block; multiple paras separated by blank lines |
| Blockquote | `<blockquote><p>text</p></blockquote>` | `> text` (can nest) | ✓ Full parity | Container for quoted content; supports nesting |
| **Code Blocks** |
| Code (fenced) | `<pre><code class="language-xyz">` | ` ```xyz ` | ✓ Full parity | Language tag for syntax highlighting |
| Code (indented) | `<pre><code>` | 4-space indent | ✓ Full parity | Legacy format, supported in both |
| **Structural** |
| Horizontal rule | `<hr>` | `---` or `***` | ✓ Full parity | Visual page divider |

### Lists

| Type | HTML | CommonMark | Status | Notes |
|------|------|-----------|--------|-------|
| **Unordered List** |
| Bullet item | `<ul><li>item</li></ul>` | `- item` or `* item` | ✓ Full parity | Single-level or nested |
| Nested bullets | `<ul><li>item<ul><li>nested</li></ul></li></ul>` | Indented `  - nested` | ✓ Full parity | Supports arbitrary depth |
| **Ordered List** |
| Numbered item | `<ol><li>item</li></ol>` | `1. item` | ✓ Full parity | Auto-numbered in output |
| Nested numbers | `<ol><li>item<ol><li>nested</li></ol></li></ol>` | Indented `  1. nested` | ✓ Full parity | Mixed nesting allowed |
| **Task List** |
| Unchecked task | `<li><input type="checkbox"> Task</li>` | `- [ ] Task` | ✓ Full parity | GFM extension |
| Checked task | `<li><input type="checkbox" checked> Done</li>` | `- [x] Done` | ✓ Full parity | Checked state preserved |

### Tables

| Type | HTML | CommonMark | Status | Notes |
|------|------|-----------|--------|-------|
| **Table Structure** |
| Header row | `<thead><tr><th>header</th></tr></thead>` | First row + separator row | ✓ Full parity | GFM table format |
| Data row | `<tbody><tr><td>cell</td></tr></tbody>` | `\| cell \|` | ✓ Full parity | Multiple rows supported |
| Table cell | `<td>content</td>` | Pipe-delimited | ✓ Full parity | Can contain inline formatting |
| Cell alignment | `<td align="left/center/right">` | `:---`, `:---:`, `---:` | ✓ Full parity | Left/center/right justify |

---

## Inline Formatting & Emphasis

| Format | HTML | CommonMark | Status | Notes |
|--------|------|-----------|--------|-------|
| **Emphasis** |
| Bold | `<strong>text</strong>` | `**text**` or `__text__` | ✓ Full parity | Strong emphasis |
| Italic | `<em>text</em>` | `*text*` or `_text_` | ✓ Full parity | Emphasis |
| Bold + Italic | `<strong><em>text</em></strong>` | `***text***` or `___text___` | ✓ Full parity | Combined |
| Strikethrough | `<s>text</s>` or `<del>text</del>` | `~~text~~` | ✓ Full parity | GFM strikethrough |
| Subscript | `<sub>text</sub>` | `~text~` (proposed) | ⚠ Limited | Not standard GFM; HTML-primary |
| Superscript | `<sup>text</sup>` | `^text^` (proposed) | ⚠ Limited | Not standard GFM; HTML-primary |
| **Links & References** |
| Hyperlink | `<a href="url">text</a>` | `[text](url)` | ✓ Full parity | External or internal |
| Link with title | `<a href="url" title="Title">text</a>` | `[text](url "Title")` | ✓ Full parity | Tooltip on hover |
| Reference link | `<a href="url">text</a>` | `[text][ref]\n[ref]: url` | ✓ Full parity | Link ref defined elsewhere |
| **Code & Math** |
| Inline code | `<code>text</code>` | `` `text` `` | ✓ Full parity | Monospace; no formatting inside |
| Code (escaped backticks) | `<code>\`text\`</code>` | ``` `` `text` `` ``` | ✓ Full parity | Literal backticks in code |
| Math inline | `<span class="math">$...$</span>` | `$...$` | ✓ Full parity | KaTeX/TeX syntax; wrapped |
| **Images** |
| Inline image | `<img src="path" alt="text">` | `![alt](path)` | ✓ Full parity | Inline media reference |
| Image with title | `<img src="path" alt="alt" title="Title">` | `![alt](path "Title")` | ✓ Full parity | Tooltip shown on hover |
| **Special Characters** |
| Hard line break | `<br>` | Two spaces + newline, or `\` | ✓ Full parity | Preserved in both formats |
| Soft break | Newline | Newline | ✓ Full parity | Treated as space in output |

---

## Math & Science Content

| Type | HTML | CommonMark | Storage | Rendering |
|------|------|-----------|---------|-----------|
| **Inline Math** |
| Math expression | `<span class="math">$E=mc^2$</span>` | `$E=mc^2$` | LaTeX/KaTeX source | MathJax render on display |
| Subscripts | `$CO_2$` | `$CO_2$` | LaTeX | Rendered in display |
| Superscripts | `$E=mc^2$` | `$E=mc^2$` | LaTeX | Rendered in display |
| **Block Math** |
| Display equation | `<div class="math">$$E=mc^2$$</div>` | `$$E=mc^2$$` | LaTeX/KaTeX source | MathJax display mode |
| Multiline equation | `<div class="math">$$...align...$$</div>` | ` ```math ... ``` ` or `$$...$$` | LaTeX | Display-mode rendering |
| **Chemical Notation** |
| Chemical formula | `<span class="chem">H₂O</span>` | `H~2~O` or `H₂O` | Unicode/HTML | Standard subscript format |

---

## Diagrams & Visualizations

### Mermaid Diagrams

| Diagram Type | HTML | CommonMark | Storage | Rendering |
|--------------|------|-----------|---------|-----------|
| **Flowcharts** |
| Flowchart | `<pre class="mermaid">graph LR...</pre>` | ` ```mermaid graph LR... ``` ` | Mermaid source | Rendered via mermaid.js |
| **Sequence** |
| Sequence diagram | `<pre class="mermaid">sequenceDiagram...</pre>` | ` ```mermaid sequenceDiagram... ``` ` | Mermaid source | Sequence chart output |
| **Structural** |
| Class diagram | `<pre class="mermaid">classDiagram...</pre>` | ` ```mermaid classDiagram... ``` ` | Mermaid source | OOP structure |
| Entity-relation | `<pre class="mermaid">erDiagram...</pre>` | ` ```mermaid erDiagram... ``` ` | Mermaid source | Database schema |
| State machine | `<pre class="mermaid">stateDiagram-v2...</pre>` | ` ```mermaid stateDiagram... ``` ` | Mermaid source | State transitions |
| **Timelines** |
| Gantt chart | `<pre class="mermaid">gantt...</pre>` | ` ```mermaid gantt... ``` ` | Mermaid source | Project timeline |
| Timeline | `<pre class="mermaid">timeline...</pre>` | ` ```mermaid timeline... ``` ` | Mermaid source | Event timeline |
| **Data Viz** |
| Pie chart | `<pre class="mermaid">pie title...</pre>` | ` ```mermaid pie... ``` ` | Mermaid source | Pie segments |
| Bar chart | `<pre class="mermaid">bar...</pre>` | ` ```mermaid bar... ``` ` | Mermaid source | Bar graph |
| Line chart | `<pre class="mermaid">line...</pre>` | ` ```mermaid line... ``` ` | Mermaid source | Line graph |
| **Mind Map** |
| Mind map | `<pre class="mermaid">mindmap...</pre>` | ` ```mermaid mindmap... ``` ` | Mermaid source | Hierarchical map |
| **Git Graph** |
| Git graph | `<pre class="mermaid">gitGraph...</pre>` | ` ```mermaid gitGraph... ``` ` | Mermaid source | Branch visualization |

### Data Visualization (Vega & D3)

| Type | HTML | CommonMark | Storage | Notes |
|------|------|-----------|---------|-------|
| **Vega Specs** |
| Vega spec | `<div class="vega" data-spec="...">` | ` ```vega {...json...} ``` ` | JSON specification | Rendered via Vega-Lite |
| Vega-Lite spec | Custom container | Code fence with JSON | JSON + metadata | High-level grammar |
| **D3 Visualizations** |
| D3 chart | `<svg class="d3">...SVG output...</svg>` | Not natively supported | SVG output (not source) | HTML-primary; MD shows placeholder |
| Custom D3 | `<div id="chart">` + script | Not supported | HTML + JavaScript | Interactive charts |

### Hand-Drawn & Geometric Content

| Type | HTML | CommonMark | Storage | Editing |
|------|------|-----------|---------|---------|
| **Ink Drawings** |
| Freehand stroke | `<svg class="ink"><path stroke="...">` | `[Hand-drawn image]` | SVG paths in assets | Ink editor UI |
| Annotated ink | `<svg class="ink" data-id="...">` | Text description | SVG + metadata | Preserve layer info |
| **Shapes** |
| Rectangle | `<rect x="..." y="..." width="..." height="...">` | Not supported | SVG element | SVG visual |
| Circle/Oval | `<circle cx="..." cy="..." r="...">` | Not supported | SVG element | SVG visual |
| Line | `<line x1="..." y1="..." x2="..." y2="...">` | Not supported | SVG element | SVG visual |
| Polygon | `<polygon points="...">` | Not supported | SVG path | SVG visual |
| Free shape | `<path d="...">` | Not supported | SVG path data | SVG visual |
| **Styling** |
| Shape fill | `fill="color"` or `fill="url(#gradient)"` | Not supported | SVG fill attribute | Color or gradient |
| Shape stroke | `stroke="color"` `stroke-width="..."` | Not supported | SVG stroke | Line color/width |
| Opacity | `opacity="0.5"` or `fill-opacity="..."` | Not supported | SVG attribute | Transparency |
| Shadow/Effect | `<filter><feDropShadow...>` | Not supported | SVG filter element | Visual effects |

---

## Media & Attachments

### Images

| Format | HTML | CommonMark | Storage | Status |
|--------|------|-----------|---------|--------|
| **Raster Images** |
| PNG | `<img src="page_assets/file.png">` | `![alt](page_assets/file.png)` | `page_assets/` folder | ✓ Full support |
| JPEG/JPG | `<img src="page_assets/file.jpg">` | `![alt](page_assets/file.jpg)` | `page_assets/` folder | ✓ Full support |
| WebP | `<img src="page_assets/file.webp">` | `![alt](page_assets/file.webp)` | `page_assets/` folder | ✓ Full support |
| GIF (static) | `<img src="page_assets/file.gif">` | `![alt](page_assets/file.gif)` | `page_assets/` folder | ✓ Full support |
| GIF (animated) | `<img src="page_assets/anim.gif">` | `![alt](page_assets/anim.gif)` | `page_assets/` folder | ✓ Full support (plays) |
| **Vector Images** |
| SVG | `<img src="page_assets/file.svg">` | `![alt](page_assets/file.svg)` | `page_assets/` folder | ✓ Full support |
| SVG (inline) | `<svg>...content...</svg>` | Not supported natively | Inline in HTML | ⚠ HTML-primary |
| **Image Properties** |
| Dimensions | `width="..." height="..."` | Not in Markdown | Implied from file | Preserved in HTML |
| Alt text | `alt="description"` | `![description](...)`  | Both formats | ✓ Full parity |
| Title/caption | `title="caption"` | `![...](... "caption")` | Both formats | ✓ Full parity |

### Document Attachments

| Type | HTML | CommonMark | Storage | Access |
|------|------|-----------|---------|--------|
| **Documents** |
| PDF | `<a href="page_assets/file.pdf">` | `[PDF](page_assets/file.pdf)` | `page_assets/` folder | ✓ Downloadable link |
| Word (.docx) | `<a href="page_assets/file.docx">` | `[Document](page_assets/file.docx)` | `page_assets/` folder | ✓ Downloadable link |
| Excel (.xlsx) | `<a href="page_assets/file.xlsx">` | `[Spreadsheet](page_assets/file.xlsx)` | `page_assets/` folder | ✓ Downloadable link |
| PowerPoint (.pptx) | `<a href="page_assets/file.pptx">` | `[Presentation](page_assets/file.pptx)` | `page_assets/` folder | ✓ Downloadable link |
| Text file (.txt) | `<a href="page_assets/notes.txt">` | `[Text](page_assets/notes.txt)` | `page_assets/` folder | ✓ Downloadable link |
| **Generic Files** |
| Any file | `<a href="page_assets/file.ext">Download</a>` | `[File](page_assets/file.ext)` | `page_assets/` folder | ✓ Downloadable link |

### Audio & Video

| Type | HTML | CommonMark | Storage | Playback |
|------|------|-----------|---------|----------|
| **Video** |
| MP4 | `<video src="page_assets/video.mp4">` | `[Video](page_assets/video.mp4)` | `page_assets/` | ✓ HTML5 player |
| WebM | `<video src="page_assets/video.webm">` | `[Video](page_assets/video.webm)` | `page_assets/` | ✓ HTML5 player |
| Fallback (multiple sources) | `<video><source src="..." type="..."></video>` | Multiple links | `page_assets/` | ✓ Browser picks best |
| **Audio** |
| MP3 | `<audio src="page_assets/sound.mp3">` | `[Audio](page_assets/sound.mp3)` | `page_assets/` | ✓ HTML5 player |
| OGG | `<audio src="page_assets/sound.ogg">` | `[Audio](page_assets/sound.ogg)` | `page_assets/` | ✓ HTML5 player |
| WAV | `<audio src="page_assets/sound.wav">` | `[Audio](page_assets/sound.wav)` | `page_assets/` | ✓ HTML5 player |

---

## Page Metadata & Structure

| Element | HTML Storage | CommonMark Storage | Format | Status |
|---------|--------------|-------------------|--------|--------|
| **Document Metadata** |
| Page title | `<title>` tag + `<h1>` | YAML front matter: `title:` | Plain text | ✓ Full parity |
| Page ID | Meta `id` attribute | Front matter: `id:` | UUID/identifier | ✓ Full parity |
| Created date | Meta `created` | Front matter: `created:` | ISO 8601 | ✓ Full parity |
| Modified date | Meta `modified` | Front matter: `modified:` | ISO 8601 | ✓ Full parity |
| Format type | Meta `format="html"` or `"cmgfm"` | Front matter: `format:` | Enum | ✓ Full parity |
| Author/Owner | Meta `author` | Front matter: `author:` | Email/name | ✓ Full parity |
| **Internationalization** |
| Direction (LTR/RTL) | `dir="ltr"` or `dir="rtl"` on `<html>` | Front matter: `dir:` | Enum | ✓ Full parity |
| Language | `lang="en-US"` | Front matter: `lang:` | BCP 47 tag | ✓ Full parity |
| **Presentation** |
| Custom CSS class | `class="custom-style"` on body | Front matter: `css_class:` | String | ✓ Full parity |
| Theme (light/dark) | CSS variable override | Front matter: `theme:` | Enum | ✓ Full parity |

---

## Specialized Content Objects

### Tags & Annotations

| Marker | HTML Display | CommonMark | Visual Mark | Editable |
|--------|--------------|-----------|------------|----------|
| To Do | `☐` or checkbox | `- [ ]` in lists | Checkbox | ✓ Toggle |
| Done | `☑` or checked | `- [x]` in lists | Checked box | ✓ Toggle |
| Important | `⭐` prefix | `⭐ text` | Star icon | ✓ Toggle |
| Question | `❓` prefix | `❓ text` | Question mark | ✓ Toggle |
| Idea | `💡` prefix | `💡 text` | Light bulb | ✓ Toggle |
| Remember | `📌` prefix | `📌 text` | Pushpin | ✓ Toggle |
| Definition | `📖` prefix | `📖 text` | Book | ✓ Toggle |
| Highlight | `🎨` prefix | `🎨 text` | Palette | ✓ Toggle |
| Person/Contact | `👤` prefix | `👤 text` | Person icon | ✓ Toggle |
| Location | `📍` prefix | `📍 text` | Pin icon | ✓ Toggle |
| Phone | `📞` prefix | `📞 text` | Phone icon | ✓ Toggle |
| Website | `🌐` prefix | `🌐 text` | Globe icon | ✓ Toggle |
| Critical/Alert | `⚠️` prefix | `⚠️ text` | Warning | ✓ Toggle |

### Containers & Layout Boxes

| Type | HTML | CommonMark | Nesting | Status |
|------|------|-----------|---------|--------|
| **Single Column** |
| Default content | No wrapper | Plain blocks | N/A | ✓ Standard |
| **Multi-Column** |
| Two-column box | `<div class="box" cols="2">` | Nested blockquotes (workaround) | Limited | ⚠ Layout support |
| Three-column box | `<div class="box" cols="3">` | Not natively supported | Limited | ⚠ Limited support |
| **Positioned** |
| Positioned box | `<div class="box" style="position:absolute...">` | Not supported | N/A | ⚠ HTML-only |
| Sidebar | `<aside>` or `<div class="sidebar">` | Not supported | N/A | ⚠ HTML-only |

### Printouts & Export

| Format | From HTML | From CommonMark | Engine | Output |
|--------|-----------|-----------------|--------|--------|
| **Print** |
| PDF (print) | ✓ browser print-to-PDF | ✓ browser print-to-PDF | Browser | PDF file |
| Page break | `<div class="page-break">` | `---` (mimics) | CSS/HTML | Hard page break |
| Print styles | `@media print { }` | N/A | CSS | Print-only styling |
| **Clipboard** |
| Rich text (RTF) | ✓ via copy handler | ✓ via copy handler | HTML clipboard | RTF with formatting |
| Plain text | ✓ stripped | ✓ stripped | Text | No formatting |
| HTML source | ✓ as-is | N/A | HTML | Raw HTML code |
| Markdown source | N/A | ✓ as-is | Markdown | Markdown code |
| **Screenshot** |
| PNG screenshot | ✓ Print as image | ✓ Rendered then image | Canvas API | Image file |

---

## Fidelity Rules (Non-Negotiable)

### Rule 1: Content Parity
Every piece of content in HTML format **must** exist identically in CommonMark format. No HTML-only content without explicit documentation of the gap.

### Rule 2: Visual Parity
At 1x zoom in light theme, both formats **must** render pixel-near identically:
- Same font sizes, weights, and spacing
- Same colors and contrast ratios
- Same line heights and block spacing
- Same image sizing and placement

### Rule 3: Metadata Parity
All page metadata (title, id, created, modified, direction, language) **must** be present in both:
- HTML: in `<meta>` tags, `<title>`, and semantic HTML
- CommonMark: in YAML front matter block

### Rule 4: Asset Path Parity
All asset references (images, files, attachments) **must** use:
- Relative paths from the page directory
- Identical structure: `page_assets/filename`
- No absolute paths or format-specific references

### Rule 5: Round-Trip Testing
Every page **must** survive a full round trip without data loss:
- HTML → parse → DOM → serialize → HTML (bitwise or semantically identical)
- MD → parse → DOM → serialize → MD (semantically identical; whitespace may normalize)
- Cross-format: HTML → MD → HTML (lossless bidirectional)

### Rule 6: No Format-Only Features
If a feature exists in one format, provide an equivalent in the other:
- If CommonMark cannot natively support it, document the workaround
- If truly impossible, mark as `⚠ Limited Support` and document in `docs/gaps.md`

---

## Questions Before Editing

1. **What is the canonical semantic value?** - Not the rendering, the data meaning
2. **Where is it serialized in HTML and CMGFM?** - Both representations of that meaning
3. **Which metadata or companion file carries it?** - Attributes, front matter, separate assets?
4. **Can another program read the page without NoteCraft and still see the data?** - Portability
5. **What happens to unknown input or a format that cannot represent it?** - Graceful degradation
6. **Which test proves read, write, reload, and cross-format behavior?** - Coverage

---

## Known Gaps & Workarounds

| Gap | Root Cause | HTML Approach | CommonMark Approach | Status |
|-----|-----------|---------------|--------------------|--------|
| **Freehand Ink** | No native MD vector drawing | SVG paths in assets | Text placeholder | ⚠ Documented |
| **Geometric Shapes** | MD has no shape primitives | SVG elements | ASCII art or text desc | ⚠ Limited |
| **Multi-Column Layout** | MD has no box model | CSS `columns` or flexbox | Nested blockquotes | ⚠ Limited |
| **Positioned Elements** | MD has no absolute positioning | CSS `position: absolute` | N/A | ⚠ HTML-only |
| **Superscript/Subscript** | Not in GFM spec | `<sup>`, `<sub>` HTML tags | `^text^` or `~text~` (proposed) | ⚠ Non-standard |
| **Custom Fonts** | Not in HTML/MD spec | `<link rel="stylesheet">` + CSS | `@import` font in front matter | ⚠ Limited |
| **Advanced D3 Charts** | No native MD visualization | `<svg>` output or `<div>` holder | Code fence with spec | ⚠ Experimental |
| **Video Streaming** | HLS/DASH not std in HTML | `<video>` with HLS source | Link to video URL | ⚠ Limited |

---

**Last Updated:** 2026-10-27  
**Format Status:** Complete and authoritative  
**Sync Requirement:** All content types must achieve parity before release

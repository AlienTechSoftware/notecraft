# NoteCraft folder format

Status: draft 1, 2026-10-17. This is the contract between NoteCraft and the file system.
The code in `crates/store` implements it and `notecraft-cli check` verifies it. If the
code and this document disagree, that is a bug in one of them: fix both in the same change.

Last reviewed: 2026-10-17. Version of the format: `0` (unstable; anything can still change).

## 1. Principles

1. **Everything is a folder.** A notebook, a section group, a section and a page are all
   folders. There is no database and no index file that the format depends on.
2. **Plain files inside.** A page is one HTML or Markdown file. Attachments are ordinary
   files in the page's folder. Any editor, backup tool or sync tool can handle them.
3. **No depth limit.** A page can hold sub-pages, which can hold sub-pages, to any depth.
   The `.one` format stops at three levels, because the limit is written into it
   (`PageLevel` is 1 to 3 in MS-ONE). NoteCraft does not have it.
4. **The folder is the unit.** Moving, copying, renaming or deleting a page folder moves,
   copies, renames or deletes the page with its sub-pages and attachments.
5. **Cross-platform.** Names and paths follow the intersection of Windows, macOS and Linux
   and the limits of common sync tools (section 8). Nothing core depends on an OS feature.
6. **A setting changes new pages, never old ones.** The format of a page is visible from its
   file name, so no setting is needed to read a page.

## 2. The tree

```text
Notes/                         the notebooks folder (any folder; not a node itself)
  my-notebook/                 notebook            _node.toml
    work/                      section group       _node.toml
      projects/                section             _node.toml
        project-x/             page                index.html + assets/
          index.html
          assets/
            diagram.png
          meeting-notes/       sub-page            index.md + assets/
            index.md
            assets/
        deep-page/             page, with sub-pages nested to any depth
          level-1/
            level-2/
              level-3/ ...
    ideas/                     section             _node.toml (format = "cmgfm")
      format-ideas/            page
        index.md
        assets/
          koala.png
```

A new notebook is not empty: like the apps people come from, it is created with one section, "New Section 1",
holding one page, "Untitled page". The page carries its title and the date it was started.
`notebook.create` with `empty=true` skips both, which is what the sample notebooks use.

| Kind | Folder contains | Allowed children |
|---|---|---|
| Notebook | `_node.toml` | section group, section, page |
| Section group | `_node.toml` | section group, section, page |
| Section | `_node.toml` | page |
| Page | `index.html` or `index.md`, `assets/` | page (sub-pages) |

Rules:

- The notebooks folder holds notebooks. Any folder that is not a node (no `_node.toml` and
  no `index.*`) is ignored.
- The kind of a folder comes from its files: a folder with `index.html` or `index.md` is a
  page. Otherwise `_node.toml` says `kind = ...`.
- Child folders named `assets`, and folders whose names start with a dot, are not nodes.
- A page may have both content and children. A section or group has no content file.

## 3. Page files

A page folder has exactly one content file:

| Format id | File | Meaning |
|---|---|---|
| `html` | `index.html` | HTML. The built-in default format. |
| `cmgfm` | `index.md` | CommonMark plus GitHub Flavored Markdown. |

If a folder has both (a mistake, usually a merge), HTML wins and `check` reports it.

### 3.1 HTML pages

```html
<!doctype html>
<html>
<head>
<meta charset="utf-8">
<title>Project X</title>
<meta name="notecraft:id" content="18df6ffd89c4bdc00001">
<meta name="notecraft:order" content="10">
<meta name="notecraft:created" content="2026-10-17T21:58:31Z">
<meta name="notecraft:updated" content="2026-10-17T21:58:41Z">
</head>
<body>
<h1>Project X</h1>
...
</body>
</html>
```

NoteCraft edits only what is inside `<body>`. The head is kept as it is, except that
`notecraft:updated` is refreshed on save. Metadata in `<meta name="notecraft:...">`:
`id`, `order`, `created`, `updated`.

Embedded things, in the body:

- Equations: `<span class="math">$ E = mc^2 $</span>` (TeX between dollar signs).
- Diagrams: `<pre class="diagram">graph TD; A--&gt;B;</pre>` (Mermaid source).
- To-do lines: a paragraph that starts with `&#9744;` (open) or `&#9745;` (done).
- Attachments: relative links such as `<img src="assets/diagram.png">`.

A strict HTML subset for pages (no scripts, no event attributes) is planned; it is not
enforced yet. See `docs/gaps.md`.

### 3.2 Markdown pages

```markdown
---
id: 18df74d7835ef6240000
title: "Format ideas"
order: 10
created: 2026-10-17T23:27:25Z
updated: 2026-10-17T23:27:25Z
tags: []
---

# Format ideas

- [x] a task list item
```

- Front matter is a block of `key: value` lines between two `---` lines at the top. A leading
  byte order mark (some editors add one) is accepted and dropped on the next save.
- Supported Markdown: headings, paragraphs, bullet, numbered and task lists (nested by
  indentation), block quotes, fenced code, tables, rules, images, bold, italic,
  strikethrough, inline code, links. Equations are `$...$` and `$$ ... $$` (TeX). A fenced
  block with the language `mermaid` is a diagram. This is a subset of CommonMark and GFM,
  not a full implementation. See `crates/formats/src/markdown.rs` for the exact list.

### 3.3 Metadata keys

| Key | Meaning | Required |
|---|---|---|
| `id` | Stable identity. Today a timestamp plus a counter (20 hex characters), unique on one machine. To be replaced by a ULID before notebooks merge across devices. | yes |
| `title` | The title shown in the tree. The folder name is a slug of the title at creation and does not follow later renames. | no (falls back to the folder name) |
| `order` | Position among siblings (integers; new nodes get 10, 20, 30, ...). Ties and missing values sort by folder name. | no |
| `created`, `updated` | UTC time, `YYYY-MM-DDTHH:MM:SSZ`. | no |
| `tags` | Reserved. | no |

Unknown keys are kept. Times in front matter and `<meta>` are the truth; file modification
times are not used, because sync tools change them.

### 3.4 Boxes on a page

A page can be a sheet of boxes instead of one column, as an outline is in OneNote. Each box is an
element in the page's own text, around the blocks it holds, so the file says what the window shows
and any program that opens it can see the structure.

HTML, in the body:

```html
<div class="nc-box" data-x="40" data-y="20" data-width="320"
     style="position:absolute;left:40px;top:20px;width:320px">
<h1>Meeting</h1>
<p>Agenda</p>
</div>
```

Markdown, after the front matter. The blank line after the opening tag and before the closing tag
keeps the inside ordinary Markdown:

```md
<div class="nc-box" data-x="40" data-y="20" data-width="320">

# Meeting

Agenda

</div>
```

| Part | Meaning |
|---|---|
| `class="nc-box"` | Marks the element as a box. |
| `data-x`, `data-y` | Points from the left and top of the page. Whole numbers. |
| `data-width` | How wide the box is, in points. Never less than 120. |
| `style` (HTML only) | For a browser, so the page opens looking like the window. NoteCraft reads the `data-` attributes and ignores `style`. |

Rules:

- Boxes are not nested. A `div` that is not a box, inside a box, is part of its words.
- The blocks stay in reading order. The first box holds the first blocks, and so on.
- Words outside every box (a hand edit) belong to the box before them, or get a box of their own
  at the top left when there is none. Nothing is dropped.
- A page nobody arranged has no boxes in its text and is read as one column. Opening or writing on
  such a page does not add any.
- A box with nothing in it is written as an empty element and kept.
- A line in Markdown that looks like a box inside a fenced code block is only code.
- An older page kept its boxes on one line instead: `containers: x,y,width,blocks; ...` in the
  Markdown front matter, or `<meta name="notecraft:containers" content="...">` in the HTML head.
  That is still read when the text has no boxes, and it is taken out the next time the page is
  saved with its boxes in the text.

## 4. Containers: `_node.toml`

```toml
kind = "section"
title = "Ideas"
id = "18df6a03a67901240000"
format = "cmgfm"
order = 20
color = "teal"
created = "2026-10-17T20:09:00Z"
```

| Key | Meaning |
|---|---|
| `kind` | `notebook`, `section-group` or `section`. |
| `title`, `id`, `order`, `created` | As in 3.3. |
| `format` | Optional. The default format for new pages below this node: `html` or `cmgfm`. A notebook always records one when it is created. |
| `color` | Optional. The colour of this notebook cover or section tab, the way notebook apps colour them: `purple`, `blue`, `teal`, `green`, `yellow`, `orange`, `red`, `magenta` or `gray`. A name, not a hex value, so it stays readable in every theme. An unknown name is ignored. |

The file is read with a small reader that understands `key = "value"` and `key = number`
lines, `#` comments and blank lines. It is not a full TOML parser; NoteCraft writes only
this simple form.

## 5. Attachments

Everything in a page's `assets` folder is an attachment: images, audio, video, PDF, any
file. A page links to an attachment by a relative path (`assets/name.ext`).

- Names are made safe: no path parts, only letters, digits, `.`, `-` and `_`, at most 80
  characters, never a reserved name. Two attachments never share a name, ignoring case
  (`pic.png`, `pic-2.png`).
- Attachments larger than 512 MB are refused.
- Files in `assets` that no page links to are still attachments and are listed in the
  editor.
- Other files placed beside `index.*` by other tools are left alone.

## 6. The format setting

The format of a new page, first match wins:

1. The format chosen for that page when it is created.
2. The nearest `format` in a `_node.toml` going up from the parent, stopping at the notebook.
3. The global setting (`default_format` in the settings file, outside any notebook).
4. The built-in default, `html`.

A setting never converts an existing page. A notebook records its own effective format when
it is created, so it behaves the same on a machine with other settings.

Format ids: `html`, `cmgfm`. The names `markdown`, `md`, `gfm` and `commonmark` are accepted
as aliases for `cmgfm`.

## 7. Where settings live

| What | Where |
|---|---|
| Global settings (`default_format`, `theme_mode`, `theme_accent`) | `NOTECRAFT_SETTINGS`, else `%APPDATA%\NoteCraft\settings.toml`, else `$XDG_CONFIG_HOME/notecraft/settings.toml`, else `$HOME/.config/notecraft/settings.toml` |
| The notebooks folder | `--notebooks`, else `NOTECRAFT_NOTEBOOKS`, else `Documents/NoteCraft` in the home folder |

Settings never live inside a notebook, so a shared notebook carries no personal settings.

## 8. Names and sync rules

Checked by `notecraft-cli check` and enforced when nodes are created. The OneDrive limits
come from secondary sources (a Microsoft support article mirrored by university IT sites)
and are not verified for every sync tool.

- Characters `" * : < > ? / \ |` and control characters are not allowed in a name.
- No leading or trailing space, and no trailing dot.
- No Windows reserved names (`CON`, `PRN`, `AUX`, `NUL`, `COM0` to `COM9`, `LPT0` to
  `LPT9`), also with an extension. Titles that would make one get a `-page` suffix.
- No `desktop.ini`, `.lock`, names containing `_vti_`, or names starting with `~`.
- At most 255 characters per name. Folder names made from titles are cut to 40 characters.
- A full path from the sync root should stay under 400 characters. Windows long paths and
  macOS and Linux allow more locally; a sync tool may not.
- Two names in one folder must not differ only by case (Windows and default macOS volumes
  are case-insensitive). New folder names are made unique ignoring case.
- Files named like conflict copies of `index.*` or `_node.toml` (for example
  `index-LAPTOP.html` or `index (1).md`) are reported by `check`. The naming patterns come
  from secondary sources.
- Unicode normalization: macOS stores some names in decomposed form. Folder names made by
  NoteCraft are lower-case letters and digits from the title; no normalization is applied
  yet, which is an open risk (see `docs/gaps.md`).

## 9. Writing pages safely

- A page is written to a temporary file next to it and renamed over the original, so a
  crash cannot leave half a page.
- Pages over 50 MB are refused.
- A delete is confined to the notebooks folder and to NoteCraft nodes.

## 10. What this format does not define yet

- A strict HTML subset, and how a free-form canvas would be stored.
- Version history of pages (git or file history is the intended answer).
- Links between pages by id, and how renames update them. Today a page links to another
  page or folder by relative path, as it links to its pictures (`../other-page/`), and the
  app follows such a link; a rename breaks it.
- Tags and templates.
- Encryption of a section.
- How `.one` files map onto it. A plan is in `docs/research/onenote-file-format/ms-one-notes.md`.

## 11. Checks

`notecraft-cli check NOTEBOOK` reports: both `index.md` and `index.html` in a folder, a
missing or duplicate `id`, a folder name that breaks the rules above, a path over 400
characters from the notebook's parent, files that look like conflict copies, and names that
differ only by case. It exits with status 1 if there is any problem.

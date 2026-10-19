# Choose the page format

A page is one of two things on disk:

| Format | File | Good for |
|---|---|---|
| **HTML** (the built-in default) | `index.html` | Rich layout, and pages that came from somewhere else |
| **CommonMark + GFM** | `index.md` | Clean diffs and merges, and reading the file by itself |

Both are edited the same way, and both carry the page's metadata with them: front matter in the
Markdown page, `<meta name="notecraft:...">` tags in the HTML page.

## Where the choice is made

First match wins:

1. The format you pick while creating the page.
2. The nearest `format` in a `_node.toml` above it, stopping at the notebook.
3. The global setting, the **New pages** format box at the right of the status bar.
4. The built-in default, HTML.

A notebook records its own default when it is created, so it behaves the same on a machine with
other settings. Changing a setting never rewrites pages that already exist: it only decides what
new pages get.

## Setting it

- **For one page**: in the Add dialog, pick the format.
- **For a section or a notebook**: in its Add dialog, the format you pick is the default for
  pages below it. In the file: `format = "cmgfm"` in its `_node.toml`.
- **Globally**: the **New pages** box in the status bar, or
  `notecraft-cli run settings.default_format.set format=cmgfm`.

The current page's format is shown at the top right of the page, beside whether it is saved.

## Changing a page's format

There is no converter yet. Create a page in the format you want and move the text across; the
block editor writes whichever format the page already has.

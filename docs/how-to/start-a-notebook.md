# Start a notebook

## Where notebooks live

NoteCraft keeps every notebook in one folder. By default that is `Documents\NoteCraft` on
Windows and `~/Documents/NoteCraft` elsewhere. To use another one:

```sh
notecraft --notebooks "D:\Notes"
```

or set `NOTECRAFT_NOTEBOOKS` once and forget it. The folder is shown at the bottom right of the
window and on the File screen.

## The first notebook

The first time NoteCraft starts on an empty notebooks folder it adds one for you: **Welcome to
NoteCraft**, a tour of the app written as an ordinary notebook, and opens it on its start page.
It is yours to change or delete. **File**, then **New**, then **Add the welcome notebook**
brings it back, or opens it if it is still there.

For a notebook of your own, click **New notebook** in the top bar, or **File**, then **New**.
Give it a name.

Like the app you are coming from, a new notebook is not empty: it gets a section called "New Section 1" with one
page called "Untitled page" inside it, and the page carries the date you started it. You can
rename both: right-click, then **Rename**.

On disk:

```text
Notes/
  my-first-notebook/
    _node.toml
    new-section-1/
      _node.toml
      untitled-page/
        index.html
        assets/
```

## Sections, groups and pages

Hover a row in the tree and click **+**, or right-click it and choose **Add inside**.

- A **notebook** can hold section groups, sections and pages.
- A **section group** holds sections, for notebooks that grow.
- A **section** holds pages.
- A **page** can hold more pages. That is how sub-pages work, and there is no limit on the
  depth. Other apps stop at three.

Everything is a folder, so you can also make one in your file manager: create the folder, put a
`_node.toml` in it (or an `index.md` for a page) and NoteCraft picks it up the next time it
reads the tree.

## Moving and deleting

Move a notebook, section or page by moving its folder. The page keeps its pictures because they
live inside it.

Delete from the right-click menu. NoteCraft asks first and tells you how many items go with it.
There is no recycle bin yet, so the deletion is real; if your notebooks folder is in a synced or
version-controlled folder, your sync tool's own history is the undo.

## Colours

Right-click a notebook, group or section, choose **Colour**, and pick one of nine. The colour
shows on the icon and on the edge of the row, the way a notebook cover or a
section tab, and it is written into `_node.toml` as `color = "teal"`.

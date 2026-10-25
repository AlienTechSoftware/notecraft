# Add pictures and files

Drag a file from your file manager onto an open page. NoteCraft asks what to do with it, the
way you are used to:

- **Insert the picture** (or the video, or the recording, depending on the file): a copy goes
  into the page's `assets` folder and the page links to it with a relative path. The notebook
  carries it everywhere.
- **Attach the file**: the same copy, shown as a link rather than a picture.
- **Link to the file where it is**: nothing is copied. The page gets a `file:` link to the
  original. The link breaks if the file moves.

**Insert**, then **Picture** or **File** in the ribbon opens a file picker. **File Printout** opens
the same kind of picker and inserts a PDF or Office document as one picture per page.

## Where they go

```text
project-x/
  index.html
  assets/
    diagram.png
    budget.xlsx
```

Names are made safe: no path parts, only letters, digits, `.`, `-` and `_`, at most 80
characters, never a name Windows reserves. Two files never share a name, ignoring case, so
`pic.png` and a second `pic.png` become `pic.png` and `pic-2.png`. A file over 512 MB is
refused.

Anything you drop into an `assets` folder yourself is an attachment too; NoteCraft lists what it
finds under the page.


## Pasting from another app

Copying a table out of another note taking app and pasting it here keeps the table: the
clipboard carries the rows as text with a tab between cells, and NoteCraft turns that back
into a grid. If the richer HTML flavour reaches it, that is used instead, which keeps the
marks inside the cells. Several lines without tabs become several paragraphs.

Pasting a **picture** from the clipboard is not built: the window's toolkit hands over text
only, so NoteCraft never sees the picture flavour. Drag the file onto the page, or save it
and use **Insert**. [docs/gaps.md](../gaps.md) says what it would take.

## Printouts

Every page of a PDF or Office document is rendered to a picture on the page, while the original
file is kept as an attachment. Windows releases bundle Poppler's `pdftoppm`; development builds
may use a local Poppler installation or `NOTECRAFT_PDFTOPPM`. LibreOffice is needed to convert
Office documents. If a renderer is unavailable, NoteCraft reports the problem and does not insert
a partial printout.

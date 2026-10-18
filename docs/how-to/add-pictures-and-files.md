# Add pictures and files

Drag a file from your file manager onto an open page. NoteCraft asks what to do with it, the
way you are used to:

- **Insert the picture** (or the video, or the recording, depending on the file): a copy goes
  into the page's `assets` folder and the page links to it with a relative path. The notebook
  carries it everywhere.
- **Attach the file**: the same copy, shown as a link rather than a picture.
- **Link to the file where it is**: nothing is copied. The page gets a `file:` link to the
  original. The link breaks if the file moves.

**Insert**, then **Picture** or **File** in the ribbon does the same thing and tells you to drop
the file on the page, because NoteCraft has no file picker of its own yet.

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

## What a printout is, and why there is not one

Some apps can insert a document as a **printout**: every page of a PDF or a Word file rendered to
pictures on your page. NoteCraft cannot do that yet, because it needs a renderer for those
formats. The dialog says so rather than pretending. A PDF you attach is kept as a file.

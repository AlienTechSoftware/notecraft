# Insert as Printout

## Behaviour

`Insert as Printout` is a compound attachment operation. It does both of the
following in one action:

1. Copies the original file into the page's `assets` folder and inserts the
   normal attachment card. The card uses the generated shell/file icon and its
   filename link.
2. Renders the source document into one raster image per printed page and
   inserts those images immediately below the attachment card, in page order.

The original attachment remains the source of truth for opening the document.
Double-clicking its icon or filename follows the original file link and shows
the existing open-file warning before launching the associated application.

Each printout page is an ordinary image object. It is selectable, shows the
normal image border and resize handles, and is not editable as document text.
The printed page images do not replace or hide the original attachment.

## Storage contract

The page source names the original attachment and every generated page image.
The page folder therefore remains truthful in source view and file listings.
Generated names use a stable family, for example:

```text
assets/brief.docx
assets/brief.docx.printout-001.png
assets/brief.docx.printout-002.png
```

Regenerating a printout replaces only its generated page images. It never
deletes or changes the original attachment. If rendering fails, the original
attachment is still preserved and the user receives an explicit error; no
partial printout is presented as complete.

## Renderer boundary

The UI owns the command, warning, insertion order and image-object behaviour.
A renderer owns document-to-page conversion and returns ordered raster pages.
The current renderer uses LibreOffice (`soffice` or `libreoffice`) for Office
documents and Poppler's `pdftoppm` for PDF page rasterization. These tools are
optional runtime dependencies: when they are unavailable, the printout action
is disabled for unsupported files or reports a clear error, and it never
silently attaches a file without its pages.

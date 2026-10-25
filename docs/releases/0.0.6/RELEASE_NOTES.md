
# NoteCraft 0.0.6

![NoteCraft 0.0.6 release notes](0.0.6.jfif)

## Highlights

- **Insert files and pictures.** Drop a file onto a page to attach it, or use the ribbon's
  Pictures menu. The file is copied into the page's folder, so it moves with the page.
- **PDF printouts.** Insert a PDF as a printout: the file is attached, and each of its pages is
  drawn below the attachment as a picture. Office documents need LibreOffice installed, because
  they are converted to PDF first. PDFs do not.
- **Local diagnostics you control.** Turn logging on or off, see what has been recorded, and stop,
  clear or export it. NoteCraft does not send it anywhere automatically.
- **Colour icons and a Discord link.** The ribbon uses Fluent UI's colour icons where they exist,
  and the header has a link to the ArtCraft Discord, which opens in your browser.

## Changes

- Attachments open with a double-click, so a single click only selects them.
- The New notebook and Add dialogs use the same theme as the rest of the window.
- The Table menu's GridCraft entry has its icon and menu style.
- The version is 0.0.6 in the README and the Welcome notebook.

## For contributors

- `cargo xtask demo` rebuilds the Welcome notebook from source and replaces the scratch copy.
- `cargo xtask screenshots` regenerates the pictures in `docs/images` headless.
- The pre-push hook runs the whole gate: fmt, clippy, tests, layers, assets, docs, text and the
  demo check.

## Known limitations

- The Windows builds are not code-signed yet. Windows SmartScreen may warn when you run them.
- Office printouts need LibreOffice (`soffice` or `libreoffice`) on the machine.

## Upgrade note

Installing this version does not change your notebook folders. Notebooks are plain folders, so
you can copy one before upgrading if you want a backup.

## Notices

The PDF renderer is the [hayro](https://github.com/LaurenzV/hayro) library by Laurenz Stampfl,
used under the MIT License. Its standard PDF substitute fonts come from PDFium, copyright Foxit
Software Inc. Their licence texts ship in `licenses/PDF-RENDERING-NOTICES.txt` in both the
installer and the portable ZIP. See `ATTRIBUTION.md` for the full list.

## Credits

OneNote parsing is provided by [OfficeIMO](https://github.com/EvotecIT/OfficeIMO), distributed under the [MIT license](https://github.com/EvotecIT/OfficeIMO/blob/master/LICENSE). Many thanks to its authors and contributors.

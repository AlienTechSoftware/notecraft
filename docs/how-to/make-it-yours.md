# Make it yours

## Themes

The sun or moon at the top right opens the theme chooser: **light and dark**, each in seven
accents (default, red, green, yellow, blue, gray, purple), so 14 in all. **View**, then
**Theme** opens the same chooser.

A swatch picks that mode: clicking a dark swatch gives you a dark window even if NoteCraft was
following the system. The switch at the bottom, **Follow the system light or dark setting**, is
the way back.

Every colour in every theme is checked against its background by a test, so no theme can ship
text you cannot read. [docs/themes.md](../themes.md) lists the targets.

## The title bar

The coloured band across the top is the window's title bar, as in OneNote. On it, from the
left: the logo and the Quick Access Toolbar (Back, Forward, Undo, Redo), the page's title in the
middle, then Search, the theme button and the window's own buttons. The ribbon's tab names sit
under them, **File** first; the chosen tab joins the ribbon below it. Drag the band to move the
window, double-click it to maximise, and drag any edge or corner to resize.

The band takes the colour of your theme, darkened until white writing reads on it.

If you would rather have the title bar your system draws, right-click a bare part of the band and
choose **Use the system title bar**. The same menu brings NoteCraft's own back. The choice is kept
in the settings file as `window_frame`, `own` or `system`.

New notebook, Expand all and Collapse all are above the notebook list. The format new pages are
written in is at the right of the status bar.

## Colours for notebooks and sections

Right-click a notebook, section group or section, then **Colour**. Nine colours, shown on the
icon and on the edge of the row. It is saved as `color = "teal"` in the node's `_node.toml`,
so it travels with the folder.

## The page's own view

Beside the page, three buttons:

| Button | What it does |
|---|---|
| **`< >`** | Opens the page's files in the code view, and adds a **Source** tab to the ribbon |
| Magnifier | Puts the caret in the search box |
| Arrows | Full page view: the notebooks step aside until you press it again |

**View** adds rule lines (none, lines or squares behind the page), hiding the page title, and
zoom (smaller, larger, back to 100%). Rule lines and the zoom are remembered.

## Two columns, or one tree

**View**, then **Panes**, then **Separate pages**: the notebooks, section groups and sections
go in one column and the pages of the chosen section in the next, which is how OneNote
arranges them. Without it, everything is one tree, which is how NoteCraft started.

The three-bar button at the top left hides the notebooks column and leaves the pages. Pressing
it from the one-tree layout switches to the two columns with the notebooks hidden, the way that
button behaves in OneNote. The choice is remembered.

## Opening where you left off

NoteCraft reopens the page you closed on, with the sections above it expanded.

## Which version is this

**File**, then **About**: the version, a link to the code
(<https://github.com/AlienTechSoftware/notecraft>), the licence status, and what the window is
built with. Nothing in NoteCraft contacts that address, or any other, by itself.

## Settings on disk

Everything above is a line in a small settings file outside your notebooks:
`%APPDATA%\NoteCraft\settings.toml` on Windows, `~/.config/notecraft/settings.toml` elsewhere.
Set `NOTECRAFT_SETTINGS` to put it somewhere else, which is what the tests do.

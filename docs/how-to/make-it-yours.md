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

## Colours for notebooks and sections

Right-click a notebook, section group or section, then **Colour**. Nine colours, shown on the
icon and on the edge of the row. It is saved as `color = "teal"` in the node's `_node.toml`,
so it travels with the folder.

## The page's own view

Beside the page, three buttons:

| Button | What it does |
|---|---|
| **`< >`** | Shows the page source beside the page, and adds a **Source** tab to the ribbon |
| Magnifier | Puts the caret in the search box |
| Arrows | Full page view: the notebooks step aside until you press it again |

**View** adds rule lines (none, lines or squares behind the page), hiding the page title, and
zoom (smaller, larger, back to 100%). Rule lines and the zoom are remembered.

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

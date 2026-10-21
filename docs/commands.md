# Commands

Generated from the command registry. Do not edit by hand: run
`UPDATE_DOCS=1 cargo test -p notecraft-engine commands_doc` to refresh it.
Every front end (window, `notecraft-cli run`, the control channel) uses these ids.

## `notebook.create`

New notebook (changes the tree).

Parameters:

- name (required): the notebook's title
- format (optional): html or cmgfm; default: the global setting
- empty (optional): true to leave the notebook empty instead of creating the first section and page

## `notebook.welcome`

Add the welcome notebook (changes the tree).

Parameters:

- (none). Writes the Welcome to NoteCraft notebook, a tour of the app, into the notebooks folder. Refused when it is already there

## `node.create`

New section group, section or page (changes the tree).

Parameters:

- parent (required): path of the notebook, group, section or page to add to
- kind (required): section-group, section or page
- title (required for section-group and section; optional for page, which is created untitled if left out)
- format (optional): html or cmgfm. For a page, its format; for a group or section, the default for pages below

## `node.color`

Colour a notebook, section group or section (changes the tree).

Parameters:

- path (required): the notebook, group or section
- color (required): purple, blue, teal, green, yellow, orange, red, magenta, gray, or none for the theme's own colour

## `node.delete`

Move a page, section or group into its notebook's recycle bin (changes the tree).

Parameters:

- path (required): the page, section or group to recycle

## `recycle.list`

List recycled items in a notebook.

Parameters:

- notebook (required): the notebook whose recycle bin to list

## `recycle.restore`

Restore an item from a notebook's recycle bin (changes the tree).

Parameters:

- notebook (required)
- id (required): an id returned by recycle.list

## `recycle.purge`

Permanently delete an item from a notebook's recycle bin (changes the tree).

Parameters:

- notebook (required)
- id (required): an id returned by recycle.list

## `node.title`

Rename the title of a node (changes the tree).

Parameters:

- path (required)
- title (required)

## `page.load`

Open a page.

Parameters:

- path (required): the page

## `page.save`

Save a page.

Parameters:

- path (required): the page
- body (required): the editable part (inside of the HTML body, or the Markdown after the front matter)

## `page.versions`

List saved page versions.

Parameters:

- path (required): the page

## `page.version.restore`

Restore a saved page version.

Parameters:

- path (required): the page
- id (required): the saved version id from page.versions

## `page.direction`

Which way a page reads.

Parameters:

- path (required): the page
- dir (optional): rtl, ltr, or auto to let every line decide; without it the current setting is reported

## `page.containers`

How a page's blocks are arranged in boxes.

Parameters:

- path (required): the page
- boxes (optional): x,y,width,blocks per container, separated by semicolons. The counts must add up to the page's blocks. They are written into the page's text as elements around the blocks; empty puts the page back in one column. Without it, the current arrangement is reported

## `page.paper`

The paper a page is drawn on, and the colour behind it.

Parameters:

- path (required): the page
- paper (optional): none, narrow, college, standard, wide, grid-small, grid-medium, grid-large, grid-huge, craft-lines, craft-grid
- color (optional): purple, blue, teal, green, yellow, orange, red, magenta, gray, or none. Without either, the page's own are reported

## `page.columns`

How wide the columns of a page's tables are.

Parameters:

- path (required): the page
- widths (optional): block:w,w,w per table, separated by semicolons; empty puts every table back to the usual width. Without it, the current widths are reported

## `page.justify`

Which paragraphs of a page are stretched to fill the column.

Parameters:

- path (required): the page
- blocks (optional): the justified paragraphs' positions among the page's blocks, separated by commas, each with an optional strength after a colon (low, medium or high; a number on its own is medium), as in '4, 11:high'; empty draws every paragraph ragged. Without it, the page's own are reported

## `page.attach`

Attach a file to a page.

Parameters:

- path (required): the page
- source (required): a file to copy into the page's assets folder

## `page.detach`

Take a file out of a page's folder.

Parameters:

- path (required): the page
- name (required): the file in the page's assets folder, as page.load reports it. The file is deleted; a line that linked to it is left alone

## `settings.default_format.get`

Show the global default format.

No parameters.

## `settings.default_format.set`

Set the global default format.

Parameters:

- format (required): html or cmgfm

## `search.query`

Search all pages.

Parameters:

- q (required): text to find
- limit (optional): at most this many hits (default 40, at most 500)

## `notebook.open`

Open a notebook or initialize an empty folder (changes the tree).

Parameters:

- path (required): a notebook folder anywhere on this machine, or an empty folder to initialize as a notebook named after the folder. The notebooks already open stay open, and this one is opened again next time

## `notebook.close`

Put away a notebook opened from somewhere else (changes the tree).

Parameters:

- path (required): the notebook's folder. Nothing is deleted

## `notebook.check`

Check a notebook against the folder rules.

Parameters:

- path (required): the notebook

## `tree.read`

List the notebooks, sections and pages.

Parameters:

- path (optional): start at this node instead of listing every notebook

## `tree.refresh`

Reload the tree from disk.

No parameters.


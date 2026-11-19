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

## `node.create`

New section group, section or page (changes the tree).

Parameters:

- parent (required): path of the notebook, group, section or page to add to
- kind (required): section-group, section or page
- title (required for section-group and section; optional for page, which is created untitled if left out)
- format (optional): html or cmgfm. For a page, its format; for a group or section, the default for pages below

## `node.demote`

Make a page a subpage of its preceding sibling (changes the tree).

Parameters:

- path (required): the page to make a subpage

## `node.promote`

Promote a subpage to its parent's level (changes the tree).

Parameters:

- path (required): the subpage to promote

## `node.move`

Move a page, section or section group into another notebook, section group, section or page (changes the tree).

Parameters:

- path (required): the page, section or section group to move
- to (required): where it goes; it is put last there

## `node.place`

Put a page, section, section group or notebook just before another, or last, where a drag drops it (changes the tree).

Parameters:

- path (required): the page, section, section group or notebook to place
- to (required, but not for a notebook): where it goes: a section, section group, notebook or page for a page, a notebook or section group for a section or group
- before (optional): the node in `to` (for a notebook, the open notebook) it goes just before; left out, it goes last

## `node.copy`

Copy a page, section or section group into another notebook, section group, section or page (changes the tree).

Parameters:

- path (required): the page, section or section group to copy
- to (required): where the copy goes; it is put last there, and every node in it gets a new id

## `export.prepare`

Review portable export.

Parameters:

- path (required): page, section, group or notebook
- snapshot_path (optional): open page inside this scope
- body (optional): unsaved body paired with snapshot_path, in its original format
- ink (optional): changed live ink.svg paired with the editor snapshot

## `export.write`

Write reviewed export.

Parameters:

- folder (required): existing destination outside the source notebook; creates a new export folder without overwriting

## `export.cancel`

Cancel export.

No parameters.

## `page.templates`

List the page templates, built in and saved: one line each, with the id, the category and the name, separated by tabs.

No parameters.

## `page.from_template`

A new page started from a template (changes the tree).

Parameters:

- parent (required): the section (or page, for a subpage) the new page goes in, last
- template (required): the template's id, as page.templates lists it (builtin:meeting-notes, saved:lab-report)

## `page.save_template`

Keep a page as a template.

Parameters:

- path (required): the page; its words, boxes, ink, files, paper and colour are kept, not its subpages
- name (required): the template's name

## `page.remove_template`

Take away a page saved as a template.

Parameters:

- template (required): the saved template's id (saved:...). Built-in templates cannot be removed

## `node.move_up`

Move a page up among its siblings (changes the tree).

Parameters:

- path (required): the page to move up

## `node.renumber`

Renumber the pages of a section or page ten apart, keeping their order (changes the tree).

Parameters:

- path (required): the section or page whose child pages are renumbered

## `node.move_down`

Move a page down among its siblings (changes the tree).

Parameters:

- path (required): the page to move down

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

## `window.new`

Open another note window.

Parameters:

- path (optional): the current page; omitted for an empty window

## `window.quick-note`

Create a persistent Quick Note (changes the tree).

No parameters.

## `page.load`

Open a page.

Parameters:

- path (required): the page

## `calendar.events`

The meetings in a calendar file on one day.

Parameters:

- file (required): an iCalendar (.ics) file, which is only read
- day (optional): the day, as 2026-11-23; today without it

## `authors.list`

Who changed the pages of the open notebooks.

No parameters.

## `authors.find`

The pages one person changed.

Parameters:

- author (optional): a name from authors.list; left out, the pages whose author is unknown

## `page.save`

Save a page.

Parameters:

- path (required): the page
- body (required): the editable part (inside of the HTML body, or the Markdown after the front matter)
- expected_body (optional): refuse a save if the editable part changed since loading
- author (optional): the name of who made the change, noted in the page beside the others who changed it

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

The paper a page is drawn on, the colour behind it and the colour of its lines.

Parameters:

- path (required): the page
- paper (optional): none, narrow, college, standard, wide, grid-small, grid-medium, grid-large, grid-huge, craft-lines, craft-grid
- color (optional): purple, blue, teal, green, yellow, orange, red, magenta, gray, or none
- rules (optional): the colour of the rule and grid lines, one of the same colours, or none for the theme's own grey. Without paper or color, the page's own are reported, followed by the lines' colour when it has one

## `page.paper_size`

The sheet a page is written for: its size, which way it is turned and its margins.

Parameters:

- path (required): the page
- size (optional): auto, or a size (letter, legal, tabloid, executive, a3, a4, a5, a6, b4, b5, index3x5, index4x6, index5x8, or custom:WIDTHxHEIGHT in millimetres) followed by portrait or landscape and the margins in millimetres as top,right,bottom,left, as in a4 portrait 20,20,20,20. Without it, the page's own is reported, or auto

## `page.columns`

How wide the columns of a page's tables are, which tables have their borders hidden, how their cells are aligned and shaded, and which are merged.

Parameters:

- path (required): the page
- widths (optional): block:w,w,w per table, separated by semicolons, with a ! after the widths (or on its own, as in 3:!) for a table whose borders are hidden, and block@row.column=a,row.column=a for cells aligned l(eft), c(enter) or r(ight) and shaded with ~ and a colour (purple, blue, teal, green, yellow, orange, red, magenta or gray), as in 2@1.0=c~yellow, and block+row.column=RxC for a merged cell R rows by C columns from that cell, as in 2+1.0=1x2, with the heading as row 0; empty puts every table back to the usual width, borders, alignment, shading and cells. Without it, the current layout is reported

## `page.justify`

Which paragraphs of a page are stretched to fill the column.

Parameters:

- path (required): the page
- blocks (optional): the justified paragraphs' positions among the page's blocks, separated by commas, each with an optional strength after a colon (low, medium or high; a number on its own is medium), as in '4, 11:high'; a paragraph can instead be aligned (2:center) or given extra room in points, sN=before/after/line, as in 's3=6/12/18' (space before, space after and the least height of a line; at most 200 each); empty draws every paragraph ragged and with no extra room. Without it, the page's own are reported

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

## `tags.find`

Find tagged notes.

Parameters:

- scope (optional): a page, section, section group or notebook to look in, relative to the notebooks folder; every open notebook when left out
- page (optional): true to look in the scope page alone, without its subpages
- tag (optional): only this tag, by its id (todo, important, question and the rest)
- unchecked (optional): true for only the To Do items not yet ticked
- limit (optional): at most this many lines (default 500, at most 2000)

## `notebook.open`

Open a notebook or initialize an empty folder (changes the tree).

Parameters:

- path (required): a notebook folder anywhere on this machine, or an empty folder to initialize as a notebook named after the folder. The notebooks already open stay open, and this one is opened again next time

## `open.page`

Find the page to show for a notebook, section group, section or page (changes the tree).

Parameters:

- path (required): relative to the notebooks folder, or a full path. A page is itself; anything else gives its first page. A notebook kept somewhere else is opened first, and one that was put away is opened again

## `notebook.close`

Put away a notebook without deleting it (changes the tree).

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

## `voice.start`

Start local reviewed dictation.

Parameters:

- target (required): opaque insertion ticket

## `voice.stop`

Stop capture and finalize dictation.

Parameters:

- session (required): voice session id

## `voice.cancel`

Discard the voice session.

Parameters:

- session (required): voice session id

## `voice.status`

Read voice lifecycle status without transcript content.

No parameters.

## `voice.draft.set`

Review and edit the local transcript.

Parameters:

- session (required): voice session id
- revision (required): expected transcript revision
- text (required): reviewed transcript, at most 64 KiB

## `voice.retarget`

Choose a new insertion position.

Parameters:

- session (required): voice session id
- target (required): opaque insertion ticket

## `voice.commit`

Insert the reviewed local transcript.

Parameters:

- session (required): voice session id
- revision (required): approved transcript revision


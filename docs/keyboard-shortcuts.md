# Keyboard shortcuts

These shortcuts act on the selected page in the navigation tree. They are also available from
that page's context menu when the action applies. Shortcuts do not run while a text field has
keyboard focus.

| Action | Windows and Linux | macOS |
|---|---|---|
| Add a new page beside the selected page | `Ctrl+N` | `Cmd+N` |
| Make the selected page a subpage of the preceding page | `Ctrl+Alt+]` | `Option+Cmd+]` |
| Promote a subpage to the level of its parent | `Ctrl+Alt+[` | `Option+Cmd+[` |

Making a subpage moves the selected page folder under its immediately preceding page sibling.
The page's content, attachments, and any pages beneath it move together. The action is unavailable
when there is no preceding page sibling.

Promoting a subpage moves it up one level, placing it immediately after its former parent. Its
content, attachments, and child pages move together. The action is available only for a subpage.

Page order is preserved when either action changes the hierarchy.

## Notebooks, sections and pages

These work from the page's words and title, and from the notebooks; another text box with the
keyboard (the search box, a rename) and an open dialog keep the keys. Pages picked together are
moved or copied together by Move or Copy, in their order; a plain click puts them down.

| Action | Windows and Linux | macOS |
|---|---|---|
| Next section, round from the last | `Ctrl+Tab` | `Cmd+Tab` |
| Previous section | `Ctrl+Shift+Tab` | `Cmd+Shift+Tab` |
| First page in the section's list | `Alt+Home` | `Option+Home` |
| Last page in the section's list | `Alt+End` | `Option+End` |
| New section, where the open section is | `Ctrl+T` | `Cmd+T` |
| Switch notebook (a list; Up, Down, Enter; Escape stays) | `Ctrl+G` | `Cmd+G` |
| Keyboard to the open section's row | `Ctrl+Shift+G` | `Cmd+Shift+G` |
| Keyboard to the open page's row | `Ctrl+Alt+G` | `Option+Cmd+G` |
| On a page row: pick it with others, or put it down | `Ctrl+Space` or `Ctrl`+click | `Cmd+Space` or `Cmd`+click |
| On a page row: pick every page from the last picked to it | `Shift+Space` or `Shift`+click | `Shift+Space` or `Shift`+click |
| Keyboard to the page's title (Escape goes back to the words) | `Ctrl+Shift+T` | `Cmd+Shift+T` |

A section opens at the page last open in it, or at its first page; an empty section says so on
the canvas, with a button for its first page. A new section starts with a page, opened.

## Writing

| Action | Windows and Linux | macOS |
|---|---|---|
| Insert the date | `Alt+Shift+D` | `Option+Shift+D` |
| Insert the time | `Alt+Shift+T` | `Option+Shift+T` |
| Insert the date and time | `Alt+Shift+F` | `Option+Shift+F` |
| Heading 1 to Heading 6 | `Ctrl+Alt+1` to `Ctrl+Alt+6` | `Option+Cmd+1` to `Option+Cmd+6` |
| Move the paragraph (or the paragraphs picked) up or down in its box | `Alt+Shift+Up` / `Alt+Shift+Down` | `Option+Shift+Up` / `Option+Shift+Down` |
| Paste the text only, without its formatting | `Ctrl+Shift+V` | `Cmd+Shift+V` |
| Select the paragraph; again, its note container; again, the whole page | `Ctrl+A` | `Cmd+A` |
| The caret to the next or the previous note container | `Alt+Down` / `Alt+Up` | `Option+Down` / `Option+Up` |

Each of these that changes the page is one step for Undo. On a locked page they change nothing and
the status line says the page is locked. Alt+Down and Alt+Up go through the note containers the way
the page reads: from the top down, with containers whose tops are within a line of each other taken
as one row, from the left, or from the right on a right-to-left page. Paste's options on the Home tab also offer **Keep text only**.

## Find on this page

Search's **Find on this page**, with a result on the open page chosen, opens the find bar over
the page (issue #526). Every place the words are is lit, the current one in a stronger colour,
and the bar says which of how many it is.

| Action | Windows and Linux | macOS |
|---|---|---|
| The next place found, round from the last | `F3`, or `Enter` in the bar | `F3`, or `Enter` in the bar |
| The place found before | `Shift+F3`, or `Shift+Enter` in the bar | `Shift+F3`, or `Shift+Enter` in the bar |
| Close the bar, the caret on the current place with its words picked | `Escape` | `Escape` |

With the bar closed, `F3` opens it again with the words last looked for. The words are matched as a
phrase, across marks such as bold and across a line break, letter case aside, in the page's words
and in each cell of its tables. Places in table cells and in a line that mixes the two directions
of writing are found and gone to, and Escape puts the caret there, but they are not lit.

## Page view

These act on the open page from anywhere in the window, except while a dialog is open.

| Action | Windows and Linux | macOS |
|---|---|---|
| Zoom the page in | `Ctrl+Alt+=` | `Option+Cmd+=` |
| Zoom the page out | `Ctrl+Alt+-` | `Option+Cmd+-` |
| Show the page at 100% | `Ctrl+Alt+0` | `Option+Cmd+0` |
| Full Page View on or off | `F11` | `F11` |

`Ctrl` with plus, minus or 0, without `Alt`, still changes the size of the whole window (its
interface scale), not the page's zoom.

## Menus

A menu opens from the keyboard on whatever has the keyboard: a row in the navigator, the words with
the caret (the page's menu, at the caret's line), or a picture or box selected on the page. It
opens with the keyboard on its first row.

| Action | Windows and Linux | macOS |
|---|---|---|
| Open the menu of what has the keyboard | `Shift+F10` or the Menu key | `Shift+F10` |
| Move between rows | `Up` / `Down` | `Up` / `Down` |
| Open a row's further menu, and come back | `Right` / `Left` | `Right` / `Left` |
| Close the menu, giving the keyboard back to where it was | `Escape` | `Escape` |

Pickers that open from the ribbon (Pictures options, Equation, Stickers, the Styles and Tags
galleries, Symbol) work the same way: from the keyboard they open on their first choice (Styles on
the style the caret is in), and `Escape` gives the keyboard back to the button that opened them.

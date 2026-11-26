# Note windows and Quick Notes

View > New window (Ctrl+M) opens the current page in another desktop window.
Navigation, page zoom, selection and the page history belong to each window.
Closing one note window leaves the others open. If the original window closes first,
the application keeps a hidden event-loop window until the final note window closes.

View > New quick note (Ctrl+Shift+M) creates an ordinary untitled page and opens a compact
editor. Notes autosave to the Quick Notes section in the Quick Notes notebook under the
configured notebooks folder. They remain after closing the window. Quick Notes, under the
notebooks in the navigator, opens the section they are kept in with its notes listed; before
the first Quick Note it says there are none yet and offers to make one (issue #566). The
notes can also be reopened or moved from the normal notebook tree. Exit Full Page View to show navigation
in the compact window. The regular formatting, attachments and Always on Top controls
remain available.

The default destination is `<notebooks>/quick-notes/quick-notes`. It never uses the bundled
Welcome notebook. File > Options > Quick Notes section chooses another section from the
sections there are, says where new Quick Notes go, and goes back to the default; notes already
made stay where they were saved (issue #567). The choice is the `quick_notes_section` setting:
a path relative to the notebooks folder or an absolute path in an open notebook.
A missing, invalid or unwritable destination reports an error. It does not silently switch
sections or remove existing notes. The new page follows the destination's format cascade.

## Editing the same page

A clean window refreshes when the page or its ink changes. Its zoom and canvas position
stay local. Text saves compare the loaded body while holding the in-process page write
lock; autosaves use the same guard. Ink saves also compare the loaded file bytes before
replacing or removing the drawing.

If a window has unsaved changes when the page changes elsewhere, it preserves those changes
and asks what to do. **Keep my edits as a new page** writes a sibling page with the retained
text and ink, leaving the newer page alone. **Discard my edits and reload** explicitly drops
local changes and reads the newer page. A failed recovery keeps the input in the window.
The window refuses to close while content remains unsaved. This is conflict recovery,
not automatic merging of two edits or a cross-process filesystem transaction.

## Platform and automation scope

The desktop host uses eframe native viewports. Browser and mobile controls explain that
separate note windows require the desktop app. They do not create temporary Quick Notes.
Native Windows window lifetime is checked locally. Other desktop backends require their
own native checks; shared model tests do not prove operating-system window behavior.

With `--control`, each child window gets its own loopback control port and key file beside
the settings file. The channel remains off by default. Use each window's port for inspection,
navigation, text input, screenshots and closing. The desktop host uses deferred viewports
with editors owned by the native event-loop thread. `ui.inspect.window` reports `secondary`
and `conflict`. Recovery is available through `ui.window.keep-copy` and `ui.window.reload`.
New window and Quick Note requests are also available through the normal engine commands.

The new recovery messages use stable catalog IDs. Locales without a translation use the
English fallback. No page format or demo content changes are required.

Windows UI Automation exposed the native child title bar but not the editor controls during
this verification. Headless semantic labels are tested; full child-window screen-reader
coverage remains a backend integration gap and is not claimed here.

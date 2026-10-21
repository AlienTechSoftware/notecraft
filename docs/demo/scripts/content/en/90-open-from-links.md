---
title: Open a page from a link
---

# Open a page from a link

## Planned: links that open NoteCraft

NoteCraft is planned to support links that open a specific notebook page from a browser, email,
chat, or another application. The operating system will use the `notecraft:` link type to
launch NoteCraft and pass it the page to open. Depending on your operating system and browser,
you may be asked to confirm before the app opens.

This feature is **not available yet**. NoteCraft does not currently register `notecraft:` with
your operating system or handle an external link by navigating to its page. Copying a
NoteCraft page link does not mean it can be opened from a browser today.

## How registration is planned to work

- **Windows installer:** register NoteCraft during installation and remove that registration
  when NoteCraft is uninstalled.
- **Windows portable ZIP:** provide a separate **Register NoteCraft links** action that you
  choose to run, plus a way to undo it. Extracting the ZIP alone will not register the link.
- **macOS:** declare the link type in the NoteCraft app bundle.
- **Linux:** install a desktop application association for the link type.

Registration only tells the operating system which application can receive a link. NoteCraft
must also find the matching notebook and page, handle links when it is already running, and
explain clearly when a notebook or page cannot be found. That handling is part of the planned
feature too.

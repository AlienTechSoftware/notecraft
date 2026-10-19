---
id: b8fa7c16cf8cf2122a14
title: "Sync and backup"
order: 60
created: 2026-10-18T00:00:00Z
updated: 2026-10-18T00:00:00Z
tags: []
---

NoteCraft has no sync of its own and no account. A notebook is a folder of small files, so put
your notebooks folder where your sync tool already looks: OneDrive, Dropbox, Google Drive,
iCloud Drive, Syncthing, a network share or a git repository. None of them has to understand
NoteCraft. They copy files.

## Why it works

- **One file per page.** Editing a page rewrites one small file, so two machines rarely touch
  the same bytes.
- **Atomic writes.** A page is written to a temporary file and renamed over the old one, so a
  sync client never sees a half-written page.
- **Short, safe names.** Folder names are at most 64 characters, paths at most 255, and
  nothing Windows refuses is ever written.
- **Conflict copies are noticed.** When a sync tool leaves a conflicted copy beside a page,
  the checker reports it: **File**, then **Check**, or `notecraft-cli check` from a terminal.

## Backup

A backup is a copy of the folder. There is nothing else to export.

## What is not handled

NoteCraft does not merge two versions of a page; your sync tool makes a conflict copy and you
decide. There is no page history of its own: use a synced folder that keeps versions, or git.

Next: [Make it yours](../make-it-yours/).

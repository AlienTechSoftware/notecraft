# Sync a notebook

NoteCraft has no sync of its own, and does not want any. A notebook is a folder of small files,
so put your notebooks folder where your sync tool already looks:

```sh
notecraft --notebooks "C:\Users\you\OneDrive\Notes"
```

OneDrive, Dropbox, Google Drive, iCloud Drive, Syncthing, a network share or a git repository
all work the same way, because none of them has to understand NoteCraft: they copy files.

## Why the folder format is shaped the way it is

Sync tools are happiest with many small files and no central index, and that is what a notebook
is. The rules that keep them happy, all checked by `notecraft-cli check`:

- **Short names.** A folder name is at most 64 characters and a path at most 255, so a deep
  notebook still fits in the limits a sync client imposes.
- **Safe names.** No reserved names (`CON`, `PRN`, `AUX`, ...), no trailing dots or spaces, no
  characters Windows refuses. A title keeps its own spelling; only the folder name is made safe.
- **One file per page.** Editing a page rewrites one small file, so two machines rarely touch
  the same bytes.
- **Atomic writes.** A page is written to a temporary file and renamed over the old one, so a
  sync client never sees a half-written page.
- **Conflict copies are noticed.** When a sync tool leaves `index (conflicted copy).md` or
  `index-PC-NAME.md` beside your page, the checker reports it:

  ```sh
  notecraft-cli check my-notebook
  ```

## What is not handled

- NoteCraft does not merge two versions of a page. If two machines change the same page, your
  sync tool makes a conflict copy and the checker tells you where it is; you decide.
- There is no page-level history. Use a synced folder that keeps versions, or put the notebooks
  folder in git.
- The one real-world test so far was about 600 folders and 900 files in a OneDrive folder, with
  renames, moves and long paths. It is recorded in the journal of the private repository, not
  re-run for every release.

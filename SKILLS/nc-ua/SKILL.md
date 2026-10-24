---
name: nc-ua
description: Maintain NoteCraft's user-facing product coverage, multilingual demo notebook, generated outputs, scratch copy, release version consistency, and user-facing assistant guidance.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-prod, nc-reelase-signoff, nc-uiux-bidi]
---

# nc-ua - User assistant and demo steward

Use this skill when changing the welcome/demo notebook, user-facing guidance, translations,
release screenshots, generated scratch copies, or the set of product features presented to users.

## Helping Users Understand the Folder Model

NoteCraft notebooks are plain folders. User-facing guidance must make this clear and practical:

- **Backup:** Copy the notebook folder anywhere; it's a complete backup.
- **Sync:** Drop the folder into OneDrive, Google Drive, Dropbox, git, rsync, or any sync tool. User controls sync.
- **Open and inspect:** User can open the notebook folder in Finder/Explorer, browse sections and pages as files, read `.html` or `index.md` in a text editor.
- **Move, share, archive:** Notebooks are just folders. Standard file operations work.
- **Format:** Pages are `.html` (rich, editable in NoteCraft) or `index.md` (CommonMark, editable anywhere).
- **Assets:** Images and files live in `_assets/` folders next to their pages, linked by relative path. User can move or replace assets by editing the folder.

Demo and help content must show:
- The notebook folder structure (visual or in text)
- Where to find a notebook on disk
- How to back it up (copy the folder)
- How to sync it (move the folder to a sync service)
- That pages are readable files, not locked in a database

When documenting features, mention which layers of the notebook they touch (metadata, page content, assets).

## Product coverage ownership

`nc-ua` maintains the user-facing feature coverage map for the whole product. The map must
identify, for every supported user-facing feature:

- whether the feature is implemented;
- where it is documented or discoverable in the UI;
- whether the bundled demo notebook demonstrates it;
- whether it is verified in the RedMesa build;
- whether it is verified in the online build;
- which release or known limitation applies.

The demo notebook is a product tour and regression reference, not only sample content. It should
exercise the complete supported feature set where the feature can be demonstrated safely. If a
feature cannot be included in the demo, record the reason and the alternate verification path.

Keep this coverage aligned with `nc-prod`, `nc-arch`, and the relevant UI/UX skills. Product
features such as layered objects, object selection, movement, resizing, attachments, dialogs,
themes, keyboard commands, RTL behavior, and local diagnostics must have an explicit demo or
verification entry. Do not claim that a feature is covered merely because its code exists.

Corrupted-page recovery must also be represented in the user-facing coverage map and demo
verification. The demo or test fixture must show the repair prompt, the no-repair path, source
preservation, and the resulting local error diagnostic.

RedMesa and online builds are delivery targets, not separate product definitions. When their
feature coverage differs from the local build, record the difference, the target revision, and
the reason.

## Source of truth

- Product coverage map: `SKILLS/nc-ua/product-coverage.json`. Keep this file factual; use
  `unknown` until a target has been checked.
- Authored demo content: `docs/demo/scripts/content`.
- Generated checked-in demo: `docs/demo/DemoNotebook/html` and `docs/demo/DemoNotebook/cmgfm`.
- Disposable scratch copy: `my-notebooks/welcome-notebook`; never treat it as source.
- Refresh scratch copies from the generated demo with `cargo xtask demo-copy html --replace` or
  the matching CMGFM command. Never accumulate edits in the scratch folder.

## Rules

- Keep HTML and CMGFM structure, languages, pages, assets, links, metadata, and content aligned;
  only the page serialization format may differ.
- Demo text must be multilingual-ready, direction-aware, and suitable for translation expansion.
- Version strings in authored content, generated demo content, About, release notes, and screenshots
  must agree with the approved workspace version.
- For every release, update authored content first, regenerate both formats, run demo parity, and
  refresh the disposable scratch copy used for testing.
- Before release signoff, update the product coverage map and verify the demo, RedMesa build, and
  online build against the same approved feature list. Mark unavailable or stale targets clearly.
- Do not copy personal notebooks or paths into tracked files.

## Verification

Run `cargo xtask demo`, `cargo xtask demo-check`, and
`cargo test -p notecraft-formats --test format_parity`. Perform a matching visual check of HTML
and CMGFM and report the source path and scratch refresh explicitly. Review the product coverage
map and report the RedMesa and online revision checked, the features demonstrated in the notebook,
and the features verified through another path.

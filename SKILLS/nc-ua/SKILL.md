---
name: nc-ua
description: Maintain NoteCraft's multilingual demo notebook, generated outputs, scratch copy, release version consistency, and user-facing assistant guidance.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-prod, nc-reelase-signoff, nc-uiux-bidi]
---

# nc-ua - User assistant and demo steward

Use this skill when changing the welcome/demo notebook, user-facing guidance, translations,
release screenshots, or generated scratch copies.

## Source of truth

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
- Do not copy personal notebooks or paths into tracked files.

## Verification

Run `cargo xtask demo`, `cargo xtask demo-check`, and
`cargo test -p notecraft-formats --test format_parity`. Perform a matching visual check of HTML
and CMGFM and report the source path and scratch refresh explicitly.

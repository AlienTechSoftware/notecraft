---
name: nc-uiux-bidi
description: Design, implement, and test NoteCraft bidirectional text, RTL controls, caret placement, selection, and mixed-script UI.
license: MIT
metadata:
  version: "0.0.20261101"
  related_skills: [nc-uiux-controls, nc-themes]
---

# nc-uiux-bidi

Use this skill for Arabic, Hebrew, Persian, Urdu, mixed-direction text, RTL page controls, caret
placement, selection, and mirrored layout.

## Roadmap position

This is the living authority for NoteCraft bidi and RTL work. The v0.0.8 release
notes may describe investigations, but they do not close gaps in this skill.
Bidi UI and ribbon alignment were moved out of v0.0.8 and are v0.0.9 candidate
work. Keep implementation gaps, measurements, test results, and visual findings
here or in the linked references as they change.

## Rules

- Preserve logical text offsets; never use visual x-position as the text index.
- Treat paragraph direction, page direction, and UI direction as separate decisions.
- Test mixed runs containing Arabic/Hebrew plus Latin, numbers, punctuation, links, and emoji.
- Verify caret, selection, backspace, paste, undo/redo, wrapping, and hit-testing in both LTR and
  RTL contexts.
- Do not mirror icons whose meaning is not directional. Mirror directional arrows only when the
  action semantics require it.
- Keep controls usable when labels are longer in translated languages.

## Verification

Read `work/rtl-and-caret-bar.md` before changing this area. Run focused bidi/caret tests and the
cross-platform UI checks; record visual review at matching scale and theme.

See `references/GOTCHAS_AND_GAPS.md` for cross-app RTL lessons and common pitfalls from NoteCraft and Wordcraft.

For durable external research on Arabic/Urdu justification, read
[`references/knowledge-base/kashida-justification.md`](references/knowledge-base/kashida-justification.md).
Add future books and freely licensed references under `references/knowledge-base/`
with source, licence, scope, and a short NoteCraft-specific takeaway.

## Current known gaps

Before planning new work, check the owner bar in `work/rtl-and-caret-bar.md` and
the reference gap notes. Known areas still needing work include UI mirroring,
direction-aware caret indication, word stepping, cross-block selection, Arabic
search normalization, Arabic collation, and export direction. Do not describe
the bidi experience as complete while those gaps remain unmeasured or open.

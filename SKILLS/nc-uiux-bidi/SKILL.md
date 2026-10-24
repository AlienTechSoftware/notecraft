---
name: nc-uiux-bidi
description: Design, implement, and test NoteCraft bidirectional text, RTL controls, caret placement, selection, and mixed-script UI.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-uiux-controls, nc-themes]
---

# nc-uiux-bidi

Use this skill for Arabic, Hebrew, Persian, Urdu, mixed-direction text, RTL page controls, caret
placement, selection, and mirrored layout.

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

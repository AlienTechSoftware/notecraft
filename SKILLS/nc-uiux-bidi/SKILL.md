---
name: nc-uiux-bidi
description: Design, implement, and test NoteCraft bidirectional text, RTL controls, caret placement, selection, and mixed-script UI.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-uiux-controls, nc-themes]
---

# nc-uiux-bidi

## Job to be done

Implement and verify bidirectional text interactions while preserving logical caret, selection, and mixed-script behavior.

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

## Caret model (owner direction, 2026-11-12)

- The caret is a logical position: one block and one character offset. Movement commands take that
  position and return one. They do not read pixels, and they do not depend on the window.
- One owner for the caret. The text field draws it; it does not keep its own copy of the position.
  Where a field keeps its own cursor, the code sets it every time the caret moves, and checks it
  afterwards.
- Layout answers the questions: which line a position is on, where it is on that line, the goal
  column for Up and Down, and whether a line end is right-to-left. Use parley (`crates/text`) for that.
- Horizontal moves step by user-perceived characters (extended grapheme clusters), with the cluster
  rules for Myanmar, Khmer and Tai Tham. A caret never stops inside a stacked consonant.
- Up and Down keep a goal column across repeated presses and clear it on any other move.
- Word and line moves follow paragraph direction: in right-to-left text, the word to the left is the
  next word.
- Edges are reported by layout (line start, line end, last line, right-to-left line) so the command
  moves to the next block in the right direction.
- The rules are tests, copied from WordCraft's `docs/typing-parity.md` where they apply.

Research and options: `references/caret-research.md` and `references/caret-options.md`.

## Verification

Read `work/rtl-and-caret-bar.md` before changing this area. Run focused bidi/caret tests and the
cross-platform UI checks; record visual review at matching scale and theme.

See `references/GOTCHAS_AND_GAPS.md` for cross-app RTL lessons and common pitfalls from NoteCraft and Wordcraft.

## Reference: WordCraft and NoteCraft document models

See `references/wordcraft_x_notecraft_dom.md` for the comparison of the two document models and what it means for caret, runs and inline objects.

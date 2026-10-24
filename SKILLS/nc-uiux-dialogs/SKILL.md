---
name: nc-uiux-dialogs
description: Create and review NoteCraft themed modal dialogs, confirmations, progress dialogs, privacy prompts, and error surfaces.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-themes, nc-uiux-controls]
---

# nc-uiux-dialogs

Use this skill for any modal, confirmation, progress, privacy, warning, or error dialog.

## Rules

- Use the shared modal frame, title treatment, buttons, borders, and theme tokens.
- A modal must block background input, have a clear focus path, and close only through an explicit
  action or documented Escape behavior.
- Use one title only. Do not combine the native egui title bar with a second in-content title.
- Make destructive consequences and affected files clear before the primary action.
- Buttons must be label-proportional, padded, theme-colored, and visibly interactive on hover and
  press.
- Progress dialogs show current operation, progress, cancellation, and failure state without
  becoming transparent or gray.
- Privacy dialogs state what is collected, where it is stored, and whether anything is sent.

## Verification

Test opening, keyboard focus, Accept/Cancel or Deny, backdrop blocking, Escape, theme contrast,
and persistence of the decision where applicable.

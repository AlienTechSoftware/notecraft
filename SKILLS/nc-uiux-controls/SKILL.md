---
name: nc-uiux-controls
description: Build and review NoteCraft buttons, split buttons, dropdowns, menus, hover states, hit targets, and keyboard interaction.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-uiux-icons, nc-themes, nc-uiux-dialogs]
---

# nc-uiux-controls

Use this skill for buttons, ribbon controls, menus, galleries, split controls, toolbars, embedded
object hit testing, selection, dragging, and small interaction regressions.

## Interaction contract

- Main action and dropdown action must have distinct hit regions when a control is a split button.
- Make the dropdown region generous and visually obvious; do not rely on a tiny chevron hitbox.
- Show hover, pressed, focused, and disabled feedback using theme tokens.
- Use readable padding and label-proportional widths. Do not let icons overlap labels.
- Add pointer cursor, tooltip, and accessible widget information where applicable.
- Keep keyboard activation and Escape/menu dismissal working.
- Disabled menu items explain why they are unavailable when the reason is not obvious.
- Embedded objects are hit-tested from highest z-order to lowest z-order. A click must select the
  visible object under the pointer, and an explicit alternate gesture must make lower overlapping
  objects reachable.
- Dragging an embedded object must select it before moving it, preserve its anchor, and report a
  clear selected/moving state.

## Verification

Test the exact pointer locations: center of the main action, center of the arrow region, edge of
the control, and a neighboring control. Add a regression test for every hit-testing bug that has
occurred. Check light/dark themes and narrow windows. For embedded objects, test isolated,
adjacent, and overlapping images, including selecting the back object and dragging it.

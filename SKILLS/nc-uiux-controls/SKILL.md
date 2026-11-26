---
name: nc-uiux-controls
description: Build and review NoteCraft buttons, split buttons, dropdowns, menus, hover states, hit targets, and keyboard interaction.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-uiux-icons, nc-themes, nc-uiux-dialogs]
---

# nc-uiux-controls

## Job to be done

Implement NoteCraft interaction controls with clear hit regions, keyboard behavior, states, and selection semantics.

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

## Reuse first: shared controls over one-off hacks

This is a rule, not a suggestion. When the same interaction is needed in two places, it becomes one shared control, and the places use it.

- **Same look, same behaviour, two places = one control.** Examples: the round opener (the block's emoji "+" and the picture's (>)) and the popup menu opened from it.
- **Do not add a second copy** of a control to fit one screen. Extend the shared control with a parameter instead.
- **The control performs no action.** It reports what the user chose, and the caller decides what that means. The caller sets the handler, the control stays generic.
- **Specialised code is the exception.** Write it only when the behaviour truly differs, and say why in the code.

Shared controls in `crates/ui-egui/src/floating.rs`:
- `round_button`: a true circle with one glyph. The block's emoji "+" and the picture's (>) both use it.
- `button_menu(opener, add)`: a menu opened by a left click on a button.
- `row_menu(response, add)`: a menu opened from a row, by right click or the keyboard.
- `menu_row`, `menu_row_if`, `menu_row_danger`, `menu_separator`: the rows of a menu.
- `point_menu`: a menu at a point on the page.
- `action_menu` (when present): a menu that returns the chosen action.

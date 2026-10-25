---
name: nc-uiux-icons
description: Select, integrate, and review NoteCraft icons for consistent, accessible, licensed desktop UI.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-themes, nc-uiux-controls]
---

# nc-uiux-icons

Use this skill when adding or replacing icons in NoteCraft UI, ribbon controls, menus, dialogs,
or the demo.

## Rules

- Prefer one coherent icon family over hand-drawn one-off symbols.
- Use locally vendored assets only. No runtime downloads, webfonts, or network lookup in the app.
- The current primary family is Fluent UI System Icons: MIT licensed, regular and filled variants,
  and a consistent 24x24 grid. Tabler Icons remains an approved alternative. Confirm the exact
  upstream license before adding assets.
- The developer clone location is read from `.env` at the root of the open repository, using
  `NOTECRAFT_ICON_SOURCE`. Read that file before searching for upstream source assets. The value
  points to a local developer-only clone and must never become a runtime dependency or a network
  download path.
- Add every vendored icon and its license to `ATTRIBUTION.md`.
- Monochrome icons live in `crates/ui-egui/assets/fluent/` and are tinted to the theme colour.
- Colour icons live in `crates/ui-egui/assets/fluent-color/`, with the same file name as their
  monochrome twin. Draw them untinted, because a tint would wash out their colours. Only fade them
  (for example a 90 alpha white tint) when their control is disabled.
- Fluent UI has colour versions of Calendar, Clock, Link, Image, Table, Video, Mic and Document,
  at `assets/<Name>/SVG/ic_fluent_<name>_24_color.svg` in the local clone. It has none of Attach,
  Math formula, Symbols, Ruler or Spacebar, so those stay monochrome until a colour source is
  chosen.
- Preserve semantic color tokens from `crates/ui-egui/src/theme.rs`. Monochrome icon color must
  communicate state without being the only state signal. Colour icons keep their own colours, so
  state comes from the control's hover, pressed and disabled appearance.
- Use accessible labels and tooltips. An icon must not replace a meaningful action name where the
  control is not self-evident.
- Test normal, hover, pressed, disabled, dark, light, and high-contrast theme states.

## Workflow

1. Search the chosen library for the semantic action, not a visual approximation.
2. Read `NOTECRAFT_ICON_SOURCE` from the open repository's `.env` and verify that the directory
   exists before selecting assets.
3. Select one stroke/fill style and optical size for the control group. Choose the colour
   version when the group is coloured, and say so if a group mixes colour and monochrome icons.
4. Vendor only the selected assets and record attribution.
5. Integrate through the shared icon mapping, not scattered raw paths.
6. Check hit targets, labels, hover states, and screenshots at the target scale.
7. Run `cargo fmt --all`, focused UI tests, and `cargo xtask ci` when the change is release-bound.

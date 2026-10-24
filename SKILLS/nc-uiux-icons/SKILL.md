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
- The developer clone location is read from `C:\dev\p-NoteCraft\.repos\o-NoteCraft\.env`, using
  `NOTECRAFT_ICON_SOURCE`. Read that file before searching for upstream source assets. The value
  points to a local developer-only clone and must never become a runtime dependency or a network
  download path.
- Add every vendored icon and its license to `ATTRIBUTION.md`.
- Preserve semantic color tokens from `crates/ui-egui/src/theme.rs`; icon color must communicate
  state without being the only state signal.
- Use accessible labels and tooltips. An icon must not replace a meaningful action name where the
  control is not self-evident.
- Test normal, hover, pressed, disabled, dark, light, and high-contrast theme states.

## Workflow

1. Search the chosen library for the semantic action, not a visual approximation.
2. Read `NOTECRAFT_ICON_SOURCE` from the open repository's `.env` and verify that the directory
   exists before selecting assets.
3. Select one stroke/fill style and optical size for the control group.
4. Vendor only the selected assets and record attribution.
5. Integrate through the shared icon mapping, not scattered raw paths.
6. Check hit targets, labels, hover states, and screenshots at the target scale.
7. Run `cargo fmt --all`, focused UI tests, and `cargo xtask ci` when the change is release-bound.

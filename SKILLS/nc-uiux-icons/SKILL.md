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
- The current candidate family is Tabler Icons: MIT licensed, outline and filled variants, and a
  consistent 24x24 grid. Confirm the exact upstream license before adding assets.
- Add every vendored icon and its license to `ATTRIBUTION.md`.
- Preserve semantic color tokens from `crates/ui-egui/src/theme.rs`; icon color must communicate
  state without being the only state signal.
- Use accessible labels and tooltips. An icon must not replace a meaningful action name where the
  control is not self-evident.
- Test normal, hover, pressed, disabled, dark, light, and high-contrast theme states.

## Workflow

1. Search the chosen library for the semantic action, not a visual approximation.
2. Select one stroke/fill style and optical size for the control group.
3. Vendor only the selected assets and record attribution.
4. Integrate through the shared icon mapping, not scattered raw paths.
5. Check hit targets, labels, hover states, and screenshots at the target scale.
6. Run `cargo fmt --all`, focused UI tests, and `cargo xtask ci` when the change is release-bound.

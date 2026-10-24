---
name: nc-themes
description: Design and implement NoteCraft theme tokens, contrast, state colors, and modal surfaces across all supported themes.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-uiux-icons, nc-uiux-controls, nc-uiux-dialogs]
---

# nc-themes

Use this skill for colors, surfaces, typography contrast, borders, hover states, dialogs, and
visual regressions.

## Rules

- Use theme tokens from `crates/ui-egui/src/theme.rs`; do not hard-code UI colors.
- Every interactive control needs normal, hover, pressed, focused, disabled, and error states.
- Borders must remain visible in the active theme, including light themes where pale strokes can
  disappear.
- Modal title bands, body surfaces, buttons, progress indicators, and backdrops must share the
  same token system.
- Do not use color as the sole indicator of state; pair it with text, shape, icon, or focus.
- Check all fourteen themes and both light/dark system modes where the test harness supports them.

## Verification

Use the theme contrast tests and focused UI snapshots. Record any visual-only check in the task
handoff. Do not commit golden screenshots unless repository policy explicitly requires them.

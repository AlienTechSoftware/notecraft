# NoteCraft visual regression

Visual tests answer whether NoteCraft is laid out and painted as intended.
Semantic UI tests answer whether controls and state transitions behave as
intended. Accessibility tests answer whether assistive technology can discover
and operate the interface. Keep these assertions separate.

The current UI harness uses `egui_kittest` and the wgpu renderer. The repository
does not generally use portable committed golden screenshots for UI tests;
same-renderer comparisons and generated diagnostics are preferred because GPU,
font, scale, and platform differences can make golden files misleading.

Use visual output for:

- layout, spacing, theme, icon, and painting regressions;
- bidi and mixed-script rendering checks;
- before/after evidence for a focused UI change.

Do not use a screenshot alone to establish keyboard behavior, caret semantics,
accessibility, persistence, or interaction correctness. Pair the image with a
semantic or state-based assertion.

When a visual test fails, preserve the new and diff images long enough to inspect
them, identify the environment, and decide whether the change is intentional.
Do not accept or commit a new image merely to silence a failure.

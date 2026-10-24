---
name: nc-uiux-features
description: Add complete NoteCraft UI features with coherent user flows, persistence, accessibility, and regression coverage.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-hl-design, nc-uiux-controls, nc-uiux-dialogs, nc-themes]
---

# nc-uiux-features

Use this skill when a feature crosses ribbon, page content, settings, storage, or dialogs.

## Required shape

- Define the user action, visible state, persistence rule, cancel path, and failure message before
  coding.
- Reuse existing `Act`, command, settings, dialog, and theme abstractions instead of adding a
  parallel path.
- Keep page storage truthful: embedded content belongs in the page folder and must round-trip in
  HTML and CMGFM where supported.
- Make destructive actions explicit and recoverable where possible.
- Add focused unit/UI tests for the happy path, cancellation, invalid input, reload persistence,
  and the regression that motivated the feature.
- Verify the feature in the built app, not only in a headless test.

## Completion gate

Report files changed, commands run, visual checks, known gaps, and whether the demo notebook was
updated or intentionally left unchanged.

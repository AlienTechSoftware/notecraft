---
name: nc-uiux-testing
description: Test and automate NoteCraft's desktop UI, including semantic interaction, screenshot capture, visual verification, accessibility integration, and platform-specific automation.
---

# nc-uiux-testing

This is the canonical skill for NoteCraft UI automation and visual test evidence.
Use it when a task drives the application, checks UI behavior or pixels, captures
screenshots, or evaluates accessibility and platform automation boundaries.

## Ownership

This skill owns the mechanics and evidence of UI testing:

- semantic interaction through the NoteCraft control protocol;
- headless and real-window screenshot capture;
- visual assertions and test-generated image artifacts;
- accessibility-test integration and platform automation limits;
- deciding which test layer is appropriate for a UI claim.

User Assistance (`nc-ua`) owns public documentation screenshots, demo-notebook
content, and the screenshot inventory. Release signoff owns release acceptance
evidence. Do not duplicate those policies here.

## Choose the smallest reliable test layer

1. Prefer in-process semantic UI tests for deterministic state and interaction.
2. Use headless `ui_shot` for reproducible screenshots and scripted visual states.
3. Use a real window when display scaling, focus, native input, pointer capture,
   compositor behavior, or platform accessibility must be verified.
4. Use external Windows UI Automation or Linux accessibility automation only for
   behavior that the application-level harness cannot observe.

Read the relevant reference before changing the workflow:

- [Screenshot capture](references/screenshot-capture.md)
- [Visual regression](references/visual-regression.md)
- [Platform automation](references/platform-automation.md)
- [Research and design rationale](references/research.md)

## Evidence rules

- Separate semantic correctness, visual correctness, and accessibility; one does
  not prove the others.
- Inspect changed PNGs when a task claims visual correctness.
- Keep generated screenshots out of commits unless they are intentional product
  or documentation assets.
- Do not use a screenshot to conceal a failed semantic or accessibility check.
- Record the exact command, platform, display conditions, and artifact location
  in the task handoff when the result depends on them.

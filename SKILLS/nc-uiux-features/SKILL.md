---
name: nc-uiux-features
description: Add complete NoteCraft UI features with coherent user flows, persistence, accessibility, and regression coverage.
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [nc-arch, nc-hl-design, nc-uiux-controls, nc-uiux-dialogs, nc-themes]
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

## Anchored images and layered objects

Use `work/inline-images-and-anchors.md` as the behavior reference for images and other embedded
objects. An inline image is an atomic object in the text flow, not an unrelated canvas overlay.
It has a persistent anchor in its owning text block. A floating image keeps that anchor and
additionally has page position, size, and z-order.

Selection and dragging must resolve the object under the pointer using the stored stacking order.
Moving an inline object must preserve its text anchor; moving a floating object must update its
position without losing its anchor. Both formats must round-trip the same object semantics.

## Corrupted-page recovery

When opening a page produces a repair candidate, show a themed modal before any write. Explain
which file is affected, what was recovered, what will change, and where the original backup or
version will be kept. Offer `Repair and open`, `Open without repairing`, and `Cancel` as distinct
actions. Only the first action may write. A denial must leave the source unchanged, use a safe
read-only/recovered view when possible, and emit an error-level local diagnostic.

## Completion gate

Report files changed, commands run, visual checks, known gaps, and whether the demo notebook was
updated or intentionally left unchanged.

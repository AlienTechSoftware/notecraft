# Workspace 2: Infinite canvas

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_2`
**Updated:** 2026-11-04
**Status:** Assigned; scope and implementation not yet recorded.

## Goal

Deliver the v0.0.9 infinite-canvas work. The release README describes the target as reliable drawing outside text blocks and across empty page space.

## Scope to verify before implementation

The private backlog also records requests for continuous scrolling backgrounds, pen/touch/keyboard input switching, drawing performance, and related settings. It notes that NoteCraft already scrolls vertically, grows pages with content, and creates a box when clicking open paper. Verify current behavior and reconcile these details with the intended canvas behavior before setting acceptance criteria.

The detailed private backlog is `work/notecraft-improvements.md`, especially its owner-priority and open-work sections. This summary is included here so the checkout can coordinate from Git without depending on the private host.

## Acceptance criteria

Record observable criteria after checking current page, ink, input, and background behavior. Include supported input modes and platform coverage. Add regression tests before implementing user-facing behavior, as required by the repository contract.

## Coordination

- At assignment, `_2` was clean on `main`. Inspect local `next` and remote state before creating a dedicated v0.0.9 feature branch.
- Keep canvas implementation in `_2`; do not force-switch branches or mix unrelated changes.
- Coordinate shared UI automation requirements with `_3` and check for regressions against `_1` localization.
- Do not merge or push without explicit authorization.

## Progress

- 2026-11-04: Workstream assigned. No files or code were changed in `_2` by the coordination setup.

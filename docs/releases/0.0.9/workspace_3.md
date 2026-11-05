# Workspace 3: Performance markers and voted issues

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_3`
**Updated:** 2026-11-04
**Status:** Assigned; issue selection and implementation not yet recorded.

## Goal

Work on performance markers and a limited set of highly voted NoteCraft GitHub issues.

## Performance markers

The v0.0.9 release plan calls for low-overhead, repeatable measurements covering startup, page loading, editing, saving, search, and rendering. Review the existing instrumentation and the private host's `work/PERF_INSTRUMENTATION.md` and `work/releases/` notes before selecting markers, storage, opt-in behavior, and acceptance criteria. This summary keeps the work visible to all checkouts through Git.

## GitHub issue selection

Use a read-only review of the current NoteCraft issue list. Record each candidate's issue number, title, vote/reaction count, date checked, and fit for v0.0.9. Select a bounded set alongside performance work, then record issue numbers and acceptance criteria in this file and the v0.0.9 release README. Do not post comments, reactions, or status updates to GitHub.

## Coordination

- At assignment, `_3` was clean on `fix/macos-ui-table-test`. Inspect its history and purpose before switching branches; preserve that branch work.
- Verify local `next` and remote state before creating a v0.0.9 feature branch. Do not force-switch branches.
- Keep performance and issue fixes in `_3`. Coordinate UI automation work with `_1` and `_2`.
- Do not merge or push without explicit authorization.

## Progress

- 2026-11-04: Workstream assigned. No issue vote audit or code changes have been made in `_3` by the coordination setup.

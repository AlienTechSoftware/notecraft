# Workspace 3: Performance markers and voted issues

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_3`
**Updated:** 2026-11-05
**Status:** Assigned; two issue candidates recorded; acceptance criteria and implementation not yet recorded.

## Goal

Work on performance markers and a limited set of highly voted NoteCraft GitHub issues.

## Performance markers

The v0.0.9 release plan calls for low-overhead, repeatable measurements covering startup, page loading, editing, saving, search, and rendering. Review the existing instrumentation and the private host's `work/PERF_INSTRUMENTATION.md` and `work/releases/` notes before selecting markers, storage, opt-in behavior, and acceptance criteria. This summary keeps the work visible to all checkouts through Git.

## GitHub issue selection

Use a read-only review of the current NoteCraft issue list. Record each candidate's issue number, title, vote/reaction count, date checked, and fit for v0.0.9. Select a bounded set alongside performance work, then record issue numbers and acceptance criteria in this file and the v0.0.9 release README. Do not post comments, reactions, or status updates to GitHub.

### Selected candidates

Checked 2026-11-05. Both currently have zero reactions.

- [#19: errors with import of existing OneNote notebooks](https://github.com/AlienTechSoftware/notecraft/issues/19) - Strong fit: import reliability is important to users bringing existing notes into NoteCraft. Scope and acceptance criteria need investigation.
- [#35: Keep the notebook color when importing from OneNote](https://github.com/AlienTechSoftware/notecraft/issues/35) - Strong, bounded fit if the import source contains the color metadata. Confirm available source data and define acceptance criteria before implementation.

### Additional tracked issue

- [#31: Translatable UI](https://github.com/AlienTechSoftware/notecraft/issues/31) - Track the remaining work to extract and translate user-visible interface strings across the 13 demo locales. `_1` owns the i18n implementation; coordinate scope and avoid duplicating that work here.

## Coordination

- At assignment, `_3` was clean on `fix/macos-ui-table-test`. Inspect its history and purpose before switching branches; preserve that branch work.
- Verify local `next` and remote state before creating a v0.0.9 feature branch. Do not force-switch branches.
- Keep performance and issue fixes in `_3`. Coordinate UI automation work with `_1` and `_2`.
- Do not merge or push without explicit authorization.

## Progress

- 2026-11-04: Workstream assigned. No issue vote audit or code changes have been made in `_3` by the coordination setup.
- 2026-11-05: Added PII-safe synthetic importer regression coverage: embedded PNG/SVG and generic file bytes survive HTML and CMGFM imports; missing image data is reported without losing page text; a damaged neighboring section does not block the valid section. Native OneNote ink is not covered because the current parser has no ink content type. This does not yet reproduce the reporter's specific #19 notebook failure.

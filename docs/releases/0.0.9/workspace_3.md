# Workspace 3: Performance markers and voted issues

**Release:** v0.0.9
**Checkout:** `o-NoteCraft_3`
**Updated:** 2026-11-06
**Status:** Issue #35 color-import fix is ready to test on `feat/import-section-color`; performance markers remain pending.

## Goal

Work on performance markers and a limited set of highly voted NoteCraft GitHub issues.

## Performance markers

The v0.0.9 release plan calls for low-overhead, repeatable measurements covering startup, page loading, editing, saving, search, and rendering. Review the existing instrumentation and the private host's `work/PERF_INSTRUMENTATION.md` and `work/releases/` notes before selecting markers, storage, opt-in behavior, and acceptance criteria. This summary keeps the work visible to all checkouts through Git.

## GitHub issue selection

Use a read-only review of the current NoteCraft issue list. Record each candidate's issue number, title, vote/reaction count, date checked, and fit for v0.0.9. Select a bounded set alongside performance work, then record issue numbers and acceptance criteria in this file and the v0.0.9 release README. Do not post comments, reactions, or status updates to GitHub.

### Selected candidates

Checked 2026-11-05. Both currently have zero reactions.

- [#19: errors with import of existing OneNote notebooks](https://github.com/AlienTechSoftware/notecraft/issues/19) - Strong fit: import reliability is important to users bringing existing notes into NoteCraft. Scope and acceptance criteria need investigation.
- [#35: Keep the notebook color when importing from OneNote](https://github.com/AlienTechSoftware/notecraft/issues/35) - Bounded fit. Preserve notebook colors from `.onetoc2`, section colors from TOC entries or `.one` metadata, and keep malformed optional TOCs from blocking section imports. Imported colors map lossily to NoteCraft's named palette.

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
- 2026-11-06: Rebased `feat/import-section-color` onto `origin/main` at `1623775`. The owner approved using the public onenote.rs and OfficeIMO source as corroboration for the otherwise undocumented `NotebookColor` property ID. Scope stays import-only; importer selection remains undecided. Added PII-safe synthetic `.one` and `.onetoc2` fixtures and tests for TOC precedence, section metadata fallback, undefined colors, identity matching, malformed TOCs, and both page formats. Verification passed: `cargo fmt --all -- --check`, focused clippy for `notecraft-onenote` and `notecraft-import`, `cargo test -p notecraft-onenote`, `cargo test -p notecraft-import`, `git diff --check`, and full `cargo xtask ci`. The importer corpus suite reports 2 documented large Git LFS cases ignored; all remaining tests pass. No commit or push was made.

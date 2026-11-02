# NoteCraft v0.0.8

**Status:** Housekeeping review
**Last reviewed:** 2026-11-01
**Target Issues:** #18 (image resize remains a major blocker), performance instrumentation

This folder contains the original v0.0.8 investigation notes. The table below is
the current disposition of those notes. A report marked fixed is historical
evidence, not an open release blocker.

| Note | Current disposition | Action |
|---|---|---|
| `issue_first_launch_dialog.md` | Fixed in the startup path | Keep as history; update acceptance notes |
| `issue_missing_privacy_dialog.md` | Fixed and covered by tests | Close as fixed |
| `issue_duplicate_notebook.md` | Original cause removed with startup deployment | Close as obsolete; monitor manual welcome installation |
| `issue_user_facing_errors.md` | Original welcome-deployment error removed | Keep a smaller generic error-message audit for later |
| `issue_img_resize.md` | Major issue remains open; implementation is not release-ready | Keep as active blocker |
| `issue_block_hit_test.md` | Core click behavior exists; sizing needs a later review | Track as follow-up, not a release blocker |
| `issue_infinte_canvas.md` | Not implemented | Future version |
| `enhancement_bidi_uiux_alignment.md` | Moved out of 0.0.8 scope | 0.0.9 candidate |
| `rc_perf.md` | Full profiling/dashboard moved out of 0.0.8 scope | 0.0.9 candidate |

## Scope

Version 0.0.8 focuses on fixing critical image resize functionality and adding performance instrumentation for profiling and optimization.

### Primary Issues
- **#18:** Image resize - double border on canvas pages, non-functional drag handles
- **PERF:** Performance instrumentation marks for profiling

## Key Changes
- ✅ Image resize handles now visible and functional
- ✅ Remove double border regression 
- ✅ Eliminate faint border for image objects
- ✅ Fix drag sense (click_and_drag) for resize
- 📝 Add comprehensive logging for double-border investigation
- 📝 Performance instrumentation framework

## Timeline
- **Development:** Startup fixes are complete; image resize remains under active investigation
- **Testing:** Covered by the repository quality gate and focused UI tests
- **Release:** Not declared here; release signoff remains a separate decision

## Architecture
### Image Resize Bug Investigation
- **Disposition:** Not fixed to release quality. The current implementation has resize code and tests,
  but the user-facing behavior remains unreliable and requires further work.
- **Root Cause:** Orphaned empty container i=7 overlapped with image i=1.
- **Files:** `crates/ui-egui/src/containers.rs`
- **Verification:** Existing UI tests cover parts of the behavior but are not sufficient to close issue #18.
- **Documentation:** `docs/releases/0.0.8/issue_img_resize.md`

### Performance Instrumentation
- **Disposition:** Deferred. The existing logging and CI resource diagnostics are useful,
  but a complete profiling workflow and dashboard are not part of this release note set.
- Measure rendering time per container
- Track layout calculation costs
- Profile text measurement overhead
- Identify optimization targets for v0.0.9+

## Related Documents
- `issue_img_resize.md` - Complete double-border investigation
- `perf_instrumentation.md` - Performance marks design

## Links
- **GitHub Issue #18:** https://github.com/AlienTechSoftware/notecraft/issues/18
- **Branch:** `feat/v0.0.8`
- **Related:** v0.0.9, v0.0.10

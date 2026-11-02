# NoteCraft v0.0.8

**Status:** In development / partially complete
**Base Branch:** `main`
**Target Issues:** #18 (image resize, completed), performance instrumentation (deferred)

## Scope

Version 0.0.8 focused on fixing critical image resize functionality and recording
the release-confidence work that remains for later versions.

### Primary Issues
- **#18:** Image resize - implemented and verified
- **Startup/privacy:** first-use flow and local diagnostic disclosure fixed
- **PERF:** performance instrumentation deferred to v0.0.9+

## Key Changes
- ✅ Image resize handles visible and functional
- ✅ Double-border and faint-border regressions removed
- ✅ Dragging resizes the image and persists dimensions
- ✅ First-use/privacy flow covered by tests
- 📝 Performance instrumentation remains deferred

## Timeline
- **Development:** Core fixes complete; follow-up work remains
- **Testing:** Current repository gate passes
- **Release:** TBD

## Architecture
### Image Resize Bug Investigation
- **Root Cause:** Orphaned empty container i=7 overlaps with image i=1
- **Files:** `crates/ui-egui/src/containers.rs`
- **Logging:** CREATE_BOX, REMOVE_CONTAINER, CONTAINER debug logs
- **Documentation:** `docs/releases/0.0.8/issue_img_resize.md`

### Performance Instrumentation
Full profiling/dashboard work is deferred to v0.0.9+.

## Related Documents
- `done/issue_img_resize.md` - Complete resize investigation and verification
- `done/issue_first_launch_dialog.md` - Startup flow implementation record
- `done/issue_missing_privacy_dialog.md` - Privacy dialog fix record
- `implemented/issue_duplicate_notebook.md` - Local-setup investigation
- `rc_perf.md` - Performance follow-up plan

## Links
- **GitHub Issue #18:** https://github.com/AlienTechSoftware/notecraft/issues/18
- **Branch:** `main`
- **Related:** v0.0.9, v0.0.10

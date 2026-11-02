# NoteCraft v0.0.8

**Status:** Release candidate / packaging verification
**Base Branch:** `rc_0.0.8`
**Release notes:** [RELEASE_NOTES.md](RELEASE_NOTES.md)

## Scope

Version 0.0.8 focuses on reliable image editing and navigation, right-to-left
writing, attachment behavior, and repeatable Windows/Linux release packaging.

### Primary Issues
- **#18:** Image resize - implemented and verified
- **Startup/privacy:** first-use flow and local diagnostic disclosure fixed
- **PERF:** performance instrumentation deferred to v0.0.9+

## Key Changes
- ✅ Image resize handles visible and functional
- ✅ Double-border and faint-border regressions removed
- ✅ Dragging resizes the image and persists dimensions
- ✅ First-use/privacy flow covered by tests
- ✅ Inline-image caret and keyboard navigation covered by UI tests
- ✅ `Ctrl+V` image paste covered by UI tests
- ✅ Windows MSI and portable ZIP include the demo notebook
- ✅ Linux tar, DEB, and AppImage paths include the demo notebook
- 📝 Performance instrumentation remains deferred

## Timeline
- **Development:** Core 0.0.8 scope complete; lower-priority work remains documented
- **Testing:** Full repository gate and packaging smoke tests pass on the RC
- **Release:** Owner-controlled final release decision

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

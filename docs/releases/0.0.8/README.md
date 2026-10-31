# NoteCraft v0.0.8

**Status:** In Development  
**Base Branch:** `feat/v0.0.8` → `next`  
**Target Issues:** #18 (image resize), performance instrumentation

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
- **Development:** In progress
- **Testing:** Pending
- **Release:** TBD

## Architecture
### Image Resize Bug Investigation
- **Root Cause:** Orphaned empty container i=7 overlaps with image i=1
- **Files:** `crates/ui-egui/src/containers.rs`
- **Logging:** CREATE_BOX, REMOVE_CONTAINER, CONTAINER debug logs
- **Documentation:** `docs/releases/0.0.8/issue_img_resize.md`

### Performance Instrumentation
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

# v0.0.10 Android: Known Issues & Constraints

**Date:** 2026-10-31  
**Status:** Analysis phase  
**Platform:** Android

---

## Critical Constraints

### 1. OfficeIMO Bridge Incompatible with Android

**Current State:**
- OfficeIMO importer only supports: Windows x64, Linux x64, macOS x64/ARM64
- Designed as external process bridge (separate executable)
- Platform-specific builds bundled with desktop releases
- No Android binary available

**File:** `crates/import/src/officeimo.rs:378-389`
```rust
fn target_name() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) { "win-x64" }
    else if cfg!(all(target_os = "linux", target_arch = "x86_64")) { "linux-x64" }
    else if cfg!(all(target_os = "macos", target_arch = "x86_64")) { "osx-x64" }
    else if cfg!(all(target_os = "macos", target_arch = "aarch64")) { "osx-arm64" }
    else { "unsupported" }  // Android falls here
}
```

**Impact on Android:**
- Cannot invoke external bridge process (no file system access to importers/)
- OfficeIMO executable not available for Android architecture
- Even if bundled, Android app sandboxing prevents running arbitrary executables

---

### 2. Solution: Embedded OneNote Parser

**Good News:** NoteCraft already has a pure Rust, cross-platform OneNote parser

**File:** `crates/onenote/src/lib.rs`
- **Type:** Embedded crate (not external process)
- **Scope:** Read-only `.one` file parser
- **Implementation:** Clean-room from MS-ONESTORE and MS-ONE specifications
- **Platform:** Pure Rust, no platform-specific dependencies
- **Status:** Already implemented and working on desktop

**Capabilities:**
- Parse .one section files
- Extract pages, text, formatting (bold, italic, underline, strike)
- Extract images and embedded files
- Extract tables
- Support lists and outline structure
- Map OneNote sub-pages (level 1-3) to NoteCraft nested folders

**Architecture:**
- Layer L1: `crates/onenote/src/store.rs` - reads binary file structure
- Layer L2: `crates/onenote/src/one.rs` - parses objects into `Section { pages: Vec<Page> }`
- Layer L3: `crates/import/src/convert.rs` - converts to NoteCraft blocks
- Layer L4: `crates/store/src/ops.rs` - writes to folder tree

---

## Android-Specific Implementation Plan

### Phase 1: Verify Embedded Parser Compiles on Android
- [ ] Test `crates/onenote` builds for Android target (`aarch64-linux-android`)
- [ ] Verify no platform-specific code blocks this
- [ ] Check dependencies compile (likely pure std Rust)

### Phase 2: Adapt Import Interface
- [ ] Make `import::run()` accept data stream (for Android ContentProvider)
- [ ] Remove external bridge process dependency
- [ ] Use embedded `crates::onenote::read()` directly
- [ ] Handle Android file URI scheme (content://)

### Phase 3: UI Integration for Android
- [ ] File picker for .one files (Android DocumentsUI)
- [ ] Permission request (READ_EXTERNAL_STORAGE or newer Storage Access Framework)
- [ ] Progress UI (import can take minutes for large notebooks)
- [ ] Error handling (encrypted sections, damaged files)

### Phase 4: Testing
- [ ] Small .one test files from owner corpus
- [ ] Verify HTML format output
- [ ] Verify Markdown (CMGFM) format output
- [ ] Test on real Android device/emulator

---

## Specific Issues to Resolve

### Issue A: File Access on Android

**Problem:** Android sandboxing and dynamic permissions

**Current Desktop Flow:**
```
importer CLI
  -> locate bridge binary (file system)
  -> read .one file (file system)
  -> write output (file system)
```

**Android Flow Needed:**
```
UI FilePickerActivity
  -> Android returns ContentUri (content://...)
  -> App has READ permission (via SAF)
  -> crates/onenote reads ContentProvider stream
  -> crates/store writes to app-specific directory
```

**Implementation Path:**
- Add `read_from_uri()` function to support Android ContentUri
- Keep `read_from_path()` for desktop
- Both converge to same `crates/onenote::read(bytes)` parser

---

### Issue B: Memory for Large Notebooks

**Problem:** OneNote backup files can be 50-160 MB; Android RAM constraints

**Current State:**
- Desktop: loads entire file into memory
- Android: devices may have only 2-4 GB total RAM

**Solutions:**
1. **Streaming parser** (preferred, better UX)
   - Parse revision store incrementally
   - Stream pages to disk as they're extracted
   - Lower peak memory usage

2. **Lazy loading** (simpler, acceptable)
   - Load file into RAM once (user waits)
   - Extract pages normally
   - Clear memory when done
   - Only viable for <100 MB files

3. **Pre-export requirement** (workaround)
   - User exports OneNote on desktop
   - Syncs to cloud
   - Android imports from synced folder (already works)

---

### Issue C: UI/UX for Mobile Import

**Problems:**
- No keyboard/mouse for interaction
- Touch screen navigation of wizard
- Small screen real estate
- Slow network (cellular)

**Solutions:**
- Single-step import (pick file, import, done)
- Detect file size and warn user
- Show progress per section
- Allow cancellation
- Save partial state on interruption

---

### Issue D: Format Parity on Android

**Requirement (from AGENTS.md):** HTML and Markdown must render identically

**Desktop:** Both formats working  
**Android:** Both formats must also work

**Verification Needed:**
- [ ] HTML and Markdown output from embedded parser are identical
- [ ] egui rendering of both formats is identical
- [ ] Images embedded in both formats work
- [ ] Tables render the same in both formats

---

## Related Files & Code

| File | Purpose |
|------|---------|
| `crates/onenote/src/lib.rs` | Clean-room OneNote parser (L1) |
| `crates/onenote/src/one.rs` | Page/content model |
| `crates/onenote/src/store.rs` | Binary file structure reader |
| `crates/import/src/officeimo.rs` | Desktop bridge (Android alternative: embedded) |
| `crates/import/src/convert.rs` | OneNote to NoteCraft blocks |
| `crates/import/src/lib.rs` | Import coordination |
| `crates/store/src/ops.rs` | Folder tree writer |
| `docs/research/onenote-file-format/` | Format research |
| `work/onenote-importer-plan.md` | Desktop importer design |

---

## Decision Points

### Q1: What if embedded parser can't compile on Android?

**Current Risk:** Low (pure Rust, no platform-specific code observed)

**Mitigation:**
- Identify dependency causing failure
- Rewrite or replace that dependency
- Last resort: feature-gate parser for non-Android builds (not ideal)

---

### Q2: Should Android support all OneNote import features?

**Desktop v0.0.8:** Supports .one backup files, single files, folder imports  
**Android v0.0.10:** Recommendation = .one single files only (start simple)

**Reasons:**
- Smaller scope for first release
- Easier UI (pick one file, import)
- Reduces memory pressure
- Can add folder import in v0.0.11

---

### Q3: What about encrypted OneNote sections?

**Current Behavior:** Desktop importer skips encrypted sections, reports warning

**Android:** Same behavior acceptable
- OneNote encryption requires user password
- UI complexity not worth v0.0.10 scope
- Document as known limitation

---

### Q4: Should Android users import or sync from desktop?

**Options:**
1. **Import directly on Android** (requires Phase 1-4 above)
   - Pro: User can import anytime
   - Con: Slower, more battery usage
   - Timeline: Phase 1-4 = 2-3 weeks

2. **Import on desktop, sync to Android** (already works now)
   - Pro: Fast, simple, proven
   - Con: Requires desktop export first
   - Timeline: Zero work

**Recommendation:** Support both
- v0.0.10: Sync workflow works (free)
- v0.0.10a: Direct Android import (future phase)

---

## Testing Strategy for Android

### Unit Tests (before device)
- [ ] `crates/onenote` builds for `aarch64-linux-android`
- [ ] Parser output unchanged from desktop build
- [ ] Sample `.one` files parse identically

### Integration Tests (egui simulator, if available)
- [ ] File picker returns ContentUri
- [ ] Parser accepts ContentUri stream
- [ ] HTML output matches desktop
- [ ] Markdown output matches desktop

### Device Tests (real hardware)
- [ ] Import small .one file (< 5 MB)
- [ ] Import medium .one file (10-50 MB)
- [ ] Verify pages, images, tables in both formats
- [ ] Verify format parity visually

### Stress Tests
- [ ] Large file import (100+ MB) - test memory/timeout
- [ ] Network interrupt during import - recovery
- [ ] Concurrent app use during import - no crash

---

## Timeline Estimate

| Phase | Work | Effort | Timeline |
|-------|------|--------|----------|
| **0** | Verify parser compiles | 2-4 hours | Week 1 |
| **1** | ContentUri integration | 1-2 days | Week 1-2 |
| **2** | UI file picker + progress | 2-3 days | Week 2 |
| **3** | Testing on device | 1-2 days | Week 3 |
| **4** | Polish + docs | 1 day | Week 3 |
| **Total** | Direct Android import | ~1.5-2 weeks | v0.0.10a candidate |

**Sync-only path (no direct import):** 0 effort, available now

---

## Related Issues on Desktop

These also affect Android if left unfixed:
- Container orphaning (issue #18) - affects both formats
- RTL text support (critical requirement)
- Image rendering quality
- Resize handle functionality

**Strategy:** Fix on desktop first (v0.0.8), inherit fixes on Android (v0.0.10)

---

## Success Criteria for v0.0.10

### MVP (Minimal for launch)
- Sync folder with OneNote imported on desktop works on Android
- All v0.0.9 features work on Android
- HTML and Markdown render identically

### Nice-to-have (for v0.0.10a)
- Direct .one import on device (Phase 1-4 above)
- Progress indication during import
- Cancel import mid-operation

### Post-v0.0.10
- Folder import (.onetoc2 notebooks)
- Encrypted section support
- OneNote metadata preservation

---

## Next Steps

1. **Immediate (before coding):**
   - [ ] Confirm user wants Android OneNote import in v0.0.10
   - [ ] Decide: direct import or sync-only for v0.0.10
   - [ ] Review `crates/onenote` for any Android blockers

2. **Phase 1 (architecture):**
   - [ ] Plan ContentUri integration
   - [ ] Design import API changes
   - [ ] Update BRANCHING_STRATEGY.md with Android phases

3. **Phase 2+ (implementation):**
   - [ ] Create feat/v0.0.10-android branch
   - [ ] Follow phase breakdown above

---

**Document:** `docs/releases/0.0.10/issues.md`  
**Status:** Analysis complete, ready for implementation planning  
**Owner:** Android release planning team

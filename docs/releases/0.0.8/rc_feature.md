# NoteCraft v0.0.8 Release Candidate  -  Feature Planning

**Status:** Planning  
**Target Release:** Q4 2026 or Q1 2027  
**Branch:** `feat/v0.0.8` (to be created)

---

## Priority Tiers

**Note:** Ongoing work items (RTL, OneNote import, Markdown format) are tracked in "Known Gaps" below.
This section lists NEW FEATURES for v0.0.8.

### 🔴 P0: Critical (Must Have)

#### 1. Localization & i18n System (with RTL UI Mirroring)
**IMPORTANT:** RTL language support requires SEPARATE MIRRORING BRANCH (very involved)
- [ ] Design NoteCraft's i18n architecture (independent vs launcher integration)
- [ ] Support 10+ languages (minimum parity with v0.0.7 launcher)
- [ ] Locale selector in UI
- [ ] RTL-aware locale handling
- [ ] **SEPARATE BRANCH:** UI mirroring for RTL layouts (Arabic, Hebrew, Persian, Urdu)
  - [ ] Right-aligned components
  - [ ] Mirrored icons and controls
  - [ ] RTL-first responsive design
- **Blocker:** Need clarity on i18n integration with ArtCraft Launcher

### 🟡 P1: High Priority (Should Have)

#### 2. Performance Instrumentation & Analysis Tool
- [ ] Integrate Rust performance profiling tools (flamegraph, cargo-flamegraph, criterion)
- [ ] Add structured logging to capture performance metrics
- [ ] Create log consumer tool that generates visualizations:
  - [ ] Flame charts (CPU time distribution)
  - [ ] Gantt charts (operation timeline)
  - [ ] Waterfall charts (dependency chains)
- [ ] Performance benchmarks against previous versions
- **Investigation needed:** Available Rust instrumentation (tracing, pprof, etc.)
- **See also:** `rc_perf.md` for detailed implementation strategy

### 🟢 P2: Medium Priority (Nice to Have)

#### 3. Canvas & Layout Features
- [ ] Improved free-form canvas editing
- [ ] Better positioning/alignment tools
- [ ] Grouped elements/containers
- [ ] Multi-select operations

#### 4. Collaboration Features (Future)
- [ ] Local multi-user access (read-only or sharing)
- [ ] Conflict resolution for synced edits
- [ ] Change history/versioning

#### 5. Plugin/Extension System (Exploratory)
- [ ] Plugin API design
- [ ] Custom rendering or data transformers
- [ ] Community extensions

---

## Parallel Branches (NOT in v0.0.8 core)

These are SEPARATE INITIATIVES that can run in parallel:

### Branch: `feat/rtl-ui-mirroring`
**Scope:** RTL UI layout mirroring for right-to-left languages
**Why separate:** Very involved, affects entire UI codebase
**Schedule:** Start after i18n base architecture is in place
**Owner approval required:** Yes (significant UX change)

### Branch: `feat/perf-instrumentation`
**Scope:** Performance profiling, logging, analysis tool
**Why separate:** Tool-building separate from core app
**Dependencies:** Need to investigate Rust instrumentation options first
**Deliverable:** Standalone performance analysis tool + flamegraph integration

---

## Ongoing Work (NOT new features for v0.0.8)

These are continuous improvements tracked separately:

| Item | Status | Reference |
|------|--------|-----------|
| **RTL text & caret excellence** | Ongoing | `work/rtl-and-caret-bar.md` |
| **OneNote import completeness** | Ongoing | `work/onenote-importer-plan.md` |
| **Markdown format improvements** | Ongoing | Feature backlog |

---

## Acceptance Criteria

### v0.0.8-rc1
- [ ] P0 items addressed (RTL, OneNote gaps)
- [ ] P1 items in progress or roadmapped
- [ ] 13 languages localized (or i18n system in place)
- [ ] Test coverage maintained from v0.0.7
- [ ] Passes all pre-release quality gates

### v0.0.8-final
- [ ] All P0 items complete
- [ ] 80% of P1 items complete
- [ ] Release notes with RTL/i18n improvements
- [ ] ArtCraft Launcher compatibility verified

---

## Dependencies & Blockers

1. **i18n Architecture Decision:** Clarify if NoteCraft will use launcher's i18n system or standalone
2. **RTL Spec Finalization:** Define exact caret behavior targets (reference app: OneNote/Obsidian/Notion)
3. **OneNote Format Research:** Complete remaining OneNote file format documentation

---

## Effort Estimate

| Category | Estimate | Notes |
|----------|----------|-------|
| RTL Implementation | 3-4 weeks | Depends on rendering library capabilities |
| OneNote Gaps | 2-3 weeks | Format research + implementation |
| i18n System | 1-2 weeks | Architecture design |
| Performance | 2 weeks | Profiling + optimization |
| Markdown Features | 1-2 weeks | Live preview implementation |
| **Total** | **~10-13 weeks** | Assuming parallel work |

---

## Review & Updates

**Last Updated:** 2026-10-30  
**Next Review:** After v0.0.7 is live on ArtCraft Launcher  
**Owner Approval:** Pending

---

**Related Documents:**
- `work/rtl-and-caret-bar.md`  -  RTL requirements & status
- `work/onenote-importer-plan.md`  -  OneNote import design
- `../../AGENTS.md`  -  Product requirements (lines 1-7)

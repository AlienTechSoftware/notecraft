# NoteCraft v0.0.8 Release Candidate  -  Feature Planning

**Status:** Planning  
**Target Release:** Q4 2026 or Q1 2027  
**Branch:** `feat/v0.0.8` (to be created)

---

## Priority Tiers

### 🔴 P0: Critical (Must Have)

#### 1. RTL Text & Caret Excellence
**Owner requirement:** "The caret and the right-to-left experience must be the best of any note taking application" (2026-10-19)
- [ ] Full RTL (Arabic, Hebrew, Persian, Urdu) support with proper text direction
- [ ] Caret positioning accuracy in mixed LTR/RTL text
- [ ] Text selection and navigation in RTL scripts
- [ ] Visual parity with OneNote, Obsidian, Notion in RTL handling
- **Reference:** `work/rtl-and-caret-bar.md`

#### 2. OneNote Import Completeness
- [ ] Close remaining format/content gaps from v0.0.7
- [ ] Support all OneNote section types
- [ ] Preserve formatting, images, attachments
- [ ] Handle nested sections and page hierarchies
- **Reference:** `work/onenote-importer-plan.md`

### 🟡 P1: High Priority (Should Have)

#### 3. Localization & i18n System
- [ ] Design NoteCraft's i18n architecture (independent vs launcher integration)
- [ ] Support 10+ languages (minimum parity with v0.0.7 launcher)
- [ ] Locale selector in UI
- [ ] RTL-aware locale handling
- **Blocker:** Need clarity on i18n integration with ArtCraft Launcher

#### 4. Performance & Scalability
- [ ] Optimize rendering for large notebooks (1000+ pages)
- [ ] Efficient notebook tree loading
- [ ] Search performance improvement
- [ ] Memory footprint reduction
- **Metric:** Sub-100ms page load, instant search on <10k notes

#### 5. Markdown Format Improvements
- [ ] Full CommonMark + GFM (CMGFM) support
- [ ] Better syntax highlighting for Markdown
- [ ] Live preview mode for .md pages
- [ ] Table editing in Markdown view

### 🟢 P2: Medium Priority (Nice to Have)

#### 6. Canvas & Layout Features
- [ ] Improved free-form canvas editing
- [ ] Better positioning/alignment tools
- [ ] Grouped elements/containers
- [ ] Multi-select operations

#### 7. Collaboration Features (Future)
- [ ] Local multi-user access (read-only or sharing)
- [ ] Conflict resolution for synced edits
- [ ] Change history/versioning

#### 8. Plugin/Extension System (Exploratory)
- [ ] Plugin API design
- [ ] Custom rendering or data transformers
- [ ] Community extensions

---

## Known Gaps from v0.0.7

| Category | Issue | Impact | P-Level |
|----------|-------|--------|---------|
| RTL | Caret positioning in mixed text | Critical for RTL users | P0 |
| i18n | No integrated i18n system | Blocks localization | P1 |
| OneNote | Format gaps (tables, embedded objects) | Import incomplete | P0 |
| Performance | Slow load on large notebooks | UX regression | P1 |
| Markdown | No live preview mode | Feature parity gap | P2 |

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

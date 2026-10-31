# v0.0.10 Android: OneNote Parser Choice Analysis

**Date:** 2026-10-31  
**Status:** Ideation - Decision Pending  
**Scope:** Which Rust OneNote parser to use for Android import

---

## The Question

With minimal effort, which OneNote parser should NoteCraft use for Android import?

Three Rust options available:
1. `crates/onenote` - embedded in o-NoteCraft
2. `onenote.rs` - external mature library (v2.1)
3. `one2html` - tool built on onenote.rs (not suitable)

---

## Option 1: Embedded `crates/onenote`

**What:** Clean-room OneNote parser already in o-NoteCraft  
**Location:** `.repos/o-NoteCraft/crates/onenote`  
**License:** (NoteCraft's own)  
**Android Effort:** 1.5-2 weeks

### Analysis
- Already integrated with NoteCraft data pipeline
- No external dependencies or licensing concerns
- Pure Rust, cross-platform ready
- May have format gaps (unknown vs onenote.rs)
- Maintenance burden on NoteCraft team

### Unknowns
- [ ] Feature completeness vs onenote.rs (OneDrive files, ink, math?)
- [ ] Bug frequency and maturity level
- [ ] Performance characteristics on Android
- [ ] Is it actively maintained or experimental?

---

## Option 2: `onenote.rs` External Library

**What:** Mature Rust OneNote parser (v2.1)  
**Location:** `C:\dev\p-NoteCraft\.repos\onenote.rs`  
**Repository:** github.com/msiemens/onenote.rs  
**License:** Mozilla Public License 2.0  
**Android Effort:** 1-2 weeks

### Features
- Reads `.one` section files ✓
- Reads `.onetoc2` table of contents files ✓
- Supports OneNote Desktop (2016, 2019, LTSC, etc) ✓
- Supports OneDrive downloads (FSSHTTP format) ✓
- Supports OneNote for Mac backups ✓
- Reads ink/handwriting content ✓
- Reads math/equation content ✓

### Analysis
- More mature, external project with more users/bug reports
- Better feature coverage (OneDrive, ink, math)
- Lower maintenance burden (external maintainer)
- Stable API (semantic versioning, major version bumps only)
- Well-architected (layered: fsshttpb -> onestore -> one -> onenote)
- Can add as dependency via Cargo.toml

### Unknowns
- [ ] MPL 2.0 license compatible with NoteCraft? (Need legal review)
- [ ] Integration effort with NoteCraft's block/format system
- [ ] Performance on Android (untested on mobile)
- [ ] Any missing features NoteCraft needs?

---

## Option 3: `one2html` Tool

**What:** CLI tool that converts OneNote to HTML  
**Built on:** onenote.rs  
**Location:** `C:\dev\p-NoteCraft\.repos\one2html`  
**Android Effort:** High/Impractical

### Why Not
- Converts to HTML output, not NoteCraft format
- Would need complete CLI tool ported to Android
- Over-engineered for our use case
- Overkill when we have parser libraries

### Verdict: ❌ Skip this option

---

## Option 4: Port OfficeIMO to Android

**Current OfficeIMO:** .NET/C# Windows-only bridge  
**Android Feasibility:** No (see issues.md)  
**Effort:** Massive (full rewrite)  
**Verdict:** ❌ Not viable, skip

---

## Comparison Matrix

| Factor | crates/onenote | onenote.rs | one2html | OfficeIMO |
|--------|---|---|---|---|
| **Android effort** | 1.5-2w | 1-2w | High | Massive |
| **Maturity** | Unknown | v2.1, proven | v1+ | N/A |
| **Feature completeness** | Unknown | Complete | Complete | N/A |
| **.one files** | Yes | Yes | Yes | Yes |
| **.onetoc2 files** | Unknown | Yes | Yes | Yes |
| **OneDrive format** | Unknown | Yes | Yes | Yes |
| **Ink support** | Unknown | Yes | Yes | Unknown |
| **Math support** | Unknown | Yes | Yes | Unknown |
| **License issues** | None | MPL 2.0 | MIT+MPL | N/A |
| **Maintenance** | Ours | External | External | N/A |
| **Cross-platform** | Yes | Yes | Yes | No |

---

## Decision Tree

```
START: Which parser for Android?
  |
  +-- Option 1: Use crates/onenote?
  |     [✓ Fast to implement]
  |     [? Unknown feature gaps]
  |     [? Unknown maturity]
  |     Action: Audit crates/onenote for completeness
  |
  +-- Option 2: Use onenote.rs?
  |     [✓ More mature]
  |     [✓ More features (OneDrive, ink, math)]
  |     [? License (MPL 2.0) compatibility]
  |     [? Integration effort with NoteCraft blocks]
  |     Action: Legal review of MPL 2.0, integration spike
  |
  +-- Option 3: Use one2html?
  |     [✗ Overkill, wrong output format]
  |     Action: SKIP
  |
  +-- Option 4: Port OfficeIMO?
  |     [✗ Not viable, massive effort]
  |     Action: SKIP

DECISION: After unknowns resolved -> pick Option 1 or 2
```

---

## Questions to Resolve

### Q1: Is `crates/onenote` feature-complete?
- **Who:** v0.0.10 planning team
- **Check:** 
  - [ ] Compare `crates/onenote` capabilities vs `onenote.rs`
  - [ ] Read `crates/onenote/src/one.rs` content model
  - [ ] Check for OneDrive format support
  - [ ] Check for ink/handwriting support
  - [ ] Check for math equation support
- **Impact:** If complete, saves dependency; if not, need onenote.rs

### Q2: Is MPL 2.0 license compatible?
- **Who:** Legal/owner review
- **Check:**
  - [ ] Review Mozilla Public License 2.0 terms
  - [ ] Confirm compatible with NoteCraft's licensing model
  - [ ] Check if any other dependencies have conflicts
- **Impact:** If incompatible, must use crates/onenote

### Q3: How much integration effort?
- **Who:** Implementation team (integration spike)
- **Check:**
  - [ ] Build proof-of-concept using onenote.rs
  - [ ] Map onenote.rs API to NoteCraft blocks
  - [ ] Estimate integration effort
- **Impact:** If <1 week, onenote.rs is viable; if >1 week, prefer crates/onenote

### Q4: What about backwards compatibility?
- **Who:** v0.0.10 planning team
- **Check:**
  - [ ] If we add onenote.rs as dependency, do we deprecate crates/onenote?
  - [ ] Or keep both for transition period?
  - [ ] What about v0.0.8 desktop importer (uses OfficeIMO)?
- **Impact:** Affects long-term maintenance strategy

### Q5: Performance on Android?
- **Who:** Testing team (after basic build)
- **Check:**
  - [ ] Build both parsers for Android target
  - [ ] Test on real device with 10-50 MB notebook
  - [ ] Measure memory usage
  - [ ] Measure parsing time
- **Impact:** If one has poor performance, affects user experience

---

## Recommended Next Steps

### Immediate (Before final decision)
1. **Audit `crates/onenote`**
   - Read implementation to understand feature completeness
   - Compare against onenote.rs capabilities
   - Document gaps

2. **Legal Review of MPL 2.0**
   - Confirm license compatibility
   - Check for any other licensing issues

3. **Integration Spike (if using onenote.rs)**
   - Proof-of-concept using onenote.rs
   - Map API to NoteCraft blocks
   - Estimate effort

### After Decision
- **If Option 1 (crates/onenote):**
  - Proceed with Android integration (ContentUri support)
  - v0.0.10 timeline: 1.5-2 weeks

- **If Option 2 (onenote.rs):**
  - Add to Cargo.toml dependencies
  - Replace crates/onenote calls in v0.0.8+ (or keep both for transition)
  - Android integration (ContentUri support)
  - v0.0.10 timeline: 1-2 weeks
  - v0.0.11: Full migration away from crates/onenote (optional)

---

## Risk Assessment

### Using `crates/onenote` (lower risk, possibly less capable)
- **Risk:** Feature gaps discovered mid-implementation
- **Mitigation:** Comprehensive audit before Android work starts

### Using `onenote.rs` (higher initial effort, more capable)
- **Risk:** Integration more complex than expected
- **Mitigation:** Integration spike before commitment
- **Risk:** License issue
- **Mitigation:** Legal review before decision

### Switching later (highest risk)
- **Risk:** Wasted effort rewriting if wrong choice
- **Mitigation:** Make decision upfront with good analysis

---

## Timeline Impact

**Assuming decision made by:** 2026-11-05 (1 week)

**If Option 1 (crates/onenote):**
- Android implementation: Week 2-3 (1.5-2 weeks)
- v0.0.10 release: ~2026-11-14

**If Option 2 (onenote.rs):**
- Spike/integration: Week 2 (3-4 days)
- Android implementation: Week 2-3 (1-2 weeks)
- v0.0.10 release: ~2026-11-14 (same timeline)

---

## Related Documents

- **docs/releases/0.0.10/issues.md** - Broader Android constraints analysis
- **docs/releases/0.0.10/README.md** - v0.0.10 scope and timeline
- **work/onenote-importer-plan.md** - Desktop importer (uses OfficeIMO)

---

## Decision Record

**Status:** ⏳ Pending decision  
**Last Updated:** 2026-10-31  
**Owner:** v0.0.10 planning team + legal review  
**Decision Needed By:** 2026-11-05

**Decision:** [TBD - to be filled in after analysis]

**Rationale:** [TBD - to be filled in after analysis]

**Implementation Plan:** [TBD - to be filled in after decision]

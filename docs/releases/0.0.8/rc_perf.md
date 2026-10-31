# NoteCraft v0.0.8 Release Candidate - Performance & Instrumentation

**Status:** Planning & Investigation  
**Target Release:** Q4 2026 or Q1 2027  
**Branch:** `0.0.8_perf` (active)  
**Related:** `feat/perf-instrumentation` (future implementation branch)

---

## Vision

Make NoteCraft's performance **visible, measurable, and optimizable**. By v0.0.8, developers and users should understand:
- Where time is spent (profiling)
- Where memory is allocated (memory analysis)
- How to identify and fix bottlenecks

This is **not about shipping faster code in v0.0.8**, but about **building the tools to measure and improve** in v0.0.9+.

---

## Phase 1: Instrumentation Infrastructure (v0.0.8)

### Goal
Add lightweight instrumentation to NoteCraft so performance data can be collected without impacting production users.

### Tier 1: Core Instrumentation (Mandatory)

**1.1 Integrate `tracing` Crate**
- [ ] Add `tracing` and `tracing-subscriber` dependencies
- [ ] Initialize tracing in main apps (window, CLI, MCP)
- [ ] Replace scattered `println!` debug logging with structured spans
- [ ] Support `RUST_LOG` environment variable for runtime control
- **Effort:** 2-3 days
- **Value:** Foundation for all profiling

**1.2 Instrument Hot Paths**
- [ ] `crates/ui-egui`: Frame rendering (`render_frame`)
- [ ] `crates/formats`: Page parsing (`parse_html`, `parse_markdown`)
- [ ] `crates/store`: File I/O (`save_page`, `load_page`)
- [ ] `crates/engine`: Command execution (`execute_command`)
- [ ] `crates/diagram`: Diagram rendering (`render_diagram`)
- **Effort:** 1 week (20-30 functions)
- **Value:** Real-time visibility into execution time

**1.3 Add Memory Tracking**
- [ ] Integrate `memory-stats` for heap monitoring
- [ ] Log memory usage at key points (startup, after load, peak)
- [ ] Add optional memory instrumentation span
- **Effort:** 2-3 days
- **Value:** Catch memory leaks, track allocation patterns

### Tier 2: Development Profiling Tools (Recommended)

**2.1 Flamegraph Support**
- [ ] Add `pprof` with flamegraph feature to dev-dependencies
- [ ] Create helper script: `scripts/profile.sh`
- [ ] Document usage in `docs/profiling.md`
- **Effort:** 1-2 days
- **Value:** Visualize CPU hotspots

**2.2 Benchmarking Framework**
- [ ] Add `criterion` to dev-dependencies
- [ ] Create baseline benchmarks for:
  - Page parsing (HTML vs. Markdown, small vs. large)
  - Rendering (empty page, full page, complex diagrams)
  - File operations (save, load, import)
- [ ] Integrate into CI (optional, non-blocking)
- **Effort:** 3-4 days
- **Value:** Regression detection, optimization verification

**2.3 Baseline Measurements (v0.0.6)**
- [ ] Document current performance (Frame time, memory, CPU)
- [ ] Test on:
  - Demo notebook (standard config)
  - Large notebook (stress test)
  - Real user notebook (if available)
- [ ] Create baseline report: `work/perf-baseline-0.0.6.md`
- **Effort:** 2-3 days
- **Value:** Reference for future improvements

---

## Phase 2: Performance Analysis Tool (v0.0.8 or v0.0.9)

**Status:** Future (scoped for Phase 2)

### Goal
Create a standalone tool that consumes tracing logs and generates performance visualizations.

### Deliverables

**2.1 Log Consumer**
- Read structured tracing output (JSON format)
- Extract timing information from spans
- Aggregate by function, module, category

**2.2 Visualizations**
- **Flame charts:** CPU time distribution (like flamegraph)
- **Gantt charts:** Operation timeline (when did each span run)
- **Waterfall charts:** Call dependency chains
- **Statistical summaries:** Min/max/median/p95/p99

**2.3 Integration**
- [ ] CLI tool: `notecraft-perf-analyze <logfile>`
- [ ] HTML dashboard: Interactive performance explorer
- [ ] CI integration: Auto-generate reports on benchmarks

**Effort:** 2-3 weeks (Phase 2 or later)

---

## Implementation Details

### Dependencies to Add

```toml
[dependencies]
# Production (low overhead)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["fmt", "env_logger"] }

[dev-dependencies]
# Development profiling
pprof = { version = "0.13", features = ["flamegraph"] }
memory-stats = "1.0"
criterion = "0.5"
```

### Code Changes (Example)

**Before:**
```rust
fn render_frame(ctx: &Context) -> Response {
    let start = std::time::Instant::now();
    // ... rendering code ...
    let elapsed = start.elapsed();
    eprintln!("Frame took {:?}", elapsed);
}
```

**After:**
```rust
#[tracing::instrument(skip(ctx))]
fn render_frame(ctx: &Context) -> Response {
    debug!("starting frame render");
    let span = info_span!("clear_surface");
    {
        let _enter = span.enter();
        // clear surface code
    }
    info!("frame complete");
    // ... rest of code ...
}
```

**Usage:**
```bash
RUST_LOG=debug cargo run --bin notecraft
# Outputs structured timing data
```

### Profiling Workflow

**1. Collect Data**
```bash
# Terminal 1: Run with tracing
RUST_LOG=debug cargo run --bin notecraft > /tmp/perf.log 2>&1

# Terminal 2: Load notebook, edit, render, etc.
# Ctrl-C to stop
```

**2. Generate Flamegraph**
```bash
CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --bin notecraft
open flamegraph.svg
```

**3. Analyze Results**
- Look for wide blocks (time spent there)
- Look for tall stacks (deep function calls)
- Identify top 3-5 hotspots

**4. Investigate**
```bash
RUST_LOG=info,notecraft::formats=debug cargo run --bin notecraft
# Focus on specific module
```

---

## Metrics to Track

### Startup Performance
| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| App launch to window | <500ms | ? | TBD |
| Load demo notebook | <1000ms | ? | TBD |
| First frame render | <100ms | ? | TBD |

### Runtime Performance
| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Frame render (60 FPS) | <16ms | ? | TBD |
| Page save | <100ms | ? | TBD |
| Page parse (10KB HTML) | <50ms | ? | TBD |
| Page parse (10KB Markdown) | <50ms | ? | TBD |
| Search (100 pages) | <100ms | ? | TBD |

### Memory Usage
| Metric | Target | Current | Status |
|--------|--------|---------|--------|
| Startup | <50MB | ? | TBD |
| After load (100 pages) | <100MB | ? | TBD |
| Peak (complex notebook) | <200MB | ? | TBD |

---

## Bottleneck Identification Strategy

### Known Potential Hotspots (To Verify)

1. **UI Rendering (`crates/ui-egui`)**
   - egui layout calculations
   - Text rendering (many fonts/scripts)
   - Tree traversal (many pages)
   - **Why it matters:** Every frame is rendered

2. **Page Parsing (`crates/formats`)**
   - HTML parsing (regex, DOM traversal)
   - Markdown parsing (multiple passes)
   - Math equation rendering (TeX parsing)
   - **Why it matters:** Happens on load, paste, import

3. **File I/O (`crates/store`)**
   - Disk reads (many small files)
   - Disk writes (notebook save)
   - Asset copying (imports)
   - **Why it matters:** Blocks UI, visible to user

4. **Allocation Patterns**
   - String concatenation (`format!`, `+`, `to_string`)
   - Vector pushes (reallocation)
   - Cloning (implicit in APIs)
   - **Why it matters:** Heap pressure, GC-like pauses

5. **Text Handling (`crates/text`)**
   - Arabic/RTL shaping
   - Caret positioning (per character)
   - Line layout (complex scripts)
   - **Why it matters:** O(n) in text length

### Measurement Plan

**Week 1: Baseline**
- [ ] Instrument all tier 1 hot paths
- [ ] Measure frame time, memory, CPU on demo notebook
- [ ] Create baseline report

**Week 2: Profiling**
- [ ] Generate flamegraph on realistic workload
- [ ] Identify top 5 hotspots
- [ ] Estimate impact of optimizing each (% of total time)

**Week 3: Initial Optimization (Optional)**
- [ ] Target top 1-2 hotspots
- [ ] Measure improvement
- [ ] Document findings

---

## Success Criteria

### v0.0.8-rc1
- [ ] `tracing` integrated and working
- [ ] 20+ hot paths instrumented
- [ ] `pprof` flamegraph capability functional
- [ ] Baseline metrics documented
- [ ] Profiling guide in `docs/profiling.md`
- [ ] CI passes with instrumentation (no regression)

### v0.0.8-final
- [ ] All tier 1 instrumentation complete
- [ ] Tier 2 (benchmarking) framework in place
- [ ] Performance analysis tool designed (not implemented)
- [ ] At least one optimization implemented (verified by benchmark)

### v0.0.9+ (Out of scope for v0.0.8)
- [ ] Optimization tooling fully built
- [ ] 10%+ improvement in at least one metric
- [ ] Continuous performance monitoring in CI

---

## Risk & Mitigation

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|-----------|
| **Instrumentation overhead** | Slower code | Medium | Use `RUST_LOG=info` by default (trace off) |
| **Over-instrumentation** | Code clutter | Low | Keep spans focused on hot paths |
| **Flamegraph inaccuracy** | Wrong conclusions | Low | Verify with multiple runs |
| **False positives** | Optimize wrong thing | Medium | Use criterion benchmarks to confirm |
| **Time overrun** | Misses schedule | Medium | Keep scope tight (tier 1 only) |

---

## Dependencies & Blockers

1. **Rust Profiling Knowledge:** Need to understand `tracing`, `pprof`, `criterion`
   - **Status:** Research complete (see `work/PERF_INSTRUMENTATION.md`)
   - **Blocker:** None

2. **Realistic Test Workload:** Real user notebook preferred
   - **Status:** Demo notebook available
   - **Blocker:** None (proceed with demo)

3. **Baseline Measurements:** Need to measure current performance
   - **Status:** To be done
   - **Blocker:** None (start immediately)

---

## Effort Estimate

| Task | Estimate | Notes |
|------|----------|-------|
| **Phase 1: Instrumentation** | 2-3 weeks | |
| 1.1 Add `tracing` | 2-3 days | Dependency + setup |
| 1.2 Instrument hot paths | 1 week | 20-30 functions |
| 1.3 Memory tracking | 2-3 days | `memory-stats` integration |
| 1.4 Testing & validation | 2-3 days | Verify tracing works end-to-end |
| **Phase 1 Subtotal** | **2-3 weeks** | |
| **Phase 2: Tools (Future)** | 2-3 weeks | Not in v0.0.8 |
| | | |
| **Parallel: Analysis** | 1 week | Profiling, baseline, recommendations |

---

## Roadmap Integration

### v0.0.8 Timeline
- **Week 1:** Instrumentation setup + tier 1 paths
- **Week 2:** Benchmarking framework + profiling
- **Week 3:** Baseline measurements + analysis
- **Week 4:** Polish, testing, documentation

### v0.0.9+
- Implement identified optimizations
- Build performance analysis tool
- Integrate into CI

---

## Open Questions for Owner

1. **Scope:** Is instrumentation a blocker for v0.0.8, or nice-to-have?
   - **Current recommendation:** Nice-to-have (don't delay release)

2. **Optimization:** Should v0.0.8 include performance improvements, or just instrumentation?
   - **Current recommendation:** Just instrumentation (tools + baseline)

3. **CI Integration:** Should benchmarks block PR merges if regression detected?
   - **Options:**
     - No (informational only)
     - Yes, with 5% tolerance
     - Yes, strict
   - **Current recommendation:** Informational, no blocking

4. **Tools:** Should we build the performance analysis tool in v0.0.8 or defer?
   - **Current recommendation:** Defer to v0.0.9

---

## Related Documents

- `rc_feature.md` - v0.0.8 feature planning (perf is P1 item #2)
- `work/PERF_INSTRUMENTATION.md` - Detailed research on Rust profiling tools
- `docs/profiling.md` - User guide for profiling NoteCraft (to be created)
- `work/perf-baseline-0.0.6.md` - Performance baseline (to be created)

---

## Review & Updates

**Last Updated:** 2026-10-30  
**Created By:** Claude (instrumentation research)  
**Status:** Ready for review  
**Next Step:** Owner decision on scope and timing

---

## Appendix: Command Reference

```bash
# Development: Run with tracing enabled
RUST_LOG=debug cargo run --bin notecraft

# Production-like: Just info level
RUST_LOG=info cargo run --bin notecraft

# Specific module: Quiet except formats parsing
RUST_LOG=info,notecraft::formats=debug cargo run --bin notecraft

# Generate flamegraph
CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --bin notecraft

# Run benchmarks
cargo bench --bench page_parsing

# Check memory usage
# (Add to main or test, run with RUST_LOG=info)
```


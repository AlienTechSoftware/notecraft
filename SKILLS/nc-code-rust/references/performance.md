# Performance & Optimization

## Profile First, Optimize Second

Never optimize without evidence of a bottleneck.

### Using flamegraph

```bash
cargo install flamegraph
cargo flamegraph -p notecraft -- [args]
# View as SVG in browser
```

### Using cargo-bench

```bash
cargo bench -p notecraft-engine
```

### Measuring Latency

```rust
use std::time::Instant;

let start = Instant::now();
expensive_operation();
let elapsed = start.elapsed();
println!("Operation took: {:?}", elapsed);  // Or log::info!
```

## Caching & Memoization

### Lazy-Initialized Globals

```rust
use std::sync::OnceLock;

fn get_config() -> &'static Config {
  static INSTANCE: OnceLock<Config> = OnceLock::new();
  INSTANCE.get_or_init(|| Config::load())
}
```

### Function Result Caching

```rust
use std::collections::HashMap;

pub struct LayoutCache {
  results: HashMap<String, CachedLayout>,
}

impl LayoutCache {
  pub fn get_or_compute(&mut self, text: &str) -> &CachedLayout {
    if let Some(cached) = self.results.get(text) {
      return cached;
    }
    
    let layout = expensive_layout_computation(text);
    self.results.insert(text.to_string(), layout);
    self.results.get(text).unwrap()
  }
}
```

## Avoiding Common Pitfalls

### Don't Clone When You Can Borrow

**Bad:**
```rust
fn process_page(page: Page) {  // Takes ownership; forced clone
  println!("{}", page.title);
}

let page = load_page()?;
process_page(page.clone());  // Clone required
process_page(page.clone());  // Another clone
```

**Good:**
```rust
fn process_page(page: &Page) {  // Borrows; no clone
  println!("{}", page.title);
}

let page = load_page()?;
process_page(&page);  // Just borrow
process_page(&page);  // No overhead
```

### Don't Re-Allocate in Loops

**Bad:**
```rust
for page in pages {
  let mut buffer = String::new();  // Allocated each iteration
  buffer.push_str(&page.title);
  process(&buffer);
}
```

**Good:**
```rust
let mut buffer = String::new();  // Allocated once
for page in pages {
  buffer.clear();  // Reuse allocation
  buffer.push_str(&page.title);
  process(&buffer);
}
```

### Use Iterator Chains Instead of Loops

**Bad (multiple allocations):**
```rust
let mut result = Vec::new();
for page in pages {
  if page.format == PageFormat::Html {
    result.push(page.title.clone());
  }
}
```

**Good (single allocation):**
```rust
let result: Vec<_> = pages
  .iter()
  .filter(|p| p.format == PageFormat::Html)
  .map(|p| p.title.clone())
  .collect();
```

### Avoid Unnecessary String Allocations

**Bad:**
```rust
format!("{}:{}", notebook, page)  // Creates String
```

**Good:**
```rust
write!(buffer, "{}:{}", notebook, page)  // No allocation
```

## Performance-Sensitive Code Paths

### RTL Text Layout (Known Bottleneck)

```rust
// This is called per-character; must be fast
fn compute_visual_position(logical_offset: usize, text: &str) -> f32 {
  // Cache results if possible
  // Avoid allocations in hot loop
  // Consider SIMD for character properties
}
```

### Search Indexing (Known Bottleneck)

```rust
// Building search index is O(n); only do it:
// - On notebook open (once)
// - After bulk edits (batched)
// - NOT on every keystroke
fn index_notebook_lazy() {
  if !needs_reindex() {
    return;  // Skip work
  }
  rebuild_search_index();
}
```

### Syntax Highlighting (Known Bottleneck)

```rust
// Highlight visible code blocks only, not entire page
fn highlight_visible_blocks(viewport: Rect, page: &Page) {
  for block in page.visible_blocks(viewport) {
    if let Block::Code(code) = block {
      highlight_code(code);  // Only visible blocks
    }
  }
}
```

## Benchmarking

Add criterion benchmarks:

```bash
cargo install cargo-criterion
```

```rust
// benches/layout_benchmark.rs
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_rtl_layout(c: &mut Criterion) {
  c.bench_function("layout_short_arabic", |b| {
    b.iter(|| {
      let text = black_box("مرحبا");
      BiDiLayout::new(text)
    })
  });
}

criterion_group!(benches, bench_rtl_layout);
criterion_main!(benches);
```

Run:
```bash
cargo bench --bench layout_benchmark
```

## Profile-Guided Optimization

For release builds, use PGO:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 16
```

Current NoteCraft settings (in `Cargo.toml`):
```toml
[profile.release]
opt-level = 3       # Maximum optimization
lto = "thin"        # Link-time optimization (balances speed/compile)
codegen-units = 16  # Parallel codegen (faster compilation)
debug = 0           # No debug info in release
```

## Measuring Frame Time (UI)

For the UI layer:

```rust
use std::time::Instant;

fn render_frame(&mut self) {
  let frame_start = Instant::now();
  
  // Rendering code here
  self.draw_ui();
  
  let frame_time = frame_start.elapsed();
  if frame_time.as_millis() > 16 {  // > 16ms = <60 FPS
    log::warn!("Slow frame: {:?}", frame_time);
  }
}
```

Target: 60 FPS = 16ms per frame

## Checklist

- [ ] Profiled before optimizing
- [ ] No unnecessary clones (use references)
- [ ] Allocations outside loops, not inside
- [ ] Iterator chains used where idiomatic
- [ ] No O(n²) algorithms without justification
- [ ] String formatting uses write!() in hot paths
- [ ] Search/layout results cached appropriately
- [ ] Benchmarks written for critical paths
- [ ] Release build settings optimized
- [ ] Frame time monitored in UI code

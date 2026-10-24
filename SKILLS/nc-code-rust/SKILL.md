---
name: nc-code-rust
description: 'NoteCraft Rust code standards, best practices, and idiom guide. Enforces 2-space indentation, error handling with Result, logging with log crate macros, comprehensive testing, zero unsafe code, and architectural awareness. References: official Rust style guide, ArtCraft patterns, NoteCraft code-quality audits. Ensures code is production-ready, maintainable, and consistent across crates. Invoke with /nc-code-rust or reference in code-assistant calls.'
license: MIT
metadata:
  version: "0.0.20261024"
  related_skills: [code-assistant, nc-arch, nc-prod]
---

# nc-code-rust - NoteCraft Rust Code Standards

## Core Rules (Non-Negotiable)

### 1. Indentation & Formatting
- **2 spaces per indentation level** (matches ArtCraft convention)
- Run `rustfmt` with project config before committing
- Max line width: 100 characters (standard, can extend to 120 for complex types)
- Use `rustfmt.toml` at workspace root to enforce 2-space indent:
  ```toml
  edition = "2024"
  hard_tabs = false
  tab_spaces = 2
  max_width = 100
  ```

### 2. Logging (Never println!)
- **Forbidden:** `println!()`, `eprintln!()` in any code except tests/examples
- **Mandatory:** Use `log` crate macros: `info!()`, `warn!()`, `error!()`, `debug!()`, `trace!()`
- Rationale: Allows log level filtering, file output, and structured logging at runtime
- Test output: Use `log::debug!()` in tests, visible with `RUST_LOG=debug cargo test`

### 3. Error Handling (No Panics in Happy Path)
- **Use `Result<T, E>`** for recoverable errors (file I/O, parsing, network)
- **Use `anyhow::Result`** or custom error types in application code
- **Use custom error types** for domain-specific errors (PageError, NotebookError, etc.)
- **Panic only for:**
  - Impossible invariant violations (with justifying comment)
  - Test setup failures
  - Unrecoverable system errors (stack overflow, OOM)
- **Never panic on user input or external data** - return Err() instead

**Error enum pattern:**
```rust
#[derive(Debug)]
pub enum PageError {
  NotFound(String),
  InvalidFormat { path: PathBuf, reason: String },
  CorruptedMetadata(String),
}

impl std::fmt::Display for PageError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}", self)  // ArtCraft pattern for simple errors
  }
}
```

### 4. Unsafe Code (Forbidden)
- **Zero unsafe code** in NoteCraft (enforced via workspace lints)
- If unsafe is truly needed, get explicit approval + design review
- Document SAFETY comments explaining why unsafe is justified (and it almost never is in application code)

### 5. File Organization (Top-to-Bottom Reading Order)
Organize code so a reader encounters important things first:

1. **Module docs** (`//!`) explaining purpose and key concepts
2. **Imports** (grouped: std, external, workspace, crate::)
3. **Constants** (configuration, magic numbers)
4. **Primary public type** (main struct/enum the file defines)
5. **Supporting types** (helpers, DTOs, serialization wrappers)
6. **impl blocks** (methods for each type)
7. **Tests** (`#[cfg(test)] mod tests { ... }`)

**Within impl blocks:**
1. Constructors (new, from_*, builder)
2. Public methods (API entry points)
3. Private helpers (meaty logic first, leaf helpers last)

### 6. Naming Conventions

| Category | Pattern | Example |
|----------|---------|---------|
| Variables, functions | `snake_case` | `create_page`, `notebook_path` |
| Types (struct, enum, trait) | `PascalCase` | `PageFormat`, `NotebookError` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_PAGES`, `DEFAULT_FORMAT` |
| Error types | `{Domain}Error` | `PageError`, `ImportError` |
| Request/Response types | `{Action}Request`, `{Action}Response` | `CreatePageRequest`, `CreatePageResponse` |
| Builders | `{Type}Builder` | `NotebookBuilder` |
| Optional fields | Prefix with `maybe_` | `maybe_description`, `maybe_parent_id` |
| Platform-specific files | `name.{platform}.rs` | `clipboard.win.rs`, `sync.mac.rs` |

### 7. Imports & Module Organization

**Grouped imports (in order):**
```rust
use std::path::{Path, PathBuf};  // stdlib first

use anyhow::Result;  // external crates
use serde::{Deserialize, Serialize};

use notecraft_model::PageFormat;  // workspace crates
use notecraft_store::tree::Node;

use crate::page;  // crate-local imports
use crate::error::PageError;
```

**Rules:**
- Explicit imports over wildcards (except in `api_doc.rs` and test modules)
- Import once at top; do NOT inline fully-qualified paths unless one-off reference
- Name collisions: alias with suffix → `use enums::CommonFormat as CommonFormatEnum;`

### 8. Tests (Comprehensive Coverage)

**Location:** Co-locate in same file via `#[cfg(test)] mod tests { ... }` at bottom

**Organization:**
```rust
#[cfg(test)]
mod tests {
  use super::*;  // Import parent module
  use test_helpers::*;  // Re-import helpers

  // Constants first (test data, fixture paths)
  const SAMPLE_NOTEBOOK_PATH: &str = "tests/fixtures/sample.nb";

  // Test functions (grouped by feature)
  mod creation_tests {
    #[test]
    fn should_create_page_with_title() { ... }
  }

  mod import_tests {
    #[test]
    fn should_import_onenote_section() { ... }
  }

  // Helpers last
  fn setup_test_notebook() -> Notebook { ... }
}
```

**What to test:**
- Enum serialization/deserialization (round-trip stability)
- Parser edge cases (empty input, malformed data)
- Error handling (expected failures return correct errors)
- Public API boundaries
- Complex business logic

**What NOT to test:**
- Trivial getters/setters
- struct construction
- Integration with external services (mock instead)

**Test naming:**
- Descriptive: `should_parse_html_page_with_embedded_images()`
- Avoid: `test1()`, `it_works()`

### 9. Documentation (Code Comments & Docs)

**Module-level docs** (`//!`) required for every crate and complex modules:
```rust
//! Bidirectional text layout for RTL content.
//!
//! Handles Arabic, Hebrew, Persian, Urdu, and mixed-direction text.
//! All text positions are logical (string indices), never visual.
//!
//! # Invariants
//! - Logical position ≤ text.len()
//! - RTL text always laid out right-to-left
//!
//! # Example
//! ```
//! let layout = BiDiLayout::new("مرحبا");  // Arabic
//! assert_eq!(layout.direction(), Direction::RightToLeft);
//! ```
```

**Function/type docs** (`///`):
```rust
/// Creates a new page with the given title and format.
///
/// # Arguments
/// * `title` - Page display name
/// * `format` - HTML or CommonMark
///
/// # Returns
/// A new Page with metadata initialized to current time.
///
/// # Errors
/// Returns `PageError::InvalidTitle` if title is empty.
pub fn create(title: &str, format: PageFormat) -> Result<Page, PageError> { ... }
```

**Comments in code:**
- Explain **why**, not **what** (code shows what; comment explains why)
- Keep to one line where possible
- Add comment above the line (preferred) or at end (for short lines)

**No stale docs.** If code changes, update docs in the same commit.

### 10. Performance (Think Before Optimizing)

- **Profile first.** Do not optimize without evidence of a bottleneck
- **Prefer clarity over micro-optimizations**
- **Use references** (`&T`) to avoid unnecessary clones
- **Lazy initialization** for expensive values (use `lazy_static` or `once_cell`)
- **Document performance-sensitive code** with comments

---

## Crate-Specific Guidelines

### Desktop App (`apps/notecraft`)
- GUI initialization with `egui` / `eframe`
- Logging to file (not stdout)
- High latency tolerance (UI responsiveness > speed)

### CLI (`apps/notecraft-cli`)
- Simple command dispatch
- Exit codes: 0 = success, 1 = error
- Help text via `clap` or manual implementation

### Engine (`crates/engine`)
- Command interpreter, zero external dependencies
- Session state machine (no panics on invalid input)
- Comprehensive error variants for all failure modes
- Every command path tested

### Store (`crates/store`)
- File I/O primitives (read/write notebooks, pages)
- Atomic writes (write-to-temp-then-rename pattern)
- Error types for filesystem failures

### Formats (`crates/formats`)
- HTML/CommonMark/CSV parsing and serialization
- No panics on malformed input
- Round-trip tests (parse → serialize → parse must be idempotent)

### UI (`crates/ui-egui`)
- Break large files (<1500 lines each)
- Document bidirectional (RTL) layout invariants
- Performance: measure frame time, target 60 FPS

### Import (`crates/import`)
- OneNote `.one` file parsing
- Graceful error handling for edge cases (missing metadata, etc.)
- Fixture tests with real `.one` files

---

## Pre-Commit Checklist

Before pushing any Rust code:

- [ ] `cargo fmt --all` passes (no changes needed)
- [ ] `cargo clippy --all-targets -- -D warnings` (zero warnings)
- [ ] `cargo test --all` passes (100% pass rate)
- [ ] No `println!()` / `eprintln!()` outside tests
- [ ] No `unwrap()` on user input or external data
- [ ] Meaningful error messages (not just error type names)
- [ ] Module-level docs added/updated for new modules
- [ ] Test coverage adequate (aim for 85%+)
- [ ] Code review: at least one other pair of eyes on non-trivial changes

**One-liner:**
```bash
cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test --all
```

---

## Common Patterns

### Custom Error Types
```rust
#[derive(Debug)]
pub enum NotebookError {
  NotFound(PathBuf),
  PermissionDenied(PathBuf),
  InvalidMetadata { path: PathBuf, detail: String },
}

impl std::fmt::Display for NotebookError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::NotFound(p) => write!(f, "Notebook not found: {}", p.display()),
      Self::PermissionDenied(p) => write!(f, "Permission denied: {}", p.display()),
      Self::InvalidMetadata { path, detail } => {
        write!(f, "Invalid metadata in {}: {}", path.display(), detail)
      }
    }
  }
}

impl std::error::Error for NotebookError {}
```

### Builder Pattern
```rust
pub struct NotebookBuilder {
  name: String,
  maybe_location: Option<PathBuf>,
  maybe_description: Option<String>,
}

impl NotebookBuilder {
  pub fn new(name: impl Into<String>) -> Self {
    Self {
      name: name.into(),
      maybe_location: None,
      maybe_description: None,
    }
  }

  pub fn set_location(mut self, path: PathBuf) -> Self {
    self.maybe_location = Some(path);
    self
  }

  pub fn build(self) -> Result<Notebook, NotebookError> {
    // Validation and construction
    Notebook::new(self.name, self.maybe_location, self.maybe_description)
  }
}
```

### Atomic File Writes
```rust
fn save_notebook(notebook: &Notebook, path: &Path) -> Result<(), std::io::Error> {
  let serialized = serde_json::to_string_pretty(notebook)?;
  let temp_path = path.with_extension("tmp");
  std::fs::write(&temp_path, serialized)?;
  std::fs::rename(&temp_path, path)?;
  Ok(())
}
```

### Logging
```rust
use log::{info, warn, error, debug};

pub fn create_page(title: &str) -> Result<Page, PageError> {
  debug!("Creating page: {}", title);
  
  if title.is_empty() {
    warn!("Attempted to create page with empty title");
    return Err(PageError::InvalidTitle);
  }
  
  let page = Page::new(title);
  info!("Page created: {} (id={})", title, page.id());
  Ok(page)
}
```

---

## When to Reach for Code-Assistant

Use `/code-assistant` for:
- Multi-file refactors
- Testing strategies
- Architecture decisions
- Complex error handling flows

Use `/nc-code-rust` for:
- Code review against these standards
- Rust idiom questions
- Style/naming guidance

---

## References

### Official Rust Standards

- **[Official Rust Style Guide](https://doc.rust-lang.org/1.0.0/style/)** - Authoritative; 100 char line width, trailing commas, block indent
- **[Cargo Book](https://doc.rust-lang.org/cargo/)** - Dependency management, workspace structure, publishing
- **[Edition 2024](https://blog.rust-lang.org/2024/11/21/Rust-2024-edition.html)** - Latest language features, breaking changes
- **[Clippy Lints](https://doc.rust-lang.org/clippy/)** - Static analysis; zero warnings required

### Summaries Created for NoteCraft

#### Code Style Foundations

**[Official Rust Style Guide Summary](C:\dev\p-NoteCraft\ideation\code-style-guide\ideation.md)**
- Core formatting: 100 char max, 4-space indent (NOTE: NoteCraft uses 2 spaces instead)
- Block indent over visual indent
- Trailing commas in multi-line lists
- Doc comments (///), attribute placement
- Comment style and length limits

**[Rust Best Practices (CodeLessGenie)](C:\dev\p-NoteCraft\ideation\code-style-guide\ideation.md)**
- Naming: `snake_case` vars, `PascalCase` types, `SCREAMING_SNAKE_CASE` constants
- Error handling: Result over panic, custom error types with `thiserror`
- Memory: borrowing over clones, Arc<Mutex<T>> for shared state
- Idiomatic patterns: iterators, Option/Result combinators, enums over booleans
- Testing: property-based tests, doctest examples
- Tools: cargo audit, tarpaulin, criterion benchmarks

#### NoteCraft-Specific Guidance

**[ArtCraft Code Style Patterns](C:\dev\p-NoteCraft\ideation\code-style-guide\code-style-artcraft.md)**
- **2-space indentation** (NOT 4) - matches NoteCraft convention
- File layout: constants → primary type → helpers → impl → tests
- impl block ordering: constructors → public → private helpers
- Error enum Display pattern: `write!(f, "{:?}", self)`
- Naming: `{verb}_{noun}_handler`, `{Feature}Error`, `{Action}Request/Response`
- Imports: grouped (std → external → workspace → crate::), explicit over wildcards
- No `println!` in production; use `log::` macros

**[ArtCraft Testing Patterns](C:\dev\p-NoteCraft\ideation\code-style-guide\testing-artcraft.md)**
- Co-locate tests: `#[cfg(test)] mod tests { ... }` at file bottom
- Test layout: imports → constants → tests → helpers
- Group related tests into sub-modules (only if 2+ tests)
- What to test: enum serialization, parsers, error mappings, edge cases
- Fixture structure: `tests/fixtures/` for sample data, `tests/common/` for helpers

**[ArtCraft Architecture Patterns](C:\dev\p-NoteCraft\ideation\code-style-guide\architecture-artcraft.md)**
- Type organization: Request → Response → Error → PathInfo
- Builder pattern for complex types with optional fields (`maybe_` prefix)
- Serde: derive with `#[serde(rename = "camelCase")]` for JSON
- Atomic file writes: write-to-temp, then rename for data safety
- Shared code: check sibling projects before duplicating

#### NoteCraft Implementation & Quality

**[Code Quality Audit](C:\dev\p-NoteCraft\work\code-quality\implementation-audit.md)**
- Current state: zero clippy warnings, 88 test modules, forbids unsafe ✓
- Issues to fix: 43 panics (convert to Result), large files (5k lines), sparse docs
- Crate breakdown: which are healthy, which need refactoring
- Summary table: panic count, file sizes, test coverage trends

**[Tidy-Up Tasks](C:\dev\p-NoteCraft\work\code-quality\tidy-up-tasks.md)**
- High priority: replace panics with errors, split large files, add docs
- Medium priority: audit panic usage, centralize test fixtures
- Low priority: duplication analysis, performance benchmarks
- Task templates: Definition of Done, estimated effort, acceptance criteria

**[Action Plan](C:\dev\p-NoteCraft\work\code-quality\action-plan.md)**
- Quick wins (5 hours): add module docs, categorize panics
- Sprint-by-sprint roadmap (Sprints 1-4)
- Success metrics: reduce panic count, file sizes, improve coverage
- One-liners: commands to run locally for audits

### Rust Idioms & Patterns

**Error Handling:**
- Use `Result<T, E>` for recoverable errors
- Define custom error enums for each module/domain
- Implement Display + Error traits
- Use `?` operator to propagate errors
- No `unwrap()` on user input or external data

**Testing:**
- 85%+ branch coverage minimum
- Unit tests co-located in source files
- Integration tests in `tests/` directory
- Property-based tests with `proptest` when round-trip matters
- Mock external services; test behavior, not implementation

**Concurrency:**
- No shared mutable state without Mutex/RwLock
- Use `Arc<T>` for shared ownership
- Test with multiple threads to catch race conditions
- Document which operations are atomic

**Documentation:**
- Module-level `//!` docs explaining purpose and invariants
- Function-level `///` docs with examples in doctest format
- Comments explain WHY, not WHAT (code shows what)
- Keep docs synchronized with code (same commit)

### Debugging & Logging

- Use `log::` crate: `info!()`, `warn!()`, `error!()`, `debug!()`, `trace!()`
- Set log level at runtime: `RUST_LOG=debug cargo run`
- Preserve context in error messages (don't just print error type)
- Avoid personally-identifiable information in logs

### Performance

- **Profile first:** use `flamegraph` or `cargo-flamegraph`
- **Measure before & after:** use `criterion` for benchmarks
- **Prefer clarity:** optimization is secondary to correctness
- **Cache expensive computations:** layout results, search indices
- **Batch operations:** don't update on every keystroke

### Cross-Platform Considerations

- Use `PathBuf` instead of string paths (platform-agnostic)
- Use `std::fs` for file I/O (works on all platforms)
- For platform-specific code, use `.win.rs`, `.mac.rs`, `.lnx.rs` files
- Test on Windows, macOS, and Linux (or CI)
- Fallback to pure Rust when platform code unavailable

---

**Last updated:** 2026-10-24  
**Enforced in:** `C:\dev\p-NoteCraft\.repos\o-NoteCraft_2`  
**Workspace lints:** `unsafe_code = "forbid"`, `clippy::all = "warn"`

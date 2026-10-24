# Testing Best Practices

## Test Layout (Co-Located)

Put tests at the bottom of the source file using `#[cfg(test)]` module:

```rust
// src/page.rs

pub struct Page { ... }

impl Page {
  pub fn new(title: &str) -> Result<Self, PageError> { ... }
}

#[cfg(test)]
mod tests {
  use super::*;
  
  // Constants first
  const SAMPLE_TITLE: &str = "My Page";
  
  // Tests grouped by feature
  mod creation {
    use super::*;
    
    #[test]
    fn should_create_page_with_title() { ... }
    
    #[test]
    fn should_reject_empty_title() { ... }
  }
  
  mod formatting {
    use super::*;
    
    #[test]
    fn should_apply_bold_formatting() { ... }
  }
  
  // Helpers at the end
  fn setup_test_page() -> Page { ... }
}
```

**Rules:**
- Only create sub-modules if you have 2+ tests in that category
- Keep nesting to max 2 levels: `mod tests { mod category { ... } }`
- Helpers go in parent module so sub-modules can access via `use super::*`

## Test Structure

Every test should follow AAA pattern:

```rust
#[test]
fn should_validate_page_title() {
  // Arrange: set up test data
  let empty_title = "";
  
  // Act: perform the operation
  let result = Page::create(empty_title, PageFormat::Html);
  
  // Assert: verify the outcome
  assert!(matches!(result, Err(PageError::InvalidTitle)));
}
```

## Fixtures & Test Data

Create a `tests/fixtures/` directory for reusable test data:

```
tests/
├── fixtures/
│   ├── notebooks/
│   │   ├── simple.nb/
│   │   │   ├── page1.html
│   │   │   └── page2.html
│   │   └── complex-rtl.nb/
│   ├── import/
│   │   ├── sample.one
│   │   └── with-tables.one
│   └── assets/
│       ├── image.png
│       └── code.txt
├── common/
│   ├── builders.rs
│   ├── assertions.rs
│   └── lib.rs
└── integration_tests.rs
```

Load fixtures in tests:

```rust
#[test]
fn should_import_onenote_section() {
  let fixture = std::fs::read("tests/fixtures/import/sample.one")
    .expect("Missing test fixture");
  let result = import_section(&fixture);
  assert!(result.is_ok());
}
```

## Test Helpers & Builders

Create builders for complex test setup:

```rust
// tests/common/builders.rs

pub struct PageBuilder {
  title: String,
  maybe_format: Option<PageFormat>,
}

impl PageBuilder {
  pub fn new(title: &str) -> Self {
    Self {
      title: title.to_string(),
      maybe_format: None,
    }
  }
  
  pub fn format(mut self, fmt: PageFormat) -> Self {
    self.maybe_format = Some(fmt);
    self
  }
  
  pub fn build(self) -> Page {
    Page {
      title: self.title,
      format: self.maybe_format.unwrap_or(PageFormat::Html),
      ..Default::default()
    }
  }
}

// Usage in tests:
#[test]
fn should_support_markdown_pages() {
  let page = PageBuilder::new("My Page")
    .format(PageFormat::Cmgfm)
    .build();
  assert_eq!(page.format, PageFormat::Cmgfm);
}
```

## Property-Based Testing

Use `proptest` for round-trip verification (especially important for formats):

```rust
use proptest::prelude::*;

#[test]
fn html_round_trip_is_idempotent() {
  proptest!(|(content in ".*")| {
    // Parse HTML → Page, then serialize Page → HTML
    let page = parse_html(&content).unwrap();
    let serialized = serialize_html(&page).unwrap();
    
    // Parsing serialized content should give back equivalent page
    let reparsed = parse_html(&serialized).unwrap();
    prop_assert_eq!(page.blocks, reparsed.blocks);
  });
}
```

## What to Test

### Error Paths (Non-Negotiable)

```rust
#[test]
fn should_return_file_not_found_for_missing_page() {
  let result = load_page(Path::new("/nonexistent"));
  assert!(matches!(result, Err(PageError::NotFound(_))));
}

#[test]
fn should_return_invalid_format_for_bad_json() {
  let result = load_page(Path::new("tests/fixtures/invalid.json"));
  assert!(matches!(result, Err(PageError::InvalidFormat { .. })));
}
```

### Enum Serialization/Deserialization

```rust
#[test]
fn page_format_round_trips() {
  for format in [PageFormat::Html, PageFormat::Cmgfm] {
    let serialized = format.to_str();
    let deserialized = PageFormat::from_str(serialized).unwrap();
    assert_eq!(format, deserialized);
  }
}

#[test]
fn page_format_has_all_variants() {
  assert_eq!(PageFormat::all().len(), 2);  // HTML and CMGFM
}
```

### Parser Edge Cases

```rust
#[test]
fn should_parse_empty_page() {
  let result = parse_html("");
  assert!(result.is_ok());
  assert_eq!(result.unwrap().blocks.len(), 0);
}

#[test]
fn should_handle_malformed_html() {
  let bad_html = "<div><p>Unclosed";
  let result = parse_html(bad_html);
  assert!(result.is_err());  // Graceful error, not panic
}

#[test]
fn should_strip_invalid_unicode() {
  let with_replacement = "Hello\u{FFFD}World";
  let result = parse_html(with_replacement);
  assert!(result.is_ok());  // Doesn't crash
}
```

### What NOT to Test

- Trivial getters/setters
- Struct construction
- Stdlib behavior (assume it works)
- Third-party library behavior (that's their job)

## Test Isolation

Each test must be independent:

```rust
#[test]
fn test_a() {
  let temp = tempfile::TempDir::new().unwrap();  // Own temp dir
  // Test operates on temp, not shared state
}

#[test]
fn test_b() {
  let temp = tempfile::TempDir::new().unwrap();  // Separate temp dir
  // No interference with test_a
  // TempDir auto-cleanup on drop
}
```

Use `tempfile::TempDir` for file system tests:

```rust
#[test]
fn should_persist_page_to_disk() {
  let temp = tempfile::TempDir::new().unwrap();
  let page_path = temp.path().join("page.html");
  
  let page = Page::new("Test");
  save_page(&page, &page_path).unwrap();
  
  assert!(page_path.exists());
  let loaded = load_page(&page_path).unwrap();
  assert_eq!(loaded.title, "Test");
}  // TempDir deleted here
```

## Coverage & Metrics

### Target: 85% Branch Coverage

Run with `tarpaulin`:

```bash
cargo tarpaulin --all --timeout 300 --out Html
```

Check per-crate:

```bash
cargo tarpaulin -p notecraft-engine
```

### What Counts

- Branch coverage: did your test exercise both sides of if statements?
- Path coverage: did you test error paths, not just happy path?

### Exclusions (OK to skip)

- Generated code
- Platform-specific code (test on native platform only)
- Throwaway scripts
- External dependencies (trust their tests)

## Running Tests

### Run All Tests

```bash
cargo test --all
```

### Run with Output

```bash
cargo test --all -- --nocapture
```

### Run Specific Crate

```bash
cargo test -p notecraft-engine
```

### Run Specific Test

```bash
cargo test page::tests::creation::should_create_page
```

### Run with Logging

```bash
RUST_LOG=debug cargo test -p notecraft-engine
```

### Run Tests Serially (Useful for Debugging)

```bash
cargo test --all -- --test-threads=1
```

## Test Performance

- Avoid expensive setup (use lazy fixtures)
- Mock external services (don't call real APIs)
- Run file tests in parallel (each uses own `TempDir`)
- Batch slow operations (e.g., don't create 1000 test pages sequentially)

## Checklist

- [ ] Every error path has a test
- [ ] Enum serialization tested (to_str, from_str, round-trip)
- [ ] Parser tested with edge cases (empty, malformed, unicode)
- [ ] Each test is independent (no shared state)
- [ ] Fixtures in `tests/fixtures/`
- [ ] Helpers in test modules or `tests/common/`
- [ ] Coverage ≥ 85%
- [ ] All tests pass before commit
- [ ] Test names describe the scenario (`should_*`, not `test1()`)

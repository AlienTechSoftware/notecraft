# Error Handling Patterns

## Custom Error Enums

Every module should define its own error type for domain-specific failures.

### Pattern 1: Simple Enum with Display

```rust
#[derive(Debug)]
pub enum PageError {
  NotFound(String),
  InvalidTitle,
  PermissionDenied(PathBuf),
}

impl std::fmt::Display for PageError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}", self)  // ArtCraft pattern
  }
}

impl std::error::Error for PageError {}
```

### Pattern 2: Rich Context (From ArtCraft)

```rust
#[derive(Debug)]
pub enum NotebookError {
  FileNotFound(String),
  InvalidMetadata { path: PathBuf, reason: String },
  CorruptedData { 
    location: String, 
    detail: String 
  },
}

impl std::fmt::Display for NotebookError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::FileNotFound(p) => write!(f, "Not found: {}", p),
      Self::InvalidMetadata { path, reason } => 
        write!(f, "Invalid metadata in {}: {}", path.display(), reason),
      Self::CorruptedData { location, detail } => 
        write!(f, "Corrupted at {}: {}", location, detail),
    }
  }
}

impl std::error::Error for NotebookError {}
```

## Error Propagation

### Use `?` Operator (Never `unwrap()` on External Data)

**Bad:**
```rust
fn load_page(path: &Path) -> Page {
  let content = std::fs::read_to_string(path).unwrap();  // Crashes on missing file!
  serde_json::from_str(&content).unwrap()  // Crashes on invalid JSON!
}
```

**Good:**
```rust
fn load_page(path: &Path) -> Result<Page, PageError> {
  let content = std::fs::read_to_string(path)
    .map_err(|e| PageError::FileNotFound(path.display().to_string()))?;
  
  serde_json::from_str(&content)
    .map_err(|e| PageError::InvalidFormat { 
      path: path.to_path_buf(), 
      reason: e.to_string() 
    })
}
```

### Converting Between Error Types

Use `From` trait for automatic conversion:

```rust
#[derive(Debug)]
pub enum ImportError {
  ParseFailed(String),
  FileNotFound(String),
  ValidationFailed(String),
}

impl From<std::io::Error> for ImportError {
  fn from(e: std::io::Error) -> Self {
    ImportError::FileNotFound(e.to_string())
  }
}

impl From<serde_json::Error> for ImportError {
  fn from(e: serde_json::Error) -> Self {
    ImportError::ParseFailed(e.to_string())
  }
}

// Now you can use `?` with multiple error types:
fn import_section(path: &Path) -> Result<Notebook, ImportError> {
  let data = std::fs::read(path)?;  // std::io::Error → ImportError
  let json: Value = serde_json::from_slice(&data)?;  // serde_json::Error → ImportError
  Ok(parse_notebook(json))
}
```

## When to Use `anyhow::Result`

Use `anyhow::Result<T>` when:
- You're in application/binary code (not library code)
- You don't need domain-specific error variants
- You want to propagate errors quickly and not worry about type conversions

```rust
// In bin/main.rs or app-level functions
use anyhow::Result;

fn main() -> Result<()> {
  let config = load_config()?;  // Any error gets wrapped
  let session = Session::open(&config.notebook_path)?;
  run_repl(session)?;
  Ok(())
}
```

Do NOT use `anyhow::Result` in library code - it hides error details from callers.

## Panic vs Error

### When to Panic

- **Impossible invariant violation** (with comment explaining why):
  ```rust
  let page_id = pages.first()
    .expect("BUG: page list should never be empty after creation");
  ```

- **Test setup failure** (test is being skipped anyway):
  ```rust
  #[test]
  fn test_import() {
    let notebook = setup_test_notebook();  // OK to panic here
    assert_eq!(notebook.pages.len(), 5);
  }
  ```

- **Unrecoverable system errors** (e.g., memory exhaustion - you're going down anyway):
  ```rust
  let buffer = Vec::with_capacity(gigabytes)
    .expect("Failed to allocate memory for notebook");
  ```

### Never Panic On

- User input (parse fails → return error)
- File I/O (file missing → return error)
- Network (timeout → return error)
- External data (malformed JSON → return error)

**Bad:**
```rust
fn parse_page(json: &str) -> Page {
  serde_json::from_str(json).expect("Invalid JSON")  // User gets panicked app!
}
```

**Good:**
```rust
fn parse_page(json: &str) -> Result<Page, PageError> {
  serde_json::from_str(json)
    .map_err(|e| PageError::InvalidFormat { 
      reason: e.to_string() 
    })
}
```

## Error Recovery Strategies

### Graceful Degradation

```rust
pub fn load_notebook_with_metadata(path: &Path) -> Result<Notebook, NotebookError> {
  let mut nb = load_notebook(path)?;  // Core content is mandatory
  
  match load_metadata(path) {  // Metadata is optional
    Ok(meta) => nb.metadata = meta,
    Err(e) => log::warn!("Metadata missing; using defaults: {}", e),
  }
  
  Ok(nb)
}
```

### Retry with Backoff

```rust
pub fn load_with_retry(path: &Path) -> Result<Page, PageError> {
  for attempt in 0..3 {
    match std::fs::read_to_string(path) {
      Ok(content) => return parse_page(&content),
      Err(e) if attempt < 2 => {
        log::warn!("Attempt {} failed: {}; retrying...", attempt + 1, e);
        std::thread::sleep(std::time::Duration::from_millis(100 * 2_u64.pow(attempt)));
      }
      Err(e) => return Err(PageError::FileNotFound(e.to_string())),
    }
  }
  unreachable!()
}
```

### Fallback Values

```rust
pub fn load_notebook_or_create_empty(path: &Path) -> Notebook {
  match load_notebook(path) {
    Ok(nb) => nb,
    Err(e) => {
      log::warn!("Failed to load notebook: {}; creating empty", e);
      Notebook::new("Untitled", None)
    }
  }
}
```

## Testing Error Paths

Always test that errors are returned correctly:

```rust
#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn should_return_not_found_for_missing_file() {
    let result = load_page(Path::new("/nonexistent/page.html"));
    assert!(matches!(result, Err(PageError::NotFound(_))));
  }

  #[test]
  fn should_return_invalid_format_for_bad_json() {
    let result = load_page(Path::new("tests/fixtures/invalid.json"));
    assert!(matches!(result, Err(PageError::InvalidFormat { .. })));
  }

  #[test]
  fn should_validate_title() {
    let result = Page::create("", PageFormat::Html);
    assert!(matches!(result, Err(PageError::InvalidTitle)));
  }
}
```

## Error Chain Messages

When converting errors, preserve context:

**Bad:**
```rust
load_page(path).map_err(|_| PageError::LoadFailed)?
```

**Good:**
```rust
load_page(path).map_err(|e| {
  log::error!("Failed to load page {}: {}", path.display(), e);
  PageError::LoadFailed(format!("{}; inner: {}", path.display(), e))
})?
```

## Checklist

- [ ] Every fallible function returns `Result<T, E>`
- [ ] Custom error enum defined for module/domain
- [ ] Error enum implements Display + Error traits
- [ ] No `unwrap()` on user input or external data
- [ ] Panics have justifying comments
- [ ] Error paths tested (every Err variant has a test)
- [ ] Error messages are user-friendly and contextual
- [ ] Retry/fallback logic documented if used

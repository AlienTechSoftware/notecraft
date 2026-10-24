# Logging & Debugging

## The `log` Crate (Not `println!`)

**Core Rule:** Never use `println!()` or `eprintln!()` in production code. Use `log::` macros.

### Setting Up Logging

In your `Cargo.toml`:

```toml
[dependencies]
log = "0.4"

[dev-dependencies]
env_logger = "0.11"  # For running tests with RUST_LOG
```

### Log Levels

```rust
use log::{trace, debug, info, warn, error};

trace!("Extremely detailed internal state");  // Rarely used
debug!("Development-level detail");          // Development/testing
info!("Important workflow events");          // User-facing progress
warn!("Unexpected but recoverable");         // Degradation
error!("Serious problem");                   // Critical failures
```

## When to Log Each Level

### `info!` - High-Level Progress

User cares about this. Happens rarely (once per operation).

```rust
info!("Opening notebook: {}", path.display());
info!("Page saved: id={}, title={}", page.id, page.title);
info!("Import complete: {} pages, {} tables", pages.len(), tables.len());
```

### `debug!` - Development Detail

Helps developers debug, not needed in production.

```rust
debug!("Parsing page: {}", page_path);
debug!("Found {} blocks in page", blocks.len());
debug!("Layout cache hit rate: {}%", hit_rate);
```

### `warn!` - Unexpected But Recoverable

App continues, but something isn't ideal.

```rust
warn!("Metadata missing for page {}; using defaults", page_id);
warn!("Failed to load thumbnail; showing placeholder");
warn!("Search index out of date; rebuilding...");
```

### `error!` - Serious Problem

Something broke, but we're handling it gracefully.

```rust
error!("Failed to save notebook: {}", e);
error!("Import aborted: corrupted section file");
error!("Disk full; cannot write checkpoint");
```

### `trace!` - Almost Never Use

```rust
trace!("Entering function: compute_layout()");  // Rarely useful
```

## Logging Context

Always include relevant context:

**Bad:**
```rust
info!("Page created");  // Which page? When? Where?
```

**Good:**
```rust
info!("Page created: id={}, title='{}', notebook={}", 
  page.id, page.title, notebook_path.display());
```

## Structured Logging (Optional but Better)

For complex operations, consider returning diagnostic info:

```rust
pub struct ImportResult {
  pub pages: usize,
  pub tables: usize,
  pub errors: Vec<ImportError>,
  pub duration: Duration,
}

pub fn import_section(path: &Path) -> Result<ImportResult, ImportError> {
  let start = Instant::now();
  let (pages, tables, errors) = do_import(path)?;
  
  let duration = start.elapsed();
  info!("Import complete: {} pages, {} tables, {} errors in {:?}",
    pages, tables, errors.len(), duration);
  
  Ok(ImportResult { pages, tables, errors, duration })
}
```

## Running with Logging

### Set Log Level at Runtime

```bash
# Show debug and above
RUST_LOG=debug cargo run

# Show only info and above (default)
RUST_LOG=info cargo run

# Specific crate
RUST_LOG=notecraft_engine=debug cargo run

# Multiple crates
RUST_LOG=notecraft_engine=debug,notecraft_store=info cargo run

# Show everything (rarely useful)
RUST_LOG=trace cargo run
```

### In Tests

```bash
# Show debug output during tests
RUST_LOG=debug cargo test

# Show output for a specific test
RUST_LOG=debug cargo test test_import -- --nocapture
```

### File Logging (Desktop App)

Apps should log to a file:

```rust
// In apps/notecraft/src/main.rs
use log4rs;

fn init_logging() -> Result<()> {
  let log_file = dirs::config_dir()
    .ok_or("No config dir")?
    .join("notecraft")
    .join("app.log");
  
  log4rs::init_file("log4rs.yaml", Default::default())?;
  Ok(())
}
```

`log4rs.yaml`:
```yaml
appenders:
  file:
    kind: file
    path: "~/.config/notecraft/app.log"

root:
  level: info
  appenders:
    - file
```

## Performance Considerations

**Avoid expensive computations in log statements:**

**Bad:**
```rust
debug!("All pages: {:?}", all_pages_in_notebook());  // Expensive function call!
```

**Good:**
```rust
if log::log_enabled!(log::Level::Debug) {
  debug!("All pages: {:?}", all_pages_in_notebook());
}
```

Or use lazy evaluation:

```rust
use log::Level;

if log::max_level() >= Level::Debug {
  debug!("All pages: {:?}", expensive_fn());
}
```

## Error Logging

Always include the underlying error:

**Bad:**
```rust
error!("Failed to save page");  // User doesn't know why
```

**Good:**
```rust
error!("Failed to save page: {}", e);  // Shows the actual error

// Even better with context:
error!("Failed to save page id={}: {}", page_id, e);
```

For nested errors, show the chain:

```rust
error!("Failed to import notebook: {}", err);
if let Some(source) = err.source() {
  error!("  Caused by: {}", source);
}
```

## No Secrets in Logs

**Never log:**
- Passwords
- API keys / tokens
- Personal data (emails, names)
- Unredacted file paths (especially on multi-user systems)

**OK to log:**
- Page titles (user's own content)
- File operation types (read/write/delete)
- Notebooks being opened (user knows they opened it)

## Debugging Without Logs

### Use `dbg!` Macro in Tests Only

```rust
#[test]
fn test_layout() {
  let text = "مرحبا";
  let layout = BiDiLayout::new(text);
  dbg!(&layout);  // Prints layout to stderr in tests
}
```

### Use a Debugger (When Logs Aren't Enough)

```bash
# With lldb (macOS/Linux)
lldb target/debug/notecraft
(lldb) break set -f page.rs -l 42
(lldb) run
(lldb) frame variable  # Print locals
```

## Checklist

- [ ] No `println!()` in production code
- [ ] All log statements use appropriate level (trace/debug/info/warn/error)
- [ ] Log statements include context (IDs, paths, counts)
- [ ] Error logs include the underlying error message
- [ ] No expensive operations inside log macros (or guarded with `log_enabled!`)
- [ ] No secrets or sensitive data in logs
- [ ] Tested with `RUST_LOG=debug cargo test`
- [ ] File logging set up for desktop app
- [ ] Log level clearly documented (what goes where)

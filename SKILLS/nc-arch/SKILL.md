---
name: nc-arch
description: 'NoteCraft architectural principles and design patterns (WORK IN PROGRESS). Documents how crates relate, data flows, ownership, and cross-cutting concerns. Foundation for design work and code reviews. Being refined as implementation evolves. Reference when architecting new features or reviewing major changes.'
license: MIT
metadata:
  version: "0.0.20261024.1"
  status: "WIP - evolving with implementation"
  related_skills: [nc-hl-design, nc-code-rust, nc-prod]
---

# nc-arch - NoteCraft Architecture

**STATUS: WORK IN PROGRESS** - This skill is being built as the implementation evolves. Principles here are derived from the current codebase (`o-NoteCraft_2`) and will be refined as new features are added.

---

## Core Architecture

### Layered Design

```
┌───────────────────────────────────────┐
│ Presentation Layer                    │
│ - UI (egui window, dialogs)           │
│ - CLI (command parsing)               │
│ - MCP (server/REPL integration)       │
└───────────────┬───────────────────────┘
                │ Commands, Output
┌───────────────▼───────────────────────┐
│ Engine Layer                          │
│ - Session (state machine)             │
│ - Commands (execute user actions)     │
│ - Error handling                      │
│ - Single code path for all UIs        │
└───────────────┬───────────────────────┘
                │ Read/Write requests
┌───────────────▼───────────────────────┐
│ Store Layer                           │
│ - Notebook/Page/Section I/O           │
│ - Tree navigation and search          │
│ - File system abstraction             │
└───────────────┬───────────────────────┘
                │ Read/Write files
┌───────────────▼───────────────────────┐
│ Format Layer                          │
│ - HTML serialization/deserialization  │
│ - CommonMark (CMGFM) parsing          │
│ - Block types, AST                    │
│ - Table, code, math rendering hints   │
└───────────────┬───────────────────────┘
                │ Structured data
┌───────────────▼───────────────────────┐
│ Model Layer                           │
│ - Core types: Notebook, Page, Block   │
│ - Metadata, timestamps                │
│ - Enums: PageFormat, BlockType        │
│ - No I/O (pure data)                  │
└───────────────────────────────────────┘
```

Each layer depends only on layers below. Horizontal dependencies (format ↔ store) must go through defined interfaces.

---

## Crate Organization

### Core (`crates/model`, `crates/store`)

| Crate | Purpose | Exports |
|-------|---------|---------|
| `model` | Core domain types | `Notebook`, `Page`, `Section`, `Block`, `PageFormat`, etc. |
| `store` | Notebook I/O, tree ops | `read_node()`, `write_page()`, `search()`, `Tree`, `Hit` |
| `engine` | Command interpreter | `Session`, `run()`, `Output`, `Params`, `EngineError` |

**Invariants:**
- `model` has ZERO dependencies on I/O, UI, or file system
- `store` uses `model` types but never modifies them (returns copies/references)
- `engine` orchestrates `store` operations; Session owns state

### Formats (`crates/formats`)

Serialization and deserialization for page content.

| Format | Location | Notes |
|--------|----------|-------|
| HTML | `formats/src/html.rs` | Primary format; optimized for round-tripping |
| CommonMark+GFM | `formats/src/markdown.rs` | Tables, strikethrough, block math |
| CSV | `formats/src/csv.rs` | Spreadsheet tables |

**Contract:**
- `parse(text) -> Result<Page, FormatError>`
- `serialize(page) -> Result<String, FormatError>`
- Round-trip: `parse(serialize(page)) ≈ page` (content-exact, metadata may differ)

### Import (`crates/import`)

OneNote `.one` section file import.

- `parse_section(bytes) -> Result<NotebookFragment, ImportError>`
- Converts OneNote structures → NoteCraft model
- Tested against real `.one` files in `tests/fixtures/`

### UI (`crates/ui-egui`)

**Sub-modules (current structure):**
- `app.rs` (5k lines) - main window loop, layout, state
- `pageedit.rs` (3k lines) - page content editing
- `ribbon.rs` (2.5k lines) - toolbar/ribbon UI
- `preview.rs` - live preview rendering
- `bidi.rs` (999 lines) - bidirectional (RTL) text layout
- `highlight.rs` (993 lines) - syntax highlighting for code blocks
- `containers.rs` - panel/dialog layout helpers

**Ownership:**
- UI owns the `Session` instance
- Session remains alive for the app's lifetime
- User actions trigger `session.run()` commands
- Output is rendered back to UI

### Specialized (`crates/math`, `crates/diagram`, `crates/ink`, `crates/text`, `crates/input-language`, `crates/mcp`)

Each handles one domain (math typesetting, diagram rendering, handwriting, text utilities, input methods, MCP integration). Low coupling; used by formats or UI as needed.

---

## Data Flow (Example: Create Page)

```
User clicks "New Page" in ribbon
    ↓
UI.command_create_page(request)
    ↓
Session.run("page.create", params)
    ↓
Engine::Commands::page_create()
    ↓
Store::create_page(notebook_path, page_title)
    ↓
File system: write page.html (or index.md)
    ↓
Return Output::Page { ... }
    ↓
UI renders page in editor
```

**Error flow:**
```
Engine::Commands returns EngineError
    ↓
UI catches error, displays in status bar
    ↓
User is notified; operation can be retried
```

---

## State Management

### Session (Engine)

A `Session` represents one open notebook:

```rust
pub struct Session {
  notebook_path: PathBuf,
  current_page_id: Option<String>,
  changes: Vec<Change>,  // Uncommitted changes
  undo_stack: Vec<Change>,
  // ...
}
```

**Invariants:**
- One Session per open notebook
- All mutations logged to `changes` (for undo/redo)
- Writing to disk clears `changes`
- No mutations happen without going through `session.run()`

### UI State (egui)

UI layer holds:
- Window geometry
- Current tab/panel
- Keyboard focus
- Theme preferences

These are **NOT** source of truth for notebook state. Model state lives in Session.

---

## Ownership & Lifetimes

```
App
  ↓
Session (owns notebook path, change log)
  ↓
Notebook (loaded from disk on open; changes tracked in Session)
  ├─ Sections
  │  └─ Pages
  │     ├─ Blocks (text, image, table, code)
  │     └─ Metadata (created_at, format, etc.)
  └─ Metadata
```

**Key rule:** After `Session` owns a path, all mutations go through `Session::run()`. Don't read from disk directly (stale data).

---

## Cross-Cutting Concerns

### 1. Error Handling

**Principle:** Use `Result<T, E>` for recoverable errors. Structure errors by domain.

```rust
enum EngineError {
  NotebookNotFound,
  PageNotFound,
  InvalidCommand { cmd: String },
  StoreError(StoreError),
  FormatError(FormatError),
}

enum StoreError {
  PermissionDenied(PathBuf),
  DiskFull,
  Corrupted(String),
}
```

Every command returns `Result<Output, EngineError>`. UI must handle all error variants.

### 2. Logging

**Principle:** Log stage transitions and important events. Use `log::` macros (never `println!`).

```rust
info!("Opening notebook: {}", path.display());
debug!("Creating page: title={}", title);
warn!("Page metadata missing: {}", page_id);
error!("Failed to save page: {}", err);
```

Log level is set at runtime; production can suppress debug messages.

### 3. Performance

**Current known bottlenecks:**
- RTL text layout (expensive per-character calculation)
- Search indexing (quadratic on very large notebooks)
- Syntax highlighting (re-runs on every keystroke)

**Approach:**
- Profile before optimizing
- Cache expensive computations (e.g., layout results)
- Batch operations (e.g., search index updates)
- Document performance assumptions in code

### 4. Concurrency

**Current:** Single-threaded UI main loop + async file I/O.

**No shared mutable state** in the current design (single Session).

If async work is added later, must handle:
- Race between autosave and user save
- Interrupted operations (user closes app while saving)
- Network sync (future work)

### 5. Cross-Platform Behavior

**Current platform-specific code:**
- `crates/ui-egui/bidi.rs` - text layout (all platforms, but RTL-heavy)
- File dialogs (rfd crate; platform-native on each OS)
- Clipboard (arboard crate; platform-native)

**Pattern:** Shared interface, platform-specific implementations when native OS behavior is needed.

---

## Architecture Style: MVC (Not MVVM)

NoteCraft uses **Model-View-Controller** architecture, not MVVM. This choice is deliberate:

### Why MVC
- **Controllers own orchestration:** `Session` is the controller - it routes commands, manages MCP integration, handles test hooks, and owns autosave logic. Simple and explicit.
- **Models are pure:** `Notebook`, `Page`, `Section`, `Block` are pure data types with no I/O or state management. Fully unit-testable.
- **Views are stateless:** UI layer (egui, CLI) responds to controller commands. Window state and theme live in the view; notebook state lives in the model/controller.
- **Rust-friendly:** Rust's ownership model and MVVM's two-way data binding don't mesh well. MVC sidesteps that complexity.

### MVC Boundaries in NoteCraft
```
Model (pure types)
  ↓
Controller (Session: routing, commands, MCP)
  ↓
View (UI rendering: egui, CLI output)
```

### Anti-Patterns
- **Do NOT:** Spread business logic across views. UI should never make Store or Engine decisions.
- **Do NOT:** Put mutable state in the View that affects Model. Window geometry, theme, focus: yes. Notebook state, page edits: goes through Controller.
- **Do NOT:** Use ViewModels to duplicate Model state. If you're tempted, you need a controller boundary instead.

---

## Design Patterns Used

### 1. Command Pattern
Every user action is a named command:
- `"page.create"` → creates a page
- `"page.delete"` → deletes a page
- All commands are testable, scriptable

### 2. Builder Pattern
For complex types with optional fields:
```rust
let page = PageBuilder::new("Title")
  .set_format(PageFormat::Html)
  .set_parent(Some(parent_id))
  .build()?;
```

### 3. Visitor Pattern (Limited)
Format serialization uses block visiting:
```rust
for block in page.blocks.iter() {
  match block {
    Block::Text(span) => /* ... */
    Block::Image(src) => /* ... */
  }
}
```

### 4. Strategy Pattern
Different serializers for different formats:
```rust
let serializer: Box<dyn Serializer> = match page.format {
  PageFormat::Html => Box::new(HtmlSerializer),
  PageFormat::Cmgfm => Box::new(MarkdownSerializer),
};
let text = serializer.serialize(&page)?;
```

---

## Module Boundaries & Imports

**Allowed imports:**
```
Store ← Model  ✓
Engine ← Store, Model  ✓
Format ← Model  ✓
UI ← Engine, Model, Format  ✓
```

**Forbidden imports:**
```
Model ← anything else  ✗ (keep pure)
Store ← Engine  ✗ (would create cycle)
Format ← Store  ✗ (would create cycle)
```

---

## Data Persistence

### Anchored embedded objects

Inline images and other embedded objects are document content, not UI-only overlays. The model
must preserve an object anchor that identifies its owning text block and inline position. A
floating object may also carry page-space position, size, and stacking order. Rendering and hit
testing are derived from this model, so a reload produces the same selection and placement.

The interaction contract is documented in `work/inline-images-and-anchors.md`. Changes to the
object representation must preserve HTML and CMGFM semantic parity and include migration or a
backward-compatible read path for existing pages.

### Corruption recovery boundary

Corruption recovery is an architectural concern only at the ownership-boundary level:

- Model and format layers represent diagnostics and recovered content.
- Store owns preservation and atomic file writes.
- Engine coordinates the load and repair commands.
- View presents the recovery state.
- Controller applies the user's repair, no-repair, or cancel decision.

The detailed product behavior and UI workflow belong in `work/corruption-recovery.md` and the
appropriate UI/UX skills. Do not put dialog copy, button layout, or visual behavior in this skill.

### Notebook Structure

```
~/MyNotebook/                 # Notebook root
├── .notecraft/
│   ├── notebook.json         # Metadata (version, created_at, etc.)
│   └── search_index.db       # (optional) full-text search cache
├── Section1/
│   ├── Page1.html
│   ├── Page1_assets/
│   │   ├── image.png
│   │   └── embedded.pdf
│   ├── Page2.html
│   └── Subsection/
│       └── Subpage.html
└── Section2/
    ├── index.md              # CommonMark format
    └── index_assets/
```

**Invariants:**
- One notebook = one folder tree
- One page = one `.html` or `index.md` file
- Assets in sibling `_assets` folder
- Metadata in `.notecraft/notebook.json`

### Metadata Format

```json
{
  "version": "1",
  "created_at": "2026-10-24T15:30:00Z",
  "format": "html",
  "maybe_parent_page_id": null
}
```

**Versioning:** If format changes, increment version and provide migration code.

---

## Testing Strategy

### Unit Tests (Co-located)
```rust
#[cfg(test)]
mod tests {
  // Test individual functions, small modules
}
```

### Integration Tests (separate)
```
tests/
├── fixtures/notebooks/sample.nb/   # Real test data
├── common/builders.rs              # Test helpers
└── integration_tests.rs            # Multi-crate tests
```

### Property-Based Tests
Consider `proptest` for format round-trips:
```rust
#[test]
fn html_round_trip() {
  // parse(serialize(page)) ≈ page (idempotent)
}
```

---

## Known Limitations & Future Work

### Current Limitations
- Single notebook at a time (no multi-notebook workspaces yet)
- No network sync (user's sync tool handles it)
- OneNote import is one-way only (no export)
- No real-time collaboration

### Future Extensions (Not committed)
- Multiple open notebooks
- Cloud sync backends
- Export to OneNote, Markdown, PDF
- Collaborative editing
- Plugin system

### Do NOT assume these features exist until they're designed & built.

---

## WIP Sections (To Be Expanded)

- [ ] Detailed API specifications for each crate
- [ ] Sequence diagrams for complex workflows
- [ ] Performance budgets (RTL layout time, search latency, etc.)
- [ ] Security model (file permissions, data isolation)
- [ ] Versioning and migration strategy
- [ ] Deployment and release process

---

## References & Links

- [Code Quality Audit](C:\dev\p-NoteCraft\work\code-quality\implementation-audit.md)
- [Tidy-Up Tasks](C:\dev\p-NoteCraft\work\code-quality\tidy-up-tasks.md)
- [Product Principles](nc-prod)
- [High-Level Design](nc-hl-design)
- [Rust Code Standards](nc-code-rust)

---

**Last updated:** 2026-10-24  
**Status:** Actively evolving with implementation  
**Next review:** After first major feature completion

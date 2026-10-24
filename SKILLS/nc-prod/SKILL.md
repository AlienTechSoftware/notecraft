---
name: nc-prod
description: 'Critical product principles and non-negotiable requirements for NoteCraft. Local file persistence, DOM/format serialization parity, OneNote look/feel, friction-free import. Reference when evaluating features, making design decisions, or reviewing changes. Used alongside nc-arch and nc-hl-design.'
license: MIT
metadata:
  version: "0.0.20261024"
  decision_date: "2026-10-17"
  approved_by: "Product Owner"
  related_skills: [nc-arch, nc-hl-design]
---

# nc-prod - NoteCraft Product Principles

## Runtime profiles and local-only diagnostics

There are three supported runtime environments: `debug`, `release`, and `production`. There are
five user-visible local logging levels: `None`, `Usage`, `Warning`, `Error`, and `Debug`. Debug
builds default to `Debug`; release and production default to `None` unless the user chooses a
level. Logs and usage telemetry remain on the user's device. NoteCraft does not send them
automatically. Settings belong in the platform configuration directory (`%APPDATA%\\NoteCraft`
on Windows), while session data belongs in its local telemetry subfolder.

**These are non-negotiable requirements set by the product owner (2026-10-17).** Every feature, design decision, and code change must respect these principles or get explicit written approval to deviate.

---

## Core Principles

### 1. Local File Persistence (Hard Requirement)

**Principle:** Notebooks live as plain files in plain folders that the user controls.

**What this means:**
- One notebook = one folder on the user's disk
- No server, no cloud account required
- No local database (SQLite, RocksDB, etc.)
- User can open the notebook folder in Finder/Explorer
- User can copy/sync the folder with `rsync`, `git`, OneDrive, Google Drive, Dropbox, etc.

**What this enables:**
- Cloud sync: User picks their own sync tool (already have sync? just use it)
- Backup: User copies the folder
- Version control: User runs `git` on the notebook folder
- Openness: User can read/edit files with other tools if needed

**Constraints (must enforce):**
- No binary formats (use human-readable formats: JSON, HTML, Markdown)
- No single-file database
- No server-side state that doesn't exist locally
- No forced vendor lock-in

#### Corruption tolerance and user-approved repair

Opening a damaged page must never silently replace or discard the user's source file. NoteCraft
should make a best-effort read when safe, identify what was recovered and what was dropped, and
ask the user before writing any repair. Before an approved repair, preserve the original in the
page's recoverable version history or an explicit backup. If the user denies repair, open the
page read-only or with the safe recovered view and record an error-level local diagnostic. A
repair must be atomic and must leave the original recoverable if interrupted.

**Test:** A notebook should be readable by opening the folder with `ls` and `cat`. Content should be understandable as plain text or XML/JSON.

---

### 2. Format Serialization Parity (Dual Format Support)

**Principle:** Notebooks support two page formats, and they MUST remain in semantic and visual parity.

**The requirement (from product owner):**

> The bundled demo's `html` and `cmgfm` folders MUST remain identical in:
> - Notebook structure (same sections, pages, hierarchy)
> - Content (same text, headings, tables)
> - Assets (same images, embedded files)
> - Metadata (same created_at, format declaration)
> - Links (same cross-references, relative paths)
>
> The ONLY permitted difference is the page file format (`.html` vs `index.md`).
>
> At the same viewport, zoom, theme, and app settings, both versions MUST render nearly pixel-for-pixel alike.

**What this means:**
- HTML format: page content in `.html` file, assets in `_assets/` folder
- CommonMark+GFM (CMGFM): page content in `index.md`, assets in `_assets/` folder
- Both formats support the same feature set (tables, code, images, math, etc.)
- When you update demo content, **regenerate both outputs together**
- Visual comparison required: side-by-side screenshots at 1x zoom, light theme, standard settings

**Examples of parity:**
```
HTML:              CommonMark:
<h1>Title</h1>     # Title
<p>Text</p>        
                   Text

<table>            | Col1 | Col2 |
<tr>               |------|------|
<td>A</td>         | A    | B    |

[link](path.html)  [link](path.md)
```

**Anti-examples (violations):**
```
✗ HTML supports images; Markdown doesn't (breaks parity)
✗ HTML renders nested lists; Markdown shows them flat (breaks parity)
✗ HTML page is "feature complete"; Markdown version is stripped-down (breaks parity)
✗ Demo HTML has 5 sample notebooks; demo Markdown has 3 (breaks parity)
```

**Consequences:**
- Format conversion must be lossless (or lossy in both directions equally)
- Feature additions require updating both formats
- Demo updates must keep both in sync

**Test:** Run a diff between `docs/demo/DemoNotebook/html` and `docs/demo/DemoNotebook/cmgfm`. The only differences should be file names and format tags.

---

### 3. OneNote Compatibility & Friction-Free Import (Product Requirement)

**Principle:** Importing an OneNote notebook should be effortless and high-fidelity.

**What "friction-free" means:**
- User exports a section from OneNote (or provides a `.one` file)
- One click: "Import OneNote"
- Pages appear in NoteCraft with minimal information loss
- User can immediately read and edit the imported content

**What "high-fidelity" covers:**
- Text formatting (bold, italic, underline, color, highlights)
- Lists (bullets, numbering, nesting)
- Tables (headers, merged cells, borders)
- Images and embedded files
- Code blocks and syntax hints
- Math equations (convert to LaTeX)
- Headings and hierarchy

**What's acceptable to drop (scope of import):**
- Exact OneNote styles (colors, fonts) → use theme/CSS instead
- OneNote-specific metadata (tags, categories) → can be added later
- Handwritten ink → not required for initial version
- Audio/video embeds → can link instead of embed

**What's NOT acceptable to drop:**
- Content loss (missing paragraphs, tables truncated)
- Hierarchy collapse (all pages flat instead of nested)
- Unreadable output (garbled text, encoding errors)

**Constraints:**
- Import command exists in CLI and UI
- Error messages are clear if something fails
- User can see a preview before committing to import
- Partial imports work (skip unrecognizable content, keep the rest)

**Test:** Take 10 diverse real OneNote notebooks (from various users). Import each into NoteCraft. Check:
- No crash or hang
- ≥85% of text content preserved
- Tables appear as tables (not flattened)
- Images load
- Hierarchy preserved
- User can open and edit each page

---

### 4. OneNote Look and Feel (Visual & Behavioral Familiarity)

**Principle:** NoteCraft should feel familiar to OneNote users, not foreign.

**Visual elements:**
- Ribbon/toolbar at top (like OneNote)
- Notebook tree on left (like OneNote)
- Page content in center (like OneNote)
- Properties panel on right (like OneNote)
- Compact, clean design (like OneNote)

**Behavior:**
- Click to edit (not "edit mode" button)
- Autosave as you type
- Navigation history (back/forward)
- Find and replace (Ctrl+F, Ctrl+H)
- Undo/redo (Ctrl+Z, Ctrl+Y)
- Drag-drop files to insert images
- Right-click context menus

**What's NOT required:**
- Exact pixel-for-pixel OneNote UI
- OneNote's color scheme (can use custom theme)
- All OneNote keyboard shortcuts (but common ones must work)
- OneNote's cloud features (we're local-first)

**Outcome:** A OneNote user should be able to pick up NoteCraft and navigate without reading a manual.

**Test:** Usability test with a OneNote power user. Can they:
1. Open a notebook from disk? ✓
2. Create a new page? ✓
3. Format text (bold, tables)? ✓
4. Search for content? ✓
5. Organize pages in hierarchy? ✓

---

### 5. RTL & Caret Excellence (Design Decision: 2026-10-19)

**Principle:** NoteCraft's RTL (right-to-left) and caret behavior is best-in-class.

**The challenge:**
- Arabic, Hebrew, Persian, Urdu scripts read right-to-left
- Mixed-direction text (English + Arabic) is complex
- Caret placement is non-obvious (visual vs. logical positions)
- Most note-taking apps (OneNote, Notion, Obsidian) get this wrong

**The requirement:**
> The caret and the right-to-left experience must be the best of any note taking application. Not "good enough for Arabic": better than OneNote, Obsidian, Notion, and Evernote at writing in Arabic, Hebrew, Persian and Urdu, and at putting the caret exactly where a writer means in text of either direction or both. The measure is a writer of those scripts preferring NoteCraft for that reason alone.

**What this means:**
- Caret always appears exactly where the user expects it
- Typing in RTL text is smooth (no jumps, no unexpected selections)
- Mixed-direction text (e.g., "مرحبا hello") is handled correctly
- Paste and undo work intuitively in RTL context
- No platform-specific weirdness (Windows ≠ macOS ≠ Linux behavior)

**Implementation notes:**
- `bidi.rs` handles bidirectional layout
- Caret position is a **logical offset** (index in the string), never visual
- Visual layout is computed from logical positions
- RTL text always lays right-to-left; no surprises

**Test:** RTL expert writes a paragraph in Arabic, edits it, undoes, pastes, and checks that:
1. Caret is always where they expect
2. Selections highlight correctly
3. Text doesn't jump unexpectedly
4. Undo/redo preserves text correctly
5. Behavior is identical on Windows, macOS, Linux

**Reference:** [work/rtl-and-caret-bar.md](C:\dev\p-NoteCraft\work\rtl-and-caret-bar.md) (detailed tracking of this requirement)

---

## Non-Negotiable Constraints

### No Server, No Authentication

- NoteCraft is a desktop app
- Notebooks are local files
- No login required
- No telemetry / usage tracking (user's data stays private)

**Exception:** MCP (Model Context Protocol) integration may connect to Claude or other services, but only with explicit user action (opt-in), and only for the current page/session.

### No Paid Features

- NoteCraft is open-source and free
- All features available to all users
- No "pro" tier, no feature gating

**Future:** Potentially optional paid services (cloud backup, sync), but core app is always free.

### Cross-Platform (Windows, macOS, Linux)

- NoteCraft runs on all three platforms
- Feature parity across platforms (no "Windows only" features)
- Platform-native features OK (file dialogs, clipboard) but fallback on all platforms

### No Vendor Lock-In

- User can export their notebook as HTML + Markdown + images (manual copy, or export command)
- Notebook format is future-proof (plain files, no binary blobs)
- User can switch to a different app without losing content

---

## What This Means for Development

### When Designing a Feature

**Ask these questions:**

1. **Persistence:** Does this require a server or local database?
   - If yes: reconsider. Local files only.
   
2. **Formats:** Does this work in both HTML and CommonMark?
   - If no: add it to both, or don't add it at all.
   
3. **Import:** Could this be imported from OneNote?
   - If yes: design import support now, not later.
   
4. **RTL:** Does this work correctly with right-to-left text?
   - If no: test with Arabic/Hebrew and fix before shipping.
   
5. **Cross-platform:** Does this work on Windows, macOS, and Linux?
   - If no: at least provide a fallback.

### When Reviewing a PR

**Check:**
- [ ] No new server dependencies
- [ ] Notebook format still works locally (not cloud-backed)
- [ ] Both HTML and CommonMark formats supported (if applicable)
- [ ] RTL text tested (even if feature isn't RTL-specific)
- [ ] Works on all three platforms (or fallback provided)
- [ ] Code can be merged and shipped (no "TODO: deploy server" comments)

### When Planning a Release

**Before shipping:**
- [ ] Demo notebooks in both HTML and CommonMark are in parity
- [ ] OneNote import test with real `.one` files passes
- [ ] RTL text editing works smoothly
- [ ] No new dependencies on cloud services
- [ ] Users can still open notebooks as plain folders

---

## Priority Hierarchy

If you have to choose, prioritize in this order:

1. **Local file persistence** - If we lose this, we're not NoteCraft
2. **Format parity** - If formats diverge, we're not credible
3. **OneNote import** - If import is broken, users can't get started
4. **RTL excellence** - If RTL is broken, we're discriminating against a billion people
5. **Cross-platform** - If Windows breaks, we've lost a large user base
6. **Look and feel** - If the UI is ugly, users might switch, but content is safe

**Translation:**
- Never trade away persistence for performance
- Never add a feature to one format only
- Never break OneNote import for convenience
- Never ignore RTL bugs ("works for English" is not sufficient)
- Never assume "we'll support Linux later" (don't break it now)
- Polish matters, but content > chrome

---

## Product Owner Decision Log

**2026-10-17:**
- Language: Rust only
- Storage: Local files, no database
- Formats: HTML (preferred) and CommonMark (CMGFM)
- Format configurability: page, section, notebook, global
- Hierarchy depth: unlimited (better than OneNote's 3-level limit)
- Import: OneNote `.one` sections (source: public documentation only)

**2026-10-19:**
- RTL/Caret requirement: best-in-class, not "good enough"

**Future decisions pending:**
- Multi-notebook workspaces?
- Network sync (or remains user's responsibility)?
- Export formats (PDF, Word, OneNote)?
- Collaborative editing?
- Plugin system?

---

## How to Use This Skill

**In design phase:**
- Use `/nc-hl-design` to sketch the feature
- During design, check this skill: "Does this violate any principles?"

**In code review:**
- Reference specific principles ("This breaks format parity")
- Require justification if a principle is being bent

**In planning:**
- When prioritizing features, check the hierarchy (persistence > features)
- If a feature compromises a principle, escalate for approval

**When tempted to take a shortcut:**
- "We'll add OneNote export later" → No. Design import now so export can follow.
- "We don't need to support Linux" → We do. Design for cross-platform from start.
- "Let's use a database for speed" → No. Local files only.

---

## References

- [AGENTS.md](C:\dev\p-NoteCraft\AGENTS.md) - Full project contract
- [JOURNAL.md](C:\dev\p-NoteCraft\JOURNAL.md) - Design decisions over time
- [RTL & Caret Bar](C:\dev\p-NoteCraft\work\rtl-and-caret-bar.md) - RTL implementation tracking
- [OneNote Importer Plan](C:\dev\p-NoteCraft\work\onenote-importer-plan.md) - Import design and progress

---

**Owner:** Product Owner  
**Decision Date:** 2026-10-17 (update 2026-10-19)  
**Last Reviewed:** 2026-10-24  
**Next Review:** Quarterly or when major features are proposed

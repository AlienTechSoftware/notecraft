# RFC 0002: Edit and render modes for page objects (interim)

| Field | Value |
|---|---|
| RFC | 0002 |
| Title | Edit and render modes for page objects (interim) |
| Status | Draft, for owner review. Not accepted. Stopgap until RFC 0001 is accepted. |
| Version of this RFC | `modes-rfc 0.1-draft` |
| Versioning | Independent of the app version. This RFC's version changes only when this document changes. |
| Owner | The NoteCraft owner, with `nc-dom-ser` as the working skill |
| Affects | `crates/formats` (`render.rs`, `markdown.rs`), `crates/model`, `crates/ui-egui` (page editor: `pageedit.rs`, `preview.rs`, `app.rs`), `docs/dom_serialization/`, `SKILLS/nc-dom-ser/references/object-types.md` |
| Related | RFC 0001, `object-types.md`, `caret-options.md`, `code-blocks.md`, `page-model.md`, issue #242, issue #60 |
| Superseded by | RFC 0001, on its acceptance, for every question this RFC defers (section 9) |

---

## 1. Summary

Some page objects have two faces: an edit form and a rendered form. The model
declares no modes, so the switch between the two is inferred in the user
interface from the caret position. Cursor movement cannot tell whether the
caret is in stream text or inside an object's editor, so movement guesses.
This is one cause of the caret confusion reported in issue #242.

RFC 0001 will decide the full document object model. It is a large list and
is not yet accepted. This RFC is a partial, pragmatic clarification that
decides one question now:

1. The rule that separates content with no modes (the text stream) from
   objects with declared edit and render modes (section 3).
2. The set of content that has no modes today, whose classification is
   deferred to RFC 0001 (section 4).
3. The set of objects whose modes are wired into the DOM now (section 5).
4. The Editor object, in the controller layer, that owns mode transitions
   and caret handoff (section 7).

This RFC changes no file format. Modes are not document content and are not
stored in the page file. On RFC 0001 acceptance, sections 3-5 are absorbed
into RFC 0001 or its successor, and this document is withdrawn.

## 2. Terms

- **Stream**: paragraph text edited as text: characters with marks, one
  caret, one selection, typing, undo, redo. Paragraphs, headings, quotes
  and list items are stream content.
- **Object**: content that is not stream text. An object has a kind
  (`object-types.md`) and, where this RFC requires it, declared modes.
- **Inline object**: an object that sits in a paragraph's text as one
  character position (RFC 0001, section 2).
- **Block object**: an object that sits in the body as a block of its own
  (RFC 0001, section 2).
- **Edit mode**: the form in which an object is changed. **Render mode**:
  the form in which it is shown when not being changed. A mode is a property
  of the object's kind, declared in the DOM. It is not a property of the
  caret.
- **Editor**: the controller object that owns mode transitions and caret
  handoff for objects with declared modes (section 7).
- **Mode declaration**: the statement, in an object's schema, of which modes
  the kind has and what enters and leaves each mode (section 6).

## 3. The rule

1. Content is stream content when the text editor and the text renderer are
   sufficient: it flows and wraps with the words, takes the caret,
   selection, typing, undo and redo like surrounding text, and carries no
   state beyond its marks.
2. Content is an object with declared modes when its edit form differs from
   its rendered form: it needs an editor or a renderer of its own.
3. The test is the mode, not the size or the family. A small object with two
   faces has modes; a large object with one face does not.
4. Where the test is unclear, the content is stream content until the owner
   rules otherwise. That is the deferral in section 4.

## 4. Content with no modes today (deferred)

| Content | Why it has no modes | Status |
|---|---|---|
| Paragraph, heading, quote, list item, rule | Edited as stream text | Deferred to RFC 0001 |
| Marks: bold, italic, underline, strikethrough, font family, font size, colour | Marks on characters; no separate editor | Deferred to RFC 0001 (see issue #60) |
| Links | A run with a target; no separate editor | Deferred to RFC 0001 |
| Line break, tab | Characters in the stream | Deferred to RFC 0001 |
| Table | Cannot be edited in place today (`page-model.md`, section 3; `gaps.md`); no edit mode exists to declare | Deferred; when in-place table editing is built, the table enters section 5 by amendment |
| Picture (block image) | Selected and resized; no text edit mode | Deferred; resize remains controller behaviour (`containers.rs`) |
| Ink | Drawn on a canvas, not edited as text; own undo; stored as files (`page-model.md`, sections 3, 5) | Out of scope for this RFC; its interaction states are unchanged |

Rules for deferred content:

1. Deferred content is stream content for the purposes of this RFC. No code
   may infer a mode for it.
2. Deferral is not a classification. RFC 0001 decides the final
   classification of every row above.

## 5. Objects with declared modes (wired in now)

The set is the source-versus-rendered family: every object whose edit form
differs from its rendered form today. Each already switches between the two
in the user interface without a declaration; this section makes the switch
official.

| Kind | Edit mode | Render mode | Enters edit mode | Leaves edit mode |
|---|---|---|---|---|
| `block.code` | Source text | Code as written, with language label | Caret enters the block | Caret leaves the block |
| `block.code`, numbered (`123\|<lang>`) | Source text, line numbers hidden | Code with line numbers | Caret enters the block | Caret leaves the block |
| Diagram (mermaid fence) | Source text | Drawn picture | Single click opens the source | Leaving the block |
| `block.math` | TeX source | Typeset mathematics | Caret enters the block | Caret leaves the block |

Notes:

- Numbered code shows its numbers on the page and none while the caret is
  in it, as a diagram shows its picture on the page and its source while it
  is written in (`code-blocks.md`).
- Both modes of a numbered block use the same box, size and language label
  (`preview.rs`, `pageedit.rs`), so the only change on entering is the
  numbers. This RFC requires the same of every kind in the table: entering
  a mode changes only the content shown, never the box.

Rules:

1. Entering an object enters its edit mode. Leaving it returns it to render
   mode.
2. The transition is owned by the Editor (section 7). No view code changes
   a mode on its own.
3. A kind enters this section only by amendment to this RFC.

## 6. Mode declarations in the DOM

1. Each kind in section 5 carries a mode declaration in its schema: the
   modes it has, and the triggers that enter and leave each mode.
2. The declaration is the source of truth. `pageedit.rs` and `preview.rs`
   read it; they do not infer a mode from caret position.
3. Modes are not stored in the page file. They are interface state. The
   owner's 1:1 rule (every element in the DOM has a 1:1 representation in
   the page file) is unaffected: the file holds the object, not its mode.
4. Until the schema registry of RFC 0001 exists, the declaration lives in
   two places: the modes column of `object-types.md`, and the kind's model
   type. On RFC 0001 acceptance, declarations move to the registry
   unchanged in meaning.
5. Adding a mode to a kind, or adding a kind to section 5, is an amendment
   to this RFC.

## 7. The Editor object

NoteCraft uses Model-View-Controller (`nc-arch`). `Session` is the
controller: it routes commands, manages MCP integration, handles test hooks
and owns autosave. The Editor is a controller object beside `Session`, for
object editing.

Responsibilities:

1. Own the active edit session: which object, if any, is in edit mode.
2. Own transitions: enter edit mode, leave to render mode. No other code
   changes an object's mode.
3. Own caret handoff: when the caret enters an object, the stream caret is
   suspended and the object's editor receives it; on leaving, the stream
   caret resumes at the object's boundary position.
4. Route input by mode: keys and clicks go to the object's editor while it
   is active, and to the stream editor otherwise.

Non-responsibilities: the Editor holds no document data (model), draws
nothing (view) and owns no file I/O. It commands; the model and the views
do the work.

The interface between the Editor and `Session` (command routing) is an
implementation matter, reviewed under `nc-design-controller` (see section
12, question 6).

## 8. Cursor movement

1. Movement is routed by the active mode, not by caret position. The Editor
   is consulted before any caret move.
2. Arrow keys at an object's boundary move into the object's edit mode on
   entering, and out of it on leaving, as section 5 defines. They do not
   place a stream caret inside an object's rendered form.
3. Clicks are routed the same way: the Editor decides, from the mode, whether
   a click selects the object, enters its edit mode, or places the caret.
   The per-kind entry triggers are in section 5.
4. While an object is in edit mode, there is one caret, owned by the active
   editor. The stream caret does not exist inside the object.
5. A mode transition is not a history step. Content changes follow the page
   history rules (`page-model.md`, section 3): whole page states, not a
   list of edits.

This section addresses the two-caret problem (`caret-options.md`) for mode
objects only. The general caret model remains the subject of RFC 0001 and
of the Option B spike.

## 9. What this RFC does not decide

- The object tree, the schema registry, and the full kind list: RFC 0001.
- The caret model and layout: `caret-options.md` (Option B spike pending
  owner approval).
- The classification of the deferred set (section 4), including: equations
  as inline objects (issue #100), fields and the date line (issue #242),
  to-do tags, inline images, links by id.
- Style, attributes and formats content: deferred by RFC 0001, sections
  3.2 and 5.1.
- Whether a picker counts as an edit mode (fields).
- Syntax colouring and line wrapping in code blocks (`code-blocks.md`:
  not decided).
- Ink storage and ink undo (`page-model.md`, section 8).

## 10. Consequences

- Behaviour: unchanged for code, numbered code, diagrams and mathematics.
  The change is that the modes are declared and owned, not inferred.
- File format: unchanged. No migration. Round-trip and format-parity tests
  are unaffected.
- Code: `pageedit.rs` and `preview.rs` read mode declarations; mode
  inference from caret position is removed for the section 5 kinds. The
  Editor is new code in the controller layer.
- Issue #242: this RFC does not fix #242. It removes one cause of caret
  confusion (mode guessing for objects) and defines the boundary the #242
  fix builds on: stream content is edited as runs; objects are edited in
  their modes.
- Issue #60: font-formatting scope remains open. Its marks are stream
  content under section 4, so its scope decision is independent of this
  RFC.

## 11. Testing

1. Each rule in sections 3-8 has a test the build runs (following RFC 0001,
   section 3.1, goal 7).
2. Mode declaration tests: each section 5 kind declares its modes; a test
   fails if a kind's code and its declaration disagree.
3. Transition tests: entering and leaving each mode, including caret
   handoff at both boundaries.
4. Cursor routing tests: arrow keys and clicks at object boundaries, in
   both modes.
5. Existing round-trip and format-parity tests continue to pass unchanged.

## 12. Open questions

1. Whether a picker counts as an edit mode (fields, date line).
2. Whether a to-do tag's tick state makes it an inline object, or stream
   content with state.
3. Whether a picture in a sentence flows inline, or every picture is a
   block object.
4. Whether a visual equation editor (issue #100) makes equations inline
   objects, and what that means for the inline-object category.
5. Whether math block entry follows the code-block rule (caret enters), to
   be confirmed against current behaviour.
6. The interface between the Editor and `Session`, for
   `nc-design-controller` review.

## 13. Changelog

| RFC version | Date | Change |
|---|---|---|
| 0.1-draft | 2026-11-14 | First draft. |

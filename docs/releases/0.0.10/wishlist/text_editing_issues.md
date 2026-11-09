# Text Editing Issues for 0.0.10

## TE-001: Click on empty or newly created block with text does not place caret at last character

**Date:** 2026-11-08  
**Severity:** HIGH  
**Confidence:** 8/10  

### Description

When the user clicks on an empty block or a newly created block that already has text, the caret does not land at the last character position. Instead it may jump to the start of the block or to an incorrect position within the text.

### Steps to reproduce

1. Open a page in edit mode.
2. Click below the existing content so that a new empty block is created (or click on an existing block that already contains text).
3. Observe where the caret lands relative to the visual content.

### Expected behaviour

The caret should land at the end of the text (i.e. `c.text.chars().count()`) when the block is empty, or at the character under the pointer when the block has text and the click falls within the visual bounds of that text. This matches OneNote's behaviour: clicking in a line puts the cursor where you clicked, and clicking past the last word puts it at the end.

### Actual behaviour

The caret may land at position 0 (the start) or at an incorrect byte offset when:
- The block is empty (no text to hit).
- The click falls in whitespace after the last visual character.

### Root cause analysis

In `crates/ui-egui/src/pageedit.rs`, the focus path (line ~2051) correctly sets the initial caret position:

```rust
let at = c.place.take().unwrap_or_else(|| c.text.chars().count()).min(c.text.chars().count());
c.selection = (at, at);
```

But then the click handler (line ~2091) recalculates the position via `crate::bidi::hit()`:

```rust
} else if resp.clicked() && !resp.dragged() {
    at = crate::bidi::hit(&c.text, point, size, width, page);
    anchor = at;
    resp.request_focus();
}
```

`bidi::hit()` (in `crates/ui-egui/src/bidi.rs:741`) measures the pointer against the laid-out text using HarfBuzz. When the block is empty or the click falls past the last visual character, there is nothing to hit at that position, so the function returns 0 or an incorrect index. There is no clamping to `c.text.len()` when the pointer is outside the visual content area.

### Affected code

- `crates/ui-egui/src/pageedit.rs` - line ~2091 (click handler in the text box draw path)
- `crates/ui-egui/src/bidi.rs` - `pub fn hit()` at line 741

### Proposed fix

In the click handler in `pageedit.rs`, clamp the result of `bidi::hit()` to the text length when the pointer falls outside the visual content:

```rust
} else if resp.clicked() && !resp.dragged() {
    let hit = crate::bidi::hit(&c.text, point, size, width, page);
    at = hit.min(c.text.len());
    anchor = at;
    resp.request_focus();
}
```

Additionally, for the empty-block case specifically, if `c.text.is_empty()` and the click is in the block's rect, use `c.text.chars().count()` (which is 0) but ensure the caret still appears at the correct visual position. This may also need a check in `bidi::hit()` to return a sensible default when the text is empty.

### Test plan

1. Click on an empty block below existing content - caret should appear at the start of the new block (position 0, which is correct for empty text).
2. Click on a block with text, within the visual bounds - caret should land at the character under the pointer.
3. Click on a block with text, past the last visual character - caret should clamp to the end of the text.
4. Repeat for RTL text (Arabic) to ensure the fix does not break bidirectional layout.
5. Regression: verify that clicking within a line still places the caret correctly at the clicked position.

## TE-002: Model → View round-trip inconsistencies across HTML editor switch

**Date:** 2026-11-09  
**Severity:** HIGH  
**Confidence:** 8/10  
**Related issues:** [#55](https://github.com/AlienTechSoftware/notecraft/issues/55)  

### Description

There is a systemic inconsistency between the source (DOM model) and the view (edit renderer) when pages are composed after coming from different sources -- specifically when the user edits content in HTML/code view, then switches back to regular edit mode. Two observable symptoms appear:

1. **Raw HTML tags visible in regular edit mode.** Inline markup such as
   ```html
   <span style="font-size: 10pt">Sunday, 25 October 2026, 03:16</span>
   ```
   renders as literal text on the page instead of being parsed and displayed as formatted content.

2. **Editing becomes disabled after an HTML edit round-trip.** As reported in [#55](https://github.com/AlienTechSoftware/notecraft/issues/55), going to the HTML editor, making a modification, and returning to regular edit mode leaves the page uneditable -- the cursor is effectively dead on the page.

Both symptoms point to the same underlying problem: after an HTML round-trip the DOM model state and the edit surface state diverge.

### Steps to reproduce

**Symptom A (raw tags):**
1. Open a page that contains inline HTML markup (e.g., a date-stamp span from a DOCX import).
2. Switch to regular edit mode.
3. Observe raw `<span style="...">…</span>` displayed as visible text.

**Symptom B (disabled editing):**
1. Open a page in regular edit mode.
2. Switch to the HTML/code editor and modify some content.
3. Switch back to regular edit mode.
4. Try to type or place the cursor -- nothing responds.

### Expected behaviour

- In regular edit mode, inline HTML should be parsed into its constituent text nodes with styles applied; no raw markup symbols should appear on screen.
- After editing in HTML view and switching back, the page should remain fully editable with a functional cursor and caret placement. The DOM model and the UI surface must stay in sync.

### Actual behaviour

- Raw HTML source appears as visible characters in regular edit output.
- Editing surface freezes after an HTML → edit round-trip; no text input or cursor positioning works until the page is reloaded or a full reload occurs.

### Root cause analysis

Both issues share the same structural root: **the model ↔ view bridge is inconsistent across code-view and edit-view paths.**

Two likely failure modes in the codebase:

1. **DOM deserializer does not emit editable nodes for inline spans.** When HTML containing `<span>` elements is parsed, the serializer either leaves them as non-editable element nodes (causing raw tag display) or drops their text content entirely, leaving the render path with nothing to hit-test against (`bidi::hit()` returns 0).

2. **Edit surface is not reattached after a code-view round-trip.** The edit renderer holds an owned text buffer and IME/hardware input state. When HTML/code view modifies the DOM directly (skipping the usual focus path), the edit surface retains stale references -- the `raw_content` buffer, focus flags, or widget state are no longer in sync with the rehydrated DOM, so all input is silently dropped.

### Affected code

- `crates/notecraft-formats/` - HTML page model parsing / DOM deserialization (symptom A)
- `crates/ui-egui/src/pageedit.rs` - edit surface, focus path, and raw-content sync (both symptoms)
- `crates/ui-egui/src/bidi.rs` - `pub fn hit()` at line 741 (symptom A: nothing to hit for unparsed nodes)
- `crates/notecraft-dom/` - DOM node types and rendering traits for inline spans

### Proposed fix

1. Ensure every HTML inline element deserializes into an **editable** text fragment with its style attributes resolved on the model side, not deferred to the renderer.
2. On switching from HTML/code view back to edit mode, re-hydrate the edit surface:
   - Replace the raw-content buffer from the current DOM (`dom.as_text()`) so it matches the model.
   - Reset focus flags and request new input focus (`request_focus()`).
   - Clear any stale widget state that prevents input capture.
3. Add a test that does an HTML → edit round-trip and asserts (a) no raw `<` / `>` in visual output, and (b) the cursor responds to typing immediately after the switch.

### Test plan

1. Reopen the `Chapter 1 25 a.docx` source or an equivalent import that produces date-stamp spans.
2. Regenerate the notebook, open it in NoteCraft, and switch to regular edit mode -- verify the span text renders cleanly with no `<span>` markers.
3. repeat the workflow from issue #55: edit in HTML view, switch back -- verify cursor is responsive and typing works immediately.
4. Add a DOM round-trip test that asserts inline spans are serialized as editable nodes (not raw element wrappers).
5. Verify code view still shows the original HTML source as-is (code view is expected to display raw markup).

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

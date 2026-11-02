# Block hit testing and initial sizing (issue - blocks)

- Date: 2026-10-31
- Status: Partially addressed; remaining sizing review deferred
- Branch: `feat/v0.0.8`

Two UX improvements for page editing:

1. New blocks on empty pages should have reasonable initial height
2. Clicking inside a block should activate it and place cursor correctly

---

## 1. Problem: Initial Block Size Too Small

When creating the first block on a new empty page, the default container width and height are minimal. A user expects to start typing and have reasonable space (roughly 15 lines of text).

**Current behavior:**
- New box appears very small (120pt minimum width, minimal height)
- User must drag to resize before typing
- Friction in the "start a new page" flow

**Expected behavior:**
- New block on empty page should be ~500-600pt wide
- Height should accommodate ~15 lines of text (roughly 300-400pt)
- Cursor ready to type immediately

---

## 2. Problem: Click Placement in Block

When clicking on a page that has multiple blocks, the hit detection and cursor placement doesn't work smoothly.

**Current behavior:**
- Clicking on a block might not activate it
- Cursor placement is imprecise (may not land on the clicked line)
- User must click multiple times to get caret in the right place

**Expected behavior:**
- Click anywhere inside a block's boundaries → that block activates
- Cursor placed at the clicked position in the text
- If click is above/below text, cursor goes to line start/end
- Single click = ready to type

---

## 3. Implementation Notes

### 3.1 Initial Block Sizing

File: `crates/ui-egui/src/containers.rs` function `start_a_box`

Current:
```rust
Container::new(at.x, at.y, 120.0, 1)  // MIN_WIDTH
```

Should be:
```rust
let initial_width = 500.0;  // Reasonable text box width
let initial_height = 15 * 16.0;  // ~15 lines at 16pt
Container::new(at.x, at.y, initial_width, 1)
```

Note: Container only stores x, y, width, and block count. Height is derived from content in the view layer. So initial sizing is just width; height grows with content.

### 3.2 Click Placement in Block

File: `crates/ui-egui/src/containers.rs` function `show`

Current hit detection:
- Only checks `frame` (container boundary)
- Only checks if pointer is in container as a whole

Needed:
- Check if pointer is inside a block's text area
- Calculate which line was clicked (rough estimation from y position)
- Call cursor placement logic with the clicked position
- Return `(block_idx, line_position, char_position)` or similar

Related code in `crates/ui-egui/src/preview.rs`:
- Line rendering calculates positions
- Could use similar logic for hit detection

---

## 4. Test Scenario

1. Create new notebook page (empty)
2. Click on the paper → block created
   - ✅ Block is ~500pt wide, ~15 lines tall
   - ✅ Cursor visible and ready to type
3. Type some text, create second block
4. Click on first block (in the middle of some text)
   - ✅ First block activates (selection visible)
   - ✅ Cursor lands on/near the clicked line
   - ✅ Type immediately works
5. Click on second block
   - ✅ Second block activates
   - ✅ Cursor placement is reasonable

---

## 5. Acceptance Criteria

- [ ] New block on empty page: width ≥ 500pt
- [ ] New block on empty page: height ≥ 300pt (15 lines)
- [ ] Click on block boundary: block activates
- [ ] Click on text: cursor placed on clicked line
- [ ] Click above text: cursor goes to line start
- [ ] Click below text: cursor goes to line end
- [ ] Single click = ready to type (no double-click needed)

---

## 6. Files to Modify

- `crates/ui-egui/src/containers.rs` - `start_a_box()`, hit detection in `show()`
- `crates/ui-egui/src/preview.rs` - possibly for line hit detection math
- Tests in `crates/ui-egui/tests/ui.rs`

# Gotcha #2: Caret Position ≠ Visual Position

**What:** A caret's byte offset in the logical string is not the same as its pixel position on screen.

## Problem

- In RTL text, caret at offset 0 appears at the RIGHT edge, not left
- In mixed text, the caret position jumps non-linearly
- Simple `px_to_char()` mapping fails in bidi
- Clicking at visual position X doesn't land at character offset X

## Example

```
Text (logical): ا ل س ع ر   1 2 3
Display (visual): 3 2 1   ر ع س ل ا

Click at visual position "3" -> lands at logical offset 6, not offset 0
```

## Why It Matters

- Click not landing where user points is disorienting
- Navigation feels broken when caret jumps
- Selection is impossible if click mapping is wrong

## Solution

**NoteCraft verified in tests:**
- Use parley's layout output, which maps character positions correctly
- Don't use visual x-position as text index (skill rule #1)
- Test specifically: `a_click_inside_a_vowelled_letter_lands_beside_it`

```rust
// Correct approach: parley gives us run positions
let runs = parley_layout.visual_runs();
for run in runs {
    let char_offset = run.logical_start;  // NOT visual x
    let visual_x = run.x;                 // Just for drawing
}
```

## Testing Checklist

- [ ] Click on RTL letter, caret lands correctly
- [ ] Click on mixed-direction line
- [ ] Click on diacritics (should land on letter, not mark)
- [ ] Triple-click selects full paragraph
- [ ] Caret at line start and end correct
- [ ] Home/End keys work in RTL
- [ ] Selection by dragging lands correctly

## References

- NoteCraft test: `crates/ui-egui/tests/rtl_quality.rs::a_click_inside_a_vowelled_letter_lands_beside_it`
- Unicode Bidi: https://www.unicode.org/reports/tr9/ (UAX #9, Rules L1-L4)
- parley layout engine: https://github.com/linebender/parley

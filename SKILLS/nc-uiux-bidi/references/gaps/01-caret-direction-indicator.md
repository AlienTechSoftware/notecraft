# Gap #1: Caret Direction Indicator

**What:** In mixed-direction text, user doesn't know if next character is LTR or RTL.

**Status:** **Nobody does this.** NoteCraft's owner said it's a feature nobody has. This is a competitive differentiator.

## Problem

```
Text on screen: English السعر 123 ريال More

User types at position after "English "
Question: Is next char LTR (English) or RTL (Arabic)?
Caret doesn't indicate.
User guesses wrong, text reorders unexpectedly.
```

In mixed-direction lines, typing direction matters:
- Left arrow moves left in LTR, right in RTL
- Typing continues in the direction of current run
- User can't predict what happens next

## Solution

Show a tiny visual mark after caret indicating next character's direction:

```
Text: English السعر 123
Caret after "English ": [|→] (arrow pointing right = LTR next)

Text: 123 ريال English
Caret after "ريال ": [|←] (arrow pointing left = RTL next)
```

Alternatives:
- Color background: blue=LTR, red=RTL
- Glyph: LTR triangle, RTL triangle
- Subtle line: angle showing direction

## Implementation

```rust
pub fn runs(ui: &Ui, text: &str, pieces: &[Piece], 
            size: f32, max_width: f32, page: Option<Dir>) 
    -> (Vec<Drawn>, Vec2) {
    // ... existing layout ...
    
    // After caret position, add direction indicator
    if let Some(next_run) = parley.next_run(caret_pos) {
        let indicator = match next_run.dir {
            Dir::Ltr => "→",
            Dir::Rtl => "←",
        };
        painter.draw_indicator(caret_x, caret_y, indicator);
    }
}
```

## Testing Checklist

- [ ] Caret shows direction indicator
- [ ] Indicator changes as caret moves
- [ ] Mixed-direction line shows both directions
- [ ] Pure RTL line shows RTL indicator
- [ ] Pure LTR line shows LTR indicator
- [ ] Indicator visible at all zoom levels
- [ ] Indicator position follows caret
- [ ] Works in single-line and multi-line text

## Impact

**HIGH** - This is the "best RTL experience" differentiator for NoteCraft. A writer of Arabic/Hebrew would choose NoteCraft partly for this feature.

## Effort

1-2 days

## References

- NoteCraft owner vision: `AGENTS.md` (2026-10-19)
- NoteCraft code: `crates/ui-egui/src/bidi.rs::runs()`
- parley: next_run() or equivalent method

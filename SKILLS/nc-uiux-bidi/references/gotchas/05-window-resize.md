# Gotcha #5: Resizing the Window Breaks Bidi Text

**What:** Line breaking in RTL text is not the same as LTR. Window resize can cause text to overflow.

## Problem

- RTL line hangs from RIGHT edge (not left)
- If you compute line breaks assuming left-to-right margin, RTL text falls off page
- Reflow on window resize must recalculate ALL lines
- Margins, indents, and padding have different meaning in RTL

## Example

```
LTR (left margin):
[margin]LOREM IPSUM DOLOR SIT AMET

RTL (right margin):
LOREM IPSUM DOLOR SIT AMET[margin]
```

If you hardcode left margin for both, RTL text wraps incorrectly.

## Why It Matters

- Long RTL lines disappear off-screen on resize
- Text becomes unreadable
- User experience breaks during responsive design

## Solution

**NoteCraft:** Uses parley, which handles this correctly.

**Implementation approach:**
```rust
let margin = match page_direction {
    Dir::Ltr => LEFT_MARGIN,
    Dir::Rtl => RIGHT_MARGIN,
};

// parley respects margin and reflows correctly
layout.fit_width(available_width - margin);
```

**Wordcraft:** Documented in layout-parity as working correctly.

## Testing Checklist

- [ ] Long RTL text wraps correctly at window edge
- [ ] Window resize reflows all lines
- [ ] Text doesn't overflow invisible space
- [ ] Margins adjust for page direction
- [ ] Mixed-direction paragraph wraps correctly
- [ ] Nested containers (quotes, tables) handle resize
- [ ] Performance acceptable on large documents

## References

- parley layout engine: https://github.com/linebender/parley
- NoteCraft: `crates/ui-egui/src/bidi.rs` (layout handling)
- Wordcraft: `crates/layout/src/` (line breaking)

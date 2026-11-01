# Gotcha #4: Paragraph Direction ≠ Text Direction ≠ UI Direction

**What:** Three different directional decisions are often confused. They are independent.

## The Three Directions

| Direction | Decided By | Affects | Example |
|-----------|-----------|---------|---------|
| **Paragraph direction** | Explicit mark or first strong char | How text is laid out in container | Page is RTL, but English paragraph reads LTR |
| **Text direction** | First strong character per UAX#9 | Which way chars are reordered | Mixed line `السعر 123 ريال` has mixed direction |
| **UI direction** | App/page setting | Which way panels and controls face | Ribbon on left (LTR) vs right (RTL) |

## Common Confusions

### Case 1: English text on RTL page
```
Page: RTL (explicit mark)
Paragraph: RTL (inherited from page)
Text: LTR (English letters are LTR, override paragraph)
UI: RTL (follows page)

Result: English reads left-to-right inside RTL page layout
```

### Case 2: Mixed-direction paragraph
```
Page: LTR
Paragraph: LTR (explicit mark or default)
Text: MIXED (first Arabic makes it RTL, then English returns to LTR)
UI: LTR (no change)

Result: Bidi algorithm reorders individual runs within line
```

### Case 3: RTL tag in Arabic text
```
Text: السعر TODO: test
First strong char: ا (Arabic, RTL)
Text direction: RTL
Reorder: text reorders but keeps TODO visible

If "TODO" was alone: first strong is T (Latin, LTR), would flip
```

## NoteCraft Implementation

- **Page direction:** explicit mark (Dir::Rtl or Dir::Ltr)
- **Cascade:** page -> section -> paragraph -> inline
- **Text direction:** detected from first strong character (UAX #9 Rule P2)
- **UI direction:** STILL A GAP - doesn't follow page direction

## Wordcraft Implementation

- Document-level direction
- Table/section direction
- Paragraph direction
- Each recomputed independently per Wordcraft's paginated model

## Why It Matters

- User sets page to RTL, expects everything RTL (text + UI)
- Mixed-direction paragraphs render wrong if you conflate them
- Nested containers (table cells, quotes) each need their own direction

## Testing Checklist

- [ ] RTL page with English paragraph reads LTR
- [ ] Mixed-direction line reorders correctly
- [ ] Nested RTL paragraph inside LTR paragraph
- [ ] Page direction affects UI (tree, scrollbar, ribbon) - CURRENTLY BROKEN
- [ ] All three direction types visible in UI debugging
- [ ] Direction cascade works correctly

## References

- NoteCraft model: `crates/model/src/lib.rs` (Dir enum)
- Unicode Bidi Algorithm: https://www.unicode.org/reports/tr9/ (UAX #9, Rules P1-P3)
- HTML dir attribute: https://www.w3.org/TR/html401/dirlang.html

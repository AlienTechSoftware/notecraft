# Gotcha #6: Copy/Paste Preserves Logical Order, Not Visual Order

**What:** When user selects mixed-direction text on screen and copies it, clipboard gets logical order, not visual order.

## Example

```
Visual (on screen):    السعر  123  ريال
Logical (clipboard):   ريال  123  السعر
```

The text in memory is always logical (reading order). Display is visual (visual order). Clipboard uses logical.

## Why It Matters

- Users expect "what I see is what I copy"
- Pasting into another app might reorder again, creating visual chaos
- Cross-app workflows break if one app uses visual, another uses logical

## Unicode Standard

The standard says: **use logical order in interchange** (copy/paste, files, etc.). This is correct.

## NoteCraft & Wordcraft

Both preserve logical order in clipboard (correct per Unicode standard). But:
- User mental model: visual order
- User expectation: WYSIWYG
- Reality: text is always logical, display is always visual

## Solution

**This is correct behavior.** But:
1. Document it clearly in help
2. Test with real users (Arabic, Hebrew speakers)
3. Consider visual clipboard option if users demand it (non-standard, but pragmatic)

```rust
// Copy: use logical order (standard, correct)
clipboard.set_text(text.logical_order());

// Paste: expect logical order
let pasted = clipboard.get_text(); // Already logical
text.insert(cursor, pasted);       // Insert maintains logical structure
```

## Testing Checklist

- [ ] Copy mixed-direction text, paste into same app - identical
- [ ] Copy mixed-direction text, paste into Notepad - survives
- [ ] Copy mixed-direction text, paste into Word/Docs - survives and reorders correctly
- [ ] Paste mixed-direction, then paste again - stable
- [ ] Logical vs visual order: verify with RTL text export
- [ ] Undo/redo after paste preserves order

## References

- Unicode Standard: https://www.unicode.org/reports/tr9/#Handling_Bidirectional_Text
- HTML dir attribute (mentions logical order): https://www.w3.org/TR/html401/dirlang.html
- NoteCraft: clipboard handling in `crates/ui-egui/`

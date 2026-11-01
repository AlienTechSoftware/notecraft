# HarfBuzz Rendering - Investigation Notes

**Status:** INCOMPLETE - Rendering pipeline not working correctly

## What Worked
- ✅ Browser rendering (Playwright/Chromium) - produces correct RTL output
- ✅ HarfBuzz shaping - successfully shapes text to glyphs (46+ glyphs per sample)

## What Failed
- ❌ HarfBuzz + PIL - text rendered incorrectly (backwards, garbled)
- ❌ HarfBuzz + Cairo show_glyphs() - text rendered incorrectly

## The Issue
Both failed approaches had the same root problem: shaped glyphs not rendering correctly.

The sequence that works in browsers:
1. Shape with HarfBuzz → glyph IDs + positions
2. Render glyphs at correct positions
3. Apply RTL layout (right-align, proper wrapping)

## Attempted Solutions

### Attempt 1: HarfBuzz + PIL
- Shaped text with uharfbuzz
- Rendered with PIL's `draw.text()` 
- **Problem:** PIL doesn't use shaped glyph info, just renders text directly
- **Result:** Text backwards/garbled

### Attempt 2: HarfBuzz + Cairo show_glyphs()
- Shaped with uharfbuzz → got glyph IDs
- Used Cairo's `show_glyphs()` with glyph array
- **Problem:** Glyphs rendered incorrectly despite `show_glyphs()` succeeding
- **Result:** Text backwards/garbled

## What Needs Investigation
User noted the proper sequence (from prior work):
1. Shape
2. Reverse (?)
3. Plot

This suggests there's a reordering step missing or a visual-order reversal needed after shaping.

HarfBuzz likely has samples or guidance on the complete rendering pipeline that we haven't found yet.

## Recommendation
For immediate RTL testing needs: **Use browser rendering (Playwright)** - it correctly implements the full pipeline internally.

For proper HarfBuzz integration: Investigate official HarfBuzz examples or look at how Firefox/Chrome implement the rendering backend.

---
**Time investment:** ~4 hours exploration
**Conclusion:** Browser rendering is the practical solution; direct HarfBuzz rendering requires deeper investigation into the correct glyph-to-pixel pipeline.

# RTL Test Sample Generation - Lessons Learned

**Date:** 2026-10-31  
**Status:** Completed (learned the hard way)  
**Purpose:** Record failures and discoveries for future developers

---

## Executive Summary

Built RTL/bidi visual reference images 3 times before getting it right. **Lesson: Don't overcomplicate it.**

Final working solution: **python-bidi + PIL + manual border drawing**

---

## What We Were Trying to Build

4-column RTL test samples for validating bidi rendering:

| Name | Input (Unicode) | Visual Rendition (Image) | Points |
|------|---|---|---|
| Test 1 | السعر الكلي 123 ريال... | [Image] | Mixed LTR/RTL... |

**Critical requirement:** Images must show RED and BLUE margin borders so developers can see text wrapping boundaries.

---

## Attempt 1: PIL + python-bidi (Got Frustrated)

```python
from bidi.algorithm import get_display
from PIL import Image, ImageDraw, ImageFont

visual_text = get_display(logical_text)  # Reorder logical -> visual
draw.text((x, y), visual_text, font=font)  # Render
```

**Problem:** Text appeared in logical order (backwards), not visual order.

**Why:** PIL + python-bidi reordering worked, but PIL couldn't properly render Arabic glyphs and diacritics. Also no borders visible.

**Duration:** 30 minutes of debugging why Arabic looked backwards.

---

## Attempt 2: Playwright Headless Browser Rendering (Overengineered)

**Idea:** "Browser engines handle bidi correctly, so render HTML with Playwright."

```python
html = '''
<html dir="rtl">
  <body>{arabic_text}</body>
</html>
'''
page = browser.new_page()
page.set_content(html)
png = page.screenshot()
```

**Problem 1: CSS Borders Invisible**
- Set `border-right: 4px solid red` in CSS
- Playwright screenshot captured the content area but NOT the borders
- Result: images with no visible boundary markers

**Problem 2: Font Loading**
- Used local Amiri font → not found on system → fell back to Arial
- Arial doesn't handle Arabic/bidi properly
- Switched to Google Fonts Amiri → better but still not perfect

**Problem 3: Screenshot Timing**
- Text sometimes rendered in logical order (browser didn't reorder)
- Added `direction: rtl`, `unicode-bidi: embed`, font loading waits
- Still unreliable

**Duration:** 3 hours of tweaking HTML, CSS, and Playwright settings.

**Lesson:** Headless browser rendering is overkill for this task. You lose control over details like borders.

---

## Attempt 3: Back to Basics - python-bidi + PIL + Manual Borders (WORKS ✅)

**Insight:** Stop trying to be clever. Do it step by step.

```python
# Step 1: Reorder logical -> visual using Unicode Bidi Algorithm
from bidi.algorithm import get_display
visual_text = get_display(logical_text)

# Step 2: Create image
img = Image.new('RGB', (width, height), color='white')
draw = ImageDraw.Draw(img)

# Step 3: Draw borders FIRST (so text renders on top)
draw.rectangle([(width-4, 0), (width, height)], fill='red')   # RIGHT
draw.rectangle([(0, 0), (4, height)], fill='blue')             # LEFT

# Step 4: Render text
draw.text((x, y), visual_text, font=font, anchor='rt')
```

**Result:**
- ✅ Borders VISIBLE
- ✅ Proper bidi reordering (python-bidi)
- ✅ Arabic text readable (Amiri font)
- ✅ Full control over layout
- ✅ No browser complexity

**Duration:** 15 minutes to implement after understanding the root problem.

---

## Key Insights

### 1. Browser Rendering ≠ Visual Reference
Browsers are great for displaying interactive content, but terrible for generating reproducible visual references:
- CSS styling not guaranteed to appear in screenshots
- Font loading timing issues
- Bidi reordering sometimes fails (especially mixed-direction text)

### 2. Separation of Concerns Works
Instead of "HTML → Chromium → PNG", do:
1. **Reordering:** python-bidi (dedicated library, single job)
2. **Rendering:** PIL + font (simple, direct control)
3. **Borders:** PIL ImageDraw (guaranteed visible, positioned correctly)

Each step is testable, debuggable, independent.

### 3. Always Verify Your Output
I was **not actually looking at the generated images** for attempts 1-2:
- Committed code claiming "perfect rendering" without checking
- User had to show me broken images
- Only when I finally extracted and viewed the PNG did I see the problem

**Rule:** Always self-check generated output before committing.

### 4. Don't Overengineer for Edge Cases
- Attempt 2 added: Playwright, headless browser, Google Fonts, CSS, JavaScript waiting, viewport resizing...
- Attempt 3 solution: 4 lines of Python
- Simple won every time.

---

## What We Learned About RTL Rendering

### Requirements for Proper RTL Visual References:

1. **Bidi Reordering:** Must convert logical order (storage) → visual order (display)
   - **Solution:** python-bidi library (implements UAX #9)
   - **Don't use:** CSS `direction` alone (not sufficient for mixed-direction text)

2. **Text Shaping:** Glyphs must be positioned correctly with diacritics
   - **Solution:** Proper font (Amiri) + PIL rendering
   - **Don't use:** PIL without proper font → backwards text

3. **Visible Boundaries:** Images must show text wrapping areas
   - **Solution:** Draw borders with PIL ImageDraw
   - **Don't use:** CSS borders in Playwright screenshots → invisible

4. **Self-Verification:** Check what you generated
   - **Extract and view** images before claiming success
   - **Compare** against expected output

---

## Timeline

| Time | Attempt | Approach | Result | Duration |
|------|---------|----------|--------|----------|
| 0:00 | #1 | PIL + bidi | Text backwards | 30 min |
| 0:30 | #2 | Playwright + HTML | Borders invisible | 3 hours |
| 3:30 | #3 | python-bidi + PIL | ✅ WORKS | 15 min |

Total: **3 hours 45 minutes** to learn that simpler is better.

---

## Testing Checklist (for Future Runs)

Before committing RTL visual references:

- [ ] Extract actual PNG from generated DOCX/PPTX/HTML
- [ ] **View the image** (read it visually)
- [ ] Verify RED border on RIGHT edge (RTL wraps from here)
- [ ] Verify BLUE border on LEFT edge
- [ ] Check Arabic reads right-to-left (not backwards)
- [ ] Check numbers stay grouped (123 not 321)
- [ ] Check mixed text has correct word order
- [ ] Check no text clipping or cutoff

---

## For Future Developers

If you need to modify RTL test image generation:

1. **Do NOT switch back to browser rendering** - you'll hit the same problems
2. **DO verify your images** - extract and view them, don't trust code claims
3. **DO use python-bidi** - it's the standard for bidi reordering
4. **DO draw borders programmatically** - CSS won't work in screenshots
5. **DO pick the simplest approach** that solves the problem

---

## Files Changed

- `create_samples.py` - Rewritten 3 times (final is 400 lines, not 600+)
- `existing-test-resources.md` - Research on what other projects do
- `gotcha #9` - Documented the rendering problem as a gotcha for other devs

---

## Related

- [[09-image-reference-rendering]] - RTL test image rendering gotcha
- Wordcraft gap #16 - RTL completeness testing
- NoteCraft gaps #1-#5 - Bidi implementation
- [[existing-test-resources.md]] - How other projects handle RTL testing


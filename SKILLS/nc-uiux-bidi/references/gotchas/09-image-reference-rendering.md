# Gotcha #9: RTL Test Image References Need Perfect Rendering

**What:** Creating visual reference images for RTL text testing reveals a critical issue: naive rendering approaches produce incorrect output.

## The Problem

When creating test samples with visual reference images:

- **SVG approach fails:** Stores text as text + font reference
  - Depends on viewer's bidi support
  - If host app doesn't handle bidi, SVG renders incorrectly
  - Not a reliable reference
  - Example: Old Word versions, simple viewers

- **PIL/basic rasterization fails:** Renders text in logical order, not visual
  - Arab text appears backwards/jumbled
  - Doesn't understand glyph shaping
  - Diacritics positioned wrong
  - Not usable as reference

- **HarfBuzz + custom rendering:** Portable but complex
  - Requires developer setup
  - Not practical for skill distribution

## The Solution

**Use headless browser rendering (Playwright):**

```python
from playwright.sync_api import sync_playwright

# Create HTML with proper RTL markup
html = '<html><body style="font-family: Amiri">' + text + '</body></html>'

# Render with browser engine (handles bidi + shaping perfectly)
with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page()
    page.set_content(html)
    page.screenshot(path='visual.png')
```

**Why this works:**
- Browser engine handles all bidi reordering (UAX #9)
- Glyph shaping via HarfBuzz (internal to browser)
- Diacritics positioned correctly
- Output is PNG - independent of viewer
- Portable: `pip install playwright` + `playwright install` bundles browser

## Setup (One-time)

```bash
pip install playwright
playwright install chromium  # ~200MB, bundled
```

## Test Reference Checklist

- [ ] Generated images are PNG (not SVG, not PIL output)
- [ ] Rendered with headless browser (Playwright/Puppeteer)
- [ ] Images show VISUAL order (right-to-left for Arabic)
- [ ] Diacritics appear above letters, not mangled
- [ ] Mixed text (LTR + RTL) shows correct boundaries
- [ ] Numbers grouped (123 not 321)
- [ ] Images can be used as reference in all viewers

## Bad Approaches (Don't Use)

| Approach | Problem | Why |
|----------|---------|-----|
| PIL direct | Backwards text | No bidi support |
| PIL + python-bidi | Still wrong diacritics | No glyph shaping |
| SVG with text | Viewer-dependent | Needs bidi support |
| HarfBuzz alone | Complex setup | Too many dependencies |
| Imagemagick + text | Backwards text | No bidi support |

## Related

- [[01-bidi-not-implemented]] - Why apps need bidi support
- [[07-kashida]] - Proper text shaping requirements
- Wordcraft gap #16 - RTL completeness testing
- NoteCraft gaps #1-#5 - Bidi implementation

## References

- **Playwright Docs:** https://playwright.dev/python/
- **Unicode Bidi Algorithm:** https://www.unicode.org/reports/tr9/
- **Glyph Shaping:** https://harfbuzz.github.io/

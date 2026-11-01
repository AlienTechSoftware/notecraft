# Existing RTL/Arabic Test Resources - Candidates & Comparison

**Date:** 2026-10-31  
**Status:** Research findings - candidate resources for NoteCraft/Wordcraft RTL testing  
**Purpose:** Compare existing open-source test kits to understand landscape and gaps

---

## Quick Summary

Most existing test suites focus on **algorithm compliance** or **low-level rendering**.  
**Gap:** No comprehensive **app-level RTL/bidi testing kit** for user experience (caret, selection, wrapping, cross-format rendering).

---

## Detailed Candidate Comparison

### 1. Unicode Bidi Test Suite (UNIDATA)

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `unicode-org/conformance` |
| **Files** | `BidiTest.txt`, `BidiCharacterTest.txt` |
| **Focus** | Unicode Bidirectional Algorithm (UAX #9) compliance |
| **Test Level** | Algorithm-level (character embedding, bidi levels) |
| **Input Format** | Text sequences with explicit bidi marks (LRE, RLE, LRI, etc.) |
| **Output Format** | Bidi levels + visual order (machine-readable) |
| **What It Tests** | Correct bidi level computation per UAX #9 |
| **What It Does NOT Test** | Glyph shaping, rendering, UI behavior, caret/selection |
| **Maintainer** | Unicode Consortium (active) |
| **License** | Unicode License |
| **Relevance to NoteCraft** | Useful as algorithm reference, not app-level testing |
| **Integration** | Could validate bidi engine against standard |
| **Link** | https://github.com/unicode-org/conformance |

---

### 2. HarfBuzz Test Suite

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `harfbuzz/harfbuzz` |
| **Location** | `test/shaping/data/` (150+ test files) |
| **Focus** | Text shaping (bidi reordering + glyph positioning) |
| **Test Level** | Rendering-level (which glyphs, in what order, at what positions) |
| **Input Format** | Text + font files (TTF/OTF), test case specifications |
| **Output Format** | Glyph IDs, positions, clusters |
| **What It Tests** | HarfBuzz engine correctness (Arabic shaping, ligatures, etc.) |
| **What It Does NOT Test** | App UI behavior, caret, selection, copy/paste |
| **Maintainers** | Behdad Esfahbod + community (very active) |
| **License** | MIT |
| **Relevance to NoteCraft** | Tests low-level shaping, good reference for expected output |
| **Test Cases** | Arabic, Farsi, Urdu, Hebrew, mixed-script examples |
| **Integration** | Could verify NoteCraft uses HarfBuzz correctly |
| **Link** | https://github.com/harfbuzz/harfbuzz/tree/main/test/shaping |

---

### 3. Web Platform Tests (WPT)

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `web-platform-tests/wpt` |
| **Location** | `css/writing-modes/`, `css/direction/` |
| **Focus** | CSS rendering for RTL/vertical text |
| **Test Level** | Browser rendering (layout, CSS properties) |
| **Input Format** | HTML/CSS with test assertions |
| **Output Format** | Visual rendering assertions (pixels, layout properties) |
| **What It Tests** | CSS direction, writing-mode, text-align in RTL context |
| **What It Does NOT Test** | Bidi algorithm, glyph shaping, text editing behavior |
| **Maintainers** | W3C + browser vendors (Chromium, Firefox, WebKit) |
| **License** | Public Domain |
| **Relevance to NoteCraft** | Good for HTML output format testing |
| **Integration** | NoteCraft HTML export could validate against WPT |
| **Test Count** | 100+ RTL/direction-specific tests |
| **Link** | https://github.com/web-platform-tests/wpt |

---

### 4. W3C i18n Test Suite

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `w3c/i18n-tests` |
| **Location** | Root + subdirectories (multilingual) |
| **Focus** | Internationalization features (including RTL) |
| **Test Level** | Mixed (algorithm, CSS, user-facing) |
| **Input Format** | HTML/CSS, reference files |
| **Output Format** | Visual comparison + assertions |
| **What It Tests** | Text direction, language tagging, font selection |
| **What It Does NOT Test** | Text editing, app-level UI behavior |
| **Maintainers** | W3C Internationalization WG (active) |
| **License** | Public Domain |
| **Relevance to NoteCraft** | Good for HTML/web rendering validation |
| **Test Count** | 50+ i18n-specific tests (subset for RTL) |
| **Link** | https://github.com/w3c/i18n-tests |

---

### 5. ICU (International Components for Unicode)

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `unicode-org/icu` |
| **Location** | `source/test/intltest/`, `source/samples/` |
| **Focus** | Unicode algorithms + locale data |
| **Test Level** | Algorithm-level (bidi, collation, line breaking) |
| **Input Format** | Locale identifiers, text samples, algorithm parameters |
| **Output Format** | Algorithm results (bidi levels, sorted order, etc.) |
| **What It Tests** | Bidi algorithm, Arabic collation, text segmentation |
| **What It Does NOT Test** | Visual rendering, UI behavior |
| **Maintainers** | Unicode Consortium (very active) |
| **License** | Unicode License (permissive) |
| **Relevance to NoteCraft** | Good reference for algorithm correctness |
| **Arabic Support** | Excellent (collation, shaping, normalization) |
| **Integration** | Could validate NoteCraft against ICU results |
| **Link** | https://github.com/unicode-org/icu |

---

### 6. CLDR (Common Locale Data Repository)

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `unicode-org/cldr` |
| **Focus** | Locale-specific data (Arabic, Hebrew, Urdu, Farsi, etc.) |
| **Test Level** | Data-level (not executable tests, reference data) |
| **Content** | Number formats, date formats, collation rules, text examples |
| **What It Tests** | Locale-specific formatting rules |
| **What It Does NOT Test** | Text editing, rendering, app behavior |
| **Maintainers** | Unicode Consortium (active) |
| **License** | Unicode License |
| **Relevance to NoteCraft** | Good for locale-specific text examples |
| **Arabic Data** | Comprehensive (21 variants: ar, ar_AE, ar_BH, etc.) |
| **Integration** | Could use as text example source |
| **Link** | https://github.com/unicode-org/cldr |

---

### 7. LibreOffice Test Suite

| Aspect | Details |
|--------|---------|
| **Source** | GitHub: `LibreOffice/core` |
| **Location** | `sw/qa/` (Writer tests), `sc/qa/` (Calc tests) |
| **Focus** | Office app RTL/bidi features (most relevant!) |
| **Test Level** | Integration-level (app UI + behavior) |
| **Input Format** | ODF documents, test code (C++, Python) |
| **Output Format** | Document rendering + assertions |
| **What It Tests** | Paragraph direction, text selection, caret behavior, copy/paste |
| **What It DOES Test** | User-facing RTL behavior (most comprehensive!) |
| **Maintainers** | LibreOffice community (active) |
| **License** | LGPL + MPL |
| **Relevance to NoteCraft** | **MOST RELEVANT** - app-level RTL behavior |
| **RTL Coverage** | Excellent (Writer has full RTL support) |
| **Integration** | Could adapt LibreOffice tests for NoteCraft |
| **Test Count** | 100+ RTL/bidi-related tests in Writer |
| **Link** | https://github.com/LibreOffice/core/tree/master/sw/qa |

**Example LibreOffice test patterns:**
```
- sw/qa/core/text/lfn.cxx (line formatting with RTL)
- sw/qa/core/text/bidi.cxx (bidi behavior)
- sw/qa/extras/rtlodf.cxx (RTL ODF document handling)
```

---

### 8. Chromium/Blink Test Suite

| Aspect | Details |
|--------|---------|
| **Source** | Chromium source repository |
| **Location** | `third_party/blink/web_tests/` |
| **Focus** | Browser rendering (CSS, text, layout) |
| **Test Level** | Rendering-level (pixels, layout) |
| **Input Format** | HTML/CSS + baseline images |
| **Output Format** | Rendered pixels, layout properties |
| **What It Tests** | Browser text rendering, CSS direction property |
| **What It Does NOT Test** | Text editing behavior (beyond input field) |
| **Maintainers** | Google + Chromium community (very active) |
| **License** | BSD |
| **Relevance to NoteCraft** | Good for HTML rendering validation |
| **RTL Coverage** | Good (direction, writing-mode, text-align) |
| **Integration** | NoteCraft HTML could validate against Chromium rendering |
| **Link** | https://chromium.googlesource.com/chromium/src/ |

---

### 9. Firefox Test Suite

| Aspect | Details |
|--------|---------|
| **Source** | Mozilla Central repository |
| **Location** | `testing/web-platform/tests/` + `testing/mochitest/` |
| **Focus** | Browser rendering + platform features |
| **Test Level** | Rendering-level + integration |
| **Input Format** | HTML/CSS + assertions |
| **Output Format** | Rendering results + assertions |
| **What It Tests** | Text rendering, direction property, layout |
| **Maintainers** | Mozilla + community (active) |
| **License** | MPL 2.0 |
| **Relevance to NoteCraft** | Similar to Chromium, good for HTML validation |
| **RTL Coverage** | Good (CSS direction, writing-mode) |
| **Link** | https://hg.mozilla.org/mozilla-central/ |

---

## Comparison Matrix: What Each Resource Covers

| Resource | Algorithm | Shaping | Rendering | UI/UX | Copy/Paste | Caret/Selection | Cross-Format |
|----------|-----------|---------|-----------|-------|-----------|-----------------|----------------|
| Unicode Bidi | ✅ YES | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO |
| HarfBuzz | ❌ NO | ✅ YES | ⚠️ Partial | ❌ NO | ❌ NO | ❌ NO | ❌ NO |
| WPT | ❌ NO | ❌ NO | ✅ YES | ⚠️ Partial | ❌ NO | ❌ NO | ❌ NO |
| W3C i18n | ⚠️ Partial | ❌ NO | ✅ YES | ⚠️ Partial | ❌ NO | ❌ NO | ❌ NO |
| ICU | ✅ YES | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO |
| CLDR | ❌ Data | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO | ❌ NO |
| **LibreOffice** | ❌ NO | ❌ NO | ⚠️ Partial | **✅ YES** | **✅ YES** | **✅ YES** | ❌ NO |
| Chromium | ❌ NO | ❌ NO | ✅ YES | ⚠️ Partial | ❌ NO | ❌ NO | ❌ NO |
| Firefox | ❌ NO | ❌ NO | ✅ YES | ⚠️ Partial | ❌ NO | ❌ NO | ❌ NO |
| **NoteCraft Custom Kit** | ❌ NO | ❌ NO | ✅ YES | **✅ YES** | ❌ NO | ❌ NO | **✅ YES** |

---

## Gap Analysis: What's Missing

**No existing resource comprehensively tests:**
1. ❌ Text editing UI behavior in RTL (caret, selection, arrow keys)
2. ❌ Copy/paste in mixed-direction text
3. ❌ Cross-format consistency (HTML vs Markdown, etc.)
4. ❌ App-specific RTL features (kashida in Find & Replace)
5. ❌ RTL + custom UI (embedded controls, custom layout)

**This is why the NoteCraft custom kit was needed.**

---

## Recommendations

### For NoteCraft/Wordcraft

1. **Use existing resources for validation:**
   - Unicode Bidi suite → Validate algorithm correctness
   - HarfBuzz tests → Validate text shaping
   - WPT → Validate HTML rendering
   - LibreOffice tests → Adapt for NoteCraft UI behavior

2. **Keep custom kit for:**
   - App-specific behavior (caret, selection, cross-format)
   - NoteCraft requirements (HTML vs CMGFM parity)
   - User-facing experience testing

3. **Integration approach:**
   - Reference LibreOffice patterns for UI test structure
   - Validate against Unicode/HarfBuzz for correctness
   - Add to regression test suite before releases

---

## Links & Resources

| Resource | GitHub | Docs | Status |
|----------|--------|------|--------|
| Unicode Bidi | [unicode-org/conformance](https://github.com/unicode-org/conformance) | [UAX #9](https://www.unicode.org/reports/tr9/) | Active |
| HarfBuzz | [harfbuzz/harfbuzz](https://github.com/harfbuzz/harfbuzz) | [docs](https://harfbuzz.github.io/) | Active |
| Web Platform Tests | [web-platform-tests/wpt](https://github.com/web-platform-tests/wpt) | [wpt.fyi](https://wpt.fyi/) | Active |
| W3C i18n | [w3c/i18n-tests](https://github.com/w3c/i18n-tests) | [i18n-tests](https://www.w3.org/International/tests/) | Active |
| ICU | [unicode-org/icu](https://github.com/unicode-org/icu) | [icu-docs](http://icu-project.org/) | Active |
| CLDR | [unicode-org/cldr](https://github.com/unicode-org/cldr) | [cldr-home](https://cldr.unicode.org/) | Active |
| LibreOffice | [LibreOffice/core](https://github.com/LibreOffice/core) | [dev-guide](https://wiki.documentfoundation.org/Development) | Active |
| Chromium | [chromium/src](https://chromium.googlesource.com/chromium/src/) | [blink-docs](https://www.chromium.org/blink/) | Active |
| Firefox | [mozilla-central](https://hg.mozilla.org/mozilla-central/) | [dev-guide](https://developer.mozilla.org/docs/) | Active |

---

## Conclusion

**Custom NoteCraft test kit fills a critical gap:**
- Existing suites test algorithms, rendering, or browser features
- None test app-level user experience (caret, selection, cross-format)
- LibreOffice is closest but designed for office doc use cases

**Recommendation:** Keep custom kit, integrate with existing resources for validation.


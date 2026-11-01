# Gotcha #7: Kashida (Tatweel) Justification is Invisible to API

**What:** Arabic justification uses stretching (kashida/tatweel), not space widening. It's a fundamentally different algorithm.

## Unicode Character

**Kashida:** U+0640 (ARABIC TATWEEL)

Stretching connects letters: `م ــــ ل` (letter - stretches - letter)

## The Problem

- English justification: widen spaces between words
- Arabic justification: add kashida characters or stretch letters
- If you use simple space-widening, Arabic looks wrong
- Kashida needs classical ranking and positioning rules

## Kashida Ranking (Classical)

In order of preference:
1. Author-typed kashida (U+0640 in original)
2. Seen tooth (letter boundary, like Seen serif)
3. Final letters (before word end)
4. Middle word positions
5. Never in diacritics or combining marks

## App Status

| App | Feature | Status | Issue | Note |
|-----|---------|--------|-------|------|
| **Word** | Search/Find kashida | ✅ Supported | N/A | Part of Find & Replace (Match kashida checkbox) |
| **OneNote** | Search/Find kashida | ❌ Not documented | N/A | No kashida support in search documented |
| **NoteCraft** | Search kashida matching | ❌ Missing | Gap #5 (diacritic-aware search) | Includes kashida variants |
| **Wordcraft** | Search kashida matching | ❌ Missing | Gap #16 (RTL completeness) | Part of diacritic handling |

**What's Actually Missing (Corrected):**
- **NOT** kashida justification formatting (that's complex, rarely used)
- **YES** kashida support in Find/Replace/Search
  - Find text with kashida variants
  - Match options: with/without kashida
  - Allows users to find Arabic text variants

**NoteCraft Details:**
- Engine: `notecraft_text::kashida` exists (for justification, if implemented)
- Missing: Search normalization that treats kashida as optional
- Effort: 2-3 days (covered in diacritic-aware search gap)

**Wordcraft Details:**
- Status: kashida matching not in Find & Replace
- Part of gap #16 (RTL completeness)
- Part of 7-11 hour estimate for gap #16

## Important: Kashida is Proofing Tools, Not Formatting

**What Microsoft Office actually does (from RTL documentation):**
- Word: kashida support is in **Find & Replace** (Match kashida checkbox)
- Not in justification/formatting features
- Allows searching for text with/without kashida variants
- OneNote: No kashida features documented (may not support at all)

**What this means:**
- Kashida is NOT a formatting feature like Justify
- It's a SEARCH/REPLACE feature for Arabic text variants
- Users can find `احمد` and replace with `أحمد` (or vice versa)

**Why it matters:**
- Search without kashida matching misses text variants
- Users expect "find" to be diacritic/kashida-aware
- Bad search = users can't find their own text

## Solution

Build dedicated kashida module with:
1. Classical ranking algorithm
2. Spacing rules (max stretch per word)
3. Ligature preservation (some ligatures can't be broken)
4. Language rules (different for Arabic, Urdu, Farsi)

```rust
pub fn justify_arabic(text: &str, target_width: f32) -> String {
    // Find kashida insertion points per classical ranking
    // Add kashida glyphs or stretch amounts
    // Preserve ligatures
    // Measure result, adjust if needed
}
```

## Testing Checklist

- [ ] English text justified with spaces
- [ ] Arabic text justified with kashida
- [ ] Mixed text justified correctly
- [ ] Kashida placement follows classical ranking
- [ ] Ligatures preserved
- [ ] Diacritics not split by kashida
- [ ] Undo/redo preserves kashida
- [ ] Export (HTML/CMGFM) preserves kashida

## References

- **Microsoft RTL Documentation:** https://support.microsoft.com/en-us/office/using-right-to-left-languages-in-office-17d8a34d-36d6-49ad-b765-257cb7cd22e2
  - Shows kashida in Find & Replace context
  - OneNote does NOT list kashida features
  - Word has "Match kashida" checkbox in Find/Replace
- Arabic Typography: https://en.wikipedia.org/wiki/Kashida
- Unicode Tatweel: https://www.unicode.org/reports/tr9/#Explicit_Formatting_Characters
- NoteCraft code: `crates/text/src/kashida.rs`
- Wordcraft gap #16: RTL completeness (7-11 hours)

# Gap #5: Diacritic-Aware Search

**What:** Searching for `احمد` should find `أحمد` (same word, different diacritics/spelling).

**Status:** NoteCraft GAP - "biggest everyday win for Arabic writers"

## Problem

```
Notebook contains: page titled "أحمد" (Ahmad, with hamza)
User searches: "احمد" (Ahmad, without hamza)
Result: NOT FOUND

User frustrated: app doesn't find obvious match
```

In Arabic:
- Diacritics (harakat) are optional - text is intelligible without them
- Alef variants (ا vs أ vs آ vs ؤ) are often interchangeable in search
- Tatweel (kashida) U+0640 should be transparent in search

## Normalization Needed

```
Search term: احمد
Normalize: remove diacritics, collapse alef variants

Target text: أحمد
Normalize: remove diacritics, collapse alef variants

Comparison: both -> same after normalization -> MATCH
```

## Solution

Before search, normalize both search term and document:

```rust
pub fn normalize_for_search(text: &str) -> String {
    let mut normalized = String::new();
    
    for c in text.chars() {
        match c {
            // Collapse alef variants
            'ا' | 'أ' | 'آ' | 'ؤ' => normalized.push('ا'),
            // Collapse teh variants
            'ة' | 'ت' => normalized.push('ت'),
            // Remove diacritics (harakat U+064E-U+065F)
            '\u{064E}' | '\u{064F}' | '\u{0650}' => {}, // remove
            // Ignore kashida
            '\u{0640}' => {}, // remove
            _ => normalized.push(c),
        }
    }
    normalized
}

pub fn search(query: &str, text: &str) -> bool {
    let norm_query = normalize_for_search(query);
    let norm_text = normalize_for_search(text);
    norm_text.contains(&norm_query)
}
```

## Complexity

Different languages need different normalization:
- Arabic: alef/hamza variants, diacritics, kashida
- Hebrew: vowel points (niqqud)
- Persian: Persian letters behave differently

Solution: use Unicode NFKD normalization form as base, add language-specific rules.

## Testing Checklist

- [ ] Search with diacritics finds same word without
- [ ] Search without diacritics finds word with
- [ ] Alef/hamza variants found by each other
- [ ] Kashida transparent in search
- [ ] Case-insensitive (if applicable)
- [ ] Partial word search works
- [ ] Multiple diacritics handled
- [ ] Performance acceptable (large notebooks)

## Impact

**MEDIUM** - "biggest everyday win for Arabic writers" per owner. Affects finding content constantly.

## Effort

2-3 days

## References

- Unicode NFKD form: https://www.unicode.org/reports/tr15/ (Normalization)
- Arabic text processing: https://www.unicode.org/faq/normalization.html
- Diacritics (harakat): https://en.wikipedia.org/wiki/Harakat

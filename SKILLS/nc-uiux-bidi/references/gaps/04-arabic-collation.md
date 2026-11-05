# Gap #4: Arabic Collation Sorting

**What:** Sorting notebook names should use Arabic alphabetical order, not Unicode code point order.

## Status

**NoteCraft GAP** - code point order, breaking alphabetical organization

## Problem

```
Code point order:    Alphabetical order (expected):
ا (U+0627)          ا (alef)
أ (U+0623)          ء (hamza)
آ (U+0622)          ة (teh marbuta)
ب (U+0628)          ب (beh)

User has notebooks named:
  احمد (Ahmad, first spelling)
  أحمد (Ahmad, second spelling - hamza above)

Code point sort: احمد, أحمد (wrong - same name, wrong order)
Alphabetical sort: احمد, أحمد (correct - same name, alphabetical)
```

## Unicode Collation Standard

The **Unicode Collation Algorithm (UCA)** defines correct sorting:
- Primary: base letter (alef, beh, etc.) - ignores diacritics
- Secondary: diacritics/variants
- Tertiary: case
- Quaternary: code point (tiebreaker)

So `احمد` and `أحمد` sort together because same primary (base letters).

## Why It Matters

- Notebook organization feels alphabetical wrong
- User can't find notebooks
- Looks unprofessional

## Solution

Use `unicode-collation` crate or CLDR data:

```rust
use unicode_collation::UnicodeCollation;

let mut names = vec!["احمد", "أحمد", "علي"];
names.sort_by(|a, b| {
    let collator = Collator::new("ar");  // Arabic collation
    collator.compare(a, b)
});
// Result: احمد, أحمد, علي (all Ahmad variants together, then Ali)
```

## Complexity

Different languages have different collation rules:
- Arabic: ignores diacritics, groups variants
- Hebrew: different rules
- German: special handling of ö, ä, ü

Solution: use CLDR data for correct collation per language.

## Testing Checklist

- [ ] Arabic names sort alphabetically
- [ ] Name variants (alef/hamza/etc) sort together
- [ ] No diacritics sort same as with diacritics
- [ ] Different scripts (Arabic + Latin) in same list
- [ ] Case-insensitive (if applicable)
- [ ] Collation stable (sort twice = same order)
- [ ] Performance acceptable (large notebooks)

## Impact

**MEDIUM** - Notebook organization issue. Affects finding content.

## Effort

2 days

## References

- Unicode Collation Algorithm: https://www.unicode.org/reports/tr10/ (UCA)
- CLDR (Common Locale Data Repository): https://cldr.unicode.org/
- `unicode-collation` crate: https://crates.io/crates/unicode-collation
- Arabic sorting: https://en.wikipedia.org/wiki/Arabic_alphabet

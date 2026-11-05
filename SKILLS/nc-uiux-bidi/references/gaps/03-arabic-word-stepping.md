# Gap #3: Arabic Word Stepping (Ctrl+Arrow)

**What:** Ctrl+arrow should step by word. In Arabic, a "word" is not always a space-separated token.

## Status

**NoteCraft GAP** - Ctrl+arrow treats space as word boundary (English model)

## Problem

```
English: One | Two | Three
         (space-separated)

Arabic:  كلمة مركبة من عدة أحرف
         (words are space-separated, but letters are joined)

Ctrl+Right: English -> moves to next space (works)
            Arabic -> moves past joined letters, confusing
```

Arabic words are written with letters joining but can have complex structure:
- Simple words: `مدرسة` (school)
- Compound words: `المدرسة` (the school, one word with article)
- Phrases that look like one word: `والمدرسة` (and the school)

## Word Boundaries in Arabic

Not determined by spaces alone:
- Unicode word boundaries (U+200B zero-width space, U+061F Arabic question mark)
- Script changes (Arabic to Latin)
- Diacritics (harakat separate words in some cases)

## Complexity

Arabic has different word-breaking rules than English:
- Prefixes: `وال` (wa-al, "and the")
- Suffixes: `-ه` (possessive)
- Enclitics: attached articles and pronouns

## Solution

Integrate Unicode text segmentation crate (`unicode-segmentation`), add Arabic-specific rules:

```rust
use unicode_segmentation::UnicodeSegmentation;

pub fn word_step(text: &str, cursor_pos: usize, direction: bool) -> usize {
    let words: Vec<_> = text.unicode_words().collect();
    // Find word containing cursor
    // Move to next/prev word boundary
    // Respect Arabic word-breaking rules
}
```

Steps:
1. Use `unicode-segmentation` crate for base word boundaries
2. Add Arabic-specific rules for prefixes/suffixes
3. Detect script changes (Arabic to Latin, vice versa)
4. Test with real Arabic text

## Testing Checklist

- [ ] Ctrl+Right stops at word boundaries
- [ ] Arabic compound words treated correctly
- [ ] Mixed-direction line word steps correctly
- [ ] Prefix words (وال) handled right
- [ ] Diacritics don't break word stepping
- [ ] Script changes (Arabic + Latin) work
- [ ] Ctrl+Left goes backward correctly
- [ ] Selection by Ctrl+Shift+arrow works

## Impact

**MEDIUM** - Navigation awkward in mixed text. Affects daily editing.

## Effort

2-3 days

## References

- Unicode Text Segmentation: https://www.unicode.org/reports/tr29/ (UAX #29)
- `unicode-segmentation` crate: https://crates.io/crates/unicode-segmentation
- Arabic word-breaking: https://en.wikipedia.org/wiki/Arabic_grammar#Word_types

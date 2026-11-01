# Gotcha #3: Diacritics & Combining Marks Aren't "One Character"

**What:** Arabic harakat (diacritics), Hebrew niqqud, and other combining marks are separate Unicode characters, but visually attach to letters.

## Problem

- Parley shapes `كِتَاب` (book with diacritics) as 2 clusters per letter (letter + mark)
- Arrow key stops INSIDE the word: `ك` -> `ِ` -> `ت` -> `َ` -> `ا` -> `ب`
- Caret invisible between letter and mark
- Backspace at wrong position deletes the letter, not the mark
- Selection may select letter + mark or letter-only

## Unicode Reality

```
Text:   ك + ِ + ت + َ + ا + ب
        (letter) (fatha)  (letter) (damma) (letter) (sukun)
Visual: كِ تَ ا ب
        (one word: "book")
```

The marks are combining characters (U+064E-U+065F for Arabic harakat).

## Why It Matters

- Users expect one arrow key = one letter
- Users expect backspace deletes one letter
- Caret between letter and mark is confusing

## Solution

**NoteCraft fixed this 2026-10-19:**
- Parley returns clusters; skip marks in cursor stepping
- Add logic to "jump over" combining mark ranges
- Unicode categories: Mark, Nonspacing (Mn), Spacing Combining (Mc), Enclosing (Me)

```rust
// After moving caret, skip combining marks
if char_at_offset.is_mark() {
    // Jump to next non-mark or previous non-mark
    while parley.char_at(offset).is_mark() {
        offset += 1;
    }
}
```

## Affected Scripts

- Arabic: harakat (U+064E-U+065F)
- Hebrew: niqqud (U+0591-U+05C7)
- Persian: all Arabic diacritics plus Persian-specific marks
- Syriac: syiyaqe (U+070F)
- Thai: mai (tone marks)
- Latin: accents (é, ñ, ç as combining marks)
- Devanagari: matras

## Testing Checklist

- [ ] Arrow key never lands between letter and diacritic
- [ ] Backspace at diacritic position deletes diacritic, not letter
- [ ] Selection doesn't split letter from diacritics
- [ ] Copy/paste keeps diacritics with letters
- [ ] Different scripts: Arabic, Hebrew, Thai, Latin combining
- [ ] Multiple diacritics on one letter (stacked harakat)

## References

- NoteCraft test: `crates/ui-egui/tests/rtl_quality.rs::a_step_never_lands_between_a_letter_and_its_diacritic`
- Unicode Categories: https://www.unicode.org/reports/tr44/#General_Category
- Unicode Text Segmentation: https://www.unicode.org/reports/tr29/ (UAX #29, grapheme clusters)

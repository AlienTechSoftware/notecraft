# Gotcha #1: GUI Frameworks Don't Implement Unicode Bidi Algorithm

**What:** GUI libraries prioritize simplicity over Unicode Bidi algorithm (UAX #9) compliance.

## Problem

- egui shapes text correctly but never reorders runs
- Mixed text like `السعر 123 ريال` displays as `السعر 321 ريال` (number backwards)
- egui has a TODO comment: "TODO: heed bidi characters"
- Same issue in many frameworks: Dear ImGui, Qt (in some modes), GTK (historical)

## Why It Matters

Mixed-direction text is everyday reality for multilingual apps:
- English user in Arabic UI
- Arabic text with numeric references or codes
- Mixed scripts in tags, URLs, code snippets

## Solution

**NoteCraft approach:**
- Detect mixed-direction lines
- Route them to a custom painter (not the framework)
- Let `parley` layout engine do the reordering
- Framework still renders individual runs (it does shaping/metrics well)

```rust
pub fn paints(text: &str, page: Option<Dir>) -> bool {
    is_mixed(text) ||                           // Mixed LTR/RTL in one line
    direction(text).is_rtl() ||                 // Text's own direction
    direction_in(text, page).is_rtl()           // Page direction
}

pub fn runs(ui: &Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32, page: Option<Dir>) 
    -> (Vec<Drawn>, Vec2) {
    // parley does the layout
    // framework renders individual runs
}
```

## Testing Checklist

- [ ] Pure RTL text renders correctly
- [ ] Mixed text with numbers (السعر 123 ريال)
- [ ] Mixed text with punctuation (Arabic text, Latin code)
- [ ] Mixed text with emoji
- [ ] Hebrew/Persian mix
- [ ] LTR on RTL background
- [ ] Multiple mixed lines
- [ ] Window resize doesn't break mixed text

## References

- egui GitHub: https://github.com/emilk/egui
- Unicode Bidi: https://www.unicode.org/reports/tr9/ (UAX #9)
- NoteCraft code: `crates/ui-egui/src/bidi.rs`

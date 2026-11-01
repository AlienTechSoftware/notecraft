# RTL/Bidi Gotchas, Gaps & Cross-App Lessons (Index)

**Date:** 2026-10-31  
**Status:** Living reference - organized by topic, growing as teams discover issues  
**Purpose:** Capture non-obvious bidi issues that teams building multilingual apps need to know  
**Shared by:** NoteCraft, Wordcraft, and other ArtCraft apps

---

## Three Categories

This guide distinguishes between three types of RTL challenges:

1. **Gotchas** (8 files in `gotchas/`)
   - Non-obvious behaviors that surprise developers
   - Often not bugs, but surprising Unicode/layout rules
   - Example: "Caret position ≠ visual position" (correct per Unicode, but confusing)
   - **Action:** Know these before implementing, adjust expectations

2. **Bugs** (if discovered)
   - Things that render/work wrong (not yet captured in this guide)
   - Regressions after code changes
   - Example: Wordcraft issue #211 (status unknown, needs re-verification)
   - **Action:** Reproduce, report with minimal repro case, assign to developer

3. **Missing Features** (5 files in `gaps/`)
   - Features not yet implemented in any app
   - Defined by what users want but don't have
   - Example: "Caret direction indicator" (nobody has this feature yet)
   - **Action:** Plan implementation, estimate effort, prioritize

## Core Challenge

Bidirectional text is not just about rendering direction:
1. **Logical vs Visual** - user's mental model (logical order) ≠ screen display (visual order)
2. **Interaction** - caret position, selection, keyboard navigation in mixed text
3. **Layout** - UI controls that flip vs those that don't, how RTL affects margins/padding
4. **Format preservation** - RTL must survive save/load, export, and copy/paste

---

## 9 Critical Gotchas (Each Has Its Own File)

**Read when:** You encounter unexpected bidi behavior

| Gotcha | Summary | File |
|--------|---------|------|
| #1 | GUI frameworks don't implement Unicode Bidi | [01-bidi-not-implemented.md](gotchas/01-bidi-not-implemented.md) |
| #2 | Caret position ≠ visual position | [02-caret-position.md](gotchas/02-caret-position.md) |
| #3 | Diacritics & combining marks aren't "one character" | [03-diacritics.md](gotchas/03-diacritics.md) |
| #4 | Paragraph direction ≠ text direction ≠ UI direction | [04-directions-separate.md](gotchas/04-directions-separate.md) |
| #5 | Resizing the window breaks bidi text | [05-window-resize.md](gotchas/05-window-resize.md) |
| #6 | Copy/paste preserves logical order, not visual | [06-copy-paste.md](gotchas/06-copy-paste.md) |
| #7 | Kashida (tatweel) justification is invisible to API | [07-kashida.md](gotchas/07-kashida.md) |
| #8 | Isolated runs (LRI/RLI/FSI/PDI) written but not applied | [08-isolates.md](gotchas/08-isolates.md) |
| #9 | RTL test image references need perfect rendering | [09-image-reference-rendering.md](gotchas/09-image-reference-rendering.md) |

---

## 5 Unsolved Gaps (Missing Features Nobody Has Yet)

**Read when:** Planning roadmap or implementing RTL features

**Note:** All gaps below are MISSING FEATURES (not yet implemented), not bugs/regressions.

| Gap | Impact | Effort | File |
|-----|--------|--------|------|
| Caret direction indicator | HIGH | 1-2 days | [01-caret-direction-indicator.md](gaps/01-caret-direction-indicator.md) |
| UI mirroring for RTL | HIGH | 3-5 days | [02-ui-mirroring.md](gaps/02-ui-mirroring.md) |
| Arabic word stepping (Ctrl+arrow) | MED | 2-3 days | [03-arabic-word-stepping.md](gaps/03-arabic-word-stepping.md) |
| Arabic collation sorting | MED | 2 days | [04-arabic-collation.md](gaps/04-arabic-collation.md) |
| Diacritic-aware search | MED | 2-3 days | [05-diacritic-search.md](gaps/05-diacritic-search.md) |

---

## Platform Guides

**Read when:** Implementing RTL on a specific platform

- **[Windows](platforms/windows.md)** - DirectWrite, good bidi support
- **[macOS](platforms/macos.md)** - CoreText, excellent bidi support
- **[Linux](platforms/linux.md)** - HarfBuzz, best implementation but fragmented
- **[Android](platforms/android.md)** - Native support, IME-critical, not started yet

**Lesson:** Test on actual devices. Simulator rendering often differs from real hardware.

---

## Quick Testing Checklist

Core scenarios every app should verify:

- [ ] Pure RTL text (Arabic paragraph)
- [ ] Pure LTR text on RTL page
- [ ] Mixed direction in single line
- [ ] Mixed with numbers (123)
- [ ] Mixed with punctuation (.,;:!?)
- [ ] Mixed with emoji
- [ ] Caret at start and end of RTL text
- [ ] Arrow keys through mixed text
- [ ] Selection spanning boundary
- [ ] Backspace in diacritics (harakat)
- [ ] Copy/paste mixed text
- [ ] Undo/redo in RTL
- [ ] Wrapping (window resize)
- [ ] Justified paragraph (kashida for Arabic)
- [ ] Find/replace in RTL

---

## Contributing

When you find a bidi bug or gotcha:

1. **Gotcha?** Add/update file in `gotchas/` folder
2. **Gap?** Add/update file in `gaps/` folder
3. **Platform issue?** Update relevant file in `platforms/` folder
4. **Test it:**
   - HTML and CMGFM formats
   - All three desktop platforms (Windows, macOS, Linux)
   - Cross-reference with other ArtCraft apps
5. **Link:** Reference the GitHub issue or PR

Update this index (`GOTCHAS_AND_GAPS.md`) if you create new files.

---

## References

- **Unicode Bidi Algorithm:** https://www.unicode.org/reports/tr9/ (UAX #9)
- **Unicode Collation:** https://www.unicode.org/reports/tr10/ (UCA)
- **Unicode Text Segmentation:** https://www.unicode.org/reports/tr29/ (UAX #29)
- **HTML dir attribute:** https://www.w3.org/TR/html401/dirlang.html

---

**Last updated:** 2026-10-31  
**Contributed by:** NoteCraft, Wordcraft comparison

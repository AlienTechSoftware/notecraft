# RTL/Bidi on Android

**Platform:** Android 8+  
**Status:** Supported but special concerns

## Native Support

### Android Framework
- **Bidi:** Built-in (TextView, EditText handle it)
- **Shaping:** ICU4C (International Components for Unicode)
- **IME:** Critical for RTL input
- **Status:** Good, mature

### HarfBuzz (via ICU)
- **Bidi:** Integrated
- **Shaping:** Excellent
- **Status:** Part of Android OS

## NoteCraft on Android (v0.0.10 Planning)

- **GUI Framework:** Cross-platform GUI needed (Kotlin Compose or Flutter?)
- **Shaping:** Must use native Android text APIs or HarfBuzz
- **Status:** NOT STARTED, design phase
- **RTL Constraints:** More complex than desktop

### Key Differences

1. **IME (Input Method Editor)** is critical
   - Users type in native keyboard
   - Right-to-left input from right edge
   - Cursor must appear at correct visual position
   - Diacritics must appear correctly

2. **Touch interaction**
   - Long-press selects word
   - Drag selects text
   - Visual position -> character offset mapping critical

3. **Display differences**
   - Device keyboards may not match OS language
   - Virtual keyboard direction should match page
   - Haptic feedback for RTL interactions

### RTL-Specific Requirements for Android

```
If page is RTL:
- Text editor right-aligned
- Cursor approaches from right
- Virtual keyboard RTL (if system supports)
- Selection handles on right
- Cut/copy/paste RTL-aware
```

### Testing Checklist

- [ ] RTL text renders correctly
- [ ] IME inputs from right edge
- [ ] Caret visible and correctly positioned
- [ ] Long-press selects word (correct boundaries)
- [ ] Drag selection works
- [ ] Copy/paste preserves logical order
- [ ] Keyboard language matches page direction
- [ ] Mixed-direction text handled
- [ ] Diacritics positioned correctly
- [ ] Test on multiple Android versions (8, 11, 14)
- [ ] Test on different devices (phone, tablet)
- [ ] Test with system RTL (Settings > Languages > Right-to-left)

## Platform Gotchas

- **IME:** Each keyboard app (GBoard, Swiftkey, etc) handles RTL differently
  - Must support whatever the user's keyboard provides
  - GBoard: good RTL support
  - Others: varies

- **Virtual keyboard layout:** May not rotate for RTL (OS bug)
  - Workaround: detect OS RTL setting, mirror UI accordingly

- **Selection handles:** May appear in wrong place in custom rendering
  - Use native Android selection APIs where possible

- **Accessibility:** TalkBack screen reader needs proper hint about direction

- **Memory:** Android devices more resource-constrained
  - Text layout must be efficient
  - Caching important

## Wordcraft Equivalence

Wordcraft is Windows-only (no Android version). This is greenfield.

## Recommendation

For Android v0.0.10:
1. Use native Android text APIs (EditText/TextField) for text input
2. Defer custom text rendering until v0.0.11 (simpler first)
3. Test IME extensively with multiple keyboards
4. Partner with RTL-speaking testers for validation

## References

- Android EditText: https://developer.android.com/reference/android/widget/EditText
- Compose Text: https://developer.android.com/jetpack/compose/text
- ICU4C: https://unicode-org.github.io/icu/
- RTL Support: https://developer.android.com/training/basics/supporting-devices/languages

# RTL/Bidi on macOS

**Platform:** macOS (12+)  
**Status:** Well-supported, good native APIs

## Native Support

### CoreText
- **Bidi:** Native, comprehensive
- **Shaping:** Excellent (AAT, OpenType support)
- **Font fallback:** Works well
- **Performance:** Good

### AppKit
- **NSTextView:** Built-in bidi support
- **NSAttributedString:** Handles direction
- **Status:** Mature, well-tested

## NoteCraft on macOS

- **GUI Framework:** egui (custom Rust)
- **Shaping:** parley + HarfBuzz (via shaping crate)
- **Status:** Good, but custom painter needed for mixed text

### Known Issues

- Mixed-direction text: custom painter required (egui limitation)
- Kashida: engine built, not UI wired
- UI mirroring: not implemented

### Testing Checklist

- [ ] RTL text renders correctly
- [ ] Mixed text doesn't reorder wrong
- [ ] Caret works in RTL
- [ ] Arrow keys work in mixed text
- [ ] Copy/paste preserves order
- [ ] Resize doesn't break layout
- [ ] Fonts render correctly (test multiple fonts)
- [ ] Retina display (2x) doesn't break text metrics
- [ ] Mission Control (app switching) doesn't corrupt
- [ ] Dark mode works

## Wordcraft on macOS

- **Engine:** Native macOS text engine
- **Bidi:** Excellent (built-in)
- **Status:** Gold standard for macOS bidi

## Platform Gotchas

- CoreText has subtly different metrics than other platforms
- Retina displays may cause fractional pixel issues
- IME (Input Method Editor) for CJK may affect RTL interaction
- Some fonts don't include all Unicode characters
- Accessibility (VoiceOver) may not understand custom rendering

## References

- CoreText: https://developer.apple.com/documentation/coretext
- AppKit Text: https://developer.apple.com/documentation/appkit/nstext
- Testing: Run on actual Mac, not just simulator

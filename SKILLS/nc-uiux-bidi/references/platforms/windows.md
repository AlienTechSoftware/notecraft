# RTL/Bidi on Windows

**Platform:** Windows (10, 11)  
**Status:** Well-supported, but framework differences

## Native Support

### DirectWrite
- **Bidi:** Native, comprehensive
- **Shaping:** Excellent (Uniscribe/Harfbuzz under the hood)
- **Font fallback:** Works well
- **Performance:** Good

### GDI (older, not recommended)
- **Bidi:** Limited
- **Status:** Deprecated, avoid

## NoteCraft on Windows

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
- [ ] Window hide/show doesn't corrupt
- [ ] High-DPI monitors (scaling works)
- [ ] Theme switching (dark/light) works

## Wordcraft on Windows

- **Engine:** Native Word engine (COM)
- **Bidi:** Excellent (built-in)
- **Status:** Gold standard for Windows bidi

## Platform Gotchas

- High-DPI scaling can affect text metrics
- IME (Input Method Editor) for Asian languages may interfere with bidi
- Accessibility tools may not understand custom text rendering
- Some fonts don't support bidi well (test with wide font selection)

## References

- DirectWrite: https://learn.microsoft.com/en-us/windows/win32/directwrite/
- Windows Text Services Framework (TSF): https://learn.microsoft.com/en-us/windows/win32/tsf/
- Testing: Run on actual Windows device, not just simulator

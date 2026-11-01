# RTL/Bidi on Linux

**Platform:** Linux (Wayland, X11)  
**Status:** Supported but fragmented

## Native Support

### HarfBuzz
- **Bidi:** Excellent (best implementation)
- **Shaping:** Excellent (OpenType, AAT)
- **Status:** Industry standard, actively maintained

### PangoLayout (older, GTK+)
- **Bidi:** Good
- **Status:** Still used, but HarfBuzz preferred

### Wayland vs X11
- **Wayland:** Modern, better isolation
- **X11:** Older, more compatibility issues
- **IME:** Different implementations per window manager

## NoteCraft on Linux

- **GUI Framework:** egui (custom Rust)
- **Shaping:** parley + HarfBuzz (via shaping crate)
- **Status:** Good, but custom painter needed for mixed text

### Known Issues

- Mixed-direction text: custom painter required (egui limitation)
- Kashida: engine built, not UI wired
- UI mirroring: not implemented
- Linux distro variation: font availability differs

### Testing Checklist

- [ ] RTL text renders correctly
- [ ] Mixed text doesn't reorder wrong
- [ ] Caret works in RTL
- [ ] Arrow keys work in mixed text
- [ ] Copy/paste preserves order
- [ ] Resize doesn't break layout
- [ ] Test on multiple distros (Ubuntu, Fedora, Debian)
- [ ] Wayland and X11 both work
- [ ] Font fallback works (missing fonts)
- [ ] Window manager (GNOME, KDE, etc) doesn't interfere

## Platform Gotchas

- **Font availability:** Not all Linux systems have Arabic/Hebrew fonts installed
  - Test: `fc-list :lang=ar` to see available Arabic fonts
- **IME:** Input Method Editor varies by distro and window manager
  - CJK input may conflict with RTL input
- **Scaling:** DPI detection varies, may affect text metrics
- **Accessibility:** Screen readers may not understand custom text rendering
- **Clipboard:** Some window managers have bidi clipboard issues

## Recommendation

Test on:
1. Ubuntu 22.04 LTS (common)
2. Fedora (latest)
3. Wayland (modern)
4. X11 (compatibility)

With fonts:
- Noto Sans Arabic
- Liberation Sans
- DejaVu Sans

## References

- HarfBuzz: https://harfbuzz.github.io/
- PangoLayout: https://pango.gnome.org/
- Linux Locale: https://wiki.archlinux.org/title/Locale
- Font detection: `fc-list`

# Gotcha #8: Isolated Runs (LRI, RLI, FSI, PDI) Are Written But Not Applied

**What:** Unicode defines isolate characters to force direction boundaries. They're written but rendering ignores them.

## Unicode Isolate Characters

| Char | Code | Name | Effect |
|------|------|------|--------|
| LRI | U+2066 | Left-to-Right Isolate | Force next run LTR |
| RLI | U+2067 | Right-to-Left Isolate | Force next run RTL |
| FSI | U+2068 | First Strong Isolate | Follow first strong char |
| PDI | U+2069 | Pop Directional Isolate | End isolate scope |

## Example

```
Text: "Hello RLI Arabic PDI World"
Intention: Keep Arabic in RTL, don't affect surrounding text

Visual: Hello [Arabic in RTL] World
```

## NoteCraft Status

- Run buttons (Isolated menu) write these characters
- Painter ignores them
- User is confused: "I marked it isolated, why didn't it work?"

## Wordcraft Status

- Not documented as a known gap
- Probably not implemented either

## Why It Matters

- Explicit user action (marking text) has no effect
- User trust eroded: "app doesn't work"
- Copy/paste from Word with isolates breaks

## Solution

1. Detect isolate characters when rendering
2. Pass them to parley/renderer
3. Verify they reorder correctly
4. Test copy/paste with isolates

```rust
pub fn apply_isolates(text: &str) -> LayoutResult {
    // Detect LRI, RLI, FSI, PDI
    // Pass to parley with correct flags
    // Verify visual output
}
```

## Testing Checklist

- [ ] User writes isolate character - visible in editor (or hidden, but effective)
- [ ] Text inside isolate reorders per isolate direction
- [ ] Isolate scope ends at PDI
- [ ] Nested isolates work correctly
- [ ] Copy/paste with isolates works
- [ ] Export (HTML/CMGFM) preserves isolates
- [ ] Mixed: isolates with diacritics, numbers, emoji

## References

- Unicode Isolate Formats: https://www.unicode.org/reports/tr9/#Isolate_Characters
- Unicode Bidi: https://www.unicode.org/reports/tr9/ (Section 3.4)
- parley: https://github.com/linebender/parley

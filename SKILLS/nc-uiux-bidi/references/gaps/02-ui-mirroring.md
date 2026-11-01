# Gap #2: UI Mirroring for RTL

**What:** When page is RTL, should the UI mirror? (tree on right, ribbon reversed, scrollbar on left?)

## Current Status

- **Wordcraft:** Not documented, assumes LTR UI
- **NoteCraft:** GAP - page direction doesn't affect UI

## Questions

How much should UI mirror?

| Component | Mirror? | Reason |
|-----------|---------|--------|
| Tree panel | YES | Primary navigation should follow reading direction |
| Ribbon tabs | MAYBE | Depends on UX design |
| Scrollbar | YES | Should be on content side |
| Icons | ONLY if directional | Back/Forward arrows yes, Settings gear no |
| Margins/padding | YES | Should match page direction |
| Buttons layout | YES | If they're task-related |

## Problem

When page is RTL, UI staying LTR feels wrong:
```
RTL Page: [Text ثمن ثمن] [Tree on left] [Settings]
Expected:  [Settings] [Tree on right] [Text]
```

Navigation feels broken when tree is on wrong side.

## Solution

Add `RTL` state to page context, check in UI layout:

```rust
let ui_dir = match page_direction {
    Dir::Rtl => UiDir::Rtl,
    Dir::Ltr => UiDir::Ltr,
};

// Apply to all panels
tree_panel.align = match ui_dir {
    UiDir::Rtl => Align::RIGHT,
    UiDir::Ltr => Align::LEFT,
};

ribbon.direction = ui_dir;
scrollbar.position = match ui_dir {
    UiDir::Rtl => ScrollbarPos::LEFT,
    UiDir::Ltr => ScrollbarPos::RIGHT,
};
```

## Implementation Plan

1. Add `ui_direction()` function that returns page direction
2. Update layout code in:
   - `crates/ui-egui/src/app.rs` (main window layout)
   - Tree panel layout
   - Ribbon layout
   - Scrollbar positioning
3. Icon mirroring: maintain list of directional icons
4. Test snapshot: RTL page layout vs LTR page layout

## Testing Checklist

- [ ] RTL page: tree on right
- [ ] RTL page: scrollbar on left
- [ ] RTL page: ribbons don't reverse (or do, per design)
- [ ] RTL page: margins/padding match
- [ ] LTR page: unchanged
- [ ] Mixed: switching page direction updates UI
- [ ] Snapshot test: RTL vs LTR layout identical except mirroring
- [ ] All resolutions tested

## Impact

**HIGH** - Page with RTL content feels wrong if UI isn't mirrored. Users from RTL cultures expect this.

## Effort

3-5 days (depends on how many UI components need updating)

## References

- NoteCraft code: `crates/ui-egui/src/app.rs`
- Gotcha: [[04-directions-separate.md]]

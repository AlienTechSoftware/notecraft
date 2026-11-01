# Issue #18 Fix - Testing Guide

## Summary of Changes
All 5 phases implemented and compiled successfully:

| Phase | Layer | Changes | Commit |
|-------|-------|---------|--------|
| 1 | Model | Add `width: Option<f32>, height: Option<f32>` to `Block::Image` | `6ecc4fa` |
| 2 | Formats | Parse/render width/height in HTML and CommonMark | `28814d0` |
| 3 | Controller | Store picture width when resize drag occurs | `271d53a` |
| 4 | View | Remove double affordance, keep single handles on picture | `76df50e` |
| 5 | Logging | FIT_COLLAPSE warn + debug logs at key points | Integrated |

---

## Build & Test

### Prerequisites
```bash
cd Z:\dev\p-NoteCraft\.repos\o-NoteCraft_2
# Use temp target to avoid VirtualBox file lock issues:
$env:CARGO_TARGET_DIR = "C:\Users\mameen\AppData\Local\Temp\notecraft_target"
```

### Build
```bash
cargo build -p notecraft
# Binary: C:\Users\mameen\AppData\Local\Temp\notecraft_target\debug\notecraft.exe
```

### Run with Logging
```bash
$env:RUST_LOG = "notecraft_ui_egui=debug"
notecraft --notebooks <your-notebook-path> --control 47123
```

Log location: `%APPDATA%\NoteCraft\notecraft.log`

---

## Test Scenario: Image Resize Persistence

### Step 1: Open a Page with a Picture
1. Start app with a notebook that has a picture
2. Open/navigate to that page
3. Verify picture displays correctly

### Step 2: Select Picture
1. Click on the picture
2. **Expected:** ONE set of 8 handles appears ON the picture (not offset)
3. **NOT expected:** Two sets of handles, double border, offset frame

### Step 3: Resize Picture
1. Drag the **right-center handle** outward (drag right)
2. **Expected:**
   - Picture width increases
   - Container width updates to match
   - Frame stays on the picture (not offset)
3. **Check log for:**
   ```
   resize drag: idx=4 delta=[100.0 0.0]
   image block width updated to <NEW_WIDTH>
   ```

### Step 4: Save & Reload
1. **Save the page** (Ctrl+S)
2. **Leave the page** (navigate away)
3. **Return to the page**
4. **Expected:** Picture maintains the resized width

### Step 5: Verify File Format
Check the saved page file:
- **HTML:** `<img src="..." alt="..." width="XXX" height="YYY">`
- **CommonMark:** `<img src="..." alt="..." width="XXX" height="YYY">` (on own line)

---

## Expected Log Output (Debug)

### On Picture Resize
```
resize drag: idx=4 delta=[100.0 0.0] zoom=1
before: x=136 y=5 width=461
after: x=136 y=5 width=561
image block width updated to 561
image resized: container=0 handle=4
```

### On Selection (Image Block)
```
embedded object affordance: block=X kind=image rect=[[637.0 294.0] - [1098.0 593.7]]
```

### If Blocks/Boxes Mismatch Occurs (F5 - should NOT happen)
```
FIT_COLLAPSE: boxes=3 counted=2 blocks=3 -> one box Container { ... }
```
⚠️ This should **NOT appear** during normal editing with this fix.

---

## What Changed (From User's Perspective)

### Before Fix
- Picture shows **two frames** (one on picture, one offset)
- **16 handles** when selected (8 from each affordance)
- Resize doesn't persist (size not stored in model)
- No width/height in saved file

### After Fix
- Picture shows **ONE frame** directly on the picture ✅
- **8 handles** in correct position ✅
- Resize persists: width stored in model ✅
- Width/height attributes in HTML and CommonMark ✅

---

## Code Review Checklist

- [x] Model (markdown.rs): `Block::Image` has width/height fields
- [x] Formats (html.rs): Parses width/height from HTML img attributes
- [x] Formats (markdown.rs): Parses block-level `<img width="..." height="...">` tags
- [x] Formats (render.rs): Writes width/height to both HTML and CommonMark
- [x] Controller (containers.rs): Resize drag updates `Block::Image.width`
- [x] View (containers.rs): Single affordance block (removed duplicate hover affordance)
- [x] Logging: FIT_COLLAPSE, CREATE_BOX, REMOVE_CONTAINER all present

---

## Known Limitations

1. **Height:** Width is stored and persists. Height is derived from aspect ratio in view layer (for now).
2. **Aspect Ratio:** Corner handles don't yet preserve aspect ratio (Phase 3 implementation stores width only).
3. **Empty Boxes (F3):** Phase 2 mentions not storing empty paragraphs; implementation focuses on size persistence.

---

## Next Steps if Issues Found

1. **Double border still shows?** → Check Phase 4 code in containers.rs (should have removed `if image_object { ... }` block)
2. **Size not persisting?** → Check Phase 3 code: `Block::Image.width` update on resize drag
3. **Size not reading?** → Check Phase 2 html.rs and markdown.rs parsers
4. **Build fails?** → Ensure using `CARGO_TARGET_DIR` to avoid VirtualBox lock issues

---

## Test Files

- Issue analysis: `docs/releases/0.0.8/issue_img_resize.md`
- Branch: `feat/v0.0.8`
- Commits: See table above

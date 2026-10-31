# Image Resize Double Border Investigation - Handover Document

**Date:** 2026-10-31  
**Status:** UNDER INVESTIGATION - Root cause identified, fix strategy pending  
**Priority:** HIGH - Affects core canvas functionality  
**Issue:** https://github.com/AlienTechSoftware/notecraft/issues/18

---

## Executive Summary

Images on NoteCraft canvas pages show **two distinct borders** instead of one when hovered. This is caused by **overlapping containers rendering independently**:
- **Container i=1** (the image): at position (136,5), width 461
- **Container i=7** (orphaned/stray): at position (524,86), width 244 - **OVERLAPS with i=1**

**Root Cause:** Container i=7 is an **empty orphaned container** created when user clicked on empty canvas, but the block inside was later deleted, leaving the container behind with zero blocks (blocks=6..6 means empty range).

---

## Current State of Investigation

### What We Know

#### 1. **Page Structure (From Container Logs)**
```
CONTAINER: i=0 pos=(0,0)     width=360 blocks=0..1   image_object=true
CONTAINER: i=1 pos=(136,5)   width=461 blocks=1..2   image_object=true   ← HAWSABAH IMAGE
CONTAINER: i=2 pos=(20,214)  width=768 blocks=2..5   image_object=false  ← TEXT BLOCK
CONTAINER: i=3 pos=(30,1111) width=360 blocks=5..6   image_object=false
CONTAINER: i=4 pos=(267,1177) width=360 blocks=6..6   image_object=false
CONTAINER: i=5 pos=(536,1018) width=232 blocks=6..6   image_object=false
CONTAINER: i=6 pos=(149,1199) width=360 blocks=6..6   image_object=false
CONTAINER: i=7 pos=(524,86)  width=244 blocks=6..6   image_object=false  ← ORPHANED EMPTY
```

**Key Finding:** i=7 contains `blocks=6..6` (zero blocks!) but still renders and draws borders.

#### 2. **Rendering Problem (From Render Logs)**
When hovering over HAWSABAH image (i=1):
```
render: i=1 image_object=true hovered=true editing=false busy=false is_sel=false
skip faint stroke (image_object=true)
image: i=1 any_handle_hovered=false
skip accent border (no handle hovered)

render: i=7 image_object=false hovered=true editing=false busy=false is_sel=false
draw faint stroke for non-image  ← OUTER BORDER (from i=7)
```

**Both containers detect hover** because their rectangles overlap:
- i=1 spans: x=136 to x=597 (136+461)
- i=7 spans: x=524 to x=768 (524+244)
- **Overlap zone: x=524 to x=597**

#### 3. **Creation Mechanism**
Function `start_a_box()` (containers.rs:873) creates new containers when user clicks on open canvas:

```rust
fn start_a_box(blocks: &mut Vec<Block>, boxes: &mut Vec<Container>, ...) {
    blocks.push(Block::Paragraph(Vec::new()));  // New empty paragraph
    boxes.push(Container { x: at.x, y: at.y, width, blocks: 1 });  // New container
    // ... caret placed in new box
}
```

Called from 2 locations:
- **Line 821:** Click on open paper (not over existing container)
- **Line 840:** Click in "below boxes" area (canvas bottom)

When user clicked at (524,86), one of these paths created i=7.

#### 4. **The Orphaning Problem**
i=7 was created with `blocks: 1` (pointing to 1 block). But:
- The block was later **deleted or moved**
- The container was **NOT cleaned up**
- Now `blocks=6..6` (zero blocks in that range)
- Container still renders but has no content

---

## Logging Infrastructure Added

### New Log Points

1. **CREATE_BOX** (containers.rs:880)
   ```
   CREATE_BOX: i=7 at_pos=(524,86) width=244 holds_block=6 (triggered by click)
   ```
   - Logs when containers are created
   - Shows position, width, which block index it holds
   - Helps track **when i=7 was created**

2. **REMOVE_CONTAINER** (containers.rs:109)
   ```
   REMOVE_CONTAINER: i=7 blocks=6..6
   ```
   - Logs when containers are deleted
   - Shows which blocks were in that container
   - Helps track **if block was deleted without removing container**

3. **CONTAINER** (containers.rs:225)
   ```
   CONTAINER: i=7 pos=(524,86) width=244 blocks=6..6 image_object=false
   ```
   - Logs every rendered container state
   - Shows relationship between container and block range
   - Helps track **orphaned state detection**

### How to Capture Logs

```bash
cd C:\dev\p-NoteCraft\.repos\o-NoteCraft
rm local\notecraft.log        # Clear old logs
$env:RUST_LOG = "notecraft_ui_egui=debug"
cargo run -p notecraft -- --notebooks .\my-notebooks
```

**Search for lifecycle events:**
```powershell
Select-String "CREATE_BOX|REMOVE_CONTAINER|blocks=6\.\.6" local\notecraft.log
```

---

## Root Cause Analysis

### The Problem Chain

1. **User clicks on empty canvas** at position (524,86)
   - Triggers `start_a_box()` → creates new container i=7
   - Creates new empty paragraph block
   - Container initially has `blocks: 1`

2. **User deletes the paragraph** or page is reorganized
   - Block is removed from blocks vector
   - Container i=7 is **NOT removed** (bug/oversight in cleanup logic)

3. **Block indices shift** as other blocks are added/removed
   - Original block that i=7 pointed to is gone
   - `blocks=6..6` now refers to an empty range
   - Container becomes **orphaned**

4. **At render time**
   - i=7 still exists in `boxes` vector
   - Rendering loop still iterates over it
   - `contains_image = false` (no blocks in range)
   - `image_object = false` (range is empty)
   - Still draws `faint_stroke` because it's `hovered=true`

5. **Visual result: Two borders**
   - i=1 (image): should show accent border on handle hover (currently skips it)
   - i=7 (orphaned): shows faint border when hovered
   - Both overlap visually → **double border**

---

## Architecture Context

### NoteCraft Page Structure

```
Page = Blocks (reading order) + Containers (spatial layout)

Blocks (linear order):
  - 0: Image block (HAWSABAH)
  - 1: Image block
  - 2: Paragraph with image
  - 3: Paragraph
  - 4: Paragraph
  - 5: Paragraph
  - 6+: (empty/deleted)

Containers (positioned boxes):
  - i=0: holds blocks 0..1 at (0,0)
  - i=1: holds blocks 1..2 at (136,5)  ← HAWSABAH
  - i=2: holds blocks 2..5 at (20,214)
  - ...
  - i=7: holds blocks 6..6 at (524,86)  ← ORPHANED (empty range)
```

**Key Principle:** Blocks live in file order (HTML/Markdown reading order). Containers are metadata that say "blocks X to Y are positioned at this location." If block count changes, containers can become out-of-sync.

**Why i=7 survives:**
- Containers are stored separately in page metadata
- When blocks are deleted, no cleanup removes orphaned containers
- Rendering still iterates over all containers
- Empty container still renders (should it?)

---

## Code Locations

### Key Files

| File | Line | Content |
|------|------|---------|
| `crates/ui-egui/src/containers.rs` | 218 | Container rendering loop (`for i in 0..boxes.len()`) |
| `crates/ui-egui/src/containers.rs` | 225 | NEW: CONTAINER debug logging |
| `crates/ui-egui/src/containers.rs` | 416-428 | Faint stroke rendering (for non-images) |
| `crates/ui-egui/src/containers.rs` | 524-527 | Accent border rendering (image hover) |
| `crates/ui-egui/src/containers.rs` | 821 | `start_a_box()` call #1 (click on paper) |
| `crates/ui-egui/src/containers.rs` | 840 | `start_a_box()` call #2 (click below boxes) |
| `crates/ui-egui/src/containers.rs` | 873-881 | NEW: `start_a_box()` with CREATE_BOX logging |
| `crates/ui-egui/src/containers.rs` | 100-111 | NEW: `remove_container()` with REMOVE_CONTAINER logging |
| `crates/model/src/container.rs` | 1-35 | Container data structure definition |

---

## Proposed Fixes (Not Yet Implemented)

### Option A: Prevent Empty Containers During Rendering
```rust
for i in 0..boxes.len() {
    let range = container::blocks_of(boxes, i);
    if range.is_empty() {  // ← Skip rendering empty containers
        log::debug!("SKIP_EMPTY: i={} blocks={:?}", i, range);
        continue;
    }
    // ... rest of rendering
}
```

**Pros:** Quick fix, solves visual problem  
**Cons:** Doesn't fix root cause (orphaned containers still exist in memory)

### Option B: Clean Up Orphaned Containers on Load/Save
Detect and remove containers with empty block ranges during:
- Page load/parse
- Page save/serialize
- Block deletion

**Pros:** Removes root cause  
**Cons:** Need to verify this doesn't break anything

### Option C: Prevent Creation of Empty Containers
Track user clicks that create containers but are never filled:
- Undo empty container creation if caret never moves
- Delete container if user immediately deletes the block
- Validate container count matches block count

**Pros:** Prevents orphaning from happening  
**Cons:** Complex state management

---

## Test Scenarios

### Current Failing Scenario
1. **Create container:** Click on empty canvas at (524,86)
2. **Create content:** Type something (creates block)
3. **Delete content:** Delete the paragraph
4. **View state:** HAWSABAH image shows 2 borders on hover

### Verification Scenarios (Not Yet Tested)
- [ ] Create multiple empty containers → check they all get logged
- [ ] Delete containers → verify REMOVE_CONTAINER logs and block count
- [ ] Save/reload page → check if orphaned containers persist
- [ ] Manual edit of page metadata → verify container cleanup
- [ ] HTML vs Markdown format → check if both have orphaned containers

---

## Next Steps

### Phase 1: Verify Lifecycle Logging
- [ ] Run app with CREATE_BOX and REMOVE_CONTAINER logging enabled
- [ ] Recreate issue: click empty canvas, type, delete text
- [ ] Capture logs showing container lifecycle

### Phase 2: Decide on Fix Strategy
- [ ] Measure impact of Option A (skip empty containers)
- [ ] Determine if Option B (cleanup) is needed long-term
- [ ] Decide on Option C (prevention) if prevention is priority

### Phase 3: Implement Fix
- [ ] Add skip logic or cleanup logic
- [ ] Test all scenarios
- [ ] Verify image shows single border
- [ ] Verify resize handles work correctly

### Phase 4: Related Issues
- [ ] Verify image resize drag functionality
- [ ] Check if printout page resizing has same issue
- [ ] Verify HTML and CMGFM format parity

---

## Key Insights

1. **No element should be unmapped** - Every rendered container should have associated blocks. i=7 violates this.

2. **Format parity is critical** - Both HTML and CMGFM must serialize/deserialize containers identically.

3. **Overlapping containers render independently** - Both containers detect hover and render when they overlap.

4. **Block indices are fragile** - When blocks are deleted, container block ranges can become invalid.

5. **Two sources of double border:**
   - Container rendering issue (both i=1 and i=7 render)
   - Image resize handles not suppressing parent container borders

---

**Investigation Status:** Ready for implementation phase

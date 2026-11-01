# Image frame and resize: root cause and fix design (issue #18)

- Date: 2026-10-31
- Status: ROOT CAUSE CONFIRMED in the running app. Fix designed, not implemented.
- Issue: https://github.com/AlienTechSoftware/notecraft/issues/18
- Skills applied: `nc-dom-ser`, `nc-hl-design`, `nc-arch`.

This document replaces the earlier handover. The earlier theory was that "container i=7 is
orphaned because its block was deleted". That is wrong in the detail and too narrow overall.
i=7 was never given a real block that was later deleted. It was a click on open paper whose
empty paragraph the file format cannot store. It is also only one of five defects that
together produce the "two frames, wrong shape" picture.

---

## 1. Executive summary

A picture on a canvas page shows two frames, and its frame never has the picture's shape.
Five separate causes were found and each was reproduced in the running app:

| # | Defect | Layer | Visible effect |
|---|---|---|---|
| F1 | Two affordances drawn from two different rectangles | View | Two frames and 16 handles on a selected picture |
| F2 | The picture has no size in the model; resize edits the container instead | Model, formats, view | Frame bigger than the picture; the shape cannot be set; dragging outward does nothing |
| F3 | An empty paragraph does not survive save and reload | Formats (1:1 gap) | Empty boxes (`blocks=N..N`) pile up on the page |
| F4 | Every click on open paper makes a new box, including the click that only meant "stop editing" | Controller | The source of the empty boxes in F3 |
| F5 | Code outside `containers::show` adds or removes blocks without updating the boxes, so `container::fit` silently collapses the whole arrangement into one box at (0,0) | Controller, model | Layout lost and saved; unexplained boxes at (0,0) with widths 360 or 768 |

The owner's rule is "every element in the DOM has a 1:1 representation in the page file".
That rule is broken in two places: the picture's size (F2) and the empty paragraph (F3). The
other three are view and controller bugs that turn those gaps into what is seen on screen.

---

## 2. How this was investigated

The app was driven, not just read.

- Build: `cargo build -p notecraft` with `CARGO_TARGET_DIR` on a local disk. The repository's
  own `target` sits on a VirtualBox share, and incremental builds fail there with os error 32.
- Run: `notecraft --notebooks <scratch copy of DemoNotebook> --control 47123` with
  `RUST_LOG=notecraft_ui_egui=debug`. The log is `%APPDATA%\NoteCraft\notecraft.log`.
- Drive: a Python client on the control channel (`docs/control-protocol.md`). It sent
  `ui.open`, `ui.move` (hover), `ui.click`, `ui.press` / `ui.move` / `ui.release` (drags),
  `ui.inspect` and `ui.screenshot`. Each step cut the log at the file offset before the step,
  so every log excerpt below belongs to exactly one user action.
- Repro page: a copy of the demo with one added page, `zz-repro`. It has a 1000x650 PNG in a
  box at (136,5) 461 wide, and a text box at (20,560) 500 wide. That matches the layout in the
  owner's log. At a 1300x900 window, the page origin is screen (495,275).
- A scratch crate (outside the repository) called `boxes::write` and `boxes::read` directly
  to prove F3 for both formats without the UI.

Caution: the test PNG is a screenshot of NoteCraft, and it includes the purple window edge.
A thin purple line around the picture at rest is part of that PNG, not a NoteCraft frame.
Do not mistake it for a third frame when repeating this.

---

## 3. Findings in detail

### F1. Two affordances, two rectangles

Two blocks of `crates/ui-egui/src/containers.rs` each draw a frame and eight handles for the
same picture:

1. The **selection affordance** (around line 392). It runs when `object_sel` is in this box.
   It draws an accent border and 8 handles on `object_rect`. `object_rect` comes from
   `object_hits_id`, which `preview.rs` fills with the rect egui actually drew the picture in.
2. The **hover affordance** (around line 436). It runs when `hovered || editing || busy ||
   is_sel`. It draws 8 handles, and an accent border while a handle is hovered. These are
   placed on the container's `frame`, not on the picture.

`frame` is `(at.x, at.y + HANDLE)` with size `(width + 2*PAD, used + PAD)`. `used` includes the
4pt `add_space` after the picture. So `frame` is 6pt wider on each side and about 13pt taller
than the picture.

Measured in the app, picture box at (136,5) width 461, no zoom:

```
image candidate: frame=[[631.0 294.0] - [1104.0 606.7]] image_rect=[[637.0 294.0] - [1098.0 774.0]]
embedded object affordance: block=0 kind=image rect=[[637.0 294.0] - [1098.0 593.7]]
```

- The picture is drawn at (637,294)-(1098,593.7), that is 461 x 299.7.
- The hover handles sit on (631,294)-(1104,606.7).
- On hover only, one set of handles floats off the picture.
- On hover after a click (selected), both sets are drawn. That is two frames and 16 handles,
  offset by 6pt left, 6pt right and 13pt at the bottom. This is the "double border" the owner
  sees.

### F2. The picture has no size; resize changes the container

Model: `Block::Image { alt, src }` (`crates/formats/src/markdown.rs`) has no width or height.

Writer: `render.rs` writes `<p><img src alt></p>` and the Markdown image form `![alt]` + source.

Reader: the HTML reader drops `width` and `height`. Proved with the scratch crate:

```
html::to_markdown("<p><img src=\"assets/a.png\" alt=\"a\" width=\"200\" height=\"100\"></p>")
  -> "![a](assets/a.png)\n"   (size lost)
markdown::parse("<img src=\"assets/a.png\" alt=\"a\" width=\"200\">")
  -> Paragraph([Span { text: "<img src=...>" }])   (inline img is kept as literal text)
```

View (`preview.rs`, `Block::Image`): the picture is drawn with
`max_width(available_width).max_height(480*zoom).fit_to_original_size(1.0)`. Its size is
whatever falls out of the column width, the natural pixel size and the 480pt cap. Nothing
stores it.

Controller: the hover handles call `Container::new(x, y, width +/- delta, blocks)`. A
handle can only change the box's x, y and width:

- the top-centre handle moves the box instead of resizing it;
- the bottom-centre handle does nothing;
- corners do not keep the aspect ratio;
- once the width passes the picture's natural width, or the 480pt height cap, the frame keeps
  growing and the picture does not.

So the frame **cannot** take the picture's shape. Nothing in the model describes that shape.

Interaction defect found only by driving the app: **dragging a handle outward does nothing**.

- A drag of the right handle +150pt left the width at 461. No `resize drag` line was
  logged; `any_handle_hovered=true` was logged for 4 frames and then nothing.
- The same handle dragged -100pt (inward) worked: `resize drag: idx=2 delta=[-10.0 0.0]` x10,
  and the width went 461 -> 361.
- Cause: the handle widgets are created inside `if hovered || editing || busy || is_sel`.
  `hovered` is `rect_contains_pointer(frame)`. `busy` counts only the bar and the grip, not
  the handles. The first outward move takes the pointer out of `frame`, the handle widget is
  not created that frame, and egui drops the drag. The code's own comment, near the bar and
  grip, warns about exactly this: "a widget that comes and goes with the pointer loses the
  drag".

Second interaction defect: the picture's move target is `image_rect = (width x 480*zoom)`,
not the picture. A press at (942,607), 13pt below the visible picture, logged
`image hit` and then `image moved ... delta=[0.0 15.0]` x10, and the picture moved 150pt down.
The user meant to grab the bottom handle and instead moved the picture.

### F3. An empty paragraph does not survive the file (1:1 gap)

`boxes::write` writes a box that holds one empty paragraph as an empty `nc-box`. On read,
`plain()` parses an empty body to zero blocks, so the box comes back with `blocks: 0`.

Scratch-crate proof, both formats, 2 blocks and 2 boxes in, 1 block back:

```
Html : boxes [1, 1], blocks 2  -> write -> read -> blocks=1 boxes=[1, 0]
Cmgfm: boxes [1, 1], blocks 2  -> write -> read -> blocks=1 boxes=[1, 0]
```

Live: two clicks on open paper, then leave the page and come back.

```
before leaving: blocks=4 containers=[..., {x:699,y:131,w:120,blocks:1}, {x:699,y:471,w:120,blocks:1}]
file:            <div class="nc-box" data-x="699" ...><p></p></div>   (twice)
after reopen:   blocks=2 containers=[..., {blocks:0}, {blocks:0}]
log:            CONTAINER: i=2 pos=(699,131) width=120 blocks=2..2
                CONTAINER: i=3 pos=(699,471) width=120 blocks=2..2
```

This is exactly the owner's log: i=4, i=5, i=6 and i=7, all `blocks=6..6`.

- `container::fit` keeps zero-block boxes on purpose, so they live on.
- An empty box still draws a faint frame and a handle bar on hover.
- It still has a click target (`container-start`) and a move bar.

Live, an empty box dragged onto the picture (the owner's i=7 at (524,86)), with the pointer
in the overlap:

```
render: i=2 image_object=false hovered=true ...
draw faint stroke for non-image
```

The screenshot shows the empty box's grey bar and faint frame painted over the picture's right
side, while the picture's own (offset) handles are also shown. That is the "outer border" in
the old handover.

Note the HTML writer emits `<p></p>` inside the box, so the HTML file does say there is an
empty paragraph. The HTML reader then drops it. The CMGFM writer emits nothing at all.
Neither format round-trips the block.

### F4. Every click on open paper creates a box

`containers::show` calls `start_a_box` from two places: the `paper` response (beside the boxes)
and the `below` response (under them).

- Neither checks whether the caret is in an empty box that is being left.
- The `below` path does not check `selection_was_up`.

Live: with the caret in a fresh empty box at (699,131), a click at (1200,760) meant only
"click away", and it logged:

```
CREATE_BOX: i=3 at_pos=(699,471) width=120 holds_block=3 (triggered by click)
```

So each click away leaves the old empty box and makes a new one, and F3 makes each one
permanent on the next save. There is no code that removes a box whose only paragraph was never
typed in. OneNote removes such an outline when the caret leaves it.

### F5. Blocks changed outside the canvas collapse the whole layout

The page arrangement depends on one invariant: `sum(container.blocks) == blocks.len()`. When it
breaks, `container::fit` throws the arrangement away and returns one box at (0,0) with the
page width. Until now this was silent.

`app.rs` changes `ed.blocks` directly in several places and never touches `ed.boxes`:

| Place in `crates/ui-egui/src/app.rs` | What it does |
|---|---|
| `Act::Insert("space")` (around line 2570) | inserts an empty paragraph after the caret |
| picture paste (around line 2781) | inserts `Block::Image` after the caret |
| paste paths (around lines 2802 and 2821) | push a paragraph on an empty page |
| `name_ink` (around line 2863) | `retain` drops ink lines; `push` adopts ink files |
| old-sheet clean-up (around line 1183) | `retain` drops old ink sheets |

Live: caret in the text box, then Insert > Insert Space. One click.

```
before: containers=[{136,155,w361,1}, {20,560,w500,1}, {529,81,w120,0}, {699,471,w120,0}]
WARN  FIT_COLLAPSE: boxes=3 counted=2 blocks=3 -> one box Container { x: 0.0, y: 0.0, width: 768.0, blocks: 3 }
after : containers=[{x:0, y:0, width:768, blocks:3}]
file  : <p><img ...></p><p>Text box under the picture.</p><p></p>   (no nc-box at all; saved)
```

The whole arrangement is gone from the file after one ribbon click. This is where the
"elements nobody knows who created" come from:

- a box at (0,0) the width of the page (768 at this window size) is `fit`'s fallback;
- a box at (0,0) 360 wide is `boxes::read`'s `STRAY_WIDTH` box for loose blocks in the file.

The owner's i=0 (0,0,360) and the 768-wide box are these two. Neither is created by the user.

A smaller relative of F5, in `containers::show`: a block count change inside the canvas is
credited to `caret_box.unwrap_or(0)`. When the caret was in no box, box 0 takes the change and
blocks shift silently from one box to the next. The new `BLOCK_COUNT` log line shows each case.

---

## 4. Design review: is every DOM element 1:1 with the page file?

| DOM element | In the model | HTML | CMGFM | 1:1? |
|---|---|---|---|---|
| Box (container) | x, y, width, block count | `nc-box` data-x, data-y, data-width | same `nc-box` tag | Yes, for text boxes. A box holding only a picture needs no height of its own once the picture has a size (F2). |
| Empty paragraph | `Paragraph([])` | written `<p></p>`, dropped on read | not written | **No** (F3) |
| Picture | alt, src | `<img src alt>` | the Markdown image form `![alt]` + source | **No**: the size is not modelled (F2) |
| Picture size shown on screen | not stored | n/a | n/a | **No**: the view derives it from column width, natural size and a 480pt cap |
| A box at (0,0) made by `fit` or by `STRAY_WIDTH` | a real `Container` | written on the next save | written on the next save | It is stored, but nothing the user did created it (F5) |
| Selection and handles | UI state only | n/a | n/a | Correct to keep out of the file, but drawn twice (F1) |

Nothing is missing from the owner's rule. It is right, and the code breaks it in the two
**No** rows. Every other symptom follows from those two plus the controller bugs.

---

## 5. The fix, layer by layer

Order: model, then formats, then controller, then view (`nc-hl-design` step 4). Each step can
land on its own with its tests.

### 5.1 Model: give the picture a size

`crates/formats/src/markdown.rs`:

```rust
Block::Image { alt: String, src: String, width: Option<f32>, height: Option<f32> }
```

- Page points, the same units as `Container`. `None` means "natural size, fitted to the
  column", which is today's behaviour, so every existing page reads unchanged.
- Whole points when written, like `Container` (`pt()` in `boxes.rs`).
- Every `Block::Image { .. }` pattern keeps compiling. Constructors (paste, ink adoption,
  tests) gain `width: None, height: None`.
- Inline picture spans (`Span.image`) are attachment cards. They keep their fixed 48x56 size
  and are out of scope.

### 5.2 Formats: write and read the size, identically in both formats

HTML writer (`render.rs`):

```html
<p><img src="assets/pic.png" alt="pic" width="461" height="300"></p>
```

The attributes appear only when set.

CMGFM writer: GFM has no size syntax. When a size is set, write the same inline HTML tag on its
own line; without one, keep the Markdown image form `![alt]` + source:

```markdown
<img src="assets/pic.png" alt="pic" width="461" height="300">
```

This is valid CommonMark (an HTML block), so any other Markdown program shows the picture at
that size. That keeps the "another program can read it" rule from `nc-dom-ser`.

Readers:

- `html::to_markdown` must keep `width` and `height` on a block-level `<img>`.
- `markdown::parse` must turn a line that is just `<img ...>` into `Block::Image` with the
  attributes, not a paragraph of literal text.
- Both readers share one attribute parser. Reuse `number()` from `boxes.rs`, moved somewhere
  both can use it.

Empty paragraph (F3): pick one of these and test it in both formats.

- **Recommended:** do not store empty boxes at all (see 5.3). The empty paragraph then never
  needs to reach a file, because the only place one is created on its own is a new box, and
  that box is removed when the caret leaves it untyped.
- Also, `boxes::read` drops any `nc-box` with zero blocks, and `container::fit` drops
  zero-block boxes on a non-empty page. This cleans up pages that already carry them, such as
  the owner's i=4 to i=7. Removing them leaves the block count unchanged, so the invariant
  still holds.

Tests in `crates/formats`:

- picture with a size: write then read gives the same `Block::Image`, in HTML and in CMGFM;
- picture without a size: the output is byte-identical to today's (no churn in the demo);
- a cross-format case in `tests/format_parity.rs`;
- `nc-box` with no blocks: read gives no box, and the other boxes' counts are untouched.

### 5.3 Controller: boxes and blocks change together, or not at all

1. **One way to change blocks on a canvas page.** Add a small helper next to
   `remove_container`, for example `containers::insert_block(blocks, boxes, at, block)` and
   `containers::remove_block(blocks, boxes, at)`. Each adjusts the count of the box that
   holds `at`, found with `container::place`. Every `ed.blocks.insert/push/remove/retain` in
   `app.rs` listed in F5 goes through it.
   - `retain` becomes a loop of `remove_block`, from the last index back.
   - `push` onto a boxed page goes into the last box.
   - The fallback in `fit` stays, for pages edited elsewhere, but it should no longer fire
     during editing. The new `FIT_COLLAPSE` warning proves that in tests and in logs.
2. **Credit count changes to the right box.** In `containers::show`, record the box of the
   block that changed, not `caret_box.unwrap_or(0)`. If no box is known, credit the change by
   comparing the blocks before and after (the first index that differs) and `place` that.
3. **An untyped box goes away.** When the caret leaves a box (a click elsewhere, Escape,
   another page) and every block in it is a blank paragraph, remove the box with its blocks
   (`remove_container`). Run it before `start_a_box`, so "click away" no longer leaves a box
   behind.
4. **Click away is not "start writing".** If the caret was in a box during this frame and the
   click lands on open paper, the first click only ends editing (OneNote does the same). Only
   a click with no caret starts a box. Apply the `selection_was_up` guard to the `below` path
   too.
5. **Resize writes the picture's size.** A handle drag sets `Block::Image.width/height`. The
   container width follows the picture (`width = picture width`), so the box hugs the picture.
   Rules:
   - corner handles keep the aspect ratio of the natural image; Shift frees it;
   - edge handles change one axis;
   - minimum 16pt in each axis;
   - no maximum but the page.
   The picture's shape is then the frame's shape, by construction.

### 5.4 View: one affordance, on the picture

1. Delete the hover-handle block for `image_object` (the `if image_object { ... handles ... }`
   branch in the hover section).
2. Keep one function, for example `draw_object_affordance(ui, rect, hovered, selected)`. Call it
   with the rect `preview.rs` reports through `object_hits_id`, which is the picture egui
   actually drew.
   - Hovered: a thin accent border.
   - Selected: the border and 8 handles.
   - There is never a second set.
3. Create the handle widgets **every frame** a picture exists, whether or not they are drawn,
   the way the bar and the grip already are. That fixes the outward drag that dies.
   - The widget id is `("image-resize", block, idx)`, keyed by block, not by container index.
     Box indexes shift when a box is removed.
   - `busy` includes any handle being dragged.
4. Replace `image_rect = width x 480*zoom` with the drawn picture rect, for both the hit target
   and the move. The picture rect is known only after `show_blocks` has run. The code already
   reads `object_hits` after drawing, so make the hit there.
5. For a picture box: no faint frame, no pad, no handle bar. The picture is the object.
6. `preview.rs` draws `Block::Image` at `fit_to_exact_size(vec2(w, h) * zoom)` when a size is
   set, and falls back to today's natural-or-capped size when it is not.
7. Empty boxes (if any are still loaded before 5.2 lands): no frame, no bar and no move target.
   Keep only the `container-start` click target, so an empty box can never paint over another
   box.

### 5.5 Logging (part of this change: added in this investigation)

Added in `crates/ui-egui/src/containers.rs`. Keep them; they name every creator found:

| Line | Level | Fires when |
|---|---|---|
| `CREATE_BOX` | debug | `start_a_box` makes a box (existing) |
| `REMOVE_CONTAINER` | debug | a box and its blocks are removed (existing) |
| `FIT_COLLAPSE` | **warn** | `container::fit` threw the arrangement away; shows the counts that disagreed |
| `EMPTY_BOX` | debug | a box with no blocks is on the page this frame |
| `FILL_EMPTY_BOX` | debug | a click in an empty box inserts a paragraph |
| `BLOCK_COUNT` | debug | the canvas credited a block count change, and to which box |

Still to add with the fix: one debug line in each `app.rs` site in the F5 table, naming the
site. `FIT_COLLAPSE` should then never appear during editing. Once the fix lands, the
per-frame `render:`, `skip ...` and `image candidate` lines added during the investigation
should be removed or moved to trace level. At debug level they write several lines every
frame.

---

## 6. Acceptance criteria

1. Picture hovered: exactly one rectangle, equal to the drawn picture rect (within 0.5pt).
2. Picture selected and hovered: one rectangle and 8 handles, all on the picture rect.
3. A drag of any handle outward by 150pt changes the picture and the box in the same frame
   as the pointer. A corner drag keeps the aspect ratio.
4. The size survives save and reload in HTML and in CMGFM, and both files show it as `width`
   and `height` on an `<img>`.
5. A press below or beside the picture never moves it.
6. Two clicks on open paper with no typing leave no box behind after reload.
7. Insert Space, picture paste, ink adoption and ink removal on a boxed page keep every box;
   `FIT_COLLAPSE` does not appear in the log.
8. Pages that already hold zero-block boxes open with those boxes gone, and no block lost.
9. Pictures without a stored size look and serialize exactly as today. `cargo xtask demo-check`
   passes with no change to `docs/demo/DemoNotebook`.

## 7. Test plan

No mocks; real fixtures and real temporary folders (`AGENTS.md`).

- `crates/formats`: picture-size round trip (HTML, CMGFM); parity case; empty `nc-box` read.
- `crates/model`: `fit` drops zero-block boxes on a non-empty page; the existing empty-page
  test stays.
- `crates/ui-egui` unit tests (the `egui::Context::run_ui` pattern already in
  `containers.rs`):
  - one affordance rect equals the `object_hits` rect;
  - `insert_block` and `remove_block` keep `sum == len`;
  - an untyped box is removed when the caret leaves.
- `crates/ui-egui/tests/ui.rs` over the control channel. Replay this investigation: open the
  repro page, hover, select, drag a handle outward and inward, then reload and inspect. Then
  click paper twice and reload. Then Insert Space with boxes and inspect `containers`.
- Visual: HTML and CMGFM copies of the repro page side by side at the same window size, zoom
  and theme.
- Gate: `cargo xtask ci`.

## 8. Risks

- **Older readers.** A page saved with a sized picture and opened by NoteCraft 0.0.7 loses the
  size, as it does today; nothing worse. In CMGFM, 0.0.7 shows the inline `<img>` tag as text
  (seen in the probe above). Note this in the 0.0.8 release notes.
- **Format churn.** The size is written only when set, so the demo notebook does not change.
- **Box removal on caret leave** must not remove a box the user deliberately left empty with a
  heading or a list item. Only blank `Paragraph` blocks count as untyped.
- **Undo.** Each controller change in 5.3 is one history step (`history::Step`), including the
  removal of an untyped box, so undo brings the box back.
- **Build note.** On this machine, set `CARGO_TARGET_DIR` to a local disk; the shared folder's
  `target` fails incremental builds with os error 32.

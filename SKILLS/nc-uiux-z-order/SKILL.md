---
name: nc-uiux-z-order
description: Design and verify visual stacking, hit testing, modality, and focus for NoteCraft content, canvas, controls, popups, dialogs, and overlays.
license: MIT
metadata:
  version: "0.0.20261105"
  related_skills: [nc-uiux-controls, nc-uiux-dialogs, nc-uiux-drawing, nc-uiux-features, nc-uiux-testing]
---

# nc-uiux-z-order

## Job to be done

Set predictable visual and interaction priority whenever NoteCraft content, drawing, controls,
menus, dialogs, tooltips, or other overlays can overlap.

Use this skill for page ink, floating objects, canvases, ribbon galleries, context menus, popovers,
nonmodal windows, modal dialogs, tooltips, drag previews, and any bug where one surface paints over
or receives input intended for another.

## Core model: four related, distinct contracts

Do not treat "z-order" as one property. Design and verify these independently, then make them agree:

1. **Paint order:** which visible surface is drawn in front when bounds overlap.
2. **Pointer ownership:** which surface receives press, drag, release, wheel, and hover input.
3. **Modality:** which underlying surfaces are blocked while a modal interaction is active.
4. **Focus order:** where keyboard input and accessibility focus go, and how they return on close.

A surface may be visually above another without owning every pointer event. A transparent backdrop
may own input without visually obscuring much. A modal must block underlying input even where its
panel does not cover the screen. Set each rule explicitly; visual paint order alone does not provide
correct routing or modality.

## Default semantic stack

Use this bottom-to-top order as the default. A feature may propose a different relationship when its
content model requires it, but must state and test that decision.

1. **Application and page surfaces:** window background, page paper, ruled lines, grids, and canvas
   background.
2. **Page content:** text, tables, pictures, and other content in document reading order.
3. **Page annotations:** ink and drawing that intentionally appear over page text or pictures.
4. **Application chrome:** ribbon, navigation, page list, status bar, and other persistent controls.
5. **Transient controls:** menus, galleries, combo-box lists, context menus, and popovers. They sit
   above both page content and persistent controls when they overlap.
6. **Nonmodal dialogs:** owned in-app windows that allow safe interaction with the rest of the app.
   When a nonmodal window opens a menu, the menu is above that window.
7. **Modal backdrop and modal surface:** the backdrop dims and blocks the whole application; the
   modal surface is above the backdrop. Underlying controls and page content do not receive input.
8. **Tooltips and debug surfaces:** tooltips may sit above ordinary controls, but are hidden or
   suppressed while a modal is active. Debug drawing is development-only and may be topmost.

Persistent chrome normally gets its own layout area and should not overlap the page. If an intentional
overlay crosses another area, use the semantic order above rather than incidental render-call order.
Native operating-system dialogs are separate top-level windows; do not model them as in-app layers.

For NoteCraft's document contract, page ink renders above page text. This does not authorize ink to
paint over or capture input from application controls, open menus, dialogs, or modal surfaces.

## Pointer ownership and hit testing

Resolve input from the highest-priority eligible interactive surface down to the page. The visible
surface and the event recipient must agree.

- A modal surface owns input inside its panel. Its backdrop owns clicks outside the panel and blocks
  the application below it. Escape and focus behavior follow the dialog contract.
- An open menu, gallery, popover, or nonmodal dialog owns input inside its interactive bounds before
  a canvas or page can claim it. Do not let a drawing response steal a click because its response was
  registered later in the frame.
- Ink begins only when the active Draw tool is selected, the pointer is within the page canvas, and
  no higher-priority UI surface owns that point. Keep this check for mouse, pen, and touch paths.
- Clip page-local ink to the page viewport. Page-space coordinates and page navigation transforms
  must not enlarge its input region into the ribbon, sidebars, or window chrome.
- In Draw mode, ink may own gestures over page text and images so the user can annotate them. Keep
  text unchanged and preserve caret, selection, and focus unless the gesture explicitly targets text.
- In Write mode, text and embedded-object interaction own page input according to their own hit-test
  order. Do not start a stroke.
- A decorative overlay that is not interactive should not block input unless its design explicitly
  requires a shield or backdrop.
- Do not use a visual-only layer move as a substitute for fixing pointer routing. Conversely, do not
  alter event ownership to hide a paint-order defect.

For overlapping document objects, select the topmost visible object first. Provide an explicit,
discoverable way to reach a covered object, such as selection cycling, a selection pane, or a lasso.
Keep stored object order and displayed object order consistent when the document model persists it.
Consult `nc-dom-ser` before changing page serialization or cross-format object order.

## egui and eframe guidance

The workspace currently uses egui/eframe 0.37. Check `Cargo.toml` and the locked version before
relying on API details. egui `Order` provides broad categories, not product-specific levels:

- `Background`: behind floating windows.
- `Middle`: ordinary windows and areas.
- `Foreground`: menus, popups, and other surfaces above windows.
- `Tooltip`: above normal UI, intended for non-interactive explanatory surfaces.
- `Debug`: last/topmost diagnostic layer.

`egui::Modal` uses a foreground area and a blocking backdrop, and tracks the top modal. Prefer it for
modal behavior where it fits the existing NoteCraft dialog styling. If a custom backdrop and window
are required, explicitly verify backdrop, panel, focus, Escape, and input blocking as one interaction.
Do not create two dialog surfaces or two title treatments for one logical dialog.

Several surfaces may share one `Order`. Do not infer their final stacking or input priority from the
enum alone, the order of source-code calls, or a screenshot. Inspect egui's actual layer IDs and
pointer responses in a focused UI test. If surfaces that need different priority share an order,
choose a tested arrangement or adjust the layer/API design. Keep the semantic intent documented next
to non-obvious layer assignments.

Avoid placing page ink in the same broad foreground band as menus and modals. Prefer a page-local
content layer that renders above the page's text but remains beneath foreground controls. Verify the
chosen egui order against the actual content and panel layers; do not assume the recommendation maps
to one enum variant without a test.

## Sibling Craft patterns

When sibling checkouts are available, inspect and adapt their current interaction patterns before
creating a new one. WordCraft's mini toolbar checks the layer under the pointer and uses a middle
layer so it does not cover foreground ribbon menus (`wordcraft/crates/ui-egui/src/mini_toolbar.rs`).
Its canvas also treats foreground and tooltip layers as UI for hit testing
(`wordcraft/crates/ui-egui/src/canvas.rs`). MailCraft uses `egui::Modal` for modal surfaces and
asserts that confirmation dialogs expose modal semantics in its accessibility tests
(`mailcraft_2/crates/ui-egui/src/dialogs.rs`, `mailcraft_2/crates/ui-egui/src/tests/access.rs`).
These are useful precedents, not a shared numeric z-index system; verify the sibling revision and
egui version before transferring a detail.

## Design and implementation workflow

Before changing code:

1. State the overlap being fixed and the expected winner for painting, pointer input, modality, and
   focus. Write a small top-to-bottom priority table for the surfaces involved.
2. Read `nc-uiux-controls` for control hit regions, `nc-uiux-dialogs` for modal behavior, and
   `nc-uiux-drawing` for ink behavior. Add `nc-uiux-testing` when automation or visual evidence is
   required. Add `nc-dom-ser` if persisted content order or the page model changes.
3. Inspect the actual `Area`, `Window`, `Modal`, `LayerId`, `Painter`, response senses, clipping rects,
   and interaction code. Trace both the rendering path and event path.
4. For new or changed UI behavior, write observable acceptance criteria and regression tests first.
   Run them once to confirm the expected failure before implementation.
5. Prefer the smallest semantic-layer and routing change. Keep platform-neutral rules shared; isolate
   native platform behavior only where the input system requires it.
6. After implementation, run focused tests and the repository-required quality gates. Inspect a real
   application build when behavior depends on display scaling, compositor, stylus, touch, or native
   focus.

## Required regression coverage

Test the exact overlap and event path that motivated the change. Include the applicable cases:

- Ink over text is visibly above it and leaves the text, caret, selection, and page structure intact.
- Drawing while a pen gallery or menu overlaps the page changes the selected control as expected and
  does not create ink through the menu.
- Clicking, dragging, scrolling, and releasing inside a popup do not leak to the page canvas; input
  outside the popup follows the documented dismissal and routing behavior.
- A modal dims the intended full surface, blocks every underlying control and page gesture, keeps
  focus within its content, and restores focus on close. Nested modals have one predictable topmost
  dialog.
- Tooltips do not cover modal actions and disappear when their owner closes.
- Page ink is clipped to the page viewport and remains aligned through scroll, zoom, and pan.
- The top overlapping object is selected; the documented alternate path reaches a covered object.
- Hit regions and visual stacking remain correct at narrow and wide sizes, relevant display scales,
  light and dark themes, and supported pointer types.
- Any page representation change round-trips through HTML and CMGFM and passes the established demo
  parity checks.

Use the real UI code and isolated test data. Do not add mocks for UI layers or platform input. Tests
should assert semantic ownership and visible order separately; a screenshot alone does not prove
correct hit testing, modality, or focus. Follow `nc-uiux-testing` for test harnesses and visual
evidence. Do not commit generated golden screenshots.

## Reference sources

- egui layer categories: <https://docs.rs/egui/0.37.0/egui/layers/enum.Order.html>
- egui modal behavior and backdrop: <https://docs.rs/egui/0.37.0/egui/containers/struct.Modal.html>
- Microsoft's Word object stacking: <https://support.microsoft.com/en-us/word/move-wordart-a-text-box-or-shape-in-word>
- Microsoft's Word text wrapping and front/behind behavior: <https://support.microsoft.com/en-us/word/wrap-text-and-move-pictures-in-word>
- Microsoft's OneNote drawing tools and ink selection: <https://support.microsoft.com/en-us/onenote/onenote-help-and-learning/learn-more-about-drawing-tools>

Microsoft's OneNote user documentation describes drawing, selecting, and editing ink. This skill
does not claim that OneNote defines the same universal UI-layer table as Word's object arrange tools.
Treat product-specific behavior as a reference to verify, and treat this semantic stack as NoteCraft's
design contract.

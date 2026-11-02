# 0.0.8 issue: inline image text-flow and caret interaction

## Status

**Status:** Active. Automated tests pass, but manual behavior is not yet
accepted. The owner reported that the caret can disappear and that an inline
image can appear selected during arrow-key navigation.

**Priority:** P0/P1 interaction regression. Inline images are part of the text
flow and must not break editing of the paragraph that owns them.

**Owner:** `nc-uiux-features`, with support from `nc-uiux-controls`,
`nc-uiux-bidi`, `nc-dom-ser`, and `nc-code-rust`.

**Related design reference:** [`work/inline-images-and-anchors.md`](../../../../work/inline-images-and-anchors.md)

**Related release handoff:** [`p2_issues.md`](./p2_issues.md)

## User-visible requirement

An inline image behaves like one atomic character in the text flow. It is not
the image's alt text, Markdown syntax, or a separate floating canvas item.

For a paragraph such as:

```text
Text before [inline image] text after
```

the editor must expose these caret boundaries:

```text
Text before | [image] | text after
```

The image has one logical position between its two boundaries. Moving across
it must not expose an intermediate image character, select the image
accidentally, remove focus, or create a new block.

## Required navigation behavior

### LTR

- Right Arrow from immediately before the image moves directly immediately
  after the image.
- Left Arrow from immediately after the image moves directly immediately
  before the image.
- One unmodified arrow press must never leave the image visibly selected.
- The caret remains visible and focused in the owning paragraph.
- A second arrow continues into the next or previous text character.
- Shift+Right and Shift+Left may intentionally select the image as one object.

### RTL and mixed direction

- The same atomic rule applies in RTL paragraphs.
- Left and Right follow the editor's visual navigation convention while
  preserving logical text offsets.
- Mixed Arabic/Hebrew and Latin text must not make the caret ping-pong,
  disappear, or land inside the image object.
- Direction must not change the persisted image anchor or source.

### Other navigation

- Home and End go to the correct visual line boundaries.
- PageUp and PageDown preserve normal visual caret behavior across lines.
- Clicking text before or after the image places the caret in the text flow.
- Clicking the image uses the defined image-selection behavior, not a new block.

### Whole-block image navigation contract

The same atomic-character rule applies to a standalone `Block::Image` in the
page stream. In LTR text, the image has three transient navigation states:

```text
caret before image -> image selected -> caret after image
```

- Right Arrow from the caret immediately before the image selects the image.
- Right Arrow from the selected image clears the selection and places a focused
  caret immediately after the image. If the image is the last block, that caret
  must allow typing a new line.
- Left Arrow from the selected image places the caret immediately before it.
- Up and Down use the image's visual rectangle as one large atomic character:
  entering it from an adjacent writable line selects it; leaving a selected
  image goes to the nearest writable line in the requested direction.
- No arrow sequence may leave the caret invisible, move the caret into the
  non-writable image block, or trap keyboard navigation.
- The image source, dimensions, and persisted block order are unchanged by
  navigation alone.
- Clicking inside the owning block must never create another block.

## Editing and selection behavior

- Typing immediately before or after the image keeps the image in the same
  paragraph and at the same relative text-flow position.
- Selecting text across the image selects one atomic image unit, not the
  image's alt text or serialized Markdown.
- Deleting a selected range removes the selected image and/or text according to
  the existing selection rules.
- Delete or Backspace must not remove the containing block accidentally.
- An image selected by an explicit image gesture can be deleted as one object.
- Undo and redo restore the text and image atomically.
- A standalone `Block::Image` remains a separate object interaction path. It
  must not be confused with an inline `Span.image`.

## Persistence and format requirements

The internal caret representation may use an object-replacement character or
another logical token, but that token must never be written to the page file.

The persisted representation must retain:

- image source or attachment path;
- image alt text;
- image span position relative to surrounding text;
- owning paragraph/block;
- inline versus floating mode where supported;
- dimensions and anchor metadata where supported.

Both formats must preserve equivalent semantics:

- HTML inline image round trip;
- CMGFM inline image round trip;
- HTML/CMGFM semantic parity;
- relative asset paths;
- multiple images in one paragraph;
- image at the start, middle, and end of a block.
- standalone image at the start, middle, and end of the page stream;
- Right, Left, Up, and Down from before, on, and after a standalone image;
- image as the final block followed by Enter and typed text;

## Observed failure

The owner reported this sequence:

1. Place the caret immediately before an inline image.
2. Press the next navigation key.
3. The image appears selected.
4. The following navigation key places the caret after the image, or reaches
   the end of the block.
5. In some cases the visible caret disappears and the next action behaves like
   block insertion or block navigation.

The expected result is a single direct transition from the boundary before the
image to the boundary after it. The image must not become an accidental
intermediate selection.

The screenshot associated with the report appeared to show an image object
selection affordance. Confirm whether the tested object is an inline
`Span.image` inside a paragraph. A standalone `Block::Image` has a different
selection and container path.

## Current implementation notes

The current editor work:

- converts inline image spans to one internal logical `U+FFFC` unit;
- restores the original image Markdown when writing the block back;
- intercepts unmodified Left/Right movement before `egui::TextEdit` can
  process the replacement character;
- includes unit tests for logical image mapping and arrow-target calculation.

This is not sufficient for acceptance until the owner confirms the visual
caret behavior in the running application. Automated UI tests currently cover
the broader editor and RTL suites, but dedicated end-to-end image-navigation
coverage should still be added.

## Required regression tests

### Pure editor/model tests

- An inline image maps to exactly one logical caret unit.
- The original source and alt text are restored on save.
- Text inserted before or after the image preserves the image span.
- Tags and inline formatting survive the mapping.
- Removing the logical image unit removes the image span.
- Multiple inline images preserve order.

### UI tests

- LTR Right Arrow crosses an image in one press.
- LTR Left Arrow crosses an image in one press.
- RTL visual Left/Right crosses an image in one press.
- Mixed-direction text keeps the caret visible.
- Home and End with an image on the line.
- PageUp and PageDown across an image-containing line.
- Shift+Arrow intentionally selects the image.
- Delete and Backspace remove an explicitly selected image/range.
- Typing on both sides preserves the image.
- Clicking before, on, and after the image has distinct expected behavior.
- Clicking inside the paragraph does not create another block.
- Save, reload, undo, and redo preserve the image and surrounding text.

### Format/parity tests

- HTML inline image round trip.
- CMGFM inline image round trip.
- Cross-format semantic parity.
- Asset path and alt-text preservation.

## Manual verification procedure

Run from the repository root:

```powershell
.\run.cmd
```

Prepare a paragraph with visible text on both sides of an inline image.

1. Click immediately before the image.
2. Press Right Arrow once.
3. Confirm the caret is immediately after the image.
4. Confirm the image is not selected.
5. Press Left Arrow once.
6. Confirm the caret is immediately before the image.
7. Type text before and after the image.
8. Repeat in an RTL paragraph.
9. Test Home, End, PageUp, PageDown, Shift+Arrow, Delete, and Backspace.
10. Save or move to another page, return, and confirm the image remains in
    the same text-flow location.

Record the exact key sequence and a screenshot if the caret disappears. Also
record whether the image is inline text content or a standalone image block.

## Verification history

The following checks passed before this issue document was created:

- `cargo fmt --all -- --check`
- `cargo clippy -p notecraft-ui-egui --all-targets -- -D warnings`
- `cargo build --release`
- `cargo test -p notecraft-ui-egui --lib pageedit::tests::inline_image`
- `cargo test -p notecraft-ui-egui --test rtl_quality`
- `cargo test -p notecraft-ui-egui --test ui -- --test-threads=1`
- `git diff --check`

The full UI suite result was 123 passed and 0 failed. These results do not
replace manual confirmation of the visual caret and image-selection behavior.

## Out of scope

- Image resize behavior and issue #18, owned by Agent 1.
- General floating-image redesign unless required to preserve the inline
  anchor contract.
- Broad editor redesign unrelated to inline image navigation.


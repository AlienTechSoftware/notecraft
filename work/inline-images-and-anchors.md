# Inline images and anchored embedded objects

Status: design reference, implementation not complete
Date: 2026-10-24

## Product behavior

NoteCraft should treat an inserted picture as an atomic document object. A picture inserted in a
paragraph participates in text flow and moves with its surrounding text. A picture can also be
selected and dragged to a different location. A floating picture keeps its text anchor while
storing independent page position, size, and stacking order.

This follows the documented OneNote behavior that pictures can be inserted in the middle of a
sentence and dragged elsewhere on the page. The OneNote HTML model represents images as inline
HTML resources in page content.

References:

- https://support.microsoft.com/en-us/onenote/onenote-for-mac-help-and-learning/insert-and-format-pictures-in-onenote-for-mac
- https://learn.microsoft.com/en-us/graph/onenote-input-output-html
- https://github.com/icmric/openote/blob/master/docs/specs/11-data-model-spec.md

## Required model

Each embedded object needs:

- stable attachment/resource reference;
- owning text block or paragraph anchor;
- inline position when it participates in text flow;
- layout mode: `inline` or `floating`;
- floating x/y position and size;
- stacking order for floating objects;
- alt text and media kind.

Existing pages containing Markdown or HTML image spans remain readable. The first implementation
may derive an implicit anchor from the existing span position, but it must not discard that
relationship when the object is selected, moved, saved, or reloaded.

## Interaction contract

- Click an image selects the image atom, not the whole page container.
- Dragging a selected image moves it and preserves its anchor.
- Overlapping floating images are hit-tested front to back.
- A deliberate alternate gesture, such as Alt-click or repeated click, reaches lower layers.
- Delete and Backspace remove the selected object only.
- Reload restores object position, size, anchor, and order.
- HTML and CMGFM preserve equivalent object behavior.

## Required verification

Add tests for:

- inline image between text runs;
- standalone image block;
- two overlapping floating images;
- selecting and dragging the back image;
- delete/backspace of the selected image;
- save and reload preserving the arrangement;
- HTML/CMGFM semantic parity.

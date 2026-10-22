---
name: nc-dom-ser
description: Use when changing NoteCraft page content, its block model, HTML or CMGFM parsing/rendering, page storage, or the UI-to-file round trip.
---

# NoteCraft page model, DOM, and serialization

Keep one semantic page across the live editor, the HTML body, CMGFM, and the files beside a page. Use this skill before changing any boundary between those representations.

## Contract to read first

- [Page model](../../docs/dom_serialization/page-model.md) is the normative contract for content, boxes, live view, code view, assets, and ink.
- [Folder format](../../docs/folder-format.md) defines the persisted HTML, CMGFM, metadata, and folder structures.
- [Architecture](../../docs/architecture.md) explains crate ownership and runtime flow.
- [Testing](../../docs/testing.md) lists the required repository gates.

If code and these contracts disagree, identify the gap and update the contract with the implementation. Do not silently add a second representation or make one format special.

## Representation boundaries

NoteCraft's editor is egui; it does not edit a browser DOM. The word *DOM* here means the HTML document structure that is serialized to `index.html`. `notecraft_formats::markdown::{Block, Span}` is the shared semantic content model used by both file formats and the editor. `notecraft_model` holds shared notebook metadata and layout types such as `Container`; it is not the rich-text block model.

Trace the change through every applicable boundary:

1. **Persisted page:** HTML head/body or CMGFM front matter/body, plus files owned by the page.
2. **Reader:** `store::page` separates metadata and body; `formats::boxes::read` reads box structure and delegates content to HTML/Markdown readers.
3. **Semantic model:** the same ordered `Vec<Block>` and inline `Span`s regardless of source format; `Container`s record page placement and block membership.
4. **Editor/view:** editing and drawing must preserve the model fields. Page name, direction, justify strengths, and table column widths have separate documented persistence contracts.
5. **Writer:** `formats::boxes::write` and `formats::render` serialize the same semantic content into the selected page format; store saves it while preserving unrelated metadata and files.
6. **Reload:** reading the saved result must recover the same semantic content, layout, and owned-file references.

Attachments and ink are files, not extra UI-only content. The page body names attachments and ink; ink's strokes live in SVG. Follow their separate page-model contracts.

## Change procedure

1. Read the relevant code and tests named in `docs/dom_serialization/page-model.md` and `docs/folder-format.md`.
2. Write down which model fields, markup constructs, metadata, and companion files the change affects. Check both HTML and CMGFM behavior.
3. Keep parsing and serialization in `crates/formats`, shared metadata types in `crates/model`, file access and head preservation in `crates/store`, and app behavior in the UI/engine layers.
4. Add or extend tests at the lowest useful boundary and at the integrated page boundary. Assert semantic equality, not just that output contains a tag or label.
5. For content changes, verify the complete HTML and CMGFM demo editions remain structurally and semantically paired.
6. Run `cargo xtask ci`. Its workspace test step includes the model/format round trips and complete demo parity test; its demo check prevents generated output from drifting from authored content.
7. Update `docs/dom_serialization/page-model.md`, `docs/folder-format.md`, and `docs/testing.md` when a persisted contract or its guarantees change.

## Required invariants

- A supported page saved without edits reads back to the same blocks, spans, containers, and persisted page attributes.
- HTML and CMGFM encode the same meaning. A format conversion may change syntax, never silently drop meaning.
- Unknown or unsupported markup is preserved or made explicitly read-only; never permit a lossy live edit to autosave over it.
- Every visible page-owned resource resolves to a named file, and the file list agrees with the page's links and ink rules.
- Add a round-trip regression test for every new model or serialization feature, including cross-format behavior when both formats support it.

# WordCraft and NoteCraft: document models compared (2026-11-12)

Sources:
- NoteCraft: `crates/formats/src/markdown.rs` (`Span`, `Block`), `docs/dom_serialization/page-model.md`,
  `SKILLS/nc-dom-ser/SKILL.md`.
- WordCraft: `crates/doc/src/block.rs`, `crates/doc/src/para.rs` (`Paragraph`), `crates/doc/src/para/run.rs`
  (`Run`), `crates/doc/src/para/inline_object.rs` (`InlineObject`), `crates/doc/src/position.rs` (`Pos`),
  `crates/doc/src/lib.rs` (`Document`).

## Side by side

| Concern | NoteCraft | WordCraft |
|---|---|---|
| Text container | `Paragraph(Vec<Span>)`: each block owns its styled spans | `Paragraph { text: String, runs, objects, props, mark }`: one text string per paragraph |
| Formatting unit | `Span { text, bold, italic, strike, underline, script, code, math, font_size, font_family, link }` | `Run { len, props: CharProps }`: a byte length over the paragraph text, with a full character property set |
| Formatting storage | flags on each span, written into the file per span | properties in a shared style sheet; runs refer to them |
| Inline objects | images are separate blocks (`Block::Image`) | objects sit in the text, marked by `OBJ` (U+FFFC); `InlineObject::Image`, fields, controls |
| Breaks | line and page breaks are parts of the text or separate blocks | `LINE_BREAK` (U+000A), `PAGE_BREAK` (U+000C) in the text |
| Block kinds | Heading, Paragraph, Item, Quote, Code, MathBlock, Table, Rule, Image: one enum | `Block::Para` and `Block::Table`; heading and list are paragraph properties |
| Position | a block index and a character offset in that block's text (`ui.caret`, `Caret { block, text, selection }`) | `Pos { story, path, off }`: a story, a path to the paragraph or cell, and a byte offset |
| Nesting | tables hold spans per cell; no nested boxes | tables and text boxes nest (`MAX_TEXT_BOX_DEPTH = 4`) |
| Sharing | blocks are owned values | blocks are `Arc<Block>`; undo copies only what changes |
| File format | HTML and CMGFM, one page per folder, both formats must match (`nc-dom-ser`) | DOCX (OOXML), with parts keyed by number |

## What this means

1. **Our caret is per block; theirs is per paragraph stream with runs and objects inside it.** A caret
   in WordCraft steps across run and object boundaries without special cases, because they are offsets
   into one string. In NoteCraft, each block is a unit, so a move across a block edge needs its own rule
   for every block kind. This is the root of the Up, Down and Left landing bugs.
2. **Our formatting is coarse.** Flags per span can express what we ship today. They can't express
   shared character styles, or a run-level change that doesn't create a new span. Changing a style on a
   part of a word means splitting spans in our model, and the file grows.
3. **Inline objects are blocks for us.** An image in the middle of a sentence is a block break. In
   WordCraft it is one character in the text.
4. **Positions are different kinds of address.** Ours is `(block, char)`, with the char counted in
   characters. WordCraft's is `(story, path, byte)`, with the byte counted in UTF-8 bytes and snapped to
   clusters by the layout. Any shared code must agree on one of these, and convert at the edge.

## Options for NoteCraft

- **A. Keep our model.** Fix the caret rules per block kind. Costs little now, and it's where the
  current bugs come from.
- **B. A single text stream per paragraph, with runs and object markers,** kept in our own model and
  written to both formats. This solves the caret problem at its root. It changes the page file, the
  parity rules, and every reader and writer. It needs an owner decision and an ADR.
- **C. Adopt WordCraft's document crate.** Not recommended: it models DOCX, and its story, path and
  style sheet are not the NoteCraft folder format.

## Decision needed

Owner: choose A now and B later, or B now. Until then, new caret and text code should keep positions as
`(block, char)` and not assume a block boundary is a text boundary, so that option B stays open.

## Related

- `nc-dom-ser`: the page model and the parity rules that option B would change.
- `nc-uiux-bidi`: the caret model section and `references/caret-options.md`.
- `nc-design-artcraft-integration`: the reuse and integration guidance this comparison supports.

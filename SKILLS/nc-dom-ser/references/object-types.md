# Object types: identifiers, names and room to grow (2026-11-12)

Status: proposal for owner review. Nothing here changes the file format until an ADR accepts it.

## Naming rules

1. **Identifier.** Every object type has a stable identifier in the form `family.kind`, lower case, dot
   separated: `inline.image`, `block.table`, `onenote.ink`. The identifier is what the file stores. It
   never changes once written. A renamed type gets a new identifier and the old one stays readable.
2. **Rust name.** PascalCase, the same word WordCraft uses where WordCraft has the object (`Image`,
   `Field`, `Equation`, `Opaque`). Where WordCraft has no object, use the OneNote word, or a plain English
   one.
3. **Family.** `block` for a whole block, `inline` for an object inside a paragraph's text, `container`
   for a box, `onenote` for a type that exists because of OneNote import or OneNote's model, `ext` for a
   type from outside NoteCraft (see Future room).
4. **Paired markers.** A start and an end marker share a family and end in `.start` and `.end`, as
   WordCraft's `FieldStart`/`FieldEnd` do.
5. **No format-specific names.** The identifier does not say HTML or CMGFM. Each format has a mapping
   column below.

## The table

| Identifier | Rust name | WordCraft | OneNote | NoteCraft today | HTML | CMGFM | Layer |
|---|---|---|---|---|---|---|---|
| `block.paragraph` | `Paragraph` | `Block::Para` | paragraph | `Paragraph` | `p` | text | model, formats |
| `block.heading` | `Heading` | paragraph style | heading 1-6 | `Heading` | `h1`-`h6` | `#` | model, formats |
| `block.item` | `Item` | list paragraph | bullet, number, to-do | `Item` | `ul`, `ol`, `li` | `-`, `1.`, `- [ ]` | model, formats |
| `block.quote` | `Quote` | paragraph style | quote | `Quote` | `blockquote` | `>` | model, formats |
| `block.code` | `Code` | paragraph style | code (OneNote has no code block) | `Code` | `pre` | fence | model, formats |
| `block.math` | `MathBlock` | `inline.equation` in a block | equation | `MathBlock` | `p.math` | `$$` | model, formats |
| `block.rule` | `Rule` | paragraph border | line | `Rule` | `hr` | `---` | model, formats |
| `block.table` | `Table` | `Block::Table` | table | `Table` | `table` | GFM table | model, formats |
| `block.container` | `Container` | text box | box and text box | `Container` (`div.nc-box`) | `div.nc-box` | `div.nc-box` | model, formats |
| `inline.image` | `Image` | `InlineObject::Image` | picture | `Image` block (not inline yet) | `img` | `![]()` | model, formats |
| `inline.graphic` | `Graphic` | `InlineObject::Graphic` | picture in text | not yet | `img` | not yet | model |
| `inline.shape` | `Shape` | `InlineObject::Shape` | drawing shape | not yet | `svg` | not yet | model |
| `inline.group` | `Group` | `InlineObject::Group` | group of drawings | not yet | `g` | not yet | model |
| `inline.field` | `Field` | `InlineObject::Field` | date, page number | date stamp (UI only) | `span.field` | not yet | model |
| `inline.field.start` | `FieldStart` | `FieldStart` | n/a | n/a | n/a | n/a | model |
| `inline.field.end` | `FieldEnd` | `FieldEnd` | n/a | n/a | n/a | n/a | model |
| `inline.control` | `Control` | `ControlStart`/`ControlEnd` | n/a (OneNote has none) | not yet | `span.control` | not yet | model |
| `inline.bookmark.start` | `BookmarkStart` | `BookmarkStart` | n/a | n/a | `a#name` | `<a id>` | model |
| `inline.bookmark.end` | `BookmarkEnd` | `BookmarkEnd` | n/a | n/a | n/a | n/a | model |
| `inline.comment.start` | `CommentStart` | `CommentStart` | n/a | n/a | not yet | not yet | model |
| `inline.comment.end` | `CommentEnd` | `CommentEnd` | n/a | n/a | not yet | not yet | model |
| `inline.equation` | `Equation` | `InlineObject::Equation` | equation | inline math `Span` | `span.math` | `$...$` | model, formats |
| `inline.ruby` | `Ruby` | `InlineObject::Ruby` | n/a | not yet | `ruby` | not yet | model |
| `inline.tab` | `PositionalTab` | `InlineObject::PositionalTab` | tab | not yet | `span.tab` | tab char | model |
| `inline.break` | `Break` | `LINE_BREAK`, `PAGE_BREAK` | line break, page break | line break in text | `br` | line break | model, formats |
| `onenote.ink` | `Ink` | n/a | ink drawing | `ink.svg` per page | `ink.svg` link | `ink.svg` link | model, formats |
| `onenote.tag` | `Tag` | n/a | tag (to-do, important, question, and so on) | `Tag` (model) | `span.tag` | `tag` marker | model, formats |
| `onenote.attachment` | `EmbeddedFile` | n/a | embedded file | attachment in `assets/` | `a` | `[]()` link | model, formats |
| `onenote.pagelink` | `PageLink` | n/a | link to a page | not yet | `a` | `[]()` | model, formats |
| `onenote.stamp` | `PageStamp` | n/a | date and time under the title | drawn stamp (UI only) | `p.stamp` | not yet | model, formats |
| `onenote.audio` | `EmbeddedAudio` | n/a | embedded audio | not yet | `audio` | not yet | model, formats |
| `onenote.video` | `EmbeddedVideo` | n/a | embedded video | not yet | `video` | not yet | model, formats |
| `onenote.printout` | `Printout` | n/a | printout | printout page (`pdf.printout-NNN.png`) | `img` | `![]()` | model, formats |
| `ext.opaque` | `Opaque` | `InlineObject::Opaque` | any object we cannot show | not yet | `span.opaque` with its original XML | same | model |

Notes on the table:
- **Not yet** means no type in the model. Adding one is a change to this table first, then the model,
  then both formats.
- **n/a** means the source has no such object, or the marker is not needed in that format.
- The OneNote rows are the types NoteCraft already imports or needs for OneNote parity. OneNote's
  own names are used where they exist.

## Future room

- **Open kinds.** Readers keep an unknown object as `ext.opaque`, with its identifier and its original
  content. An unknown kind is never dropped on save. This is WordCraft's `Opaque` rule.
- **Versioned object payloads.** Each object payload carries a version. A reader that meets a newer
  version keeps the object as opaque.
- **Namespaces.** `ext.<vendor>.<kind>` is reserved for types that come from another app (for example
  `ext.artcraft.chart`). They round-trip the same way as opaque objects until NoteCraft implements them.
- **Inline or block.** An object is either inline (a character in the text) or a block. Adding a type
  means choosing one. The table says which.

## Checks before a new object type lands

1. The identifier is in this table, with its mapping for HTML and CMGFM.
2. A round-trip test: HTML to model to HTML, and CMGFM to model to CMGFM, with the object unchanged.
3. Cross-format parity (`cargo test -p notecraft-formats --test format_parity`).
4. An opaque fallback test: a file with the object read by a build that doesn't know it.
5. The page model doc (`docs/dom_serialization/page-model.md`) names it.

## Decision needed

Owner: accept the identifier scheme and the table as the basis for the next format change, and decide
which rows come first. Suggested first set: `inline.image`, `inline.field`, `inline.equation`,
`inline.break`, `onenote.ink`, `onenote.tag`, `onenote.attachment`, `onenote.pagelink`, `ext.opaque`.

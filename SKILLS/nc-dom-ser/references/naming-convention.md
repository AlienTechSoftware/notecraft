# Naming convention for page objects (proposal, 2026-11-12)

Status: proposal for owner review. It applies to new object types and to the table in
`object-types.md`. It does not rename anything already in a file.

## Terms

"DOM" in this repo means the semantic page model, not the browser DOM (see
`docs/dom_serialization/README.md`). WordCraft's `crates/doc` uses overlapping names (`Block`, `Frame`,
`ListItem`) with different meanings, so every name in this convention is NoteCraft's own. Research
context: `dom-naming-context.md` (untracked).

## Why

Neither WordCraft nor craft-libs has a written naming scheme for its document model. WordCraft's names
are the Rust type names in `crates/doc`, which are good, but they don't say how a name is stored in a
file. NoteCraft stores pages as HTML and CMGFM, so every name must work in both formats and never change
once written. This convention fixes that.

## The three names of one object

Each object type has three names. They are kept in one place (`object-types.md`).

| Name | Rule | Example (`inline.image`) |
|---|---|---|
| Identifier | `family.kind`, lower case, dot separated, stored in the file | `inline.image` |
| Rust name | PascalCase, WordCraft's word where it has one | `Image` |
| Format form | the HTML tag and class, and the CMGFM syntax, from the table | `img`, `![alt](src)` |

The identifier is the contract. The Rust name and the format form may change with the code; the
identifier may not.

## Rules

1. **Lower case, dot separated, no spaces.** `inline.image`, not `Inline-Image` or `image`.
2. **Families are fixed:** `block`, `inline`, `container`, `onenote`, `ext`. A new family needs an ADR.
3. **Kind words are singular nouns**, taken from WordCraft where it has the object (`image`, `field`,
   `equation`, `control`, `bookmark`, `comment`), otherwise from OneNote (`ink`, `tag`, `pagelink`), and
   otherwise a plain English noun.
4. **Start and end markers** share the kind and add `.start` or `.end`: `inline.bookmark.start`.
5. **No format words in identifiers.** `html`, `md`, `cmgfm`, `css` do not appear. The format forms
   live in their own column.
6. **Class names follow the identifier.** The HTML class for an object is the identifier with dots
   turned into hyphens and the prefix `nc-`: `inline.image` becomes `nc-inline-image`. The `nc-`
   prefix keeps our classes apart from the reader's own CSS.
7. **Data attributes use the same words.** `data-nc-kind="inline.image"`, so a reader can check the kind
   without parsing the class.
8. **Opaque and extension names are reserved.** `ext.opaque` is the fallback. `ext.<vendor>.<kind>` is
   reserved for types from other apps; the vendor is a short lower-case name, for example `artcraft`.
9. **Never reuse an identifier.** A removed type keeps its identifier in the table, marked retired, so an
   old file still reads.
10. **Renames are new identifiers.** The old one is kept as an alias in the reader until a migration
    exists. The alias is written down in `object-types.md`.

## Worked example

Adding an inline callout (a highlighted note inside a paragraph):

- Identifier: `inline.callout`
- Rust name: `Callout` (no WordCraft equivalent; OneNote has no such object, so the plain noun is used)
- HTML: `<span class="nc-inline-callout" data-nc-kind="inline.callout">…</span>`
- CMGFM: no native syntax, so the object is written as a `ext.opaque`-style HTML fragment in CMGFM and
  listed in the parity exceptions with its reason. Parity exceptions need an owner decision.
- Table row, round-trip test, parity check, opaque fallback test: as in `object-types.md`, "Checks
  before a new object type lands".

## Checks

1. The identifier follows rules 1 to 5 and is in `object-types.md`.
2. The class and data attribute follow rules 6 and 7, and a test reads them back.
3. A retired or renamed identifier has its alias row.

## Decision needed

Owner: accept this convention as NoteCraft's naming rule for page objects, and decide who owns the
`object-types.md` table (suggested: `nc-dom-ser`).

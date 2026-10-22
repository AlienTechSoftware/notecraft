# DOM and serialization

Status: started 2026-10-22. This folder collects the design documents for how NoteCraft's page
content model relates to the formats it is saved in. The normative page contract is [page-model.md](page-model.md); the persisted layout is
[../folder-format.md](../folder-format.md). Documents here explain, propose and record decisions,
and are folded into those contracts when they are agreed.

## The principle

NoteCraft's editor is egui and edits no browser DOM. "DOM" here means the **semantic page model**:
an ordered `Vec<Block>` of `Span`s (`notecraft_formats::markdown`), plus the positioned `Container`s
(`notecraft_model`). Every page format is only a serialization of that one model.

1. **One model, many formats.** HTML and CMGFM encode the same meaning. Converting may change
   syntax, never meaning, and no format is special.
2. **The model owns the meaning, the format owns the syntax.** Parsing and rendering live in
   `crates/formats`; file access and head preservation in `crates/store`; behavior in the UI and
   engine layers.
3. **Round trip.** A supported page saved without edits reads back to the same blocks, spans,
   containers and page attributes.
4. **Nothing silently lost.** Unknown or unsupported markup is preserved or made read-only; a
   lossy live edit must never autosave over it.
5. **Pages own their files.** Pictures, attachments and ink strokes are files beside the page,
   named in the body.

## Where it lives in the code

| Boundary | Code |
|---|---|
| Model types | `crates/formats/src/markdown.rs` (`Block`, `Span`), `crates/model/src/container.rs` (`Container`) |
| File to model | `formats::boxes::read` (`markdown::parse`; HTML goes through `html::to_markdown` first) |
| Model to file | `formats::boxes::write`, `render::blocks_to_html`, `render::blocks_to_markdown` |
| Storage | `crates/store/src/page.rs` |
| Editor | `crates/ui-egui/src/app.rs`, `pageedit.rs` |
| Tests | `crates/formats/tests/format_parity.rs`, the demo parity test |

The agent skill `SKILLS/nc-dom-ser/SKILL.md` is the change procedure.

## Documents

| Document | Status |
|---|---|
| [page-model.md](page-model.md): the normative page, view and storage contract | Normative |
| [code-blocks.md](code-blocks.md): the two code styles and the `123|` name | Built |

Open questions to turn into documents:

- HTML is read by flattening to Markdown text, not by an HTML-to-`Block` parser. What does that lose?
- `Item` is flat with a `depth`; the model has no nested lists, nested quotes or lists in quotes.
- Per-paragraph direction, alignment and other block attributes: where they live in the model and
  in each format (see the right-to-left bar).
- A fidelity matrix: each model field against its HTML and CMGFM spelling.

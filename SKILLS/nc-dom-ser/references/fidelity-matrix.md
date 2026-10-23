# Page fidelity matrix

Use this matrix to decide which boundaries and checks a change needs. It is a
routing aid, not a replacement for the normative page model.

| Change affects | Inspect | Minimum proof |
|---|---|---|
| Block or span meaning | `crates/formats/src/markdown.rs`, parser, renderers | model round trip in both formats |
| HTML syntax | HTML reader and `render.rs` | HTML parse/write plus format parity |
| CMGFM syntax | Markdown parser and writer | CMGFM parse/write plus format parity |
| Containers or placement | `crates/model`, `formats::boxes`, page editor | layout round trip and page test |
| Page head or metadata | `crates/store/src/page.rs`, folder format | save/reload with unrelated head preserved |
| Attachments or ink | page body links, assets, `ink.svg`, code view | file-list agreement and reload |
| Editor behavior | `crates/ui-egui`, engine commands | integrated UI/control test and reload |
| Code view | source pane and file listing | visible file ownership and edit round trip |
| Demo content | authored content and generated demo | `demo-check`, semantic parity, visual side-by-side |

## Questions before editing

1. What is the canonical semantic value?
2. Where is it serialized in HTML and CMGFM?
3. Which metadata or companion file carries it?
4. Can another program read the page without NoteCraft and still see the data?
5. What happens to unknown input or a format that cannot represent it?
6. Which test proves read, write, reload, and cross-format behavior?

# Caret research: how the Craft apps move the caret (2026-11-12)

Sources read, with the place each fact came from:

- `storytold/wordcraft`: `crates/engine/src/cmd/caret.rs` (commands), `crates/layout/src/hit.rs`
  (`vertical`, hit testing), `crates/doc/src/cluster.rs` (cluster rules), `docs/typing-parity.md`
  (Editing keys table, Complex scripts, Text direction).
- `storytold/craft-libs`: `crates/text/README.md` ("shaping, glyph mirroring and caret placement stay
  in each application"). No shared caret or editing crate was found.
- `storytold/craft-wiki`: user-facing app docs only. No caret design was found.

## What WordCraft does

1. **The caret is a logical position.** A `Pos` names a story, a path and a character offset. Every
   command takes and returns a `Pos`. The UI only draws it.
2. **Commands live in the engine.** `caret.left`, `caret.right`, `caret.up`, `caret.down`,
   `caret.wordLeft` and the rest are registered commands with key bindings and an `extend` flag. They
   run without a window, so they can be tested and driven by the control channel and MCP.
3. **Horizontal moves step by clusters.** Left and Right step by user-perceived characters (extended
   grapheme clusters, UAX #29), and `cluster.rs` adds the rules for Myanmar, Khmer and Tai Tham, so a
   caret never stops inside a stacked consonant.
4. **Vertical moves use the layout and a goal column.** `Layout::vertical(pos, goal_x, dir,
   page_hint)` finds the line above or below, keeps the goal x (`s.goal_x`) across consecutive Up and
   Down presses, and clears it on any other move. Moving into a table picks the nearest cell row.
5. **Edges are reported, not guessed.** `VisualStep::Edge { start, stop, last_line, rtl }` tells the
   command that a move hit a line end, and whether the line is right-to-left, so Left and Right at a
   line end move to the next line in the right direction.
6. **Word and line moves depend on paragraph direction.** In a right-to-left paragraph the "word to the
   left" is the next word (`rtl_para`). On macOS, ⌘← and ⌘→ go to the line ends.
7. **The rules are written down and tested.** `docs/typing-parity.md` lists each key, Word's result,
   and the test that checks it.

## What NoteCraft does today

- The page editor draws each text block with egui's text field. egui owns the caret, the line layout
  and the hit testing. Our code reads and sets the caret through `Caret { block, text, selection,
  place }`, and the text field can ignore a placement or restore its own cursor a frame later. That is
  the cause of the landing bugs: two owners of the caret.
- `crates/text` (parley) has shaping, bidi and a caret for Arabic, but the page editor does not use it
  for caret movement.
- Code blocks are drawn as one text field per block, so Up and Down cross block edges in our code,
  not in a layout.

## Conclusion

WordCraft's approach works because one model owns the caret and the layout answers the questions.
NoteCraft has two owners. Copying WordCraft's engine would be large. Copying its rules is cheap.

See `caret-options.md` for the options.

# Caret options for NoteCraft (for owner review, 2026-11-12)

Background: `caret-research.md`. The problem is two owners for the caret: our code and egui's text
field.

## Option A: keep egui's text field and patch the paths

- **What:** keep each block as an egui `TextEdit`. Fix placements and edge moves case by case.
- **Pros:** no new code, no new dependencies, the current tests keep running.
- **Cons:** this is where we are now. Each fix has moved the bug to another path (see the history
  of the edge tests). egui restores its own cursor a frame later, so placements get lost.
- **Effort:** low for each fix, high in total, and it doesn't reach a stable state.
- **Verdict:** stop here unless the owner wants only the current behaviour.

## Option B: a logical caret model with a layout (WordCraft's rules, our code)

- **What:** the page holds one `Pos` (block, character offset). Commands take and return a `Pos` and
  live in `pageedit` or a new `caret` module without egui. Layout (parley, already in `crates/text`)
  maps a `Pos` to a line and an x, and back. egui only paints the text and the caret.
- **Pros:** one owner. The rules from WordCraft's `typing-parity.md` can be copied as tests. Commands
  are testable without a window. Right-to-left and mixed text come from parley's bidi output.
- **Cons:** the editor draws text itself, or pins egui's galley to our offsets. Painting is still an open
  question in `crates/text/src/lib.rs`. This is a large change.
- **Dependencies:** parley is already in the repo. HarfBuzz and ICU are not needed directly: parley
  uses HarfRust and the Unicode bidi algorithm. Adding either would need a CHECKPOINT.
- **Effort:** a few weeks of focused work. Needs an estimate and the owner's nod.
- **Verdict:** the recommended direction if the owner wants correct caret behaviour in every block.

## Option C: adopt WordCraft's engine or layout crates

- **What:** use `wordcraft-layout` and the engine's caret commands directly.
- **Pros:** the most complete behaviour, already tested.
- **Cons:** WordCraft's layout is for a page document with a different block model, and it would
  pull in its story model. Licensing and repo boundaries need review. We would be adopting another
  app's architecture.
- **Verdict:** not recommended now. Reuse its rules and test cases (Option B), not its code.

## Recommendation

Option B, after a short spike that answers two questions:
1. Can a `Pos` drive the egui text field without losing placements (for example, by owning the cursor
   state for every block and never letting egui keep its own)?
2. Can parley's layout give the line and x for a `Pos` in a code block and in a paragraph?

The spike is written as a failing test first, then code, under the usual gates.

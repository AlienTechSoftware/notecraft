# Draw ribbon: current state and target

Status: UX baseline recorded on 2026-10-25. This is a design reference, not a claim that every
button shown in the target is implemented.

## Why this matters

The Draw ribbon is the user's instrument panel. It should make the pen, pressure, selection,
paper, and ink actions legible at a glance. A technically correct ink engine is not enough if
the ribbon makes the drawing workflow feel unlike OneNote or hides the difference between a
working tool and a planned tool.

The target is the OneNote-like layout shown in the issue screenshots: compact, grouped, and
visually calm, with the drawing tools as the focal point. NoteCraft should become familiar to
someone who already knows OneNote without copying proprietary assets or source code.

## Current NoteCraft state

The current implementation is in `crates/ui-egui/src/ribbon.rs` (`draw_tab`). It already has
the right broad grouping:

- **Undo:** Undo stroke and a planned Redo stroke.
- **Selection:** Select, Lasso Select, and NoteCraft's Type mode.
- **Drawing Tools:** eraser, the configurable pen rack, Add, and sensitivity.
- **Input Mode:** a planned Panning Hand.
- **Stencils:** a planned Ruler.
- **Edit:** Draw here, Save picture, a planned Insert Space, and Format Background.
- **Shapes:** existing shape tools and a planned Automatic Shapes button.
- **Text / Math / Replay / Help:** visible planned actions.
- **Mode:** working Full Page View.

This honest "visible but not built" approach is useful for product planning, but it currently
creates a ribbon that is wider and less polished than the OneNote reference. Some controls are
placeholders, some labels wrap awkwardly, and the working controls do not yet have a consistent
visual hierarchy. The pen rack and sensitivity control need to remain prominent without making
the rest of the ribbon feel crowded.

## OneNote target

Microsoft's public documentation describes the Draw workflow as a grouped set of tools for
selection, pens/highlighters, erasing, shapes, ruler, ink-to-text, ink-to-math, and replay. It
also documents configurable pen colour and thickness, plus optional pressure sensitivity:

- [Draw and sketch notes in OneNote](https://support.microsoft.com/en-us/onenote/onenote-help-and-learning/draw-and-sketch-notes-in-onenote)
- [Learn more about Drawing Tools](https://support.microsoft.com/en-us/onenote/onenote-help-and-learning/learn-more-about-drawing-tools)
- [OneNote Options (Advanced)](https://support.microsoft.com/en-us/onenote/onenote-options-advanced)

The visual target is not pixel copying. Match the interaction language and information density:

1. Keep groups visually separated with restrained dividers and short captions.
2. Use large, consistent glyph-over-label controls for primary actions.
3. Keep the active tool unmistakable with themed selection feedback.
4. Keep dropdown affordances attached to the tool they configure.
5. Put the pen rack and pressure control first, because they are the main drawing decisions.
6. Keep planned tools discoverable, but make unavailable actions clearly disabled and explain
   why they are unavailable.
7. Make the ribbon scroll or adapt at narrow widths without clipping or overlapping controls.
8. Preserve keyboard focus, tooltips, accessible labels, and pointer hit targets.

## Improvement goals

Future Draw-ribbon work should be incremental and measurable:

- reduce unnecessary horizontal width while preserving readable hit targets;
- align icon size, label baselines, group spacing, and divider treatment;
- use the same theme tokens as the rest of the application in light and dark themes;
- make the selected pen, eraser, Select, Lasso Select, and sensitivity state easy to see;
- make the pen dropdown and sensitivity control feel like one coherent drawing-tool area;
- distinguish working controls from planned controls without making the toolbar look broken;
- add visual regression coverage for the full ribbon at normal and narrow window sizes;
- compare against the supplied OneNote reference screenshot during visual review.

Do not add decorative controls merely to fill space. Every visible button must either work,
explain its unavailable state, or be removed until the underlying command exists.

## Images and source boundaries

The supplied OneNote screenshot is the visual reference for this work. Do not commit copied
Microsoft artwork, proprietary icons, or screenshots unless their provenance and redistribution
rights are recorded. Prefer NoteCraft's code-drawn icons and theme tokens, plus links to public
Microsoft documentation. If a local comparison image is added later, record its source in
`ATTRIBUTION.md` and keep it in this skill's `references/` folder.

## Verification checklist

For any Draw-ribbon change:

- inspect the full Draw tab at the normal application size;
- inspect a narrow window and confirm scrolling/adaptation rather than clipping;
- test light and dark themes;
- test active, hover, disabled, dropdown, and keyboard-focus states;
- verify accessible labels and tooltips for every visible control;
- confirm the pen rack, sensitivity, Select, Lasso Select, eraser, Full Page View, and working
  shape actions still operate;
- run the UI tests and the complete `cargo xtask ci` gate.

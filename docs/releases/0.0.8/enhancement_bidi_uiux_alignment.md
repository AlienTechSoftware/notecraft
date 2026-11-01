# v0.0.8 Enhancement: Bidi UI and Ribbon Alignment

**Status:** Planned  
**Branch:** `feat/v0.0.8_bidi`  
**Base:** `81d78cf116818834f4bf3e22493a9729df3c11a1`  
**Scope:** Bidi-related ribbon controls, icons, labels, and page setup placement

## Purpose

Align NoteCraft's bidi controls with the visual language users already know from
WordCraft while preserving NoteCraft's OneNote-oriented behavior.

This is a UI and interaction alignment effort. It is not a request to copy
WordCraft assets or implementation. Icons must be recreated or adapted using
NoteCraft's own theme tokens, dimensions, and rendering conventions.

## Product position

Word exposes page setup primarily as a dialog and places section direction
inside page layout settings. NoteCraft is a note-taking application with
frequent page-level direction changes. Page direction and page setup therefore
remain directly visible in the ribbon instead of being hidden behind a dialog.

NoteCraft should follow OneNote's discoverability for common writing actions:

- Paragraph alignment and paragraph direction stay together.
- Page direction is a first-class ribbon action.
- Page setup controls remain available while editing a page.
- Direction controls are available without requiring a separate language-pack
  configuration step.

## Controls to align with WordCraft

### Paragraph direction

Use the WordCraft icon concept for paragraph direction as the visual reference:

- Left-to-right paragraph.
- Right-to-left paragraph.
- Direction from the words, or automatic direction.

The icons must use NoteCraft colors and theme tokens. They must work in light
and dark themes and remain legible at the existing ribbon icon size.

The controls belong with alignment, because that is where users expect
paragraph-level direction. Existing NoteCraft behavior already follows this
model.

### Text-run direction

Keep text-run direction separate from paragraph and page direction. It applies
to selected text or the next text typed inside a mixed-direction paragraph.

The control may remain in the compact direction menu. It must not be presented
as if it changes the whole page or paragraph.

### Indentation

Use the WordCraft terminology:

- **Increase indentation**
- **Decrease indentation**

Use the corresponding WordCraft icon concepts, with NoteCraft's icon stroke,
accent color, hover state, selected state, and disabled state.

The keyboard hints remain:

- `Tab` for increase indentation.
- `Shift+Tab` for decrease indentation.

The command behavior must remain NoteCraft's existing list and block
indentation behavior. This change is terminology and visual alignment, not a
change to the document model.

## NoteCraft-specific controls that must remain

### Page direction

NoteCraft has a page direction control that WordCraft does not have. It must
remain visible as its own action.

Page direction affects paragraphs that do not have their own direction mark.
A paragraph-level direction mark continues to override the page direction.
The available page choices are:

- Left-to-right.
- Right-to-left.
- Direction from the words, when supported by the page model.

The page direction control must not be merged into paragraph direction in a
way that makes the page scope unclear. Its label, tooltip, accessibility name,
and visual grouping must say that it changes the page.

### Page setup

Page setup is a ribbon feature in NoteCraft because page layout is part of the
normal note-writing workflow. The initial visible controls should follow
NoteCraft's existing page model and avoid presenting unsupported Word options.

Candidate controls include:

- Page direction.
- Rule-line style.
- Grid or paper style.
- Page color, when implemented.
- Page width or layout options, when implemented.

Unsupported actions must not appear as successful controls. If a control is
shown for discoverability, it must use NoteCraft's standard not-built behavior
and label.

Page setup should not become a Word-style modal dialog by default. A dialog can
be added later for settings that cannot be edited directly in the ribbon, but
the common page controls must remain one click away.

## Icon and theme requirements

- Reuse the existing icon registry and ribbon painting helpers.
- Prefer the existing Fluent assets when a matching polished icon exists.
- Use custom NoteCraft icons for bidi-specific concepts where the existing
  assets do not match.
- Do not add WordCraft or Microsoft assets.
- Do not hard-code light-theme colors.
- Use `Tokens` for text, accent, hover, selected, muted, and disabled states.
- Preserve the current ribbon geometry and accessibility metadata.
- Icons must remain understandable when the ribbon is narrow or horizontally
  scrolled.
- Mirrored icons must be intentionally mirrored. Do not rely on an RTL layout
  to mirror an icon whose meaning is directional.

## Scope boundaries

Included:

- Bidi icon design alignment with WordCraft concepts.
- NoteCraft theme and color integration.
- Ribbon grouping and labels.
- Page direction discoverability.
- Page setup controls that already have NoteCraft behavior.
- Increase/decrease indentation terminology and icon alignment.
- Accessibility labels and tooltips.
- Focused UI tests and icon visual tests.

Not included:

- Replacing the bidi text engine.
- Changing paragraph direction precedence.
- Changing the page file format.
- Adding a new icon dependency.
- Full RTL mirroring of application chrome.
- Implementing unsupported Word page-layout features.
- Changing the document model for indentation.

## Likely implementation surfaces

- `crates/ui-egui/src/icons.rs`
  - Add or refine bidi and indentation icon definitions.
- `crates/ui-egui/src/ribbon.rs`
  - Update groups, labels, tooltips, and page setup placement.
- `crates/ui-egui/src/app.rs`
  - Preserve or connect page direction and page setup actions.
- `crates/ui-egui/tests/ui.rs`
  - Verify labels, menu placement, command scope, and page direction.
- `crates/ui-egui/tests/icons_visual.rs`
  - Verify icon rendering across themes and relevant icon sizes.
- Related documentation under `docs/`
  - Keep the UI behavior and bidi scope documented.

## Acceptance criteria

### Visual

- Bidi icons communicate paragraph direction, page direction, and text-run
  direction as separate scopes.
- Icons follow WordCraft's recognizable concepts without copying its assets.
- Icons use NoteCraft theme colors in light and dark themes.
- Increase and decrease indentation use clear, distinct directional icons.
- Page direction is visually discoverable in the ribbon without opening a
  dialog.

### Behavioral

- Paragraph direction continues to apply only to the current paragraph or
  selected paragraphs.
- Page direction continues to apply at page scope.
- A paragraph-specific direction mark continues to override page direction.
- Text-run direction continues to apply only to selected text or the next
  typing context.
- Increase and decrease indentation retain existing keyboard and document
  behavior.
- Page setup actions do not silently claim support for unimplemented features.

### Accessibility

- Every control has a stable, scope-specific accessible name.
- Tooltips use the labels "Increase indentation" and "Decrease indentation".
- Page direction is explicitly named as page direction.
- Paragraph and text-run direction are not announced with the same label.

### Verification

Run the focused checks first:

```text
cargo test -p notecraft-ui-egui --test ui
cargo test -p notecraft-ui-egui --test icons_visual
```

Then run the repository gate:

```text
cargo xtask ci
```

Review the controls in at least:

- Light theme.
- Dark theme.
- A page with left-to-right direction.
- A page with right-to-left direction.
- A mixed Arabic and Latin paragraph.
- A narrow ribbon that requires horizontal scrolling.

## Related documents

- [`docs/rtl-in-word-and-onenote.md`](../../rtl-in-word-and-onenote.md)
- [`docs/releases/0.0.8/rc_feature.md`](./rc_feature.md)

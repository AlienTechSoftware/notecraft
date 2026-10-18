# Themes

Last reviewed: 2026-10-17.

NoteCraft has 14 themes: **light and dark**, each with **seven accents**:

| Accent | Light accent | Dark accent | Surfaces |
|---|---|---|---|
| Default | NoteCraft purple | lighter purple | neutral |
| Red | red | salmon | tinted red |
| Green | green | mint | tinted green |
| Yellow | dark amber | yellow | tinted yellow |
| Blue | blue | sky | tinted blue |
| Gray | slate | light slate | neutral |
| Purple | purple | lighter purple | tinted purple |

"Default" is the brand purple on neutral surfaces; "Purple" also tints the surfaces. "Gray" is
neutral on purpose.

## The switcher

The top bar has a **Theme: Light Default** button. It opens a grid of 14 swatches (a light row
and a dark row, seven accents each) and a switch, **Follow the system light or dark setting**.
Each swatch shows the theme's surface colour ringed and dotted in its accent. A swatch names a
mode, so clicking one picks that mode and turns the switch off. The switch is the way back to
following the system. (Until 2026-10-17 a swatch changed only the accent while the switch was
on, so picking "Dark Default" on a light desktop did nothing visible. That was a bug.)

The choice is saved in the settings file (`theme_mode`: `light`, `dark` or `system`;
`theme_accent`) outside any notebook, and is restored at the next start.

## How colours are made

`crates/ui-egui/src/theme.rs`. Each theme builds a `Tokens` set from a base surface (light or
dark), the accent, and a tint amount. The UI uses tokens only; there are no literal colours
in the drawing code.

Contrast is part of the construction, not a hope. The tokens are adjusted until:

| Pair | Target (WCAG contrast ratio) |
|---|---|
| Body text on a panel or the editor | 7 to 1 |
| Strong text on a panel | 7 to 1 |
| Text on a hovered or selected tree row | 7 to 1 |
| Secondary (muted) text on a panel | 4.5 to 1 |
| Links on the editor | 4.5 to 1 |
| Warning text on a panel | 4.5 to 1 |
| Accent colour, folder and section icons, success colour on a panel | 3 to 1 |
| Rule lines on a page (`rule_line`) | 1.25 to 1, and at most 4.5 to 1 |
| Grid and margin lines (`rule_line_strong`) | 1.8 to 1, and at most 4.5 to 1 |

The last two are page decoration, not text, so they have an upper bound as well: a rule line
must be visible on the page but must never compete with the writing on top of it. Text keeps
4.5 to 1 against a rule line it crosses. Ruled and grid page backgrounds are not drawn yet
(see `gaps.md`); the colours exist so that when they are, they are part of the same check.

The test `every_theme_is_readable` checks every pair in all 14 themes. When it was first run it
found a real problem (text on the selected row in the dark yellow theme was 6.4 to 1), which
the selection tint now avoids by lowering its strength until the target is met.

## Adding or changing a theme

- A new accent: add it to `Accent`, to `Accent::ALL`, to `id`, `label`, `base` and `tint`, and
  run the tests. The contrast tests tell you if the colours are unreadable. Regenerate the
  theme screenshots if you want one for it.
- Changing a colour: edit `theme.rs`, run `cargo test -p notecraft-ui-egui`, look at the diffs
  of the screenshot tests, and accept them if intended (see `testing.md`).

## How this compares with the apps people come from

What follows comes from web searches (support pages and articles), not from any
specification. The Office note taking app on Windows offers an Office theme (Colorful, Dark
Gray, Black, White) in File, Options, General, and its Windows 10 version has a light, dark or
follow-Windows setting under Settings and Options. Notebooks and sections can be coloured, and a page can have a
background colour (View, Page Color). NoteCraft's accents are a per-app setting; per-notebook
and per-section colours are not built yet (see `gaps.md`).

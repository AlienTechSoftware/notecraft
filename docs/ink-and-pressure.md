# Ink thickness, pressure, and edge treatment

This is the NoteCraft product behavior for pressure-sensitive drawing. The detailed engineering
contract is maintained by the [`nc-uiux-drawing` skill](../SKILLS/nc-uiux-drawing/SKILL.md), in
[Ink scaling and edge treatment](../SKILLS/nc-uiux-drawing/references/ink-scaling-and-edge-treatment.md).
The skill is the owner of this contract: implementation changes must update it first, and this
document must remain consistent with it.

## Thickness

The thickness chooser should use perceptual, geometric spacing rather than a linear `1 -> 50 px`
scale. A useful initial family is:

```text
1, 1.4, 2, 3, 4.5, 6.5, 9, 13, 19, 28, 40, 60 px
```

These values are a design target, not a promise that the current selector already uses them.
The selector may show friendly labels, while the renderer keeps a precise width.

## Pressure

The selected thickness is the full-pressure width. Pressure changes the width around that base;
it does not replace the selected thickness. Sensitivity changes the response curve:

- low sensitivity requires more pressure before the stroke becomes broad;
- neutral sensitivity follows the device reading approximately;
- high sensitivity makes a lighter hand become broad more quickly;
- sensitivity 100 must still reach the selected width at maximum pressure.

The intended model is a bounded power curve. It must produce a monotonic, finite response and a
clearly visible difference between light, medium, and hard pressure. Highlighters may remain flat
because their broad translucent mark is intentional.

Pressure readings must be inspected before tuning the curve. If a tablet driver compresses the
range delivered to NoteCraft, a curve alone cannot restore the missing physical distinction.

## Ink edges

Ink should not be rendered with a generic blur. Thin pen lines should remain crisp and
antialiased. Broad pen and brush strokes may have a subtle translucent fringe; pencil may be
softer or textured; fountain pen should stay mostly crisp; highlighter should have a broader,
translucent edge. The fringe is a small fixed physical width, not a percentage of the stroke, so
thick ink remains solid instead of becoming hazy.

The editable ink data must preserve the original points, pressure readings, tool, selected width,
and sensitivity. Edge treatment is a rendering concern and must not destroy that information.

## Acceptance checks

Synthetic tests must cover pressures `0`, `0.1`, `0.25`, `0.5`, `0.75`, and `1.0` at minimum,
neutral, and maximum sensitivity. Verify monotonic widths, full-pressure maximums, visible
light-to-hard separation, finite bounded values, and behavior in both light and dark themes.
Repeat with a real pressure pen and a mouse before claiming pressure behavior is complete.

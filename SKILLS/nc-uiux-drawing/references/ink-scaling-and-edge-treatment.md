# Ink scaling and edge treatment

Status: product and rendering contract recorded on 2026-10-25. This describes the target
behavior; it is not a claim that every part is implemented yet.

## Thickness is perceptual, not linear

The thickness chooser should behave more like a musical scale than a linear ruler. A linear
`1 -> 50 px` mapping gives too little useful control among thin writing sizes and makes the
largest sizes dominate the control. Use geometric/logarithmic spacing for the selectable
thicknesses, for example:

```text
1, 1.4, 2, 3, 4.5, 6.5, 9, 13, 19, 28, 40, 60 px
```

The exact values may change after visual testing, but the important rule is multiplicative
spacing: adjacent choices should feel like consistent increases in perceived weight. The
selector may display friendly labels while the renderer stores a precise width.

Pressure remains continuous. It must not be quantized to the thickness steps.

## Pressure response

The selected thickness is the baseline full-pressure width at neutral sensitivity. The upper
half of the sensitivity control also exposes the broader diagnostic range: at 100 the
full-pressure maximum is four times the selected baseline, matching the pressure canvas's
default 12 px maximum against a 3 px pen baseline. Sensitivity changes both how quickly the
stroke approaches that width and, above neutral, the available full-pressure range.

Use a bounded power curve after input normalization:

```text
width = floor + (maximum - floor) * pressure^gamma
```

where `pressure` is in `0..=1` and `gamma` is selected by the sensitivity control:

- low sensitivity: `gamma > 1`, requiring a firmer hand;
- neutral sensitivity: `gamma ~= 1`;
- high sensitivity: `gamma < 1`, making a lighter hand produce more width.

At sensitivity 100, a medium-pressure stroke should already be visibly close to the expanded
maximum, and a maximum-pressure stroke must reach that maximum. The curve must remain monotonic,
finite, and visibly separated at light, medium, and hard pressure for tools that support
pressure. Highlighters may intentionally remain pressure-flat.

Before applying the curve, record and inspect the device readings. A compressed driver range
cannot be repaired by changing the curve alone; it needs input calibration or a better platform
input path.

## Edge treatment

Ink should not use a generic Gaussian blur. The renderer should provide:

- crisp, antialiased coverage for ordinary thin pen strokes;
- a subtle translucent fringe, roughly 1-2 physical pixels, for broad pen and brush strokes;
- a softer, more textured edge for pencil;
- mostly crisp pressure-shaped edges for fountain pen;
- a broad translucent edge for highlighter.

Edge softness is a fixed physical fringe, not a percentage of stroke width. A thick stroke
must therefore remain visually solid instead of becoming hazy. Antialiasing belongs to the
display/export renderer; the editable stroke must continue to preserve its original points,
pressures, tool, width, and sensitivity information.

## Verification contract

Test synthetic pressure readings at `0`, `0.1`, `0.25`, `0.5`, `0.75`, and `1.0` at minimum,
neutral, and maximum sensitivity. Confirm monotonic widths, selected-width maximums, visible
light-to-hard separation, bounded values, and no NaN/infinite results. Review thin and thick
strokes in light and dark themes, then repeat with a real pressure pen and a mouse.

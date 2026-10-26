# OneNote ink and pressure reference

Status: verified against Microsoft's public support documentation on 2026-10-25.

## Important fact

Microsoft documents OneNote as using variable pen pressure for drawings and handwritten notes.
The setting is **Use pen pressure sensitivity** in OneNote's advanced options. Microsoft says
that enabling it gives drawings and handwriting a more natural appearance:

- [OneNote Options (Advanced)](https://support.microsoft.com/en-us/onenote/onenote-options-advanced)

Microsoft also documents a separate, fixed pen or highlighter thickness selection. The chosen
thickness is the tool's configured size, while pressure sensitivity varies the stroke as the
pen is pressed. The public documentation does not specify OneNote's exact pressure curve,
calibration algorithm, minimum-width floor, or driver normalization.

Relevant documentation:

- [Draw and sketch notes in OneNote](https://support.microsoft.com/en-us/onenote/onenote-help-and-learning/draw-and-sketch-notes-in-onenote)
- [Learn more about Drawing Tools](https://support.microsoft.com/en-us/onenote/onenote-help-and-learning/learn-more-about-drawing-tools)
- [Change the color and thickness of ink strokes](https://support.microsoft.com/en-us/onenote/change-the-color-and-thickness-of-ink-strokes-in-onenote-for-windows-10)
- [Draw and write with ink in Office](https://support.microsoft.com/en-gb/office/draw-and-write-with-ink-in-office)

## What the documentation establishes

OneNote's drawing workflow includes:

- selectable pens and highlighters;
- configurable ink colour and thickness;
- pressure-sensitive handwriting and drawing when enabled and supported by the device;
- erasing, lasso selection, shapes, ruler, ink-to-text, ink-to-math, and ink replay in the
  Windows drawing experience;
- smoothing as an available drawing option in the advanced settings.

The documentation describes pressure sensitivity as a behavior of the ink, not as a replacement
for the selected pen thickness. A NoteCraft pen therefore needs both a configured full-pressure
width and a pressure response curve.

## What the documentation does not establish

Do not present any of the following as OneNote facts unless a future primary source documents
them:

- the exact mathematical pressure curve;
- the number of pressure levels used;
- the minimum width at zero or light pressure;
- how OneNote compensates for an XP-Pen or other tablet driver's hardness curve;
- whether every OneNote platform exposes the same pressure behavior;
- whether a particular driver setting is passed through unchanged.

The XP-Pen driver and OneNote are separate calibration stages. A driver can compress the
physical force range before NoteCraft receives it. Hardware testing must therefore record the
pressure values that reach the application, not only the driver's slider position.

## NoteCraft decision

NoteCraft should preserve the stronger, user-visible capability of a tunable pressure response,
but the default should remain OneNote-like:

1. the selected thickness is the full-pressure width;
2. light pressure is clearly thinner;
3. medium pressure is visibly intermediate;
4. maximum pressure reaches the selected width;
5. sensitivity changes curve hardness without flattening the light-to-hard distinction;
6. original readings remain in editable `ink.svg` data for later re-weighting.

This is a clean-room product decision based on the documented behavior and observed user
requirements. It is not a claim about OneNote's unpublished implementation.

## Investigation checklist

Before changing the pressure curve, run the pen diagnostic and capture:

- whether the device is recognized as a pressure pen;
- minimum, maximum, and representative middle readings;
- the number of readings collected;
- the selected tool, thickness, and sensitivity;
- a saved stroke and its editable pressure readings.

Compare synthetic and real strokes at light, medium, and maximum pressure. A curve cannot
restore distinctions that the platform or driver has already discarded; that case needs input
calibration or a platform-specific tablet reader before geometry tuning.

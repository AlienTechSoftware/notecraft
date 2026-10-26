---
name: nc-uiux-drawing
description: Design, implement, and review NoteCraft handwriting, drawing tools, stylus pressure, ink selection, and OneNote-compatible drawing workflows.
license: MIT
metadata:
  version: "0.0.20261026"
  related_skills: [nc-uiux-controls, nc-themes, nc-uiux-bidi]
---

# nc-uiux-drawing

Use this skill for the Draw tab, pen and highlighter tools, stylus input, pressure response,
ink rendering, erasing, lasso selection, shapes, and drawing-mode interactions.

The drawing experience is a product-defining part of NoteCraft. It must feel natural with a
real pen while remaining useful with a mouse, touch, and platforms that do not expose tablet
pressure. Do not reduce pressure support to a decorative feature or tune it from a single
device. Preserve the user's readings, make the visible response understandable, and verify
the result with both real hardware and synthetic input.

## Read the references first

The technical record for this skill lives in [references](./references/). Start with
[OneNote ink and pressure](./references/onenote-ink-and-pressure.md) before changing pressure,
pen thickness, or drawing-tool behavior. Read [Draw ribbon current state and target]
(./references/draw-ribbon-current-state.md) before changing the Draw ribbon. It records the
current NoteCraft layout, the OneNote target, and the visual/interaction gaps we are trying to
close. These references link to Microsoft's public documentation and separate documented
OneNote behavior from our clean-room design decisions.

The skill owns the NoteCraft drawing behavior contract. For thickness scaling, pressure curves,
and ink edge treatment, read [Ink scaling and edge treatment]
(./references/ink-scaling-and-edge-treatment.md). Keep the public product summary in
[`docs/ink-and-pressure.md`](../../docs/ink-and-pressure.md) synchronized with that reference;
do not let implementation changes silently redefine either document.

For Windows input, device identity, HID ranges, and calibration, read [Windows pen input and
device metrics](./references/win/windows-pen-input.md). Treat that reference as the authority for
what Windows reports versus what remains driver-specific.

When investigating a drawing issue:

1. Read this skill and the relevant reference in `references/`.
2. Read the current implementation in `crates/ink/` and `crates/ui-egui/src/inkpad.rs`.
3. Check the platform input path, including `crates/ui-egui/src/stylus.rs` and the desktop app.
4. Use `cargo run -p notecraft --example pen_check` to inspect pressure readings on hardware.
5. Add or update focused tests for the pressure curve and run the full repository gate.

For visual pressure investigation, use the isolated [pressure canvas](./tools/pressure_canvas.ps1)
with `powershell -ExecutionPolicy Bypass -File SKILLS/nc-uiux-drawing/tools/pressure_canvas.ps1`.
It shows the raw pressure value, the hardest reading, reading count, synthetic pressure bands,
and the live rendered stroke. Use it to separate a tablet-input problem from a width/curve
problem before changing the normal page editor.

## Interaction contract

- A selected pen has a base thickness. Pressure changes the visible width around that base;
  it must not silently replace the selected thickness.
- Thickness choices should use perceptual geometric spacing, like musical intervals, rather than
  a linear pixel ruler. Pressure remains continuous between those choices.
- Light, medium, and maximum pressure must produce visibly distinguishable widths for a
  pressure-reporting pen, unless the selected tool is intentionally pressure-flat, such as a
  highlighter.
- Maximum pressure reaches the selected full-pressure width. Light pressure must not be
  boosted so far that normal writing becomes almost full width.
- Sensitivity changes a bounded power-curve response, not the stored readings or the selected
  maximum width. At sensitivity 100, medium pressure should be close to full width and maximum
  pressure must reach the selected width.
- Sensitivity changes the response curve, not the stored device readings. Reopening or
  re-weighting a stroke must preserve the original readings.
- Edge treatment is not a generic blur: use crisp antialiasing for thin ink and a small,
  fixed-width translucent fringe only where the tool calls for softness. Thick strokes must stay
  solid rather than becoming hazy.
- A mouse or a platform without pressure must produce a stable, usable line rather than a
  hairline or an invisible stroke.
- Pressure values are bounded and finite before they reach stroke geometry or persistence.
- Pen, pencil, fountain pen, brush pen, and highlighter may have different pressure floors,
  but the difference must be intentional, documented, and tested.
- Do not claim Linux or macOS pressure support until the platform input path supplies it.
- Keep the editable pressure readings in the page's ink file; plain export may omit them.

## Verification

Test the response with synthetic readings at 0, 0.1, 0.25, 0.5, 0.75, and 1.0, at the
minimum, middle, and maximum sensitivity settings. Verify that widths are monotonic, finite,
and visibly separated where the tool promises pressure response. Include negative tests for
NaN, infinity, and out-of-range readings.

On a real pressure pen, record the diagnostic reading range and sample count. Test a light,
normal, and hard stroke with the same tool and thickness. Repeat with a mouse and with a
platform that reports no pressure. Check the rendered page, saved `ink.svg`, reopening, undo,
selection, and plain-picture export.

Run `cargo xtask ci` before claiming completion. Do not replace the full gate with a focused
test, skip hooks, or push a pressure change that has not passed the gate.

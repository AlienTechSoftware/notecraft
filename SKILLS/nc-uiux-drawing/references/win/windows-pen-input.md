# Windows pen input and device metrics

Status: verified against Microsoft documentation on 2026-10-26. This is the Windows-specific
input reference for the `nc-uiux-drawing` skill.

## What Windows can report

The standard Windows pointer stack can report these pen properties:

- contact and in-range state;
- normalized tip pressure;
- X and Y tilt;
- rotation/twist;
- barrel-button state;
- inverted/eraser state;
- pointer and device capabilities.

The Win32 `POINTER_PEN_INFO` structure reports pressure, rotation, and tilt. Its pressure value
is normalized to `0..1024`; if the device does not report pressure, Windows supplies zero. The
`penMask` field says which optional values are valid, so NoteCraft must check the mask instead of
assuming that every pen supports every metric.

Windows Ink's `PointerPointProperties.Pressure` presents pressure as a floating-point value from
`0.0..1.0`. Microsoft documents this as a normalized value derived from the HID tip-pressure
usage.

## Device ranges are not universal

A tablet driver may expose a different raw logical range. For example, an XP-Pen control panel
may display `0..8191`, while the Windows pointer layer normalizes the same signal to `0..1024` or
`0.0..1.0`. These values must not be compared as if they were the same unit.

The raw HID descriptor can specify logical and physical ranges, but the device is responsible for
reporting accurate values. Windows also normalizes pressure before delivering it through the
standard pointer path. A driver hardness curve can compress the useful range before NoteCraft
receives it; the application cannot infer the original physical force from the normalized value.

## Device and pen identity

Windows can enumerate pointer devices and their supported capabilities. HID digitizer reports
may also include vendor and transducer identifiers, including a transducer serial number, when
the hardware provides them. These identifiers are optional and must not be required for drawing.

NoteCraft should key a calibration profile by the strongest stable identity available:

1. transducer identifier or serial number, when exposed;
2. tablet device identity and vendor/product information;
3. a session-only fallback when no stable identity is available.

Do not claim that a Windows device identity proves the pen's physical hardness or pressure curve.
Those remain device- and driver-specific.

## NoteCraft implementation plan

The Windows input adapter should:

1. consume native `WM_POINTER` pen data, including coalesced pointer history;
2. check `POINTER_PEN_INFO.penMask` before using pressure, tilt, or rotation;
3. normalize the reported pressure to `0.0..1.0` while retaining the original normalized reading;
4. identify the device when Windows exposes a stable identifier;
5. expose raw input diagnostics to the pressure canvas;
6. apply per-device calibration before the sensitivity curve;
7. preserve the original readings in editable `ink.svg` data.

The existing egui event path is useful as a fallback, but it must not be treated as the complete
Windows pen implementation until the pressure canvas proves that real pen pressure reaches it.

## Calibration requirements

Calibration must be explicit about its stages:

```text
driver/device reading -> Windows normalization -> NoteCraft calibration -> sensitivity curve -> ink width
```

At minimum, record the observed light, normal, and hard readings for the selected device. A
calibration profile may remap the observed interval to the useful `0..1` range, but it must not
silently rewrite the stored readings. The UI should show the raw/normalized value, calibrated
value, device identity, and sample count while calibrating.

## Official references

- [POINTER_PEN_INFO](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-pointer_pen_info)
- [GetPointerPenInfo](https://learn.microsoft.com/en-us/windows/desktop/api/winuser/nf-winuser-getpointerpeninfo)
- [Pen Mask constants](https://learn.microsoft.com/en-us/windows/win32/inputmsg/pen-mask-constants)
- [PointerPointProperties](https://learn.microsoft.com/en-us/uwp/api/windows.ui.input.pointerpointproperties)
- [PointerPointProperties.Pressure](https://learn.microsoft.com/en-us/uwp/api/windows.ui.input.pointerpointproperties.pressure)
- [Supporting usages in digitizer report descriptors](https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/supporting-usages-in-digitizer-report-descriptors)
- [Handle pointer input](https://learn.microsoft.com/en-us/windows/apps/develop/input/handle-pointer-input)

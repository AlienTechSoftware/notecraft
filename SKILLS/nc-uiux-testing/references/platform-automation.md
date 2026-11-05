# Platform UI automation boundaries

NoteCraft's application-level control protocol and `egui_kittest` harness are
the default cross-platform test layers. External automation is a fallback for
claims that require a real desktop integration.

## Windows

Windows UI Automation, commonly driven through `pywinauto`, can verify semantic
controls exposed by the native accessibility adapter. Native input or a real
window is needed for physical cursor movement, pointer capture, display scaling,
focus, and compositor behavior.

## Linux

Linux desktop checks may use the platform accessibility path (AT-SPI) and a real
Wayland or X11 session when that integration is the subject under test. WSLg is
useful for manual display checks, but it is not a substitute for native Linux
coverage or a deterministic headless test.

## Boundary

External desktop tests are timing-, focus-, permission-, display-, and scale-
sensitive. Keep them small and diagnostic. Do not replace deterministic
application-level tests with desktop automation merely because a screenshot is
available.

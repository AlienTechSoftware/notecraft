# NoteCraft screenshot capture

This is the canonical operational guide for capturing NoteCraft UI images.

## Headless capture

Use the off-screen wgpu runner when no desktop window is required:

```text
cargo run -p notecraft-ui-egui --example ui_shot -- script.jsonl
cargo xtask screenshots
```

`ui_shot` executes JSON-lines control scripts against an off-screen NoteCraft
window. `cargo xtask screenshots` runs `scripts/readme-screens.jsonl` and writes
the documented README images under `docs/images/`. Keep the script viewport at
`1440x900` for the shipped README set unless the task explicitly changes it.

The control protocol's `ui.screenshot` method can save a PNG to a requested path
or return a base64 PNG. See [`docs/control-protocol.md`](../../../docs/control-protocol.md)
for the request format.

## Test captures

Set `NOTECRAFT_UI_VISUAL_DIR` when a UI test needs before/after/difference images:

```text
cargo test -p notecraft-ui-egui --test ui
```

These are diagnostic artifacts. They are normally written outside the repository
or remain ignored; inspect them before making a visual claim.

## Real-window capture

On Windows only, `scripts/capture-window.ps1` captures a window that is open on
the desktop. Use it only when headless rendering cannot test the claim, such as
native focus, display scaling, pointer capture, compositor behavior, or a
platform-specific dialog. It is not the generator for shipped documentation
images.

Real-window checks must record the OS, display scale, window size, theme, and
whether the window was focused. Do not treat a failed desktop capture as proof
that the application failed; report the capture limitation separately.

## Review and ownership

For public documentation or demo images, follow `nc-ua/screenshot-audit.md`.
For release evidence, follow `nc-reelase-signoff`. For protocol details, use
`docs/control-protocol.md`; do not copy the full protocol into this skill.

# Control protocol

Last reviewed: 2026-10-17.

A running NoteCraft window can be driven by scripts and tools over a loopback port. This is
how the UI tests, the headless screenshot runner and any later automation talk to the app. It
follows the control channel of the other Craft apps, so the same tools can drive them.

## Starting it

```bash
notecraft --control 0
```

`0` picks a free port. The window prints nothing; read the port and the key from the key file.
The channel is off unless `--control` is given.

## Security

- The listener binds `127.0.0.1` only.
- Every connection must present the window's key. The key is 256 random bits (64 hex
  characters), written to `control-key.local-PORT` next to the settings file, and removed when
  the window closes. On Unix the file is mode 0600. On Windows it relies on the user folder's
  permissions (assumed, not verified here).
- Keys are compared without early exit.
- A line is at most 1 MiB. A connection idle for 30 seconds is dropped. Junk, a wrong key or an
  over-long line gets one error reply and the connection closes.
- The window answers within 30 seconds or the caller gets an error.

## Wire format

JSON lines over TCP: one request object per line, one reply line per request.

```json
{"id": 1, "key": "<key>", "method": "ui.inspect", "params": {}}
```

The key is needed on the first request of a connection (or on a `method: "auth"` request, which
returns `{}`); later requests on the same connection may leave it out. Replies:

```json
{"id": 1, "ok": true, "result": {}}
{"id": 1, "ok": false, "error": "unknown method 'x'"}
```

## Methods

| Method | Params | Result |
|---|---|---|
| `auth` | none | `{}` |
| `engine.commands` | none | every command: `id`, `label`, `params`, `changesTree` |
| `engine.execute` | `command`, `params` (object) | the command's output; same as `notecraft-cli run` |
| `ui.inspect` | none | visible tree rows, open page, theme, window size |
| `ui.open` | `path` (relative to the notebooks folder) | the same as `ui.inspect`, or an error |
| `ui.expandAll`, `ui.collapseAll` | none | `{}` |
| `ui.theme` | `mode` (`light`, `dark`, `system`), `accent` | `{theme, modePref}` |
| `ui.click` | `x`, `y`, `button`, `count`, `alt`, `ctrl`, `shift` | `{}` (events are queued one per frame) |
| `ui.move`, `ui.press`, `ui.release` | `x`, `y`, `button` | `{}` |
| `ui.key` | `key` (an egui key name), modifiers | `{}` |
| `ui.text` | `text` | `{}` (typed into the focused widget) |
| `ui.resize` | `width`, `height` | `{}` |
| `ui.screenshot` | `path` (optional) | the PNG saved to `path`, or returned as base64 |
| `app.quit` | none | `{}` (saves first) |

Coordinates are in points, not pixels. `engine.execute` saves the open page before it runs, so
a command sees what the user sees. Paths in replies from the headless runner are redacted.

## Headless use

`ui_shot` runs the same methods against a window drawn off screen, so no real window opens:

```bash
cargo run -p notecraft-ui-egui --example ui_shot -- script.jsonl
```

A script is JSON lines. Control requests are sent as they are. Extra lines: `{"shot": "/abs/out.png"}`,
`{"steps": n}`, `{"wait": ms}`, and options (`width`, `height`, `scale`, `empty`, `theme`)
before the first other line. The README pictures are made this way:
`scripts/readme-screens.jsonl` and `cargo xtask screenshots`.

## Tests

`control.rs` tests the transport with a stand-in window: key required, key file lifetime, junk,
over-long line, busy port. `tests/ui.rs` drives the real window in process and over a real TCP
connection.

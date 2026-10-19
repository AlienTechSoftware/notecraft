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
| `ui.inspect` | none | visible tree rows, open page (the caret: its block, text, `at` and `anchor` in characters; `containers`: the boxes the blocks are arranged in; `selectedContainer`: the index of the box that is selected, or null; and `history`: how many steps of undo the page is keeping, where in them it is, and whether undo and redo have anywhere to go), theme, window size, `panes` (whether the side panel is split into organisers and pages, and whether the organisers column is showing), `paper` (the rule lines behind the pages, by id) and `pen` (see below) |
| `ui.open` | `path` (relative to the notebooks folder) | the same as `ui.inspect`, or an error |
| `ui.expandAll`, `ui.collapseAll` | none | `{}` |
| `ui.theme` | `mode` (`light`, `dark`, `system`), `accent` | `{theme, modePref}` |
| `ui.click` | `x`, `y`, `button`, `count`, `alt`, `ctrl`, `shift` | `{}` (events are queued one per frame) |
| `ui.move`, `ui.press`, `ui.release` | `x`, `y`, `button` | `{}` |
| `ui.key` | `key` (an egui key name), modifiers | `{}` |
| `ui.text` | `text` | `{}` (typed into the focused widget) |
| `ui.paste` | `text` | `{}` (pasted, so a tab-separated table becomes a table) |
| `ui.resize` | `width`, `height` | `{}` |
| `ui.screenshot` | `path` (optional) | the PNG saved to `path`, or returned as base64 |
| `app.quit` | none | `{}` (saves first) |

Coordinates are in points, not pixels. `engine.execute` saves the open page before it runs, so
a command sees what the user sees. Paths in replies from the headless runner are redacted.

### What `pen` reports

`{"seen": false, "pressure": 1.0, "hardest": 0.0, "readings": 0}` until a device reports a
pressure to this window; after that, `seen` is true and the three numbers are the pressure
now, the hardest press so far and how many readings have arrived. A mouse always reads 1.0
with `seen` false.

Nothing draws with this yet. It is here so the question "does a pen reach NoteCraft on this
machine" can be answered without a person in front of the window. On Windows the pressure is
real: measured on 2026-10-18 at a hardest press of 0.930 over 1223 readings (winit reads
`WM_POINTER`). On macOS, Linux and Wayland winit drops the tablet fields, so a pen there
reports nothing. Tilt and the eraser end are not available on any platform.
`crates\ui-egui\src\stylus.rs` has the detail, and
`cargo run -p notecraft --example pen_check` opens a sheet to scribble on.

## Headless use

`ui_shot` runs the same methods against a window drawn off screen, so no real window opens:

```bash
cargo run -p notecraft-ui-egui --example ui_shot -- script.jsonl
```

A script is JSON lines. Control requests are sent as they are. Extra lines: `{"shot": "/abs/out.png"}`,
`{"steps": n}`, `{"wait": ms}`, and options (`width`, `height`, `scale`, `empty`, `theme`,
`notebook`) before the first other line. `notebook` names a notebook folder to copy into the
window's temporary notebooks folder, for example `docs/demo/DemoNotebook`; the original is not
touched. The README pictures are made this way: `scripts/readme-screens.jsonl` and
`cargo xtask screenshots`.

## Tests

`control.rs` tests the transport with a stand-in window: key required, key file lifetime, junk,
over-long line, busy port. `tests/ui.rs` drives the real window in process and over a real TCP
connection.

# Drive it from a script

Every user-visible action in NoteCraft is a command with an id. The window, the command line and
the control channel all dispatch the same ones, so anything you can click you can script, and a
script is tested by the same code the window uses.

## The command line

```sh
notecraft-cli commands                               # every command, with its parameters
notecraft-cli run notebook.create name=Work
notecraft-cli run node.create parent=work kind=section title=Projects format=cmgfm
notecraft-cli run node.create parent=work/projects kind=page title="Kickoff"
notecraft-cli run page.save path=work/projects/kickoff body="# Kickoff

Notes."
notecraft-cli tree
notecraft-cli search kickoff
notecraft-cli check work
notecraft-cli sample                                 # the sample notebooks, for trying things
```

`--notebooks FOLDER` points it at another notebooks folder. The exit status is 1 when a command
fails or a check finds problems, so a script can rely on it. The whole list, generated from the
code, is in [../commands.md](../commands.md).

## The running window

```sh
notecraft --control 0
```

The window then listens on a loopback port. The port and a random key are written next to the
settings file as `control-key.local-PORT`; every request needs that key. It speaks JSON lines:

```json
{"id": 1, "key": "<key>", "method": "ui.open", "params": {"path": "work/projects/kickoff"}}
{"id": 2, "method": "ui.caret", "params": {"block": 1}}
{"id": 3, "method": "ui.text", "params": {"text": "typed into the page"}}
{"id": 4, "method": "engine.execute", "params": {"command": "search.query", "params": {"q": "kickoff"}}}
{"id": 5, "method": "ui.screenshot", "params": {"path": "C:/tmp/shot.png"}}
{"id": 6, "method": "ui.inspect"}
```

`ui.inspect` answers with what the window shows: the visible tree rows, the open page, where the
caret is, the theme and the window size. That is how the tests check their work without
screenshots. The full list of methods is in [../control-protocol.md](../control-protocol.md).

## Pictures without a window

The same methods drive a window drawn off screen, which is how this repository's screenshots are
made:

```sh
cargo run -p notecraft-ui-egui --example ui_shot -- script.jsonl
cargo xtask screenshots          # rebuilds docs/images from scripts/readme-screens.jsonl
```

A script is JSON lines: control requests as above, plus `{"shot": "out.png"}`, `{"wait": 500}`,
`{"steps": 4}` and options such as `{"width": 1440, "height": 900, "theme": "dark-green"}`.

## An assistant, over MCP

```sh
notecraft-cli mcp                  # opens the notebooks folder itself
notecraft-cli mcp --connect 54321  # drives the running window instead
```

That speaks the Model Context Protocol on stdin and stdout, so an assistant can list the tree,
search, read and write pages, make notebooks and sections, check a notebook, and run any command
by id. Connected to a window it can also open a page, look at what is on screen and take a
picture of it. Nothing goes over the network. In a client's configuration:

```json
{"mcpServers": {"notecraft": {"command": "notecraft-cli", "args": ["mcp"]}}}
```

The tools, the resources and what the server does not do are in [../mcp.md](../mcp.md).

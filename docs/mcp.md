# MCP server

Last reviewed: 2026-10-17.

NoteCraft speaks the [Model Context Protocol](https://modelcontextprotocol.io), so an assistant
can read and write your notes the same way you can. It is not a second way of doing things:
every tool runs an engine command, the same ones the window's buttons and `notecraft-cli run`
dispatch, so a tool cannot do anything the app cannot, and one test covers both.

## Starting it

```bash
notecraft-cli mcp                       # opens the notebooks folder itself
notecraft-cli mcp --notebooks FOLDER    # another notebooks folder
notecraft-cli mcp --connect 54321       # drives a running window instead
```

The server reads JSON-RPC 2.0 requests from stdin, one per line, and writes one reply line
each, which is what MCP calls the stdio transport. It prints nothing else, so stdout stays
clean for the protocol.

In a client's configuration file that looks like:

```json
{
  "mcpServers": {
    "notecraft": {
      "command": "notecraft-cli",
      "args": ["mcp"]
    }
  }
}
```

### Headless, or the running window

| | `notecraft-cli mcp` | `notecraft-cli mcp --connect PORT` |
|---|---|---|
| Needs the app running | no | yes: `notecraft --control PORT` |
| Sees | the notebooks folder on disk | what the person has open |
| Tools | the nine that work on files, and `open`, which says what it would open | those ten and `inspect`, `screenshot` |

Connecting needs the window's key, which the window writes to `control-key.local-PORT` beside
the settings file; the server reads it from there, or from `NOTECRAFT_CONTROL_KEY`. The rest of
that channel is in [control-protocol.md](control-protocol.md).

## What the protocol says

- Protocol version `2025-06-18`. A client that asks for `2025-03-26` or `2024-11-05` is
  answered in its own revision; anything else gets ours.
- Capabilities: `tools` and `resources`. No prompts, no sampling, no change notifications.
- `initialize`, `ping`, `tools/list`, `tools/call`, `resources/list`,
  `resources/templates/list` and `resources/read` are implemented. Any other method is a
  JSON-RPC "method not found".
- A notification (a request with no `id`) gets no reply.
- A tool that cannot do its job answers with `isError: true` and says why, rather than failing
  the call, so the model can read the reason and try something else. Only a tool that does not
  exist is a protocol error.

## Tools

| Tool | Runs | Notes |
|---|---|---|
| `list_commands` | the registry | every command and its parameters |
| `execute` | any command by id | the escape hatch, including `node.delete` |
| `tree` | `tree.read` | one line per node, with the path the other tools take |
| `search` | `search.query` | page, path and a snippet |
| `read_page` | `page.load` | format, title and body |
| `write_page` | `page.save` | replaces the body; read it first |
| `new_notebook` | `notebook.create` | starts with one section and one page |
| `new_node` | `node.create` | section group, section or page |
| `check` | `notebook.check` | reports, changes nothing |
| `inspect` | `ui.inspect` | needs the window |
| `open` | `ui.open`, or `open.page` alone | a notebook, section group, section or page; anything but a page opens on its first page. With no window, an error that names the page and says how to start a window on it |
| `screenshot` | `ui.screenshot` | needs the window |

Deleting is deliberately not a tool of its own. `execute` can run `node.delete`, so a model has
to name the command to throw anything away.

## Resources

| URI | What |
|---|---|
| `notecraft://tree` | every notebook, section and page with its path |
| `notecraft://commands` | the command registry as JSON |
| `notecraft://page/{path}` | one page's body, for example `notecraft://page/work/projects/kickoff` |

## What it does not do

- No network: the headless server touches only the notebooks folder; the connected one talks to
  `127.0.0.1` and nothing else. Nothing is uploaded anywhere.
- No OAuth, no HTTP transport, no server-sent events. Stdio only, which is what a desktop
  assistant uses.
- No prompts or sampling capability.
- A tool cannot reach outside the notebooks folder: paths are resolved and confined by the
  engine, the same as for the window.

## Tests

`crates/mcp` tests the lifecycle (initialize with three protocol revisions, ping,
notifications, junk, unknown methods), the tool list and its schemas, a whole notebook made,
written, searched and checked through the tools, the errors a tool reports instead of failing,
the resources, and a conversation read from a pipe. The command line has its own test that a
bad `--connect` is reported rather than hung on.

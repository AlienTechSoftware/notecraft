//! JSON-RPC 2.0 over newline-delimited stdio: the MCP lifecycle, tools and resources.

use crate::backend::Backend;
use crate::tools;
use serde_json::{Value, json};
use std::io::{BufRead, Write};

/// The protocol revision this server speaks, and the older ones it will answer in.
pub const PROTOCOL_VERSION: &str = "2025-06-18";
const SUPPORTED_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const RESOURCE_NOT_FOUND: i64 = -32002;

const SERVER_NAME: &str = "notecraft";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the model is told once, at the start.
const INSTRUCTIONS: &str = "\
NoteCraft keeps notes as ordinary folders: a notebook holds section groups and sections, a \
section holds pages, and a page is a folder with index.html or index.md and an assets folder \
beside it. Paths in these tools are relative to the notebooks folder, lower case with slashes, \
for example work/projects/kickoff.

Every action is an engine command. `list_commands` shows all of them with their parameters and \
`execute` runs any one; the other tools are shortcuts for the common ones. Start with `tree` to \
see what exists. Write a page with `write_page`: the body is the editable part, HTML inside the \
body element or Markdown after the front matter, and it replaces what was there, so read the \
page first unless you mean to.";

/// Reads one request per line and writes one reply per line until the input ends.
pub fn serve(backend: &mut dyn Backend, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if let Some(reply) = handle_line(backend, &line) {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
    Ok(())
}

/// Answers one line. `None` means nothing is sent back: a blank line, or a notification.
pub fn handle_line(backend: &mut dyn Backend, line: &str) -> Option<String> {
    if line.trim().is_empty() {
        return None;
    }
    let reply = match serde_json::from_str::<Value>(line) {
        Ok(req) => handle(backend, &req)?,
        Err(e) => error_reply(&Value::Null, PARSE_ERROR, &format!("not valid JSON: {e}")),
    };
    Some(reply.to_string())
}

/// Answers one request object. `None` for a notification, which gets no reply.
fn handle(backend: &mut dyn Backend, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return Some(error_reply(&id.unwrap_or(Value::Null), INVALID_REQUEST, "a request needs a method"));
    };
    // A notification has no id, so it gets no reply, not even for an unknown method.
    let id = id?;
    let params = req.get("params").cloned().unwrap_or(json!({}));
    Some(match request(backend, method, &params) {
        Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
        Err((code, message)) => error_reply(&id, code, &message),
    })
}

fn error_reply(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn request(backend: &mut dyn Backend, method: &str, params: &Value) -> Result<Value, (i64, String)> {
    match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL_VERSION);
            let version = if SUPPORTED_VERSIONS.contains(&asked) { asked } else { PROTOCOL_VERSION };
            Ok(json!({
                "protocolVersion": version,
                "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false}},
                "serverInfo": {"name": SERVER_NAME, "version": SERVER_VERSION},
                "instructions": format!("{INSTRUCTIONS}\n\nThese tools reach {}.", backend.describe()),
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools::definitions(backend.has_window())})),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).ok_or((INVALID_PARAMS, "a tool call needs a name".to_string()))?;
            let empty = json!({});
            let args = params.get("arguments").unwrap_or(&empty);
            if !tools::exists(name) {
                return Err((INVALID_PARAMS, format!("there is no tool called '{name}'")));
            }
            // A tool that fails is reported in the result, not as a protocol error: the model
            // is meant to read the reason and try something else.
            Ok(match tools::call(backend, name, args) {
                Ok(text) => json!({"content": [{"type": "text", "text": text}], "isError": false}),
                Err(e) => json!({"content": [{"type": "text", "text": e}], "isError": true}),
            })
        }
        "resources/list" => Ok(json!({"resources": tools::resources()})),
        "resources/templates/list" => Ok(json!({"resourceTemplates": tools::resource_templates()})),
        "resources/read" => {
            let uri = params.get("uri").and_then(Value::as_str).ok_or((INVALID_PARAMS, "a resource read needs a uri".to_string()))?;
            let text = tools::read_resource(backend, uri).map_err(|e| (RESOURCE_NOT_FOUND, e))?;
            Ok(json!({"contents": [{"uri": uri, "mimeType": "text/plain", "text": text}]}))
        }
        other => Err((METHOD_NOT_FOUND, format!("this server does not have a method '{other}'"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Headless;
    use notecraft_store::Settings;
    use notecraft_store::testing::TempDir;

    fn backend(name: &str) -> (TempDir, Headless) {
        let t = TempDir::new(name);
        let settings = Settings::at(t.path().join("settings.toml"));
        let h = Headless::open(&t.path().join("nb"), settings).expect("open");
        (t, h)
    }

    fn ask(b: &mut dyn Backend, line: &str) -> Value {
        let reply = handle_line(b, line).expect("a reply");
        serde_json::from_str(&reply).expect("the reply is JSON")
    }

    fn call(b: &mut dyn Backend, id: u32, name: &str, args: Value) -> Value {
        let req = json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}});
        ask(b, &req.to_string())
    }

    fn text_of(reply: &Value) -> String {
        reply.pointer("/result/content/0/text").and_then(Value::as_str).unwrap_or_default().to_string()
    }

    #[test]
    fn it_starts_the_way_the_protocol_says() {
        let (_t, mut b) = backend("mcp-server1");
        let r = ask(&mut b, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": PROTOCOL_VERSION}}).to_string());
        assert_eq!(r.pointer("/result/protocolVersion").and_then(Value::as_str), Some(PROTOCOL_VERSION));
        assert_eq!(r.pointer("/result/serverInfo/name").and_then(Value::as_str), Some("notecraft"));
        assert!(r.pointer("/result/capabilities/tools").is_some());
        let instructions = r.pointer("/result/instructions").and_then(Value::as_str).unwrap_or_default();
        assert!(instructions.contains("no window is running"), "{instructions}");

        // An older client is answered in its own revision; an unknown one in ours.
        let older = ask(&mut b, &json!({"id": 2, "method": "initialize", "params": {"protocolVersion": "2024-11-05"}}).to_string());
        assert_eq!(older.pointer("/result/protocolVersion").and_then(Value::as_str), Some("2024-11-05"));
        let future = ask(&mut b, &json!({"id": 3, "method": "initialize", "params": {"protocolVersion": "3000-01-01"}}).to_string());
        assert_eq!(future.pointer("/result/protocolVersion").and_then(Value::as_str), Some(PROTOCOL_VERSION));

        assert_eq!(ask(&mut b, &json!({"id": 4, "method": "ping"}).to_string()).pointer("/result"), Some(&json!({})));
    }

    #[test]
    fn a_notification_gets_no_reply_and_junk_does() {
        let (_t, mut b) = backend("mcp-server2");
        assert!(handle_line(&mut b, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string()).is_none());
        assert!(handle_line(&mut b, "   ").is_none());
        let junk = ask(&mut b, "{not json");
        assert_eq!(junk.pointer("/error/code").and_then(Value::as_i64), Some(PARSE_ERROR));
        let no_method = ask(&mut b, &json!({"id": 1}).to_string());
        assert_eq!(no_method.pointer("/error/code").and_then(Value::as_i64), Some(INVALID_REQUEST));
        let unknown = ask(&mut b, &json!({"id": 2, "method": "nope"}).to_string());
        assert_eq!(unknown.pointer("/error/code").and_then(Value::as_i64), Some(METHOD_NOT_FOUND));
    }

    #[test]
    fn the_tools_are_listed_with_schemas() {
        let (_t, mut b) = backend("mcp-server3");
        let r = ask(&mut b, &json!({"id": 1, "method": "tools/list"}).to_string());
        let list = r.pointer("/result/tools").and_then(Value::as_array).cloned().unwrap_or_default();
        assert!(list.len() >= 9, "{} tools", list.len());
        for t in &list {
            assert!(t.get("name").and_then(Value::as_str).is_some_and(|n| !n.is_empty()));
            assert!(t.get("description").and_then(Value::as_str).is_some_and(|d| d.len() > 20));
            assert_eq!(t.pointer("/inputSchema/type").and_then(Value::as_str), Some("object"));
        }
        let names: Vec<&str> = list.iter().filter_map(|t| t.get("name").and_then(Value::as_str)).collect();
        assert!(names.contains(&"tree") && names.contains(&"execute") && names.contains(&"write_page"));
        // Without a window, the window-only tools are not offered.
        assert!(!names.contains(&"inspect"), "{names:?}");
    }

    #[test]
    fn a_notebook_is_made_read_and_written_through_the_tools() {
        let (_t, mut b) = backend("mcp-server4");
        let made = call(&mut b, 1, "new_notebook", json!({"name": "Work"}));
        assert_eq!(made.pointer("/result/isError").and_then(Value::as_bool), Some(false));
        let tree = text_of(&call(&mut b, 2, "tree", json!({})));
        assert!(tree.contains("Work  [notebook") && tree.contains("Untitled page"), "{tree}");

        call(&mut b, 3, "new_node", json!({"parent": "work", "kind": "page", "title": "Kickoff", "format": "cmgfm"}));
        let written = call(&mut b, 4, "write_page", json!({"path": "work/kickoff", "body": "# Kickoff\n\nship it"}));
        assert_eq!(written.pointer("/result/isError").and_then(Value::as_bool), Some(false));
        let page = text_of(&call(&mut b, 5, "read_page", json!({"path": "work/kickoff"})));
        assert!(page.contains("ship it"), "{page}");
        let hits = text_of(&call(&mut b, 6, "search", json!({"query": "ship"})));
        assert!(hits.contains("Kickoff"), "{hits}");
        let report = text_of(&call(&mut b, 7, "check", json!({"path": "work"})));
        assert!(report.contains("\"problems\": []"), "{report}");
        let ran = text_of(&call(&mut b, 8, "execute", json!({"command": "node.title", "params": {"path": "work/kickoff", "title": "Kick off"}})));
        assert!(!ran.is_empty());
        assert!(text_of(&call(&mut b, 9, "tree", json!({"path": "work"}))).contains("Kick off"));
    }

    #[test]
    fn a_tool_that_cannot_work_says_why_without_failing_the_protocol() {
        let (_t, mut b) = backend("mcp-server5");
        let missing = call(&mut b, 1, "read_page", json!({}));
        assert_eq!(missing.pointer("/result/isError").and_then(Value::as_bool), Some(true));
        assert!(text_of(&missing).contains("path"), "{}", text_of(&missing));

        let gone = call(&mut b, 2, "read_page", json!({"path": "nothing/here"}));
        assert_eq!(gone.pointer("/result/isError").and_then(Value::as_bool), Some(true));

        // A window-only tool, asked for anyway, explains how to get one.
        let ui = call(&mut b, 3, "inspect", json!({}));
        assert_eq!(ui.pointer("/result/isError").and_then(Value::as_bool), Some(true));
        assert!(text_of(&ui).contains("--control"), "{}", text_of(&ui));

        // A tool that does not exist at all is a protocol error, not a tool result.
        let nope = call(&mut b, 4, "fly", json!({}));
        assert_eq!(nope.pointer("/error/code").and_then(Value::as_i64), Some(INVALID_PARAMS));
    }

    #[test]
    fn the_resources_are_the_tree_and_the_commands() {
        let (_t, mut b) = backend("mcp-server6");
        call(&mut b, 1, "new_notebook", json!({"name": "Work"}));
        let list = ask(&mut b, &json!({"id": 2, "method": "resources/list"}).to_string());
        let uris: Vec<String> = list
            .pointer("/result/resources")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|r| r.get("uri").and_then(Value::as_str)).map(str::to_string).collect())
            .unwrap_or_default();
        assert!(uris.contains(&"notecraft://tree".to_string()) && uris.contains(&"notecraft://commands".to_string()), "{uris:?}");

        let read = ask(&mut b, &json!({"id": 3, "method": "resources/read", "params": {"uri": "notecraft://tree"}}).to_string());
        assert!(read.pointer("/result/contents/0/text").and_then(Value::as_str).is_some_and(|t| t.contains("Work")));

        let page = ask(
            &mut b,
            &json!({"id": 4, "method": "resources/read", "params": {"uri": "notecraft://page/work/new-section-1/untitled-page"}}).to_string(),
        );
        assert!(page.pointer("/result/contents/0/text").is_some(), "{page}");

        let missing = ask(&mut b, &json!({"id": 5, "method": "resources/read", "params": {"uri": "notecraft://nope"}}).to_string());
        assert_eq!(missing.pointer("/error/code").and_then(Value::as_i64), Some(RESOURCE_NOT_FOUND));

        let templates = ask(&mut b, &json!({"id": 6, "method": "resources/templates/list"}).to_string());
        assert!(templates.pointer("/result/resourceTemplates/0/uriTemplate").and_then(Value::as_str).is_some_and(|u| u.contains("{")));
    }

    #[test]
    fn it_reads_a_whole_conversation_from_a_pipe() {
        let (_t, mut b) = backend("mcp-server7");
        let input = format!(
            "{}\n{}\n\n{}\n",
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        );
        let mut out: Vec<u8> = Vec::new();
        serve(&mut b, std::io::Cursor::new(input), &mut out).expect("serve");
        let lines: Vec<&str> = std::str::from_utf8(&out).unwrap_or_default().lines().collect();
        assert_eq!(lines.len(), 2, "one reply each for the two requests, none for the notification: {lines:?}");
        assert!(lines.first().is_some_and(|l| l.contains("protocolVersion")));
        assert!(lines.get(1).is_some_and(|l| l.contains("inputSchema")));
    }
}

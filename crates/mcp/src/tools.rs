//! The tools the model sees, and what each one runs.
//!
//! Every tool is a shortcut for an engine command, except the three that drive a running
//! window. Deleting is deliberately not a tool of its own: `execute` can run `node.delete`,
//! but a model has to mean it.

use crate::backend::Backend;
use serde_json::{Value, json};

/// One parameter of a tool: name, JSON type, required, what it means.
type Param = (&'static str, &'static str, bool, &'static str);

pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub params: &'static [Param],
    /// True if it needs a running window, so it is only offered when one is connected.
    pub needs_window: bool,
    /// True if it can change what is on disk, which clients show to the person.
    pub writes: bool,
}

pub static TOOLS: &[Tool] = &[
    Tool {
        name: "list_commands",
        description: "Every NoteCraft command with its parameters. These are the same actions the window's menus and buttons run; `execute` runs any of them.",
        params: &[],
        needs_window: false,
        writes: false,
    },
    Tool {
        name: "execute",
        description: "Run any command from `list_commands` by id, for example node.delete or settings.default_format.set. The other tools are shortcuts for the usual ones.",
        params: &[
            ("command", "string", true, "the command's id, as `list_commands` gives it"),
            ("params", "object", false, "the command's parameters, as named in `list_commands`"),
        ],
        needs_window: false,
        writes: true,
    },
    Tool {
        name: "tree",
        description: "List notebooks, section groups, sections and pages, one per line, indented, each with its kind and the path the other tools take.",
        params: &[("path", "string", false, "start here instead of listing every notebook")],
        needs_window: false,
        writes: false,
    },
    Tool {
        name: "search",
        description: "Find text in every page. Answers with the matching pages, their paths and a snippet around the match.",
        params: &[("query", "string", true, "the text to find"), ("limit", "number", false, "at most this many hits (default 40)")],
        needs_window: false,
        writes: false,
    },
    Tool {
        name: "read_page",
        description: "Read one page: its format, its title and its body, which is the HTML inside the body element or the Markdown after the front matter.",
        params: &[("path", "string", true, "the page, for example work/projects/kickoff")],
        needs_window: false,
        writes: false,
    },
    Tool {
        name: "write_page",
        description: "Replace a page's body. Read the page first unless you mean to discard what is there. The body must match the page's format: HTML for an html page, Markdown for a cmgfm one.",
        params: &[("path", "string", true, "the page"), ("body", "string", true, "the new body")],
        needs_window: false,
        writes: true,
    },
    Tool {
        name: "new_notebook",
        description: "Create a notebook. It starts with one section and one empty page, the way a desktop notebook app does.",
        params: &[("name", "string", true, "the notebook's title"), ("format", "string", false, "html or cmgfm; default: the person's setting")],
        needs_window: false,
        writes: true,
    },
    Tool {
        name: "new_node",
        description: "Create a section group, a section or a page inside a notebook, group or section.",
        params: &[
            ("parent", "string", true, "path of the notebook, group or section to add to"),
            ("kind", "string", true, "section-group, section or page"),
            ("title", "string", true, "the title"),
            ("format", "string", false, "html or cmgfm"),
        ],
        needs_window: false,
        writes: true,
    },
    Tool {
        name: "check",
        description: "Check a notebook against the folder rules and report anything wrong with it. Nothing is changed.",
        params: &[("path", "string", true, "the notebook")],
        needs_window: false,
        writes: false,
    },
    Tool {
        name: "inspect",
        description: "What the window is showing: the visible rows, the open page, where the caret is, the theme and the window size. Needs a running window.",
        params: &[],
        needs_window: true,
        writes: false,
    },
    Tool {
        name: "open_page",
        description: "Open a page in the running window, so the person sees it. Needs a running window.",
        params: &[("path", "string", true, "the page to open")],
        needs_window: true,
        writes: false,
    },
    Tool {
        name: "screenshot",
        description: "A picture of the running window, saved to a file. Needs a running window.",
        params: &[("path", "string", false, "where to save the PNG; without it the picture comes back as base64")],
        needs_window: true,
        writes: false,
    },
];

pub fn exists(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

/// The tool list for `tools/list`. Window-only tools are left out when nothing is running,
/// so the model is not offered something that can only fail.
pub fn definitions(has_window: bool) -> Vec<Value> {
    TOOLS
        .iter()
        .filter(|t| has_window || !t.needs_window)
        .map(|t| {
            let properties: serde_json::Map<String, Value> =
                t.params.iter().map(|(name, ty, _, about)| ((*name).to_string(), json!({"type": ty, "description": about}))).collect();
            let required: Vec<&str> = t.params.iter().filter(|(_, _, req, _)| *req).map(|(name, ..)| *name).collect();
            json!({
                "name": t.name,
                "description": t.description,
                "inputSchema": {"type": "object", "properties": properties, "required": required},
                "annotations": {"readOnlyHint": !t.writes, "destructiveHint": t.name == "execute"},
            })
        })
        .collect()
}

fn text(args: &Value, key: &str) -> Result<String, String> {
    match args.get(key) {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(v) => Ok(v.to_string()),
        None => Err(format!("this tool needs a '{key}'")),
    }
}

fn optional(args: &Value, key: &str) -> Option<String> {
    match args.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()),
        Some(Value::String(_)) | None | Some(Value::Null) => None,
        Some(v) => Some(v.to_string()),
    }
}

/// Runs a command and hands back its result as text.
fn execute(backend: &mut dyn Backend, command: &str, params: Value) -> Result<String, String> {
    backend.call("engine.execute", &json!({"command": command, "params": params})).map(|v| render(&v))
}

/// A result as the model reads it: text as it is, everything else as indented JSON.
fn render(v: &Value) -> String {
    match v {
        Value::Null => "done".to_string(),
        Value::String(s) if s.is_empty() => "(nothing)".to_string(),
        Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_else(|_| other.to_string()),
    }
}

/// A page as the model reads it: what it is, then the body on its own.
fn render_page(v: &Value) -> String {
    let field = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default();
    match v.as_object() {
        Some(_) => format!("format: {}\ntitle: {}\n\n{}", field("format"), field("title"), field("body")),
        None => render(v),
    }
}

pub fn call(backend: &mut dyn Backend, name: &str, args: &Value) -> Result<String, String> {
    match name {
        "list_commands" => backend.call("engine.commands", &json!({})).map(|v| render(&v)),
        "execute" => {
            let command = text(args, "command")?;
            execute(backend, &command, args.get("params").cloned().unwrap_or(json!({})))
        }
        "tree" => {
            let mut p = json!({});
            if let (Some(path), Some(obj)) = (optional(args, "path"), p.as_object_mut()) {
                obj.insert("path".into(), json!(path));
            }
            let out = execute(backend, "tree.read", p)?;
            Ok(if out == "(nothing)" { "there are no notebooks yet".to_string() } else { out })
        }
        "search" => {
            let mut p = json!({"q": text(args, "query")?});
            if let (Some(limit), Some(obj)) = (optional(args, "limit"), p.as_object_mut()) {
                obj.insert("limit".into(), json!(limit));
            }
            let out = execute(backend, "search.query", p)?;
            Ok(if out == "[]" { "nothing matched".to_string() } else { out })
        }
        "read_page" => {
            backend.call("engine.execute", &json!({"command": "page.load", "params": {"path": text(args, "path")?}})).map(|v| render_page(&v))
        }
        "write_page" => {
            let path = text(args, "path")?;
            execute(backend, "page.save", json!({"path": path.clone(), "body": text(args, "body")?}))?;
            Ok(format!("saved {path}"))
        }
        "new_notebook" => {
            let mut p = json!({"name": text(args, "name")?});
            if let (Some(format), Some(obj)) = (optional(args, "format"), p.as_object_mut()) {
                obj.insert("format".into(), json!(format));
            }
            execute(backend, "notebook.create", p)
        }
        "new_node" => {
            let mut p = json!({"parent": text(args, "parent")?, "kind": text(args, "kind")?, "title": text(args, "title")?});
            if let (Some(format), Some(obj)) = (optional(args, "format"), p.as_object_mut()) {
                obj.insert("format".into(), json!(format));
            }
            execute(backend, "node.create", p)
        }
        "check" => execute(backend, "notebook.check", json!({"path": text(args, "path")?})),
        "inspect" => backend.call("ui.inspect", &json!({})).map(|v| render(&v)),
        "open_page" => backend.call("ui.open", &json!({"path": text(args, "path")?})).map(|v| render(&v)),
        "screenshot" => {
            let mut p = json!({});
            if let (Some(path), Some(obj)) = (optional(args, "path"), p.as_object_mut()) {
                obj.insert("path".into(), json!(path));
            }
            backend.call("ui.screenshot", &p).map(|v| render(&v))
        }
        other => Err(format!("there is no tool called '{other}'")),
    }
}

const TREE_URI: &str = "notecraft://tree";
const COMMANDS_URI: &str = "notecraft://commands";
const PAGE_PREFIX: &str = "notecraft://page/";

pub fn resources() -> Vec<Value> {
    vec![
        json!({"uri": TREE_URI, "name": "Notebook tree", "description": "Every notebook, section and page, with its path", "mimeType": "text/plain"}),
        json!({"uri": COMMANDS_URI, "name": "Commands", "description": "Every NoteCraft command and its parameters", "mimeType": "application/json"}),
    ]
}

pub fn resource_templates() -> Vec<Value> {
    vec![json!({
        "uriTemplate": "notecraft://page/{path}",
        "name": "Page",
        "description": "One page's body, by its path, for example notecraft://page/work/projects/kickoff",
        "mimeType": "text/plain",
    })]
}

pub fn read_resource(backend: &mut dyn Backend, uri: &str) -> Result<String, String> {
    match uri {
        TREE_URI => execute(backend, "tree.read", json!({})),
        COMMANDS_URI => backend.call("engine.commands", &json!({})).map(|v| render(&v)),
        _ => match uri.strip_prefix(PAGE_PREFIX) {
            Some(path) if !path.is_empty() => {
                backend.call("engine.execute", &json!({"command": "page.load", "params": {"path": path}})).map(|v| render_page(&v))
            }
            _ => Err(format!("there is no resource at '{uri}'")),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_is_described_and_handled() {
        for t in TOOLS {
            assert!(exists(t.name));
            assert!(t.description.len() > 30, "{} needs a description", t.name);
            assert!(t.description.ends_with('.'), "{} should read as a sentence", t.name);
            for (name, ty, _, about) in t.params {
                assert!(["string", "number", "object", "boolean"].contains(ty), "{}: odd type {ty}", t.name);
                assert!(!name.is_empty() && !about.is_empty());
            }
        }
        assert!(!exists("delete_everything"));
    }

    #[test]
    fn the_window_tools_appear_only_with_a_window() {
        let without = definitions(false).len();
        let with = definitions(true).len();
        assert_eq!(with, TOOLS.len());
        assert_eq!(with - without, TOOLS.iter().filter(|t| t.needs_window).count());
        assert!(without > 0);
    }

    #[test]
    fn arguments_are_read_leniently_but_not_invented() {
        let args = json!({"path": "work", "limit": 5, "blank": "  "});
        assert_eq!(text(&args, "path").as_deref(), Ok("work"));
        assert_eq!(text(&args, "limit").as_deref(), Ok("5"));
        assert!(text(&args, "missing").is_err());
        assert_eq!(optional(&args, "limit").as_deref(), Some("5"));
        assert_eq!(optional(&args, "blank"), None);
        assert_eq!(optional(&args, "missing"), None);
    }

    #[test]
    fn results_are_rendered_for_reading() {
        assert_eq!(render(&Value::Null), "done");
        assert_eq!(render(&json!("a line")), "a line");
        assert_eq!(render(&json!({"a": 1})), "{\n  \"a\": 1\n}");
        let page = render_page(&json!({"format": "cmgfm", "title": "Plan", "body": "# Plan"}));
        assert_eq!(page, "format: cmgfm\ntitle: Plan\n\n# Plan");
        assert_eq!(render_page(&json!("not a page")), "not a page");
    }
}

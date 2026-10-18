//! A command's registry entry and its output as JSON.
//!
//! The control channel and the MCP server both answer in JSON, and they must answer the same
//! way, so the shapes live here rather than in either of them.

use crate::commands::{Output, Params, registry};
use notecraft_store::tree::display_path;
use serde_json::{Value, json};

/// Every command: `id`, `label`, `params`, `changesTree`.
pub fn commands_json() -> Value {
    Value::Array(registry().iter().map(|c| json!({"id": c.id, "label": c.label, "params": c.params, "changesTree": c.changes_tree})).collect())
}

/// A command's result as JSON.
pub fn output_json(o: &Output) -> Value {
    match o {
        Output::None => Value::Null,
        Output::Path(p) => json!(display_path(p)),
        Output::Text(t) => json!(t),
        Output::Count(n) => json!(n),
        Output::Page(p) => json!({"format": p.format.id(), "title": p.title, "body": p.body}),
        Output::Hits(h) => Value::Array(h.iter().map(|x| json!({"path": display_path(&x.path), "title": x.title, "snippet": x.snippet})).collect()),
        Output::Report(r) => {
            json!({"nodes": r.nodes, "problems": r.problems.iter().map(|p| json!({"path": display_path(&p.path), "message": p.message})).collect::<Vec<_>>()})
        }
    }
}

/// A JSON object of parameters as the string map every command takes. A value that is not a
/// string keeps its JSON text, so `{"limit": 5}` and `{"limit": "5"}` mean the same thing.
pub fn params_from_json(v: Option<&Value>) -> Params {
    let mut params = Params::new();
    if let Some(obj) = v.and_then(Value::as_object) {
        for (k, v) in obj {
            params.insert(k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
        }
    }
    params
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_is_listed_with_its_parameters() {
        let v = commands_json();
        let list = v.as_array().map(Vec::as_slice).unwrap_or_default();
        assert_eq!(list.len(), registry().len());
        let first = list.first().and_then(|c| c.get("id")).and_then(Value::as_str);
        assert_eq!(first, Some("notebook.create"));
        assert!(list.iter().all(|c| c.get("params").is_some() && c.get("changesTree").is_some()));
    }

    #[test]
    fn outputs_turn_into_their_own_shapes() {
        assert_eq!(output_json(&Output::None), Value::Null);
        assert_eq!(output_json(&Output::Count(3)), json!(3));
        assert_eq!(output_json(&Output::Text("hi".into())), json!("hi"));
    }

    #[test]
    fn parameters_come_from_strings_or_numbers() {
        let p = params_from_json(Some(&json!({"q": "notes", "limit": 5})));
        assert_eq!(p.get("q").map(String::as_str), Some("notes"));
        assert_eq!(p.get("limit").map(String::as_str), Some("5"));
        assert!(params_from_json(None).is_empty());
        assert!(params_from_json(Some(&json!("not an object"))).is_empty());
    }
}

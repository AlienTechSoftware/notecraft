//! Where the tools land: a notebooks folder opened here, or a running window.

use notecraft_engine::{Session, commands_json, output_json, params_from_json};
use notecraft_store::Settings;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What a tool call reaches. The methods are the control channel's, so one tool works the
/// same whether a window is running or not.
pub trait Backend {
    /// Runs one control method, for example `engine.execute` or `ui.inspect`.
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, String>;

    /// True when a window is on the other end, so the `ui.*` tools can be offered.
    fn has_window(&self) -> bool;

    /// One line for the model: where these tools write.
    fn describe(&self) -> String;
}

/// What a `ui.*` tool answers with when nothing is running.
pub const NEEDS_APP: &str = "that needs a running NoteCraft window. Start one with `notecraft --control 0`, \
then run the MCP server with `notecraft-cli mcp --connect PORT`.";

/// A notebooks folder opened by the server itself: no window, no control channel.
pub struct Headless {
    session: Session,
}

impl Headless {
    /// Opens (and creates, if needed) the notebooks folder `root`.
    pub fn open(root: &Path, settings: Settings) -> Result<Headless, String> {
        Ok(Headless { session: Session::new(root, settings).map_err(|e| e.to_string())? })
    }

    pub fn session(&mut self) -> &mut Session {
        &mut self.session
    }
}

impl Backend for Headless {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "engine.commands" => Ok(commands_json()),
            "engine.execute" => {
                let id = params.get("command").and_then(Value::as_str).ok_or("missing command")?;
                let out = self.session.run(id, &params_from_json(params.get("params"))).map_err(|e| e.to_string())?;
                Ok(output_json(&out))
            }
            m if m.starts_with("ui.") || m == "app.quit" => Err(NEEDS_APP.to_string()),
            other => Err(format!("unknown method '{other}'")),
        }
    }

    fn has_window(&self) -> bool {
        false
    }

    fn describe(&self) -> String {
        format!("the notebooks folder {} (no window is running)", notecraft_store::tree::display_path(self.session.root()))
    }
}

/// The folder the key files live in: beside the settings file, where the window writes them.
pub fn key_dir(settings: &Settings) -> PathBuf {
    settings.path().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

const REPLY_TIMEOUT: Duration = Duration::from_secs(60);

/// A running window, over its control channel.
pub struct Remote {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
    port: u16,
    next_id: u64,
    key: Option<String>,
}

impl Remote {
    /// Connects to a window listening on `127.0.0.1:port`. The key is `NOTECRAFT_CONTROL_KEY`
    /// if it is set, else the window's own `control-key.local-PORT` file in `key_dir`.
    pub fn connect(port: u16, key_dir: &Path) -> Result<Remote, String> {
        let key = match std::env::var("NOTECRAFT_CONTROL_KEY") {
            Ok(k) if !k.trim().is_empty() => k.trim().to_string(),
            _ => {
                let file = key_dir.join(format!("control-key.local-{port}"));
                std::fs::read_to_string(&file)
                    .map_err(|e| format!("cannot read the window's key from {}: {e}. Is NoteCraft running with --control {port}?", file.display()))?
                    .trim()
                    .to_string()
            }
        };
        let stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| format!("cannot reach 127.0.0.1:{port}: {e}"))?;
        stream.set_read_timeout(Some(REPLY_TIMEOUT)).map_err(|e| e.to_string())?;
        let reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
        let mut remote = Remote { stream, reader, port, next_id: 1, key: Some(key) };
        remote.call("ui.inspect", &json!({})).map_err(|e| format!("the window did not answer: {e}"))?;
        Ok(remote)
    }
}

impl Backend for Remote {
    fn call(&mut self, method: &str, params: &Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let mut req = json!({"id": id, "method": method, "params": params});
        // The key is needed on the first request of a connection only.
        if let (Some(key), Some(obj)) = (self.key.take(), req.as_object_mut()) {
            obj.insert("key".into(), json!(key));
        }
        writeln!(self.stream, "{req}").and_then(|()| self.stream.flush()).map_err(|e| format!("cannot send to the window: {e}"))?;
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => return Err("the window closed the connection".into()),
            Ok(_) => {}
            Err(e) => return Err(format!("no answer from the window: {e}")),
        }
        let reply: Value = serde_json::from_str(&line).map_err(|e| format!("the window sent something that is not JSON: {e}"))?;
        if reply.get("ok").and_then(Value::as_bool) == Some(true) {
            Ok(reply.get("result").cloned().unwrap_or(Value::Null))
        } else {
            Err(reply.get("error").and_then(Value::as_str).unwrap_or("the window refused the request").to_string())
        }
    }

    fn has_window(&self) -> bool {
        true
    }

    fn describe(&self) -> String {
        format!("the NoteCraft window on 127.0.0.1:{} (what the person is looking at)", self.port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;

    fn headless(name: &str) -> (TempDir, Headless) {
        let t = TempDir::new(name);
        let settings = Settings::at(t.path().join("settings.toml"));
        let h = Headless::open(&t.path().join("nb"), settings).unwrap_or_else(|e| panic!("{e}"));
        (t, h)
    }

    #[test]
    fn the_headless_backend_runs_commands() {
        let (_t, mut h) = headless("mcp-backend1");
        let list = h.call("engine.commands", &json!({})).unwrap_or_else(|e| panic!("{e}"));
        assert!(list.as_array().is_some_and(|a| a.len() > 5));
        let made = h.call("engine.execute", &json!({"command": "notebook.create", "params": {"name": "Work"}}));
        assert!(made.is_ok(), "{made:?}");
        let tree = h.call("engine.execute", &json!({"command": "tree.read", "params": {}})).unwrap_or_else(|e| panic!("{e}"));
        let text = tree.as_str().unwrap_or_default();
        assert!(text.contains("Work  [notebook") && text.contains("New Section 1"), "{text}");
    }

    #[test]
    fn the_headless_backend_says_what_it_cannot_do() {
        let (_t, mut h) = headless("mcp-backend2");
        assert_eq!(h.call("ui.inspect", &json!({})), Err(NEEDS_APP.to_string()));
        assert!(h.call("nonsense", &json!({})).is_err_and(|e| e.contains("unknown method")));
        assert!(h.call("engine.execute", &json!({})).is_err_and(|e| e.contains("missing command")));
        assert!(h.call("engine.execute", &json!({"command": "no.such"})).is_err());
        assert!(!h.has_window() && h.describe().contains("no window"));
    }

    #[test]
    fn a_missing_window_is_reported_not_guessed() {
        let t = TempDir::new("mcp-backend3");
        // Port 1 is not something a window listens on, and there is no key file either.
        let e = Remote::connect(1, t.path()).err().unwrap_or_default();
        assert!(e.contains("cannot read the window's key"), "{e}");
    }

    #[test]
    fn the_key_folder_is_beside_the_settings_file() {
        let s = Settings::at(PathBuf::from("a").join("b").join("settings.toml"));
        assert_eq!(key_dir(&s), PathBuf::from("a").join("b"));
        assert_eq!(key_dir(&Settings::at("settings.toml")), PathBuf::from(""));
    }
}

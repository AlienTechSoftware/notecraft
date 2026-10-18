//! The control channel: scripted access to a running NoteCraft over a loopback port.
//!
//! Start the app with `--control PORT`. It listens on `127.0.0.1` only and speaks JSON
//! lines: one request object per line, one reply line each. Every request needs the window's
//! key, a random 256-bit value written to `control-key.local-PORT` next to the settings file
//! and removed when the window closes. The protocol is in `docs/control-protocol.md`; it
//! mirrors the control channel of the other Craft apps so the same tools can drive them.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

/// A request from a connection, waiting for the window to answer it.
pub struct ControlRequest {
    pub method: String,
    pub params: Value,
    reply: Sender<Result<Value, String>>,
}

impl ControlRequest {
    /// A request and the receiver its answer arrives on (used by the headless runner and tests).
    pub fn new(method: &str, params: Value) -> (ControlRequest, Receiver<Result<Value, String>>) {
        let (tx, rx) = mpsc::channel();
        (ControlRequest { method: method.to_string(), params, reply: tx }, rx)
    }

    /// Sends the answer. A closed connection is not an error.
    pub fn respond(self, result: Result<Value, String>) {
        let _ = self.reply.send(result);
    }
}

/// Keeps the listener's key file alive; removes it when dropped.
pub struct ControlServer {
    pub port: u16,
    pub key_file: PathBuf,
    pub key: String,
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.key_file);
    }
}

const MAX_LINE: usize = 1024 * 1024;
const IDLE: Duration = Duration::from_secs(30);

/// 256 random bits as hex. `RandomState` is seeded from the operating system's random source.
fn random_key() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    (0..4u64)
        .map(|i| {
            let mut h = RandomState::new().build_hasher();
            h.write_u64(nanos);
            h.write_u64(i);
            format!("{:016x}", h.finish())
        })
        .collect()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Starts listening on `127.0.0.1:port` (0 picks a free port). The key file goes in `key_dir`.
/// Requests arrive on the returned receiver; the window answers each with `respond`.
pub fn start(port: u16, key_dir: &Path, ctx: egui::Context) -> Result<(ControlServer, Receiver<ControlRequest>), String> {
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("cannot listen on port {port}: {e}"))?;
    let port = listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())?;
    let key = random_key();
    std::fs::create_dir_all(key_dir).map_err(|e| format!("cannot create {}: {e}", key_dir.display()))?;
    let key_file = key_dir.join(format!("control-key.local-{port}"));
    write_private(&key_file, &key).map_err(|e| format!("cannot write {}: {e}", key_file.display()))?;
    let (tx, rx) = mpsc::channel::<ControlRequest>();
    let server_key = key.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let (tx, key, ctx) = (tx.clone(), server_key.clone(), ctx.clone());
            std::thread::spawn(move || serve_connection(stream, &key, &tx, &ctx));
        }
    });
    Ok((ControlServer { port, key_file, key }, rx))
}

#[cfg(unix)]
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)?;
    f.write_all(text.as_bytes())
}

#[cfg(not(unix))]
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    std::fs::write(path, text)
}

fn reply(stream: &mut TcpStream, id: &Value, result: Result<Value, String>) -> bool {
    let body = match result {
        Ok(v) => json!({"id": id, "ok": true, "result": v}),
        Err(e) => json!({"id": id, "ok": false, "error": e}),
    };
    writeln!(stream, "{body}").and_then(|()| stream.flush()).is_ok()
}

fn serve_connection(mut stream: TcpStream, key: &str, tx: &Sender<ControlRequest>, ctx: &egui::Context) {
    let _ = stream.set_read_timeout(Some(IDLE));
    let Ok(read_half) = stream.try_clone() else { return };
    let mut reader = BufReader::new(read_half);
    let mut authed = false;
    let started = std::time::Instant::now();
    loop {
        let mut line = String::new();
        match reader.by_ref().take(MAX_LINE as u64 + 1).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.len() > MAX_LINE {
            let _ = reply(&mut stream, &Value::Null, Err("line too long".into()));
            return;
        }
        if line.trim().is_empty() {
            if !authed && started.elapsed() > IDLE {
                return;
            }
            continue;
        }
        let Ok(req) = serde_json::from_str::<Value>(&line) else {
            let _ = reply(&mut stream, &Value::Null, Err("not valid JSON".into()));
            return;
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let given = req.get("key").and_then(Value::as_str).unwrap_or("");
        if !(authed || same(given, key)) {
            let _ = reply(&mut stream, &id, Err("unauthorized".into()));
            return;
        }
        authed = true;
        let method = req.get("method").and_then(Value::as_str).unwrap_or("").to_string();
        if method == "auth" {
            if !reply(&mut stream, &id, Ok(json!({}))) {
                return;
            }
            continue;
        }
        let (request, rx) = ControlRequest::new(&method, req.get("params").cloned().unwrap_or(Value::Null));
        if tx.send(request).is_err() {
            let _ = reply(&mut stream, &id, Err("the window is closing".into()));
            return;
        }
        ctx.request_repaint();
        let result = rx.recv_timeout(IDLE).unwrap_or_else(|_| Err("the window did not answer in time".into()));
        if !reply(&mut stream, &id, result) {
            return;
        }
    }
}

/// Standard base64 (RFC 4648) with padding, for screenshots returned in a reply.
pub fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk.first().copied().unwrap_or(0), chunk.get(1).copied().unwrap_or(0), chunk.get(2).copied().unwrap_or(0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(T[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    #[test]
    fn base64_matches_the_rfc_vectors() {
        for (input, want) in
            [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foob", "Zm9vYg=="), ("fooba", "Zm9vYmE="), ("foobar", "Zm9vYmFy")]
        {
            assert_eq!(base64(input.as_bytes()), want);
        }
    }

    #[test]
    fn keys_are_long_hex_and_differ() {
        let (a, b) = (random_key(), random_key());
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(same(&a, &a) && !same(&a, &b) && !same("a", "ab"));
    }

    /// A tiny stand-in for the window: answers every request with its method name.
    fn echo_server(label: &str) -> (ControlServer, std::thread::JoinHandle<()>, super::super::testing_dir::Dir) {
        let dir = super::super::testing_dir::Dir::new(label);
        let (server, rx) = start(0, dir.path(), egui::Context::default()).expect("start");
        let h = std::thread::spawn(move || {
            while let Ok(req) = rx.recv() {
                if req.method == "fail" {
                    req.respond(Err("boom".into()));
                } else {
                    let m = req.method.clone();
                    let p = req.params.clone();
                    req.respond(Ok(json!({"method": m, "params": p})));
                }
            }
        });
        (server, h, dir)
    }

    fn talk(port: u16, lines: &[String]) -> Vec<Value> {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
        let mut r = BufReader::new(s.try_clone().expect("clone"));
        let mut out = Vec::new();
        for l in lines {
            if writeln!(s, "{l}").is_err() {
                break; // the server already closed the connection
            }
            let mut reply = String::new();
            if r.read_line(&mut reply).unwrap_or(0) == 0 {
                break;
            }
            out.push(serde_json::from_str(&reply).expect("json reply"));
        }
        out
    }

    #[test]
    fn requests_need_the_key_and_get_answers() {
        let (server, _h, _dir) = echo_server("ctl1");
        let k = server.key.clone();
        let replies = talk(
            server.port,
            &[format!(r#"{{"id":1,"key":"{k}","method":"ui.inspect","params":{{"x":1}}}}"#), r#"{"id":2,"method":"ui.inspect"}"#.to_string()],
        );
        assert_eq!(replies[0]["ok"], true);
        assert_eq!(replies[0]["id"], 1);
        assert_eq!(replies[0]["result"]["method"], "ui.inspect");
        assert_eq!(replies[0]["result"]["params"]["x"], 1);
        assert_eq!(replies[1]["ok"], true, "the connection stays authorised after the first good request");
    }

    #[test]
    fn a_wrong_or_missing_key_is_refused_and_the_connection_closes() {
        let (server, _h, _dir) = echo_server("ctl2");
        let replies = talk(server.port, &[r#"{"id":1,"key":"nope","method":"x"}"#.into(), r#"{"id":2,"method":"x"}"#.into()]);
        assert_eq!(replies.len(), 1, "closed after the refusal");
        assert_eq!(replies[0]["ok"], false);
        assert_eq!(replies[0]["error"], "unauthorized");
        let none = talk(server.port, &[r#"{"id":1,"method":"x"}"#.into()]);
        assert_eq!(none[0]["error"], "unauthorized");
    }

    #[test]
    fn auth_once_per_connection_and_errors_pass_through() {
        let (server, _h, _dir) = echo_server("ctl3");
        let k = server.key.clone();
        let replies = talk(server.port, &[format!(r#"{{"id":1,"key":"{k}","method":"auth"}}"#), r#"{"id":2,"method":"fail"}"#.into()]);
        assert_eq!(replies[0]["ok"], true);
        assert_eq!(replies[1]["ok"], false);
        assert_eq!(replies[1]["error"], "boom");
    }

    #[test]
    fn junk_closes_the_connection_after_one_error() {
        let (server, _h, _dir) = echo_server("ctl4");
        let replies = talk(server.port, &["GET / HTTP/1.1".into(), r#"{"method":"x"}"#.into()]);
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["error"], "not valid JSON");
    }

    #[test]
    fn an_over_long_line_is_refused() {
        let (server, _h, _dir) = echo_server("ctl5");
        let long = format!(r#"{{"method":"x","pad":"{}"}}"#, "a".repeat(MAX_LINE));
        let replies = talk(server.port, &[long]);
        assert_eq!(replies[0]["error"], "line too long");
    }

    #[test]
    fn the_key_file_exists_while_running_and_goes_away_after() {
        let dir = super::super::testing_dir::Dir::new("ctl6");
        let (server, _rx) = start(0, dir.path(), egui::Context::default()).expect("start");
        let file = server.key_file.clone();
        assert_eq!(std::fs::read_to_string(&file).expect("key"), server.key);
        assert!(file.file_name().is_some_and(|n| n.to_string_lossy() == format!("control-key.local-{}", server.port)));
        drop(server);
        assert!(!file.exists());
    }

    #[test]
    fn a_busy_port_is_an_error_not_a_panic() {
        let dir = super::super::testing_dir::Dir::new("ctl7");
        let (server, _rx) = start(0, dir.path(), egui::Context::default()).expect("start");
        assert!(start(server.port, dir.path(), egui::Context::default()).is_err());
    }
}

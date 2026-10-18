#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft command line. Every command is an engine command, the same ones the window runs.
//!
//! ```text
//! notecraft-cli commands                      list every command and its parameters
//! notecraft-cli run ID key=value ...          run one command
//! notecraft-cli tree                          print the notebooks as a tree
//! notecraft-cli check NOTEBOOK                check a notebook against the folder rules
//! notecraft-cli search TEXT                   search all pages
//! notecraft-cli sample                        create the sample notebooks (for trying the app)
//! ```
//! Add `--notebooks FOLDER` to use another notebooks folder (default: `NOTECRAFT_NOTEBOOKS`,
//! then `Documents/NoteCraft`). Exit status: 0 on success, 1 on an error or a failed check.

use notecraft_engine::{Output, Params, Session, registry};
use notecraft_mcp::{Backend, Headless, Remote, key_dir, serve};
use notecraft_store::Settings;
use notecraft_store::settings::default_notebooks_dir;
use notecraft_store::tree::display_path;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn take_flag_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let i = args.iter().position(|a| a == name)?;
    if i + 1 >= args.len() {
        return None;
    }
    let v = args.remove(i + 1);
    args.remove(i);
    Some(v)
}

/// `key=value` words into parameters. A word without `=` is an error.
fn parse_params(words: &[String]) -> Result<Params, String> {
    let mut p = Params::new();
    for w in words {
        let (k, v) = w.split_once('=').ok_or_else(|| format!("'{w}' is not key=value"))?;
        if k.is_empty() {
            return Err(format!("'{w}' has no key"));
        }
        p.insert(k.to_string(), v.to_string());
    }
    Ok(p)
}

/// Serves the Model Context Protocol on stdin and stdout, headless or against a running window.
fn serve_mcp(root: &Path, connect: Option<String>) -> Result<(), String> {
    let settings = Settings::default_location();
    let mut backend: Box<dyn Backend> = match connect {
        Some(target) => {
            let port: u16 = target.rsplit(':').next().unwrap_or(&target).trim().parse().map_err(|_| format!("'{target}' is not a port"))?;
            Box::new(Remote::connect(port, &key_dir(&settings))?)
        }
        None => Box::new(Headless::open(root, settings)?),
    };
    let input = std::io::stdin();
    let output = std::io::stdout();
    serve(backend.as_mut(), input.lock(), output.lock()).map_err(|e| format!("the connection to the client ended: {e}"))
}

fn describe(o: &Output) -> String {
    match o {
        Output::None => "ok".to_string(),
        Output::Path(p) => display_path(p),
        Output::Text(t) => t.clone(),
        Output::Count(n) => n.to_string(),
        Output::Page(p) => format!("{} page \"{}\" ({} characters)\n{}", p.format.id(), p.title, p.body.chars().count(), p.body),
        Output::Hits(h) => h.iter().map(|x| format!("{}  {}\n    {}", x.title, display_path(&x.path), x.snippet)).collect::<Vec<_>>().join("\n"),
        Output::Report(r) => {
            let mut s = r.problems.iter().map(|p| format!("problem: {}: {}", display_path(&p.path), p.message)).collect::<Vec<_>>().join("\n");
            if !s.is_empty() {
                s.push('\n');
            }
            s.push_str(&format!("checked {} nodes, {} problems", r.nodes, r.problems.len()));
            s
        }
    }
}

fn run(mut args: Vec<String>) -> Result<(String, bool), String> {
    let root = take_flag_value(&mut args, "--notebooks").map(PathBuf::from).unwrap_or_else(default_notebooks_dir);
    let connect = take_flag_value(&mut args, "--connect");
    let Some(cmd) = args.first().cloned() else {
        return Ok((HELP.to_string(), true));
    };
    let rest = args.get(1..).unwrap_or(&[]).to_vec();
    if cmd == "commands" {
        let text = registry()
            .iter()
            .map(|c| format!("{}\n    {}\n{}", c.id, c.label, c.params.lines().map(|l| format!("      {l}")).collect::<Vec<_>>().join("\n")))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok((text, true));
    }
    if cmd == "mcp" {
        serve_mcp(&root, connect)?;
        return Ok((String::new(), true));
    }
    let mut session = Session::new(&root, Settings::default_location()).map_err(|e| e.to_string())?;
    match cmd.as_str() {
        "tree" => {
            let text = describe(&session.run("tree.read", &Params::new()).map_err(|e| e.to_string())?);
            Ok((if text.is_empty() { "no notebooks".to_string() } else { text }, true))
        }
        "sample" => {
            notecraft_engine::sample::create_sample(&mut session, None).map_err(|e| e.to_string())?;
            Ok((format!("sample notebooks created in {}", display_path(&root)), true))
        }
        "run" => {
            let id = rest.first().ok_or("usage: run ID key=value ...")?;
            let p = parse_params(rest.get(1..).unwrap_or(&[]))?;
            let o = session.run(id, &p).map_err(|e| e.to_string())?;
            Ok((describe(&o), true))
        }
        "check" => {
            let path = rest.first().ok_or("usage: check NOTEBOOK")?;
            let o = session.run("notebook.check", &parse_params(&[format!("path={path}")])?).map_err(|e| e.to_string())?;
            let ok = matches!(&o, Output::Report(r) if r.problems.is_empty());
            Ok((describe(&o), ok))
        }
        "search" => {
            let q = rest.join(" ");
            let o = session.run("search.query", &parse_params(&[format!("q={q}")])?).map_err(|e| e.to_string())?;
            Ok((describe(&o), true))
        }
        other => Err(format!("unknown command '{other}'. Try: notecraft-cli commands")),
    }
}

const HELP: &str =
    "notecraft-cli commands | run ID key=value ... | tree | check NOTEBOOK | search TEXT | sample | mcp [--connect PORT]   (add --notebooks FOLDER)";

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok((text, ok)) => {
            if !text.is_empty() {
                println!("{text}");
            }
            if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;

    fn args(root: &TempDir, words: &[&str]) -> Vec<String> {
        let mut a: Vec<String> = words.iter().map(|w| w.to_string()).collect();
        a.push("--notebooks".into());
        a.push(root.path().join("nb").to_string_lossy().into_owned());
        a
    }

    #[test]
    fn params_parse() {
        let p = parse_params(&["a=1".into(), "b=x=y".into()]).expect("params");
        assert_eq!(p.get("b").map(String::as_str), Some("x=y"));
        assert!(parse_params(&["novalue".into()]).is_err());
        assert!(parse_params(&["=v".into()]).is_err());
    }

    #[test]
    fn flag_values_are_taken_out() {
        let mut a = vec!["x".to_string(), "--notebooks".into(), "d".into(), "y".into()];
        assert_eq!(take_flag_value(&mut a, "--notebooks").as_deref(), Some("d"));
        assert_eq!(a, vec!["x".to_string(), "y".into()]);
        let mut b = vec!["--notebooks".to_string()];
        assert_eq!(take_flag_value(&mut b, "--notebooks"), None);
    }

    #[test]
    fn lists_commands_without_opening_a_folder() {
        let (text, ok) = run(vec!["commands".into(), "--notebooks".into(), "/definitely/not/created".into()]).expect("commands");
        assert!(ok && text.contains("notebook.create") && text.contains("node.create"));
        assert!(!std::path::Path::new("/definitely/not/created").exists());
    }

    #[test]
    fn a_session_through_the_command_line() {
        let t = TempDir::new("cli1");
        let (out, ok) = run(args(&t, &["run", "notebook.create", "name=Work"])).expect("create");
        assert!(ok && out.ends_with("work"), "{out}");
        let (tree, _) = run(args(&t, &["tree"])).expect("tree");
        assert!(tree.contains("Work  [notebook, html]"), "{tree}");
        let (out, _) = run(args(&t, &["run", "node.create", "parent=work", "kind=page", "title=Plan", "format=cmgfm"])).expect("page");
        assert!(out.ends_with("plan"), "{out}");
        run(args(&t, &["run", "page.save", "path=work/plan", "body=# Plan\n\nship it"])).expect("save");
        let (hits, _) = run(args(&t, &["search", "ship", "it"])).expect("search");
        assert!(hits.contains("Plan"), "{hits}");
        let (report, clean) = run(args(&t, &["check", "work"])).expect("check");
        assert!(clean && report.contains("0 problems"), "{report}");
    }

    #[test]
    fn the_mcp_server_needs_somewhere_to_point() {
        let t = TempDir::new("cli3");
        let e = run(args(&t, &["mcp", "--connect", "not-a-port"])).err().unwrap_or_default();
        assert!(e.contains("is not a port"), "{e}");
        // A port with no window behind it says so instead of hanging.
        let e = run(args(&t, &["mcp", "--connect", "127.0.0.1:1"])).err().unwrap_or_default();
        assert!(e.contains("key"), "{e}");
    }

    #[test]
    fn errors_are_reported_not_panicked() {
        let t = TempDir::new("cli2");
        assert!(run(args(&t, &["run"])).is_err());
        assert!(run(args(&t, &["run", "no.such"])).is_err());
        assert!(run(args(&t, &["run", "notebook.create"])).is_err());
        assert!(run(args(&t, &["bogus"])).is_err());
        assert!(run(args(&t, &["check", "../outside"])).is_err());
    }
}

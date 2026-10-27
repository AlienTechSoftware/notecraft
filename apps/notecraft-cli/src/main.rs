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
//! notecraft-cli open PATH [--connect PORT]    show a notebook, section or page in the window
//! notecraft-cli import onenote [SOURCE]       import OneNote section files (see `import onenote --help`)
//! notecraft-cli import officeimo SOURCE       import through the optional OfficeIMO bridge
//! ```
//! Add `--notebooks FOLDER` to use another notebooks folder (default: `NOTECRAFT_NOTEBOOKS`,
//! then `Documents/NoteCraft`). Exit status: 0 on success, 1 on an error or a failed check.

use notecraft_engine::{Output, Params, Session, registry};
use notecraft_import::{Event, Existing, Options};
use notecraft_mcp::{Backend, Headless, Remote, key_dir, serve};
use notecraft_store::settings::default_notebooks_dir;
use notecraft_store::tree::display_path;
use notecraft_store::{PageFormat, Settings};
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

/// `PORT` or `HOST:PORT` from `--connect`.
fn parse_port(target: &str) -> Result<u16, String> {
    target.rsplit(':').next().unwrap_or(target).trim().parse().map_err(|_| format!("'{target}' is not a port"))
}

/// Serves the Model Context Protocol on stdin and stdout, headless or against a running window.
fn serve_mcp(root: &Path, connect: Option<String>) -> Result<(), String> {
    let settings = Settings::default_location();
    let mut backend: Box<dyn Backend> = match connect {
        Some(target) => Box::new(Remote::connect(parse_port(&target)?, &key_dir(&settings))?),
        None => Box::new(Headless::open(root, settings)?),
    };
    let input = std::io::stdin();
    let output = std::io::stdout();
    serve(backend.as_mut(), input.lock(), output.lock()).map_err(|e| format!("the connection to the client ended: {e}"))
}

/// The page `open` shows for `path`, through the engine's `open.page`.
fn page_for(root: &Path, settings: Settings, path: &str) -> Result<PathBuf, String> {
    let mut session = Session::new(root, settings).map_err(|e| e.to_string())?;
    match session.run("open.page", &parse_params(&[format!("path={path}")])?).map_err(|e| e.to_string())? {
        Output::Path(page) => Ok(page),
        other => Err(format!("open.page answered {}", describe(&other))),
    }
}

/// `open PATH`: shows a notebook, section group, section or page in NoteCraft. With
/// `--connect PORT` the running window opens it; otherwise a new window starts on it. The path
/// is checked here first, so a typo is reported in the terminal instead of in a window.
fn open_in_window(root: &Path, explicit_root: bool, connect: Option<String>, rest: &[String]) -> Result<(String, bool), String> {
    let path = rest.first().ok_or("usage: open PATH [--connect PORT]")?;
    if let Some(target) = connect {
        let port = parse_port(&target)?;
        let mut window = Remote::connect(port, &key_dir(&Settings::default_location()))?;
        window.call("ui.open", &serde_json::json!({ "path": path }))?;
        return Ok((format!("opened {path} in the window on port {port}"), true));
    }
    let page = page_for(root, Settings::default_location(), path)?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.with_file_name(format!("notecraft{}", std::env::consts::EXE_SUFFIX));
    if !exe.is_file() {
        return Err(format!("the NoteCraft app was not found next to notecraft-cli, at {}", display_path(&exe)));
    }
    let mut window = std::process::Command::new(&exe);
    // The window keeps a notebooks folder given on its command line for next time, so it is
    // only passed on when it was asked for here too.
    if explicit_root {
        window.arg("--notebooks").arg(root);
    }
    window.arg("--open").arg(&page).spawn().map_err(|e| format!("cannot start {}: {e}", display_path(&exe)))?;
    Ok((format!("opening {} in a new window", display_path(&page)), true))
}

const IMPORT_HELP: &str = r"notecraft-cli import onenote [SOURCE] [options]

SOURCE is a folder of OneNote section files (.one), searched all the way down, or one .one file.
Left out on Windows, it is the OneNote backup folder: %LOCALAPPDATA%\Microsoft\OneNote\16.0\Backup.
The first folder under SOURCE becomes a notebook, folders below it section groups, each .one file a
section, each page a folder, sub-pages folders inside their page. Pictures and files go in the
page's assets folder. SOURCE is only read.

  --to FOLDER        write into this notebooks folder (default: --notebooks, then the usual one)
  --format html|cmgfm  page format to write (default: html)
  --all-versions     also import older dated copies of a section, named with their date
  --existing skip|keep|replace  when a section is already there (default: skip)
  --only WORD        only notebooks, groups or sections whose name contains WORD (repeatable)
  --dry-run          show what would be imported and write nothing";

const OFFICEIMO_HELP: &str = r"notecraft-cli import officeimo SOURCE [options]

SOURCE is one .one file or a folder tree. A folder becomes a notebook, nested folders become
section groups, and .one files become sections. OneNote_RecycleBin is skipped.
The bridge is selected by NOTECRAFT_OFFICEIMO_BRIDGE or the platform cache.

  --to FOLDER          write the generated notebook under this folder
  --format html|cmgfm  page format to write (default: html)";

/// `import onenote`: the command line over `notecraft-import`.
fn import_onenote(root: &Path, mut args: Vec<String>) -> Result<(String, bool), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok((IMPORT_HELP.to_string(), true));
    }
    let to = take_flag_value(&mut args, "--to").map(PathBuf::from).unwrap_or_else(|| root.to_path_buf());
    let format = match take_flag_value(&mut args, "--format") {
        Some(f) => PageFormat::parse(&f).map_err(|e| e.to_string())?,
        None => PageFormat::Html,
    };
    let existing = match take_flag_value(&mut args, "--existing").as_deref() {
        None | Some("skip") => Existing::Skip,
        Some("keep") => Existing::Keep,
        Some("replace") => Existing::Replace,
        Some(other) => return Err(format!("--existing takes skip, keep or replace, not '{other}'")),
    };
    let mut only = Vec::new();
    while let Some(w) = take_flag_value(&mut args, "--only") {
        only.push(w);
    }
    let mut take_switch = |name: &str| args.iter().position(|a| a == name).map(|i| args.remove(i)).is_some();
    let (all_versions, dry_run) = (take_switch("--all-versions"), take_switch("--dry-run"));
    if let Some(flag) = args.iter().find(|a| a.starts_with("--")) {
        return Err(format!("unknown option {flag}. Try: notecraft-cli import onenote --help"));
    }
    let source = match args.first() {
        Some(s) => PathBuf::from(s),
        None => notecraft_import::discover::default_source()
            .ok_or("no source given, and the OneNote folder was not found here. Give the folder of .one files")?,
    };
    if args.len() > 1 {
        return Err("only one source can be given".to_string());
    }
    let mut options = Options::new(source, to);
    options.format = format;
    options.existing = existing;
    options.only = only;
    options.all_versions = all_versions;
    options.dry_run = dry_run;
    let mut progress = |e: &Event| match e {
        Event::Found { sections, copies_skipped, ignored } => {
            eprintln!("found {sections} sections ({copies_skipped} older copies left out, {ignored} other OneNote files noticed)")
        }
        Event::Section { index, total, label } => eprintln!("[{index}/{total}] {label}"),
        Event::Done => {}
    };
    let report = notecraft_import::run(&options, &mut progress)?;
    let ok = report.skipped.iter().all(|s| s.reason == "already in the destination" || s.reason == "no pages");
    Ok((report.to_string(), ok))
}

fn import_officeimo(root: &Path, mut args: Vec<String>) -> Result<(String, bool), String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Ok((OFFICEIMO_HELP.to_string(), true));
    }
    let to = take_flag_value(&mut args, "--to").map(PathBuf::from).unwrap_or_else(|| root.to_path_buf());
    let format = match take_flag_value(&mut args, "--format") {
        Some(value) => notecraft_import::officeimo::Format::parse(&value)?,
        None => notecraft_import::officeimo::Format::Html,
    };
    if args.len() != 1 || args[0].starts_with("--") {
        return Err("usage: import officeimo SOURCE [--to FOLDER] [--format html|cmgfm]".to_string());
    }
    let output = notecraft_import::officeimo::run(Path::new(&args[0]), &to, format)?;
    Ok((if output.trim().is_empty() { format!("OfficeIMO import written to {}", display_path(&to)) } else { output }, true))
}

fn describe(o: &Output) -> String {
    match o {
        Output::None => "ok".to_string(),
        Output::Path(p) => display_path(p),
        Output::Text(t) => t.clone(),
        Output::Count(n) => n.to_string(),
        Output::Page(p) => format!("{} page \"{}\" ({} characters)\n{}", p.format.id(), p.title, p.body.chars().count(), p.body),
        Output::Versions(versions) => versions.iter().map(|v| format!("{}  {}  {} bytes", v.id, v.created, v.bytes)).collect::<Vec<_>>().join("\n"),
        Output::Recycled(items) => {
            items.iter().map(|item| format!("{}  {}  {} item(s)  {}", item.id, item.title, item.count, item.deleted)).collect::<Vec<_>>().join("\n")
        }
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
    let explicit_root = take_flag_value(&mut args, "--notebooks").map(PathBuf::from);
    let connect = take_flag_value(&mut args, "--connect");
    let Some(cmd) = args.first().cloned() else {
        return Ok((HELP.to_string(), true));
    };
    let rest = args.get(1..).unwrap_or(&[]).to_vec();
    if cmd == "open" {
        // The folder the window would use: the one it remembers, unless one is given here.
        let remembered = Settings::default_location().get("last_notebooks_root").map(PathBuf::from);
        let root = explicit_root.clone().or(remembered).unwrap_or_else(default_notebooks_dir);
        return open_in_window(&root, explicit_root.is_some(), connect, &rest);
    }
    let root = explicit_root.unwrap_or_else(default_notebooks_dir);
    if cmd == "commands" {
        let text = registry()
            .iter()
            .map(|c| format!("{}\n    {}\n{}", c.id, c.label, c.params.lines().map(|l| format!("      {l}")).collect::<Vec<_>>().join("\n")))
            .collect::<Vec<_>>()
            .join("\n\n");
        return Ok((text, true));
    }
    if cmd == "import" {
        return match rest.first().map(String::as_str) {
            Some("onenote") => import_onenote(&root, rest[1..].to_vec()),
            Some("officeimo") => import_officeimo(&root, rest[1..].to_vec()),
            _ => Err("usage: import onenote [SOURCE] or import officeimo SOURCE. Try --help".to_string()),
        };
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

const HELP: &str = "notecraft-cli commands | run ID key=value ... | tree | check NOTEBOOK | search TEXT | sample | open PATH [--connect PORT] | import onenote [SOURCE] | import officeimo SOURCE | mcp [--connect PORT]   (add --notebooks FOLDER)";

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("notecraft-cli {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
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
    fn open_checks_the_path_before_starting_a_window() {
        let t = TempDir::new("cli-open");
        run(args(&t, &["run", "notebook.create", "name=Work"])).expect("create");
        assert!(run(args(&t, &["open"])).err().unwrap_or_default().contains("usage"));
        // A path that is not there is reported here, and no window starts.
        let e = run(args(&t, &["open", "work/no-such-page"])).err().unwrap_or_default();
        assert!(e.contains("no-such-page"), "{e}");
        // A good path resolves to its first page. Checked below `open` itself: a test never
        // starts a window, which would use the real profile and wait on its first-run dialog.
        let settings = Settings::at(t.path().join("settings.toml"));
        let page = page_for(&t.path().join("nb"), settings, "work").unwrap_or_else(|e| panic!("{e}"));
        assert!(page.ends_with(Path::new("work").join("new-section-1").join("untitled-page")), "{}", page.display());
        // With --connect, a port with no window behind it says so.
        let e = run(args(&t, &["open", "work", "--connect", "127.0.0.1:1"])).err().unwrap_or_default();
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

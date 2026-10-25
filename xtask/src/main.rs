#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Repository tasks, run as `cargo xtask <task>`.
//!
//! ```text
//! ci        fmt check, clippy -D warnings, tests, layers, assets, docs, text
//! fmt       cargo fmt --check
//! clippy    cargo clippy --workspace --all-targets -- -D warnings
//! test      cargo test --workspace
//! layers    check that crates only depend on lower layers, and that nothing below the UI uses egui
//! assets    check that every file under assets/ has a row in ATTRIBUTION.md
//! docs      check that relative links in the Markdown files point at files that exist
//! text      check text files: no byte order mark in source, manifests or Markdown, and plain
//!           punctuation (no em or en dashes, no curly quotes) in Markdown, per .agents/TONE.md;
//!           add `--fix` to rewrite those issues
//! screenshots  regenerate docs/images from the real window with the headless runner
//! coverage  line coverage with cargo-llvm-cov, if it is installed
//! version   print the workspace version; `version set X.Y.Z[-pre]` changes it
//! demo-copy [html|cmgfm] [--replace]  copy one bundled demo format; --replace approves overwriting a changed scratch copy
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

mod version;

/// The layer of each workspace crate. A crate may depend only on crates in lower layers.
const LAYERS: &[(&str, u8)] = &[
    ("notecraft-model", 0),
    ("notecraft-math", 1),
    ("notecraft-diagram", 1),
    ("notecraft-ink", 1),
    ("notecraft-input-language", 1),
    ("notecraft-onenote", 1),
    ("notecraft-formats", 1),
    ("notecraft-text", 1),
    ("notecraft-store", 2),
    ("notecraft-import", 3),
    ("notecraft-engine", 4),
    ("notecraft-mcp", 5),
    ("notecraft-ui-egui", 6),
    ("notecraft", 7),
    ("notecraft-cli", 7),
    ("xtask", 7),
    ("notecraft-demo", 7),
];

/// Crates that nothing below the UI layer (6) may depend on.
const UI_ONLY: &[&str] = &["egui", "eframe", "egui_extras", "egui_kittest", "winit", "rfd"];

fn root() -> PathBuf {
    // xtask lives at <root>/xtask, and cargo sets this to its directory.
    let here = std::env::var("CARGO_MANIFEST_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("xtask"));
    here.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn cargo(root: &Path, args: &[&str]) -> Result<(), String> {
    println!("$ cargo {}", args.join(" "));
    let status = Command::new("cargo").args(args).current_dir(root).status().map_err(|e| format!("cannot run cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.first().copied().unwrap_or(""))) }
}

/// The dependency names listed in the `[dependencies]` table of a Cargo.toml (not dev or build).
fn dependencies(toml: &str) -> Vec<String> {
    let toml = toml.trim_start_matches('\u{feff}');
    let mut in_deps = false;
    let mut out = Vec::new();
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_deps = t == "[dependencies]";
            continue;
        }
        if in_deps
            && !t.is_empty()
            && !t.starts_with('#')
            && let Some((name, _)) = t.split_once('=')
        {
            out.push(name.trim().to_string());
        }
    }
    out
}

fn package_name(toml: &str) -> Option<String> {
    let toml = toml.trim_start_matches('\u{feff}');
    let mut in_pkg = false;
    for line in toml.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_pkg = t == "[package]";
            continue;
        }
        if in_pkg && let Some(v) = t.strip_prefix("name") {
            return v.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn layer_of(name: &str) -> Option<u8> {
    LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// Problems with the dependencies of one crate, given its manifest text.
fn layer_problems(toml: &str) -> Vec<String> {
    let Some(name) = package_name(toml) else {
        return vec!["a Cargo.toml has no [package] name".into()];
    };
    let Some(mine) = layer_of(&name) else {
        return vec![format!("{name}: not listed in xtask LAYERS (add it)")];
    };
    let mut problems = Vec::new();
    for dep in dependencies(toml) {
        if let Some(theirs) = layer_of(&dep) {
            if theirs >= mine {
                problems.push(format!("{name} (layer {mine}) depends on {dep} (layer {theirs}); dependencies must point to lower layers"));
            }
        } else if mine < 6 && UI_ONLY.contains(&dep.as_str()) {
            problems.push(format!("{name} (layer {mine}) depends on {dep}; only the UI layer and apps may use it"));
        }
    }
    problems
}

fn layers(root: &Path) -> Result<(), String> {
    let mut problems = Vec::new();
    let mut manifests: Vec<PathBuf> = Vec::new();
    for dir in ["crates", "apps"] {
        if let Ok(rd) = fs::read_dir(root.join(dir)) {
            manifests.extend(rd.filter_map(|e| e.ok()).map(|e| e.path().join("Cargo.toml")).filter(|p| p.is_file()));
        }
    }
    manifests.push(root.join("xtask").join("Cargo.toml"));
    manifests.push(root.join("docs").join("demo").join("scripts").join("Cargo.toml"));
    for m in &manifests {
        let text = fs::read_to_string(m).map_err(|e| format!("{}: {e}", m.display()))?;
        problems.extend(layer_problems(&text));
    }
    println!("layers: checked {} crates", manifests.len());
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

fn files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                files_under(&p, out);
            } else {
                out.push(p);
            }
        }
    }
}

/// Relative paths (with `/`) of asset files that ATTRIBUTION.md does not mention in backticks.
fn unattributed(attribution: &str, assets: &[String]) -> Vec<String> {
    assets.iter().filter(|a| !attribution.contains(&format!("`{a}`"))).cloned().collect()
}

fn assets(root: &Path) -> Result<(), String> {
    let attribution = fs::read_to_string(root.join("ATTRIBUTION.md")).map_err(|e| format!("ATTRIBUTION.md: {e}"))?;
    let mut files = Vec::new();
    files_under(&root.join("assets"), &mut files);
    files_under(&root.join("docs").join("images"), &mut files);
    let rel: Vec<String> = files
        .iter()
        .filter_map(|f| f.strip_prefix(root).ok())
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .filter(|p| !p.ends_with(".gitkeep"))
        .collect();
    let missing = unattributed(&attribution, &rel);
    println!("assets: {} files, {} without an ATTRIBUTION.md row", rel.len(), missing.len());
    if missing.is_empty() { Ok(()) } else { Err(format!("add a row to ATTRIBUTION.md for:\n  {}", missing.join("\n  "))) }
}

/// Relative link targets in Markdown text: `[text](target)` where the target is not a web
/// address, an anchor, or a mail link. The `#anchor` part is dropped.
fn relative_links(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut rest = line;
        while let Some(i) = rest.find("](") {
            let after = &rest[i + 2..];
            let Some(end) = after.find(')') else { break };
            let target = after[..end].trim();
            rest = &after[end..];
            let target = target.split_whitespace().next().unwrap_or("");
            if target.is_empty() || target.starts_with('#') || target.contains("://") || target.starts_with("mailto:") {
                continue;
            }
            out.push(target.split('#').next().unwrap_or("").to_string());
        }
    }
    out.retain(|t| !t.is_empty());
    out
}

fn docs(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    files_under(&root.join("docs"), &mut files);
    // The Welcome notebook's content files are laid out by `cargo xtask demo`; their links
    // are relative to the pages they become, and the generator checks every one against the
    // built notebook. The built notebook under docs/demo/DemoNotebook is checked here.
    let content = root.join("docs").join("demo").join("scripts").join("content");
    files.retain(|f| !f.starts_with(&content));
    for name in ["README.md", "AGENTS.md", "CLAUDE.md", "ROADMAP.md", "ATTRIBUTION.md"] {
        files.push(root.join(name));
    }
    let mut problems = BTreeMap::new();
    let mut checked = 0;
    for f in files.iter().filter(|f| f.extension().is_some_and(|e| e == "md") && f.is_file()) {
        let text = fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?;
        let dir = f.parent().unwrap_or(root);
        for link in relative_links(&text) {
            checked += 1;
            if !dir.join(&link).exists() {
                problems.insert(format!("{} -> {link}", f.strip_prefix(root).unwrap_or(f).display()), ());
            }
        }
    }
    println!("docs: {checked} relative links checked, {} broken", problems.len());
    if problems.is_empty() { Ok(()) } else { Err(format!("broken links:\n  {}", problems.keys().cloned().collect::<Vec<_>>().join("\n  "))) }
}

/// Problems in one text file, given its name and bytes.
fn text_problems(name: &str, bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        out.push(format!("{name}: starts with a byte order mark (save as UTF-8 without BOM)"));
    }
    if name.ends_with(".md") {
        let text = String::from_utf8_lossy(bytes);
        for (c, what) in [
            ('\u{2014}', "an em dash"),
            ('\u{2013}', "an en dash"),
            ('\u{2018}', "a curly quote"),
            ('\u{2019}', "a curly quote"),
            ('\u{201c}', "a curly quote"),
            ('\u{201d}', "a curly quote"),
        ] {
            if text.contains(c) {
                out.push(format!("{name}: contains {what}; use plain ASCII punctuation"));
            }
        }
    }
    out
}

fn fixed_text(name: &str, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut text = String::from_utf8(bytes.to_vec()).map_err(|_| format!("{name}: invalid UTF-8; cannot fix automatically"))?;
    if text.starts_with('\u{FEFF}') {
        text = text.trim_start_matches('\u{FEFF}').to_string();
    }
    if name.ends_with(".md") {
        text = text.replace(['\u{2014}', '\u{2013}'], "-").replace(['\u{2018}', '\u{2019}'], "'").replace(['\u{201C}', '\u{201D}'], "\"");
    }
    Ok(text.into_bytes())
}

fn text(root: &Path, fix: bool) -> Result<(), String> {
    let mut files = Vec::new();
    files_under(root, &mut files);
    let skip = ["target", ".git", "out", "prototypes", "snapshots", "my-notebooks"];
    let mut problems = Vec::new();
    let mut n = 0;
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(&f);
        if rel.components().any(|c| {
            let component = c.as_os_str().to_string_lossy();
            skip.contains(&component.as_ref()) || component.starts_with(".work-officeimo-")
        }) {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        if !(name.ends_with(".rs") || name.ends_with(".toml") || name.ends_with(".md")) {
            continue;
        }
        n += 1;
        let bytes = fs::read(&f).map_err(|e| format!("{name}: {e}"))?;
        if fix {
            let updated = fixed_text(&name, &bytes)?;
            if updated != bytes {
                fs::write(&f, updated).map_err(|e| format!("{name}: {e}"))?;
            }
            continue;
        }
        problems.extend(text_problems(&name, &bytes));
    }
    println!("text: {n} files checked, {} problems", problems.len());
    if problems.is_empty() { Ok(()) } else { Err(problems.join("\n")) }
}

/// Regenerates `docs/images` by running scripts/readme-screens.jsonl through the headless
/// runner (`crates/ui-egui/examples/ui_shot.rs`).
fn screenshots(root: &Path) -> Result<(), String> {
    fs::create_dir_all(root.join("docs").join("images")).map_err(|e| e.to_string())?;
    cargo(root, &["run", "-q", "-p", "notecraft-ui-egui", "--example", "ui_shot", "--", "scripts/readme-screens.jsonl"])
}

/// Copies one checked-in demo format to the ignored test notebooks folder.
/// The source is read-only; `--replace` is the explicit approval to replace a changed copy.
fn demo_copy(root: &Path, args: &[String]) -> Result<(), String> {
    let mut format: Option<&str> = None;
    let mut replace = false;
    for arg in args {
        match arg.as_str() {
            "html" | "cmgfm" if format.is_none() => format = Some(arg),
            "--replace" if !replace => replace = true,
            _ => return Err(format!("unknown or repeated demo-copy argument '{arg}'. Usage: cargo xtask demo-copy [html|cmgfm] [--replace]")),
        }
    }
    let format = format.unwrap_or("html");
    let source = root.join("docs").join("demo").join("DemoNotebook").join(format);
    if !source.is_dir() {
        return Err(format!("demo source folder is missing: {} (run `cargo xtask demo` first)", source.display()));
    }

    let notebooks = root.join("my-notebooks");
    match fs::symlink_metadata(&notebooks) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(format!("refusing to copy through a symlink: {}", notebooks.display())),
        Ok(meta) if !meta.is_dir() => return Err(format!("not a folder: {}", notebooks.display())),
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(&notebooks).map_err(|e| format!("create {}: {e}", notebooks.display()))?
        }
        Err(e) => return Err(format!("inspect {}: {e}", notebooks.display())),
    }

    let target = notebooks.join("welcome-notebook");
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.file_type().is_symlink() => return Err(format!("refusing to replace a symlink: {}", target.display())),
        Ok(meta) if !meta.is_dir() => return Err(format!("not a folder: {}", target.display())),
        Ok(_) if same_directory(&source, &target)? => {
            println!("{} is already current; scratch copy unchanged", target.display());
            return Ok(());
        }
        Ok(_) if !replace => {
            return Err(format!(
                "scratch demo differs from the official demo at {}; review it, then approve replacement with `cargo xtask demo-copy {format} --replace`",
                target.display()
            ));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("inspect {}: {e}", target.display())),
    }

    let staging = notebooks.join(format!(".demo-copy-{}", std::process::id()));
    if staging.exists() {
        return Err(format!("temporary copy folder already exists: {}", staging.display()));
    }
    fs::create_dir(&staging).map_err(|e| format!("create {}: {e}", staging.display()))?;
    let staged_notebook = staging.join("welcome-notebook");
    if let Err(e) = copy_directory(&source, &staged_notebook) {
        return Err(match fs::remove_dir_all(&staging) {
            Ok(()) => e,
            Err(cleanup) => format!("{e}; partial copy remains at {} because cleanup failed: {cleanup}", staging.display()),
        });
    }

    let backup = staging.join("previous-copy");
    if target.exists()
        && let Err(e) = fs::rename(&target, &backup)
    {
        return Err(match fs::remove_dir_all(&staging) {
            Ok(()) => format!("move existing test copy to staging: {e}"),
            Err(cleanup) => format!(
                "move existing test copy to staging: {e}; temporary folder remains at {} because cleanup failed: {cleanup}",
                staging.display()
            ),
        });
    }
    if let Err(e) = fs::rename(&staged_notebook, &target) {
        if backup.exists()
            && let Err(restore) = fs::rename(&backup, &target)
        {
            return Err(format!(
                "install new test copy failed ({e}); restoring the previous copy also failed ({restore}); it remains at {}",
                backup.display()
            ));
        }
        return Err(match fs::remove_dir_all(&staging) {
            Ok(()) => format!("install new test copy: {e}"),
            Err(cleanup) => {
                format!("install new test copy: {e}; temporary folder remains at {} because cleanup failed: {cleanup}", staging.display())
            }
        });
    }
    fs::remove_dir_all(&staging)
        .map_err(|e| format!("copy is ready at {}, but temporary cleanup failed at {}: {e}", target.display(), staging.display()))?;
    println!("copied {} to {} (source unchanged)", source.display(), target.display());
    Ok(())
}

/// Compares regular directory trees without following symlinks.
fn same_directory(left: &Path, right: &Path) -> Result<bool, String> {
    let mut left_files = Vec::new();
    let mut right_files = Vec::new();
    files_under(left, &mut left_files);
    files_under(right, &mut right_files);
    let relative = |root: &Path, files: &mut Vec<PathBuf>| -> Result<(), String> {
        for file in files.iter_mut() {
            *file = file.strip_prefix(root).map_err(|e| format!("compare {}: {e}", file.display()))?.to_path_buf();
        }
        files.sort();
        Ok(())
    };
    relative(left, &mut left_files)?;
    relative(right, &mut right_files)?;
    if left_files != right_files {
        return Ok(false);
    }
    for rel in left_files {
        let a = fs::read(left.join(&rel)).map_err(|e| format!("read {}: {e}", left.join(&rel).display()))?;
        let b = fs::read(right.join(&rel)).map_err(|e| format!("read {}: {e}", right.join(&rel).display()))?;
        if a != b {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Copies a directory tree without following or reproducing symlinks.
fn copy_directory(source: &Path, target: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(source).map_err(|e| format!("inspect {}: {e}", source.display()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(format!("not a regular directory: {}", source.display()));
    }
    fs::create_dir(target).map_err(|e| format!("create {}: {e}", target.display()))?;
    let entries = fs::read_dir(source).map_err(|e| format!("read {}: {e}", source.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("read an entry in {}: {e}", source.display()))?;
        let from = entry.path();
        let to = target.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| format!("inspect {}: {e}", from.display()))?;
        if kind.is_dir() {
            copy_directory(&from, &to)?;
        } else if kind.is_file() {
            fs::copy(&from, &to).map_err(|e| format!("copy {} to {}: {e}", from.display(), to.display()))?;
        } else {
            return Err(format!("refusing to copy a special file: {}", from.display()));
        }
    }
    Ok(())
}

fn coverage(root: &Path) -> Result<(), String> {
    let have = Command::new("cargo").args(["llvm-cov", "--version"]).current_dir(root).output().map(|o| o.status.success()).unwrap_or(false);
    if !have {
        return Err(
            "cargo-llvm-cov is not installed. Install it with: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov".into()
        );
    }
    cargo(root, &["llvm-cov", "--workspace", "--summary-only"])
}

fn run(task: &str, options: &[String]) -> Result<(), String> {
    let root = root();
    match task {
        "fmt" => cargo(&root, &["fmt", "--all", "--", "--check"]),
        "clippy" => cargo(&root, &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
        // The UI tests share the desktop clipboard and drive one window each, so they run one at a time;
        // the other crates run in parallel as before.
        "test" => {
            cargo(&root, &["test", "--workspace", "--exclude", "notecraft-ui-egui"])?;
            cargo(&root, &["test", "-p", "notecraft-ui-egui", "--", "--test-threads=1"])
        }
        "layers" => layers(&root),
        "assets" => assets(&root),
        "docs" => docs(&root),
        "text" => {
            let fix = options == ["--fix"];
            if !fix && !options.is_empty() {
                return Err("usage: cargo xtask text [--fix]".into());
            }
            text(&root, fix)
        }
        "screenshots" => screenshots(&root),
        "coverage" => coverage(&root),
        // The Welcome notebook (docs/demo/DemoNotebook) from its content, and the check that
        // the checked-in copy is current.
        "demo" => {
            cargo(&root, &["run", "--quiet", "-p", "notecraft-demo", "--", "build"])?;
            // The scratch copy is always replaced from docs/demo: the source is the one truth.
            demo_copy(&root, &["html".into(), "--replace".into()])
        }
        "demo-check" => cargo(&root, &["run", "--quiet", "-p", "notecraft-demo", "--", "check"]),
        "demo-copy" => Err("demo-copy accepts options; use `cargo xtask demo-copy [html|cmgfm] [--replace]`".into()),
        "ci" => {
            for t in ["fmt", "clippy", "test", "layers", "assets", "docs", "text", "demo-check"] {
                println!("\n== {t} ==");
                run(t, &[]).map_err(|e| format!("{t}: {e}"))?;
            }
            println!("\nci: all checks passed");
            Ok(())
        }
        other => Err(format!(
            "unknown task '{other}'. Tasks: ci fmt clippy test layers assets docs text screenshots coverage version demo demo-check demo-copy"
        )),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = args.first().cloned().unwrap_or_else(|| "ci".into());
    let result = match task.as_str() {
        "version" => version::run(&root(), &args[1..]),
        "demo-copy" => demo_copy(&root(), &args[1..]),
        _ => run(&task, &args[1..]),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODEL: &str = "[package]\nname = \"notecraft-model\"\n\n[dependencies]\n";
    const GOOD_STORE: &str = "[package]\nname = \"notecraft-store\"\n[dependencies]\nnotecraft-model = { workspace = true }\nnotecraft-formats = { workspace = true }\n[dev-dependencies]\negui_kittest = { workspace = true }\n";

    #[test]
    fn reads_package_and_dependencies() {
        assert_eq!(package_name(MODEL).as_deref(), Some("notecraft-model"));
        assert_eq!(dependencies(GOOD_STORE), vec!["notecraft-model", "notecraft-formats"], "dev-dependencies are not layer dependencies");
    }

    #[test]
    fn an_upward_dependency_is_a_problem() {
        let bad = "[package]\nname = \"notecraft-model\"\n[dependencies]\nnotecraft-store = { workspace = true }\n";
        assert_eq!(layer_problems(bad).len(), 1);
        let same = "[package]\nname = \"notecraft-store\"\n[dependencies]\nnotecraft-store = { workspace = true }\n";
        assert_eq!(layer_problems(same).len(), 1, "a crate may not depend on its own layer");
        assert!(layer_problems(GOOD_STORE).is_empty());
    }

    #[test]
    fn egui_below_the_ui_layer_is_a_problem() {
        let bad = "[package]\nname = \"notecraft-engine\"\n[dependencies]\negui = { workspace = true }\n";
        assert_eq!(layer_problems(bad).len(), 1);
        let ok = "[package]\nname = \"notecraft-ui-egui\"\n[dependencies]\negui = { workspace = true }\nnotecraft-engine = { workspace = true }\n";
        assert!(layer_problems(ok).is_empty());
    }

    #[test]
    fn an_unlisted_crate_is_reported() {
        assert_eq!(layer_problems("[package]\nname = \"mystery\"\n").len(), 1);
        assert_eq!(layer_problems("no package here").len(), 1);
    }

    #[test]
    fn attribution_rows_are_matched_by_path() {
        let a = "| `assets/brand/a.png` | x |\n";
        assert_eq!(unattributed(a, &["assets/brand/a.png".into(), "assets/brand/b.png".into()]), vec!["assets/brand/b.png"]);
    }

    #[test]
    fn finds_relative_links_only() {
        let md = "See [a](docs/a.md), [b](https://x.test), [c](#top), [d](../e.md#sec \"title\") and ![i](img/x.png).\n```\n[no](not/a/link.md)\n```\n[mail](mailto:a@b.c)\n";
        assert_eq!(relative_links(md), vec!["docs/a.md", "../e.md", "img/x.png"]);
    }

    #[test]
    fn text_rules() {
        assert!(text_problems("a.rs", b"fn main() {}").is_empty());
        assert_eq!(text_problems("a.toml", &[0xEF, 0xBB, 0xBF, b'x']).len(), 1);
        assert_eq!(text_problems("a.md", "a \u{2014} b".as_bytes()).len(), 1);
        assert_eq!(text_problems("a.md", "\u{201c}quoted\u{201d}".as_bytes()).len(), 2);
        assert!(text_problems("a.rs", "// \u{2014} allowed in source".as_bytes()).is_empty(), "only Markdown is held to plain punctuation");
    }

    #[test]
    fn text_fix_rewrites_bom_and_markdown_punctuation() {
        let fixed = fixed_text("a.md", "\u{FEFF}a \u{2014} \u{2018}quote\u{2019}".as_bytes()).expect("valid UTF-8");
        assert_eq!(fixed, b"a - 'quote'");
        let source = fixed_text("a.rs", "// \u{2014}".as_bytes()).expect("valid UTF-8");
        assert_eq!(source, "// \u{2014}".as_bytes());
    }

    #[test]
    fn the_layer_table_covers_every_crate_of_this_repo() {
        let root = root();
        let mut manifests = Vec::new();
        for dir in ["crates", "apps"] {
            if let Ok(rd) = fs::read_dir(root.join(dir)) {
                manifests.extend(rd.filter_map(|e| e.ok()).map(|e| e.path().join("Cargo.toml")).filter(|p| p.is_file()));
            }
        }
        assert!(manifests.len() >= 8, "found {} crates", manifests.len());
        for m in manifests {
            let text = fs::read_to_string(&m).expect("manifest");
            let name = package_name(&text).expect("name");
            assert!(layer_of(&name).is_some(), "{name} is missing from LAYERS");
        }
    }

    #[test]
    fn this_repository_passes_its_own_layer_check() {
        assert_eq!(layers(&root()), Ok(()));
    }
}

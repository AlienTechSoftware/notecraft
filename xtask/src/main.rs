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
//!           punctuation (no em or en dashes, no curly quotes) in Markdown, per .agents/TONE.md
//! screenshots  regenerate docs/images from the real window with the headless runner
//! coverage  line coverage with cargo-llvm-cov, if it is installed
//! version   print the workspace version; `version set X.Y.Z[-pre]` changes it
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
    ("notecraft-ink", 1),
    ("notecraft-onenote", 1),
    ("notecraft-formats", 1),
    ("notecraft-text", 1),
    ("notecraft-store", 2),
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

fn text(root: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    files_under(root, &mut files);
    let skip = ["target", ".git", "out", "prototypes", "snapshots"];
    let mut problems = Vec::new();
    let mut n = 0;
    for f in files {
        let rel = f.strip_prefix(root).unwrap_or(&f);
        if rel.components().any(|c| skip.contains(&c.as_os_str().to_string_lossy().as_ref())) {
            continue;
        }
        let name = rel.to_string_lossy().replace('\\', "/");
        if !(name.ends_with(".rs") || name.ends_with(".toml") || name.ends_with(".md")) {
            continue;
        }
        n += 1;
        let bytes = fs::read(&f).map_err(|e| format!("{name}: {e}"))?;
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

fn coverage(root: &Path) -> Result<(), String> {
    let have = Command::new("cargo").args(["llvm-cov", "--version"]).current_dir(root).output().map(|o| o.status.success()).unwrap_or(false);
    if !have {
        return Err(
            "cargo-llvm-cov is not installed. Install it with: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov".into()
        );
    }
    cargo(root, &["llvm-cov", "--workspace", "--summary-only"])
}

fn run(task: &str) -> Result<(), String> {
    let root = root();
    match task {
        "fmt" => cargo(&root, &["fmt", "--all", "--", "--check"]),
        "clippy" => cargo(&root, &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
        "test" => cargo(&root, &["test", "--workspace"]),
        "layers" => layers(&root),
        "assets" => assets(&root),
        "docs" => docs(&root),
        "text" => text(&root),
        "screenshots" => screenshots(&root),
        "coverage" => coverage(&root),
        // The Welcome notebook (docs/demo/DemoNotebook) from its content, and the check that
        // the checked-in copy is current.
        "demo" => cargo(&root, &["run", "--quiet", "-p", "notecraft-demo", "--", "build"]),
        "demo-check" => cargo(&root, &["run", "--quiet", "-p", "notecraft-demo", "--", "check"]),
        "ci" => {
            for t in ["fmt", "clippy", "test", "layers", "assets", "docs", "text", "demo-check"] {
                println!("\n== {t} ==");
                run(t).map_err(|e| format!("{t}: {e}"))?;
            }
            println!("\nci: all checks passed");
            Ok(())
        }
        other => {
            Err(format!("unknown task '{other}'. Tasks: ci fmt clippy test layers assets docs text screenshots coverage version demo demo-check"))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let task = args.first().cloned().unwrap_or_else(|| "ci".into());
    let result = if task == "version" { version::run(&root(), &args[1..]) } else { run(&task) };
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

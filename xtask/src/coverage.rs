//! `cargo xtask coverage [--update]`: line, function and region coverage with cargo-llvm-cov,
//! compared with the baseline in `docs/coverage.json`.
//!
//! A drop is a warning, never a failure: it is printed, shown as an annotation on GitHub and
//! written to the job summary, and the task still succeeds. `--update` writes the numbers of
//! this run as the new baseline, so a change that raises coverage carries the new figure in
//! the same pull request and git keeps the history.

use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the baseline lives, from the repository root.
pub const BASELINE: &str = "docs/coverage.json";

/// How many percentage points a figure may fall before it is reported. Smaller moves are
/// noise from tests that take a different path from run to run.
const SLACK: f64 = 0.5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Share {
    pub covered: u64,
    pub count: u64,
}

impl Share {
    fn percent(self) -> f64 {
        if self.count == 0 { 100.0 } else { one_decimal(100.0 * self.covered as f64 / self.count as f64) }
    }
}

#[derive(Debug, PartialEq)]
pub struct Summary {
    pub lines: Share,
    pub functions: Share,
    pub regions: Share,
    /// Lines per crate, keyed `crates/engine` or `apps/notecraft`.
    pub crates: BTreeMap<String, Share>,
}

fn one_decimal(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

fn share(v: &Value) -> Share {
    Share { covered: v["covered"].as_u64().unwrap_or(0), count: v["count"].as_u64().unwrap_or(0) }
}

/// `crates/engine` for `.../crates/engine/src/session.rs`. Files outside `crates/` and `apps/`
/// count towards the total only.
fn crate_of(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    for top in ["crates", "apps"] {
        let marker = format!("/{top}/");
        if let Some(i) = path.rfind(&marker) {
            let name = path[i + marker.len()..].split('/').next().filter(|n| !n.is_empty())?;
            return Some(format!("{top}/{name}"));
        }
    }
    None
}

/// Reads the JSON summary cargo-llvm-cov writes (LLVM's export format).
pub fn summarize(report: &Value) -> Result<Summary, String> {
    let data = report["data"].get(0).ok_or("the coverage report has no data")?;
    let totals = &data["totals"];
    let mut crates: BTreeMap<String, Share> = BTreeMap::new();
    for file in data["files"].as_array().into_iter().flatten() {
        let Some(key) = file["filename"].as_str().and_then(crate_of) else { continue };
        let s = share(&file["summary"]["lines"]);
        let entry = crates.entry(key).or_default();
        entry.covered += s.covered;
        entry.count += s.count;
    }
    Ok(Summary { lines: share(&totals["lines"]), functions: share(&totals["functions"]), regions: share(&totals["regions"]), crates })
}

/// The baseline file's contents for this summary.
pub fn baseline_json(s: &Summary, measured: Value) -> Value {
    let crates: Map<String, Value> = s.crates.iter().map(|(k, v)| (k.clone(), json!({"lines": v.percent(), "count": v.count}))).collect();
    json!({
        "about": "Test coverage of the workspace, from `cargo xtask coverage --update`. CI warns on a pull request that lowers it by more than 0.5 points; it does not block. See SKILLS/nc-devx/SKILL.md.",
        "measured": measured,
        "total": {"lines": s.lines.percent(), "functions": s.functions.percent(), "regions": s.regions.percent()},
        "crates": crates,
    })
}

/// One sentence per figure that fell by more than `SLACK` since the baseline.
pub fn drops(baseline: &Value, now: &Summary) -> Vec<String> {
    let mut out = Vec::new();
    let mut check = |what: String, before: Option<f64>, after: f64| {
        if let Some(before) = before
            && before - after > SLACK
        {
            out.push(format!("{what} coverage fell from {before:.1}% to {after:.1}%"));
        }
    };
    for (name, s) in [("line", now.lines), ("function", now.functions), ("region", now.regions)] {
        let key = format!("{name}s");
        check(format!("total {name}"), baseline["total"][&key].as_f64(), s.percent());
    }
    for (krate, s) in &now.crates {
        check(format!("{krate} line"), baseline["crates"][krate]["lines"].as_f64(), s.percent());
    }
    out
}

fn table(s: &Summary) -> String {
    let mut t = String::from("| Crate | Lines | Covered |\n|---|---:|---:|\n");
    let mut rows: Vec<_> = s.crates.iter().collect();
    rows.sort_by_key(|(_, s)| std::cmp::Reverse(s.count));
    for (k, v) in rows {
        t.push_str(&format!("| {k} | {} | {:.1}% |\n", v.count, v.percent()));
    }
    t.push_str(&format!(
        "| **total** | {} | **{:.1}%** (functions {:.1}%, regions {:.1}%) |\n",
        s.lines.count,
        s.lines.percent(),
        s.functions.percent(),
        s.regions.percent()
    ));
    t
}

/// The first line a command prints, or an empty string.
fn first_line(program: &str, args: &[&str], root: &Path) -> String {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(str::to_string))
        .unwrap_or_default()
}

fn target_dir(root: &Path) -> PathBuf {
    std::env::var("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|_| root.join("target"))
}

pub fn run(root: &Path, options: &[String]) -> Result<(), String> {
    let update = match options {
        [] => false,
        [o] if o == "--update" => true,
        _ => return Err("usage: cargo xtask coverage [--update]".into()),
    };
    let tool = first_line("cargo", &["llvm-cov", "--version"], root);
    if tool.is_empty() {
        return Err(
            "cargo-llvm-cov is not installed. Install it with: rustup component add llvm-tools && cargo install cargo-llvm-cov --locked".into()
        );
    }
    let report_path = target_dir(root).join("coverage-summary.json");
    if let Some(dir) = report_path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    }
    let report_arg = report_path.to_string_lossy().into_owned();
    crate::cargo(root, &["llvm-cov", "--workspace", "--exclude", "xtask", "--json", "--summary-only", "--output-path", &report_arg])?;
    let text = fs::read_to_string(&report_path).map_err(|e| format!("read {}: {e}", report_path.display()))?;
    let report: Value = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", report_path.display()))?;
    let now = summarize(&report)?;
    let shown = table(&now);
    println!("\n{shown}");

    let baseline_path = root.join(BASELINE);
    let mut notes = Vec::new();
    if update {
        let measured = json!({
            "commit": first_line("git", &["rev-parse", "--short", "HEAD"], root),
            "date": first_line("git", &["log", "-1", "--format=%cs"], root),
            "platform": std::env::consts::OS,
            "rust": first_line("rustc", &["--version"], root),
            "tool": tool,
        });
        let body = serde_json::to_string_pretty(&baseline_json(&now, measured)).map_err(|e| e.to_string())?;
        fs::write(&baseline_path, format!("{body}\n")).map_err(|e| format!("write {}: {e}", baseline_path.display()))?;
        println!("coverage: wrote {BASELINE}");
    } else if let Ok(saved) = fs::read_to_string(&baseline_path) {
        let baseline: Value = serde_json::from_str(&saved).map_err(|e| format!("{BASELINE}: {e}"))?;
        let platform = baseline["measured"]["platform"].as_str().unwrap_or_default();
        if !platform.is_empty() && platform != std::env::consts::OS {
            notes.push(format!(
                "the baseline was measured on {platform} and this run is on {}; code for one platform only moves the figures",
                std::env::consts::OS
            ));
        }
        let fell = drops(&baseline, &now);
        if fell.is_empty() {
            println!("coverage: nothing fell by more than {SLACK} points since {BASELINE}");
        }
        for w in &fell {
            println!("warning: {w}");
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                println!("::warning title=Coverage fell::{w}");
            }
        }
        notes.extend(fell);
    } else {
        println!("coverage: no {BASELINE} yet; `cargo xtask coverage --update` writes one");
    }
    // On GitHub the numbers also go on the run's summary page.
    if let Some(summary) = std::env::var_os("GITHUB_STEP_SUMMARY") {
        let mut page = format!("## Coverage\n\n{shown}");
        for n in &notes {
            page.push_str(&format!("\n- {n}"));
        }
        page.push('\n');
        let _ = fs::OpenOptions::new().create(true).append(true).open(summary).and_then(|mut f| std::io::Write::write_all(&mut f, page.as_bytes()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Value {
        json!({"data": [{
            "totals": {
                "lines": {"covered": 75, "count": 100},
                "functions": {"covered": 9, "count": 10},
                "regions": {"covered": 8, "count": 10}
            },
            "files": [
                {"filename": "C:\\r\\crates\\engine\\src\\a.rs", "summary": {"lines": {"covered": 40, "count": 50}}},
                {"filename": "/r/crates/engine/src/b.rs", "summary": {"lines": {"covered": 10, "count": 10}}},
                {"filename": "/r/apps/notecraft/src/main.rs", "summary": {"lines": {"covered": 5, "count": 20}}},
                {"filename": "/r/tests/x.rs", "summary": {"lines": {"covered": 20, "count": 20}}}
            ]
        }]})
    }

    #[test]
    fn files_are_grouped_by_crate() {
        let s = summarize(&report()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(s.lines, Share { covered: 75, count: 100 });
        assert_eq!(s.crates.get("crates/engine"), Some(&Share { covered: 50, count: 60 }));
        assert_eq!(s.crates.get("apps/notecraft"), Some(&Share { covered: 5, count: 20 }));
        assert_eq!(s.crates.len(), 2, "a file outside crates/ and apps/ counts in the total only");
        assert!(summarize(&json!({})).is_err());
    }

    #[test]
    fn only_a_real_drop_is_reported() {
        let s = summarize(&report()).unwrap_or_else(|e| panic!("{e}"));
        let same = baseline_json(&s, json!({}));
        assert_eq!(same["total"]["lines"], json!(75.0));
        assert_eq!(same["crates"]["crates/engine"]["lines"], json!(83.3));
        assert!(drops(&same, &s).is_empty(), "the run it was written from does not drop");

        let mut higher = same.clone();
        higher["total"]["lines"] = json!(75.4);
        assert!(drops(&higher, &s).is_empty(), "within the slack");
        higher["total"]["lines"] = json!(80.0);
        higher["crates"]["apps/notecraft"]["lines"] = json!(30.0);
        let fell = drops(&higher, &s);
        assert_eq!(fell.len(), 2, "{fell:?}");
        assert!(fell[0].contains("total line coverage fell from 80.0% to 75.0%"), "{fell:?}");
        assert!(fell[1].contains("apps/notecraft"), "{fell:?}");
        // A crate the baseline has never seen is not a drop.
        assert!(drops(&json!({"total": {}, "crates": {}}), &s).is_empty());
    }
}

//! Timings for the developer validation pipeline. Local history stays under `target/`.

use serde_json::{Value, json};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const REQUIRED_SAMPLES: usize = 3;
const BASELINE_WINDOW: usize = 5;
const REGRESSION_PERCENT: f64 = 0.25;
const MIN_REGRESSION_MS: u64 = 30_000;

pub struct Recorder {
    root: PathBuf,
    task: String,
    platform: String,
    rustc: String,
    started: Instant,
    started_at: u64,
    phases: Vec<Value>,
}

impl Recorder {
    pub fn new(root: &Path, task: &str) -> Self {
        Self {
            root: root.to_path_buf(),
            task: task.to_string(),
            platform: std::env::consts::OS.to_string(),
            rustc: rustc_version(),
            started: Instant::now(),
            started_at: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_secs()),
            phases: Vec::new(),
        }
    }

    pub fn measure(&mut self, name: &str, operation: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let started = Instant::now();
        let result = operation();
        let elapsed_ms = duration_ms(started.elapsed());
        let success = result.is_ok();
        self.phases.push(json!({"name": name, "duration_ms": elapsed_ms, "success": success}));
        println!("devx timing: {}/{} took {}", self.task, name, format_duration(elapsed_ms));
        result
    }

    pub fn run_command(&mut self, name: &str, program: &str, args: &[String]) -> Result<(), String> {
        let command = std::iter::once(program).chain(args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ");
        println!("$ {command}");
        let root = self.root.clone();
        self.measure(name, || {
            let status = Command::new(program).args(args).current_dir(root).status().map_err(|error| format!("cannot run {command}: {error}"))?;
            if status.success() { Ok(()) } else { Err(format!("{command} failed with {status}")) }
        })
    }

    /// Records the run and reports regressions against recent successful runs on the same
    /// platform and Rust toolchain. Failure to save timing data is visible but does not change
    /// the quality-gate result.
    pub fn finish(&self, success: bool) {
        let current = json!({
            "schema": 1,
            "task": self.task,
            "platform": self.platform,
            "rustc": self.rustc,
            "started_at": self.started_at,
            "duration_ms": duration_ms(self.started.elapsed()),
            "success": success,
            "phases": self.phases,
        });
        let path = history_path(&self.root);
        let history = match read_history(&path) {
            Ok(history) => history,
            Err(error) => {
                eprintln!("devx timing warning: {error}");
                Vec::new()
            }
        };
        let warnings = regression_warnings(&history, &current);
        for warning in &warnings {
            eprintln!("devx timing warning: {warning}");
            if std::env::var_os("GITHUB_ACTIONS").is_some() {
                println!("::warning title=Developer workflow regression::{warning}");
            }
        }
        if let Err(error) = append_history(&path, &current) {
            eprintln!("devx timing warning: could not save timing history: {error}");
        }
        let summary = summary_markdown(&self.task, &self.platform, &self.rustc, &current, &warnings);
        if let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY") {
            if let Err(error) = append_summary(Path::new(&path), &summary) {
                eprintln!("devx timing warning: could not append GitHub summary: {error}");
            }
        } else {
            println!("{summary}");
        }
    }
}

pub fn run_timed_command(root: &Path, args: &[String]) -> Result<(), String> {
    let (phase, program, command_args) = parse_command_args(args)?;
    let mut recorder = Recorder::new(root, "platform-build-test");
    let result = recorder.run_command(phase, program, command_args);
    recorder.finish(result.is_ok());
    result
}

fn parse_command_args(args: &[String]) -> Result<(&str, &str, &[String]), String> {
    let Some(separator) = args.iter().position(|arg| arg == "--") else {
        return Err("usage: cargo xtask devx-time PHASE -- COMMAND [ARGS...]".into());
    };
    if separator != 1 || args.len() <= separator + 1 {
        return Err("usage: cargo xtask devx-time PHASE -- COMMAND [ARGS...]".into());
    }
    Ok((&args[0], &args[separator + 1], &args[separator + 2..]))
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn history_path(root: &Path) -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target"));
    let target = if target.is_absolute() { target } else { root.join(target) };
    target.join("devx").join("timings.jsonl")
}

fn read_history(path: &Path) -> Result<Vec<Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        match serde_json::from_str(line) {
            Ok(record) => records.push(record),
            Err(error) => eprintln!("devx timing warning: ignoring malformed history record {} in {}: {error}", index + 1, path.display()),
        }
    }
    Ok(records)
}

fn append_history(path: &Path, record: &Value) -> Result<(), String> {
    let Some(parent) = path.parent() else { return Err(format!("timing history has no parent: {}", path.display())) };
    fs::create_dir_all(parent).map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    let mut bytes = serde_json::to_vec(record).map_err(|error| format!("cannot encode timing record: {error}"))?;
    bytes.push(b'\n');
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    file.write_all(&bytes).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn append_summary(path: &Path, summary: &str) -> Result<(), String> {
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("cannot open {}: {error}", path.display()))?;
    file.write_all(summary.as_bytes()).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

fn regression_warnings(history: &[Value], current: &Value) -> Vec<String> {
    if current["success"].as_bool() != Some(true) {
        return Vec::new();
    }
    let mut warnings = Vec::new();
    let phases = current["phases"].as_array().into_iter().flatten();
    for phase in phases {
        if phase["success"].as_bool() != Some(true) {
            continue;
        }
        let Some(name) = phase["name"].as_str() else { continue };
        let Some(now) = phase["duration_ms"].as_u64() else { continue };
        let matching_count = history
            .iter()
            .filter(|record| {
                record["success"].as_bool() == Some(true)
                    && record["task"] == current["task"]
                    && record["platform"] == current["platform"]
                    && record["rustc"] == current["rustc"]
            })
            .flat_map(|record| record["phases"].as_array().into_iter().flatten())
            .filter(|candidate| candidate["name"].as_str() == Some(name) && candidate["success"].as_bool() == Some(true))
            .count();
        if matching_count < REQUIRED_SAMPLES {
            continue;
        }
        // Keep the newest five matching samples. They are appended in chronological order.
        let mut samples = history
            .iter()
            .filter(|record| {
                record["success"].as_bool() == Some(true)
                    && record["task"] == current["task"]
                    && record["platform"] == current["platform"]
                    && record["rustc"] == current["rustc"]
            })
            .flat_map(|record| record["phases"].as_array().into_iter().flatten())
            .filter_map(|candidate| {
                (candidate["name"].as_str() == Some(name) && candidate["success"].as_bool() == Some(true))
                    .then(|| candidate["duration_ms"].as_u64())
                    .flatten()
            })
            .collect::<Vec<_>>();
        if samples.len() > BASELINE_WINDOW {
            samples.drain(..samples.len() - BASELINE_WINDOW);
        }
        let baseline = samples.into_iter().max();
        let Some(baseline) = baseline else { continue };
        let delta = now.saturating_sub(baseline);
        if delta >= MIN_REGRESSION_MS && (delta as f64) / (baseline.max(1) as f64) > REGRESSION_PERCENT {
            let percent = delta as f64 * 100.0 / baseline.max(1) as f64;
            warnings.push(format!(
                "{}/{} took {} vs recent same-machine high-water {} (+{percent:.0}%)",
                current["task"].as_str().unwrap_or("task"),
                name,
                format_duration(now),
                format_duration(baseline)
            ));
        }
    }
    warnings
}

fn duration_ms(duration: std::time::Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn format_duration(milliseconds: u64) -> String {
    if milliseconds >= 60_000 {
        format!("{}m {}s", milliseconds / 60_000, (milliseconds % 60_000) / 1_000)
    } else if milliseconds >= 1_000 {
        format!("{:.1}s", milliseconds as f64 / 1_000.0)
    } else {
        format!("{milliseconds}ms")
    }
}

fn summary_markdown(task: &str, platform: &str, rustc: &str, record: &Value, warnings: &[String]) -> String {
    let mut summary = format!(
        "### Developer workflow timing: `{task}`\n\nPlatform: `{platform}`; toolchain: `{rustc}`\n\n| Phase | Time | Result |\n|---|---:|---|\n"
    );
    for phase in record["phases"].as_array().into_iter().flatten() {
        let name = phase["name"].as_str().unwrap_or("unknown");
        let time = phase["duration_ms"].as_u64().map(format_duration).unwrap_or_else(|| "unknown".to_string());
        let result = if phase["success"].as_bool() == Some(true) { "pass" } else { "fail" };
        summary.push_str(&format!("| {name} | {time} | {result} |\n"));
    }
    if warnings.is_empty() {
        summary.push_str("\nTiming warnings compare successful runs on the same machine, platform, and Rust toolchain; at least three samples are required. The baseline is the slowest of the latest five runs to reduce noise.\n\n");
    } else {
        for warning in warnings {
            summary.push_str(&format!("\n- {warning}"));
        }
        summary.push_str("\n\n");
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn record(platform: &str, task: &str, rustc: &str, samples: &[(u64, bool)]) -> Value {
        json!({
            "success": true,
            "platform": platform,
            "task": task,
            "rustc": rustc,
            "phases": [{"name": "ui-tests", "duration_ms": samples[0].0, "success": samples[0].1}]
        })
    }

    #[test]
    fn timing_warning_requires_three_matching_successes_and_a_material_regression() {
        let history = vec![
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 1", &[(110_000, true)]),
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
        ];
        let current = record("windows", "ci", "rustc 1", &[(150_000, true)]);

        assert_eq!(regression_warnings(&history[..2], &current).len(), 0, "two samples are not a baseline");
        assert_eq!(regression_warnings(&history, &current).len(), 1);
        assert!(regression_warnings(&history, &current)[0].contains("ui-tests"));
    }

    #[test]
    fn timing_warning_compares_against_the_recent_slowest_run_and_ignores_small_deltas() {
        let history = vec![
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 1", &[(110_000, true)]),
            record("windows", "ci", "rustc 1", &[(120_000, true)]),
        ];
        let current = record("windows", "ci", "rustc 1", &[(149_000, true)]);
        assert!(regression_warnings(&history, &current).is_empty(), "29 seconds does not clear the noise floor");
        let current = record("windows", "ci", "rustc 1", &[(151_000, true)]);
        assert_eq!(regression_warnings(&history, &current).len(), 1);
    }

    #[test]
    fn timing_warning_ignores_noise_and_nonmatching_platforms_or_toolchains() {
        let history = vec![
            record("linux", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 1", &[(100_000, true)]),
            record("windows", "other", "rustc 1", &[(100_000, true)]),
            record("windows", "ci", "rustc 2", &[(100_000, true)]),
        ];
        let current = record("windows", "ci", "rustc 1", &[(129_000, true)]);

        assert!(regression_warnings(&history, &current).is_empty());
    }

    #[test]
    fn timed_command_requires_a_phase_and_a_command_after_the_separator() {
        assert!(parse_command_args(&["build".into(), "--".into(), "cargo".into()]).is_ok());
        assert!(parse_command_args(&["build".into(), "cargo".into()]).is_err());
        assert!(parse_command_args(&["--".into(), "cargo".into()]).is_err());
        assert!(parse_command_args(&["build".into(), "--".into()]).is_err());
    }

    #[test]
    fn failed_previous_runs_do_not_seed_the_timing_baseline() {
        let history = vec![
            record("windows", "ci", "rustc 1", &[(100_000, false)]),
            record("windows", "ci", "rustc 1", &[(110_000, false)]),
            record("windows", "ci", "rustc 1", &[(100_000, false)]),
        ];
        let current = record("windows", "ci", "rustc 1", &[(150_000, true)]);

        assert!(regression_warnings(&history, &current).is_empty());
    }
}

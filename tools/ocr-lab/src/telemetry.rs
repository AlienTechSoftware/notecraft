//! What the lab records while it runs: one timestamped line per event, kept in a file and in memory
//! so the copy report can carry the recent events. Times are Unix seconds; no paths leave the lab
//! except relative to the test folder.

use serde::Serialize;
use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

const KEEP: usize = 200;

pub struct Log {
    file: Option<std::fs::File>,
    recent: VecDeque<String>,
}

impl Log {
    /// Opens (or creates) the log file. A failure to open it is not fatal: events still reach
    /// the in-memory buffer and the copy report.
    pub fn open(path: &Path) -> Log {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let file = std::fs::OpenOptions::new().create(true).append(true).open(path).ok();
        Log { file, recent: VecDeque::new() }
    }

    pub fn event(&mut self, message: &str) {
        let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let line = format!("{at} {message}");
        if let Some(file) = self.file.as_mut() {
            let _ = writeln!(file, "{line}");
        }
        if self.recent.len() == KEEP {
            self.recent.pop_front();
        }
        self.recent.push_back(line);
    }

    pub fn recent(&self) -> Vec<String> {
        self.recent.iter().cloned().collect()
    }
}

/// One timed step of a run, in milliseconds.
#[derive(Debug, Clone, Serialize)]
pub struct Timing {
    pub step: &'static str,
    pub ms: f64,
}

pub fn timed<T>(step: &'static str, timings: &mut Vec<Timing>, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    timings.push(Timing { step, ms: start.elapsed().as_secs_f64() * 1000.0 });
    out
}

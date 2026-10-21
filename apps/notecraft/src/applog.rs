//! The app's log file. A release build on Windows has no console, so without this a crash, a
//! GPU error or a failed start leaves nothing behind.
//!
//! `notecraft.log` sits next to the settings file (`%APPDATA%\NoteCraft` on Windows). Each line
//! is `UTC time, level, text`. The file is opened for every line and never held open, so the last
//! line before a hard crash is on disk. When it passes 1 MB it is renamed to `notecraft.log.old`.
//! Warnings and errors from every library are kept, and so is the information from NoteCraft,
//! eframe and the window and GPU glue, which is where start-up problems show.

use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use web_time::{SystemTime, UNIX_EPOCH};

const MAX_BYTES: u64 = 1_000_000;

static PATH: OnceLock<PathBuf> = OnceLock::new();

/// Sets the log file up, rotates it if it is large, and routes the `log` crate and panics to it.
pub fn init(dir: PathBuf) {
    let file = dir.join("notecraft.log");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&file).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&file, dir.join("notecraft.log.old"));
    }
    let _ = PATH.set(file);
    if log::set_logger(&Logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        line("PANIC", &format!("{info}\n{}", std::backtrace::Backtrace::force_capture()));
        previous(info);
    }));
}

/// Appends one line, `time level text`. Never fails the caller.
pub fn line(level: &str, text: &str) {
    let Some(path) = PATH.get() else { return };
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} {level} {text}", utc_now());
    }
}

pub fn info(text: &str) {
    line("INFO", text);
}

struct Logger;

impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata) -> bool {
        let t = m.target();
        m.level() <= log::Level::Warn
            || t.starts_with("notecraft")
            || t.starts_with("eframe")
            || t.starts_with("egui_wgpu")
            || t.starts_with("egui_winit")
    }

    fn log(&self, r: &log::Record) {
        if self.enabled(r.metadata()) {
            line(r.level().as_str(), &format!("{}: {}", r.target(), r.args()));
        }
    }

    fn flush(&self) {}
}

/// `YYYY-MM-DD HH:MM:SS` in UTC, from the system clock.
fn utc_now() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    format_utc(secs)
}

fn format_utc(secs: u64) -> String {
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Days since 1970-01-01 to a calendar date (Howard Hinnant's civil_from_days).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_known_instants() {
        assert_eq!(format_utc(0), "1970-01-01 00:00:00");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00:00");
        assert_eq!(format_utc(1_792_340_000), "2026-10-18 16:13:20");
    }
}

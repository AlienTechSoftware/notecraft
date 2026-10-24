//! Local-only usage and diagnostic session events.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::{SystemTime, UNIX_EPOCH};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    None,
    Usage,
    Warning,
    Error,
    Debug,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Usage => "usage",
            Self::Warning => "warning",
            Self::Error => "error",
            Self::Debug => "debug",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "usage" => Some(Self::Usage),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            "debug" => Some(Self::Debug),
            _ => None,
        }
    }

    pub fn allows(self, event: Self) -> bool {
        self != Self::None && event <= self
    }

    pub fn for_build() -> Self {
        if cfg!(debug_assertions) { Self::Debug } else { Self::None }
    }
}

#[derive(Debug)]
pub struct Session {
    id: String,
    dir: PathBuf,
    level: Level,
}

impl Session {
    pub fn start(root: impl AsRef<Path>, level: Level) -> io::Result<Self> {
        let root = root.as_ref();
        fs::create_dir_all(root)?;
        let id = session_id();
        let dir = root.join(&id);
        fs::create_dir(&dir)?;
        let session = Self { id, dir, level };
        session.write_atomic(
            "manifest.json",
            &format!(
                "{{\"schema\":1,\"session_id\":{},\"level\":{},\"build_profile\":{}}}",
                json_string(&session.id),
                json_string(level.as_str()),
                json_string(build_profile())
            ),
        )?;
        Ok(session)
    }

    pub fn record(&self, level: Level, event: &str, fields: &[(&str, &str)]) -> io::Result<()> {
        if !self.level.allows(level) {
            return Ok(());
        }
        let mut json =
            format!("{{\"timestamp\":{},\"level\":{},\"event\":{}", json_string(&utc_now()), json_string(level.as_str()), json_string(event));
        for (key, value) in fields {
            json.push_str(&format!(",{}:{}", json_string(key), json_string(&redact(value))));
        }
        json.push('}');
        let path = self.dir.join("events.jsonl");
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        writeln!(file, "{json}")
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn write_atomic(&self, name: &str, contents: &str) -> io::Result<()> {
        let target = self.dir.join(name);
        let temp = self.dir.join(format!(".{name}.tmp-{}", std::process::id()));
        {
            let mut file = File::create(&temp)?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
        }
        fs::rename(temp, target)
    }
}

pub fn telemetry_root(settings_dir: &Path) -> PathBuf {
    settings_dir.join("telemetry")
}

fn session_id() -> String {
    let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let seconds = (millis / 1000) as i64;
    let serial = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let stamp = date_stamp(seconds);
    format!("{stamp}-{:08x}{:08x}", std::process::id(), (serial ^ millis as u64) as u32)
}

fn date_stamp(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}{:02}{:02}{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

fn build_profile() -> &'static str {
    if cfg!(debug_assertions) { "debug" } else { "production" }
}

fn utc_now() -> String {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs().to_string()).unwrap_or_else(|_| "0".into())
}

fn redact(value: &str) -> String {
    value.replace(['\\', '/'], "[path]").replace(['\r', '\n'], " ")
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn levels_are_ordered_and_none_is_silent() {
        assert!(Level::Debug.allows(Level::Error));
        assert!(!Level::Error.allows(Level::Debug));
        assert!(!Level::None.allows(Level::Usage));
    }

    #[test]
    fn session_ids_start_with_a_readable_datetime() {
        let id = session_id();
        assert_eq!(id.len(), 31);
        assert_eq!(&id[14..15], "-");
        assert!(id[..14].chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn simultaneous_sessions_have_unique_ids_and_atomic_manifest() {
        let root = std::env::temp_dir().join(format!(
            "notecraft-telemetry-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        let a = Session::start(&root, Level::Debug).expect("session");
        let b = Session::start(&root, Level::Debug).expect("session");
        assert_ne!(a.id, b.id);
        assert!(a.dir.join("manifest.json").is_file());
        assert!(fs::read_dir(&root).expect("root").filter_map(Result::ok).map(|e| e.file_name()).collect::<HashSet<_>>().len() == 2);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn events_are_json_and_redact_paths() {
        let root = std::env::temp_dir().join(format!(
            "notecraft-telemetry-event-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        let session = Session::start(&root, Level::Debug).expect("session");
        session.record(Level::Warning, "example", &[("path", "C:\\private\\note")]).expect("event");
        let text = fs::read_to_string(session.dir.join("events.jsonl")).expect("events");
        assert!(serde_json::from_str::<serde_json::Value>(text.trim()).is_ok());
        assert!(!text.contains("C:\\private"));
        let _ = fs::remove_dir_all(root);
    }
}

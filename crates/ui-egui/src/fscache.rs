//! What the window last learned about a file, kept for a moment.
//!
//! Drawing a page asks about its files every frame: whether each picture changed, whether an
//! attachment and its icon are there. On a local disk each answer is a few microseconds. On a
//! synced folder mounted through FUSE (OneDrive through onedriver) an answer the kernel has not
//! kept goes to the mount's own process, and a page of printouts asked hundreds of them a frame:
//! the window drew a few frames a second. Files change far less often than frames are drawn, so
//! an answer is kept for [`FRESH`] and asked again after that.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
// std's Instant panics in the web build, which has no clock of its own.
use web_time::Instant;

/// How long an answer about a file is trusted.
pub const FRESH: Duration = Duration::from_secs(2);

/// What a file was: whether it is a file, and its length and time when it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub file: bool,
    pub len: u64,
    pub modified: Option<SystemTime>,
}

#[cfg(test)]
thread_local! {
    static DISK_ASKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many times the disk was asked, on this thread, for tests.
#[cfg(test)]
pub(crate) fn disk_asks() -> usize {
    DISK_ASKS.with(std::cell::Cell::get)
}

thread_local! {
    static SEEN: std::cell::RefCell<HashMap<PathBuf, (Instant, Seen)>> = std::cell::RefCell::new(HashMap::new());
}

/// What `path` is, asked of the disk at most once every [`FRESH`].
pub fn seen(path: &Path) -> Seen {
    seen_at(path, Instant::now())
}

fn seen_at(path: &Path, now: Instant) -> Seen {
    if let Some(kept) = SEEN.with(|s| s.borrow().get(path).filter(|(at, _)| now.duration_since(*at) < FRESH).map(|(_, seen)| *seen)) {
        return kept;
    }
    #[cfg(test)]
    DISK_ASKS.with(|n| n.set(n.get() + 1));
    let seen = match std::fs::metadata(path) {
        Ok(m) => Seen { file: m.is_file(), len: m.len(), modified: m.modified().ok() },
        Err(_) => Seen { file: false, len: 0, modified: None },
    };
    SEEN.with(|s| {
        let mut s = s.borrow_mut();
        // Pages come and go; what is long out of date is let go now and then.
        if s.len() > 4096 {
            s.retain(|_, (at, _)| now.duration_since(*at) < FRESH);
        }
        s.insert(path.to_path_buf(), (now, seen));
    });
    seen
}

/// Whether `path` is a file, as [`seen`].
pub fn is_file(path: &Path) -> bool {
    seen(path).file
}

/// Forgets what is known about `path`, after the window wrote or removed it itself.
pub fn forget(path: &Path) {
    SEEN.with(|s| s.borrow_mut().remove(path));
}

/// Forgets everything known, after a command that may have changed any file of the page.
pub fn forget_all() {
    SEEN.with(|s| s.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_kept_for_a_moment_and_then_asked_again() {
        let t = notecraft_store::testing::TempDir::new("fscache");
        let file = t.path().join("a.png");
        let now = Instant::now();
        assert!(!seen_at(&file, now).file);
        std::fs::write(&file, b"png").expect("write");
        assert!(!seen_at(&file, now + Duration::from_millis(500)).file, "within the moment, the kept answer");
        let later = seen_at(&file, now + FRESH + Duration::from_millis(1));
        assert!(later.file && later.len == 3, "after it, the disk is asked again");
    }

    #[test]
    fn forgetting_asks_the_disk_at_once() {
        let t = notecraft_store::testing::TempDir::new("fscache-forget");
        let file = t.path().join("b.svg");
        assert!(!is_file(&file));
        std::fs::write(&file, b"<svg/>").expect("write");
        forget(&file);
        assert!(is_file(&file));
    }
}

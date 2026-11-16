//! Which pages the user has read since they last changed, for History > Next Unread and Mark as
//! Read (issue #291).
//!
//! The state is the user's own, not the notebook's: it lives beside their settings, keyed by
//! each page's id (so moving or renaming a page keeps it), with the page's `updated` stamp as
//! it was when they last had it open. A page is unread when its stamp is later than that,
//! that is when it changed after they last saw it, or when they marked it unread. Pages they
//! have never opened count from when the state was started, so a first run does not make every
//! old page unread. Their own edits do not make a page unread: the window records the page as
//! read whenever it saves it or leaves it.
//!
//! Stamps are compared as NoteCraft writes them, `YYYY-MM-DDTHH:MM:SSZ` in UTC, which sort in
//! time order as text.

use crate::tree::page_file;
use notecraft_model::PageFormat;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Marks a page the user asked to see as unread, whatever its stamp.
const FORCED: &str = "unread";

#[derive(Clone, Debug)]
pub struct ReadState {
    file: PathBuf,
    /// When the state was started; pages never opened are unread only if changed after it.
    since: String,
    /// Page id to the stamp it had when last read, or [`FORCED`].
    seen: BTreeMap<String, String>,
}

/// The `updated` stamp a page keeps in its own file, as written.
pub fn page_stamp(dir: &Path) -> Option<String> {
    let (format, file) = page_file(dir)?;
    let text = std::fs::read_to_string(&file).ok()?;
    let value = match format {
        PageFormat::Html => notecraft_formats::html::meta(&text, "notecraft:updated"),
        PageFormat::Cmgfm => notecraft_formats::frontmatter::value(&notecraft_formats::frontmatter::split_front(&text).0, "updated"),
    }?;
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

impl ReadState {
    /// The state kept in `file`, or a new one starting now when there is none yet (it is
    /// written straight away, so "now" stays the start).
    pub fn load(file: &Path) -> Self {
        let mut state = Self { file: file.to_path_buf(), since: String::new(), seen: BTreeMap::new() };
        if let Ok(text) = std::fs::read_to_string(file) {
            for line in text.lines() {
                let Some((key, value)) = line.split_once('\t') else { continue };
                if key == "since" {
                    state.since = value.to_string();
                } else if !key.is_empty() {
                    state.seen.insert(key.to_string(), value.to_string());
                }
            }
        }
        if state.since.is_empty() {
            state.since = notecraft_model::time::iso_now();
            let _ = state.save();
        }
        state
    }

    /// Writes the state back to its file.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut text = format!("since\t{}\n", self.since);
        for (id, stamp) in &self.seen {
            text.push_str(&format!("{id}\t{stamp}\n"));
        }
        let tmp = self.file.with_extension("tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.file)
    }

    /// True when the page with `id` and stamp `stamp` changed after the user last read it, or
    /// they marked it unread.
    pub fn is_unread(&self, id: &str, stamp: Option<&str>) -> bool {
        match self.seen.get(id).map(String::as_str) {
            Some(FORCED) => true,
            Some(read) => stamp.is_some_and(|s| s > read),
            None => stamp.is_some_and(|s| s > self.since.as_str()),
        }
    }

    /// Whether the page was marked unread by hand.
    pub fn is_marked_unread(&self, id: &str) -> bool {
        self.seen.get(id).map(String::as_str) == Some(FORCED)
    }

    /// Records the page as read as it stands, at stamp `stamp`.
    pub fn mark_read(&mut self, id: &str, stamp: Option<&str>) {
        if id.is_empty() {
            return;
        }
        let stamp = stamp.map_or_else(notecraft_model::time::iso_now, str::to_string);
        self.seen.insert(id.to_string(), stamp);
    }

    /// Marks the page unread until it is next opened or marked read.
    pub fn mark_unread(&mut self, id: &str) {
        if !id.is_empty() {
            self.seen.insert(id.to_string(), FORCED.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn a_page_is_unread_when_it_changed_after_it_was_read() {
        let t = TempDir::new("read-state");
        let file = t.path().join("read-state.tsv");
        let mut s = ReadState::load(&file);
        s.since = "2026-11-01T00:00:00Z".into();
        // Never opened: unread only if changed after the state began.
        assert!(!s.is_unread("a", Some("2026-10-01T00:00:00Z")));
        assert!(s.is_unread("a", Some("2026-11-02T00:00:00Z")));
        assert!(!s.is_unread("a", None), "a page with no stamp is not called unread");
        s.mark_read("a", Some("2026-11-02T00:00:00Z"));
        assert!(!s.is_unread("a", Some("2026-11-02T00:00:00Z")), "read as it stands");
        assert!(s.is_unread("a", Some("2026-11-03T08:00:00Z")), "changed since");
        s.mark_unread("a");
        assert!(s.is_unread("a", Some("2026-11-02T00:00:00Z")) && s.is_marked_unread("a"), "marked by hand");
        s.mark_read("a", Some("2026-11-03T08:00:00Z"));
        assert!(!s.is_marked_unread("a"));
        s.save().expect("save");

        let again = ReadState::load(&file);
        assert_eq!(again.since, "2026-11-01T00:00:00Z");
        assert!(!again.is_unread("a", Some("2026-11-03T08:00:00Z")), "kept across runs");
        assert!(again.is_unread("a", Some("2026-11-04T00:00:00Z")));
    }

    #[test]
    fn a_new_state_starts_now_and_is_written_at_once() {
        let t = TempDir::new("read-state-new");
        let file = t.path().join("deep").join("read-state.tsv");
        let s = ReadState::load(&file);
        assert!(file.is_file());
        assert!(!s.is_unread("old", Some("2020-01-01T00:00:00Z")));
        assert!(s.is_unread("new", Some("2999-01-01T00:00:00Z")));
    }

    #[test]
    fn a_page_keeps_its_own_stamp() {
        let t = TempDir::new("read-state-stamp");
        let settings = crate::settings::Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(t.path(), "NB", None, &settings).expect("nb");
        let section = crate::ops::create_node(&nb, "S", notecraft_model::NodeKind::Section, None, &settings).expect("section");
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let page = crate::ops::create_node(&section, "P", notecraft_model::NodeKind::Page, Some(format), &settings).expect("page");
            let saved = crate::page::save_body(&page, "words").expect("save");
            assert_eq!(page_stamp(&page), Some(saved), "{format:?}");
        }
    }
}

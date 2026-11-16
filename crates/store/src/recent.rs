//! The pages changed lately, for History > Recent Edits (issue #279).
//!
//! A page's own `updated` stamp is what counts, the one every save writes into its file (a
//! `notecraft:updated` meta in HTML, `updated:` in CMGFM front matter): a sync tool that copies
//! a folder sets every file's modified time to the moment of the copy, and would make every page
//! look just edited. Only a page with no stamp at all falls back to its file's modified time.

use crate::tree::{Node, page_file};
use notecraft_model::{NodeKind, PageFormat};
use std::path::{Path, PathBuf};

/// A page changed in the range asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edited {
    pub page: PathBuf,
    pub title: String,
    /// The notebook it is in, by its title.
    pub notebook: String,
    /// The section groups and section it is in, outermost first, by their titles.
    pub section: Vec<String>,
    /// When it was last changed, seconds since 1970 UTC.
    pub at: i64,
    /// Read from the page's own stamp rather than its file's modified time.
    pub stamped: bool,
}

/// When the page at `dir` was last changed, by the stamp its file keeps. `None` when it has
/// none that can be read.
pub fn stamp(dir: &Path) -> Option<i64> {
    let (format, file) = page_file(dir)?;
    // The stamp is in the head: there is no need to read a large page to the end.
    let text = std::fs::read_to_string(&file).ok()?;
    let value = match format {
        PageFormat::Html => notecraft_formats::html::meta(&text, "notecraft:updated"),
        PageFormat::Cmgfm => notecraft_formats::frontmatter::value(&notecraft_formats::frontmatter::split_front(&text).0, "updated"),
    }?;
    notecraft_model::time::secs_from_iso(&value)
}

fn file_time(dir: &Path) -> Option<i64> {
    let (_, file) = page_file(dir)?;
    let modified = std::fs::metadata(file).ok()?.modified().ok()?;
    let secs = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// The pages under `notebooks` changed at or after `since` (every page with `None`), newest
/// first; pages changed at the same second keep the order they are in.
pub fn edited(notebooks: &[Node], since: Option<i64>) -> Vec<Edited> {
    let mut out = Vec::new();
    for notebook in notebooks {
        walk(notebook, &notebook.info.title, &mut Vec::new(), since, &mut out);
    }
    out.sort_by_key(|e| std::cmp::Reverse(e.at));
    out
}

fn walk(node: &Node, notebook: &str, trail: &mut Vec<String>, since: Option<i64>, out: &mut Vec<Edited>) {
    for child in &node.children {
        match child.info.kind {
            NodeKind::Page => {
                let (at, stamped) = match stamp(&child.path) {
                    Some(at) => (Some(at), true),
                    None => (file_time(&child.path), false),
                };
                if let Some(at) = at
                    && since.is_none_or(|s| at >= s)
                {
                    out.push(Edited {
                        page: child.path.clone(),
                        title: child.info.title.clone(),
                        notebook: notebook.to_string(),
                        section: trail.clone(),
                        at,
                        stamped,
                    });
                }
                // A page's subpages are pages too.
                walk(child, notebook, trail, since, out);
            }
            _ => {
                trail.push(child.info.title.clone());
                walk(child, notebook, trail, since, out);
                trail.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::testing::TempDir;

    fn set_stamp(dir: &Path, iso: &str) {
        let (format, file) = page_file(dir).expect("page");
        let text = std::fs::read_to_string(&file).expect("read");
        let out = match format {
            PageFormat::Html => notecraft_formats::html::set_meta(&text, "notecraft:updated", iso),
            PageFormat::Cmgfm => {
                let (front, rest) = notecraft_formats::frontmatter::split_front(&text);
                format!("{}{rest}", notecraft_formats::frontmatter::set_value(&front, "updated", iso))
            }
        };
        std::fs::write(&file, out).expect("write");
    }

    #[test]
    fn pages_are_listed_newest_first_by_their_own_stamp_with_where_they_are() {
        let t = TempDir::new("recent-edits");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(t.path(), "Work", None, &s).expect("notebook");
        let section = crate::ops::create_node(&nb, "Plans", NodeKind::Section, None, &s).expect("section");
        let old = crate::ops::create_node(&section, "Old", NodeKind::Page, Some(PageFormat::Html), &s).expect("page");
        let new = crate::ops::create_node(&section, "New", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("page");
        let sub = crate::ops::create_node(&new, "Sub", NodeKind::Page, Some(PageFormat::Html), &s).expect("subpage");
        set_stamp(&old, "2026-11-01T09:00:00Z");
        set_stamp(&new, "2026-11-14T09:00:00Z");
        set_stamp(&sub, "2026-11-10T09:00:00Z");
        let notebooks = crate::tree::load_notebooks(t.path());

        let all = edited(&notebooks, None);
        assert_eq!(all.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["New", "Sub", "Old"]);
        assert!(all.iter().all(|e| e.stamped && e.notebook == "Work" && e.section == ["Plans"]), "{all:?}");

        let since = notecraft_model::time::secs_from_iso("2026-11-05T00:00:00Z");
        assert_eq!(edited(&notebooks, since).iter().map(|e| e.title.as_str()).collect::<Vec<_>>(), ["New", "Sub"]);
        assert_eq!(stamp(&old), notecraft_model::time::secs_from_iso("2026-11-01T09:00:00Z"));
    }

    #[test]
    fn a_page_without_a_stamp_falls_back_to_its_file_time() {
        let t = TempDir::new("recent-edits-unstamped");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(t.path(), "Home", None, &s).expect("notebook");
        let section = crate::ops::create_node(&nb, "Lists", NodeKind::Section, None, &s).expect("section");
        let page = crate::ops::create_node(&section, "Plain", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("page");
        let (_, file) = page_file(&page).expect("file");
        std::fs::write(&file, "just words\n").expect("write");
        let got = edited(&crate::tree::load_notebooks(t.path()), None);
        assert_eq!(got.len(), 1);
        assert!(!got[0].stamped);
        assert!((got[0].at - notecraft_model::time::now_secs()).abs() < 600);
    }
}

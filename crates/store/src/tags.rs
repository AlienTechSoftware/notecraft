//! Finding the tagged notes in a page, a section, a notebook or every notebook: what OneNote's
//! Find Tags lists in its Tags Summary (issue #166).
//!
//! A tag is part of the line it is on (see [`notecraft_model::tag`]): a To Do is a task list
//! item, and every other tag is its marker word at the start of the line. So the pages are read
//! the way the editor reads them, block by block, and a block counts by the same rule the editor
//! draws its tag by. Nothing is cached or indexed: the folders are the notebook, and what they
//! say now is what is listed.

use crate::page::load_page;
use crate::tree::{NodeInfo, child_dirs, page_file, read_node};
use notecraft_formats::boxes;
use notecraft_formats::markdown::{Block, Span};
use notecraft_model::{NodeKind, Tag};
use std::path::{Path, PathBuf};

/// One tagged line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tagged {
    /// The page it is on.
    pub path: PathBuf,
    /// That page's title.
    pub page: String,
    /// The section the page is in, or empty when it is in none.
    pub section: String,
    pub tag: Tag,
    /// The words on the line, without the tag's marker.
    pub text: String,
    /// For a To Do, whether its box is ticked.
    pub done: Option<bool>,
    /// Which block of the page the line is, counted the way the editor counts them.
    pub block: usize,
}

/// The most lines listed: a summary longer than this is not one a person reads.
pub const MAX: usize = 2000;

fn pages_under(dir: &Path, out: &mut Vec<PathBuf>) {
    if page_file(dir).is_some() {
        out.push(dir.to_path_buf());
    }
    for c in child_dirs(dir) {
        pages_under(&c, out);
    }
}

fn plain(spans: &[Span]) -> String {
    spans.iter().filter(|s| s.image.is_none()).map(|s| s.text.as_str()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The tags on one block: the To Do box of a task item, and the tag marker at the start of
/// its words. A task item can carry both, the way a OneNote line can carry two tags.
pub fn tags_of(block: &Block) -> Vec<(Tag, String, Option<bool>)> {
    let (spans, task) = match block {
        Block::Item { spans, task, .. } => (spans, *task),
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) => (spans, None),
        _ => return Vec::new(),
    };
    let text = plain(spans);
    let (marked, rest) = Tag::split(&text);
    let rest = notecraft_model::text::without_mark(rest).trim().to_string();
    let mut out = Vec::new();
    if let Some(done) = task {
        out.push((Tag::ToDo, rest.clone(), Some(done)));
    }
    if let Some(tag) = marked.filter(|t| *t != Tag::ToDo || task.is_none()) {
        out.push((tag, rest, None));
    }
    out
}

/// The section a page is in: the nearest folder above it that is a section.
fn section_of(page: &Path) -> String {
    let mut at = page.parent();
    while let Some(dir) = at {
        match read_node(dir) {
            Some(NodeInfo { kind: NodeKind::Section, title, .. }) => return title,
            Some(NodeInfo { kind: NodeKind::Page, .. }) => {}
            _ => return String::new(),
        }
        at = dir.parent();
    }
    String::new()
}

/// Every tagged line on the pages at or below each folder in `scopes` (a page and its
/// subpages, a section, a section group or a notebook), in the order the tree shows them.
/// With `only_page`, a page scope means that page alone. A page reached by more than one
/// scope is read once. At most `limit` lines, and never more than [`MAX`].
pub fn find(scopes: &[PathBuf], only_page: bool, limit: usize) -> Vec<Tagged> {
    let limit = limit.min(MAX);
    let mut pages = Vec::new();
    for scope in scopes {
        if only_page && page_file(scope).is_some() {
            pages.push(scope.clone());
        } else {
            pages_under(scope, &mut pages);
        }
    }
    let mut seen = std::collections::HashSet::new();
    pages.retain(|page| seen.insert(std::fs::canonicalize(page).unwrap_or_else(|_| page.clone())));
    let mut out = Vec::new();
    for dir in pages {
        let Ok(page) = load_page(&dir) else { continue };
        let (blocks, _) = boxes::read(page.format, &page.body);
        let section = section_of(&dir);
        for (block, b) in blocks.iter().enumerate() {
            for (tag, text, done) in tags_of(b) {
                if out.len() >= limit {
                    return out;
                }
                out.push(Tagged { path: dir.clone(), page: page.title.clone(), section: section.clone(), tag, text, done, block });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::page::save_body;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::PageFormat;

    struct Fixture {
        root: PathBuf,
        notebook: PathBuf,
        section: PathBuf,
        html: PathBuf,
        md: PathBuf,
        other: PathBuf,
    }

    fn fixture(t: &TempDir) -> Fixture {
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let notebook = ops::create_notebook(&root, "Work", None, &s).expect("nb");
        let section = ops::create_node(&notebook, "Plans", NodeKind::Section, None, &s).expect("section");
        let html = ops::create_node(&section, "Kickoff", NodeKind::Page, Some(PageFormat::Html), &s).expect("html");
        // Written the way the editor writes an HTML page, from the same blocks.
        let blocks = notecraft_formats::markdown::parse("#important Book the room\n\n- [ ] Send the agenda\n\nPlain words\n\nsee #idea here\n");
        save_body(&html, &notecraft_formats::render::blocks_to_html(&blocks)).expect("save");
        let md = ops::create_node(&section, "Minutes", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("md");
        save_body(&md, "#question Who **owns** the budget?\n\n- [x] Write minutes\n- [ ] #critical Fix the build\n\n#ideas for later\n")
            .expect("save");
        let other_section = ops::create_node(&notebook, "Later", NodeKind::Section, None, &s).expect("section 2");
        let other = ops::create_node(&other_section, "Someday", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("other");
        save_body(&other, "#idea A garden\n").expect("save");
        Fixture { root, notebook, section, html, md, other }
    }

    fn summary(found: &[Tagged]) -> Vec<(String, Tag, String, Option<bool>)> {
        found.iter().map(|t| (t.page.clone(), t.tag, t.text.clone(), t.done)).collect()
    }

    #[test]
    fn tagged_lines_are_found_in_both_formats() {
        let t = TempDir::new("tags-both-formats");
        let f = fixture(&t);
        let found = find(std::slice::from_ref(&f.section), false, 100);
        assert_eq!(
            summary(&found),
            vec![
                ("Kickoff".into(), Tag::Important, "Book the room".into(), None),
                ("Kickoff".into(), Tag::ToDo, "Send the agenda".into(), Some(false)),
                ("Minutes".into(), Tag::Question, "Who owns the budget?".into(), None),
                ("Minutes".into(), Tag::ToDo, "Write minutes".into(), Some(true)),
                ("Minutes".into(), Tag::ToDo, "Fix the build".into(), Some(false)),
                ("Minutes".into(), Tag::Critical, "Fix the build".into(), None),
            ],
            "a marker in the middle of a line, or a word that only starts like one, is not a tag"
        );
        assert!(found.iter().all(|t| t.section == "Plans"), "{found:?}");
        assert_eq!(found[0].path, f.html);
        assert_eq!(found[2].path, f.md);
    }

    #[test]
    fn the_block_is_the_one_the_editor_counts() {
        let t = TempDir::new("tags-block");
        let f = fixture(&t);
        let page = load_page(&f.md).expect("page");
        let (blocks, _) = boxes::read(page.format, &page.body);
        for tagged in find(std::slice::from_ref(&f.md), true, 100) {
            let b = blocks.get(tagged.block).expect("the block exists");
            assert!(tags_of(b).iter().any(|(tag, _, _)| *tag == tagged.tag), "{tagged:?} points at {b:?}");
        }
    }

    #[test]
    fn the_scope_decides_which_pages_are_read() {
        let t = TempDir::new("tags-scope");
        let f = fixture(&t);
        let pages = |found: Vec<Tagged>| {
            let mut p: Vec<String> = found.into_iter().map(|t| t.page).collect();
            p.dedup();
            p
        };
        assert_eq!(pages(find(std::slice::from_ref(&f.html), true, 100)), vec!["Kickoff"]);
        assert_eq!(pages(find(std::slice::from_ref(&f.section), false, 100)), vec!["Kickoff", "Minutes"]);
        assert_eq!(pages(find(std::slice::from_ref(&f.notebook), false, 100)), vec!["Kickoff", "Minutes", "Someday"]);
        let other = find(std::slice::from_ref(&f.other), true, 100);
        assert_eq!(other[0].section, "Later");
        // A page reached twice is listed once.
        assert_eq!(find(&[f.root.join("work"), f.notebook.clone(), f.section.clone()], false, 100).len(), 7);
    }

    #[test]
    fn limits_and_missing_folders() {
        let t = TempDir::new("tags-limits");
        let f = fixture(&t);
        assert_eq!(find(std::slice::from_ref(&f.notebook), false, 2).len(), 2);
        assert!(find(std::slice::from_ref(&f.notebook), false, 0).is_empty());
        assert!(find(&[PathBuf::from("/definitely/not/here")], false, 10).is_empty());
        assert!(find(&[], false, 10).is_empty());
    }
}

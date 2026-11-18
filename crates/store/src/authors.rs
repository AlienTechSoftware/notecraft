//! Who changed a page, for History > Find by Author and Hide Authors (issue #292).
//!
//! A page keeps the names of the people who saved changes to it, in its own file beside its
//! `updated` stamp: `notecraft:authors` and `notecraft:edited-by` metas in HTML, `authors:` and
//! `edited-by:` in CMGFM front matter. A name is written only by a save made while a name is
//! set (File > Account, issue #304), and only into the page that save changed. A page nobody
//! saved with a name keeps none: its author is unknown, and it is never labelled after the
//! fact as the person who happens to open it now.
//!
//! The names are the people's own words, kept on this computer and in the pages they changed.
//! Nothing is sent anywhere, and a page copied to someone else carries the names with it, which
//! File > Account says.

use crate::tree::{Node, page_file};
use notecraft_formats::{frontmatter, html};
use notecraft_model::{NodeKind, PageFormat};
use std::path::{Path, PathBuf};

const AUTHORS_KEY: &str = "authors";
const AUTHORS_META: &str = "notecraft:authors";
const LAST_KEY: &str = "edited-by";
const LAST_META: &str = "notecraft:edited-by";

/// The longest name kept, in characters, and how many names one page keeps.
pub const MAX_NAME: usize = 64;
const MAX_AUTHORS: usize = 32;

/// A name as it can be kept: one line, no commas (they separate the names), at most
/// [`MAX_NAME`] characters. `None` for a name that is empty once tidied.
pub fn clean(name: &str) -> Option<String> {
    let one: String = name.chars().map(|c| if c == ',' || c == ';' || c.is_control() { ' ' } else { c }).collect();
    let tidy = one.split_whitespace().collect::<Vec<_>>().join(" ");
    let tidy: String = tidy.chars().take(MAX_NAME).collect();
    let tidy = tidy.trim().to_string();
    (!tidy.is_empty()).then_some(tidy)
}

/// Who changed one page: everyone who saved a change with a name, in the order they first
/// did, and who saved the last such change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Authors {
    pub names: Vec<String>,
    pub last: Option<String>,
}

impl Authors {
    /// True when the page keeps no name: who wrote it is unknown.
    pub fn unknown(&self) -> bool {
        self.names.is_empty()
    }
}

fn split(list: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in list.split(',').filter_map(clean) {
        if !out.contains(&name) && out.len() < MAX_AUTHORS {
            out.push(name);
        }
    }
    out
}

/// Who changed the page whose file text is `text`.
pub fn read(format: PageFormat, text: &str) -> Authors {
    let (list, last) = match format {
        PageFormat::Html => (html::meta(text, AUTHORS_META), html::meta(text, LAST_META)),
        PageFormat::Cmgfm => {
            let front = frontmatter::split_front(text).0;
            (frontmatter::value(&front, AUTHORS_KEY), frontmatter::value(&front, LAST_KEY))
        }
    };
    let names = list.as_deref().map(split).unwrap_or_default();
    let last = last.as_deref().and_then(clean).filter(|l| names.contains(l));
    Authors { names, last }
}

/// Who changed the page in `dir`. A page that cannot be read has no names.
pub fn of(dir: &Path) -> Authors {
    let Some((format, file)) = page_file(dir) else { return Authors::default() };
    std::fs::read_to_string(&file).map(|text| read(format, &text)).unwrap_or_default()
}

/// The page file text `text` with `author` noted as having changed it: added to its names when
/// it is not one of them, and made the last. Called by a save, on the text it is about to write.
/// A CMGFM page with no front matter has nowhere to keep it and is left as it is.
pub fn note(format: PageFormat, text: &str, author: &str) -> String {
    let Some(author) = clean(author) else { return text.to_string() };
    let mut have = read(format, text);
    if have.last.as_deref() == Some(author.as_str()) {
        return text.to_string();
    }
    if !have.names.contains(&author) {
        if have.names.len() >= MAX_AUTHORS {
            have.names.remove(0);
        }
        have.names.push(author.clone());
    }
    let list = have.names.join(", ");
    match format {
        PageFormat::Html => html::set_meta(&html::set_meta(text, AUTHORS_META, &list), LAST_META, &author),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(text);
            if front.is_empty() {
                return text.to_string();
            }
            let front = frontmatter::set_value(&frontmatter::set_value(&front, AUTHORS_KEY, &list), LAST_KEY, &author);
            format!("{front}{rest}")
        }
    }
}

/// One page and who changed it, with where it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub page: PathBuf,
    pub title: String,
    pub notebook: String,
    /// The section groups and section it is in, outermost first.
    pub section: Vec<String>,
    pub authors: Authors,
}

/// Every page under `notebooks`, subpages included, with who changed it, in the order they are
/// in the notebooks.
pub fn pages(notebooks: &[Node]) -> Vec<Page> {
    fn walk(node: &Node, notebook: &str, trail: &mut Vec<String>, out: &mut Vec<Page>) {
        for child in &node.children {
            if child.info.kind == NodeKind::Page {
                out.push(Page {
                    page: child.path.clone(),
                    title: child.info.title.clone(),
                    notebook: notebook.to_string(),
                    section: trail.clone(),
                    authors: of(&child.path),
                });
                walk(child, notebook, trail, out);
            } else {
                trail.push(child.info.title.clone());
                walk(child, notebook, trail, out);
                trail.pop();
            }
        }
    }
    let mut out = Vec::new();
    for notebook in notebooks {
        walk(notebook, &notebook.info.title, &mut Vec::new(), &mut out);
    }
    out
}

/// Everyone named in `pages`, with how many pages they changed, by name; then how many pages
/// keep no name.
pub fn names(pages: &[Page]) -> (Vec<(String, usize)>, usize) {
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut unknown = 0;
    for p in pages {
        if p.authors.unknown() {
            unknown += 1;
        }
        for name in &p.authors.names {
            match out.iter_mut().find(|(n, _)| n == name) {
                Some((_, count)) => *count += 1,
                None => out.push((name.clone(), 1)),
            }
        }
    }
    out.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()).then_with(|| a.0.cmp(&b.0)));
    (out, unknown)
}

/// The pages `author` changed, or with `None` the pages whose author is unknown.
pub fn by<'a>(pages: &'a [Page], author: Option<&str>) -> Vec<&'a Page> {
    pages
        .iter()
        .filter(|p| match author {
            Some(name) => p.authors.names.iter().any(|n| n == name),
            None => p.authors.unknown(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use crate::testing::TempDir;

    #[test]
    fn a_name_is_tidied_to_one_line_without_commas() {
        assert_eq!(clean("  Ada,  Lovelace\n").as_deref(), Some("Ada Lovelace"));
        assert_eq!(clean(" , ; \t"), None);
        assert_eq!(clean(&"x".repeat(200)).map(|n| n.chars().count()), Some(MAX_NAME));
    }

    #[test]
    fn a_save_with_a_name_notes_it_in_both_formats_and_nothing_else_changes() {
        let html =
            "<!doctype html><html><head><meta name=\"notecraft:updated\" content=\"2026-11-01T09:00:00Z\"></head><body><p>Hi</p></body></html>";
        assert!(read(PageFormat::Html, html).unknown(), "a page nobody saved with a name is unknown");
        let once = note(PageFormat::Html, html, "Ada");
        let twice = note(PageFormat::Html, &once, "Grace");
        let back = note(PageFormat::Html, &twice, "Ada");
        assert_eq!(read(PageFormat::Html, &back), Authors { names: vec!["Ada".into(), "Grace".into()], last: Some("Ada".into()) });
        assert!(back.contains("<p>Hi</p>") && back.contains("2026-11-01T09:00:00Z"), "{back}");
        assert_eq!(note(PageFormat::Html, &back, "Ada"), back, "the same person again changes nothing");

        let md = "---\ntitle: Plans\nupdated: 2026-11-01T09:00:00Z\n---\n\nHi\n";
        let md = note(PageFormat::Cmgfm, &note(PageFormat::Cmgfm, md, "Ada"), "Grace");
        assert_eq!(read(PageFormat::Cmgfm, &md), Authors { names: vec!["Ada".into(), "Grace".into()], last: Some("Grace".into()) });
        assert!(md.ends_with("\nHi\n") && md.contains("title: Plans") && md.contains("updated: 2026-11-01T09:00:00Z"), "{md}");
        assert_eq!(note(PageFormat::Cmgfm, "just words\n", "Ada"), "just words\n", "no front matter, nowhere to keep it");
        assert_eq!(note(PageFormat::Cmgfm, md.as_str(), "  "), md, "no name, nothing noted");
    }

    #[test]
    fn pages_are_found_by_who_changed_them_and_the_rest_are_unknown() {
        let t = TempDir::new("authors-find");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(t.path(), "Work", None, &s).expect("notebook");
        let section = crate::ops::create_node(&nb, "Plans", NodeKind::Section, None, &s).expect("section");
        let a = crate::ops::create_node(&section, "A", NodeKind::Page, Some(PageFormat::Html), &s).expect("page");
        let b = crate::ops::create_node(&section, "B", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("page");
        let _c = crate::ops::create_node(&section, "C", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("page");
        crate::page::save_body_by(&a, "<p>one</p>", Some("Ada")).expect("save");
        crate::page::save_body_by(&b, "two\n", Some("Ada")).expect("save");
        crate::page::save_body_by(&b, "two more\n", Some("Grace")).expect("save");
        let all = pages(&crate::tree::load_notebooks(t.path()));
        let (named, unknown) = names(&all);
        assert_eq!(named, [("Ada".to_string(), 2), ("Grace".to_string(), 1)]);
        assert_eq!(unknown, 1, "C was never saved with a name");
        assert_eq!(by(&all, Some("Grace")).iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["B"]);
        assert_eq!(by(&all, None).iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["C"]);
        assert!(by(&all, Some("Ada")).iter().all(|p| p.notebook == "Work" && p.section == ["Plans"]));
        assert_eq!(of(&b).last.as_deref(), Some("Grace"));
        assert_eq!(crate::page::load_page(&b).expect("load").body.trim(), "two more", "the words are what was saved");
    }
}

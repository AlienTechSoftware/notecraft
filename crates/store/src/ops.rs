//! Creating, renaming and deleting nodes.

use crate::error::{Result, StoreError};
use crate::page::write_atomic;
use crate::settings::{FormatSource, Settings};
use crate::tree::{self, child_dirs, page_file, read_node};
use notecraft_formats::{frontmatter, html, templates};
use notecraft_model::{NodeColor, NodeKind, PageFormat, id::new_id, kv, names, time::iso_now};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const MAX_TITLE: usize = 200;

/// The format for a new page under `start`: explicit choice, then the nearest
/// `_node.toml` that sets a format (stopping at the notebook), then the global
/// setting, then the built-in default.
pub fn effective_format(start: Option<&Path>, explicit: Option<PageFormat>, settings: &Settings) -> (PageFormat, FormatSource) {
    if let Some(f) = explicit {
        return (f, FormatSource::Explicit);
    }
    let mut cur = start;
    while let Some(dir) = cur {
        if let Ok(text) = fs::read_to_string(dir.join("_node.toml")) {
            let map = kv::parse(&text);
            if let Some(f) = map.get("format").and_then(|v| PageFormat::parse(v).ok()) {
                return (f, FormatSource::Tree);
            }
            if map.get("kind").map(String::as_str) == Some("notebook") {
                break;
            }
        }
        cur = dir.parent();
    }
    settings.global_format()
}

fn clean_title(title: &str) -> Result<String> {
    let t = title.trim();
    if t.is_empty() {
        return Err(StoreError::invalid("a title is needed"));
    }
    if t.chars().count() > MAX_TITLE {
        return Err(StoreError::invalid(format!("a title cannot be longer than {MAX_TITLE} characters")));
    }
    if t.chars().any(char::is_control) {
        return Err(StoreError::invalid("a title cannot contain control characters"));
    }
    Ok(t.to_string())
}

/// A folder path under `parent` that does not exist yet, ignoring case, so two
/// pages titled "Notes" and "notes" never collide on Windows or macOS.
fn unique_child(parent: &Path, base: &str) -> PathBuf {
    let taken: HashSet<String> = match fs::read_dir(parent) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_lowercase()).collect(),
        Err(_) => HashSet::new(),
    };
    let mut name = base.to_string();
    let mut n = 2;
    while taken.contains(&name.to_lowercase()) {
        name = format!("{base}-{n}");
        n += 1;
    }
    parent.join(name)
}

fn node_file(kind: NodeKind, title: &str, id: &str, order: Option<u32>, format: Option<PageFormat>) -> String {
    let mut s = format!("kind = {}\ntitle = {}\nid = {}\n", kv::quote(kind.id()), kv::quote(title), kv::quote(id));
    if let Some(f) = format {
        s.push_str(&format!("format = {}\n", kv::quote(f.id())));
    }
    if let Some(o) = order {
        s.push_str(&format!("order = {o}\n"));
    }
    s.push_str(&format!("created = {}\n", kv::quote(&iso_now())));
    s
}

/// Creates a notebook folder inside `root`. The notebook records its own effective
/// default format so it behaves the same on a machine with other settings.
pub fn create_notebook(root: &Path, title: &str, explicit: Option<PageFormat>, settings: &Settings) -> Result<PathBuf> {
    let title = clean_title(title)?;
    fs::create_dir_all(root).map_err(|e| StoreError::io("create", root, e))?;
    let dir = unique_child(root, &names::slug(&title));
    let (format, _) = effective_format(None, explicit, settings);
    fs::create_dir(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    if let Err(e) = fs::write(dir.join("_node.toml"), node_file(NodeKind::Notebook, &title, &new_id(), None, Some(format))) {
        let _ = fs::remove_dir_all(&dir);
        return Err(StoreError::io("write", &dir.join("_node.toml"), e));
    }
    Ok(dir)
}

/// Creates a section group, section or page inside `parent`. The parent must be a
/// notebook, section group, section or page that may contain `kind`.
///
/// A page is created in `explicit` format, or the format from `effective_format`. A
/// section group or section is a folder with a `_node.toml`; `explicit` there sets the
/// default format for pages below it.
pub fn create_node(parent: &Path, title: &str, kind: NodeKind, explicit: Option<PageFormat>, settings: &Settings) -> Result<PathBuf> {
    create_node_at(parent, title, kind, explicit, settings, None)
}

/// The same, with a place in the order of its siblings. Siblings are numbered ten apart, so
/// a new node can be slipped between two of them by asking for the number in between.
pub fn create_node_at(
    parent: &Path,
    title: &str,
    kind: NodeKind,
    explicit: Option<PageFormat>,
    settings: &Settings,
    want_order: Option<u32>,
) -> Result<PathBuf> {
    let title = clean_title(title)?;
    create_node_inner(parent, &title, kind, explicit, settings, want_order, Some(&title))
}

/// A page with no title of its own: the "Add Page" case. The folder is still named from
/// `tree::UNTITLED_PAGE`, the way a page always needs some name, but nothing is written
/// into its head. The tree, the page list and the window's title bar show
/// `tree::UNTITLED_PAGE` as a stand-in for it, the way OneNote calls a page that with
/// nothing typed in it "Untitled Page", and the title area on the page itself is drawn
/// blank. The page stops being untitled the moment a title is written in, however short;
/// see `set_title`.
pub fn create_untitled_page(parent: &Path, explicit: Option<PageFormat>, settings: &Settings) -> Result<PathBuf> {
    create_node_inner(parent, tree::UNTITLED_PAGE, NodeKind::Page, explicit, settings, None, None)
}

/// Shared by `create_node_at` and `create_untitled_page`. `slug_title` names the folder;
/// `page_title` is what is written into a page's own head, `None` leaving it out
/// entirely. A section group or section always gets `slug_title` as its real title: only a
/// page can be nameless.
fn create_node_inner(
    parent: &Path,
    slug_title: &str,
    kind: NodeKind,
    explicit: Option<PageFormat>,
    settings: &Settings,
    want_order: Option<u32>,
    page_title: Option<&str>,
) -> Result<PathBuf> {
    let parent_info =
        read_node(parent).ok_or_else(|| StoreError::NotFound(format!("{} is not a notebook, section, group or page", parent.display())))?;
    if !parent_info.kind.may_contain().contains(&kind) {
        return Err(StoreError::invalid(format!("a {} cannot contain a {}", parent_info.kind.label().to_lowercase(), kind.label().to_lowercase())));
    }
    let order = match want_order {
        Some(o) => o,
        None => u32::try_from(child_dirs(parent).len()).unwrap_or(0).saturating_mul(10).saturating_add(10),
    };
    let dir = unique_child(parent, &names::slug(slug_title));
    let (now, id) = (iso_now(), new_id());
    fs::create_dir(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    let written = (|| -> Result<()> {
        if kind == NodeKind::Page {
            let (format, _) = effective_format(Some(parent), explicit, settings);
            let body = match format {
                PageFormat::Html => templates::html_page(page_title, &id, order, &now),
                PageFormat::Cmgfm => templates::markdown_page(page_title, &id, order, &now),
            };
            let file = dir.join(format.file_name());
            fs::write(&file, body).map_err(|e| StoreError::io("write", &file, e))?;
            let assets = dir.join("assets");
            fs::create_dir(&assets).map_err(|e| StoreError::io("create", &assets, e))?;
        } else {
            let file = dir.join("_node.toml");
            fs::write(&file, node_file(kind, slug_title, &id, Some(order), explicit)).map_err(|e| StoreError::io("write", &file, e))?;
        }
        Ok(())
    })();
    if let Err(e) = written {
        let _ = fs::remove_dir_all(&dir);
        return Err(e);
    }
    Ok(dir)
}

/// Deletes a node and everything inside it. `path` must be inside `root` and be a
/// NoteCraft node. Returns the number of nodes removed.
pub fn delete_node(root: &Path, path: &Path) -> Result<usize> {
    let root = fs::canonicalize(root).map_err(|e| StoreError::io("open", root, e))?;
    let path = fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))?;
    if path == root || !path.starts_with(&root) {
        return Err(StoreError::invalid("only a notebook, section, group or page inside the notebooks folder can be deleted"));
    }
    let node = tree::load_tree(&path).ok_or_else(|| StoreError::invalid(format!("{} is not a NoteCraft node", path.display())))?;
    let count = node.count();
    fs::remove_dir_all(&path).map_err(|e| StoreError::io("delete", &path, e))?;
    Ok(count)
}

/// Gives a notebook, section group or section a colour, the way OneNote colours a notebook
/// cover or a section tab. `None` removes it, so the node takes the theme's colour again.
/// Pages have no colour: their folder holds the page, not a `_node.toml`.
pub fn set_color(dir: &Path, color: Option<NodeColor>) -> Result<()> {
    if page_file(dir).is_some() {
        return Err(StoreError::invalid("a page has no colour of its own; colour its section instead"));
    }
    let file = dir.join("_node.toml");
    let text = fs::read_to_string(&file).map_err(|_| StoreError::NotFound(format!("{} is not a NoteCraft folder", dir.display())))?;
    let out = match color {
        Some(c) => {
            let line = format!("color = {}", kv::quote(c.id()));
            if text.lines().any(|l| l.trim_start().starts_with("color =")) {
                kv::replace_line(&text, "color =", &line)
            } else {
                let mut s = text;
                if !s.ends_with('\n') {
                    s.push('\n');
                }
                s.push_str(&line);
                s.push('\n');
                s
            }
        }
        None => text.lines().filter(|l| !l.trim_start().starts_with("color =")).map(|l| format!("{l}\n")).collect(),
    };
    write_atomic(&file, &out)
}

/// Changes the title of a node. The folder name is not changed.
///
/// A new page is written with its title in two places: where the page records it, and as
/// the heading the reader sees. Renaming used to change only the first, so the page said
/// one thing at the top of the window and another on the page - two titles, drifting
/// apart with every rename. The heading moves with the title now, as long as it still
/// says what the title said: a heading someone has edited themselves is theirs, and is
/// left alone.
pub fn set_title(dir: &Path, title: &str) -> Result<()> {
    let title = clean_title(title)?;
    if let Some((format, file)) = page_file(dir) {
        let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
        let out = match format {
            PageFormat::Html => {
                let was = html::title(&text).unwrap_or_default();
                let renamed = html::replace_between(&text, "<title>", "</title>", &html::escape(&title));
                retitle_heading(&renamed, &was, &title, PageFormat::Html)
            }
            PageFormat::Cmgfm => {
                let (front, rest) = frontmatter::split_front(&text);
                let was = frontmatter::value(&front, "title").unwrap_or_default();
                let front = kv::replace_line(&front, "title:", &format!("title: {}", kv::quote(&title)));
                format!("{front}{}", retitle_heading(&rest, &was, &title, PageFormat::Cmgfm))
            }
        };
        write_atomic(&file, &out)
    } else {
        let file = dir.join("_node.toml");
        let text = fs::read_to_string(&file).map_err(|_| StoreError::NotFound(format!("{} is not a NoteCraft folder", dir.display())))?;
        write_atomic(&file, &kv::replace_line(&text, "title =", &format!("title = {}", kv::quote(&title))))
    }
}

/// Rewrites a page's first heading when it still says `was`, leaving it alone otherwise.
///
/// Only the first heading, and only when it matches: a page whose heading has been
/// written by hand is not the title's to change.
fn retitle_heading(body: &str, was: &str, now: &str, format: PageFormat) -> String {
    if was.trim().is_empty() || was.trim() == now.trim() {
        return body.to_string();
    }
    match format {
        PageFormat::Html => {
            let heading = html::between(body, "<h1>", "</h1>");
            match heading {
                Some(h) if html::unescape(&h).trim() == was.trim() => html::replace_between(body, "<h1>", "</h1>", &html::escape(now)),
                _ => body.to_string(),
            }
        }
        PageFormat::Cmgfm => {
            let mut out = String::with_capacity(body.len());
            let mut done = false;
            for line in body.split_inclusive('\n') {
                let bare = line.trim_end_matches(['\n', '\r']);
                if !done && let Some(rest) = bare.strip_prefix("# ") {
                    done = true;
                    if rest.trim() == was.trim() {
                        out.push_str("# ");
                        out.push_str(now);
                        out.push_str(&line[bare.len()..]);
                        continue;
                    }
                }
                out.push_str(line);
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    /// A page says its title twice: once where the page records it, once as the heading a
    /// reader sees. Renaming has to move both, or the window shows one and the page the
    /// other - which is what the owner saw.
    #[test]
    fn renaming_a_page_moves_its_heading_too() {
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let t = TempDir::new(&format!("retitle-{}", format.id()));
            let s = settings(&t);
            let nb = create_notebook(&root(&t), "Book", Some(format), &s).expect("notebook");
            let page = create_node(&nb, "First name", notecraft_model::NodeKind::Page, Some(format), &s).expect("page");

            // A page written before the title moved out of the body carries it as a
            // heading too. Renaming moves both, or the window and the page disagree.
            let file = page_file(&page).expect("file").1;
            let had = fs::read_to_string(&file).expect("read");
            let old_style = match format {
                PageFormat::Html => had.replace("<body>", "<body>\n<h1>First name</h1>"),
                PageFormat::Cmgfm => had.replace("---\n\n", "---\n\n# First name\n\n"),
            };
            fs::write(&file, &old_style).expect("write");

            set_title(&page, "Second name").expect("rename");
            let text = fs::read_to_string(&file).expect("read");
            assert!(text.contains("Second name"), "{format:?}: the new name is there:\n{text}");
            assert!(!text.contains("First name"), "{format:?}: and the old one is gone everywhere:\n{text}");

            // A heading someone has written themselves is theirs: renaming leaves it. A new
            // page has none - its title is in the head alone - so the test writes one.
            let file = page_file(&page).expect("file").1;
            let text = fs::read_to_string(&file).expect("read");
            let mine = match format {
                PageFormat::Html => text.replace("<body>", "<body>\n<h1>My own heading</h1>"),
                PageFormat::Cmgfm => text.replace("---\n\n", "---\n\n# My own heading\n\n"),
            };
            fs::write(&file, &mine).expect("write");
            set_title(&page, "Third name").expect("rename again");
            let text = fs::read_to_string(&file).expect("read");
            assert!(text.contains("My own heading"), "{format:?}: a heading of their own is left alone:\n{text}");
            assert!(text.contains("Third name"), "{format:?}: while the title still changes:\n{text}");
        }
    }

    fn settings(t: &TempDir) -> Settings {
        Settings::at(t.path().join("settings").join("s.toml"))
    }

    fn root(t: &TempDir) -> PathBuf {
        t.path().join("notebooks")
    }

    #[test]
    fn notebook_records_its_own_format() {
        let t = TempDir::new("ops1");
        let s = settings(&t);
        s.set_default_format(PageFormat::Cmgfm).expect("set");
        let nb = create_notebook(&root(&t), "Work", None, &s).expect("nb");
        assert_eq!(read_node(&nb).and_then(|i| i.format), Some(PageFormat::Cmgfm));
        // Changing the global setting later does not change this notebook.
        s.set_default_format(PageFormat::Html).expect("set");
        let page = create_node(&nb, "P", NodeKind::Page, None, &s).expect("page");
        assert!(page.join("index.md").is_file(), "the notebook's own setting wins over the global one");
    }

    #[test]
    fn format_cascade_in_order() {
        let t = TempDir::new("ops2");
        let s = settings(&t);
        let r = root(&t);
        let nb = create_notebook(&r, "NB", None, &s).expect("nb");
        assert_eq!(effective_format(Some(&nb), None, &s), (PageFormat::Html, FormatSource::Tree));
        let sec = create_node(&nb, "Ideas", NodeKind::Section, Some(PageFormat::Cmgfm), &s).expect("sec");
        assert_eq!(effective_format(Some(&sec), None, &s), (PageFormat::Cmgfm, FormatSource::Tree));
        assert_eq!(effective_format(Some(&sec), Some(PageFormat::Html), &s), (PageFormat::Html, FormatSource::Explicit));
        let plain = t.path().join("plain");
        fs::create_dir_all(&plain).expect("dir");
        assert_eq!(effective_format(Some(&plain), None, &s), (PageFormat::Html, FormatSource::BuiltIn));
        s.set_default_format(PageFormat::Cmgfm).expect("set");
        assert_eq!(effective_format(Some(&plain), None, &s), (PageFormat::Cmgfm, FormatSource::Global));
    }

    #[test]
    fn kinds_follow_the_containment_rules() {
        let t = TempDir::new("ops3");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let sec = create_node(&nb, "S", NodeKind::Section, None, &s).expect("sec");
        assert!(create_node(&sec, "Bad", NodeKind::Section, None, &s).is_err(), "a section cannot hold a section");
        assert!(create_node(&sec, "Bad", NodeKind::Notebook, None, &s).is_err());
        let g = create_node(&nb, "G", NodeKind::SectionGroup, None, &s).expect("group");
        assert!(create_node(&g, "Inner", NodeKind::SectionGroup, None, &s).is_ok(), "groups nest");
        let page = create_node(&sec, "P", NodeKind::Page, None, &s).expect("page");
        assert!(create_node(&page, "Sub", NodeKind::Page, None, &s).is_ok(), "pages nest");
        let plain = t.path().join("plain");
        fs::create_dir_all(&plain).expect("dir");
        assert!(create_node(&plain, "X", NodeKind::Page, None, &s).is_err(), "not inside a notebook");
    }

    #[test]
    fn pages_nest_far_beyond_three_levels() {
        let t = TempDir::new("ops4");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let mut p = create_node(&nb, "Top", NodeKind::Page, None, &s).expect("top");
        for i in 1..=20 {
            p = create_node(&p, &format!("Level {i}"), NodeKind::Page, None, &s).expect("sub");
        }
        assert!(read_node(&p).is_some());
    }

    #[test]
    fn same_titles_get_distinct_folders_ignoring_case() {
        let t = TempDir::new("ops5");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let a = create_node(&nb, "Notes", NodeKind::Page, None, &s).expect("a");
        let b = create_node(&nb, "NOTES", NodeKind::Page, None, &s).expect("b");
        let c = create_node(&nb, "notes", NodeKind::Page, None, &s).expect("c");
        let names: HashSet<String> = [a, b, c].iter().map(|p| tree::folder_name(p).to_lowercase()).collect();
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn reserved_names_are_avoided() {
        let t = TempDir::new("ops6");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let p = create_node(&nb, "CON", NodeKind::Page, None, &s).expect("page");
        assert_eq!(tree::folder_name(&p), "con-page");
    }

    #[test]
    fn colours_are_saved_removed_and_survive_a_rename() {
        let t = TempDir::new("ops-color");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let section = create_node(&nb, "Work", NodeKind::Section, None, &s).expect("section");
        assert_eq!(read_node(&section).and_then(|i| i.color), None, "a new section has no colour");
        set_color(&section, Some(NodeColor::Teal)).expect("set");
        assert_eq!(read_node(&section).and_then(|i| i.color), Some(NodeColor::Teal));
        // Setting it again replaces the line instead of adding a second one.
        set_color(&section, Some(NodeColor::Red)).expect("set again");
        let text = fs::read_to_string(section.join("_node.toml")).expect("read");
        assert_eq!(text.lines().filter(|l| l.starts_with("color =")).count(), 1);
        assert_eq!(read_node(&section).and_then(|i| i.color), Some(NodeColor::Red));
        // The rest of the file is untouched.
        set_title(&section, "Work v2").expect("rename");
        assert_eq!(read_node(&section).and_then(|i| i.color), Some(NodeColor::Red));
        assert_eq!(read_node(&section).map(|i| i.title), Some("Work v2".to_string()));
        set_color(&section, None).expect("clear");
        assert_eq!(read_node(&section).and_then(|i| i.color), None);
        assert!(read_node(&section).is_some(), "clearing the colour keeps the node");
        // A notebook can be coloured; a page cannot.
        set_color(&nb, Some(NodeColor::Blue)).expect("notebook");
        assert_eq!(read_node(&nb).and_then(|i| i.color), Some(NodeColor::Blue));
        let page = create_node(&section, "Note", NodeKind::Page, None, &s).expect("page");
        assert!(set_color(&page, Some(NodeColor::Blue)).is_err());
        assert!(set_color(&t.path().join("nowhere"), Some(NodeColor::Blue)).is_err());
    }

    #[test]
    fn titles_are_validated() {
        let t = TempDir::new("ops7");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        assert!(create_node(&nb, "   ", NodeKind::Page, None, &s).is_err());
        assert!(create_node(&nb, &"x".repeat(MAX_TITLE + 1), NodeKind::Page, None, &s).is_err());
        assert!(create_node(&nb, "a\u{0}b", NodeKind::Page, None, &s).is_err());
        assert!(create_notebook(&root(&t), "", None, &s).is_err());
    }

    #[test]
    fn unusual_titles_still_make_valid_folders() {
        let t = TempDir::new("ops8");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        for title in
            ["a/b\\c:d*e?f\"g<h>i|j", "  spaced  ", "tab\tchars", "emoji \u{1F600}", "???", "\u{e9}\u{e8} accents", "CON.txt", "trailing dot."]
        {
            let title = title.replace('\t', " ");
            let p = create_node(&nb, &title, NodeKind::Page, None, &s).expect("page");
            assert!(names::validate_component(&tree::folder_name(&p)).is_ok(), "{title:?} gave {:?}", tree::folder_name(&p));
        }
    }

    #[test]
    fn rename_title_for_pages_and_containers() {
        let t = TempDir::new("ops9");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let html_page = create_node(&nb, "One", NodeKind::Page, Some(PageFormat::Html), &s).expect("html");
        let md_page = create_node(&nb, "Two", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("md");
        set_title(&html_page, "A & B").expect("title");
        set_title(&md_page, "Say \"hi\"").expect("title");
        set_title(&nb, "Renamed").expect("title");
        assert_eq!(read_node(&html_page).map(|i| i.title).as_deref(), Some("A & B"));
        assert_eq!(read_node(&md_page).map(|i| i.title).as_deref(), Some("Say \"hi\""));
        assert_eq!(read_node(&nb).map(|i| i.title).as_deref(), Some("Renamed"));
        assert!(set_title(&md_page, "  ").is_err());
    }

    #[test]
    fn delete_is_confined_and_counts() {
        let t = TempDir::new("ops10");
        let s = settings(&t);
        let r = root(&t);
        let nb = create_notebook(&r, "NB", None, &s).expect("nb");
        let p = create_node(&nb, "P", NodeKind::Page, None, &s).expect("p");
        create_node(&p, "Sub", NodeKind::Page, None, &s).expect("sub");
        assert!(delete_node(&r, &r).is_err(), "never the root");
        assert!(delete_node(&r, t.path()).is_err(), "never outside the root");
        assert!(delete_node(&r, &p.join("assets")).is_err(), "assets is not a node");
        assert_eq!(delete_node(&r, &p).expect("delete"), 2);
        assert!(!p.exists());
        assert!(nb.exists());
    }
}

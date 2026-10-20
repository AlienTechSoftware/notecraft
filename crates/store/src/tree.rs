//! Reading the notebook tree from folders.

use notecraft_formats::{frontmatter, html};
use notecraft_model::{NodeColor, NodeKind, PageFormat, kv};
use std::fs;
use std::path::{Path, PathBuf};

/// What a folder says about itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeInfo {
    pub kind: NodeKind,
    pub title: String,
    /// The colour the user gave this notebook, group or section. Pages have none.
    pub color: Option<NodeColor>,
    /// For a page, the format of its content file. For a notebook, section group or
    /// section, the default format it sets for new pages below it, if any.
    pub format: Option<PageFormat>,
    pub id: String,
    pub order: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub path: PathBuf,
    pub info: NodeInfo,
    pub children: Vec<Node>,
}

impl Node {
    /// This node and everything below it.
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Node::count).sum::<usize>()
    }

    /// Paths of every node that has children, at every depth.
    pub fn branches(&self, out: &mut Vec<PathBuf>) {
        if !self.children.is_empty() {
            out.push(self.path.clone());
            for c in &self.children {
                c.branches(out);
            }
        }
    }

    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(Node::depth).max().unwrap_or(0)
    }
}

/// A path for people: without the `\\?\` prefix Windows adds to resolved paths.
pub fn display_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

pub fn folder_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// What an untitled page is called in the tree, the page list and the window's title bar.
/// Never written into a page's own file as real content: a page with no title of its own
/// has nothing in its head that says so, and this is only ever the fallback shown for it.
/// A page carrying a `title` key is never untitled, however short or plain that title is.
pub const UNTITLED_PAGE: &str = "Untitled page";

/// The title written in a page's own file, with no fallback: `None` when there is no
/// title key at all, or the key is there but empty once trimmed. Shared by `read_node`,
/// which falls back to [`UNTITLED_PAGE`] for display, and by `page::load_page`, which
/// needs to tell a real title from the fallback so the title area can be drawn blank.
pub(crate) fn raw_title(format: PageFormat, text: &str) -> Option<String> {
    let title = match format {
        PageFormat::Html => html::title(text),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(text).0, "title"),
    };
    title.filter(|t| !t.trim().is_empty())
}

/// The content file of a page. If a folder has both `index.html` and `index.md`
/// (which `check` reports), HTML wins.
pub fn page_file(dir: &Path) -> Option<(PageFormat, PathBuf)> {
    PageFormat::ALL.iter().map(|f| (*f, dir.join(f.file_name()))).find(|(_, p)| p.is_file())
}

pub fn read_node(dir: &Path) -> Option<NodeInfo> {
    if let Some((format, file)) = page_file(dir) {
        let text = fs::read_to_string(&file).unwrap_or_default();
        let (id, order) = match format {
            PageFormat::Html => (html::meta(&text, "notecraft:id"), html::meta(&text, "notecraft:order").and_then(|v| v.parse().ok())),
            PageFormat::Cmgfm => {
                let (front, _) = frontmatter::split_front(&text);
                (frontmatter::value(&front, "id"), frontmatter::value(&front, "order").and_then(|v| v.parse().ok()))
            }
        };
        // A page with no title of its own is called "Untitled page" here, not by the
        // folder's slug: the owner asked for a page with nothing typed yet to read as
        // untitled, the same way it does in OneNote, rather than show the name the folder
        // happened to be given at creation.
        return Some(NodeInfo {
            kind: NodeKind::Page,
            title: raw_title(format, &text).unwrap_or_else(|| UNTITLED_PAGE.to_string()),
            color: None,
            format: Some(format),
            id: id.unwrap_or_default(),
            order: order.unwrap_or(0),
        });
    }
    let text = fs::read_to_string(dir.join("_node.toml")).ok()?;
    let map = kv::parse(&text);
    Some(NodeInfo {
        kind: map.get("kind").and_then(|k| NodeKind::parse(k)).unwrap_or(NodeKind::Section),
        title: map.get("title").cloned().unwrap_or_else(|| folder_name(dir)),
        color: map.get("color").and_then(|c| NodeColor::parse(c)),
        format: map.get("format").and_then(|f| PageFormat::parse(f).ok()),
        id: map.get("id").cloned().unwrap_or_default(),
        order: map.get("order").and_then(|v| v.parse().ok()).unwrap_or(0),
    })
}

/// Child folders in display order: by `order`, then by name. Skips `assets` and
/// folders whose names start with a dot.
pub fn child_dirs(dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<(u32, String, PathBuf)> = match fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .map(|p| (folder_name(&p), p))
            .filter(|(n, _)| n != "assets" && !n.starts_with('.'))
            .map(|(n, p)| (read_node(&p).map(|i| i.order).unwrap_or(0), n, p))
            .collect(),
        Err(_) => Vec::new(),
    };
    dirs.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    dirs.into_iter().map(|(_, _, p)| p).collect()
}

pub fn load_tree(dir: &Path) -> Option<Node> {
    let info = read_node(dir)?;
    let children = child_dirs(dir).iter().filter_map(|c| load_tree(c)).collect();
    Some(Node { path: dir.to_path_buf(), info, children })
}

/// Every notebook directly inside `root`.
pub fn load_notebooks(root: &Path) -> Vec<Node> {
    child_dirs(root).iter().filter_map(|d| load_tree(d)).filter(|n| n.info.kind == NodeKind::Notebook).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::settings::Settings;
    use crate::testing::TempDir;

    #[test]
    fn reads_a_page_and_a_container() {
        let t = TempDir::new("tree1");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(t.path(), "My Notebook", None, &s).expect("notebook");
        let page = ops::create_node(&nb, "First page", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("page");
        let info = read_node(&page).expect("page info");
        assert_eq!((info.kind, info.title.as_str(), info.format), (NodeKind::Page, "First page", Some(PageFormat::Cmgfm)));
        assert!(!info.id.is_empty());
        let nb_info = read_node(&nb).expect("nb info");
        assert_eq!((nb_info.kind, nb_info.format), (NodeKind::Notebook, Some(PageFormat::Html)));
    }

    #[test]
    fn display_paths_drop_the_windows_verbatim_prefix() {
        assert_eq!(display_path(Path::new(r"\\?\D:\notes\nb")), r"D:\notes\nb");
        assert_eq!(display_path(Path::new("/home/me/notes")), "/home/me/notes");
    }

    #[test]
    fn a_plain_folder_is_not_a_node() {
        let t = TempDir::new("tree2");
        assert!(read_node(t.path()).is_none());
        assert!(load_notebooks(t.path()).is_empty());
    }

    #[test]
    fn children_are_ordered_and_assets_are_hidden() {
        let t = TempDir::new("tree3");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(t.path(), "NB", None, &s).expect("nb");
        let a = ops::create_node(&nb, "Zed", NodeKind::Section, None, &s).expect("a");
        let b = ops::create_node(&nb, "Alpha", NodeKind::Section, None, &s).expect("b");
        let p = ops::create_node(&a, "Page", NodeKind::Page, None, &s).expect("p");
        assert_eq!(child_dirs(&nb), vec![a.clone(), b.clone()], "created order wins over name");
        assert!(p.join("assets").is_dir());
        assert!(child_dirs(&p).is_empty(), "assets is not a child");
    }

    #[test]
    fn counts_depth_and_branches() {
        let t = TempDir::new("tree4");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(t.path(), "NB", None, &s).expect("nb");
        let sec = ops::create_node(&nb, "S", NodeKind::Section, None, &s).expect("s");
        let mut p = ops::create_node(&sec, "L0", NodeKind::Page, None, &s).expect("p");
        for i in 1..=9 {
            p = ops::create_node(&p, &format!("L{i}"), NodeKind::Page, None, &s).expect("sub");
        }
        let tree = load_tree(&nb).expect("tree");
        assert_eq!(tree.count(), 12);
        assert_eq!(tree.depth(), 12, "notebook, section and ten pages: no depth limit");
        let mut branches = Vec::new();
        tree.branches(&mut branches);
        assert_eq!(branches.len(), 11);
    }
}

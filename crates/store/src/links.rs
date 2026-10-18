//! What a link in a page points at.
//!
//! A page links to things by relative path, the way its pictures are linked: `../other-page/`
//! is the page next door, `assets/notes.pdf` is a file of its own, `https://...` is the web.
//! This module turns such a target into something the window can act on, and keeps every
//! path inside the notebooks folder: a link cannot climb out of it, however many `..` it has.
//!
//! The resolution is lexical first (no file is touched while the path is normalised) and
//! looks at the disk only at the end, to tell a node from a file from nothing.

use crate::tree::read_node;
use std::path::{Component, Path, PathBuf};

/// Where a link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// A web address or another scheme the operating system handles (`https:`, `mailto:`).
    Web(String),
    /// A NoteCraft node: a notebook, section group, section or page folder.
    Node(PathBuf),
    /// A file, or a folder that is not a node (such as `assets/`).
    File(PathBuf),
    /// A path inside the notebooks folder with nothing there.
    Missing(PathBuf),
    /// A path that would leave the notebooks folder, or one that cannot be read.
    Outside,
}

/// Resolves `href`, as written in a page in `page_dir`, against the notebooks folder `root`.
/// Both folders are expected absolute and `page_dir` inside `root`; a relative link is taken
/// from the page's own folder, which is where its file lives.
pub fn resolve(root: &Path, page_dir: &Path, href: &str) -> Link {
    let href = href.trim();
    if href.is_empty() {
        return Link::Missing(page_dir.to_path_buf());
    }
    if href.get(..5).is_some_and(|s| s.eq_ignore_ascii_case("file:")) {
        // A `file:` address is a path in disguise and could name anything on the disk.
        return Link::Outside;
    }
    if let Some(url) = web(href) {
        return Link::Web(url);
    }
    // The part before `#` or `?`: a fragment or query has no meaning for a folder.
    let path_part = href.split(['#', '?']).next().unwrap_or("");
    if path_part.is_empty() {
        // `#heading` on the page itself.
        return Link::Node(page_dir.to_path_buf());
    }
    let decoded = percent_decode(path_part);
    if decoded.contains('\\') || decoded.contains('\0') {
        return Link::Outside;
    }
    let target = Path::new(&decoded);
    let Some(full) = normalise(if target.is_absolute() { target.to_path_buf() } else { page_dir.join(target) }) else {
        return Link::Outside;
    };
    if !full.starts_with(root) {
        return Link::Outside;
    }
    if full.is_dir() {
        if read_node(&full).is_some() { Link::Node(full) } else { Link::File(full) }
    } else if full.is_file() {
        Link::File(full)
    } else {
        Link::Missing(full)
    }
}

/// `scheme:rest` for a web or application scheme, `None` for a path. A single letter before
/// the colon is a Windows drive, not a scheme.
fn web(href: &str) -> Option<String> {
    let colon = href.find(':')?;
    let scheme = href.get(..colon)?;
    let ok = scheme.len() > 1
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if ok && !scheme.eq_ignore_ascii_case("file") { Some(href.to_string()) } else { None }
}

/// Folds `.` and `..` away without touching the disk. `None` when `..` would climb above the
/// root of the path, which no link inside a notebook needs.
fn normalise(path: PathBuf) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    let mut depth = 0usize;
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if depth == 0 {
                    return None;
                }
                out.pop();
                depth -= 1;
            }
            Component::Normal(part) => {
                out.push(part);
                depth += 1;
            }
        }
    }
    Some(out)
}

/// `%20` and friends turned back into their bytes; anything that is not valid UTF-8 after
/// that, or not a percent escape, is kept as written.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (bytes.get(i + 1).copied().and_then(hex), bytes.get(i + 2).copied().and_then(hex))
        {
            out.push(h * 16 + l);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    /// A notebooks folder with one notebook, a section, two pages and an attachment.
    fn fixture(label: &str) -> (TempDir, PathBuf, PathBuf, PathBuf) {
        let t = TempDir::new(label);
        let root = t.path().join("notebooks");
        let settings = crate::Settings::at(t.path().join("settings.toml"));
        let nb = crate::ops::create_notebook(&root, "Book", None, &settings).expect("notebook");
        let section = crate::ops::create_node(&nb, "Section", notecraft_model::NodeKind::Section, None, &settings).expect("section");
        let a = crate::ops::create_node(&section, "Page A", notecraft_model::NodeKind::Page, None, &settings).expect("a");
        let b = crate::ops::create_node(&section, "Page B", notecraft_model::NodeKind::Page, None, &settings).expect("b");
        std::fs::create_dir_all(a.join("assets")).expect("assets");
        std::fs::write(a.join("assets").join("my notes.pdf"), b"%PDF").expect("pdf");
        (t, root, a, b)
    }

    #[test]
    fn a_relative_link_finds_the_page_next_door_and_the_levels_above() {
        let (_t, root, a, b) = fixture("links-rel");
        assert_eq!(resolve(&root, &a, "../page-b/"), Link::Node(b.clone()));
        assert_eq!(resolve(&root, &a, "../page-b"), Link::Node(b.clone()));
        assert_eq!(resolve(&root, &a, "./../page-b/#heading"), Link::Node(b));
        assert_eq!(resolve(&root, &a, ".."), Link::Node(a.parent().expect("section").to_path_buf()));
        assert_eq!(resolve(&root, &a, "../../"), Link::Node(root.join("book")));
        assert_eq!(resolve(&root, &a, "#top"), Link::Node(a.clone()));
    }

    #[test]
    fn files_and_folders_that_are_not_nodes_are_files() {
        let (_t, root, a, _b) = fixture("links-file");
        assert_eq!(resolve(&root, &a, "assets/my%20notes.pdf"), Link::File(a.join("assets").join("my notes.pdf")));
        assert_eq!(resolve(&root, &a, "assets/"), Link::File(a.join("assets")));
        assert_eq!(resolve(&root, &a, "assets/none.pdf"), Link::Missing(a.join("assets").join("none.pdf")));
        assert_eq!(resolve(&root, &a, "../no-such-page/"), Link::Missing(a.parent().expect("s").join("no-such-page")));
    }

    #[test]
    fn nothing_climbs_out_of_the_notebooks_folder() {
        let (_t, root, a, _b) = fixture("links-out");
        assert_eq!(resolve(&root, &a, "../../../../"), Link::Outside, "above the notebooks folder");
        assert_eq!(resolve(&root, &a, "../../../../../../../../../../etc/passwd"), Link::Outside);
        assert_eq!(resolve(&root, &a, "..\\..\\x"), Link::Outside, "backslashes are not path separators in a link");
        assert_eq!(resolve(&root, &a, "/etc/passwd"), Link::Outside, "an absolute path outside");
        assert_eq!(resolve(&root, &a, "C:/Windows"), Link::Outside, "a drive is not a scheme");
        assert_eq!(resolve(&root, &a, "file:///etc/passwd"), Link::Outside, "file: is a path in disguise");
        // An absolute path inside the folder is allowed, written with forward slashes as every
        // link is: it is the page's own folder. With backslashes it is not a link at all.
        let forward = a.to_string_lossy().replace('\\', "/");
        assert_eq!(resolve(&root, &a, &forward), Link::Node(a.clone()));
    }

    #[test]
    fn web_and_mail_addresses_are_handed_to_the_system() {
        let (_t, root, a, _b) = fixture("links-web");
        assert_eq!(resolve(&root, &a, "https://example.test/x?y#z"), Link::Web("https://example.test/x?y#z".into()));
        assert_eq!(resolve(&root, &a, " mailto:someone@example.test "), Link::Web("mailto:someone@example.test".into()));
        assert_eq!(resolve(&root, &a, ""), Link::Missing(a));
    }

    #[test]
    fn percent_escapes_decode_and_junk_is_kept() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("%D9%85"), "\u{645}");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%FF"), "%FF", "not UTF-8: kept as written");
    }
}

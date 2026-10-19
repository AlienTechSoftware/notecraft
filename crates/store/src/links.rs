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

/// The scheme a link to a page of one's own carries: `notecraft:/notebook/section/page`,
/// the path from the notebooks folder.
///
/// OneNote has `onenote:` for the same job. A relative path cannot do it, because what it
/// is relative to is the page the link is pasted into, which nobody knows when the link is
/// copied; this names the page from the top and reads the same wherever it is pasted.
pub const SCHEME: &str = "notecraft:";

/// A link to this page, to paste into another one. `None` when the folder is not under any
/// of the open notebooks.
pub fn page_uri(roots: &[PathBuf], dir: &Path) -> Option<String> {
    let full = normalise(dir.to_path_buf())?;
    let root = roots.iter().find(|r| full.starts_with(r))?;
    let rest = full.strip_prefix(root).ok()?;
    let path = rest.components().filter_map(|c| c.as_os_str().to_str()).collect::<Vec<_>>().join("/");
    Some(format!("{SCHEME}/{path}"))
}

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

/// Resolves `href` against the notebooks folder alone. Most callers want [`resolve_in`],
/// which also knows about notebooks opened from elsewhere.
pub fn resolve(root: &Path, page_dir: &Path, href: &str) -> Link {
    resolve_in(std::slice::from_ref(&root.to_path_buf()), page_dir, href)
}

/// Resolves `href`, as written in a page in `page_dir`, against every folder a notebook is
/// open from: the notebooks folder, and any notebook opened from somewhere else.
///
/// A link may not leave all of them, however many `..` it has. It took more than one
/// folder the day notebooks could be opened from outside the notebooks folder: until then
/// `resolve` knew one root, so every link inside such a notebook was judged to point out
/// of the notebooks folder and was refused, which is what the owner saw.
///
/// `page_dir` is expected absolute and inside one of `roots`; a relative link is taken
/// from the page's own folder, which is where its file lives.
pub fn resolve_in(roots: &[PathBuf], page_dir: &Path, href: &str) -> Link {
    let href = href.trim();
    if href.is_empty() {
        return Link::Missing(page_dir.to_path_buf());
    }
    // A link of our own: the path after the scheme is read from each open notebook's
    // folder, so the same link works from any page and in any notebook that holds it.
    if href.len() >= SCHEME.len() && href[..SCHEME.len()].eq_ignore_ascii_case(SCHEME) {
        let rest = href[SCHEME.len()..].trim_start_matches('/');
        let rest = rest.split(['#', '?']).next().unwrap_or("");
        let decoded = percent_decode(rest);
        if decoded.is_empty() || decoded.contains('\\') || decoded.contains('\0') {
            return Link::Outside;
        }
        let mut inside_somewhere = None;
        for root in roots {
            let Some(full) = normalise(root.join(&decoded)) else { continue };
            if !full.starts_with(root) {
                // A link that climbs out of this notebook; it may still be in another.
                continue;
            }
            if full.is_dir() {
                return if read_node(&full).is_some() { Link::Node(full) } else { Link::File(full) };
            }
            if full.is_file() {
                return Link::File(full);
            }
            inside_somewhere.get_or_insert(full);
        }
        // Nothing of that name: missing when the path at least stayed inside a notebook,
        // out when it climbed above every one of them.
        return match inside_somewhere {
            Some(full) => Link::Missing(full),
            None => Link::Outside,
        };
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
    // Only Windows reads `C:/x` as absolute. Elsewhere it would be a folder called `C:` inside the
    // page, and one notebook, synced between machines, would mean two things. A drive path is
    // outside unless this system really treats it as an absolute path (and then the root check
    // below decides).
    if is_drive(&decoded) && !target.is_absolute() {
        return Link::Outside;
    }
    let Some(full) = normalise(if target.is_absolute() { target.to_path_buf() } else { page_dir.join(target) }) else {
        return Link::Outside;
    };
    // Inside any one of the open notebooks is inside. A link that leaves all of them is
    // out, which is the rule that keeps a notebook from reaching into the rest of a disk.
    if !roots.iter().any(|r| full.starts_with(r)) {
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

/// `C:/x` or `C:x`: starts with a Windows drive letter.
fn is_drive(path: &str) -> bool {
    matches!(path.as_bytes(), [d, b':', ..] if d.is_ascii_alphabetic())
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

    /// A notebook opened from outside the notebooks folder: its pages link to each other
    /// exactly as they would inside it. Judging such a page by the notebooks folder alone
    /// made every link in it dead, with "that link points outside the notebooks folder",
    /// which is what the owner saw on the Welcome notebook.
    #[test]
    fn a_notebook_opened_from_elsewhere_can_link_inside_itself() {
        let t = TempDir::new("links-elsewhere");
        let root = t.path().join("notebooks");
        let away = t.path().join("somewhere-else");
        let settings = crate::Settings::at(t.path().join("settings.toml"));
        std::fs::create_dir_all(&root).expect("root");
        let nb = crate::ops::create_notebook(&away, "Away", None, &settings).expect("notebook");
        let section = crate::ops::create_node(&nb, "Section", notecraft_model::NodeKind::Section, None, &settings).expect("section");
        let a = crate::ops::create_node(&section, "Page A", notecraft_model::NodeKind::Page, None, &settings).expect("a");
        let b = crate::ops::create_node(&section, "Page B", notecraft_model::NodeKind::Page, None, &settings).expect("b");

        // Knowing only the notebooks folder, every link in it is refused.
        assert_eq!(resolve(&root, &a, "../page-b/"), Link::Outside, "the fault, as it was");

        // Knowing the notebook is open too, the link is the page next door.
        let roots = vec![root.clone(), nb.clone()];
        assert_eq!(resolve_in(&roots, &a, "../page-b/"), Link::Node(b));
        assert_eq!(resolve_in(&roots, &a, "../../"), Link::Node(nb.clone()));

        // A link that leaves every open notebook is still out, which is the rule that
        // keeps a notebook from reaching into the rest of the disk.
        assert_eq!(resolve_in(&roots, &a, "../../../"), Link::Outside);
        assert_eq!(resolve_in(&roots, &a, "../../../../etc/passwd"), Link::Outside);
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
    fn a_drive_is_recognised_on_every_system() {
        for drive in ["C:/Windows", "c:/x", "Z:", "d:file"] {
            assert!(is_drive(drive), "{drive}");
        }
        for not in ["", "C", "page/C:/x", "1:/x", "ab:/x", "../C:/x"] {
            assert!(!is_drive(not), "{not}");
        }
    }

    #[test]
    fn a_link_of_our_own_names_a_page_from_the_notebooks_folder() {
        let (_t, root, a, b) = fixture("links-scheme");
        let roots = vec![root.clone()];
        let uri = page_uri(&roots, &b).expect("a link to the page");
        assert!(uri.starts_with("notecraft:/"), "{uri}");
        assert!(!uri.contains('\\'), "written with forward slashes: {uri}");

        // It leads to that page from anywhere, not only from the page next door.
        assert_eq!(resolve_in(&roots, &a, &uri), Link::Node(b.clone()));
        assert_eq!(resolve_in(&roots, &b, &uri), Link::Node(b.clone()), "and from the page itself");

        // Upper case scheme, and a fragment on the end, are the same link.
        let shouted = uri.replacen("notecraft:", "NoteCraft:", 1);
        assert_eq!(resolve_in(&roots, &a, &shouted), Link::Node(b.clone()));
        assert_eq!(resolve_in(&roots, &a, &format!("{uri}#heading")), Link::Node(b.clone()));

        // A link of this kind cannot climb out of the notebooks folder.
        assert_eq!(resolve_in(&roots, &a, "notecraft:/../secret"), Link::Outside);
        assert_eq!(resolve_in(&roots, &a, "notecraft:/"), Link::Outside);
        assert!(matches!(resolve_in(&roots, &a, "notecraft:/nothing/here"), Link::Missing(_)));
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

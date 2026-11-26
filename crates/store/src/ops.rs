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
pub(crate) fn unique_child(parent: &Path, base: &str) -> PathBuf {
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

/// Turns an existing, empty folder into a notebook in place. The folder is left alone if it
/// already contains anything, so opening an ordinary folder cannot absorb its files.
pub fn initialize_notebook(dir: &Path, title: &str, explicit: Option<PageFormat>, settings: &Settings) -> Result<PathBuf> {
    let title = clean_title(title)?;
    if !dir.is_dir() {
        return Err(StoreError::NotFound(dir.display().to_string()));
    }
    let mut entries = fs::read_dir(dir).map_err(|e| StoreError::io("read", dir, e))?;
    if entries.next().is_some() {
        return Err(StoreError::invalid(format!("{} is not an empty folder", dir.display())));
    }
    let (format, _) = effective_format(None, explicit, settings);
    let file = dir.join("_node.toml");
    let node = node_file(NodeKind::Notebook, &title, &new_id(), None, Some(format));
    let mut output = fs::OpenOptions::new().write(true).create_new(true).open(&file).map_err(|e| StoreError::io("create", &file, e))?;
    use std::io::Write;
    if let Err(e) = output.write_all(node.as_bytes()) {
        drop(output);
        let _ = fs::remove_file(&file);
        return Err(StoreError::io("write", &file, e));
    }
    Ok(dir.to_path_buf())
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
pub fn create_untitled_page(parent: &Path, explicit: Option<PageFormat>, want_order: Option<u32>, settings: &Settings) -> Result<PathBuf> {
    create_node_inner(parent, tree::UNTITLED_PAGE, NodeKind::Page, explicit, settings, want_order, None)
}

/// Demotes a page beneath its preceding page sibling. The page folder and everything in it
/// move together; the remaining siblings are compacted without changing their order.
pub fn make_subpage(path: &Path) -> Result<PathBuf> {
    let page = canonical_page(path)?;
    let parent = page.parent().ok_or_else(|| StoreError::invalid("a page has no parent"))?;
    let siblings = node_children(parent);
    let index = siblings.iter().position(|sibling| sibling == &page).ok_or_else(|| StoreError::invalid("the page is not in its parent's tree"))?;
    let previous = index
        .checked_sub(1)
        .and_then(|previous| siblings.get(previous))
        .ok_or_else(|| StoreError::invalid("there is no preceding page to make this a subpage of"))?;
    if !read_node(previous).is_some_and(|info| info.kind == NodeKind::Page) {
        return Err(StoreError::invalid("the preceding sibling is not a page"));
    }
    let target_parent = previous.clone();
    let mut target_order = node_children(&target_parent);
    target_order.push(page.clone());
    let target_name = unique_child(&target_parent, &tree::folder_name(&page));
    fs::rename(&page, &target_name).map_err(|e| StoreError::io("move page", &target_name, e))?;
    compact_orders(parent)?;
    compact_orders_in_order(&target_order, &page, &target_name)?;
    Ok(target_name)
}

/// Promotes a subpage to its grandparent, immediately after its former parent.
pub fn promote_subpage(path: &Path) -> Result<PathBuf> {
    let page = canonical_page(path)?;
    let parent = page.parent().ok_or_else(|| StoreError::invalid("a page has no parent"))?.to_path_buf();
    if !read_node(&parent).is_some_and(|info| info.kind == NodeKind::Page) {
        return Err(StoreError::invalid("this page is not a subpage"));
    }
    let target_parent = parent.parent().ok_or_else(|| StoreError::invalid("the parent page has no parent"))?.to_path_buf();
    if !read_node(&target_parent).is_some_and(|info| info.kind.may_contain().contains(&NodeKind::Page)) {
        return Err(StoreError::invalid("the subpage cannot be promoted to this parent"));
    }
    let siblings = node_children(&target_parent);
    let parent_index =
        siblings.iter().position(|sibling| sibling == &parent).ok_or_else(|| StoreError::invalid("the parent page is not in its parent's tree"))?;
    let mut desired = siblings;
    desired.insert(parent_index + 1, page.clone());
    let target_name = unique_child(&target_parent, &tree::folder_name(&page));
    fs::rename(&page, &target_name).map_err(|e| StoreError::io("move page", &target_name, e))?;
    compact_orders(&parent)?;
    compact_orders_in_order(&desired, &page, &target_name)?;
    Ok(target_name)
}

/// Whether a node of `kind` may go into `dest`, as Move or Copy puts it (issue #167): a page
/// into a section (or under a page, as a subpage), a section into a notebook or a section group,
/// a section group into a notebook or another group. Never into itself or anything inside it.
fn check_destination(path: &Path, dest: &Path) -> Result<NodeKind> {
    let kind = read_node(path).map(|n| n.kind).ok_or_else(|| StoreError::invalid("only a page, section or section group can be moved or copied"))?;
    if kind == NodeKind::Notebook {
        return Err(StoreError::invalid("a notebook cannot be moved into another one"));
    }
    let into =
        read_node(dest).map(|n| n.kind).ok_or_else(|| StoreError::invalid("the destination is not a notebook, section group, section or page"))?;
    if !into.may_contain().contains(&kind) {
        return Err(StoreError::invalid(format!("a {} cannot hold a {}", into.label().to_lowercase(), kind.label().to_lowercase())));
    }
    if dest.starts_with(path) {
        return Err(StoreError::invalid("a page or section cannot go inside itself"));
    }
    Ok(kind)
}

/// Moves a page (with its subpages, files and ink), a section or a section group into another
/// parent, last among the nodes there, as OneNote's Move does (issue #167). A folder of the same
/// name there is not overwritten: the moved one gets the next free name. Returns its new place.
pub fn move_node(path: &Path, dest: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))?;
    let dest = fs::canonicalize(dest).map_err(|e| StoreError::io("open", dest, e))?;
    check_destination(&path, &dest)?;
    let from = path.parent().map(Path::to_path_buf);
    if from.as_deref() == Some(dest.as_path()) {
        return Ok(path);
    }
    let order = next_order(&dest);
    let target = unique_child(&dest, &tree::folder_name(&path));
    if fs::rename(&path, &target).is_err() {
        // Another drive, for a notebook opened from elsewhere: copied over, then the old one
        // taken away once the copy is whole.
        copy_tree(&path, &target).map_err(|e| StoreError::io("copy", &target, e))?;
        remove_tree(&path).map_err(|e| StoreError::io("delete", &path, e))?;
    }
    set_node_order(&target, order)?;
    if let Some(from) = from {
        compact_orders(&from)?;
    }
    Ok(target)
}

/// Puts a page (with its subpages, files and ink), a section or a section group into `dest`, just
/// before `before` (one of `dest`'s own children), or last there when `before` is `None`: where a
/// drag in the navigation pane drops it. A notebook is placed among the notebooks beside it, with
/// `dest` its own folder's parent. Returns where it now is.
///
/// Only the node placed is written in the usual case: it takes an order between its new
/// neighbours'. Every node beside it is renumbered only when there is no number left between
/// them. On a synced folder each write waits on the mount, and renumbering a section of twenty
/// pages for every drag took that many waits.
pub fn place_node(path: &Path, dest: &Path, before: Option<&Path>) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))?;
    let dest = fs::canonicalize(dest).map_err(|e| StoreError::io("open", dest, e))?;
    let before = before.map(|b| fs::canonicalize(b).map_err(|e| StoreError::io("open", b, e))).transpose()?;
    let kind =
        read_node(&path).map(|n| n.kind).ok_or_else(|| StoreError::invalid("only a page, section, section group or notebook can be placed"))?;
    if kind == NodeKind::Notebook {
        if path.parent() != Some(dest.as_path()) {
            return Err(StoreError::invalid("a notebook is placed among the notebooks beside it"));
        }
    } else {
        check_destination(&path, &dest)?;
    }
    if let Some(before) = &before {
        if before.parent() != Some(dest.as_path()) {
            return Err(StoreError::invalid("a node is placed before one of the nodes in the same place"));
        }
        if *before == path {
            return Ok(path);
        }
    }
    // The nodes it goes among, in their order, without it.
    let mut siblings: Vec<(PathBuf, u32)> =
        node_children(&dest).into_iter().filter(|c| *c != path).map(|c| (c.clone(), read_node(&c).map_or(0, |n| n.order))).collect();
    let at = before.as_ref().map_or(siblings.len(), |b| siblings.iter().position(|(c, _)| c == b).unwrap_or(siblings.len()));
    let order = match order_between(&siblings, at) {
        Some(order) => order,
        None => {
            for (index, (child, order)) in siblings.iter_mut().enumerate() {
                *order = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1).saturating_mul(10);
                set_node_order(child, *order)?;
            }
            order_between(&siblings, at).unwrap_or(u32::MAX)
        }
    };
    let target = if path.parent() == Some(dest.as_path()) {
        path
    } else {
        let target = unique_child(&dest, &tree::folder_name(&path));
        if fs::rename(&path, &target).is_err() {
            // Another drive, for a notebook opened from elsewhere: copied, then the old one taken
            // away once the copy is whole.
            copy_tree(&path, &target).map_err(|e| StoreError::io("copy", &target, e))?;
            remove_tree(&path).map_err(|e| StoreError::io("delete", &path, e))?;
        }
        target
    };
    set_node_order(&target, order)?;
    Ok(target)
}

/// An order for a node put at `at` among `siblings` (their orders, in order): between the one
/// before and the one after, or ten past the last. `None` when there is no number between them.
fn order_between(siblings: &[(PathBuf, u32)], at: usize) -> Option<u32> {
    let previous = at.checked_sub(1).and_then(|i| siblings.get(i)).map_or(0, |(_, order)| *order);
    match siblings.get(at) {
        None => Some(previous.saturating_add(10)),
        Some((_, next)) if *next > previous.saturating_add(1) => Some(previous + (next - previous) / 2),
        Some(_) => None,
    }
}

/// What [`merge_section`] did: where each page went, in the order they now stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// Each page moved, as (where it was, where it is now).
    pub moved: Vec<(PathBuf, PathBuf)>,
}

/// Merge into Another Section (issue #517): every page of the section `source`, with its
/// subpages, files and ink, goes into the section `dest`, after the pages already there and in
/// the order it had; then `source`, empty, is taken away. A copy of `source` as it was is put in
/// its notebook's recycle bin first, so what the merge did can be undone from there. The pages
/// keep their ids, so a link to one still finds it where it went; a page whose folder name is
/// taken in `dest` gets the next free name, and its title is not changed.
///
/// Each page's move is noted the moment it is made, before anything else is written, so should
/// any step fail every page moved goes back where it was, in its place, across drives as well,
/// and `source` is left as it was; an error says so if one could not be put back. A page that
/// is a link to a folder elsewhere is not moved, and the merge is refused before anything is
/// done. Pages that arrive in `source` while it runs (from a sync tool) are moved too, and
/// `source` is only taken away once no page is left in it.
pub fn merge_section(source: &Path, dest: &Path) -> Result<Merged> {
    let source = fs::canonicalize(source).map_err(|e| StoreError::io("open", source, e))?;
    let dest = fs::canonicalize(dest).map_err(|e| StoreError::io("open", dest, e))?;
    let is_section = |p: &Path| read_node(p).is_some_and(|n| n.kind == NodeKind::Section);
    if !is_section(&source) || !is_section(&dest) {
        return Err(StoreError::invalid("only a section can be merged, and only into another section"));
    }
    if source == dest {
        return Err(StoreError::invalid("a section cannot be merged into itself"));
    }
    let notebook = source
        .ancestors()
        .find(|a| read_node(a).is_some_and(|n| n.kind == NodeKind::Notebook))
        .map(Path::to_path_buf)
        .ok_or_else(|| StoreError::invalid("the section is not in a notebook"))?;
    let listed = merge_pages(&source)?;
    let parent = source.parent().ok_or_else(|| StoreError::invalid("the section has no parent"))?;
    // The section as it was, kept where it can be brought back from.
    let kept = copy_node(&source, parent)?;
    if let Err(e) = crate::recycle::delete(&notebook, &kept) {
        let _ = remove_tree(&kept);
        return Err(e);
    }
    let mut moved: Vec<(PathBuf, PathBuf, u32)> = Vec::new();
    // Old folders a cross-drive move could not take away whole: their pages are moved, and what
    // is left of them goes with the section.
    let mut left_behind: Vec<PathBuf> = Vec::new();
    let mut order = next_order(&dest);
    let mut pages = listed;
    // A few rounds at most: a page a sync tool brings in while this runs is moved as well.
    for _ in 0..4 {
        for (page, was) in &pages {
            // A free name, tried again should something else take it first.
            let mut placed = None;
            for _ in 0..3 {
                let target = unique_child(&dest, &tree::folder_name(page));
                match relocate(page, &target) {
                    Ok(whole) => {
                        placed = Some(Ok((target, whole)));
                        break;
                    }
                    // Only a name something else took first is tried again under another.
                    Err(MoveError::Taken(e)) => placed = Some(Err(e)),
                    Err(MoveError::Failed(e)) => {
                        placed = Some(Err(e));
                        break;
                    }
                }
            }
            let target = match placed {
                Some(Ok((target, whole))) => {
                    if !whole {
                        left_behind.push(page.clone());
                    }
                    target
                }
                Some(Err(e)) => return Err(undo_merge(&source, &dest, &moved, e)),
                None => return Err(undo_merge(&source, &dest, &moved, StoreError::invalid("no free name for a page"))),
            };
            moved.push((page.clone(), target.clone(), *was));
            if let Err(e) = set_node_order(&target, order) {
                return Err(undo_merge(&source, &dest, &moved, e));
            }
            order = order.saturating_add(10);
        }
        pages = match merge_pages(&source) {
            Ok(more) => more.into_iter().filter(|(p, _)| !left_behind.contains(p)).collect(),
            Err(e) => return Err(undo_merge(&source, &dest, &moved, e)),
        };
        if pages.is_empty() {
            break;
        }
    }
    if node_children(&source).iter().any(|c| !left_behind.contains(c)) {
        let e = StoreError::invalid("pages kept arriving in the section while it was merged; try again once the sync is done");
        return Err(undo_merge(&source, &dest, &moved, e));
    }
    if let Err(e) = remove_tree(&source) {
        // Every page is in its new place: the empty section staying behind is all that went wrong.
        return Err(StoreError::io("delete the emptied section", &source, e));
    }
    Ok(Merged { moved: moved.into_iter().map(|(from, to, _)| (from, to)).collect() })
}

/// The pages of a section to merge, in order, with the order each had. Refused if one is a link
/// to a folder elsewhere: moving it would move a folder from outside the section.
fn merge_pages(section: &Path) -> Result<Vec<(PathBuf, u32)>> {
    let mut pages = Vec::new();
    for child in node_children(section) {
        let linked = fs::symlink_metadata(&child).map(|m| m.file_type().is_symlink()).unwrap_or(true);
        let inside = fs::canonicalize(&child).ok().is_some_and(|c| c.parent() == Some(section));
        if linked || !inside {
            return Err(StoreError::invalid(format!("{} is a link to a folder elsewhere; the section was not merged", child.display())));
        }
        // A link inside a page would not survive a move across drives, nor be in the copy kept.
        if let Some(odd) = unsupported_entry(&child) {
            return Err(StoreError::invalid(format!("{} is a link or other special file; the section was not merged", odd.display())));
        }
        if let Some(info) = read_node(&child).filter(|n| n.kind == NodeKind::Page) {
            pages.push((child, info.order));
        }
    }
    Ok(pages)
}

/// Why [`relocate`] did not move a folder.
enum MoveError {
    /// Something else had the name first: nothing was written, and another name may be tried.
    Taken(StoreError),
    /// The move failed; the error says whether anything was left half made, and where.
    Failed(StoreError),
}

impl MoveError {
    fn into_error(self) -> StoreError {
        match self {
            MoveError::Taken(e) | MoveError::Failed(e) => e,
        }
    }
}

/// Moves a folder to `to`: renamed where it can be, else copied whole and the old one taken
/// away once the copy is complete. `to` must not be there yet: it is made here, so a folder
/// that something else put there in the meantime is never written into or taken away. A copy
/// left incomplete is taken away again, being this move's own; should that fail too, the error
/// names it. Once the copy is complete it is kept whatever happens: if the old folder then
/// cannot be taken away whole, `Ok(false)` says some of it is left behind, the page itself
/// being whole in its new place.
fn relocate(from: &Path, to: &Path) -> std::result::Result<bool, MoveError> {
    let taken = || MoveError::Taken(StoreError::io("move", to, std::io::Error::from(std::io::ErrorKind::AlreadyExists)));
    if to.exists() {
        return Err(taken());
    }
    if fs::rename(from, to).is_ok() {
        return Ok(true);
    }
    match fs::create_dir(to) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Err(taken()),
        Err(e) => return Err(MoveError::Failed(StoreError::io("create", to, e))),
    }
    let copied = (|| -> std::io::Result<()> {
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let target = to.join(entry.file_name());
            if kind.is_dir() {
                copy_tree(&entry.path(), &target)?;
            } else if kind.is_file() {
                fs::copy(entry.path(), &target)?;
            }
        }
        Ok(())
    })();
    if let Err(e) = copied {
        if let Err(left) = remove_tree(to) {
            return Err(MoveError::Failed(StoreError::invalid(format!(
                "copying {} failed ({e}), and the part copied to {} could not be taken away ({left})",
                from.display(),
                to.display()
            ))));
        }
        return Err(MoveError::Failed(StoreError::io("copy", to, e)));
    }
    if let Err(e) = remove_tree(from) {
        log::warn!("moved a page, but its old folder could not be taken away whole: from={from:?} to={to:?} error={e}");
        return Ok(false);
    }
    Ok(true)
}

/// The first thing in `dir`, at any depth, that is neither a file nor a folder (a link, say),
/// which a copy across drives would not carry.
fn unsupported_entry(dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { return Some(entry.path()) };
        if kind.is_dir() {
            if let Some(found) = unsupported_entry(&entry.path()) {
                return Some(found);
            }
        } else if !kind.is_file() {
            return Some(entry.path());
        }
    }
    None
}

/// Puts back the pages a merge had moved, last first, each in its old place with its old order,
/// and says what went wrong, and which pages could not be put back if any.
fn undo_merge(source: &Path, dest: &Path, moved: &[(PathBuf, PathBuf, u32)], cause: StoreError) -> StoreError {
    let mut stuck = Vec::new();
    for (from, to, was) in moved.iter().rev() {
        let back = if from.exists() { unique_child(source, &tree::folder_name(from)) } else { from.clone() };
        match relocate(to, &back).map_err(MoveError::into_error) {
            Ok(_) => {
                let _ = set_node_order(&back, *was);
            }
            Err(_) => stuck.push(to.display().to_string()),
        }
    }
    let _ = compact_orders(dest);
    if stuck.is_empty() {
        cause
    } else {
        StoreError::invalid(format!("{cause}; these pages could not be put back and are still in the other section: {}", stuck.join(", ")))
    }
}

/// Copies a page (with its subpages, files and ink), a section or a section group into another
/// parent, last among the nodes there, as OneNote's Copy does (issue #167). Every node in the
/// copy gets an id of its own, so a link to the original still finds the original.
pub fn copy_node(path: &Path, dest: &Path) -> Result<PathBuf> {
    let path = fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))?;
    let dest = fs::canonicalize(dest).map_err(|e| StoreError::io("open", dest, e))?;
    check_destination(&path, &dest)?;
    let order = next_order(&dest);
    let target = unique_child(&dest, &tree::folder_name(&path));
    if let Err(e) = copy_tree(&path, &target) {
        let _ = fs::remove_dir_all(&target);
        return Err(StoreError::io("copy", &target, e));
    }
    // A copy half made is taken away again, so no second section with the same ids is left.
    if let Err(e) = renew_ids(&target).and_then(|()| set_node_order(&target, order)) {
        let _ = remove_tree(&target);
        return Err(e);
    }
    Ok(target)
}

/// File > Save a Copy (issue #301): copies a page (with its subpages, files and ink), a section,
/// a section group or a whole notebook into `folder`, anywhere on disk, as content of its own.
/// The copy leaves out NoteCraft's own records kept beside the content (a page's saved
/// versions, a notebook's recycle bin and templates: everything under `.notecraft`), and every
/// node in it gets an id of its own, so opening the copy beside the original confuses neither.
/// The original is not touched. Returns where the copy is.
pub fn save_copy(node: &Path, folder: &Path) -> Result<PathBuf> {
    let node = fs::canonicalize(node).map_err(|e| StoreError::io("open", node, e))?;
    if read_node(&node).is_none() {
        return Err(StoreError::invalid("only a page, section, section group or notebook can be copied"));
    }
    let folder = fs::canonicalize(folder).map_err(|e| StoreError::io("open", folder, e))?;
    if !folder.is_dir() {
        return Err(StoreError::invalid("the copy goes into a folder"));
    }
    if folder.starts_with(&node) {
        return Err(StoreError::invalid("a copy cannot go inside what is being copied"));
    }
    let target = unique_child(&folder, &tree::folder_name(&node));
    if let Err(e) = copy_content(&node, &target) {
        let _ = fs::remove_dir_all(&target);
        return Err(StoreError::io("copy", &target, e));
    }
    renew_ids(&target)?;
    // Writing the new ids kept a copy of each page as it was a moment ago: that is history
    // too, and the copy starts without any.
    drop_records(&target).map_err(|e| StoreError::io("copy", &target, e))?;
    Ok(target)
}

/// Removes every `.notecraft` folder under `dir`.
fn drop_records(dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if entry.file_name() == ".notecraft" {
                fs::remove_dir_all(entry.path())?;
            } else {
                drop_records(&entry.path())?;
            }
        }
    }
    Ok(())
}

/// Copies a folder and everything in it except NoteCraft's `.notecraft` records.
fn copy_content(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            if entry.file_name() == ".notecraft" {
                continue;
            }
            copy_content(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// The order that puts a node last among `parent`'s children.
fn next_order(parent: &Path) -> u32 {
    node_children(parent).iter().filter_map(|c| read_node(c).map(|n| n.order)).max().unwrap_or(0).saturating_add(10)
}

/// Copies a folder and everything in it.
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Gives the node at `dir` and every node inside it a new id.
fn renew_ids(dir: &Path) -> Result<()> {
    if let Some((format, file)) = page_file(dir) {
        let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
        let id = new_id();
        let updated = match format {
            PageFormat::Html => html::set_meta(&text, "notecraft:id", &id),
            PageFormat::Cmgfm => {
                let (front, body) = frontmatter::split_front(&text);
                format!("{}{body}", frontmatter::set_value(&front, "id", &id))
            }
        };
        write_atomic(&file, &updated)?;
    } else if dir.join("_node.toml").is_file() {
        let file = dir.join("_node.toml");
        let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
        let line = format!("id = {}", kv::quote(&new_id()));
        let updated = if text.lines().any(|l| l.trim_start().starts_with("id =")) {
            kv::replace_line(&text, "id =", &line)
        } else {
            format!("{}\n{line}\n", text.trim_end())
        };
        write_atomic(&file, &updated)?;
    }
    for child in node_children(dir) {
        renew_ids(&child)?;
    }
    Ok(())
}

/// Moves a page one place among its siblings. Crossing a non-page sibling is refused so a
/// page cannot jump over a section or group.
pub fn move_page(path: &Path, up: bool) -> Result<PathBuf> {
    let page = canonical_page(path)?;
    let parent = page.parent().ok_or_else(|| StoreError::invalid("a page has no parent"))?;
    let mut siblings = node_children(parent);
    let index = siblings.iter().position(|sibling| sibling == &page).ok_or_else(|| StoreError::invalid("the page is not in its parent's tree"))?;
    let adjacent = if up { index.checked_sub(1) } else { index.checked_add(1).filter(|next| *next < siblings.len()) }
        .ok_or_else(|| StoreError::invalid("the page is already at that end of the list"))?;
    if !read_node(&siblings[adjacent]).is_some_and(|info| info.kind == NodeKind::Page) {
        return Err(StoreError::invalid("a page cannot be moved past a non-page sibling"));
    }
    siblings.swap(index, adjacent);
    for (position, sibling) in siblings.iter().enumerate() {
        set_node_order(sibling, u32::try_from(position).unwrap_or(u32::MAX).saturating_add(1).saturating_mul(10))?;
    }
    Ok(page)
}

fn canonical_page(path: &Path) -> Result<PathBuf> {
    let page = fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))?;
    if !read_node(&page).is_some_and(|info| info.kind == NodeKind::Page) {
        return Err(StoreError::invalid("the selected node is not a page"));
    }
    Ok(page)
}

/// Renumbers the pages of `parent` ten apart, keeping their order, so there is room to put a page between any two.
pub fn renumber_children(parent: &Path) -> Result<()> {
    compact_orders(parent)
}

fn compact_orders(parent: &Path) -> Result<()> {
    let children = node_children(parent);
    for (index, child) in children.iter().enumerate() {
        set_node_order(child, u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1).saturating_mul(10))?;
    }
    Ok(())
}

fn node_children(parent: &Path) -> Vec<PathBuf> {
    child_dirs(parent).into_iter().filter(|child| read_node(child).is_some()).collect()
}

fn compact_orders_in_order(before_move: &[PathBuf], moved_before: &Path, moved_after: &Path) -> Result<()> {
    for (index, child) in before_move.iter().enumerate() {
        let path = if child == moved_before { moved_after } else { child };
        set_node_order(path, u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1).saturating_mul(10))?;
    }
    Ok(())
}

fn set_node_order(path: &Path, order: u32) -> Result<()> {
    if let Some((format, file)) = page_file(path) {
        let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
        let updated = match format {
            PageFormat::Html => html::set_meta(&text, "notecraft:order", &order.to_string()),
            PageFormat::Cmgfm => {
                let (front, body) = frontmatter::split_front(&text);
                format!("{}{body}", frontmatter::set_value(&front, "order", &order.to_string()))
            }
        };
        return write_atomic(&file, &updated);
    }
    let file = path.join("_node.toml");
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let prefix = "order =";
    let updated = if text.lines().any(|line| line.trim_start().starts_with(prefix)) {
        kv::replace_line(&text, prefix, &format!("order = {order}"))
    } else {
        format!("{}{newline}order = {order}{newline}", text.trim_end(), newline = if text.contains("\r\n") { "\r\n" } else { "\n" })
    };
    write_atomic(&file, &updated)
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
    remove_tree(&path).map_err(|e| StoreError::io("delete", &path, e))?;
    Ok(count)
}

/// Deletes a folder and everything in it. `fs::remove_dir_all` fails on a VirtualBox shared
/// folder with OS error 234 ("More data is available"), because the share does not support
/// the directory query it uses; the folder is then emptied entry by entry instead.
pub fn remove_tree(path: &Path) -> std::io::Result<()> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(()),
        Err(first) if path.exists() => {
            log::warn!("remove_dir_all failed, deleting entry by entry: path={path:?} error={first}");
            remove_entries(path)
        }
        Err(first) => Err(first),
    }
}

fn remove_entries(dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            remove_entries(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    fs::remove_dir(dir)
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

/// The template a section's new pages start from (issue #519), as `page_templates` names it
/// (`builtin:<id>` or `saved:<folder>`), if the section has one.
pub fn default_template(section: &Path) -> Option<String> {
    let text = fs::read_to_string(section.join("_node.toml")).ok()?;
    kv::parse(&text).get("template").map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Sets the template a section's new pages start from (issue #519), or with `None` clears it.
/// Only a section has one; the pages already in it are not changed.
pub fn set_default_template(section: &Path, template: Option<&str>) -> Result<()> {
    if read_node(section).is_none_or(|n| n.kind != NodeKind::Section) {
        return Err(StoreError::invalid("only a section has a template for its new pages"));
    }
    let file = section.join("_node.toml");
    let text = fs::read_to_string(&file).map_err(|_| StoreError::NotFound(format!("{} is not a NoteCraft folder", section.display())))?;
    // Every line the reader takes for the key, however it is spaced, so clearing clears it.
    let kept: String = text.lines().filter(|l| l.split_once('=').is_none_or(|(key, _)| key.trim() != "template")).map(|l| format!("{l}\n")).collect();
    let out = match template.map(str::trim).filter(|t| !t.is_empty()) {
        Some(id) => format!("{kept}template = {}\n", kv::quote(id)),
        None => kept,
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
    if let Some((format, file)) = page_file(dir) {
        // A page may have no title: an empty one is written as an empty title key, which is
        // how a page reads as untitled, and its heading is left as it is (issue #132).
        let title = if title.trim().is_empty() { String::new() } else { clean_title(title)? };
        let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
        // A locked page keeps its title until it is unlocked (issue #441).
        if crate::page::read_only_in(format, &text) {
            return Err(StoreError::invalid("this page is locked read only"));
        }
        let was = tree::raw_title(format, &text).unwrap_or_default();
        let renamed = write_page_title(format, &text, &title);
        let out = if title.is_empty() { renamed } else { retitle_heading(&renamed, &was, &title, format) };
        write_atomic(&file, &out)
    } else {
        let title = clean_title(title)?;
        let file = dir.join("_node.toml");
        let text = fs::read_to_string(&file).map_err(|_| StoreError::NotFound(format!("{} is not a NoteCraft folder", dir.display())))?;
        write_atomic(&file, &kv::replace_line(&text, "title =", &format!("title = {}", kv::quote(&title))))
    }
}

/// Writes `title` into a page's head: replaces its existing title key, or inserts one if
/// it has none yet, in the same spot a new page would have written it (see
/// `notecraft_formats::templates`). A page with no title of its own needs this path the
/// first time it is given one, which a plain find-and-replace cannot do.
fn write_page_title(format: PageFormat, text: &str, title: &str) -> String {
    match format {
        PageFormat::Html => {
            if html::between(text, "<title>", "</title>").is_some() {
                html::replace_between(text, "<title>", "</title>", &html::escape(title))
            } else {
                match text.find("<meta name=\"notecraft:id\"") {
                    Some(i) => format!("{}<title>{}</title>\n{}", &text[..i], html::escape(title), &text[i..]),
                    None => text.to_string(),
                }
            }
        }
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(text);
            let front = if frontmatter::value(&front, "title").is_some() {
                kv::replace_line(&front, "title:", &format!("title: {}", kv::quote(title)))
            } else {
                let mut out = String::with_capacity(front.len() + title.len() + 16);
                let mut inserted = false;
                for line in front.split_inclusive('\n') {
                    out.push_str(line);
                    if !inserted && line.starts_with("id:") {
                        out.push_str(&format!("title: {}\n", kv::quote(title)));
                        inserted = true;
                    }
                }
                out
            };
            format!("{front}{rest}")
        }
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

    /// Issue #517 (review): a move never writes into or takes away a folder it did not make.
    #[test]
    fn a_move_leaves_a_folder_already_there_alone() {
        let t = TempDir::new("relocate-taken");
        let from = t.path().join("from");
        let to = t.path().join("to");
        fs::create_dir_all(&from).expect("from");
        fs::write(from.join("a.txt"), "mine").expect("file");
        fs::create_dir_all(&to).expect("someone else's");
        fs::write(to.join("b.txt"), "theirs").expect("file");
        assert!(matches!(relocate(&from, &to), Err(MoveError::Taken(_))), "the name is taken");
        assert!(from.join("a.txt").is_file() && to.join("b.txt").is_file(), "both as they were");
        let free = t.path().join("free");
        assert!(relocate(&from, &free).map_err(MoveError::into_error).expect("moved"), "a whole move");
        assert!(!from.exists() && free.join("a.txt").is_file());
    }

    /// Issue #517: a section's pages go into another section after its own, in their order,
    /// with their subpages, files and ids; a folder name taken there gets the next free one;
    /// the emptied section goes, and a copy of it as it was is in the recycle bin.
    #[test]
    fn a_section_merges_into_another() {
        let t = TempDir::new("merge-section");
        let st = Settings::at(t.path().join("s.toml"));
        let nb = create_notebook(&t.path().join("books"), "Book", None, &st).expect("notebook");
        let from = create_node(&nb, "From", NodeKind::Section, None, &st).expect("from");
        let into = create_node(&nb, "Into", NodeKind::Section, None, &st).expect("into");
        let kept = create_node(&into, "Plan", NodeKind::Page, None, &st).expect("page there");
        let a = create_node(&from, "Plan", NodeKind::Page, None, &st).expect("same name");
        let _b = create_node(&from, "Budget", NodeKind::Page, Some(PageFormat::Html), &st).expect("b");
        let sub = create_node(&a, "Detail", NodeKind::Page, None, &st).expect("subpage");
        fs::create_dir_all(a.join("assets")).expect("assets");
        fs::write(a.join("assets").join("p.png"), b"png").expect("file");
        let a_id = read_node(&a).expect("a").id;
        let sub_name = tree::folder_name(&sub);
        let merged = merge_section(&from, &into).expect("merge");
        assert!(!from.exists(), "the emptied section is gone");
        let titles: Vec<String> = child_dirs(&into).iter().filter_map(|p| read_node(p)).map(|n| n.title).collect();
        assert_eq!(titles, ["Plan", "Plan", "Budget"], "after the pages there, in their order");
        assert_eq!(merged.moved.len(), 2);
        let (_, new_a) = &merged.moved[0];
        assert_ne!(new_a, &kept, "the name taken there was not written over");
        assert_eq!(read_node(new_a).expect("moved").id, a_id, "the page keeps its id, so links still find it");
        assert!(new_a.join(&sub_name).is_dir() && new_a.join("assets").join("p.png").is_file(), "subpages and files go with it");
        let bin = crate::recycle::list(&nb).expect("bin");
        assert!(bin.iter().any(|r| r.title == "From"), "a copy of the section as it was: {bin:?}");
        // Not into itself, nor a page into a section.
        assert!(merge_section(&into, &into).is_err());
        assert!(merge_section(new_a, &into).is_err());
        // A page that is a link to a folder elsewhere: nothing is moved.
        #[cfg(unix)]
        {
            let other = create_node(&nb, "Other", NodeKind::Section, None, &st).expect("other");
            let away = create_notebook(&t.path().join("away"), "Away", None, &st).expect("another notebook");
            let away_section = create_node(&away, "There", NodeKind::Section, None, &st).expect("section there");
            let far = create_node(&away_section, "Far", NodeKind::Page, None, &st).expect("far page");
            std::os::unix::fs::symlink(&far, other.join("far")).expect("link");
            let here = create_node(&other, "Here", NodeKind::Page, None, &st).expect("here");
            assert!(merge_section(&other, &into).is_err(), "refused");
            assert!(far.is_dir() && here.is_dir() && other.is_dir(), "nothing moved");
            // A link inside a page: refused too, before anything moves.
            fs::remove_file(other.join("far")).expect("unlink");
            std::os::unix::fs::symlink(far.join("x"), here.join("link.png")).expect("asset link");
            assert!(merge_section(&other, &into).is_err(), "refused for a link inside a page");
            assert!(here.join("link.png").symlink_metadata().is_ok() && here.is_dir(), "nothing moved");
        }
    }

    /// A drag in the navigation pane puts a node just before another, or last, in the same parent
    /// or another; only the node placed is written while there is room between its neighbours.
    #[test]
    fn a_node_is_placed_where_it_was_dropped_writing_only_itself() {
        let t = TempDir::new("place-node");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = create_notebook(&t.path().join("books"), "Work", None, &s).expect("notebook");
        let a = create_node(&nb, "A", NodeKind::Section, None, &s).expect("section a");
        let b = create_node(&nb, "B", NodeKind::Section, None, &s).expect("section b");
        let pages: Vec<PathBuf> =
            ["One", "Two", "Three", "Four"].iter().map(|n| create_node(&a, n, NodeKind::Page, Some(PageFormat::Html), &s).expect("page")).collect();
        compact_orders(&a).expect("ten apart");
        let names = |dir: &Path| node_children(dir).iter().map(|c| read_node(c).expect("node").title).collect::<Vec<_>>();
        assert_eq!(names(&a), ["One", "Two", "Three", "Four"]);
        let stamp = |p: &Path| fs::metadata(page_file(p).expect("page file").1).and_then(|m| m.modified()).expect("time");

        // Four goes before Two; nothing but Four is written.
        let untouched: Vec<_> = pages[..3].iter().map(|p| stamp(p)).collect();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let four = place_node(&pages[3], &a, Some(&pages[1])).expect("place");
        assert_eq!(names(&a), ["One", "Four", "Two", "Three"]);
        assert_eq!(pages[..3].iter().map(|p| stamp(p)).collect::<Vec<_>>(), untouched, "the other pages were not written");

        // First, and last.
        place_node(&pages[2], &a, Some(&pages[0])).expect("first");
        assert_eq!(names(&a), ["Three", "One", "Four", "Two"]);
        place_node(&pages[2], &a, None).expect("last");
        assert_eq!(names(&a), ["One", "Four", "Two", "Three"]);

        // Into another section, before nothing there yet; and into a page, as its subpage.
        let moved = place_node(&four, &b, None).expect("other section");
        assert_eq!(names(&b), ["Four"]);
        assert_eq!(names(&a), ["One", "Two", "Three"]);
        let sub = place_node(&moved, &pages[0], None).expect("subpage");
        assert_eq!(names(&pages[0]), ["Four"]);
        assert!(sub.starts_with(fs::canonicalize(&pages[0]).expect("page")));

        // Squeezed in until there is no room: then the section is renumbered and the order holds.
        for _ in 0..8 {
            place_node(&pages[2], &a, Some(&pages[1])).expect("before two");
            place_node(&pages[1], &a, Some(&pages[2])).expect("before three");
        }
        assert_eq!(names(&a), ["One", "Two", "Three"]);

        // Sections among sections; a notebook only among notebooks; never into itself.
        place_node(&b, &nb, Some(&a)).expect("section first");
        assert_eq!(names(&nb), ["B", "A"]);
        assert!(place_node(&nb, &a, None).is_err());
        assert!(place_node(&a, &pages[0], None).is_err(), "a page holds no section");
        assert!(place_node(&pages[0], &sub, None).is_err(), "not inside itself");
    }

    /// Issue #301: Save a Copy makes content of its own, anywhere, without the history.
    #[test]
    fn save_a_copy_copies_content_without_its_history_under_new_ids() {
        let t = TempDir::new("save-a-copy");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = create_notebook(&t.path().join("books"), "Work", None, &s).expect("notebook");
        let section = create_node(&nb, "Plans", NodeKind::Section, None, &s).expect("section");
        let page = create_node(&section, "Launch", NodeKind::Page, Some(PageFormat::Html), &s).expect("page");
        crate::assets::attach_bytes(&page, "pic.png", b"not really a picture").expect("asset");
        crate::page::save_body(&page, "<p>first</p>").expect("save");
        crate::page::save_body(&page, "<p>second</p>").expect("save");
        assert!(page.join(".notecraft").join("versions").is_dir(), "the page has history");
        fs::create_dir_all(nb.join(".notecraft").join("recycle-bin")).expect("bin");
        let out = t.path().join("copies");
        fs::create_dir_all(&out).expect("out");

        for (node, kind) in [(&page, NodeKind::Page), (&section, NodeKind::Section), (&nb, NodeKind::Notebook)] {
            let copy = save_copy(node, &out).expect("copy");
            assert!(copy.starts_with(fs::canonicalize(&out).expect("out")));
            let info = read_node(&copy).expect("a node");
            assert_eq!(info.kind, kind);
            assert_ne!(info.id, read_node(node).expect("original").id, "{kind:?}: an id of its own");
            assert!(!copy.join(".notecraft").exists(), "{kind:?}: no history or recycle bin");
        }
        // The copied page is whole: its words and its files.
        let copied_page = fs::canonicalize(&out).expect("out").join(tree::folder_name(&page));
        assert!(crate::page::load_page(&copied_page).expect("load").body.contains("second"));
        assert!(copied_page.join("assets").join("pic.png").is_file());
        // Copying again picks a free name, and the original is as it was.
        let again = save_copy(&page, &out).expect("again");
        assert_ne!(again, copied_page);
        assert!(page.join(".notecraft").join("versions").is_dir());
        // Not into itself, and only nodes.
        assert!(save_copy(&nb, &section).is_err());
        assert!(save_copy(&out, t.path()).is_err());
    }

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

    /// Issue #167: Move puts a page, with its subpages and files, into another section, last
    /// there; Copy leaves the original and gives every copied node an id of its own.
    #[test]
    fn a_page_moves_and_copies_into_another_section_with_everything_in_it() {
        let t = TempDir::new("ops-move-copy");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let a = create_node(&nb, "Section A", NodeKind::Section, None, &s).expect("a");
        let b = create_node(&nb, "Section B", NodeKind::Section, None, &s).expect("b");
        let page = create_node(&a, "Plan", NodeKind::Page, None, &s).expect("page");
        let sub = create_node(&page, "Detail", NodeKind::Page, None, &s).expect("subpage");
        fs::write(page.join("assets").join("note.txt"), "kept").expect("a file");
        fs::write(page.join("ink.svg"), "<svg/>").expect("ink");
        let there = create_node(&b, "Already", NodeKind::Page, None, &s).expect("a page already there");
        let id_of = |p: &Path| read_node(p).map(|n| n.id).expect("an id");
        let (page_id, sub_id) = (id_of(&page), id_of(&sub));

        // Copy: the original stays, the copy has everything and new ids, and goes last.
        let copy = copy_node(&page, &b).expect("copy");
        assert!(page.is_dir(), "the original is still there");
        assert_eq!(fs::read_to_string(copy.join("assets").join("note.txt")).expect("the file came along"), "kept");
        assert!(copy.join("ink.svg").is_file(), "and the ink");
        let copied_sub = node_children(&copy).into_iter().next().expect("the subpage came along");
        assert_ne!(id_of(&copy), page_id, "the copy has an id of its own");
        assert_ne!(id_of(&copied_sub), sub_id, "and so does its subpage");
        let order = |p: &Path| read_node(p).map(|n| n.order).expect("order");
        assert!(order(&copy) > order(&there), "the copy goes last");

        // Move: into B again, where its folder name is taken now, so it gets the next free one.
        let moved = move_node(&page, &b).expect("move");
        assert!(!page.exists(), "it left section A");
        assert_ne!(moved, copy, "the copy is not overwritten");
        assert_eq!(id_of(&moved), page_id, "a moved page is the same page");
        assert!(moved.join("assets").join("note.txt").is_file(), "with its files");
        assert_eq!(node_children(&moved).len(), 1, "and its subpage");
        assert!(order(&moved) > order(&copy), "last again");
    }

    #[test]
    fn move_and_copy_refuse_a_place_that_cannot_hold_the_node() {
        let t = TempDir::new("ops-move-refuse");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        let other = create_notebook(&root(&t), "Other", None, &s).expect("other");
        let section = create_node(&nb, "Section", NodeKind::Section, None, &s).expect("section");
        let page = create_node(&section, "Page", NodeKind::Page, None, &s).expect("page");
        let sub = create_node(&page, "Sub", NodeKind::Page, None, &s).expect("sub");
        assert!(move_node(&section, &page).is_err(), "a section cannot go into a page");
        assert!(move_node(&page, &sub).is_err(), "a page cannot go inside itself");
        assert!(copy_node(&page, &page).is_err(), "nor be copied into itself");
        assert!(move_node(&nb, &other).is_err(), "a notebook is not moved into another");
        // A section into another notebook is fine, and keeps its pages.
        let moved = move_node(&section, &other).expect("a section into another notebook");
        assert!(moved.starts_with(fs::canonicalize(&other).expect("other")));
        assert_eq!(node_children(&moved).len(), 1);
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
        set_default_template(&section, Some("builtin:to-do")).expect("template");
        assert_eq!(default_template(&section).as_deref(), Some("builtin:to-do"));
        set_default_template(&section, Some("saved:mine")).expect("again");
        assert_eq!(default_template(&section).as_deref(), Some("saved:mine"), "set again, not added twice");
        set_default_template(&section, None).expect("clear");
        assert_eq!(default_template(&section), None);
        // Written by hand without the spaces: still read, and still cleared.
        let file = section.join("_node.toml");
        let text = fs::read_to_string(&file).expect("read");
        fs::write(&file, format!("{text}template=\"builtin:grid\"\n")).expect("write");
        assert_eq!(default_template(&section).as_deref(), Some("builtin:grid"));
        set_default_template(&section, None).expect("clear again");
        assert_eq!(default_template(&section), None, "however it was spaced");
        assert!(read_node(&section).is_some_and(|n| n.kind == NodeKind::Section), "the section's other lines are kept");
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
        assert!(set_title(&nb, "  ").is_err(), "a notebook or section needs a title");
    }

    /// Issue #132: a page's title can be cleared, in both formats. The page is then untitled,
    /// as a new page is, and stays so when it is opened again; its heading is left alone.
    #[test]
    fn a_page_title_can_be_cleared() {
        let t = TempDir::new("ops-clear-title");
        let s = settings(&t);
        let nb = create_notebook(&root(&t), "NB", None, &s).expect("nb");
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let page = create_node(&nb, "Named Page", NodeKind::Page, Some(format), &s).expect("page");
            crate::page::save_body(&page, if format == PageFormat::Html { "<p>words</p>" } else { "words\n" }).expect("body");
            set_title(&page, "  ").expect("cleared");
            let loaded = crate::page::load_page(&page).expect("load");
            assert!(!loaded.has_title, "{format:?}: the page is untitled");
            assert_eq!(read_node(&page).map(|i| i.title).as_deref(), Some(crate::tree::UNTITLED_PAGE), "{format:?}");
            assert!(loaded.body.contains("words"), "{format:?}: the body is as it was");
            set_title(&page, "Named again").expect("titled again");
            assert_eq!(read_node(&page).map(|i| i.title).as_deref(), Some("Named again"), "{format:?}");
        }
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

    /// The fallback `remove_tree` takes on a VirtualBox share, where `remove_dir_all` fails
    /// with OS error 234: a whole notebook, nested folders and files, goes entry by entry.
    #[test]
    fn a_notebook_is_deleted_entry_by_entry() {
        let t = TempDir::new("ops-remove-entries");
        let s = settings(&t);
        let r = root(&t);
        let nb = create_notebook(&r, "NB", None, &s).expect("nb");
        let sec = create_node(&nb, "Section", NodeKind::Section, None, &s).expect("section");
        let p = create_node(&sec, "Page", NodeKind::Page, None, &s).expect("page");
        fs::create_dir_all(p.join("assets")).expect("assets");
        fs::write(p.join("assets").join("a.docx"), b"x").expect("attachment");

        remove_entries(&nb).expect("delete entry by entry");
        assert!(!nb.exists(), "the notebook folder is gone");
        assert!(r.exists(), "the notebooks folder is kept");
    }

    /// The same delete through a verbatim network share path (`\\?\UNC\...`), the form a
    /// notebook on a shared folder has in the app.
    #[cfg(target_os = "windows")]
    #[test]
    fn a_notebook_on_a_network_share_is_deleted() {
        let t = TempDir::new("ops-remove-unc");
        let s = settings(&t);
        let r = root(&t);
        let nb = create_notebook(&r, "NB", None, &s).expect("nb");
        create_node(&nb, "Section", NodeKind::Section, None, &s).expect("section");
        let local = tree::display_path(&fs::canonicalize(&nb).expect("canonical"));
        let (drive, rest) = local.split_once(":\\").expect("a drive path");
        let shared = PathBuf::from(format!(r"\\?\UNC\localhost\{drive}$\{rest}"));
        assert!(shared.is_dir(), "the admin share reaches the notebook: {shared:?}");

        remove_tree(&shared).expect("delete through the share");
        assert!(!nb.exists());
    }
}

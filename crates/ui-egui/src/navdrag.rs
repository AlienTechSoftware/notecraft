//! Dragging notebooks, section groups, sections and pages in the navigation pane, as in OneNote.
//!
//! A row taken hold of and moved is carried; the row under the pointer says where it would go:
//! just above it, just below it, or into it (a page into a section or into another page, as its
//! subpage; a section into a notebook or a section group). Let go, it goes there; Escape, or
//! letting go anywhere it cannot go, leaves everything where it was. A press that hardly moves
//! is still a click and opens the row, as before.

use notecraft_model::NodeKind;
use std::path::{Path, PathBuf};

/// What is being carried.
#[derive(Clone, Debug, PartialEq)]
pub struct Carried {
    pub path: PathBuf,
    pub kind: NodeKind,
    pub title: String,
}

/// Where it would go if let go now, by the row under the pointer.
#[derive(Clone, Debug, PartialEq)]
pub enum Drop {
    /// Just above this row, among the rows beside it.
    Before(PathBuf),
    /// Just below this row, among the rows beside it.
    After(PathBuf),
    /// Into this row, last among what it holds.
    Into(PathBuf),
}

/// What is carried, kept between frames while the button is down.
pub fn carried_id() -> egui::Id {
    egui::Id::new("nav-drag-carried")
}

/// Where it would go, with the frame that said so: a drop is only taken from the frame the
/// button came up in or the one before, so a row the pointer has left does not keep its say.
pub fn drop_id() -> egui::Id {
    egui::Id::new("nav-drag-drop")
}

/// Whether `carried` may sit beside a row of `kind`: pages among pages, sections and section
/// groups among each other, notebooks among notebooks.
fn may_sit_beside(carried: NodeKind, kind: NodeKind) -> bool {
    use NodeKind::{Notebook, Page, Section, SectionGroup};
    matches!((carried, kind), (Notebook, Notebook) | (Page, Page) | (Section | SectionGroup, Section | SectionGroup))
}

/// Where `carried` goes if let go at height `y` over the row of the node at `path`, of `kind`,
/// drawn from `top` to `bottom`. `None` where it cannot go: onto itself or into what it holds,
/// a page beside a section, a section into a page.
///
/// A row that can take it in is split in three: the top quarter is above it, the bottom quarter
/// below, the middle into it. A row that cannot is split in two, above and below.
pub fn drop_at(carried: &Carried, path: &Path, kind: NodeKind, top: f32, bottom: f32, y: f32) -> Option<Drop> {
    if path == carried.path || path.starts_with(&carried.path) {
        return None;
    }
    let into = kind.may_contain().contains(&carried.kind) && carried.kind != NodeKind::Notebook;
    let beside = may_sit_beside(carried.kind, kind);
    let height = (bottom - top).max(1.0);
    let at = ((y - top) / height).clamp(0.0, 1.0);
    match (into, beside) {
        (true, true) if at < 0.25 => Some(Drop::Before(path.to_path_buf())),
        (true, true) if at > 0.75 => Some(Drop::After(path.to_path_buf())),
        (true, _) => Some(Drop::Into(path.to_path_buf())),
        (false, true) if at < 0.5 => Some(Drop::Before(path.to_path_buf())),
        (false, true) => Some(Drop::After(path.to_path_buf())),
        (false, false) => None,
    }
}

/// The node a drop names.
pub fn target(drop: &Drop) -> &Path {
    match drop {
        Drop::Before(p) | Drop::After(p) | Drop::Into(p) => p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn carried(path: &str, kind: NodeKind) -> Carried {
        Carried { path: PathBuf::from(path), kind, title: String::new() }
    }

    #[test]
    fn a_page_goes_above_below_or_into_another_page() {
        let page = carried("/nb/s/one", NodeKind::Page);
        let row = Path::new("/nb/s/two");
        assert_eq!(drop_at(&page, row, NodeKind::Page, 0.0, 28.0, 2.0), Some(Drop::Before(row.into())));
        assert_eq!(drop_at(&page, row, NodeKind::Page, 0.0, 28.0, 14.0), Some(Drop::Into(row.into())), "the middle makes it a subpage");
        assert_eq!(drop_at(&page, row, NodeKind::Page, 0.0, 28.0, 26.0), Some(Drop::After(row.into())));
    }

    #[test]
    fn a_page_goes_into_a_section_and_a_section_beside_sections() {
        let page = carried("/nb/s/one", NodeKind::Page);
        let section = Path::new("/nb/other");
        assert_eq!(drop_at(&page, section, NodeKind::Section, 0.0, 28.0, 2.0), Some(Drop::Into(section.into())), "a page is never beside a section");
        let moving = carried("/nb/s", NodeKind::Section);
        assert_eq!(drop_at(&moving, section, NodeKind::Section, 0.0, 28.0, 3.0), Some(Drop::Before(section.into())));
        assert_eq!(drop_at(&moving, section, NodeKind::Section, 0.0, 28.0, 20.0), Some(Drop::After(section.into())));
        assert_eq!(drop_at(&moving, Path::new("/nb/s/one"), NodeKind::Page, 0.0, 28.0, 14.0), None, "a page holds no section");
        assert_eq!(drop_at(&moving, Path::new("/nb"), NodeKind::Notebook, 0.0, 28.0, 14.0), Some(Drop::Into("/nb".into())));
    }

    #[test]
    fn notebooks_go_beside_notebooks_only() {
        let notebook = carried("/a", NodeKind::Notebook);
        assert_eq!(drop_at(&notebook, Path::new("/b"), NodeKind::Notebook, 0.0, 28.0, 14.0), Some(Drop::After("/b".into())));
        assert_eq!(drop_at(&notebook, Path::new("/b/s"), NodeKind::Section, 0.0, 28.0, 14.0), None);
    }

    #[test]
    fn nothing_goes_onto_itself_or_into_what_it_holds() {
        let page = carried("/nb/s/one", NodeKind::Page);
        assert_eq!(drop_at(&page, Path::new("/nb/s/one"), NodeKind::Page, 0.0, 28.0, 14.0), None);
        assert_eq!(drop_at(&page, Path::new("/nb/s/one/sub"), NodeKind::Page, 0.0, 28.0, 14.0), None);
    }
}

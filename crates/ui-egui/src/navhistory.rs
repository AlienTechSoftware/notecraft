//! Back and Forward: the pages that were visited, in the order they were visited.
//!
//! The same model every browser and OneNote use. Opening a page by hand adds a visit and drops
//! whatever was ahead; Back and Forward walk the list without adding to it. A page that has gone
//! since (deleted, renamed) is skipped over rather than stopping the walk. Pure data, so it is
//! tested without a window.

use std::path::{Path, PathBuf};

/// How many earlier pages are kept.
const LIMIT: usize = 100;

#[derive(Clone, Debug, Default)]
pub struct NavHistory {
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    current: Option<PathBuf>,
}

impl NavHistory {
    /// A page was opened by hand. Opening the page that is already showing changes nothing.
    pub fn visit(&mut self, page: &Path) {
        if self.current.as_deref() == Some(page) {
            return;
        }
        if let Some(was) = self.current.take() {
            self.back.push(was);
            if self.back.len() > LIMIT {
                self.back.remove(0);
            }
        }
        self.forward.clear();
        self.current = Some(page.to_path_buf());
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn back_count(&self) -> usize {
        self.back.len()
    }

    pub fn forward_count(&self) -> usize {
        self.forward.len()
    }

    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }

    /// Steps to the nearest earlier page for which `exists` is true, and says which. Pages that
    /// no longer exist are dropped on the way. `None` when there is nowhere to go.
    pub fn back(&mut self, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        while let Some(candidate) = self.back.pop() {
            if exists(&candidate) {
                if let Some(was) = self.current.replace(candidate.clone()) {
                    self.forward.push(was);
                }
                return Some(candidate);
            }
        }
        None
    }

    /// The same, the other way.
    pub fn forward(&mut self, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
        while let Some(candidate) = self.forward.pop() {
            if exists(&candidate) {
                if let Some(was) = self.current.replace(candidate.clone()) {
                    self.back.push(was);
                }
                return Some(candidate);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    fn all(_: &Path) -> bool {
        true
    }

    #[test]
    fn a_new_history_goes_nowhere() {
        let mut h = NavHistory::default();
        assert!(!h.can_back() && !h.can_forward());
        assert_eq!(h.back(all), None);
        assert_eq!(h.forward(all), None);
        assert_eq!(h.current(), None);
    }

    #[test]
    fn the_first_page_is_not_something_to_go_back_to() {
        let mut h = NavHistory::default();
        h.visit(&p("a"));
        assert!(!h.can_back(), "there is nothing before the first page");
        assert_eq!(h.current(), Some(Path::new("a")));
    }

    #[test]
    fn back_and_forward_walk_the_visits_in_order() {
        let mut h = NavHistory::default();
        for page in ["a", "b", "c"] {
            h.visit(&p(page));
        }
        assert_eq!(h.back(all), Some(p("b")));
        assert_eq!(h.back(all), Some(p("a")));
        assert_eq!(h.back(all), None, "at the start");
        assert_eq!(h.current(), Some(Path::new("a")));
        assert_eq!(h.forward(all), Some(p("b")));
        assert_eq!(h.forward(all), Some(p("c")));
        assert_eq!(h.forward(all), None, "at the end");
        assert_eq!((h.back_count(), h.forward_count()), (2, 0));
    }

    #[test]
    fn opening_a_page_drops_what_was_ahead() {
        let mut h = NavHistory::default();
        for page in ["a", "b", "c"] {
            h.visit(&p(page));
        }
        h.back(all);
        h.back(all);
        assert!(h.can_forward());
        h.visit(&p("d"));
        assert!(!h.can_forward(), "a new visit ends the way forward, as in a browser");
        assert_eq!(h.back(all), Some(p("a")));
    }

    #[test]
    fn opening_the_page_already_showing_adds_nothing() {
        let mut h = NavHistory::default();
        h.visit(&p("a"));
        h.visit(&p("b"));
        h.visit(&p("b"));
        assert_eq!(h.back_count(), 1);
        assert_eq!(h.back(all), Some(p("a")));
    }

    #[test]
    fn a_page_that_is_gone_is_skipped() {
        let mut h = NavHistory::default();
        for page in ["a", "b", "c", "d"] {
            h.visit(&p(page));
        }
        let gone = |q: &Path| q != Path::new("c");
        assert_eq!(h.back(gone), Some(p("b")), "c is gone, so back lands on b");
        assert_eq!(h.back(|q| q == Path::new("zzz")), None, "nothing left that exists");
        assert_eq!(h.current(), Some(Path::new("b")), "and the page showing is unchanged");
    }

    #[test]
    fn only_the_last_hundred_pages_are_kept() {
        let mut h = NavHistory::default();
        for i in 0..250 {
            h.visit(&p(&format!("page-{i}")));
        }
        assert_eq!(h.back_count(), 100);
        assert_eq!(h.back(all), Some(p("page-248")));
    }
}

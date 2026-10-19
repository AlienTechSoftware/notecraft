//! What the page looked like a moment ago, so a change can be taken back.
//!
//! Every editor has an undo, and a note taker without one is not safe to write in: one
//! Delete on a picked block of cells, one Backspace at the top of a line, one box dragged
//! away, and the words are gone with nothing to ask for them back. NoteCraft had undo for
//! ink strokes only; this is undo for the page.
//!
//! **Whole states, not edits.** A step here is the page as it stood: its text, its blocks,
//! the boxes they are arranged in and where the caret was. Undo is then "put that back",
//! which cannot drift out of step with the page the way a list of reversible edits can,
//! and which costs a clone of a page - a few tens of kilobytes - per step. The stack is
//! capped by both steps and bytes so a long session cannot eat the memory of a machine.
//!
//! **Typing is one step, not one per letter.** A run of letters into the same block, with
//! no pause longer than [`PAUSE`], folds into the step before it, which is what Word and
//! OneNote do: Ctrl+Z there takes back what was just written, not the last character.

use notecraft_formats::Block;
use notecraft_model::Container;
use std::time::{Duration, Instant};

/// A run of typing into one block folds into one step until the writer pauses this long.
pub const PAUSE: Duration = Duration::from_millis(700);

/// How many steps are kept, and how much they may weigh in all.
pub const MAX_STEPS: usize = 200;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;

/// What a step was, which is how typing knows it may fold into the step before it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The page as it was opened, or as a change left it.
    Edit,
    /// Letters going into this block.
    Typing(usize),
}

/// The page as it stood at one moment.
#[derive(Clone, Debug)]
pub struct Step {
    pub body: String,
    pub blocks: Vec<Block>,
    pub boxes: Vec<Container>,
    /// Where the caret was, so undo puts it back where the writer left it.
    pub caret: Option<crate::pageedit::Caret>,
    pub kind: Kind,
    when: Instant,
}

impl Step {
    pub fn new(body: String, blocks: Vec<Block>, boxes: Vec<Container>, caret: Option<crate::pageedit::Caret>, kind: Kind) -> Step {
        Step { body, blocks, boxes, caret, kind, when: Instant::now() }
    }

    /// Roughly what this step weighs, for the cap. The text is most of it.
    fn weight(&self) -> usize {
        self.body.len() + self.blocks.len() * 64 + self.boxes.len() * 24 + 128
    }
}

/// The steps of one open page, and where in them the writer is.
#[derive(Debug)]
pub struct History {
    steps: Vec<Step>,
    /// Which step the page is showing now.
    at: usize,
}

impl History {
    /// A history that starts with the page as it was opened.
    pub fn new(first: Step) -> History {
        History { steps: vec![first], at: 0 }
    }

    /// Takes a change. Anything undone and not redone is dropped first, the way every
    /// editor does it: once you type after undoing, there is no going forward again.
    pub fn record(&mut self, step: Step) {
        self.steps.truncate(self.at + 1);
        if self.folds_into_last(&step) {
            let when = self.steps[self.at].when;
            self.steps[self.at] = Step { when, ..step };
            return;
        }
        self.steps.push(step);
        self.at = self.steps.len() - 1;
        self.trim();
    }

    /// True when this step is more letters going into the block the last step was typing
    /// in, soon enough after it to be the same run of writing.
    fn folds_into_last(&self, step: &Step) -> bool {
        let last = &self.steps[self.at];
        match (last.kind, step.kind) {
            (Kind::Typing(a), Kind::Typing(b)) => a == b && step.when.duration_since(last.when) < PAUSE,
            _ => false,
        }
    }

    /// Drops the oldest steps until the stack is inside both caps. The step the page is
    /// showing is never dropped.
    fn trim(&mut self) {
        let mut weight: usize = self.steps.iter().map(Step::weight).sum();
        while self.steps.len() > 1 && (self.steps.len() > MAX_STEPS || weight > MAX_BYTES) && self.at > 0 {
            weight -= self.steps[0].weight();
            self.steps.remove(0);
            self.at -= 1;
        }
    }

    /// When the step the page is showing was taken. The Quick Access Toolbar's Undo uses it to
    /// tell whether the pen or the page changed last.
    pub fn when(&self) -> Option<Instant> {
        self.steps.get(self.at).map(|s| s.when)
    }

    pub fn can_undo(&self) -> bool {
        self.at > 0
    }

    pub fn can_redo(&self) -> bool {
        self.at + 1 < self.steps.len()
    }

    /// The page as it was before the last change, or `None` when there is nothing to undo.
    pub fn undo(&mut self) -> Option<&Step> {
        if !self.can_undo() {
            return None;
        }
        self.at -= 1;
        self.steps.get(self.at)
    }

    /// The change that was just undone, put back.
    pub fn redo(&mut self) -> Option<&Step> {
        if !self.can_redo() {
            return None;
        }
        self.at += 1;
        self.steps.get(self.at)
    }

    /// How many steps are kept, for the tests and for `ui.inspect`.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Where in the steps the page is, for `ui.inspect`.
    pub fn at(&self) -> usize {
        self.at
    }

    /// The step the page is showing now.
    pub fn here(&self) -> Option<&Step> {
        self.steps.get(self.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_formats::markdown::parse;

    fn step(text: &str, kind: Kind) -> Step {
        Step::new(text.to_string(), parse(text), Vec::new(), None, kind)
    }

    #[test]
    fn undo_walks_back_through_the_changes_and_redo_walks_forward() {
        let mut h = History::new(step("one", Kind::Edit));
        assert!(!h.can_undo(), "nothing to undo on a page just opened");
        h.record(step("one two", Kind::Edit));
        h.record(step("one two three", Kind::Edit));

        assert_eq!(h.undo().map(|s| s.body.as_str()), Some("one two"));
        assert_eq!(h.undo().map(|s| s.body.as_str()), Some("one"));
        assert_eq!(h.undo().map(|s| s.body.as_str()), None, "and no further back than the page was opened");
        assert_eq!(h.redo().map(|s| s.body.as_str()), Some("one two"));
        assert_eq!(h.redo().map(|s| s.body.as_str()), Some("one two three"));
        assert_eq!(h.redo().map(|s| s.body.as_str()), None);
    }

    #[test]
    fn typing_in_one_block_is_one_step() {
        let mut h = History::new(step("", Kind::Edit));
        for text in ["h", "he", "hel", "hell", "hello"] {
            h.record(step(text, Kind::Typing(0)));
        }
        assert_eq!(h.len(), 2, "the page as opened, and the writing");
        assert_eq!(h.undo().map(|s| s.body.as_str()), Some(""), "one undo takes back the run, not a letter");

        // Letters into another block are their own step.
        let mut h = History::new(step("", Kind::Edit));
        h.record(step("a", Kind::Typing(0)));
        h.record(step("a b", Kind::Typing(1)));
        assert_eq!(h.len(), 3);
    }

    #[test]
    fn a_change_after_an_undo_drops_what_was_undone() {
        let mut h = History::new(step("one", Kind::Edit));
        h.record(step("one two", Kind::Edit));
        h.undo();
        assert!(h.can_redo());
        h.record(step("one else", Kind::Edit));
        assert!(!h.can_redo(), "the other way forward is gone, as in every editor");
        assert_eq!(h.undo().map(|s| s.body.as_str()), Some("one"));
    }

    #[test]
    fn the_stack_is_capped_and_keeps_the_newest() {
        let mut h = History::new(step("start", Kind::Edit));
        for i in 0..MAX_STEPS + 50 {
            h.record(step(&format!("change {i}"), Kind::Edit));
        }
        assert!(h.len() <= MAX_STEPS, "no more than the cap: {}", h.len());
        assert_eq!(h.undo().map(|s| s.body.clone()), Some(format!("change {}", MAX_STEPS + 48)), "the newest are the ones kept");
    }

    #[test]
    fn a_heavy_page_is_capped_by_weight_too() {
        let big = "x".repeat(1024 * 1024);
        let mut h = History::new(step("start", Kind::Edit));
        for _ in 0..40 {
            h.record(step(&big, Kind::Edit));
        }
        assert!(h.len() < 40, "the heavy steps were trimmed: {}", h.len());
        assert!(h.can_undo(), "but there is still something to undo");
    }
}

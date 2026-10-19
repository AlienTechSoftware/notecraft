//! What writing Arabic in NoteCraft actually does, measured.
//!
//! The owner's bar is that the caret and the right-to-left experience are the best of any
//! note taking application (`work/rtl-and-caret-bar.md` in the planning repo). A claim
//! like that is only worth making if each part of it is measured, so every test here
//! records what the app does today - whether that is the bar or a gap - and says which it
//! is. A test that fails after a change is an answer that moved, not necessarily a
//! regression: read the comment and decide.

use egui_kittest::Harness;
use notecraft_ui_egui::{bidi, fonts, theme::Theme};

/// One sentence of plain Arabic: "what you can do here".
const LINE: &str = "ما الذي يمكنك فعله هنا";
/// The same with harakat on the first word, and a word with a bracket in it.
const VOWELLED: &str = "كِتَاب";
const BRACKETED: &str = "النص (مثال) هنا";

fn harness() -> Harness<'static> {
    let mut h = Harness::builder().with_size(egui::vec2(600.0, 200.0)).build_ui(|ui| fonts::install_once(ui.ctx()));
    h.run();
    h
}

/// Runs `f` inside a frame that has our fonts, and gives back what it returns.
fn in_frame<T: Default>(mut f: impl FnMut(&egui::Ui) -> T) -> T {
    let h = harness();
    let mut out = T::default();
    let mut frame = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        fonts::install_once(ui.ctx());
        out = f(ui);
    });
    frame.textures_delta.clear();
    out
}

/// 2.4 of the bar. Home and End move to the ends of the line's text. In a line that reads
/// right to left that puts the caret at the right edge for Home and the left edge for End,
/// which is what Word and a browser do: the key means the start of the line, and where
/// that is on screen follows the direction.
///
/// Measured: **the bar**. The caret is further right after Home than after End.
#[test]
fn home_and_end_go_to_the_visual_ends_of_a_right_to_left_line() {
    let t = Theme::DEFAULT.tokens();
    let (home, end) = in_frame(|ui| {
        let pieces = bidi::plain(LINE, 16.0, &t);
        let home = bidi::caret(ui, LINE, &pieces, 0, 16.0, 400.0).map(|r| r.min.x).unwrap_or(0.0);
        let end = bidi::caret(ui, LINE, &pieces, LINE.len(), 16.0, 400.0).map(|r| r.min.x).unwrap_or(0.0);
        (home, end)
    });
    assert!(home > end + 50.0, "Home is at the right edge and End at the left: home {home}, end {end}");
    assert!(home > 380.0, "and the line hangs from the right edge it was given: {home}");
}

/// 2.9 of the bar. A step never lands between a letter and the diacritic on it: a caret
/// there would be between two things that are drawn as one, and the next keystroke would
/// split the letter from its vowel.
///
/// Measured: **the bar**. parley moves by grapheme, so `كِتَاب` is four stops, not six.
#[test]
fn a_step_never_lands_between_a_letter_and_its_diacritic() {
    let marks: Vec<usize> = VOWELLED.char_indices().filter(|(_, c)| matches!(c, '\u{64b}'..='\u{65f}' | '\u{670}')).map(|(i, _)| i).collect();
    assert_eq!(marks.len(), 2, "the word has two harakat in it: {marks:?}");

    // Every stop the caret makes walking the word. In a right-to-left word the left
    // arrow is the one that goes forward through the letters, which is itself worth
    // saying: at the start of the word the right arrow has nowhere to go.
    assert_eq!(bidi::step(VOWELLED, 0, true, 16.0, 400.0), 0, "the right arrow is already at the start of a right-to-left word");
    let mut stops = vec![0usize];
    let mut at = 0usize;
    for _ in 0..12 {
        let next = bidi::step(VOWELLED, at, false, 16.0, 400.0);
        if next == at {
            break;
        }
        at = next;
        stops.push(at);
    }
    assert_eq!(*stops.last().unwrap_or(&0), VOWELLED.len(), "the steps reach the end: {stops:?}");
    for mark in &marks {
        assert!(!stops.contains(mark), "no stop sits on the diacritic at {mark}: {stops:?}");
    }
    assert_eq!(stops.len(), 5, "four letters, four stops and the end: {stops:?}");
}

/// 2.9 again, from the other side: a click near a vowelled letter puts the caret beside
/// the letter, not between it and its vowel.
///
/// Measured: **the bar**, once the step was fixed on 2026-10-19.
#[test]
fn a_click_inside_a_vowelled_letter_lands_beside_it() {
    let marks: Vec<usize> = VOWELLED.char_indices().filter(|(_, c)| matches!(c, '\u{64b}'..='\u{65f}' | '\u{670}')).map(|(i, _)| i).collect();
    // Points across the word, from its left edge to its right.
    for step in 0..40 {
        let x = step as f32 * 3.0;
        let at = bidi::hit(VOWELLED, egui::vec2(x, 8.0), 16.0, 400.0);
        assert!(!marks.contains(&at), "a click at x={x} landed on the mark at {at}");
    }
}

/// 2.7 of the bar. A triple click takes the line.
///
/// Measured: **a gap**. The page editor answers a double click, which takes the word, and
/// nothing answers a third. Written as what it does today so that building it fails here.
#[test]
fn a_triple_click_does_not_take_the_line_yet() {
    let source = include_str!("../src/pageedit.rs");
    assert!(source.contains("double_clicked()"), "a double click takes the word");
    assert!(!source.contains("triple_clicked()"), "a triple click is not answered yet: when it is, this test should say what it takes instead");
}

/// 1.5 of the bar. In a right-to-left line a bracket is mirrored when it is drawn, and
/// sits on the side the Unicode algorithm puts it (UAX #9, L4): an opening bracket typed
/// before a word appears to the right of that word, drawn as `)`.
///
/// Measured: **the bar**, and not by reading the characters - they say nothing about which
/// glyph is drawn. The page `النص (مثال` was rendered, then the same with a closing
/// bracket, and the two pictures differ in exactly one 9 by 28 patch: the bracket. In the
/// first, the glyph drawn is `)`, to the right of `مثال` and left of `النص`, which is
/// where and how it belongs. egui mirrors it; nothing in NoteCraft has to.
///
/// What is asserted here is the part a test can hold: the characters reach egui as they
/// were typed, so the mirroring is egui's and would be lost if a run were ever handed over
/// pre-reversed. The picture is in the journal.
#[test]
fn a_bracket_in_a_right_to_left_line_reaches_egui_as_it_was_typed() {
    let t = Theme::DEFAULT.tokens();
    let drawn: Vec<char> = in_frame(|ui| {
        let pieces = bidi::plain(BRACKETED, 16.0, &t);
        let (runs, _) = bidi::runs(ui, BRACKETED, &pieces, 16.0, 400.0);
        runs.iter().flat_map(|d| d.galley.text().chars().collect::<Vec<_>>()).collect()
    });
    assert!(drawn.contains(&'('), "the opening bracket is handed over as itself: {drawn:?}");
    assert!(drawn.contains(&')'), "and so is the closing one");
}

/// 4.3 of the bar. Pages and sections are listed in the order a reader of the script
/// expects.
///
/// Measured: **a gap, in part**. The tree sorts by the `order` in each `_node.toml` first,
/// and only falls back to the folder name, compared byte by byte. For Arabic that is code
/// point order, which is the alphabet's order for the plain letters but puts every word
/// starting with a hamza form (`أ`, `إ`, `آ`) after plain `ا`, and ignores the collation
/// rules a reader would expect. Pages the owner has ordered are unaffected.
#[test]
fn arabic_names_sort_by_code_point_not_by_collation() {
    let mut names = ["بيت".to_string(), "أحمد".to_string(), "تاج".to_string(), "احمد".to_string()];
    names.sort();
    assert_eq!(
        names,
        ["أحمد", "احمد", "بيت", "تاج"],
        "code point order: the hamza form sorts before the plain alef because U+0623 is below U+0627,          and a reader expects the two spellings of the same name to collate together"
    );
}

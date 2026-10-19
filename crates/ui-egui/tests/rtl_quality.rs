//! What writing Arabic in NoteCraft actually does, measured.
//!
//! The owner's bar is that the caret and the right-to-left experience are the best of any
//! note taking application (`work/rtl-and-caret-bar.md` in the planning repo). A claim
//! like that is only worth making if each part of it is measured, so every test here
//! records what the app does today - whether that is the bar or a gap - and says which it
//! is. A test that fails after a change is an answer that moved, not necessarily a
//! regression: read the comment and decide.
//!
//! What it has found so far, in the order the tests sit below: the caret keeps off the
//! vowels and Home and End go to the visual ends of the line; a bracket reaches egui as it
//! was typed and is mirrored there; a line, and then a whole paragraph, is justified by
//! stretching the letters rather than the spaces, which is the line of the bar no other
//! note taking application answers at all; and the words of an Arabic name are drawn in
//! reading order in the window around the page - the notebook tree and the title band -
//! and not only inside the page. Still gaps: a triple click that takes the line, and
//! sorting names by Arabic collation rather than by code point.

use egui_kittest::Harness;
use notecraft_ui_egui::{bidi, fonts, theme::Theme};

/// One sentence of plain Arabic: "what you can do here".
const LINE: &str = "ما الذي يمكنك فعله هنا";
/// The same with harakat on the first word, and a word with a bracket in it.
const VOWELLED: &str = "كِتَاب";
const BRACKETED: &str = "النص (مثال) هنا";
/// A page of the demo notebook, as the tree lists it: "pictures and files". The two words
/// have a letter each that is in neither of the others, which is how a glyph is found again
/// after shaping: `ص` in the first word, `ف` in the second.
const NAME: &str = "الصور والملفات";

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

/// 1.8 of the bar: justification by kashida, the one line on it that no other note taking
/// application answers at all. OneNote, Obsidian and Notion all justify Arabic the Latin
/// way, by pulling the words apart, which is what leaves the rivers of white down a
/// justified Arabic page.
///
/// Measured: **it works**, in the face the page is drawn with. `notecraft_text::kashida`
/// picks the joins to stretch and egui shapes the tatweels into the words, so the line
/// reaches the column within one tatweel of the target with every space exactly as wide as
/// it was. Set `NOTECRAFT_UI_VISUAL_DIR` to look at it.
///
/// Wired into the window on 2026-10-19: the Home tab's alignment menu has a Justify
/// entry, the page records which paragraphs are justified on a line in its head
/// (`notecraft_model::justify`), and the painter stretches them. This test is the engine
/// on one line; the next one is the paragraph as the page draws it.
#[test]
fn a_line_is_justified_by_stretching_the_letters_not_the_spaces() {
    const COLUMN: f32 = 520.0;
    const SENTENCE: &str = "الملاحظات تعيش أطول من البرنامج";
    let t = Theme::DEFAULT.tokens();
    let (natural, filled, step, out) = in_frame(|ui| {
        // Measured the way the line is actually drawn, and in the column it is drawn in:
        // the bidi painter places each word itself and decides its own wrapping, so its
        // answer is the one that counts. Measuring the string on its own instead is off by
        // the spaces, and the line then wraps one word past the column. A stretch that
        // wraps the line is no width at all, which is how the search knows to stop.
        let line_height = {
            let pieces = bidi::plain(SENTENCE, 18.0, &t);
            bidi::runs(ui, SENTENCE, &pieces, 18.0, COLUMN).1.y
        };
        let mut width = |s: &str| {
            let pieces = bidi::plain(s, 18.0, &t);
            // Laid out in the column, the painter reports the column's own width, so the
            // height is what says whether the line still fits; the ink width comes from
            // laying the same line out with nothing to wrap against.
            if bidi::runs(ui, s, &pieces, 18.0, COLUMN).1.y > line_height + 1.0 {
                return f32::INFINITY;
            }
            bidi::runs(ui, s, &pieces, 18.0, f32::INFINITY).1.x
        };
        let natural = width(SENTENCE);
        let out = notecraft_text::kashida::justify(SENTENCE, COLUMN, &mut width);
        let step = width("ـ");
        (natural, width(&out), step, out)
    });
    assert!(natural < COLUMN, "the sentence starts short of the column: {natural}");
    assert!(filled.is_finite(), "the justified line still fits on one line");
    assert!(filled <= COLUMN, "it does not run past the column: {filled}");
    assert!(COLUMN - filled <= step + 0.5, "and it gets there: {natural} to {filled} of {COLUMN}, one step is {step}");
    // Only tatweels were added: every word, and every space between them, is untouched.
    let stripped: String = out.chars().filter(|c| *c != notecraft_text::kashida::TATWEEL).collect();
    assert_eq!(stripped, SENTENCE);
    assert_eq!(out.matches(' ').count(), SENTENCE.matches(' ').count(), "no space was widened or added");
    assert!(out.contains(notecraft_text::kashida::TATWEEL), "something was stretched");
    picture(SENTENCE, &out, COLUMN);
}

/// 1.8 of the bar, as a page draws it: a whole paragraph of Arabic, several lines long,
/// with every line but the last reaching the column and the last one left as it fell.
///
/// Measured: **it works**. `bidi::stretched` is what the painter calls for a paragraph the
/// page has marked justified. It walks the paragraph's lines, stretches each but the last,
/// and leaves the words alone: what comes back is the same sentences with tatweels in the
/// joins. Set `NOTECRAFT_UI_VISUAL_DIR` to look at it.
///
/// The last line is the part worth saying out loud. A justified paragraph whose closing
/// half-sentence is pulled out to the margin is the mark of a bad typesetter, and every
/// word processor that gets Arabic justification wrong gets this wrong too.
#[test]
fn a_whole_paragraph_is_justified_except_its_last_line() {
    const COLUMN: f32 = 420.0;
    const SIZE: f32 = 16.0;
    // Four sentences, long enough to wrap several times in a column of that width.
    const PARAGRAPH: &str = "الملاحظات يجب أن تعيش أطول من البرنامج الذي كتبها، ولهذا تحفظ نوت كرافت كل صفحة في مجلد من ملفات نصية عادية يقرؤها أي محرر. الدفتر مجلد، والقسم مجلد، والصفحة مجلد فيه ملف واحد وصوره ومرفقاته.";

    let t = Theme::DEFAULT.tokens();
    let (lines, stretched, tatweels) = in_frame(|ui| {
        let pieces = bidi::plain(PARAGRAPH, SIZE, &t);
        let (out, moved) = bidi::stretched(ui, PARAGRAPH, &pieces, SIZE, COLUMN);
        // Every line as it is drawn: its left edge and its right edge in the column.
        let (drawn, _) = bidi::runs(ui, &out, &moved, SIZE, COLUMN);
        let mut rows: Vec<(f32, f32, f32)> = Vec::new();
        for d in &drawn {
            let (left, right) = (d.at.x, d.at.x + d.galley.rect.width());
            match rows.iter_mut().find(|(y, _, _)| (*y - d.at.y).abs() < 1.0) {
                Some((_, l, r)) => {
                    *l = l.min(left);
                    *r = r.max(right);
                }
                None => rows.push((d.at.y, left, right)),
            }
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        let count = out.chars().filter(|c| *c == notecraft_text::kashida::TATWEEL).count();
        (rows, out, count)
    });

    assert!(lines.len() >= 3, "the paragraph wraps to several lines: {}", lines.len());
    // One tatweel is the smallest step a plain face has, so a line is as full as it can be
    // when it is within a few points of the column. A little over is the search landing on
    // the nearer side of the target, which is what `kashida::justify` is asked to do.
    // The paragraph reads right to left, so every line hangs from the right edge of the
    // column and it is the left edge that says whether a line was filled.
    let (_, last_left, _) = lines.last().copied().unwrap_or_default();
    for (n, (_, left, right)) in lines.iter().enumerate().take(lines.len() - 1) {
        assert!(*left < 14.0, "line {n} was stretched all the way to the far edge: it starts at {left}");
        assert!((COLUMN - *right).abs() < 14.0, "line {n} still hangs from the near edge: {left} to {right} of {COLUMN}");
    }
    assert!(last_left > 20.0, "the last line is left where it fell, not pulled out: it starts at {last_left} of {COLUMN}");

    // Nothing but tatweels was added, and no space was widened or taken away.
    assert!(tatweels > 0, "something was stretched");
    let stripped: String = stretched.chars().filter(|c| *c != notecraft_text::kashida::TATWEEL).collect();
    assert_eq!(stripped, PARAGRAPH, "the words are the writer's own");
    assert_eq!(stretched.matches(' ').count(), PARAGRAPH.matches(' ').count(), "no space was widened or added");
    paragraph_picture(PARAGRAPH, &stretched, COLUMN, SIZE);
}

/// Draws the ragged paragraph above the justified one.
fn paragraph_picture(plain: &str, justified: &str, column: f32, size: f32) {
    let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let t = Theme { mode: notecraft_ui_egui::theme::Mode::Light, accent: notecraft_ui_egui::theme::Accent::Default }.tokens();
    let (plain, justified) = (plain.to_string(), justified.to_string());
    let mut h = Harness::builder().with_size(egui::vec2(column + 48.0, 320.0)).with_pixels_per_point(2.0).wgpu().build_ui(move |ui| {
        fonts::install_once(ui.ctx());
        ui.painter().rect_filled(ui.max_rect(), 0.0, t.editor);
        ui.add_space(16.0);
        for text in [plain.as_str(), justified.as_str()] {
            let pieces = bidi::plain(text, size, &t);
            let (drawn, used) = bidi::runs(ui, text, &pieces, size, column);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(column, used.y.max(24.0)), egui::Sense::hover());
            bidi::paint(ui, rect.min, &drawn);
            ui.painter().rect_stroke(
                egui::Rect::from_min_size(rect.min, egui::vec2(column, used.y.max(24.0))),
                0.0,
                egui::Stroke::new(1.0, t.muted),
                egui::StrokeKind::Outside,
            );
            ui.add_space(22.0);
        }
    });
    h.run();
    if let Ok(img) = h.render() {
        let _ = img.save(dir.join("kashida-paragraph.png"));
    }
}

/// Draws the ragged line above the justified one, so the difference can be looked at
/// rather than argued about. Only when `NOTECRAFT_UI_VISUAL_DIR` says where to put it.
fn picture(plain: &str, justified: &str, column: f32) {
    let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") else { return };
    let dir = std::path::PathBuf::from(dir);
    let _ = std::fs::create_dir_all(&dir);
    let t = Theme { mode: notecraft_ui_egui::theme::Mode::Light, accent: notecraft_ui_egui::theme::Accent::Default }.tokens();
    let (plain, justified) = (plain.to_string(), justified.to_string());
    let mut h = Harness::builder().with_size(egui::vec2(column + 40.0, 150.0)).with_pixels_per_point(2.0).wgpu().build_ui(move |ui| {
        fonts::install_once(ui.ctx());
        ui.painter().rect_filled(ui.max_rect(), 0.0, t.editor);
        ui.add_space(20.0);
        for line in [plain.as_str(), justified.as_str()] {
            let pieces = bidi::plain(line, 18.0, &t);
            let (drawn, size) = bidi::runs(ui, line, &pieces, 18.0, column);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(column, size.y.max(24.0)), egui::Sense::hover());
            bidi::paint(ui, rect.min, &drawn);
            // The column the line is being justified into, so the eye can see which
            // one reaches it.
            ui.painter().rect_stroke(
                egui::Rect::from_min_size(rect.min, egui::vec2(column, size.y.max(24.0))),
                0.0,
                egui::Stroke::new(1.0, t.muted),
                egui::StrokeKind::Outside,
            );
            ui.add_space(24.0);
        }
    });
    h.run();
    if let Ok(img) = h.render() {
        let _ = img.save(dir.join("kashida.png"));
    }
}

/// 1.2 of the bar, in the window rather than in the page. Every label the window draws
/// around the page - a row of the notebook tree, the title band above it - puts the words
/// of an Arabic name in the order they are read.
///
/// Measured: **the bar**, from 2026-10-19. Before that day it was a gap, and the most
/// visible one left in the app: `Painter::text` shapes the letters of each word correctly
/// and then puts the next word to the right of the one before it, so the demo notebook's
/// Arabic section listed `الصور والملفات` as `والملفات الصور` in a tree that is on screen
/// all the time. `bidi::line_at` is what the window now draws a label with, and the second
/// half of this test is the fault it was built for, still there in egui underneath.
#[test]
fn the_words_of_an_arabic_name_are_drawn_in_reading_order_in_the_window() {
    let first = 'ص'; // in الصور, the word that is read first
    let second = 'ف'; // in والملفات, the word that is read second
    assert_eq!(NAME.matches(first).count(), 1, "the letter is in one word only");
    assert_eq!(NAME.matches(second).count(), 1);

    // What the window now draws: one galley per word, each placed where it belongs.
    let h = harness();
    let mut frame = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        fonts::install_once(ui.ctx());
        let painter = ui.painter().clone();
        bidi::line_at(
            ui,
            &painter,
            egui::pos2(40.0, 40.0),
            egui::Align2::LEFT_CENTER,
            NAME,
            14.0,
            egui::Color32::BLACK,
            notecraft_model::Dir::default(),
        );
    });
    let words: Vec<(f32, String)> = frame
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => Some((t.pos.x, t.galley.text().to_string())),
            _ => None,
        })
        .collect();
    frame.textures_delta.clear();
    assert_eq!(words.len(), 2, "a word to a galley, placed one by one: {words:?}");
    let x_of = |needle: char| words.iter().find(|(_, t)| t.contains(needle)).map(|(x, _)| *x);
    let (a, b) = (x_of(first).unwrap_or(0.0), x_of(second).unwrap_or(0.0));
    assert!(a > b, "the word that is read first is drawn to the right of the second: {a} then {b} in {words:?}");

    // And the fault underneath, so this test says why the helper exists: handed the whole
    // string, egui draws the first word to the left of the second, which is backwards.
    let mut frame = h.ctx.run_ui(egui::RawInput::default(), |ui| {
        fonts::install_once(ui.ctx());
        ui.painter().text(egui::pos2(40.0, 40.0), egui::Align2::LEFT_CENTER, NAME, egui::FontId::proportional(14.0), egui::Color32::BLACK);
    });
    let plain: Vec<(f32, f32)> = frame
        .shapes
        .iter()
        .filter_map(|c| match &c.shape {
            egui::Shape::Text(t) => {
                let at = |needle: char| t.galley.rows.first().and_then(|r| r.glyphs.iter().find(|g| g.chr == needle)).map(|g| g.pos.x);
                Some((at(first)?, at(second)?))
            }
            _ => None,
        })
        .collect();
    frame.textures_delta.clear();
    let (a, b) = plain.first().copied().unwrap_or_default();
    assert!(a < b, "egui on its own still draws the words the other way round: {a} then {b}");
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

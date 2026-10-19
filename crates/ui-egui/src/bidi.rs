//! Drawing a line that mixes the two directions, in the order it should be read.
//!
//! egui shapes a run of one direction correctly but never reorders: in a line that mixes
//! Arabic and English it draws `السعر 123 ريال` with the number backwards, because nothing
//! applies the Unicode bidirectional algorithm (`crates/ui-egui/tests/arabic_layout.rs`
//! measures it, and epaint's own source says "TODO: heed bidi characters").
//!
//! So the work is split. [`notecraft_text`] (parley) says what the runs of the line are, in
//! what order they are drawn, and where the caret goes. egui still shapes, measures and
//! draws each run, which is the part it does well. Nothing here draws a glyph itself.
//!
//! A mixed line goes through this, and so does any paragraph that reads right to left: egui
//! does not wrap such a paragraph into lines that hang from the right edge, and parley does.
//! A left-to-right line of one direction is left to egui, which gets it right and keeps
//! every feature the page editor has.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, Pos2, Rect, Ui, Vec2, pos2, vec2};
use std::cell::RefCell;

thread_local! {
    /// One text engine per thread: it holds the font collection, which is not cheap to build.
    static TEXT: RefCell<notecraft_text::Text> = RefCell::new(notecraft_text::Text::new());
}

/// True when this painter should draw `text` rather than egui: a line of both directions,
/// or a paragraph that reads right to left (given the page's direction for a line with no
/// strong letters of its own).
pub fn paints(text: &str, page: notecraft_model::Dir) -> bool {
    is_mixed(text) || (notecraft_model::direction_or(text, page).is_rtl() && !text.trim().is_empty())
}

/// True when a line has strong characters of both directions in it, which is the only case
/// egui gets wrong.
pub fn is_mixed(text: &str) -> bool {
    let mut other = false;
    let mut rtl = false;
    for c in text.chars() {
        if notecraft_model::direction(c.encode_utf8(&mut [0u8; 4])) == notecraft_model::Dir::Rtl {
            rtl = true;
        } else if c.is_alphabetic() || c.is_ascii_digit() {
            // Western digits are the catch: they are not a direction of their own, but
            // inside a right-to-left line they read left to right, and that is exactly
            // what egui draws backwards.
            other = true;
        }
        if other && rtl {
            return true;
        }
    }
    false
}

/// A piece of the line and how it is drawn: the byte range in the line's text, and the
/// format egui should give it. This is what the page's spans turn into.
pub type Piece = (std::ops::Range<usize>, TextFormat);

/// One run, ready to draw: where it goes and the galley that draws it.
pub struct Drawn {
    pub at: Vec2,
    pub galley: std::sync::Arc<egui::Galley>,
    /// The bytes of the line this run holds, and which way it reads.
    pub text: std::ops::Range<usize>,
    pub rtl: bool,
}

/// Lays `text` out with parley, builds one egui job per run with the formats of the pieces
/// inside it, and places each run where parley put it.
///
/// parley decides the order, the split into lines, and where every run starts, which is
/// also how a line that reads right to left comes to hang from the right edge. egui shapes
/// and draws each run; its galley can be about a pixel wider than parley's run (egui rounds
/// each advance), which leans into the gap after it and never accumulates. The caret and
/// the selection below are mapped through the same places.
pub fn runs(ui: &Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32) -> (Vec<Drawn>, Vec2) {
    let Some(split) = split(text, size, max_width) else { return (Vec::new(), Vec2::ZERO) };
    let space = space_width(ui, size);
    let mut out: Vec<Drawn> = Vec::new();
    let mut size_used = Vec2::ZERO;
    for line in split.lines {
        let mut placed: Vec<(f32, std::sync::Arc<egui::Galley>, std::ops::Range<usize>, bool)> = Vec::new();
        for run in line.runs {
            let Some(whole) = text.get(run.text.clone()) else { continue };
            // The spaces at the ends of a run are the gaps around it. They are measured and
            // placed here rather than left inside the galley, because egui puts the space of
            // a right-to-left run on the far side from the one the algorithm asks for.
            let lead = whole.len() - whole.trim_start_matches(' ').len();
            let trail = whole.len() - whole.trim_end_matches(' ').len();
            let inner = run.text.start + lead..run.text.end.saturating_sub(trail).max(run.text.start + lead);
            let galley_of = |ui: &Ui, range: std::ops::Range<usize>| -> Option<std::sync::Arc<egui::Galley>> {
                let mut job = LayoutJob { halign: Align::LEFT, keep_trailing_whitespace: true, ..LayoutJob::default() };
                for (piece, format) in pieces {
                    let (a, b) = (piece.start.max(range.start), piece.end.min(range.end));
                    if a >= b {
                        continue;
                    }
                    if let Some(part) = text.get(a..b) {
                        job.append(part, 0.0, format.clone());
                    }
                }
                (!job.sections.is_empty()).then(|| ui.fonts_mut(|f| f.layout_job(job)))
            };
            if inner.is_empty() {
                continue;
            }
            if run.rtl {
                // egui lays the letters of an Arabic or Hebrew word out right to left, but
                // puts the next word to the right of the one before it, so a line of them
                // comes out with its words backwards: the owner read one on 2026-10-19 and
                // said so. Each word is drawn on its own and placed from the right edge of
                // the run leftwards, which is where they belong.
                //
                // The gaps between the words are measured here, not left in a galley, for
                // the same reason the gaps round a run are.
                let mut right = run.x + run.width - lead as f32 * space;
                for (word, gap) in words_of(text, inner.clone()) {
                    let Some(galley) = galley_of(ui, word.clone()) else { continue };
                    let width = galley.rect.width();
                    placed.push((right - width, galley, word, true));
                    right -= width + gap as f32 * space;
                }
            } else {
                let Some(galley) = galley_of(ui, inner.clone()) else { continue };
                placed.push((run.x + lead as f32 * space, galley, inner, false));
            }
        }
        // Each run starts where parley put it. That is what makes a right-to-left line hang
        // from the right edge of `max_width`, a shorter last line included, and what keeps
        // the drawing under the clicks: `hit` maps a point through parley's coordinates. An
        // egui galley is up to about a pixel wider than parley's run, so a run may lean that
        // far into the gap after it; it never accumulates, because the next run starts at
        // its own place, not after this one.
        for (x, galley, range, rtl) in placed {
            let width = galley.rect.width();
            size_used.x = size_used.x.max(x + width);
            size_used.y = size_used.y.max(line.y + line.height);
            out.push(Drawn { at: vec2(x, line.y), galley, text: range, rtl });
        }
    }
    (out, size_used)
}

/// The words of a range, each with the number of spaces that follow it.
///
/// Only spaces: a word is what egui shapes as one piece, and shaping does not join across
/// a space. Anything else inside the run - a comma, a digit - rides along with its word.
fn words_of(text: &str, range: std::ops::Range<usize>) -> Vec<(std::ops::Range<usize>, usize)> {
    let Some(inner) = text.get(range.clone()) else { return Vec::new() };
    let mut out: Vec<(std::ops::Range<usize>, usize)> = Vec::new();
    let mut at = range.start;
    for part in inner.split(' ') {
        let word = at..at + part.len();
        at = word.end + 1;
        if part.is_empty() {
            // Two spaces in a row: the gap after the word before grows by one.
            if let Some((_, gap)) = out.last_mut() {
                *gap += 1;
            }
            continue;
        }
        out.push((word, 1));
    }
    if let Some((_, gap)) = out.last_mut() {
        // The last word has no space after it inside this run.
        *gap = 0;
    }
    out
}

/// How wide a space is in this face and size.
fn space_width(ui: &Ui, size: f32) -> f32 {
    let mut job = LayoutJob { keep_trailing_whitespace: true, ..LayoutJob::default() };
    job.append(" ", 0.0, egui::text::TextFormat { font_id: egui::FontId::proportional(size), ..egui::text::TextFormat::default() });
    ui.fonts_mut(|f| f.layout_job(job)).rect.width()
}

/// What parley says about a line: its runs, in drawing order.
struct Split {
    lines: Vec<notecraft_text::Line>,
}

fn split(text: &str, size: f32, max_width: f32) -> Option<Split> {
    TEXT.with(|t| {
        let mut engine = t.try_borrow_mut().ok()?;
        let layout = engine.layout(text, size, Some(max_width));
        Some(Split { lines: notecraft_text::lines(&layout) })
    })
}

/// Draws the runs at `origin`.
pub fn paint(ui: &Ui, origin: Pos2, drawn: &[Drawn]) {
    for d in drawn {
        // A right-to-left galley is laid out to the left of its origin, so it is shifted
        // back by its own left edge: then its left edge lands where the run belongs.
        let at = origin + d.at - vec2(d.galley.rect.min.x, 0.0);
        ui.painter().galley(at, d.galley.clone(), Color32::PLACEHOLDER);
    }
}

/// A line as it is heard rather than as it is stored: the direction marks and isolates that
/// carry the writing direction are invisible on screen, so they are left out of the label a
/// screen reader reads and the tests look a line up by.
fn spoken(text: &str) -> String {
    const INVISIBLE: [char; 5] = ['\u{200F}', '\u{200E}', '\u{2066}', '\u{2067}', '\u{2069}'];
    text.chars().filter(|c| !INVISIBLE.contains(c)).collect()
}

/// Draws one mixed line into `ui`, taking the space it needs. A line that reads right to
/// left hangs from the right edge, as it does everywhere else in the page. Returns its
/// rectangle.
///
/// The galleys are painted straight onto the canvas, so nothing here is a widget that egui
/// would describe by itself. The line is given its text as a label, the way `Label` does, or
/// a screen reader would find nothing on a right-to-left page and the window tests could not
/// look a line up by what it says.
pub fn show(ui: &mut Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32, page: notecraft_model::Dir) -> Rect {
    let (drawn, used) = runs(ui, text, pieces, size, max_width);
    let full = vec2(max_width, used.y.max(size));
    let (rect, resp) = ui.allocate_exact_size(full, egui::Sense::hover());
    let enabled = ui.is_enabled();
    let said = spoken(text);
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &said));
    let origin = if notecraft_model::direction_or(text, page).is_rtl() { rect.min + vec2((rect.width() - used.x).max(0.0), 0.0) } else { rect.min };
    paint(ui, origin, &drawn);
    rect
}

/// Where a character sits inside a drawn run, measured from the run's left edge.
///
/// The glyphs of a shaped word do not line up with its characters one for one: a mark above
/// a letter is its own glyph with no width. Counting only the glyphs that advance gives the
/// characters back, which `tests/arabic_layout.rs` measures. In a run that reads right to
/// left the characters are drawn from the right, so the count is taken from the other end.
fn x_in_run(galley: &egui::Galley, chars_before: usize, rtl: bool) -> f32 {
    let Some(row) = galley.rows.first() else { return 0.0 };
    let advancing: Vec<&egui::epaint::text::Glyph> = row.glyphs.iter().filter(|g| g.advance_width > 0.0).collect();
    let total = advancing.len();
    let target = if rtl { total.saturating_sub(chars_before.min(total)) } else { chars_before.min(total) };
    let left = galley.rect.min.x;
    match advancing.get(target) {
        Some(g) => g.pos.x - left,
        None => galley.rect.width(),
    }
}

/// How many characters of a run come before a byte in it.
fn chars_before(text: &str, run: &std::ops::Range<usize>, at: usize) -> usize {
    let start = run.start.min(at);
    text.get(start..at.min(run.end)).map(|s| s.chars().count()).unwrap_or(0)
}

/// The run a byte belongs to, preferring the one it starts inside.
fn run_of(drawn: &[Drawn], at: usize) -> Option<&Drawn> {
    drawn.iter().find(|d| d.text.contains(&at)).or_else(|| drawn.iter().find(|d| d.text.end == at)).or_else(|| drawn.last())
}

/// Where the caret belongs on a line, in the drawn runs' own space.
pub fn caret(ui: &Ui, text: &str, pieces: &[Piece], at: usize, size: f32, max_width: f32) -> Option<Rect> {
    let (drawn, _) = runs(ui, text, pieces, size, max_width);
    caret_in(text, &drawn, at, size)
}

fn caret_in(text: &str, drawn: &[Drawn], at: usize, size: f32) -> Option<Rect> {
    let run = run_of(drawn, at)?;
    let x = run.at.x + x_in_run(&run.galley, chars_before(text, &run.text, at), run.rtl);
    Some(Rect::from_min_size(pos2(x, run.at.y), vec2(1.5, run.galley.rect.height().max(size))))
}

/// The rectangles to paint behind a selection between two bytes. A selection that covers
/// both directions is more than one rectangle, one per run it touches.
pub fn selection(ui: &Ui, text: &str, pieces: &[Piece], from: usize, to: usize, size: f32, max_width: f32) -> Vec<Rect> {
    let (drawn, _) = runs(ui, text, pieces, size, max_width);
    selection_in(text, &drawn, from, to, size)
}

fn selection_in(text: &str, drawn: &[Drawn], from: usize, to: usize, size: f32) -> Vec<Rect> {
    let (a, b) = (from.min(to), from.max(to));
    if a == b {
        return Vec::new();
    }
    let mut out = Vec::new();
    for run in drawn {
        let (s, e) = (a.max(run.text.start), b.min(run.text.end));
        if s >= e {
            continue;
        }
        let x0 = run.at.x + x_in_run(&run.galley, chars_before(text, &run.text, s), run.rtl);
        let x1 = run.at.x + x_in_run(&run.galley, chars_before(text, &run.text, e), run.rtl);
        let (left, right) = (x0.min(x1), x0.max(x1));
        out.push(Rect::from_min_max(pos2(left, run.at.y), pos2(right.max(left + 1.0), run.at.y + run.galley.rect.height().max(size))));
    }
    out
}

/// A character that is drawn on the letter before it rather than beside it: an Arabic
/// haraka, a Hebrew point, a combining accent. The caret must never stand between one of
/// these and the letter it belongs to - the two are drawn as one mark on the page, so a
/// caret between them is in a place the eye cannot see, and the next Backspace takes the
/// vowel off a letter that looks untouched.
///
/// The ranges here are the ones NoteCraft's own scripts need. A general answer is Unicode's
/// grapheme breaking, which wants a crate of its own; until that is weighed, this covers
/// Arabic, Hebrew and the Latin combining accents, and anything else steps as it did.
fn is_mark(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F   // combining diacritical marks (Latin and Greek)
        | 0x0483..=0x0489 // Cyrillic
        | 0x0591..=0x05BD | 0x05BF | 0x05C1..=0x05C2 | 0x05C4..=0x05C5 | 0x05C7 // Hebrew points
        | 0x0610..=0x061A // Arabic honorifics
        | 0x064B..=0x065F | 0x0670 // harakat and the dagger alef
        | 0x06D6..=0x06DC | 0x06DF..=0x06E4 | 0x06E7..=0x06E8 | 0x06EA..=0x06ED // Quranic marks
        | 0x0711 | 0x0730..=0x074A // Syriac
        | 0x07A6..=0x07B0 // Thaana
        | 0x0E31 | 0x0E34..=0x0E3A | 0x0E47..=0x0E4E // Thai
        | 0x20D0..=0x20F0 // combining marks for symbols
        | 0xFE00..=0xFE0F // variation selectors
    )
}

/// Whether a byte index falls on a mark, which means it is inside a letter rather than
/// between two of them.
fn inside_a_letter(text: &str, at: usize) -> bool {
    text.get(at..).and_then(|rest| rest.chars().next()).is_some_and(is_mark)
}

/// One step left or right on a mixed line, as the eye moves.
///
/// parley steps by shaped cluster, and a letter with a haraka on it is two of those, so a
/// plain step would stop inside the letter. The step carries on in the same direction
/// until it is between two letters.
pub fn step(text: &str, at: usize, right: bool, size: f32, max_width: f32) -> usize {
    TEXT.with(|t| {
        let Ok(mut engine) = t.try_borrow_mut() else { return at };
        let layout = engine.layout(text, size, Some(max_width));
        let mut now = at;
        // At most a few marks hang off one letter; the bound is there so a strange string
        // cannot spin here.
        for _ in 0..8 {
            let caret = notecraft_text::Caret::at(now);
            let moved = if right { notecraft_text::step_right(&layout, caret) } else { notecraft_text::step_left(&layout, caret) };
            let next = moved.index();
            if next == now {
                return now;
            }
            now = next;
            if !inside_a_letter(text, now) {
                return now;
            }
        }
        now
    })
}

/// The word under a point: what a double click takes.
pub fn word_at(text: &str, at: Vec2, size: f32, max_width: f32) -> std::ops::Range<usize> {
    TEXT.with(|t| {
        let Ok(mut engine) = t.try_borrow_mut() else { return 0..0 };
        let layout = engine.layout(text, size, Some(max_width));
        notecraft_text::word_at(&layout, at.x, at.y)
    })
}

/// The byte a click at `x`, `y` (from the top left of the line) lands on.
pub fn hit(text: &str, at: Vec2, size: f32, max_width: f32) -> usize {
    TEXT.with(|t| {
        let Ok(mut engine) = t.try_borrow_mut() else { return 0 };
        let layout = engine.layout(text, size, Some(max_width));
        let index = notecraft_text::caret_at(&layout, at.x, at.y).index();
        // A click near a letter that carries a haraka can land between the two; the caret
        // belongs before the letter, not inside it.
        let mut now = index;
        while now < text.len() && inside_a_letter(text, now) {
            now += text[now..].chars().next().map(char::len_utf8).unwrap_or(1);
        }
        now
    })
}

/// The pieces of a plain, unstyled line.
pub fn plain(text: &str, size: f32, t: &Tokens) -> Vec<Piece> {
    vec![(0..text.len(), TextFormat { font_id: egui::FontId::proportional(size), color: t.text, ..TextFormat::default() })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    const PRICE: &str = "السعر 123 ريال";

    /// A context whose fonts are ours. `set_fonts` takes effect on the next frame, so one
    /// empty frame runs first; measuring in the same frame as the install would measure
    /// egui's own faces, which have no Arabic and give every letter a box a full em wide.
    fn ctx_with_fonts() -> egui::Context {
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        ctx
    }

    /// A plain Arabic line reads right to left, word by word. egui lays the letters of one
    /// word out the right way but puts the next word to the right of it, so a line came out
    /// with its words backwards until each word was placed here.
    #[test]
    fn the_words_of_an_arabic_line_go_right_to_left() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let line = "ما الذي يمكنك فعله هنا";
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let (drawn, _) = runs(ui, line, &plain(line, 16.0, &t), 16.0, 400.0);
            assert_eq!(drawn.len(), 5, "one for each word: {:?}", drawn.iter().map(|d| d.galley.text().to_string()).collect::<Vec<_>>());
            // The first word of the line is the rightmost thing on it, and each word after
            // it is further left, with a gap between them.
            let mut last: Option<&Drawn> = None;
            for d in &drawn {
                assert!(d.rtl, "{:?} reads right to left", d.galley.text());
                if let Some(before) = last {
                    assert!(d.at.x + d.galley.rect.width() <= before.at.x + 1.0, "{:?} is left of {:?}", d.galley.text(), before.galley.text());
                }
                last = Some(d);
            }
            // And the line still hangs from the right edge it was given.
            let right = drawn.first().map(|d| d.at.x + d.galley.rect.width()).unwrap_or(0.0);
            assert!(right > 380.0 && right <= 401.0, "the line ends at the right edge: {right}");
        });
        frame.textures_delta.clear();
    }

    /// The words must not run into the digits: the space between two runs is a real gap,
    /// wide enough to read, and no run is drawn on top of another.
    #[test]
    fn the_runs_are_spaced_and_never_overlap() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let (drawn, size) = runs(ui, PRICE, &plain(PRICE, 16.0, &t), 16.0, 400.0);
            assert!(drawn.len() >= 3, "{} runs", drawn.len());
            let space = space_width(ui, 16.0);
            assert!(space > 1.0, "a space has a width: {space}");
            // The line hangs from the right edge, so it starts wherever its first run is.
            let mut end = drawn.first().map(|d| d.at.x).unwrap_or(0.0);
            let mut gaps = 0;
            for d in &drawn {
                // egui's galley may be about a pixel wider than parley's run; it leans into
                // the gap, never onto the next word.
                assert!(d.at.x >= end - 1.5, "no run sits on top of the one before it: {} after {end}", d.at.x);
                if d.at.x - end > space * 0.5 {
                    gaps += 1;
                }
                end = d.at.x + d.galley.rect.width();
            }
            assert_eq!(
                gaps,
                2,
                "a gap on each side of the number: {:?}",
                drawn.iter().map(|d| (d.at.x, d.galley.text().to_string())).collect::<Vec<_>>()
            );
            assert!(size.x >= end - 0.01);
        });
        frame.textures_delta.clear();
    }

    /// A paragraph that reads right to left wraps into lines that each hang from the right
    /// edge, the shorter last line too, and a left-to-right line starts at the left.
    #[test]
    fn right_to_left_lines_hang_from_the_right_edge() {
        const LONG: &str = "כל העמוד הזה נקרא מימין, והמחברת הזאת נכתבה כך מההתחלה: כיוון העמוד מוגדר לימין בקובץ שלו, ולכן פסקה שאין לה אותיות משלה הולכת אחריו. השורה נתלית מהקצה הימני.";
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let (drawn, used) = runs(ui, LONG, &plain(LONG, 16.0, &t), 16.0, 300.0);
            let mut lines: std::collections::BTreeMap<i32, (f32, f32)> = std::collections::BTreeMap::new();
            for d in &drawn {
                let e = lines.entry(d.at.y.round() as i32).or_insert((f32::INFINITY, 0.0));
                e.0 = e.0.min(d.at.x);
                e.1 = e.1.max(d.at.x + d.galley.rect.width());
            }
            assert!(lines.len() >= 3, "the paragraph wrapped: {} lines", lines.len());
            for (y, (left, right)) in &lines {
                assert!((right - 300.0).abs() < 4.0, "line at {y} ends at the right edge: {left}..{right}");
                assert!(*left >= -1.0, "and starts inside the column: {left}");
            }
            // The last line is the one that proves it: it is shorter than the column, and
            // it still ends at the right edge, so it hangs from the right rather than
            // starting at the left. How much shorter depends on the words and the face,
            // so what is checked is that it does not start at the left edge.
            let lefts: Vec<f32> = lines.values().map(|(l, _)| *l).collect();
            assert!(lefts.last().is_some_and(|l| *l > 10.0), "the last line hangs from the right, not the left: {lefts:?}");
            assert!((used.x - 300.0).abs() < 4.0, "the block is as wide as the column: {}", used.x);

            let (ltr, _) = runs(ui, "Hello مرحبا world", &plain("Hello مرحبا world", 16.0, &t), 16.0, 300.0);
            assert!(ltr.first().is_some_and(|d| d.at.x.abs() < 1.0), "a left-to-right line starts at the left");
        });
        frame.textures_delta.clear();
        assert!(paints("שלום עולם", notecraft_model::Dir::Ltr), "a right-to-left paragraph is painted here");
        assert!(paints("123", notecraft_model::Dir::Rtl), "a line with no letters follows the page");
        assert!(!paints("123", notecraft_model::Dir::Ltr));
        assert!(!paints("Hello world", notecraft_model::Dir::Rtl), "a left-to-right line is egui's even on a right-to-left page");
    }

    #[test]
    fn only_a_line_of_two_directions_needs_this() {
        assert!(is_mixed(PRICE));
        assert!(is_mixed("Hello مرحبا"));
        assert!(!is_mixed("Hello world"), "egui draws this correctly already");
        assert!(!is_mixed("مرحبا بالعالم"), "and this: one direction, shaped and ordered right");
        assert!(!is_mixed("123 456"), "digits alone are not a direction");
        assert!(is_mixed("\u{627}\u{644}\u{633}\u{639}\u{631} 123"), "digits inside Arabic: the case egui draws backwards");
        assert!(!is_mixed(""));
    }

    #[test]
    fn a_mixed_line_is_drawn_as_runs_in_reading_order() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let (drawn, size) = runs(ui, PRICE, &plain(PRICE, 16.0, &t), 16.0, 400.0);
            assert!(drawn.len() >= 3, "Arabic, digits, Arabic: {} runs", drawn.len());
            assert!(size.x > 0.0 && size.y > 0.0);
            let xs: Vec<f32> = drawn.iter().map(|d| d.at.x).collect();
            assert!(xs.windows(2).all(|w| w[0] <= w[1]), "runs in drawing order: {xs:?}");
            // The digits are one run, in the order they were typed, with Arabic either side.
            let at = drawn.iter().position(|d| d.galley.text().contains("123")).unwrap_or(usize::MAX);
            assert!(at != usize::MAX && at > 0 && at + 1 < drawn.len(), "digits between the words: run {at} of {}", drawn.len());
            assert!(!drawn.iter().any(|d| d.galley.text().contains("321")));
        });
        frame.textures_delta.clear();
    }

    #[test]
    fn a_selection_is_one_rectangle_for_each_run_it_covers() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let pieces = plain(PRICE, 16.0, &t);
            // Nothing selected, nothing painted.
            assert!(selection(ui, PRICE, &pieces, 4, 4, 16.0, 400.0).is_empty());
            // The whole line: one rectangle per run, together covering what is drawn.
            let (drawn, size) = runs(ui, PRICE, &pieces, 16.0, 400.0);
            let all = selection(ui, PRICE, &pieces, 0, PRICE.len(), 16.0, 400.0);
            assert_eq!(all.len(), drawn.len(), "a rectangle for each run: {all:?}");
            let covered: f32 = all.iter().map(egui::Rect::width).sum();
            let extent = drawn.iter().map(|d| d.at.x + d.galley.rect.width()).fold(0.0f32, f32::max) - drawn.first().map(|d| d.at.x).unwrap_or(0.0);
            assert!(covered > extent * 0.75, "they cover the line: {covered} of {extent}");
            for r in &all {
                assert!(r.min.x >= -1.0 && r.max.x <= size.x + 1.0, "inside the line: {r:?}");
                assert!(r.height() > 4.0);
            }
            // A selection of the digits alone is one rectangle, over the digits.
            let start = PRICE.find("123").unwrap_or(0);
            let digits = selection(ui, PRICE, &pieces, start, start + 3, 16.0, 400.0);
            assert_eq!(digits.len(), 1, "the digits are one run: {digits:?}");
            let run = drawn.iter().find(|d| d.galley.text().contains("123")).map(|d| (d.at.x, d.galley.rect.width()));
            if let (Some(rect), Some((x, w))) = (digits.first(), run) {
                assert!(rect.min.x >= x - 1.0 && rect.max.x <= x + w + 1.0, "over the digits: {rect:?} against {x}..{}", x + w);
            }
        });
        frame.textures_delta.clear();
    }

    #[test]
    fn a_double_click_takes_the_word_under_the_pointer() {
        // The middle of the line is the number; the far right is the first word.
        let word = word_at(PRICE, vec2(500.0, 8.0), 16.0, 400.0);
        assert_eq!(PRICE.get(word.clone()).map(str::trim), Some("السعر"), "{word:?}");
        assert!(!word.is_empty());
    }

    #[test]
    fn the_caret_and_the_steps_come_from_the_same_layout() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let pieces = plain(PRICE, 16.0, &t);
            let start = caret(ui, PRICE, &pieces, 0, 16.0, 400.0).unwrap_or(Rect::ZERO);
            let end = caret(ui, PRICE, &pieces, PRICE.len(), 16.0, 400.0).unwrap_or(Rect::ZERO);
            assert!(start.height() > 4.0, "a caret bar: {start:?}");
            // The line reads right to left, so the start of the text is at the right.
            assert!(start.min.x > end.min.x, "start on the right, end on the left: {start:?} {end:?}");
            // And the caret never lands outside the drawn line.
            let (_, size) = runs(ui, PRICE, &pieces, 16.0, 400.0);
            for at in [0, 5, 9, PRICE.len()] {
                let r = caret(ui, PRICE, &pieces, at, 16.0, 400.0).unwrap_or(Rect::ZERO);
                assert!(r.min.x >= -1.0 && r.min.x <= size.x + 1.0, "{at}: {r:?} inside {size:?}");
            }
        });
        frame.textures_delta.clear();

        // The line reads right to left, so the first character is at the right edge and
        // there is nowhere further right to go; the way forward through the text is left.
        assert_eq!(step(PRICE, 0, true, 16.0, 400.0), 0, "the start is already the right end");
        let left = step(PRICE, 0, false, 16.0, 400.0);
        assert_ne!(left, 0, "a step left moves forward through the text");
        assert_eq!(step(PRICE, left, true, 16.0, 400.0), 0, "and back again");
        // A click near the right edge lands at the beginning of the text.
        let at = hit(PRICE, vec2(500.0, 8.0), 16.0, 400.0);
        assert!(at <= 4, "the right edge is the start of the line: {at}");
    }
}

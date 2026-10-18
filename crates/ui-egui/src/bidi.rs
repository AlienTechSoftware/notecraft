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
//! Only a mixed line goes through this. A line of one direction is left to egui, which gets
//! it right and keeps every feature the page editor has.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{Align, Color32, Pos2, Rect, Ui, Vec2, pos2, vec2};
use std::cell::RefCell;

thread_local! {
    /// One text engine per thread: it holds the font collection, which is not cheap to build.
    static TEXT: RefCell<notecraft_text::Text> = RefCell::new(notecraft_text::Text::new());
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
/// inside it, and places the runs left to right using the widths **egui** measures.
///
/// parley and egui differ by about a pixel per run (egui rounds each advance), so taking
/// parley's x for the drawing would slowly swallow the spaces between runs. parley decides
/// the order and the split; egui decides the widths; the caret below is mapped through the
/// same places, so the two can never drift apart.
pub fn runs(ui: &Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32) -> (Vec<Drawn>, Vec2) {
    let Some(split) = split(text, size, max_width) else { return (Vec::new(), Vec2::ZERO) };
    let space = space_width(ui, size);
    let mut out: Vec<Drawn> = Vec::new();
    let mut size_used = Vec2::ZERO;
    for line in split.lines {
        let mut placed: Vec<(notecraft_text::Run, std::sync::Arc<egui::Galley>, f32, f32)> = Vec::new();
        for run in line.runs {
            let Some(whole) = text.get(run.text.clone()) else { continue };
            // The spaces at the ends of a run are the gaps around it. They are measured and
            // placed here rather than left inside the galley, because egui puts the space of
            // a right-to-left run on the far side from the one the algorithm asks for.
            let lead = whole.len() - whole.trim_start_matches(' ').len();
            let trail = whole.len() - whole.trim_end_matches(' ').len();
            let inner = run.text.start + lead..run.text.end.saturating_sub(trail).max(run.text.start + lead);
            let mut job = LayoutJob { halign: Align::LEFT, keep_trailing_whitespace: true, ..LayoutJob::default() };
            for (range, format) in pieces {
                let (a, b) = (range.start.max(inner.start), range.end.min(inner.end));
                if a >= b {
                    continue;
                }
                if let Some(part) = text.get(a..b) {
                    job.append(part, 0.0, format.clone());
                }
            }
            if job.sections.is_empty() {
                // A run of nothing but spaces is a gap of its own.
                if lead + trail > 0 {
                    placed.push((run, ui.fonts_mut(|f| f.layout_job(LayoutJob::default())), (lead + trail) as f32 * space, 0.0));
                }
                continue;
            }
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            // In a right-to-left run the first characters are drawn on the right, so a
            // space at the start of it belongs on the right-hand side.
            let (before, after) = if run.rtl { (trail as f32 * space, lead as f32 * space) } else { (lead as f32 * space, trail as f32 * space) };
            placed.push((run, galley, before, after));
        }
        let mut x = 0.0f32;
        for (run, galley, before, after) in placed {
            x += before;
            let width = galley.rect.width();
            size_used.x = size_used.x.max(x + width);
            size_used.y = size_used.y.max(line.y + line.height);
            out.push(Drawn { at: vec2(x, line.y), galley, text: run.text.clone(), rtl: run.rtl });
            x += width + after;
        }
    }
    (out, size_used)
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

/// Draws one mixed line into `ui`, taking the space it needs. A line that reads right to
/// left hangs from the right edge, as it does everywhere else in the page. Returns its
/// rectangle.
pub fn show(ui: &mut Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32, page: notecraft_model::Dir) -> Rect {
    let (drawn, used) = runs(ui, text, pieces, size, max_width);
    let full = vec2(max_width, used.y.max(size));
    let (rect, _) = ui.allocate_exact_size(full, egui::Sense::hover());
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

/// One step left or right on a mixed line, as the eye moves.
pub fn step(text: &str, at: usize, right: bool, size: f32, max_width: f32) -> usize {
    TEXT.with(|t| {
        let Ok(mut engine) = t.try_borrow_mut() else { return at };
        let layout = engine.layout(text, size, Some(max_width));
        let caret = notecraft_text::Caret::at(at);
        let moved = if right { notecraft_text::step_right(&layout, caret) } else { notecraft_text::step_left(&layout, caret) };
        moved.index()
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
        notecraft_text::caret_at(&layout, at.x, at.y).index()
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

    /// The words must not run into the digits: the space between two runs is a real gap,
    /// wide enough to read, and no run is drawn on top of another.
    #[test]
    fn the_runs_are_spaced_and_never_overlap() {
        let t = Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let (drawn, size) = runs(ui, PRICE, &plain(PRICE, 16.0, &t), 16.0, 400.0);
            assert!(drawn.len() >= 3, "{} runs", drawn.len());
            let space = space_width(ui, 16.0);
            assert!(space > 1.0, "a space has a width: {space}");
            let mut end = 0.0f32;
            let mut gaps = 0;
            for d in &drawn {
                assert!(d.at.x >= end - 0.01, "no run sits on top of the one before it: {} after {end}", d.at.x);
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
        let ctx = egui::Context::default();
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
        let ctx = egui::Context::default();
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
            assert!(covered > size.x * 0.75, "they cover the line: {covered} of {}", size.x);
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
        let ctx = egui::Context::default();
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

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
use egui::{Align, Align2, Color32, Pos2, Rect, Ui, Vec2, pos2, vec2};
use notecraft_model::justify::Strength;
use std::cell::RefCell;

thread_local! {
    /// One text engine per thread: it holds the font collection, which is not cheap to build.
    static TEXT: RefCell<notecraft_text::Text> = RefCell::new(notecraft_text::Text::new());
}

/// True when this painter should draw `text` rather than egui.
///
/// Three reasons, and any one of them is enough.
///
/// 1. The line has both directions in it, which egui draws in the wrong order.
/// 2. The line's own words are right to left, so they must be ordered right to left
///    whatever the paragraph's base direction is. A Hebrew sentence inside a page set to
///    left to right still reads right to left as a sentence; what the page decides is which
///    edge the line hangs from, not the order of the words in it.
/// 3. The paragraph's base direction is right to left, from its own mark or from the page,
///    so the line hangs from the right edge even if its words are Latin.
pub fn paints(text: &str, page: Option<notecraft_model::Dir>) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    is_mixed(text) || notecraft_model::direction(text).is_rtl() || notecraft_model::direction_in(text, page).is_rtl()
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
    paint_with(ui.painter(), origin, drawn);
}

/// The same, onto a painter the caller chose: one that is clipped to a row of the tree, or
/// one made for a band of the window. The page's own words go through [`paint`]; this is
/// for the places that draw straight onto a rectangle they have already laid out.
pub fn paint_with(painter: &egui::Painter, origin: Pos2, drawn: &[Drawn]) {
    for d in drawn {
        // A right-to-left galley is laid out to the left of its origin, so it is shifted
        // back by its own left edge: then its left edge lands where the run belongs.
        let at = origin + d.at - vec2(d.galley.rect.min.x, 0.0);
        painter.galley(at, d.galley.clone(), Color32::PLACEHOLDER);
    }
}

/// Draws one short line of words where `painter.text` would draw it, but with the words in
/// the order they are read.
///
/// `Painter::text` hands the whole string to egui, which shapes the letters of each word
/// correctly and then puts the next word to the right of the one before it. In Arabic or
/// Hebrew that comes out with the words backwards: a section called
/// `من اليمين إلى اليسار` is drawn `اليسار إلى اليمين من`. The page's own words have gone
/// through this module since 2026-10-19; the window around them had not, so a notebook
/// written in Arabic read correctly in the page and backwards in the tree beside it and in
/// the band above it.
///
/// This is for a label that is one line and already has its place: a row of the tree, the
/// window's title band. `at` and `align` mean what they mean to `Painter::text`, and the
/// line is measured and then anchored the same way, so a label that needs none of this is
/// drawn exactly as it was before. A line too long for the painter's clip is cut by it, as
/// it was before.
pub fn line_at(ui: &Ui, painter: &egui::Painter, at: Pos2, align: Align2, text: &str, size: f32, color: Color32, page: Option<notecraft_model::Dir>) {
    let font = egui::FontId::proportional(size);
    if !paints(text, page) {
        painter.text(at, align, text, font, color);
        return;
    }
    let pieces = [(0..text.len(), TextFormat { font_id: font.clone(), color, ..TextFormat::default() })];
    // Nothing to wrap against: a label is one line, however long it is.
    let (drawn, _) = runs(ui, text, &pieces, size, f32::INFINITY);
    let left = drawn.iter().map(|d| d.at.x).fold(f32::INFINITY, f32::min);
    let right = drawn.iter().map(|d| d.at.x + d.galley.rect.width()).fold(f32::NEG_INFINITY, f32::max);
    let tall = drawn.iter().map(|d| d.galley.rect.height()).fold(0.0_f32, f32::max);
    if !left.is_finite() || !right.is_finite() {
        painter.text(at, align, text, font, color);
        return;
    }
    // Anchored on the ink the runs actually cover, which is what `Painter::text` anchors on.
    let box_ = align.anchor_size(at, vec2(right - left, tall));
    paint_with(painter, box_.min - vec2(left, 0.0), &drawn);
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
/// What of a line is selected, and the colour to lay behind it.
#[derive(Clone, Copy, Debug)]
pub struct Selected {
    /// The bytes of the line, as the writer typed them: a stretched line is mapped on to.
    pub range: (usize, usize),
    pub color: Color32,
}

pub fn show(
    ui: &mut Ui,
    text: &str,
    pieces: &[Piece],
    size: f32,
    max_width: f32,
    page: Option<notecraft_model::Dir>,
    justify: Option<Strength>,
    selected: Option<Selected>,
) -> Rect {
    // A justified paragraph is drawn from its stretched words, but it is still the words
    // the writer typed that a screen reader is told and that the page file holds: a tatweel
    // is the shape of a join, not a letter anybody meant to write.
    // A justified line is drawn from its stretched words, so what is selected has to be
    // moved on to them the same way the formats are.
    let (long, drawn, used) = match justify {
        Some(strength) => {
            let (long, moved) = stretched(ui, text, pieces, size, max_width, strength);
            let (drawn, used) = runs(ui, &long, &moved, size, max_width);
            (long, drawn, used)
        }
        None => {
            let (drawn, used) = runs(ui, text, pieces, size, max_width);
            (text.to_string(), drawn, used)
        }
    };
    let full = vec2(max_width, used.y.max(size));
    let (rect, resp) = ui.allocate_exact_size(full, egui::Sense::hover());
    let enabled = ui.is_enabled();
    let said = spoken(text);
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &said));
    let origin = if notecraft_model::direction_in(text, page).is_rtl() { rect.min + vec2((rect.width() - used.x).max(0.0), 0.0) } else { rect.min };
    // Behind the words, never over them: the fill is opaque.
    if let Some(sel) = selected {
        let (a, b) = (moved_to(text, &long, sel.range.0), moved_to(text, &long, sel.range.1));
        for band in selection_in(&long, &drawn, a, b, size, space_width(ui, size)) {
            ui.painter().rect_filled(band.translate(origin.to_vec2()), 1.0, sel.color);
        }
    }
    paint(ui, origin, &drawn);
    rect
}

/// Where a byte of `text` ended up in `stretched`, which is `text` with tatweels put in.
/// The two are the same string when nothing was stretched, and then this is the identity.
fn moved_to(text: &str, stretched: &str, at: usize) -> usize {
    if text.len() == stretched.len() {
        return at;
    }
    let (mut a, mut b) = (text.chars().peekable(), stretched.chars());
    let (mut seen, mut extra) = (0usize, 0usize);
    for c in b.by_ref() {
        if seen >= at {
            break;
        }
        if a.peek() == Some(&c) {
            seen += a.next().map(char::len_utf8).unwrap_or(0);
        } else {
            extra += c.len_utf8();
        }
    }
    at + extra
}

/// A justified paragraph, ready to draw: its words with tatweels put into the joins, so
/// every line but the last reaches the column.
///
/// This is where `notecraft_text::kashida` meets the page. Only the drawing is stretched -
/// the page file keeps the words as they were typed - so this runs on the way to the
/// painter, every time the paragraph is drawn, which is why the answer is remembered
/// below.
///
/// **What is measured.** The joins are chosen from the line's words, but how much a
/// candidate is worth is a question for the painter, not for parley: an egui galley is what
/// ends up on the page. So a candidate is laid out the way the line will be drawn. Laid out
/// in the column the painter reports the column's own width, so the number of lines is what
/// says whether the stretched line still fits; the ink width comes from laying the same
/// line out with nothing to wrap against.
///
/// **What it does not know.** The candidate is measured in the paragraph's own type, not in
/// the mixture of faces a line of code or maths inside it would bring. parley breaks the
/// lines from the size alone, so that is the same answer the line break itself was made on,
/// but a line with a run of monospace in it may stop a little short of the column.
pub fn stretched(ui: &Ui, text: &str, pieces: &[Piece], size: f32, max_width: f32, strength: Strength) -> (String, Vec<Piece>) {
    let out = remembered(ui, text, size, max_width, strength);
    if out == text {
        return (out, pieces.to_vec());
    }
    let moved = moved_pieces(pieces, text, &out);
    (out, moved)
}

/// How many stretched paragraphs are kept. A page shows a few dozen at once; past that the
/// oldest answers are dropped rather than grown into.
const REMEMBERED: usize = 256;

thread_local! {
    /// The stretched text of a paragraph, against the paragraph, the type size, the
    /// column it was stretched into and the strength it was stretched to. Stretching walks the joins of a line one tatweel at a
    /// time and lays the line out again for each, which is far too much work to do on every
    /// frame of a window that is only being scrolled.
    static STRETCHED: RefCell<Vec<((String, u32, u32, Strength), String)>> = const { RefCell::new(Vec::new()) };
}

/// The stretched paragraph, from the memory if it is there and worked out if it is not.
fn remembered(ui: &Ui, text: &str, size: f32, max_width: f32, strength: Strength) -> String {
    let key = (text.to_string(), size.to_bits(), max_width.to_bits(), strength);
    let known = STRETCHED.with(|c| c.try_borrow().ok().and_then(|c| c.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone())));
    if let Some(known) = known {
        return known;
    }
    let out = stretch(ui, text, size, max_width, strength);
    STRETCHED.with(|c| {
        if let Ok(mut c) = c.try_borrow_mut() {
            if c.len() >= REMEMBERED {
                c.clear();
            }
            c.push((key, out.clone()));
        }
    });
    out
}

/// Stretches every line of the paragraph but the last to `max_width`.
fn stretch(ui: &Ui, text: &str, size: f32, max_width: f32, strength: Strength) -> String {
    let Some(split) = split(text, size, max_width) else { return text.to_string() };
    let lines = split.lines;
    // One line is all last line, and the last line of a paragraph is never justified: a
    // closing half-sentence pulled out to the margin is the mark of a bad typesetter.
    if lines.len() < 2 || !max_width.is_finite() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + lines.len() * 8);
    let mut at = 0usize;
    for (n, line) in lines.iter().enumerate() {
        let range = line.text.start.max(at)..line.text.end.min(text.len());
        if range.start > range.end {
            continue;
        }
        // Anything the lines do not cover is copied across untouched.
        let Some(before) = text.get(at..range.start) else { continue };
        out.push_str(before);
        at = range.end;
        let Some(whole) = text.get(range) else { continue };
        // The spaces and the break at the end of a line are not part of its words.
        let core = whole.trim_end();
        let tail = whole.get(core.len()..).unwrap_or("");
        if n + 1 == lines.len() || core.is_empty() {
            out.push_str(whole);
            continue;
        }
        out.push_str(&stretch_line(ui, core, size, max_width, strength));
        out.push_str(tail);
    }
    out.push_str(text.get(at..).unwrap_or(""));
    out
}

/// One line, stretched to the column by kashida.
fn stretch_line(ui: &Ui, core: &str, size: f32, max_width: f32, strength: Strength) -> String {
    let format = TextFormat { font_id: egui::FontId::proportional(size), ..TextFormat::default() };
    let mut width = |s: &str| -> f32 {
        let pieces = [(0..s.len(), format.clone())];
        // A stretch that wraps the line is no width at all, which is how the search stops.
        if split(s, size, max_width).is_none_or(|l| l.lines.len() > 1) {
            return f32::INFINITY;
        }
        runs(ui, s, &pieces, size, f32::INFINITY).1.x
    };
    notecraft_text::kashida::justify(core, max_width, strength, &mut width)
}

/// The same pieces against the stretched text: every range moved past the tatweels that
/// were put in before it.
///
/// `stretched` is `text` with tatweels inserted and nothing else changed, so walking the two
/// together says where each byte of the original went. A tatweel falls to the piece before
/// it, which is the piece of the letter whose join it lengthens.
fn moved_pieces(pieces: &[Piece], text: &str, stretched: &str) -> Vec<Piece> {
    let mut shift: Vec<usize> = Vec::with_capacity(text.len() + 1);
    let (mut a, mut b) = (text.chars().peekable(), stretched.chars());
    let mut extra = 0usize;
    for c in b.by_ref() {
        if a.peek() == Some(&c) {
            let used = a.next().map(char::len_utf8).unwrap_or(0);
            for _ in 0..used {
                shift.push(extra);
            }
        } else {
            extra += c.len_utf8();
        }
    }
    shift.push(extra);
    let at = |x: usize| x + shift.get(x).copied().unwrap_or(extra);
    pieces.iter().map(|(r, f)| (at(r.start)..at(r.end), f.clone())).collect()
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
    selection_in(text, &drawn, from, to, size, space_width(ui, size))
}

fn selection_in(text: &str, drawn: &[Drawn], from: usize, to: usize, size: f32, space: f32) -> Vec<Rect> {
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
    joined(out, space)
}

/// The rectangles of a line joined into one band where they are only a space apart.
///
/// A line that reads right to left is drawn word by word, with the gaps between the words
/// measured and left empty rather than kept inside a galley, so a selection across it comes
/// back as a rectangle per word and is drawn as a row of separate blocks with the spaces
/// showing through. OneNote, Word and a browser all draw one unbroken band, and so should
/// this: the owner put the two side by side on 2026-10-19 and the difference is plain.
///
/// Only a gap no wider than a space is closed. A wider one is left alone, because on a line
/// that mixes the two directions the selected text really is in two places and a band drawn
/// across the middle would cover words that are not selected.
fn joined(mut rects: Vec<Rect>, space: f32) -> Vec<Rect> {
    rects.sort_by(|a, b| a.min.y.total_cmp(&b.min.y).then(a.min.x.total_cmp(&b.min.x)));
    let mut out: Vec<Rect> = Vec::new();
    for r in rects {
        match out.last_mut() {
            // The same line, and close enough that what lies between is a space.
            Some(last) if (last.min.y - r.min.y).abs() < 1.0 && r.min.x - last.max.x <= space * 1.5 => {
                *last = Rect::from_min_max(last.min, pos2(last.max.x.max(r.max.x), last.max.y.max(r.max.y)));
            }
            _ => out.push(r),
        }
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
/// The caret is kept as a byte offset and nothing else, which is what makes this harder
/// than asking parley. parley's own `next_visual` is sensitive to affinity, meaning which
/// side of a direction boundary the caret is on, and a byte offset cannot carry that, so a step
/// built on it reads the wrong side back and the caret ping-pongs. Measured on 2026-10-19:
/// holding the right arrow in `السعر 123 ريال` cycled bytes 12, 13 and 14 for ever and the
/// caret never left the digits. The owner reported it as the caret jumping the wrong way
/// and skipping.
///
/// So a step is taken by where the caret is drawn rather than by what parley calls next:
/// the nearest stop on the same line that is further along in the direction asked for.
/// That is what "as the eye moves" means, it cannot cycle because every step moves strictly
/// one way across the line, and it needs no affinity. A step off the end of a line has
/// nowhere further to go on it, and parley answers that one, which is the case affinity
/// does not arise in.
///
/// A stop is a boundary between letters: the insides of a letter are left out, so a step
/// never lands between a letter and the mark that hangs on it.
pub fn step(text: &str, at: usize, right: bool, size: f32, max_width: f32) -> usize {
    TEXT.with(|t| {
        let Ok(mut engine) = t.try_borrow_mut() else { return at };
        let layout = engine.layout(text, size, Some(max_width));
        let place = |b: usize| {
            let r = notecraft_text::caret_rect(&layout, notecraft_text::Caret::at(b), 1.0);
            (r.x0, r.y0)
        };
        let (here_x, here_y) = place(at);

        // The nearest stop that is further along the line in the direction asked for. Only
        // stops on the same line are considered: a step off the end of one is a change of
        // line, which parley works out below.
        let mut best: Option<(f32, usize)> = None;
        for stop in stops(text) {
            if stop == at {
                continue;
            }
            let (x, y) = place(stop);
            if (y - here_y).abs() > 1.0 {
                continue;
            }
            let ahead = if right { x > here_x + 0.5 } else { x < here_x - 0.5 };
            if !ahead {
                continue;
            }
            let away = (x - here_x).abs();
            if best.is_none_or(|(near, _)| away < near) {
                best = Some((away, stop));
            }
        }
        if let Some((_, stop)) = best {
            return stop;
        }

        // Nothing further along this line: the caret is at one end of it, so parley says
        // where the next line begins. The marks are stepped past the same way as before.
        let mut now = at;
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

/// Every place on the line the caret may stand: the boundaries between letters, with the
/// insides of a letter left out so a step never lands between a letter and its mark.
fn stops(text: &str) -> Vec<usize> {
    let mut out: Vec<usize> = (0..=text.len()).filter(|b| text.is_char_boundary(*b) && !inside_a_letter(text, *b)).collect();
    out.dedup();
    out
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
        assert!(paints("שלום עולם", Some(notecraft_model::Dir::Ltr)), "Hebrew words are ordered here whatever the page says");
        assert!(paints("שלום עולם", None), "and on a page that says nothing");
        assert!(paints("123", Some(notecraft_model::Dir::Rtl)), "a line with no letters follows the page");
        assert!(!paints("123", Some(notecraft_model::Dir::Ltr)));
        // A page that was given a direction gives it to its paragraphs, so English on a
        // right-to-left page hangs from the right edge and is drawn here rather than by
        // egui. Until the page direction was fixed this line said the opposite, because
        // the words beat the page and the page setting did nothing.
        assert!(paints("Hello world", Some(notecraft_model::Dir::Rtl)), "a left-to-right line follows the page it is on");
        assert!(!paints("Hello world", None), "and on a page that says nothing it is egui's, as it always was");
        assert!(!paints("Hello world", Some(notecraft_model::Dir::Ltr)));
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

    /// A selection is drawn as one unbroken band to a line, not a block to a word.
    ///
    /// A right-to-left line is drawn word by word with the gaps measured and left empty, so
    /// a selection across it starts life as a rectangle per word. Painted like that it
    /// shows the page through every space, which is not what OneNote, Word or a browser
    /// draw and not what the owner saw beside them on 2026-10-19. The rectangles of a line
    /// are joined where the gap between them is only a space wide.
    #[test]
    fn a_selection_is_one_band_to_a_line_and_covers_the_spaces() {
        let t = Theme::DEFAULT.tokens();
        let ctx = ctx_with_fonts();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let pieces = plain(PRICE, 16.0, &t);
            // Nothing selected, nothing painted.
            assert!(selection(ui, PRICE, &pieces, 4, 4, 16.0, 400.0).is_empty());

            let (drawn, size) = runs(ui, PRICE, &pieces, 16.0, 400.0);
            assert!(drawn.len() > 2, "the line is drawn as several runs: {}", drawn.len());
            let all = selection(ui, PRICE, &pieces, 0, PRICE.len(), 16.0, 400.0);
            assert_eq!(all.len(), 1, "the whole of one line is one band: {all:?}");

            // The band runs the length of the drawn line, spaces and all, where the
            // rectangles it was made from together covered less.
            let left = drawn.iter().map(|d| d.at.x).fold(f32::INFINITY, f32::min);
            let right = drawn.iter().map(|d| d.at.x + d.galley.rect.width()).fold(0.0f32, f32::max);
            let band = all.first().copied().unwrap_or(Rect::ZERO);
            assert!(band.min.x <= left + 1.0 && band.max.x >= right - 1.0, "end to end: {band:?} against {left}..{right}");
            assert!(band.min.x >= -1.0 && band.max.x <= size.x + 1.0, "and inside the line: {band:?}");
            assert!(band.height() > 4.0);

            // A selection of the digits alone is still only the digits: joining a line's
            // rectangles must not reach past what was asked for.
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

    /// Joining closes a space and leaves a real gap alone.
    #[test]
    fn a_gap_wider_than_a_space_is_not_painted_over() {
        let line = |x0: f32, x1: f32, y: f32| Rect::from_min_max(pos2(x0, y), pos2(x1, y + 18.0));
        // Two words a space apart on one line: one band.
        assert_eq!(joined(vec![line(0.0, 40.0, 0.0), line(45.0, 90.0, 0.0)], 6.0).len(), 1);
        // The same two with a gap no space could fill: left as they are.
        assert_eq!(joined(vec![line(0.0, 40.0, 0.0), line(200.0, 240.0, 0.0)], 6.0).len(), 2);
        // Two lines, however close their edges: never joined to each other.
        assert_eq!(joined(vec![line(0.0, 40.0, 0.0), line(41.0, 90.0, 18.0)], 6.0).len(), 2);
        // The band spans from the first to the last, and keeps the taller height.
        let one = joined(vec![line(0.0, 40.0, 0.0), line(45.0, 90.0, 0.0)], 6.0);
        let band = one.first().copied().unwrap_or(Rect::ZERO);
        assert_eq!((band.min.x, band.max.x), (0.0, 90.0));
        assert!(joined(Vec::new(), 6.0).is_empty());
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
    /// A justified paragraph is drawn from stretched words, so the formats of its pieces
    /// have to move with them: a link or a word in bold must still be the link or the bold
    /// word after the tatweels go in, not the letters beside it.
    #[test]
    fn the_formats_move_with_the_words_they_belong_to() {
        let t = Theme::DEFAULT.tokens();
        // Two pieces: the first word, then the rest of the line.
        let text = "مسار كتاب";
        let split_at = "مسار".len();
        let one = TextFormat { font_id: egui::FontId::proportional(16.0), color: t.strong, ..TextFormat::default() };
        let two = TextFormat { font_id: egui::FontId::proportional(16.0), color: t.link, ..TextFormat::default() };
        let pieces = vec![(0..split_at, one), (split_at..text.len(), two)];

        // What the painter would get back after a stretch that put one tatweel in each word.
        let stretched = "مسـار كتـاب";
        let moved = moved_pieces(&pieces, text, stretched);
        assert_eq!(moved.len(), 2);
        let first = stretched.get(moved[0].0.clone()).unwrap_or_default();
        let second = stretched.get(moved[1].0.clone()).unwrap_or_default();
        assert_eq!(first, "مسـار", "the first word, its own tatweel included");
        assert_eq!(second, " كتـاب", "and the rest of the line");
        assert_eq!(format!("{first}{second}"), stretched, "the pieces cover the stretched line exactly once");
        assert_eq!(moved[0].1.color, t.strong, "and each keeps its own format");
        assert_eq!(moved[1].1.color, t.link);

        // A line that was not stretched at all comes back as it was.
        assert_eq!(moved_pieces(&pieces, text, text), pieces);
    }
}

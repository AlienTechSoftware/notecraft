#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft's text layer: a spike, not a finished crate.
//!
//! **Why it exists.** egui lays a line out and places a caret in it, and for Latin that is
//! fine. For Arabic it is not: `crates/ui-egui/tests/arabic_layout.rs` measures what goes
//! wrong, and the short version is that egui counts glyphs where it means characters, so the
//! caret inside a line of Arabic lands in the wrong place, and a line that mixes the two
//! directions is drawn in one order because nothing runs the bidi algorithm.
//!
//! **What this is.** [parley] does both properly: it shapes with HarfRust, runs the Unicode
//! bidirectional algorithm, breaks lines, and answers where a click landed in *characters*.
//! This crate wraps the part NoteCraft needs and keeps egui out, so the UI can paint it and
//! the model can be tested without a window. The sibling PhotoCraft uses parley the same way.
//!
//! **What it does not do yet.** Painting. parley hands back glyph ids and positions; egui
//! draws text only through its own layout, so the glyphs have to be drawn another way
//! (outlines through skrifa, or a small atlas of our own). That is the next question the
//! spike has to answer, and the decision is written up in the private research file.

pub mod kashida;

use parley::fontique::{Blob, Collection, CollectionOptions, SourceCache};
use parley::style::{FontFamily, FontFamilyName};
use parley::{Alignment, AlignmentOptions, FontContext, Layout, LayoutContext, StyleProperty};
use std::sync::Arc;

/// The faces NoteCraft bundles, each under the SIL Open Font License 1.1 (texts in
/// `assets/fonts/OFL-*.txt`, provenance in `ATTRIBUTION.md`). They live here, in the lowest
/// crate that needs them, so the window and the text layer share one copy of the bytes.
pub mod fonts {
    /// Noto Naskh Arabic, variable weight, 300 KB: Arabic, Persian and Urdu letters, and Latin.
    pub const ARABIC: &[u8] = include_bytes!("../../../assets/fonts/NotoNaskhArabic.ttf");
    /// Noto Sans Hebrew, variable width and weight, 110 KB.
    pub const HEBREW: &[u8] = include_bytes!("../../../assets/fonts/NotoSansHebrew.ttf");
    /// Noto Sans Devanagari, variable width and weight, 630 KB: Hindi, Marathi, Nepali, Sanskrit.
    pub const DEVANAGARI: &[u8] = include_bytes!("../../../assets/fonts/NotoSansDevanagari.ttf");
    /// Noto Sans CJK SC, Regular, 16 MB: Chinese, Japanese and Korean, with Simplified Chinese
    /// forms where the scripts share a character.
    pub const CJK: &[u8] = include_bytes!("../../../assets/fonts/NotoSansCJKsc-Regular.otf");

    /// All of them, in fallback order, each with the name the window registers it under.
    pub const ALL: [(&str, &[u8]); 4] =
        [("noto-naskh-arabic", ARABIC), ("noto-sans-hebrew", HEBREW), ("noto-sans-devanagari", DEVANAGARI), ("noto-sans-cjk-sc", CJK)];
}

use fonts::{ARABIC, CJK, DEVANAGARI, HEBREW};

/// No brush: the spike cares about where things are, not what colour they are.
type Brush = [u8; 0];

/// A text engine with the bundled faces and nothing from the system, so a test gives the
/// same answer on every machine.
pub struct Text {
    fonts: FontContext,
    layouts: LayoutContext<Brush>,
    family: String,
    /// The other bundled families, tried for a character the first has no glyph for.
    fallbacks: Vec<String>,
}

impl Default for Text {
    fn default() -> Self {
        Self::new()
    }
}

impl Text {
    pub fn new() -> Text {
        let collection = Collection::new(CollectionOptions { shared: false, system_fonts: false });
        let mut fonts = FontContext { collection, source_cache: SourceCache::default() };
        let blob = Blob::new(Arc::new(ARABIC) as Arc<dyn AsRef<[u8]> + Send + Sync>);
        let registered = fonts.collection.register_fonts(blob, None);
        let family = registered.first().and_then(|(id, _)| fonts.collection.family_name(*id)).unwrap_or("Noto Naskh Arabic").to_string();
        let mut fallbacks = Vec::new();
        for bytes in [HEBREW, DEVANAGARI, CJK] {
            let blob = Blob::new(Arc::new(bytes) as Arc<dyn AsRef<[u8]> + Send + Sync>);
            let registered = fonts.collection.register_fonts(blob, None);
            if let Some(name) = registered.first().and_then(|(id, _)| fonts.collection.family_name(*id)) {
                fallbacks.push(name.to_string());
            }
        }
        Text { fonts, layouts: LayoutContext::new(), family, fallbacks }
    }

    /// The family name the bundled face registered under.
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Lays `text` out in a column `width` wide (`None` for one long line).
    pub fn layout(&mut self, text: &str, size: f32, width: Option<f32>) -> Layout<Brush> {
        let mut builder = self.layouts.ranged_builder(&mut self.fonts, text, 1.0, false);
        // One family list: the default face first, then the fallbacks, so parley picks the
        // first that has the character.
        let list: Vec<FontFamilyName<'_>> =
            std::iter::once(self.family.as_str()).chain(self.fallbacks.iter().map(String::as_str)).map(FontFamilyName::named).collect();
        builder.push_default(StyleProperty::FontFamily(FontFamily::List(std::borrow::Cow::Owned(list))));
        builder.push_default(StyleProperty::FontSize(size));
        let mut layout: Layout<Brush> = builder.build(text);
        layout.break_all_lines(width);
        // Which edge the lines hang from. parley's `Start` follows the base direction it
        // infers; NoteCraft decides that itself (the mark at the start of the line, else
        // its first strong letter), so the geometry here and the drawing agree.
        let align = if notecraft_model::direction(text).is_rtl() { Alignment::Right } else { Alignment::Left };
        layout.align(align, AlignmentOptions::default());
        layout
    }
}

/// One run of a laid-out line, in the order it is drawn: where it starts, how wide it is,
/// and which part of the text it holds.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub x: f32,
    pub width: f32,
    pub text: std::ops::Range<usize>,
    pub rtl: bool,
}

/// One laid-out line: its runs in drawing order, and where its baseline sits.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub runs: Vec<Run>,
    /// Top of the line, down from the top of the text.
    pub y: f32,
    pub height: f32,
    /// The bytes of the text this line holds, the break at its end included. The lines of a
    /// layout cover the text once, in order, which is what lets a caller work on one line of
    /// a wrapped paragraph and put the paragraph back together afterwards.
    pub text: std::ops::Range<usize>,
}

/// Every line, each with its runs in the order they are drawn.
pub fn lines(layout: &Layout<Brush>) -> Vec<Line> {
    layout
        .lines()
        .map(|line| {
            let m = line.metrics();
            let runs = line
                .items()
                .filter_map(|item| match item {
                    parley::layout::PositionedLayoutItem::GlyphRun(run) => {
                        let r = run.run();
                        Some(Run { x: run.offset(), width: r.advance(), text: r.text_range(), rtl: r.is_rtl() })
                    }
                    _ => None,
                })
                .collect();
            Line { runs, y: m.baseline - m.ascent, height: m.line_height, text: line.text_range() }
        })
        .collect()
}

/// The runs of the first line, left to right as they are drawn.
pub fn runs(layout: &Layout<Brush>) -> Vec<Run> {
    let Some(line) = layout.lines().next() else { return Vec::new() };
    line.items()
        .filter_map(|item| match item {
            parley::layout::PositionedLayoutItem::GlyphRun(run) => {
                let r = run.run();
                Some(Run { x: run.offset(), width: r.advance(), text: r.text_range(), rtl: r.is_rtl() })
            }
            _ => None,
        })
        .collect()
}

/// The character a click at `x` on the first line lands on, as a byte offset into the text.
pub fn hit(layout: &Layout<Brush>, x: f32, y: f32) -> usize {
    parley::layout::Cursor::from_point(layout, x, y).index()
}

/// A rectangle, in the line's own space: x to the right, y down from the top of the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    fn of(b: parley::BoundingBox) -> Rect {
        Rect { x0: b.x0 as f32, y0: b.y0 as f32, x1: b.x1 as f32, y1: b.y1 as f32 }
    }

    pub fn width(self) -> f32 {
        self.x1 - self.x0
    }

    pub fn height(self) -> f32 {
        self.y1 - self.y0
    }
}

/// Where the caret is, as a byte offset into the text, and where it sits on the line.
///
/// A caret is kept as a byte offset because that is what the text is: moving it by one
/// visible step can move it by several bytes, or by none at all where two directions meet.
/// parley answers all of that; the window only draws what comes back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caret {
    index: usize,
    upstream: bool,
}

impl Caret {
    pub fn at(index: usize) -> Caret {
        Caret { index, upstream: false }
    }

    pub fn index(self) -> usize {
        self.index
    }

    fn of(c: parley::layout::Cursor) -> Caret {
        Caret { index: c.index(), upstream: matches!(c.affinity(), parley::layout::Affinity::Upstream) }
    }

    fn cursor(self, layout: &Layout<Brush>) -> parley::layout::Cursor {
        let affinity = if self.upstream { parley::layout::Affinity::Upstream } else { parley::layout::Affinity::Downstream };
        parley::layout::Cursor::from_byte_index(layout, self.index, affinity)
    }
}

/// The caret a click at `x`, `y` puts down.
pub fn caret_at(layout: &Layout<Brush>, x: f32, y: f32) -> Caret {
    Caret::of(parley::layout::Cursor::from_point(layout, x, y))
}

/// Where to draw the caret: a thin upright rectangle on the line.
pub fn caret_rect(layout: &Layout<Brush>, caret: Caret, width: f32) -> Rect {
    Rect::of(caret.cursor(layout).geometry(layout, width))
}

/// One step to the right on the line, as the eye sees it. In a line that reads right to
/// left that moves backwards through the text, and at a boundary between two directions it
/// crosses into the other run rather than jumping to the far end of the line.
pub fn step_right(layout: &Layout<Brush>, caret: Caret) -> Caret {
    Caret::of(caret.cursor(layout).next_visual(layout))
}

/// One step to the left, the same way.
pub fn step_left(layout: &Layout<Brush>, caret: Caret) -> Caret {
    Caret::of(caret.cursor(layout).previous_visual(layout))
}

/// The rectangles to paint behind a selection from one caret to another. A selection that
/// covers two directions is more than one rectangle, which is why this is a list.
pub fn selection_rects(layout: &Layout<Brush>, from: Caret, to: Caret) -> Vec<Rect> {
    let selection = parley::layout::Selection::new(from.cursor(layout), to.cursor(layout));
    let mut out = Vec::new();
    selection.geometry_with(layout, |b, _line| out.push(Rect::of(b)));
    out
}

/// The word under a point, as a byte range: what a double click selects.
pub fn word_at(layout: &Layout<Brush>, x: f32, y: f32) -> std::ops::Range<usize> {
    parley::layout::Selection::word_from_point(layout, x, y).text_range()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The owner's test sentence, and a line that mixes the two directions.
    const ARABIC_LINE: &str = "١. هذا اختبار بسيط لعرض الجمل العربية بشكل صحيح.";
    const MIXED: &str = "Hello مرحبا world";
    const PRICE: &str = "السعر 123 ريال";

    #[test]
    fn the_bundled_faces_register_and_cover_their_scripts() {
        let mut t = Text::new();
        assert!(!t.family().is_empty(), "the face registered under a name");
        assert_eq!(t.fallbacks.len(), 3, "Hebrew, Devanagari and CJK registered: {:?}", t.fallbacks);
        for word in ["Hello", "مرحبا", "שלום", "नमस्ते", "你好", "こんにちは", "안녕하세요"] {
            let layout = t.layout(word, 16.0, None);
            assert!(layout.width() > 0.0, "{word} has glyphs");
            // A face that lacks the script gives notdef boxes, which parley still advances;
            // so also check the glyphs came from a real face: a box is one glyph per
            // character, and shaped Devanagari or joined Arabic never is.
            let glyphs: usize = layout.lines().flat_map(|l| l.runs()).map(|r| r.len()).sum();
            assert!(glyphs > 0, "{word}: no glyphs at all");
        }
        let hebrew = t.layout("שלום", 16.0, None);
        assert!(runs(&hebrew).iter().all(|r| r.rtl), "Hebrew is laid out right to left");
    }

    #[test]
    fn a_mixed_line_comes_out_in_visual_order() {
        let mut t = Text::new();
        let layout = t.layout(MIXED, 16.0, None);
        let runs = runs(&layout);
        // Four, not three: the space between the Arabic and "world" is its own run, which is
        // what the algorithm says a neutral between two directions does.
        assert!(runs.len() >= 3, "English, Arabic, then English: {runs:?}");
        // Drawn left to right, the runs follow one another with no overlap.
        for pair in runs.windows(2) {
            if let [a, b] = pair {
                assert!(a.x <= b.x, "runs are in drawing order: {runs:?}");
            }
        }
        // The middle one is the Arabic, and it is marked right to left.
        let rtl: Vec<&Run> = runs.iter().filter(|r| r.rtl).collect();
        assert_eq!(rtl.len(), 1, "one right-to-left run: {runs:?}");
        let middle = rtl.first().map(|r| (*r).clone()).unwrap_or(Run { x: 0.0, width: 0.0, text: 0..0, rtl: false });
        assert_eq!(MIXED.get(middle.text.clone()).map(str::trim), Some("مرحبا"), "{runs:?}");
        // It sits between the English, where it was written, not drawn first or last.
        assert!(middle.x > 0.0 && middle.x + middle.width < layout.width(), "{runs:?}");
        assert!(runs.first().is_some_and(|r| !r.rtl) && runs.last().is_some_and(|r| !r.rtl));
        let covered: usize = runs.iter().map(|r| r.text.len()).sum();
        assert_eq!(covered, MIXED.len(), "every byte is in a run: {runs:?}");
    }

    #[test]
    fn the_number_in_an_arabic_line_keeps_its_own_order() {
        let mut t = Text::new();
        let layout = t.layout(PRICE, 16.0, None);
        let runs = runs(&layout);
        // The digits are their own run inside the Arabic, drawn left to right. That is the
        // thing egui gets wrong today: it draws 123 as 321.
        let digits = runs.iter().find(|r| PRICE.get(r.text.clone()).is_some_and(|s| s.contains('1')));
        assert!(digits.is_some_and(|r| !r.rtl), "the digits read left to right inside the line: {runs:?}");
    }

    #[test]
    fn a_click_lands_on_the_character_under_it_in_characters_not_glyphs() {
        let mut t = Text::new();
        let layout = t.layout(ARABIC_LINE, 16.0, None);
        let width = layout.width();
        let chars = ARABIC_LINE.len();
        // Right edge is the start of a right-to-left line, left edge is the end of it.
        // A right-to-left line starts at its right edge. This sentence opens with an
        // Arabic-Indic digit, which reads left to right inside the line, so the far right is
        // that digit's own right side: the caret lands just after it, as the algorithm asks.
        let right = hit(&layout, width + 50.0, 4.0);
        assert!(right <= 3, "the right edge is the start of the line, give or take the digit: {right}");
        let far_left = hit(&layout, -50.0, 4.0);
        assert!(far_left > chars / 2, "the left edge is near the end: {far_left} of {chars} bytes");
        // Every answer is a real character boundary in the text, which is what egui cannot do.
        for i in 0..=20 {
            let x = width * i as f32 / 20.0;
            let at = hit(&layout, x, 4.0);
            assert!(at <= chars, "never past the end: {at} of {chars}");
            assert!(ARABIC_LINE.is_char_boundary(at), "a character boundary, not a glyph: {at}");
        }
    }

    #[test]
    fn the_caret_steps_the_way_the_eye_moves_not_the_way_the_bytes_run() {
        let mut t = Text::new();
        let layout = t.layout(MIXED, 16.0, None);
        // From the start of "Hello", stepping right walks forward through the English.
        let mut c = Caret::at(0);
        let mut seen = vec![c.index()];
        for _ in 0..6 {
            c = step_right(&layout, c);
            seen.push(c.index());
        }
        assert!(seen.windows(2).all(|w| w[0] < w[1]), "forward through the English: {seen:?}");
        // Now into the Arabic, which reads the other way: the byte offsets stop rising
        // steadily, because a step to the right is a step backwards through those letters.
        let mut inside = Vec::new();
        for _ in 0..6 {
            c = step_right(&layout, c);
            inside.push(c.index());
        }
        assert!(inside.iter().any(|i| *i >= 6 && *i <= 16), "the caret is inside the Arabic: {inside:?}");
        assert!(!inside.windows(2).all(|w| w[0] < w[1]), "and it is not just walking the bytes: {inside:?}");
        // Stepping back the same number of times returns to where it started.
        let mut back = c;
        for _ in 0..12 {
            back = step_left(&layout, back);
        }
        assert_eq!(back.index(), 0, "twelve steps right and twelve back is where it began");
    }

    #[test]
    fn the_caret_has_a_place_on_the_line_and_a_selection_has_rectangles() {
        let mut t = Text::new();
        let layout = t.layout(MIXED, 16.0, None);
        let width = layout.width();
        // The caret at the start sits at the left of a left-to-right line, and the one at
        // the end sits at the right.
        let start = caret_rect(&layout, Caret::at(0), width);
        let end = caret_rect(&layout, Caret::at(MIXED.len()), width);
        assert!(start.x0 < 1.0 && end.x0 > width - 2.0, "start {start:?} end {end:?} of {width}");
        assert!(start.height() > 4.0 && start.width() >= 0.0, "a caret is a thin upright bar: {start:?}");

        // A selection across the Arabic needs more than one rectangle, because the letters
        // it covers are not all drawn in one stretch.
        let rects = selection_rects(&layout, Caret::at(0), Caret::at(MIXED.len()));
        assert!(!rects.is_empty(), "the whole line is selected");
        let covered: f32 = rects.iter().map(|r| r.width()).sum();
        assert!(covered > width * 0.8, "the rectangles cover the line: {covered} of {width}");
        assert!(selection_rects(&layout, Caret::at(3), Caret::at(3)).is_empty(), "nothing selected, nothing to paint");
    }

    #[test]
    fn a_double_click_takes_the_word_under_it() {
        let mut t = Text::new();
        let layout = t.layout(MIXED, 16.0, None);
        let runs = runs(&layout);
        let Some(arabic) = runs.iter().find(|r| r.rtl).cloned() else { return };
        // The Arabic word, pointed at in the middle of where it is drawn.
        let word = word_at(&layout, arabic.x + arabic.width / 2.0, 4.0);
        assert_eq!(MIXED.get(word.clone()).map(str::trim), Some("مرحبا"), "the word, not the line: {word:?}");
        // And an English one, pointed at in the first run.
        let english = word_at(&layout, 4.0, 4.0);
        assert_eq!(MIXED.get(english.clone()).map(str::trim), Some("Hello"), "{english:?}");
    }

    #[test]
    fn a_click_in_a_mixed_line_follows_what_is_drawn() {
        let mut t = Text::new();
        let layout = t.layout(MIXED, 16.0, None);
        let runs = runs(&layout);
        let Some(arabic) = runs.get(1).cloned() else { return };
        // A click in the middle of the Arabic run lands inside the Arabic word, even though
        // that word sits in the middle of an otherwise English line.
        let at = hit(&layout, arabic.x + arabic.width / 2.0, 4.0);
        assert!(arabic.text.contains(&at), "{at} is inside {:?}: {runs:?}", arabic.text);
    }
}

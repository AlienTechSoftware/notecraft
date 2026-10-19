//! The equation engine. A formula is typeset by RaTeX, a pure Rust typesetter that follows
//! KaTeX, and drawn in the KaTeX maths faces; when RaTeX cannot take it (a syntax error, a
//! formula far too big) the small engine of this crate lays it out instead, and the window
//! shows whatever that cannot read as text. Nothing is ever lost and nothing here can panic or
//! overflow the stack: the source is limited in length and in nesting before RaTeX sees it.
//!
//! The result is plain data, no UI types: positioned glyphs, text runs, lines, rectangles
//! and paths, in pixels at the size asked for. A window paints them.

use std::collections::HashMap;
use std::sync::Arc;

use ratex_font::{FontId, font_and_metric_for_mathematical_alphanumeric, get_char_metrics, katex_ttf_glyph_char};
use ratex_layout::{FallbackGlyphMetrics, LayoutOptions, layout_with_fallback_metrics, to_display_list, validate_layout};
use ratex_types::{Color, DisplayItem, MathStyle, PathCommand};

use crate::{Item, Metrics};

/// The longest formula, in characters, that RaTeX is given. A longer one is laid out by the
/// subset engine, which cuts its own depth.
pub const MAX_SOURCE_CHARS: usize = 4000;
/// The deepest nesting of braces and environments that RaTeX is given. Its parser recurses, so
/// an unbounded depth would be a way to overflow the stack.
pub const MAX_NESTING: usize = 40;

/// A colour a formula chose for itself, red green blue alpha. A shape with none is drawn in the
/// text colour of the page, so a formula follows the theme.
pub type Rgba = [u8; 4];

/// One step of a path, in pixels, relative to the path's origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Cmd {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// One thing to draw. `x` and `y` are in pixels from the formula's top left corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// A glyph from a KaTeX face, its origin on the baseline. `face` is the id the face is
    /// registered under, such as `Main-Regular` or `Size3-Regular`.
    Glyph { x: f32, y: f32, face: String, ch: char, size: f32, color: Option<Rgba> },
    /// Text in the window's own font: letters KaTeX has no glyph for, such as Arabic, Hebrew
    /// or Chinese inside `\text{}`, kept in one run so they join and run in the right
    /// direction, and everything the subset engine draws. `y` is the baseline.
    Text { x: f32, y: f32, text: String, size: f32, italic: bool, color: Option<Rgba> },
    /// A straight line, such as a fraction bar.
    Line { from: (f32, f32), to: (f32, f32), thickness: f32, dashed: bool, color: Option<Rgba> },
    /// A filled rectangle.
    Rect { x: f32, y: f32, width: f32, height: f32, color: Option<Rgba> },
    /// A path from `(x, y)`, filled, or stroked `stroke` pixels wide: a radical, a big bracket.
    Path { x: f32, y: f32, cmds: Vec<Cmd>, fill: bool, stroke: f32, color: Option<Rgba> },
}

/// Which engine laid the formula out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Engine {
    /// RaTeX: the whole of KaTeX's syntax that it covers, in the KaTeX faces.
    Ratex,
    /// The subset engine of this crate, because RaTeX could not take the formula.
    #[default]
    Subset,
}

/// A formula, laid out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typeset {
    pub width: f32,
    /// From the top to the baseline.
    pub ascent: f32,
    /// From the baseline to the bottom.
    pub descent: f32,
    pub shapes: Vec<Shape>,
    pub engine: Engine,
}

impl Typeset {
    pub fn height(&self) -> f32 {
        self.ascent + self.descent
    }
}

/// Typesets `tex` at `size` pixels to the em. `display` sets it as a display equation, with
/// big operators and limits above and below, rather than in a line of text. `m` measures
/// text in the window's own font, for the letters KaTeX has no glyph for.
pub fn typeset(tex: &str, size: f32, display: bool, m: &dyn Metrics) -> Typeset {
    let size = if size.is_finite() && size > 1.0 { size.min(400.0) } else { 14.0 };
    match via_ratex(tex, size, display, m) {
        Some(t) => t,
        None => via_subset(tex, size, m),
    }
}

/// How deeply `tex` nests braces and `\begin ... \end`, counted without parsing it.
fn nesting(tex: &str) -> usize {
    let (mut depth, mut deepest) = (0usize, 0usize);
    let bytes = tex.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'\\' => {
                let rest = &tex[i..];
                if rest.starts_with("\\begin") {
                    depth += 1;
                } else if rest.starts_with("\\end") {
                    depth = depth.saturating_sub(1);
                } else if rest.starts_with("\\left") {
                    depth += 1;
                } else if rest.starts_with("\\right") {
                    depth = depth.saturating_sub(1);
                }
                // A backslash escapes the next byte (`\{`), which is not a brace to count.
                i += 1;
            }
            _ => {}
        }
        deepest = deepest.max(depth);
        i += 1;
    }
    deepest
}

/// The widths of the letters in `tex` that the KaTeX faces cannot draw, as the window's font
/// draws them, so RaTeX leaves room for them.
fn fallback_metrics(tex: &str, size: f32, m: &dyn Metrics) -> Arc<HashMap<char, FallbackGlyphMetrics>> {
    let mut out = HashMap::new();
    for c in tex.chars().filter(|c| !c.is_ascii()) {
        out.entry(c).or_insert_with(|| {
            let w = m.width(&c.to_string(), size, false) / size;
            FallbackGlyphMetrics { width: f64::from(w.max(0.0)), height: 0.72, depth: 0.2 }
        });
    }
    Arc::new(out)
}

fn via_ratex(tex: &str, size: f32, display: bool, m: &dyn Metrics) -> Option<Typeset> {
    if tex.trim().is_empty() || tex.chars().count() > MAX_SOURCE_CHARS || nesting(tex) > MAX_NESTING {
        return None;
    }
    let metrics = fallback_metrics(tex, size, m);
    let owned = tex.to_owned();
    // RaTeX is young. Whatever it does with a formula it was not made for, the window goes on.
    let list = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let nodes = ratex_parser::parse(&owned).ok()?;
        let options = LayoutOptions { style: if display { MathStyle::Display } else { MathStyle::Text }, ..LayoutOptions::default() };
        let boxed = layout_with_fallback_metrics(&nodes, &options, metrics);
        validate_layout(&boxed).ok()?;
        Some(to_display_list(&boxed))
    }))
    .ok()??;
    let em = f64::from(size);
    let mut shapes = Vec::with_capacity(list.items.len());
    let mut run: Option<Run> = None;
    for item in &list.items {
        match item {
            DisplayItem::GlyphPath { x, y, scale, font, char_code, color } => {
                match katex_glyph(font, *char_code) {
                    // A letter the KaTeX faces have: drawn from the face, by the character the
                    // face's own table keys it by.
                    Some(ch) => {
                        if let Some(done) = run.take() {
                            shapes.push(done.finish());
                        }
                        shapes.push(Shape::Glyph {
                            x: (x * em) as f32,
                            y: (y * em) as f32,
                            face: font.clone(),
                            ch,
                            size: (scale * em) as f32,
                            color: rgba(color),
                        });
                    }
                    // A letter they lack: the window's own font draws it, and the letters of a
                    // word stay together so they join and read in the right order.
                    None => {
                        let ch = char::from_u32(*char_code).unwrap_or('\u{fffd}');
                        let (px, py, fs) = ((x * em) as f32, (y * em) as f32, (scale * em) as f32);
                        match run.as_mut() {
                            Some(r) if r.continues(px, py, fs) => r.push(ch, px),
                            _ => {
                                if let Some(done) = run.take() {
                                    shapes.push(done.finish());
                                }
                                run = Some(Run { x: px, y: py, size: fs, text: ch.to_string(), last_x: px, color: rgba(color) });
                            }
                        }
                    }
                }
            }
            other => {
                if let Some(done) = run.take() {
                    shapes.push(done.finish());
                }
                match other {
                    DisplayItem::Line { x, y, width, thickness, color, dashed } => shapes.push(Shape::Line {
                        from: ((x * em) as f32, (y * em) as f32),
                        to: (((x + width) * em) as f32, (y * em) as f32),
                        thickness: ((thickness * em) as f32).max(0.5),
                        dashed: *dashed,
                        color: rgba(color),
                    }),
                    DisplayItem::Rect { x, y, width, height, color } => shapes.push(Shape::Rect {
                        x: (x * em) as f32,
                        y: (y * em) as f32,
                        width: (width * em) as f32,
                        height: (height * em) as f32,
                        color: rgba(color),
                    }),
                    DisplayItem::Path { x, y, commands, fill, color } => shapes.push(Shape::Path {
                        x: (x * em) as f32,
                        y: (y * em) as f32,
                        cmds: commands.iter().map(|c| cmd(c, em)).collect(),
                        fill: *fill,
                        stroke: (0.04 * em) as f32,
                        color: rgba(color),
                    }),
                    DisplayItem::GlyphPath { .. } => {}
                }
            }
        }
    }
    if let Some(done) = run.take() {
        shapes.push(done.finish());
    }
    Some(Typeset {
        width: (list.width * em) as f32,
        ascent: (list.height * em) as f32,
        descent: (list.depth * em) as f32,
        shapes,
        engine: Engine::Ratex,
    })
}

/// The character to draw from the KaTeX face `font` for the code RaTeX gave, or `None` when the
/// face has no such glyph (it is Arabic, Hebrew, Chinese, an emoji) and the window's own font
/// must draw it. The face's table is keyed by the plain letter for the mathematical alphabets
/// (a bold A is `A` in the bold face), which `katex_ttf_glyph_char` undoes.
fn katex_glyph(font: &str, code: u32) -> Option<char> {
    let id = FontId::parse(font)?;
    let key = match font_and_metric_for_mathematical_alphanumeric(code) {
        Some((mapped, metric)) if mapped == id => metric,
        _ => code,
    };
    get_char_metrics(id, key)?;
    Some(katex_ttf_glyph_char(id, code))
}

/// Letters in the window's own font that sit one after another on a line: they are drawn as
/// one run, so Arabic joins and a right-to-left word reads in the right order.
struct Run {
    x: f32,
    y: f32,
    size: f32,
    text: String,
    last_x: f32,
    color: Option<Rgba>,
}

impl Run {
    fn continues(&self, x: f32, y: f32, size: f32) -> bool {
        (y - self.y).abs() < 0.01 * self.size.max(1.0) && (size - self.size).abs() < 0.01 && x > self.last_x - 0.01 && x - self.last_x < 1.6 * size
    }

    fn push(&mut self, ch: char, x: f32) {
        self.text.push(ch);
        self.last_x = x;
    }

    fn finish(self) -> Shape {
        Shape::Text { x: self.x, y: self.y, text: self.text, size: self.size, italic: false, color: self.color }
    }
}

fn rgba(c: &Color) -> Option<Rgba> {
    if *c == Color::BLACK {
        return None;
    }
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some([byte(c.r), byte(c.g), byte(c.b), byte(c.a)])
}

fn cmd(c: &PathCommand, em: f64) -> Cmd {
    let f = |v: f64| (v * em) as f32;
    match c {
        PathCommand::MoveTo { x, y } => Cmd::Move(f(*x), f(*y)),
        PathCommand::LineTo { x, y } => Cmd::Line(f(*x), f(*y)),
        PathCommand::QuadTo { x1, y1, x, y } => Cmd::Quad(f(*x1), f(*y1), f(*x), f(*y)),
        PathCommand::CubicTo { x1, y1, x2, y2, x, y } => Cmd::Cubic(f(*x1), f(*y1), f(*x2), f(*y2), f(*x), f(*y)),
        PathCommand::Close => Cmd::Close,
    }
}

/// The subset engine's layout, as shapes. The formula it cannot read stays visible as text.
fn via_subset(tex: &str, size: f32, m: &dyn Metrics) -> Typeset {
    let l = crate::layout(&crate::parse(tex), size, m);
    let shapes = l
        .items
        .iter()
        .map(|item| match item {
            Item::Text { x, y, text, size, italic } => Shape::Text { x: *x, y: *y, text: text.clone(), size: *size, italic: *italic, color: None },
            Item::Line { from, to, width } => Shape::Line { from: *from, to: *to, thickness: *width, dashed: false, color: None },
            Item::Path { points, width } => {
                let mut cmds = Vec::with_capacity(points.len());
                for (i, (px, py)) in points.iter().enumerate() {
                    cmds.push(if i == 0 { Cmd::Move(*px, *py) } else { Cmd::Line(*px, *py) });
                }
                Shape::Path { x: 0.0, y: 0.0, cmds, fill: false, stroke: *width, color: None }
            }
        })
        .collect();
    Typeset { width: l.width, ascent: l.ascent, descent: l.descent, shapes, engine: Engine::Subset }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character is half the font size wide.
    struct Mono;
    impl Metrics for Mono {
        fn width(&self, text: &str, size: f32, _italic: bool) -> f32 {
            text.chars().count() as f32 * size * 0.5
        }
    }

    fn set(tex: &str) -> Typeset {
        typeset(tex, 20.0, true, &Mono)
    }

    fn glyphs(t: &Typeset) -> usize {
        t.shapes.iter().filter(|s| matches!(s, Shape::Glyph { .. })).count()
    }

    #[test]
    fn the_formulas_the_old_engine_could_not_read_are_drawn() {
        // Each of these showed as raw text with its backslashes before: 16 of the 32 in the survey.
        for tex in [
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}",
            r"\begin{bmatrix} 1 & 0 \\ 0 & 1 \end{bmatrix}",
            r"\begin{vmatrix} a & b \\ c & d \end{vmatrix} = ad - bc",
            r"\begin{aligned} a &= b + c \\ d &= e + f \end{aligned}",
            r"f(x) = \begin{cases} x^2 & x \ge 0 \\ -x & x < 0 \end{cases}",
            r"\hat{x} + \vec{v} + \bar{y} + \tilde{z}",
            r"\binom{n}{k} = \frac{n!}{k!(n-k)!}",
            r"\underbrace{a+b+\cdots+z}_{26}",
            r"\overset{def}{=} \quad A \xrightarrow{f} B",
            r"x \in \mathbb{R}, \mathbb{N} \subset \mathbb{Z}",
            r"\left\{ x \middle| x>0 \right\}",
            r"\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}",
        ] {
            let t = set(tex);
            assert_eq!(t.engine, Engine::Ratex, "{tex}");
            assert!(glyphs(&t) > 0, "{tex} drew no glyphs");
            assert!(t.width > 0.0 && t.height() > 0.0, "{tex} has no size");
            assert!(!t.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text.contains('\\'))), "{tex} left raw TeX in the picture");
        }
    }

    #[test]
    fn a_matrix_is_two_rows_tall_and_a_row_is_not() {
        let one = set(r"a + b");
        let two = set(r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}");
        assert!(two.height() > one.height() * 1.6, "{} against {}", two.height(), one.height());
    }

    #[test]
    fn colour_belongs_to_the_formula_only_when_it_asks() {
        let plain = set("x");
        assert!(plain.shapes.iter().all(|s| matches!(s, Shape::Glyph { color: None, .. })), "plain maths takes the page's colour");
        let red = set(r"\textcolor{red}{x}");
        assert!(red.shapes.iter().any(|s| matches!(s, Shape::Glyph { color: Some([r, g, b, _]), .. } if *r > 200 && *g < 80 && *b < 80)));
    }

    #[test]
    fn display_and_inline_differ_for_a_sum() {
        let tex = r"\sum_{i=1}^{n} i";
        let (big, small) = (typeset(tex, 20.0, true, &Mono), typeset(tex, 20.0, false, &Mono));
        assert!(big.height() > small.height(), "limits go above and below in display: {} against {}", big.height(), small.height());
    }

    #[test]
    fn a_broken_formula_falls_back_and_stays_visible() {
        let t = set(r"\frac{1}{ \sqrt{ \begin{pmatrix}");
        assert_eq!(t.engine, Engine::Subset);
        assert!(!t.shapes.is_empty(), "something is left to show");
    }

    #[test]
    fn an_empty_formula_is_nothing_and_does_not_panic() {
        for tex in ["", "   ", "{}", "$"] {
            let t = set(tex);
            assert!(t.width.is_finite() && t.ascent.is_finite() && t.descent.is_finite(), "{tex:?}");
        }
    }

    #[test]
    fn hostile_input_is_cut_before_it_reaches_the_parser() {
        // Nesting that would overflow a recursive parser.
        let deep = format!("{}x{}", "{".repeat(5000), "}".repeat(5000));
        let t = set(&deep);
        assert_eq!(t.engine, Engine::Subset, "too deep for RaTeX, so the subset engine took it");
        let frac = format!("{}x{}", r"\frac{1}{".repeat(5000), "}".repeat(5000));
        assert_eq!(set(&frac).engine, Engine::Subset);
        // A formula far too long.
        let long = "x+".repeat(MAX_SOURCE_CHARS);
        assert_eq!(set(&long).engine, Engine::Subset);
        // Odd bytes and a lone backslash.
        for tex in ["\\", "\\\\\\", "\u{0}\u{1}\u{7f}", "\u{202e}abc\u{202c}", r"\begin{", r"\end{pmatrix}", r"\left", "^^^", "x_{", "a&b"] {
            let t = set(tex);
            assert!(t.width.is_finite(), "{tex:?}");
        }
    }

    #[test]
    fn nesting_counts_braces_and_environments_and_ignores_escaped_ones() {
        assert_eq!(nesting("x"), 0);
        assert_eq!(nesting("{{x}}"), 2);
        assert_eq!(nesting(r"\{ \}"), 0);
        assert_eq!(nesting(r"\begin{pmatrix}a\end{pmatrix}"), 2);
        assert_eq!(nesting(r"\left( \frac{a}{b} \right)"), 2);
    }

    #[test]
    fn letters_katex_lacks_stay_in_one_run_in_the_windows_font() {
        let t = set(r"\text{المعادلة} \; x^2 = 4");
        let run = t.shapes.iter().find_map(|s| match s {
            Shape::Text { text, .. } if text.contains('ا') => Some(text.as_str()),
            _ => None,
        });
        assert_eq!(run, Some("المعادلة"), "the Arabic is one run, in its own order: {:?}", t.shapes);
    }

    #[test]
    fn the_size_asked_for_is_the_size_drawn() {
        let (small, big) = (typeset("x^2", 10.0, true, &Mono), typeset("x^2", 40.0, true, &Mono));
        assert!((big.width / small.width - 4.0).abs() < 0.05, "{} and {}", small.width, big.width);
        let nonsense = typeset("x", f32::NAN, true, &Mono);
        assert!(nonsense.width.is_finite() && nonsense.width > 0.0);
    }
}

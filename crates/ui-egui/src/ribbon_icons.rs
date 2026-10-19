//! The ribbon's own icons: the tag marks and the formatting marks, drawn in code in the
//! colours OneNote uses for them (a purple tick, an amber star, a red exclamation), so the
//! ribbon reads at a glance instead of showing letters.
//!
//! These are original line drawings on the same 24 x 24 grid as [`crate::icons`]. They are
//! tested differently from that set: each one must draw ink, must differ from every other
//! one, and must come out the same twice. Restating twenty small drawings a second time in
//! the test would be copying, not checking. See `docs/testing.md`.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    // Tags, in OneNote's order.
    ToDo,
    Important,
    Question,
    Remember,
    Definition,
    Highlighter,
    Contact,
    Address,
    Phone,
    Website,
    Idea,
    Critical,
    // Formatting.
    Bold,
    Italic,
    Underline,
    Strike,
    FontColor,
    IndentMore,
    IndentLess,
    ClearFormat,
    Cut,
    Copy,
    Paste,
    Painter,
    /// A red cross: delete.
    Delete,
}

impl Mark {
    pub const ALL: [Mark; 25] = [
        Mark::ToDo,
        Mark::Important,
        Mark::Question,
        Mark::Remember,
        Mark::Definition,
        Mark::Highlighter,
        Mark::Contact,
        Mark::Address,
        Mark::Phone,
        Mark::Website,
        Mark::Idea,
        Mark::Critical,
        Mark::Bold,
        Mark::Italic,
        Mark::Underline,
        Mark::Strike,
        Mark::FontColor,
        Mark::IndentMore,
        Mark::IndentLess,
        Mark::ClearFormat,
        Mark::Cut,
        Mark::Copy,
        Mark::Paste,
        Mark::Painter,
        Mark::Delete,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Mark::ToDo => "to-do",
            Mark::Important => "important",
            Mark::Question => "question",
            Mark::Remember => "remember",
            Mark::Definition => "definition",
            Mark::Highlighter => "highlighter",
            Mark::Contact => "contact",
            Mark::Address => "address",
            Mark::Phone => "phone",
            Mark::Website => "website",
            Mark::Idea => "idea",
            Mark::Critical => "critical",
            Mark::Bold => "bold",
            Mark::Italic => "italic",
            Mark::Underline => "underline",
            Mark::Strike => "strike",
            Mark::FontColor => "font-colour",
            Mark::IndentMore => "indent-more",
            Mark::IndentLess => "indent-less",
            Mark::ClearFormat => "clear-format",
            Mark::Cut => "cut",
            Mark::Copy => "copy",
            Mark::Paste => "paste",
            Mark::Painter => "painter",
            Mark::Delete => "delete",
        }
    }

    /// The colour OneNote gives this mark. `ink` is the theme's own text colour, used for
    /// the marks that have no colour of their own.
    pub fn color(self, ink: Color32, accent: Color32) -> Color32 {
        match self {
            Mark::ToDo => accent,
            Mark::Important => Color32::from_rgb(0xE8, 0xA3, 0x17),
            Mark::Question => Color32::from_rgb(0x8B, 0x5C, 0xD6),
            Mark::Remember => Color32::from_rgb(0xE6, 0xCC, 0x22),
            Mark::Definition => Color32::from_rgb(0x3F, 0xB9, 0x50),
            Mark::Highlighter => Color32::from_rgb(0xE6, 0xCC, 0x22),
            Mark::Critical | Mark::Delete => Color32::from_rgb(0xD1, 0x34, 0x38),
            Mark::Contact | Mark::Address | Mark::Phone | Mark::Website | Mark::Idea => accent,
            _ => ink,
        }
    }
}

struct Grid {
    rect: Rect,
}

impl Grid {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.rect.min.x + self.rect.width() * x / 24.0, self.rect.min.y + self.rect.height() * y / 24.0)
    }
}

/// Draws one mark. `color` is what it is drawn in; [`Mark::color`] picks the usual one.
pub fn paint(painter: &Painter, rect: Rect, mark: Mark, color: Color32) {
    for s in shapes(rect, mark, color) {
        painter.add(s);
    }
}

/// The shapes one mark is made of. Pure, so the tests can compare drawings without a window.
pub fn shapes(rect: Rect, mark: Mark, color: Color32) -> Vec<Shape> {
    let g = Grid { rect };
    let w = rect.width().max(1.0);
    let stroke = Stroke::new((w / 13.0).max(1.2), color);
    let thick = Stroke::new((w / 9.0).max(1.6), color);
    let line = |a: (f32, f32), b: (f32, f32)| Shape::line_segment([g.p(a.0, a.1), g.p(b.0, b.1)], stroke);
    let thick_line = |a: (f32, f32), b: (f32, f32)| Shape::line_segment([g.p(a.0, a.1), g.p(b.0, b.1)], thick);
    let path = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| g.p(x, y)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(pts.iter().map(|&(x, y)| g.p(x, y)).collect(), stroke);
    let fill = |pts: &[(f32, f32)]| Shape::convex_polygon(pts.iter().map(|&(x, y)| g.p(x, y)).collect(), color, Stroke::NONE);
    match mark {
        Mark::ToDo => vec![closed(&[(4.0, 5.0), (20.0, 5.0), (20.0, 19.0), (4.0, 19.0)]), path(&[(7.5, 12.0), (10.5, 15.5), (16.5, 8.5)])],
        // A five pointed star, filled.
        Mark::Important => {
            let mut pts = Vec::new();
            for i in 0..10 {
                let a = std::f32::consts::TAU * f32::from(i as u8) / 10.0 - std::f32::consts::FRAC_PI_2;
                let r = if i % 2 == 0 { 8.5 } else { 3.6 };
                pts.push((12.0 + a.cos() * r, 12.0 + a.sin() * r));
            }
            vec![fill(&pts)]
        }
        Mark::Question => vec![
            path(&[(8.0, 8.5), (9.5, 6.0), (13.5, 6.0), (15.0, 8.5), (14.0, 11.0), (12.0, 12.5), (12.0, 14.5)]),
            Shape::circle_filled(g.p(12.0, 18.0), w / 16.0, color),
        ],
        // A block of highlighted text: a wide one to remember, a tighter one for a word
        // being defined, as OneNote shows them.
        Mark::Remember => vec![Shape::rect_filled(Rect::from_min_max(g.p(3.0, 8.0), g.p(21.0, 16.0)), egui::CornerRadius::same(2), color)],
        Mark::Definition => vec![
            Shape::rect_filled(Rect::from_min_max(g.p(5.0, 9.0), g.p(19.0, 15.0)), egui::CornerRadius::same(1), color),
            line((5.0, 18.5), (19.0, 18.5)),
        ],
        Mark::Highlighter => vec![
            closed(&[(8.0, 4.5), (15.0, 4.5), (15.0, 13.0), (8.0, 13.0)]),
            fill(&[(7.0, 13.0), (16.0, 13.0), (14.0, 17.5), (9.0, 17.5)]),
            thick_line((6.0, 20.0), (18.0, 20.0)),
        ],
        Mark::Contact => vec![
            Shape::circle_stroke(g.p(9.0, 9.5), w * 3.0 / 24.0, stroke),
            path(&[(4.0, 18.5), (5.5, 14.5), (12.5, 14.5), (14.0, 18.5)]),
            line((16.0, 9.0), (20.5, 9.0)),
            line((16.0, 12.5), (20.5, 12.5)),
        ],
        Mark::Address => vec![path(&[(4.0, 12.0), (12.0, 5.0), (20.0, 12.0)]), closed(&[(6.5, 12.0), (17.5, 12.0), (17.5, 19.5), (6.5, 19.5)])],
        Mark::Phone => vec![path(&[
            (5.0, 6.0),
            (8.5, 4.5),
            (11.0, 8.5),
            (9.0, 10.5),
            (13.5, 15.0),
            (15.5, 13.0),
            (19.5, 15.5),
            (18.0, 19.0),
            (13.0, 18.5),
            (7.0, 13.0),
            (5.0, 6.0),
        ])],
        Mark::Website => vec![
            Shape::circle_stroke(g.p(12.0, 12.0), w * 8.0 / 24.0, stroke),
            line((4.0, 12.0), (20.0, 12.0)),
            path(&[(12.0, 4.0), (8.5, 8.0), (8.5, 16.0), (12.0, 20.0), (15.5, 16.0), (15.5, 8.0), (12.0, 4.0)]),
        ],
        Mark::Idea => vec![
            path(&[(9.0, 15.0), (7.0, 11.5), (8.0, 7.0), (12.0, 5.0), (16.0, 7.0), (17.0, 11.5), (15.0, 15.0)]),
            line((9.5, 17.5), (14.5, 17.5)),
            line((10.5, 20.0), (13.5, 20.0)),
        ],
        Mark::Critical => vec![thick_line((12.0, 4.5), (12.0, 14.5)), Shape::circle_filled(g.p(12.0, 18.5), w / 13.0, color)],
        // A B with two bowls.
        Mark::Bold => vec![
            thick_line((8.0, 5.0), (8.0, 19.0)),
            path(&[(8.0, 5.0), (14.0, 5.0), (16.5, 7.5), (14.0, 11.5), (8.0, 11.5)]),
            path(&[(8.0, 11.5), (15.5, 11.5), (18.0, 15.0), (15.0, 19.0), (8.0, 19.0)]),
        ],
        Mark::Italic => vec![line((9.0, 5.0), (16.0, 5.0)), line((8.0, 19.0), (15.0, 19.0)), line((13.5, 5.0), (10.0, 19.0))],
        Mark::Underline => vec![path(&[(8.0, 4.5), (8.0, 12.5), (12.0, 16.0), (16.0, 12.5), (16.0, 4.5)]), thick_line((6.5, 19.5), (17.5, 19.5))],
        Mark::Strike => vec![
            path(&[(8.0, 6.0), (16.0, 6.0)]),
            path(&[(12.0, 6.0), (12.0, 18.0)]),
            path(&[(8.0, 18.0), (16.0, 18.0)]),
            thick_line((4.5, 12.0), (19.5, 12.0)),
        ],
        // An A over a bar of colour.
        Mark::FontColor => vec![
            path(&[(6.5, 15.0), (12.0, 4.5), (17.5, 15.0)]),
            line((8.8, 11.0), (15.2, 11.0)),
            Shape::rect_filled(Rect::from_min_max(g.p(5.0, 17.5), g.p(19.0, 20.5)), egui::CornerRadius::same(1), color),
        ],
        Mark::IndentMore => vec![
            line((9.0, 6.0), (20.0, 6.0)),
            line((9.0, 12.0), (20.0, 12.0)),
            line((9.0, 18.0), (20.0, 18.0)),
            fill(&[(3.5, 8.5), (7.5, 12.0), (3.5, 15.5)]),
        ],
        Mark::IndentLess => vec![
            line((9.0, 6.0), (20.0, 6.0)),
            line((9.0, 12.0), (20.0, 12.0)),
            line((9.0, 18.0), (20.0, 18.0)),
            fill(&[(7.5, 8.5), (3.5, 12.0), (7.5, 15.5)]),
        ],
        // An A with a cross beside it.
        Mark::ClearFormat => vec![
            path(&[(4.0, 17.0), (9.0, 6.0), (14.0, 17.0)]),
            line((6.0, 13.0), (12.0, 13.0)),
            line((15.5, 10.0), (21.0, 15.5)),
            line((21.0, 10.0), (15.5, 15.5)),
        ],
        Mark::Cut => vec![
            Shape::circle_stroke(g.p(8.0, 17.5), w * 2.4 / 24.0, stroke),
            Shape::circle_stroke(g.p(16.0, 17.5), w * 2.4 / 24.0, stroke),
            line((16.0, 15.0), (7.5, 4.5)),
            line((8.0, 15.0), (16.5, 4.5)),
        ],
        Mark::Copy => {
            vec![closed(&[(4.5, 4.5), (14.0, 4.5), (14.0, 16.0), (4.5, 16.0)]), closed(&[(9.0, 8.5), (19.5, 8.5), (19.5, 20.0), (9.0, 20.0)])]
        }
        Mark::Paste => vec![
            closed(&[(5.0, 6.0), (19.0, 6.0), (19.0, 20.5), (5.0, 20.5)]),
            fill(&[(9.0, 3.5), (15.0, 3.5), (15.0, 7.5), (9.0, 7.5)]),
            line((8.0, 12.0), (16.0, 12.0)),
            line((8.0, 16.0), (16.0, 16.0)),
        ],
        Mark::Delete => vec![thick_line((6.0, 6.0), (18.0, 18.0)), thick_line((18.0, 6.0), (6.0, 18.0))],
        Mark::Painter => vec![
            fill(&[(4.0, 5.0), (12.0, 5.0), (12.0, 9.5), (4.0, 9.5)]),
            path(&[(12.0, 7.0), (16.0, 7.0), (16.0, 12.5), (11.0, 12.5), (11.0, 20.0)]),
            closed(&[(9.5, 15.0), (12.5, 15.0), (12.5, 20.5), (9.5, 20.5)]),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    /// One mark's drawing, as text, so two of them can be compared without a renderer.
    fn draw(mark: Mark, color: Color32) -> String {
        format!("{:?}", shapes(Rect::from_min_size(pos2(0.0, 0.0), vec2(18.0, 18.0)), mark, color))
    }

    #[test]
    fn every_mark_draws_something_and_is_its_own_drawing() {
        let ink = Color32::BLACK;
        let drawings: Vec<(Mark, String)> = Mark::ALL.iter().map(|m| (*m, draw(*m, ink))).collect();
        for (mark, drawing) in &drawings {
            assert!(drawing.len() > 40, "{} draws nothing", mark.name());
        }
        for (i, (a, da)) in drawings.iter().enumerate() {
            for (b, db) in drawings.iter().skip(i + 1) {
                assert_ne!(da, db, "{} and {} are the same drawing", a.name(), b.name());
            }
        }
    }

    #[test]
    fn a_mark_comes_out_the_same_twice() {
        for mark in Mark::ALL {
            assert_eq!(draw(mark, Color32::BLACK), draw(mark, Color32::BLACK), "{}", mark.name());
        }
    }

    #[test]
    fn nothing_panics_on_a_tiny_or_huge_box() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for mark in Mark::ALL {
                paint(&painter, Rect::from_min_size(pos2(0.0, 0.0), vec2(0.0, 0.0)), mark, Color32::WHITE);
                paint(&painter, Rect::from_min_size(pos2(-50.0, -50.0), vec2(4000.0, 4000.0)), mark, Color32::WHITE);
            }
        });
        out.textures_delta.clear();
    }

    #[test]
    fn the_tag_colours_are_the_ones_onenote_uses() {
        let (ink, accent) = (Color32::BLACK, Color32::from_rgb(0x7A, 0x45, 0xB8));
        assert_eq!(Mark::ToDo.color(ink, accent), accent, "the to-do tick takes the theme's accent");
        assert_eq!(Mark::Important.color(ink, accent), Color32::from_rgb(0xE8, 0xA3, 0x17), "a star is amber");
        assert_eq!(Mark::Critical.color(ink, accent), Color32::from_rgb(0xD1, 0x34, 0x38), "critical is red");
        assert_eq!(Mark::Bold.color(ink, accent), ink, "formatting marks are the text colour");
        // Every mark has a name, and no two share one.
        let names: std::collections::HashSet<&str> = Mark::ALL.iter().map(|m| m.name()).collect();
        assert_eq!(names.len(), Mark::ALL.len());
    }
}

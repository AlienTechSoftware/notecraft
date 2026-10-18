//! The icon set, drawn in code with the painter. Original line drawings (see
//! ATTRIBUTION.md): no image files and nothing copied from another application.
//!
//! Notebooks, section groups and sections are drawn as solid shapes in their own colour,
//! which is how OneNote shows a notebook cover and a section tab. Pages stay as outlines,
//! so a page never competes with the coloured containers above it.

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// A closed notebook seen from the front: a cover with a darker spine.
    Notebook,
    /// Two section tabs behind one another: a group of sections.
    SectionGroup,
    /// One open section group: the tabs fan out.
    SectionGroupOpen,
    /// A single section tab, the coloured divider of a notebook.
    Section,
    /// One sheet with a folded corner.
    Page,
    /// A stack of sheets: a page that has sub-pages, shown while collapsed.
    PageStack,
    /// Angle brackets: show or hide the page source.
    Code,
    /// A magnifier: search.
    Search,
    /// Two arrows pushing apart: the page fills the window.
    FullPage,
    /// Two arrows pulling together: leave the full page view.
    ExitFullPage,
    /// The sun: the light themes.
    Sun,
    /// The moon: the dark themes.
    Moon,
    /// Three lines with dots: a bullet list.
    ListBullet,
    /// Three lines with numbers: a numbered list.
    ListNumber,
    /// A ticked box: a to-do.
    Todo,
    /// A grid: a table.
    Table,
    /// A frame with a hill and a sun: a picture.
    Picture,
    /// A paperclip: a file on the page.
    Attach,
    /// Two links of a chain.
    Link,
    /// A radical sign: an equation.
    Equation,
    /// Two boxes and an arrow: a diagram.
    Diagram,
    /// Quotation marks: a quote.
    Quote,
    /// Three thin lines, well apart: show or hide the notebooks column.
    Bars,
    /// Lines of text with an arrow under them pointing right: this reads left to right.
    DirLtr,
    /// The same, mirrored: this reads right to left.
    DirRtl,
}

/// Maps points from a 24 x 24 design grid into `rect`.
struct Grid {
    rect: Rect,
}

impl Grid {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        pos2(self.rect.min.x + self.rect.width() * x / 24.0, self.rect.min.y + self.rect.height() * y / 24.0)
    }

    fn pts(&self, pts: &[(f32, f32)]) -> Vec<Pos2> {
        pts.iter().map(|&(x, y)| self.p(x, y)).collect()
    }
}

/// Mixes towards black (or towards white when the colour is already dark) for the spine and
/// the tabs behind the first one, so one colour gives a whole icon.
fn shade(c: Color32, amount: f32) -> Color32 {
    let bright = f32::from(c.r()) * 0.299 + f32::from(c.g()) * 0.587 + f32::from(c.b()) * 0.114;
    let target = if bright > 110.0 { 0.0f32 } else { 255.0 };
    let mix = |x: u8| (f32::from(x) + (target - f32::from(x)) * amount).clamp(0.0, 255.0) as u8;
    Color32::from_rgb(mix(c.r()), mix(c.g()), mix(c.b()))
}

pub fn paint(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let g = Grid { rect };
    let w = rect.width().max(1.0);
    let stroke = Stroke::new((w / 14.0).max(1.2), color);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(g.pts(pts), stroke);
    let line = |a: (f32, f32), b: (f32, f32)| Shape::line_segment([g.p(a.0, a.1), g.p(b.0, b.1)], stroke);
    let path = |pts: &[(f32, f32)]| Shape::line(g.pts(pts), stroke);
    let fill = |pts: &[(f32, f32)], c: Color32| Shape::convex_polygon(g.pts(pts), c, Stroke::NONE);
    let shapes: Vec<Shape> = match icon {
        // A cover with a darker spine down its left edge, and two faint rule lines.
        Icon::Notebook => {
            let spine = shade(color, 0.35);
            let rule = shade(color, 0.55);
            vec![
                fill(&[(5.0, 3.0), (19.0, 3.0), (19.0, 21.0), (5.0, 21.0)], color),
                fill(&[(5.0, 3.0), (8.5, 3.0), (8.5, 21.0), (5.0, 21.0)], spine),
                Shape::line_segment([g.p(11.5, 8.5), g.p(16.5, 8.5)], Stroke::new((w / 16.0).max(1.0), rule)),
                Shape::line_segment([g.p(11.5, 12.0), g.p(16.5, 12.0)], Stroke::new((w / 16.0).max(1.0), rule)),
            ]
        }
        // One tab: a rounded rectangle standing on its end, like a section divider.
        Icon::Section => {
            vec![Shape::rect_filled(Rect::from_min_max(g.p(8.5, 3.5), g.p(15.5, 20.5)), egui::CornerRadius::same((w / 8.0).max(1.0) as u8), color)]
        }
        // Several tabs, the ones behind in a lighter shade.
        Icon::SectionGroup => {
            let back = shade(color, 0.45);
            vec![
                Shape::rect_filled(Rect::from_min_max(g.p(13.5, 5.5), g.p(18.0, 18.5)), egui::CornerRadius::same(2), back),
                Shape::rect_filled(Rect::from_min_max(g.p(9.5, 4.5), g.p(14.0, 19.5)), egui::CornerRadius::same(2), shade(color, 0.2)),
                Shape::rect_filled(Rect::from_min_max(g.p(5.5, 3.5), g.p(10.0, 20.5)), egui::CornerRadius::same(2), color),
            ]
        }
        // The same tabs, fanned out: the group is open.
        Icon::SectionGroupOpen => {
            let back = shade(color, 0.45);
            vec![
                Shape::rect_filled(Rect::from_min_max(g.p(16.0, 7.0), g.p(20.5, 20.0)), egui::CornerRadius::same(2), back),
                Shape::rect_filled(Rect::from_min_max(g.p(10.0, 5.0), g.p(14.5, 20.0)), egui::CornerRadius::same(2), shade(color, 0.2)),
                Shape::rect_filled(Rect::from_min_max(g.p(4.0, 3.5), g.p(8.5, 20.0)), egui::CornerRadius::same(2), color),
            ]
        }
        Icon::Page => vec![
            closed(&[(5.0, 3.0), (14.0, 3.0), (19.0, 8.0), (19.0, 21.0), (5.0, 21.0)]),
            path(&[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)]),
            line((8.5, 13.0), (15.5, 13.0)),
            line((8.5, 16.5), (15.5, 16.5)),
        ],
        Icon::PageStack => vec![
            path(&[(8.5, 3.0), (15.0, 3.0), (20.0, 8.0), (20.0, 15.0)]),
            closed(&[(4.5, 6.0), (12.0, 6.0), (17.0, 11.0), (17.0, 21.0), (4.5, 21.0)]),
            path(&[(12.0, 6.0), (12.0, 11.0), (17.0, 11.0)]),
            line((7.5, 16.0), (14.0, 16.0)),
        ],
        Icon::Code => vec![path(&[(9.0, 6.0), (4.0, 12.0), (9.0, 18.0)]), path(&[(15.0, 6.0), (20.0, 12.0), (15.0, 18.0)])],
        Icon::Search => vec![Shape::circle_stroke(g.p(10.5, 10.5), w * 5.5 / 24.0, stroke), line((14.6, 14.6), (19.5, 19.5))],
        Icon::FullPage => vec![
            path(&[(14.0, 4.0), (20.0, 4.0), (20.0, 10.0)]),
            line((20.0, 4.0), (14.5, 9.5)),
            path(&[(10.0, 20.0), (4.0, 20.0), (4.0, 14.0)]),
            line((4.0, 20.0), (9.5, 14.5)),
        ],
        Icon::ExitFullPage => vec![
            path(&[(20.0, 10.0), (14.0, 10.0), (14.0, 4.0)]),
            line((14.0, 10.0), (19.5, 4.5)),
            path(&[(4.0, 14.0), (10.0, 14.0), (10.0, 20.0)]),
            line((10.0, 14.0), (4.5, 19.5)),
        ],
        Icon::Sun => {
            let mut v = vec![Shape::circle_stroke(g.p(12.0, 12.0), w * 4.6 / 24.0, stroke)];
            for i in 0..8 {
                let a = std::f32::consts::TAU * f32::from(i as u8) / 8.0;
                let (sx, sy) = (12.0 + a.cos() * 7.4, 12.0 + a.sin() * 7.4);
                let (ex, ey) = (12.0 + a.cos() * 9.6, 12.0 + a.sin() * 9.6);
                v.push(line((sx, sy), (ex, ey)));
            }
            v
        }
        Icon::ListBullet => vec![
            Shape::circle_filled(g.p(5.0, 7.0), w / 22.0, color),
            Shape::circle_filled(g.p(5.0, 12.0), w / 22.0, color),
            Shape::circle_filled(g.p(5.0, 17.0), w / 22.0, color),
            line((10.0, 7.0), (20.0, 7.0)),
            line((10.0, 12.0), (20.0, 12.0)),
            line((10.0, 17.0), (20.0, 17.0)),
        ],
        Icon::ListNumber => vec![
            line((4.0, 5.5), (4.0, 9.0)),
            line((3.0, 6.5), (4.0, 5.5)),
            path(&[(3.0, 11.0), (5.0, 11.0), (3.0, 14.5), (5.0, 14.5)]),
            path(&[(3.0, 16.5), (5.0, 16.5), (5.0, 18.0), (3.0, 18.0), (5.0, 18.0), (5.0, 19.5), (3.0, 19.5)]),
            line((10.0, 7.0), (20.0, 7.0)),
            line((10.0, 12.5), (20.0, 12.5)),
            line((10.0, 18.0), (20.0, 18.0)),
        ],
        Icon::Todo => vec![closed(&[(4.0, 5.0), (20.0, 5.0), (20.0, 19.0), (4.0, 19.0)]), path(&[(7.5, 12.0), (10.5, 15.5), (16.5, 8.5)])],
        Icon::Table => vec![
            closed(&[(3.5, 5.0), (20.5, 5.0), (20.5, 19.0), (3.5, 19.0)]),
            line((3.5, 9.5), (20.5, 9.5)),
            line((3.5, 14.5), (20.5, 14.5)),
            line((9.0, 5.0), (9.0, 19.0)),
            line((15.0, 5.0), (15.0, 19.0)),
        ],
        Icon::Picture => vec![
            closed(&[(3.5, 5.0), (20.5, 5.0), (20.5, 19.0), (3.5, 19.0)]),
            Shape::circle_stroke(g.p(8.5, 9.5), w * 1.6 / 24.0, stroke),
            path(&[(5.0, 17.0), (10.0, 12.0), (13.0, 15.0), (16.0, 11.5), (19.5, 16.5)]),
        ],
        Icon::Attach => vec![path(&[(16.5, 7.0), (9.0, 14.5), (9.0, 17.0), (11.5, 17.0), (19.0, 9.5), (19.0, 6.0), (15.5, 4.5), (7.0, 13.0)])],
        Icon::Link => vec![
            path(&[(10.0, 14.0), (7.0, 17.0), (4.5, 14.5), (7.5, 11.5)]),
            path(&[(14.0, 10.0), (17.0, 7.0), (19.5, 9.5), (16.5, 12.5)]),
            line((9.5, 14.5), (14.5, 9.5)),
        ],
        Icon::Equation => vec![path(&[(4.0, 13.0), (7.0, 13.0), (10.0, 19.5), (14.0, 5.0), (20.0, 5.0)]), line((11.5, 12.0), (18.0, 12.0))],
        Icon::Diagram => vec![
            closed(&[(3.5, 4.5), (10.5, 4.5), (10.5, 9.5), (3.5, 9.5)]),
            closed(&[(13.5, 14.5), (20.5, 14.5), (20.5, 19.5), (13.5, 19.5)]),
            line((7.0, 9.5), (7.0, 17.0)),
            path(&[(7.0, 17.0), (13.5, 17.0)]),
            path(&[(11.5, 15.5), (13.5, 17.0), (11.5, 18.5)]),
        ],
        Icon::Quote => vec![
            path(&[(9.0, 6.0), (5.0, 9.0), (5.0, 13.0), (9.0, 13.0), (9.0, 9.0), (5.0, 9.0)]),
            path(&[(19.0, 6.0), (15.0, 9.0), (15.0, 13.0), (19.0, 13.0), (19.0, 9.0), (15.0, 9.0)]),
        ],
        // Three lines, thin and well apart, for the button that hides the notebooks.
        Icon::Bars => vec![path(&[(4.0, 7.0), (20.0, 7.0)]), path(&[(4.0, 12.0), (20.0, 12.0)]), path(&[(4.0, 17.0), (20.0, 17.0)])],
        // Lines of text with an arrow beneath them, the way Word marks text direction.
        Icon::DirLtr => vec![
            path(&[(5.0, 6.0), (17.0, 6.0)]),
            path(&[(5.0, 10.0), (19.0, 10.0)]),
            path(&[(5.0, 14.0), (14.0, 14.0)]),
            path(&[(5.0, 19.0), (19.0, 19.0)]),
            path(&[(16.0, 16.0), (19.0, 19.0), (16.0, 22.0)]),
        ],
        Icon::DirRtl => vec![
            path(&[(19.0, 6.0), (7.0, 6.0)]),
            path(&[(19.0, 10.0), (5.0, 10.0)]),
            path(&[(19.0, 14.0), (10.0, 14.0)]),
            path(&[(19.0, 19.0), (5.0, 19.0)]),
            path(&[(8.0, 16.0), (5.0, 19.0), (8.0, 22.0)]),
        ],
        // A crescent: the outer circle, with the inner edge drawn across it.
        Icon::Moon => vec![
            Shape::circle_stroke(g.p(12.0, 12.0), w * 7.4 / 24.0, stroke),
            path(&[(16.6, 5.0), (12.6, 7.2), (11.0, 11.6), (12.6, 16.0), (16.6, 19.0)]),
        ],
    };
    for s in shapes {
        painter.add(s);
    }
}

/// The crescent needs the surface behind it, because it is drawn by covering a disc.
pub fn paint_moon(painter: &Painter, rect: Rect, color: Color32, behind: Color32) {
    let g = Grid { rect };
    let w = rect.width().max(1.0);
    painter.add(Shape::circle_filled(g.p(13.0, 12.0), w * 8.0 / 24.0, color));
    painter.add(Shape::circle_filled(g.p(17.0, 8.6), w * 7.2 / 24.0, behind));
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const ALL: [Icon; 25] = [
        Icon::Notebook,
        Icon::SectionGroup,
        Icon::SectionGroupOpen,
        Icon::Section,
        Icon::Page,
        Icon::PageStack,
        Icon::Code,
        Icon::Search,
        Icon::FullPage,
        Icon::ExitFullPage,
        Icon::Sun,
        Icon::Moon,
        Icon::ListBullet,
        Icon::ListNumber,
        Icon::Todo,
        Icon::Table,
        Icon::Picture,
        Icon::Attach,
        Icon::Link,
        Icon::Equation,
        Icon::Diagram,
        Icon::Quote,
        Icon::DirLtr,
        Icon::DirRtl,
        Icon::Bars,
    ];

    #[test]
    fn the_grid_maps_corners() {
        let g = Grid { rect: Rect::from_min_size(pos2(10.0, 20.0), egui::vec2(24.0, 24.0)) };
        assert_eq!(g.p(0.0, 0.0), pos2(10.0, 20.0));
        assert_eq!(g.p(24.0, 24.0), pos2(34.0, 44.0));
        assert_eq!(g.p(12.0, 12.0), pos2(22.0, 32.0));
    }

    #[test]
    fn every_icon_paints_without_panicking() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let painter = ui.painter().clone();
            for icon in ALL {
                paint(&painter, Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(17.0, 17.0)), icon, Color32::WHITE);
                paint(&painter, Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(0.0, 0.0)), icon, Color32::WHITE);
            }
            paint_moon(&painter, Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(17.0, 17.0)), Color32::WHITE, Color32::BLACK);
        });
        out.textures_delta.clear();
    }

    /// The shading keeps a spine visible on a light colour and on a dark one.
    #[test]
    fn shading_moves_away_from_the_colour() {
        let light = Color32::from_rgb(0xF2, 0xC9, 0x4C);
        let dark = Color32::from_rgb(0x1E, 0x2A, 0x44);
        assert_ne!(shade(light, 0.35), light);
        assert_ne!(shade(dark, 0.35), dark);
        // A light colour gets darker, a dark one gets lighter.
        assert!(shade(light, 0.35).r() < light.r());
        assert!(shade(dark, 0.35).r() > dark.r());
        assert_eq!(shade(light, 0.0), light);
    }
}

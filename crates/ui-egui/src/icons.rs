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
    /// A pilcrow with an arrow beside it pointing right: this paragraph reads left to
    /// right. Word, LibreOffice and the Office web apps all mark paragraph direction with
    /// a pilcrow and an arrow, and a writer of Arabic looks for that shape, so NoteCraft
    /// draws one too. The lines-and-arrow pair above is the whole page; this pair is one
    /// paragraph.
    ParaLtr,
    /// The same, mirrored: this paragraph reads right to left.
    ParaRtl,
    /// A block rubber held at an angle, with the worn end on the page: the ink eraser.
    Eraser,
    /// An arrow curving back to the left: take the last thing back.
    Undo,
    /// The same arrow the other way: do it again.
    Redo,
    /// An arrow head: the select tool.
    Pointer,
    /// A loop drawn round something: lasso select.
    Lasso,
    /// An open hand: drag the page about instead of drawing on it.
    Hand,
    /// A ruler lying at an angle, with its marks along one edge.
    Ruler,
    /// Two arrows pushing apart with a line between: make room.
    InsertSpace,
    /// A circle behind a square: shapes.
    Shapes,
    /// A written stroke beside printed lines: handwriting becomes words.
    InkToText,
    /// An arrow running round a circle: watch the ink drawn again.
    Replay,
    /// A question mark in a ring.
    Help,
    /// A pen nib pointing down and to the left: ink.
    Pen,
    /// A plain plus: add one more of something.
    Plus,
    /// A plain minus: zoom out.
    Minus,
    /// A magnifying glass with a minus sign: zoom out of the page.
    ZoomOut,
    /// A magnifying glass with a plus sign: zoom into the page.
    ZoomIn,
    /// A page fitted across the available view width.
    PageWidth,
    /// A calendar page with two rings: a date.
    Calendar,
    /// A clock face with two hands: a time.
    Clock,
    /// A screen with a play triangle on it.
    Video,
    /// A microphone on its stand: recording and transcribing.
    Mic,
    /// A sheet with a filled header row: a spreadsheet.
    Spreadsheet,
    /// Four corner brackets: clipping part of the screen.
    Clip,
    /// A round face: stickers.
    Sticker,
    /// An omega: a symbol from outside the keyboard.
    Symbol,
    /// A table with one row picked out.
    TableRow,
    /// A table with one column picked out.
    TableColumn,
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
        // A block rubber leaning on the page: the body, the worn band across its foot, and
        // the line it sits on, which is what says it is rubbing something out.
        // An arrow curving back on itself to the left: take the last thing back.
        Icon::Undo => {
            vec![path(&[(9.0, 6.0), (5.0, 10.0), (9.0, 14.0)]), path(&[(5.0, 10.0), (15.0, 10.0), (18.0, 13.0), (18.0, 17.0), (15.0, 20.0)])]
        }
        // The same arrow, mirrored: do it again.
        Icon::Redo => {
            vec![path(&[(15.0, 6.0), (19.0, 10.0), (15.0, 14.0)]), path(&[(19.0, 10.0), (9.0, 10.0), (6.0, 13.0), (6.0, 17.0), (9.0, 20.0)])]
        }
        // An arrow head with its tail: the ordinary select tool.
        Icon::Pointer => vec![fill(&[(6.0, 3.0), (18.5, 13.5), (12.5, 14.0), (15.5, 20.5), (12.0, 22.0), (9.0, 15.5), (6.0, 19.5)], color)],
        // A loop thrown round something, with its tail hanging down.
        Icon::Lasso => vec![closed(&[(12.0, 4.0), (19.0, 8.5), (16.5, 15.0), (7.5, 15.0), (5.0, 8.5)]), path(&[(8.0, 15.0), (8.0, 21.0)])],
        // An open hand: the fingers and the thumb as one outline.
        Icon::Hand => vec![fill(
            &[
                (9.0, 4.0),
                (11.0, 4.0),
                (11.0, 12.0),
                (13.0, 12.0),
                (13.0, 5.5),
                (15.0, 5.5),
                (15.0, 12.5),
                (17.5, 13.5),
                (17.5, 19.0),
                (14.0, 22.0),
                (9.5, 22.0),
                (6.5, 17.0),
                (6.5, 11.0),
                (9.0, 11.0),
            ],
            color,
        )],
        // A ruler lying at an angle, with its marks along the upper edge.
        Icon::Ruler => vec![
            closed(&[(3.5, 15.0), (15.0, 3.5), (20.5, 9.0), (9.0, 20.5)]),
            line((7.0, 11.5), (9.5, 14.0)),
            line((10.0, 8.5), (12.5, 11.0)),
            line((13.0, 5.5), (15.5, 8.0)),
        ],
        // Two arrows pushing apart, with the line they are making room at.
        Icon::InsertSpace => {
            vec![line((4.0, 12.0), (20.0, 12.0)), path(&[(9.0, 7.0), (12.0, 4.0), (15.0, 7.0)]), path(&[(9.0, 17.0), (12.0, 20.0), (15.0, 17.0)])]
        }
        // A circle behind a square.
        Icon::Shapes => vec![
            Shape::circle_stroke(g.p(9.5, 9.5), rect.width() * 5.5 / 24.0, stroke),
            closed(&[(11.5, 11.5), (20.5, 11.5), (20.5, 20.5), (11.5, 20.5)]),
        ],
        // A written stroke beside printed lines: handwriting turning into words.
        Icon::InkToText => vec![
            path(&[(4.0, 17.0), (6.5, 7.0), (9.0, 17.0), (11.5, 7.0), (14.0, 17.0)]),
            line((16.5, 8.0), (20.5, 8.0)),
            line((16.5, 12.5), (20.5, 12.5)),
            line((16.5, 17.0), (20.5, 17.0)),
        ],
        // An arrow running round a circle: the ink drawn again.
        Icon::Replay => vec![
            path(&[(18.5, 7.5), (18.5, 12.0), (14.0, 12.0)]),
            path(&[(18.0, 12.0), (16.0, 17.5), (10.5, 19.5), (5.5, 16.5), (4.5, 11.0), (8.0, 6.5), (13.5, 5.5), (18.0, 8.5)]),
        ],
        // A question mark in a ring.
        Icon::Help => vec![
            Shape::circle_stroke(g.p(12.0, 12.0), rect.width() * 8.5 / 24.0, stroke),
            path(&[(9.5, 9.5), (10.5, 7.5), (13.5, 7.5), (14.5, 9.5), (13.5, 11.5), (12.0, 12.5), (12.0, 14.5)]),
            Shape::circle_filled(g.p(12.0, 17.5), rect.width() / 20.0, color),
        ],
        // A pen nib pointing down and to the left, with its slit.
        // A calendar page hanging from two rings.
        Icon::Calendar => vec![
            closed(&[(4.0, 6.5), (20.0, 6.5), (20.0, 20.5), (4.0, 20.5)]),
            line((4.0, 11.0), (20.0, 11.0)),
            line((8.5, 3.5), (8.5, 8.0)),
            line((15.5, 3.5), (15.5, 8.0)),
        ],
        // A clock face, reading ten past ten as every clock in a shop window does.
        Icon::Clock => vec![
            Shape::circle_stroke(g.p(12.0, 12.5), rect.width() * 8.5 / 24.0, stroke),
            line((12.0, 12.5), (12.0, 7.0)),
            line((12.0, 12.5), (16.0, 14.5)),
        ],
        // A screen with a play triangle on it.
        Icon::Video => vec![closed(&[(3.5, 5.5), (20.5, 5.5), (20.5, 18.5), (3.5, 18.5)]), fill(&[(10.0, 9.0), (16.0, 12.0), (10.0, 15.0)], color)],
        // A microphone: the capsule, the cradle it sits in, and its stand.
        Icon::Mic => vec![
            closed(&[(9.5, 3.5), (14.5, 3.5), (14.5, 13.5), (9.5, 13.5)]),
            path(&[(6.5, 11.0), (6.5, 13.5), (12.0, 17.5), (17.5, 13.5), (17.5, 11.0)]),
            line((12.0, 17.5), (12.0, 20.5)),
        ],
        // A sheet with its header row filled, which is what tells it from a plain table.
        Icon::Spreadsheet => vec![
            fill(&[(4.0, 5.5), (20.0, 5.5), (20.0, 9.5), (4.0, 9.5)], shade(color, 0.45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 20.5), (4.0, 20.5)]),
            line((4.0, 9.5), (20.0, 9.5)),
            line((4.0, 15.0), (20.0, 15.0)),
            line((12.0, 9.5), (12.0, 20.5)),
        ],
        // Four corner brackets: the part of the screen being cut out.
        Icon::Clip => vec![
            path(&[(5.0, 9.5), (5.0, 5.0), (9.5, 5.0)]),
            path(&[(14.5, 5.0), (19.0, 5.0), (19.0, 9.5)]),
            path(&[(19.0, 14.5), (19.0, 19.0), (14.5, 19.0)]),
            path(&[(9.5, 19.0), (5.0, 19.0), (5.0, 14.5)]),
        ],
        // A round face.
        Icon::Sticker => vec![
            Shape::circle_stroke(g.p(12.0, 12.0), rect.width() * 8.5 / 24.0, stroke),
            Shape::circle_filled(g.p(9.0, 10.0), rect.width() / 20.0, color),
            Shape::circle_filled(g.p(15.0, 10.0), rect.width() / 20.0, color),
            path(&[(8.0, 14.5), (10.0, 17.0), (14.0, 17.0), (16.0, 14.5)]),
        ],
        // An omega, which is the mark every word processor puts on this button.
        Icon::Symbol => vec![path(&[
            (6.5, 20.0),
            (10.5, 20.0),
            (10.5, 17.0),
            (7.0, 14.0),
            (7.0, 9.5),
            (12.0, 6.0),
            (17.0, 9.5),
            (17.0, 14.0),
            (13.5, 17.0),
            (13.5, 20.0),
            (17.5, 20.0),
        ])],
        // A table with its middle row picked out.
        Icon::TableRow => vec![
            fill(&[(4.0, 10.0), (20.0, 10.0), (20.0, 14.5), (4.0, 14.5)], shade(color, 0.45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 19.0), (4.0, 19.0)]),
            line((4.0, 10.0), (20.0, 10.0)),
            line((4.0, 14.5), (20.0, 14.5)),
        ],
        // The same, with a column picked out instead.
        Icon::TableColumn => vec![
            fill(&[(9.5, 5.5), (14.5, 5.5), (14.5, 19.0), (9.5, 19.0)], shade(color, 0.45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 19.0), (4.0, 19.0)]),
            line((9.5, 5.5), (9.5, 19.0)),
            line((14.5, 5.5), (14.5, 19.0)),
        ],
        // A plain plus.
        Icon::Plus => vec![line((12.0, 4.5), (12.0, 19.5)), line((4.5, 12.0), (19.5, 12.0))],
        // A plain minus, the companion to the plus above.
        Icon::Minus => vec![line((4.5, 12.0), (19.5, 12.0))],
        Icon::ZoomOut => {
            vec![Shape::circle_stroke(g.p(10.0, 10.0), w * 6.0 / 24.0, stroke), line((14.5, 14.5), (20.0, 20.0)), line((6.5, 10.0), (13.5, 10.0))]
        }
        Icon::ZoomIn => vec![
            Shape::circle_stroke(g.p(10.0, 10.0), w * 6.0 / 24.0, stroke),
            line((14.5, 14.5), (20.0, 20.0)),
            line((6.5, 10.0), (13.5, 10.0)),
            line((10.0, 6.5), (10.0, 13.5)),
        ],
        Icon::PageWidth => vec![
            closed(&[(5.0, 3.5), (19.0, 3.5), (19.0, 20.5), (5.0, 20.5)]),
            line((8.0, 8.0), (16.0, 8.0)),
            path(&[(8.0, 12.0), (5.5, 14.0), (8.0, 16.0)]),
            path(&[(16.0, 12.0), (18.5, 14.0), (16.0, 16.0)]),
        ],
        Icon::Pen => vec![
            fill(&[(16.0, 3.5), (20.5, 8.0), (9.0, 19.5), (3.5, 20.5), (4.5, 15.0)], color),
            Shape::line_segment([g.p(6.0, 18.0), g.p(13.5, 10.5)], Stroke::new((w / 18.0).max(1.0), shade(color, 0.45))),
        ],
        Icon::Eraser => vec![
            fill(&[(4.5, 15.5), (12.0, 5.0), (19.5, 10.5), (12.0, 21.0)], color),
            fill(&[(7.6, 18.0), (12.0, 11.8), (19.5, 10.5), (12.0, 21.0)], shade(color, 0.45)),
            Shape::line_segment([g.p(3.0, 21.0), g.p(21.0, 21.0)], stroke),
        ],
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
        // A pilcrow with an arrow beside it, the way Word marks a paragraph's direction:
        // the arrow on the side the paragraph starts from, pointing the way it is read.
        // The bowl is filled, because a hollow one closes up at this size.
        Icon::ParaLtr => vec![
            fill(&[(13.0, 4.5), (20.0, 4.5), (20.0, 12.5), (13.0, 12.5)], color),
            path(&[(17.0, 4.5), (17.0, 20.0)]),
            path(&[(20.0, 4.5), (20.0, 20.0)]),
            path(&[(3.5, 12.0), (10.0, 12.0)]),
            path(&[(7.5, 9.0), (10.5, 12.0), (7.5, 15.0)]),
        ],
        Icon::ParaRtl => vec![
            fill(&[(4.0, 4.5), (11.0, 4.5), (11.0, 12.5), (4.0, 12.5)], color),
            path(&[(7.0, 4.5), (7.0, 20.0)]),
            path(&[(4.0, 4.5), (4.0, 20.0)]),
            path(&[(20.5, 12.0), (14.0, 12.0)]),
            path(&[(16.5, 9.0), (13.5, 12.0), (16.5, 15.0)]),
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

    pub const ALL: [Icon; 55] = [
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
        Icon::ParaLtr,
        Icon::ParaRtl,
        Icon::Bars,
        Icon::Eraser,
        Icon::Undo,
        Icon::Redo,
        Icon::Pointer,
        Icon::Lasso,
        Icon::Hand,
        Icon::Ruler,
        Icon::InsertSpace,
        Icon::Shapes,
        Icon::InkToText,
        Icon::Replay,
        Icon::Help,
        Icon::Pen,
        Icon::Plus,
        Icon::Minus,
        Icon::ZoomOut,
        Icon::ZoomIn,
        Icon::PageWidth,
        Icon::Calendar,
        Icon::Clock,
        Icon::Video,
        Icon::Mic,
        Icon::Spreadsheet,
        Icon::Clip,
        Icon::Sticker,
        Icon::Symbol,
        Icon::TableRow,
        Icon::TableColumn,
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

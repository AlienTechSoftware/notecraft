//! Paired visual tests for the icon set, in the style of the Craft UI components: each icon is
//! drawn by the production painter and by an independent reference painter with literal
//! geometry, on the same offscreen renderer, and the two images must be identical. No
//! tolerance, no committed pictures, no automatic baseline. Negative controls move one point
//! by a pixel and must be caught. Set `NOTECRAFT_UI_VISUAL_DIR` to keep before, after and
//! difference pictures for review.

use egui::{Color32, CornerRadius, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use egui_wgpu::{RenderState, Renderer, RendererOptions};
use image::RgbaImage;
use notecraft_ui_egui::icons::{self, Icon};
use std::path::PathBuf;

/// Every test here builds its own offscreen GPU renderer. Letting four of them do that at
/// once killed the process with a heap corruption inside the graphics driver, twice, while
/// the whole suite was running; one at a time is both enough and stable.
static GPU: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn gpu() -> std::sync::MutexGuard<'static, ()> {
    GPU.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

const CANVAS: f32 = 40.0;
const SIZE: f32 = 17.0;
const ALL: [Icon; 49] = [
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

const LIGHT_INK: Color32 = Color32::from_rgb(0x7A, 0x45, 0xB8);
const DARK_INK: Color32 = Color32::from_rgb(0xB0, 0x85, 0xE8);

fn name(icon: Icon) -> &'static str {
    match icon {
        Icon::Notebook => "notebook",
        Icon::SectionGroup => "section-group",
        Icon::SectionGroupOpen => "section-group-open",
        Icon::Section => "section",
        Icon::Page => "page",
        Icon::PageStack => "page-stack",
        Icon::Code => "code",
        Icon::Eraser => "eraser",
        Icon::Undo => "undo",
        Icon::Redo => "redo",
        Icon::Pointer => "pointer",
        Icon::Lasso => "lasso",
        Icon::Hand => "hand",
        Icon::Ruler => "ruler",
        Icon::InsertSpace => "insert-space",
        Icon::Shapes => "shapes",
        Icon::InkToText => "ink-to-text",
        Icon::Replay => "replay",
        Icon::Help => "help",
        Icon::Pen => "pen",
        Icon::Plus => "plus",
        Icon::Calendar => "calendar",
        Icon::Clock => "clock",
        Icon::Video => "video",
        Icon::Mic => "mic",
        Icon::Spreadsheet => "spreadsheet",
        Icon::Clip => "clip",
        Icon::Sticker => "sticker",
        Icon::Symbol => "symbol",
        Icon::TableRow => "table-row",
        Icon::TableColumn => "table-column",
        Icon::Search => "search",
        Icon::FullPage => "full-page",
        Icon::ExitFullPage => "exit-full-page",
        Icon::ListBullet => "list-bullet",
        Icon::ListNumber => "list-number",
        Icon::Todo => "todo",
        Icon::Table => "table",
        Icon::Picture => "picture",
        Icon::Attach => "attach",
        Icon::Link => "link",
        Icon::Equation => "equation",
        Icon::Diagram => "diagram",
        Icon::Quote => "quote",
        Icon::Bars => "bars",
        Icon::DirLtr => "dir-ltr",
        Icon::DirRtl => "dir-rtl",
        Icon::Sun => "sun",
        Icon::Moon => "moon",
    }
}

/// The shades of the two test colours, worked out by hand from the rule in `icons.rs`
/// (towards white when the colour is dark, towards black when it is light). Writing them
/// out is the point: if the production rule changes, these no longer match.
fn shade(ink: Color32, percent: u32) -> Color32 {
    let table: &[(u32, (u8, u8, u8))] = if ink == LIGHT_INK {
        // (122, 69, 184) is darker than the midpoint, so its shades go towards white.
        &[(20, (148, 106, 198)), (35, (168, 134, 208)), (45, (181, 152, 215)), (55, (195, 171, 223))]
    } else {
        // (176, 133, 232) is a light colour, so its shades go towards black.
        &[(20, (140, 106, 185)), (35, (114, 86, 150)), (45, (96, 73, 127)), (55, (79, 59, 104))]
    };
    let (r, g, b) = table.iter().find(|(p, _)| *p == percent).map(|(_, c)| *c).expect("a known shade");
    Color32::from_rgb(r, g, b)
}

/// The reference painter: no grid helper, just points scaled by hand.
fn reference(painter: &Painter, origin: Pos2, color: Color32, icon: Icon, nudge: f32) {
    // The same arithmetic as the painter's grid: scaling first and dividing after would be
    // equal on paper but not in floating point, and a circle can land half a pixel off.
    let at = |x: f32, y: f32| pos2(origin.x + SIZE * x / 24.0, origin.y + SIZE * y / 24.0);
    let poly = |pts: &[(f32, f32)], c: Color32| Shape::convex_polygon(pts.iter().map(|p| at(p.0, p.1)).collect(), c, Stroke::NONE);
    let stroke = Stroke::new((SIZE / 14.0).max(1.2), color);
    let thin = Stroke::new((SIZE / 16.0).max(1.0), shade(color, 55));
    let open = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|p| at(p.0, p.1)).collect(), stroke);
    let closed = |pts: &[(f32, f32)]| Shape::closed_line(pts.iter().map(|p| at(p.0, p.1)).collect(), stroke);
    let seg = |a: (f32, f32), b: (f32, f32)| Shape::line_segment([at(a.0, a.1), at(b.0, b.1)], stroke);
    let tab = |x0: f32, y0: f32, x1: f32, y1: f32, radius: u8, c: Color32| {
        Shape::rect_filled(Rect::from_min_max(at(x0, y0), at(x1, y1)), CornerRadius::same(radius), c)
    };
    let shapes: Vec<Shape> = match icon {
        Icon::Notebook => vec![
            poly(&[(5.0, 3.0), (19.0, 3.0), (19.0, 21.0), (5.0, 21.0)], color),
            poly(&[(5.0, 3.0), (8.5, 3.0), (8.5, 21.0), (5.0, 21.0)], shade(color, 35)),
            Shape::line_segment([at(11.5, 8.5), at(16.5, 8.5)], thin),
            Shape::line_segment([at(11.5, 12.0), at(16.5, 12.0)], thin),
        ],
        Icon::Undo => {
            vec![open(&[(9.0, 6.0), (5.0, 10.0), (9.0, 14.0)]), open(&[(5.0, 10.0), (15.0, 10.0), (18.0, 13.0), (18.0, 17.0), (15.0, 20.0)])]
        }
        Icon::Redo => {
            vec![open(&[(15.0, 6.0), (19.0, 10.0), (15.0, 14.0)]), open(&[(19.0, 10.0), (9.0, 10.0), (6.0, 13.0), (6.0, 17.0), (9.0, 20.0)])]
        }
        Icon::Pointer => vec![poly(&[(6.0, 3.0), (18.5, 13.5), (12.5, 14.0), (15.5, 20.5), (12.0, 22.0), (9.0, 15.5), (6.0, 19.5)], color)],
        Icon::Lasso => vec![closed(&[(12.0, 4.0), (19.0, 8.5), (16.5, 15.0), (7.5, 15.0), (5.0, 8.5)]), open(&[(8.0, 15.0), (8.0, 21.0)])],
        Icon::Hand => vec![poly(
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
        Icon::Ruler => vec![
            closed(&[(3.5, 15.0), (15.0, 3.5), (20.5, 9.0), (9.0, 20.5)]),
            seg((7.0, 11.5), (9.5, 14.0)),
            seg((10.0, 8.5), (12.5, 11.0)),
            seg((13.0, 5.5), (15.5, 8.0)),
        ],
        Icon::InsertSpace => {
            vec![seg((4.0, 12.0), (20.0, 12.0)), open(&[(9.0, 7.0), (12.0, 4.0), (15.0, 7.0)]), open(&[(9.0, 17.0), (12.0, 20.0), (15.0, 17.0)])]
        }
        Icon::Shapes => {
            vec![Shape::circle_stroke(at(9.5, 9.5), SIZE * 5.5 / 24.0, stroke), closed(&[(11.5, 11.5), (20.5, 11.5), (20.5, 20.5), (11.5, 20.5)])]
        }
        Icon::InkToText => vec![
            open(&[(4.0, 17.0), (6.5, 7.0), (9.0, 17.0), (11.5, 7.0), (14.0, 17.0)]),
            seg((16.5, 8.0), (20.5, 8.0)),
            seg((16.5, 12.5), (20.5, 12.5)),
            seg((16.5, 17.0), (20.5, 17.0)),
        ],
        Icon::Replay => vec![
            open(&[(18.5, 7.5), (18.5, 12.0), (14.0, 12.0)]),
            open(&[(18.0, 12.0), (16.0, 17.5), (10.5, 19.5), (5.5, 16.5), (4.5, 11.0), (8.0, 6.5), (13.5, 5.5), (18.0, 8.5)]),
        ],
        Icon::Help => vec![
            Shape::circle_stroke(at(12.0, 12.0), SIZE * 8.5 / 24.0, stroke),
            open(&[(9.5, 9.5), (10.5, 7.5), (13.5, 7.5), (14.5, 9.5), (13.5, 11.5), (12.0, 12.5), (12.0, 14.5)]),
            Shape::circle_filled(at(12.0, 17.5), SIZE / 20.0, color),
        ],
        Icon::Calendar => vec![
            closed(&[(4.0, 6.5), (20.0, 6.5), (20.0, 20.5), (4.0, 20.5)]),
            seg((4.0, 11.0), (20.0, 11.0)),
            seg((8.5, 3.5), (8.5, 8.0)),
            seg((15.5, 3.5), (15.5, 8.0)),
        ],
        Icon::Clock => {
            vec![Shape::circle_stroke(at(12.0, 12.5), SIZE * 8.5 / 24.0, stroke), seg((12.0, 12.5), (12.0, 7.0)), seg((12.0, 12.5), (16.0, 14.5))]
        }
        Icon::Video => vec![closed(&[(3.5, 5.5), (20.5, 5.5), (20.5, 18.5), (3.5, 18.5)]), poly(&[(10.0, 9.0), (16.0, 12.0), (10.0, 15.0)], color)],
        Icon::Mic => vec![
            closed(&[(9.5, 3.5), (14.5, 3.5), (14.5, 13.5), (9.5, 13.5)]),
            open(&[(6.5, 11.0), (6.5, 13.5), (12.0, 17.5), (17.5, 13.5), (17.5, 11.0)]),
            seg((12.0, 17.5), (12.0, 20.5)),
        ],
        Icon::Spreadsheet => vec![
            poly(&[(4.0, 5.5), (20.0, 5.5), (20.0, 9.5), (4.0, 9.5)], shade(color, 45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 20.5), (4.0, 20.5)]),
            seg((4.0, 9.5), (20.0, 9.5)),
            seg((4.0, 15.0), (20.0, 15.0)),
            seg((12.0, 9.5), (12.0, 20.5)),
        ],
        Icon::Clip => vec![
            open(&[(5.0, 9.5), (5.0, 5.0), (9.5, 5.0)]),
            open(&[(14.5, 5.0), (19.0, 5.0), (19.0, 9.5)]),
            open(&[(19.0, 14.5), (19.0, 19.0), (14.5, 19.0)]),
            open(&[(9.5, 19.0), (5.0, 19.0), (5.0, 14.5)]),
        ],
        Icon::Sticker => vec![
            Shape::circle_stroke(at(12.0, 12.0), SIZE * 8.5 / 24.0, stroke),
            Shape::circle_filled(at(9.0, 10.0), SIZE / 20.0, color),
            Shape::circle_filled(at(15.0, 10.0), SIZE / 20.0, color),
            open(&[(8.0, 14.5), (10.0, 17.0), (14.0, 17.0), (16.0, 14.5)]),
        ],
        Icon::Symbol => vec![open(&[
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
        Icon::TableRow => vec![
            poly(&[(4.0, 10.0), (20.0, 10.0), (20.0, 14.5), (4.0, 14.5)], shade(color, 45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 19.0), (4.0, 19.0)]),
            seg((4.0, 10.0), (20.0, 10.0)),
            seg((4.0, 14.5), (20.0, 14.5)),
        ],
        Icon::TableColumn => vec![
            poly(&[(9.5, 5.5), (14.5, 5.5), (14.5, 19.0), (9.5, 19.0)], shade(color, 45)),
            closed(&[(4.0, 5.5), (20.0, 5.5), (20.0, 19.0), (4.0, 19.0)]),
            seg((9.5, 5.5), (9.5, 19.0)),
            seg((14.5, 5.5), (14.5, 19.0)),
        ],
        Icon::Plus => vec![seg((12.0, 4.5), (12.0, 19.5)), seg((4.5, 12.0), (19.5, 12.0))],
        Icon::Pen => vec![
            poly(&[(16.0, 3.5), (20.5, 8.0), (9.0, 19.5), (3.5, 20.5), (4.5, 15.0)], color),
            Shape::line_segment([at(6.0, 18.0), at(13.5, 10.5)], Stroke::new((SIZE / 18.0).max(1.0), shade(color, 45))),
        ],
        Icon::Eraser => vec![
            poly(&[(4.5, 15.5), (12.0, 5.0), (19.5, 10.5), (12.0, 21.0)], color),
            poly(&[(7.6, 18.0), (12.0, 11.8), (19.5, 10.5), (12.0, 21.0)], shade(color, 45)),
            Shape::line_segment([at(3.0, 21.0), at(21.0, 21.0)], stroke),
        ],
        Icon::Section => vec![tab(8.5, 3.5, 15.5, 20.5, 2, color)],
        Icon::SectionGroup => {
            vec![tab(13.5, 5.5, 18.0, 18.5, 2, shade(color, 45)), tab(9.5, 4.5, 14.0, 19.5, 2, shade(color, 20)), tab(5.5, 3.5, 10.0, 20.5, 2, color)]
        }
        Icon::SectionGroupOpen => {
            vec![tab(16.0, 7.0, 20.5, 20.0, 2, shade(color, 45)), tab(10.0, 5.0, 14.5, 20.0, 2, shade(color, 20)), tab(4.0, 3.5, 8.5, 20.0, 2, color)]
        }
        Icon::Page => vec![
            closed(&[(5.0, 3.0), (14.0, 3.0), (19.0, 8.0), (19.0, 21.0), (5.0, 21.0)]),
            open(&[(14.0, 3.0), (14.0, 8.0), (19.0, 8.0)]),
            seg((8.5, 13.0), (15.5, 13.0)),
            seg((8.5, 16.5), (15.5, 16.5)),
        ],
        Icon::PageStack => vec![
            open(&[(8.5, 3.0), (15.0, 3.0), (20.0, 8.0), (20.0, 15.0)]),
            closed(&[(4.5, 6.0), (12.0, 6.0), (17.0, 11.0), (17.0, 21.0), (4.5, 21.0)]),
            open(&[(12.0, 6.0), (12.0, 11.0), (17.0, 11.0)]),
            seg((7.5, 16.0), (14.0, 16.0)),
        ],
        Icon::Code => vec![open(&[(9.0, 6.0), (4.0, 12.0), (9.0, 18.0)]), open(&[(15.0, 6.0), (20.0, 12.0), (15.0, 18.0)])],
        Icon::Search => vec![Shape::circle_stroke(at(10.5, 10.5), SIZE * 5.5 / 24.0, stroke), seg((14.6, 14.6), (19.5, 19.5))],
        Icon::FullPage => vec![
            open(&[(14.0, 4.0), (20.0, 4.0), (20.0, 10.0)]),
            seg((20.0, 4.0), (14.5, 9.5)),
            open(&[(10.0, 20.0), (4.0, 20.0), (4.0, 14.0)]),
            seg((4.0, 20.0), (9.5, 14.5)),
        ],
        Icon::ExitFullPage => vec![
            open(&[(20.0, 10.0), (14.0, 10.0), (14.0, 4.0)]),
            seg((14.0, 10.0), (19.5, 4.5)),
            open(&[(4.0, 14.0), (10.0, 14.0), (10.0, 20.0)]),
            seg((10.0, 14.0), (4.5, 19.5)),
        ],
        Icon::Sun => {
            let mut v = vec![Shape::circle_stroke(at(12.0, 12.0), SIZE * 4.6 / 24.0, stroke)];
            for i in 0..8u8 {
                let a = std::f32::consts::TAU * f32::from(i) / 8.0;
                v.push(seg((12.0 + a.cos() * 7.4, 12.0 + a.sin() * 7.4), (12.0 + a.cos() * 9.6, 12.0 + a.sin() * 9.6)));
            }
            v
        }
        Icon::Moon => vec![
            Shape::circle_stroke(at(12.0, 12.0), SIZE * 7.4 / 24.0, stroke),
            open(&[(16.6, 5.0), (12.6, 7.2), (11.0, 11.6), (12.6, 16.0), (16.6, 19.0)]),
        ],
        Icon::ListBullet => vec![
            Shape::circle_filled(at(5.0, 7.0), SIZE / 22.0, color),
            Shape::circle_filled(at(5.0, 12.0), SIZE / 22.0, color),
            Shape::circle_filled(at(5.0, 17.0), SIZE / 22.0, color),
            seg((10.0, 7.0), (20.0, 7.0)),
            seg((10.0, 12.0), (20.0, 12.0)),
            seg((10.0, 17.0), (20.0, 17.0)),
        ],
        Icon::ListNumber => vec![
            seg((4.0, 5.5), (4.0, 9.0)),
            seg((3.0, 6.5), (4.0, 5.5)),
            open(&[(3.0, 11.0), (5.0, 11.0), (3.0, 14.5), (5.0, 14.5)]),
            open(&[(3.0, 16.5), (5.0, 16.5), (5.0, 18.0), (3.0, 18.0), (5.0, 18.0), (5.0, 19.5), (3.0, 19.5)]),
            seg((10.0, 7.0), (20.0, 7.0)),
            seg((10.0, 12.5), (20.0, 12.5)),
            seg((10.0, 18.0), (20.0, 18.0)),
        ],
        Icon::Todo => vec![closed(&[(4.0, 5.0), (20.0, 5.0), (20.0, 19.0), (4.0, 19.0)]), open(&[(7.5, 12.0), (10.5, 15.5), (16.5, 8.5)])],
        Icon::Table => vec![
            closed(&[(3.5, 5.0), (20.5, 5.0), (20.5, 19.0), (3.5, 19.0)]),
            seg((3.5, 9.5), (20.5, 9.5)),
            seg((3.5, 14.5), (20.5, 14.5)),
            seg((9.0, 5.0), (9.0, 19.0)),
            seg((15.0, 5.0), (15.0, 19.0)),
        ],
        Icon::Picture => vec![
            closed(&[(3.5, 5.0), (20.5, 5.0), (20.5, 19.0), (3.5, 19.0)]),
            Shape::circle_stroke(at(8.5, 9.5), SIZE * 1.6 / 24.0, stroke),
            open(&[(5.0, 17.0), (10.0, 12.0), (13.0, 15.0), (16.0, 11.5), (19.5, 16.5)]),
        ],
        Icon::Attach => vec![open(&[(16.5, 7.0), (9.0, 14.5), (9.0, 17.0), (11.5, 17.0), (19.0, 9.5), (19.0, 6.0), (15.5, 4.5), (7.0, 13.0)])],
        Icon::Link => vec![
            open(&[(10.0, 14.0), (7.0, 17.0), (4.5, 14.5), (7.5, 11.5)]),
            open(&[(14.0, 10.0), (17.0, 7.0), (19.5, 9.5), (16.5, 12.5)]),
            seg((9.5, 14.5), (14.5, 9.5)),
        ],
        Icon::Equation => vec![open(&[(4.0, 13.0), (7.0, 13.0), (10.0, 19.5), (14.0, 5.0), (20.0, 5.0)]), seg((11.5, 12.0), (18.0, 12.0))],
        Icon::Diagram => vec![
            closed(&[(3.5, 4.5), (10.5, 4.5), (10.5, 9.5), (3.5, 9.5)]),
            closed(&[(13.5, 14.5), (20.5, 14.5), (20.5, 19.5), (13.5, 19.5)]),
            seg((7.0, 9.5), (7.0, 17.0)),
            open(&[(7.0, 17.0), (13.5, 17.0)]),
            open(&[(11.5, 15.5), (13.5, 17.0), (11.5, 18.5)]),
        ],
        Icon::Quote => vec![
            open(&[(9.0, 6.0), (5.0, 9.0), (5.0, 13.0), (9.0, 13.0), (9.0, 9.0), (5.0, 9.0)]),
            open(&[(19.0, 6.0), (15.0, 9.0), (15.0, 13.0), (19.0, 13.0), (19.0, 9.0), (15.0, 9.0)]),
        ],
        Icon::Bars => vec![open(&[(4.0, 7.0), (20.0, 7.0)]), open(&[(4.0, 12.0), (20.0, 12.0)]), open(&[(4.0, 17.0), (20.0, 17.0)])],
        Icon::DirLtr => vec![
            open(&[(5.0, 6.0), (17.0, 6.0)]),
            open(&[(5.0, 10.0), (19.0, 10.0)]),
            open(&[(5.0, 14.0), (14.0, 14.0)]),
            open(&[(5.0, 19.0), (19.0, 19.0)]),
            open(&[(16.0, 16.0), (19.0, 19.0), (16.0, 22.0)]),
        ],
        Icon::DirRtl => vec![
            open(&[(19.0, 6.0), (7.0, 6.0)]),
            open(&[(19.0, 10.0), (5.0, 10.0)]),
            open(&[(19.0, 14.0), (10.0, 14.0)]),
            open(&[(19.0, 19.0), (5.0, 19.0)]),
            open(&[(8.0, 16.0), (5.0, 19.0), (8.0, 22.0)]),
        ],
    };
    for s in shapes {
        painter.add(s);
    }
    if nudge != 0.0 {
        // Negative control: one extra stray line, one pixel long.
        painter.line_segment([origin + vec2(2.0, 2.0), origin + vec2(2.0 + nudge, 2.0)], Stroke::new(1.0, color));
    }
}

/// One offscreen GPU device for the whole file. Building a device costs about half a second,
/// and this file used to build one per picture, some 350 of them: three minutes of a four-minute
/// push check. Each picture still gets its own renderer and textures, so nothing leaks from one
/// picture into the next; only the adapter, device and queue are shared.
fn shared() -> &'static RenderState {
    static STATE: std::sync::OnceLock<RenderState> = std::sync::OnceLock::new();
    STATE.get_or_init(|| create_render_state(default_wgpu_setup(), RendererOptions::PREDICTABLE))
}

fn fresh_renderer() -> WgpuTestRenderer {
    let base = shared();
    let renderer = Renderer::new(&base.device, base.target_format, RendererOptions::PREDICTABLE);
    WgpuTestRenderer::from_render_state(RenderState { renderer: std::sync::Arc::new(egui::mutex::RwLock::new(renderer)), ..base.clone() })
}

fn render(scale: f32, bg: Color32, draw: impl Fn(&Painter) + 'static) -> RgbaImage {
    let mut h = Harness::builder().with_size(vec2(CANVAS, CANVAS)).with_pixels_per_point(scale).renderer(fresh_renderer()).build_ui(move |ui| {
        let screen = ui.max_rect();
        ui.set_clip_rect(screen);
        ui.painter().rect_filled(screen, 0.0, bg);
        draw(ui.painter());
    });
    h.run();
    h.render().expect("an offscreen renderer is required for the visual suite")
}

fn changed(a: &RgbaImage, b: &RgbaImage) -> usize {
    assert_eq!(a.dimensions(), b.dimensions());
    a.pixels().zip(b.pixels()).filter(|(x, y)| x != y).count()
}

fn keep(label: &str, before: &RgbaImage, after: &RgbaImage) {
    let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") else { return };
    let dir = PathBuf::from(dir).join("icons");
    let _ = std::fs::create_dir_all(&dir);
    let _ = before.save(dir.join(format!("{label}-before.png")));
    let _ = after.save(dir.join(format!("{label}-after.png")));
    let mut diff = after.clone();
    for (x, y, p) in diff.enumerate_pixels_mut() {
        if before.get_pixel(x, y) != after.get_pixel(x, y) {
            *p = image::Rgba([255, 0, 255, 255]);
        }
    }
    let _ = diff.save(dir.join(format!("{label}-difference.png")));
}

#[test]
fn every_icon_matches_its_reference_pixel_for_pixel() {
    let _gpu = gpu();
    let origin = pos2(10.0, 10.0);
    let mut pairs = 0;
    for icon in ALL {
        // One scale per theme, not every pairing. Every render here builds an offscreen
        // GPU device of its own, and this test is the whole suite's limit: at 39 icons,
        // two themes and three scales it asked for 468 of them in one process and the
        // process died on the way out ("Failed to initialise snmalloc"), leaving the
        // binary locked so the next build could not replace it (work/issues/007). Every
        // icon is still checked in both themes and at both a whole and a doubled scale;
        // what is given up is checking each icon at every combination of the two.
        for (theme, bg, ink, scale) in
            [("light", Color32::from_rgb(247, 247, 249), LIGHT_INK, 1.0_f32), ("dark", Color32::from_rgb(27, 25, 34), DARK_INK, 2.0)]
        {
            {
                let rect = Rect::from_min_size(origin, vec2(SIZE, SIZE));
                let production = render(scale, bg, move |p| icons::paint(p, rect, icon, ink));
                let expected = render(scale, bg, move |p| reference(p, origin, ink, icon, 0.0));
                keep(&format!("{}-{theme}-{scale}x", name(icon)), &expected, &production);
                assert!(production.pixels().any(|p| *p != *production.get_pixel(0, 0)), "{} draws something", name(icon));
                assert_eq!(
                    changed(&expected, &production),
                    0,
                    "{} in {theme} at {scale}x differs from its reference ({}x{} each)",
                    name(icon),
                    production.width(),
                    production.height()
                );
                pairs += 1;
            }
        }
    }
    assert_eq!(pairs, 98);
}

#[test]
fn negative_controls_are_caught() {
    let _gpu = gpu();
    let origin = pos2(10.0, 10.0);
    let bg = Color32::from_rgb(247, 247, 249);
    let ink = LIGHT_INK;
    for icon in ALL {
        let rect = Rect::from_min_size(origin, vec2(SIZE, SIZE));
        let production = render(2.0, bg, move |p| icons::paint(p, rect, icon, ink));
        let stray = render(2.0, bg, move |p| reference(p, origin, ink, icon, 1.0));
        assert!(changed(&stray, &production) > 0, "{}: a one pixel change must not pass", name(icon));
        let other_color = render(2.0, bg, move |p| icons::paint(p, rect, icon, Color32::from_rgb(0x7a, 0x45, 0xb9)));
        assert!(changed(&other_color, &production) > 0, "{}: a one step colour change must not pass", name(icon));
        let moved = render(2.0, bg, move |p| icons::paint(p, Rect::from_min_size(origin + vec2(0.5, 0.0), vec2(SIZE, SIZE)), icon, ink));
        assert!(changed(&moved, &production) > 0, "{}: a half pixel shift must not pass", name(icon));
    }
}

#[test]
fn icons_are_distinct_from_each_other() {
    let _gpu = gpu();
    let origin = pos2(10.0, 10.0);
    let bg = Color32::WHITE;
    let images: Vec<RgbaImage> = ALL
        .iter()
        .map(|icon| render(2.0, bg, move |p| icons::paint(p, Rect::from_min_size(origin, vec2(SIZE, SIZE)), *icon, Color32::BLACK)))
        .collect();
    for i in 0..images.len() {
        for j in i + 1..images.len() {
            assert!(changed(&images[i], &images[j]) > 20, "{} and {} look alike", name(ALL[i]), name(ALL[j]));
        }
    }
}

/// The notebook and the section carry their colour, so a coloured notebook is visible at a
/// glance. The page outline stays the same ink it is given.
#[test]
fn coloured_icons_use_the_colour_they_are_given() {
    let _gpu = gpu();
    let origin = pos2(10.0, 10.0);
    let bg = Color32::WHITE;
    let rect = Rect::from_min_size(origin, vec2(SIZE, SIZE));
    for icon in [Icon::Notebook, Icon::Section, Icon::SectionGroup] {
        let teal = Color32::from_rgb(0x0F, 0x82, 0x8C);
        let red = Color32::from_rgb(0xC0, 0x39, 0x2B);
        let a = render(2.0, bg, move |p| icons::paint(p, rect, icon, teal));
        let b = render(2.0, bg, move |p| icons::paint(p, rect, icon, red));
        assert!(changed(&a, &b) > 100, "{} ignores its colour", name(icon));
        // The icon is filled, not an outline: a good share of its box is painted.
        let filled = a.pixels().filter(|p| **p != image::Rgba([255, 255, 255, 255])).count();
        assert!(filled > 200, "{} is not a solid shape ({filled} pixels)", name(icon));
    }
}

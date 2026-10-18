//! Draws a formula from `notecraft-math` with egui.

use egui::text::{LayoutJob, TextFormat};
use egui::{Align2, Color32, FontId, Painter, Pos2, Response, Sense, Stroke, Ui, vec2};
use notecraft_math::{Item, Layout, Metrics, layout, parse};

struct EguiMetrics<'a> {
    painter: &'a Painter,
}

impl Metrics for EguiMetrics<'_> {
    fn width(&self, text: &str, size: f32, _italic: bool) -> f32 {
        self.painter.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE).size().x
    }
}

/// The share of a text row's height that lies above the baseline in egui's fonts.
const BASELINE: f32 = 0.82;

/// Lays out `tex` at `size` points with the UI's fonts.
pub fn measure(ui: &Ui, tex: &str, size: f32) -> Layout {
    layout(&parse(tex), size, &EguiMetrics { painter: ui.painter() })
}

fn paint(painter: &Painter, origin: Pos2, l: &Layout, color: Color32) {
    for item in &l.items {
        match item {
            Item::Text { x, y, text, size, italic } => {
                let job = {
                    let mut j = LayoutJob::default();
                    j.append(text, 0.0, TextFormat { font_id: FontId::proportional(*size), color, italics: *italic, ..TextFormat::default() });
                    j
                };
                let galley = painter.layout_job(job);
                let top = origin.y + y - galley.size().y * BASELINE;
                painter.galley(Pos2::new(origin.x + x, top), galley, color);
            }
            Item::Line { from, to, width } => {
                painter.line_segment(
                    [Pos2::new(origin.x + from.0, origin.y + from.1), Pos2::new(origin.x + to.0, origin.y + to.1)],
                    Stroke::new(*width, color),
                );
            }
            Item::Path { points, width } => {
                let pts: Vec<Pos2> = points.iter().map(|(x, y)| Pos2::new(origin.x + x, origin.y + y)).collect();
                painter.add(egui::Shape::line(pts, Stroke::new(*width, color)));
            }
        }
    }
}

/// An inline formula, as wide as it needs to be.
pub fn inline(ui: &mut Ui, tex: &str, size: f32, color: Color32) -> Response {
    let l = measure(ui, tex, size);
    let (rect, resp) = ui.allocate_exact_size(vec2(l.width + 2.0, l.height()), Sense::hover());
    paint(ui.painter(), rect.min + vec2(1.0, 0.0), &l, color);
    resp.on_hover_text(tex)
}

/// A display formula, centred in the available width.
pub fn display(ui: &mut Ui, tex: &str, size: f32, color: Color32) -> Response {
    let l = measure(ui, tex, size);
    let full = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(full.max(l.width), l.height() + 4.0), Sense::hover());
    let x = rect.min.x + ((rect.width() - l.width) / 2.0).max(0.0);
    paint(ui.painter(), Pos2::new(x, rect.min.y + 2.0), &l, color);
    let _ = Align2::CENTER_CENTER;
    resp.on_hover_text(tex)
}

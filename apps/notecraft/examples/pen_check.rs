//! Does a pen's pressure reach a NoteCraft window? Scribble here and see.
//!
//! `cargo run -p notecraft-ui-egui --example pen_check`
//!
//! This is the spike named first in `work\requests\pen-and-drawing.md`, not the drawing
//! feature. It opens a window with nothing but a sheet to scribble on. Each stroke is drawn
//! with its width following the pressure reported for every point, so a pen that is read
//! properly makes a stroke that thickens as it is pressed, and a mouse makes an even one.
//! The readings are printed at the top: whether a pen was seen at all, the pressure now, the
//! hardest press so far, and how many readings arrived.
//!
//! What it proves, and what it does not: a thickening stroke means winit's `WM_POINTER` path
//! is delivering real pen pressure to us on this machine. An even stroke from a real pen
//! means the platform is not reading it, which on anything but Windows is expected (see
//! `stylus.rs`). Tilt and the eraser end are not read by winit anywhere, so they are not
//! shown here at all.

use eframe::egui;
use notecraft_ui_egui::stylus::Stylus;

/// One point of a stroke: where it was and how hard the pen was pressed there.
#[derive(Clone, Copy)]
struct Dab {
    at: egui::Pos2,
    pressure: f32,
}

#[derive(Default)]
struct PenCheck {
    stylus: Stylus,
    strokes: Vec<Vec<Dab>>,
    drawing: bool,
    width: f32,
}

impl eframe::App for PenCheck {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().input(|i| self.stylus.update(&i.events));
        let sample = self.stylus.sample();
        let (highest, readings) = self.stylus.seen();
        if self.width <= 0.0 {
            self.width = 14.0;
        }

        let found =
            if sample.pen { "A pen is reporting pressure." } else { "No pressure reported yet: a mouse, or a pen this platform does not read." };
        ui.label(egui::RichText::new(found).strong());
        ui.horizontal(|ui| {
            ui.label(format!("now {:.3}", sample.pressure));
            ui.separator();
            ui.label(format!("hardest {highest:.3}"));
            ui.separator();
            ui.label(format!("{readings} readings"));
            ui.separator();
            ui.add(egui::Slider::new(&mut self.width, 2.0..=48.0).text("widest"));
            if ui.button("Clear").clicked() {
                self.strokes.clear();
            }
        });
        ui.add_space(4.0);

        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);

        if resp.drag_started() {
            self.strokes.push(Vec::new());
            self.drawing = true;
        }
        if resp.drag_stopped() {
            self.drawing = false;
        }
        if self.drawing
            && let Some(at) = resp.interact_pointer_pos()
        {
            // A pen that is down but reporting nothing is resting, not writing.
            let pressure = if sample.pen { sample.pressure } else { 1.0 };
            if let Some(stroke) = self.strokes.last_mut() {
                stroke.push(Dab { at, pressure });
            }
        }

        // Each pair of points is its own line, as thick as the pressure between them.
        // Crude on purpose: real ink will be one outline per stroke.
        let ink = egui::Color32::from_rgb(40, 30, 80);
        for stroke in &self.strokes {
            for pair in stroke.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let width = (self.width * (a.pressure + b.pressure) / 2.0).max(0.6);
                painter.line_segment([a.at, b.at], egui::Stroke::new(width, ink));
                painter.circle_filled(b.at, width / 2.0, ink);
            }
        }
        if self.strokes.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Scribble here with a pen, then with the mouse, and compare the strokes",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(150),
            );
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 700.0]).with_title("NoteCraft pen check"),
        ..Default::default()
    };
    eframe::run_native("notecraft-pen-check", options, Box::new(|_cc| Ok(Box::<PenCheck>::default())))
}

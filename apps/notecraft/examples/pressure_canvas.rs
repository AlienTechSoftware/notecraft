//! A focused pressure and sensitivity test canvas.
//!
//! Run with `cargo run -p notecraft --example pressure_canvas`, or use the launcher in
//! `SKILLS/nc-uiux-drawing/tools/pressure_canvas.ps1`.
//!
//! The left panel shows the raw pressure stream and synthetic reference widths. The large
//! canvas records the real stroke exactly as the renderer currently interprets it. This keeps
//! input delivery, sensitivity, selected width, and visual response easy to compare.

use eframe::egui;
use notecraft_ui_egui::stylus::Stylus;

const PRESSURES: [f32; 6] = [0.0, 0.1, 0.25, 0.5, 0.75, 1.0];

#[derive(Clone, Copy)]
struct Dab {
    at: egui::Pos2,
    pressure: f32,
}

#[derive(Default)]
struct PressureCanvas {
    stylus: Stylus,
    strokes: Vec<Vec<Dab>>,
    pressure_history: Vec<f32>,
    drawing: bool,
    sensitivity: u8,
    maximum_width: f32,
}

impl PressureCanvas {
    fn gain(&self) -> f32 {
        let share = f32::from(self.sensitivity) / 50.0;
        if share <= 1.0 { 0.3 + 0.7 * share } else { 1.0 + 2.0 * (share - 1.0) }
    }

    fn width_at(&self, pressure: f32) -> f32 {
        let pressure = pressure.clamp(0.0, 1.0);
        let felt = pressure.powf(1.0 / self.gain());
        self.maximum_width * (0.25 + 0.75 * felt)
    }

    fn paint_stroke(&self, painter: &egui::Painter, stroke: &[Dab], colour: egui::Color32) {
        for pair in stroke.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let width = self.width_at((a.pressure + b.pressure) / 2.0).max(0.6);
            painter.line_segment([a.at, b.at], egui::Stroke::new(width, colour));
            painter.circle_filled(b.at, width / 2.0, colour);
        }
        if let Some(last) = stroke.last() {
            painter.circle_filled(last.at, self.width_at(last.pressure) / 2.0, colour);
        }
    }
}

impl eframe::App for PressureCanvas {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        ui.ctx().input(|i| self.stylus.update(&i.events));
        let sample = self.stylus.sample();
        let (highest, readings) = self.stylus.seen();
        if sample.down || sample.pen {
            self.pressure_history.push(sample.pressure);
            if self.pressure_history.len() > 240 {
                self.pressure_history.remove(0);
            }
        }
        if self.maximum_width <= 0.0 {
            self.maximum_width = 12.0;
            self.sensitivity = 100;
        }

        ui.heading("NoteCraft pressure canvas");
        ui.label("Compare the raw tablet reading with the rendered width. Draw on the canvas with a pen, then with a mouse.");
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("PRESSURE  {:.3}", sample.pressure)).size(22.0).strong());
            ui.add(egui::ProgressBar::new(sample.pressure.clamp(0.0, 1.0)).desired_width(260.0).text(format!("{:.0}%", sample.pressure * 100.0)));
        });
        let (meter, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 54.0), egui::Sense::hover());
        let painter = ui.painter_at(meter);
        painter.rect_filled(meter, 4.0, egui::Color32::from_gray(242));
        let filled = egui::Rect::from_min_max(meter.min, egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.bottom()));
        painter.rect_filled(filled, 4.0, egui::Color32::from_rgb(112, 54, 190));
        painter.text(
            meter.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            "raw pressure 0",
            egui::FontId::proportional(11.0),
            egui::Color32::DARK_GRAY,
        );
        painter.text(
            meter.right_top() - egui::vec2(8.0, -8.0),
            egui::Align2::RIGHT_TOP,
            "1.0 maximum",
            egui::FontId::proportional(11.0),
            egui::Color32::DARK_GRAY,
        );
        painter.circle_filled(
            egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.center().y),
            8.0,
            egui::Color32::WHITE,
        );
        painter.circle_stroke(
            egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.center().y),
            8.0,
            egui::Stroke::new(2.0, egui::Color32::from_rgb(112, 54, 190)),
        );
        ui.label(egui::RichText::new("Recent pressure").strong());
        let (history, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.0), egui::Sense::hover());
        let painter = ui.painter_at(history);
        painter.rect_filled(history, 4.0, egui::Color32::from_gray(248));
        if self.pressure_history.len() > 1 {
            let points: Vec<egui::Pos2> = self
                .pressure_history
                .iter()
                .enumerate()
                .map(|(index, pressure)| {
                    egui::pos2(
                        history.left() + history.width() * index as f32 / (self.pressure_history.len() - 1) as f32,
                        history.bottom() - history.height() * pressure.clamp(0.0, 1.0),
                    )
                })
                .collect();
            painter.add(egui::Shape::line(points, egui::Stroke::new(2.0, egui::Color32::from_rgb(112, 54, 190))));
        }
        ui.horizontal(|ui| {
            ui.label(if sample.pen { "Pressure pen detected" } else { "No pressure pen detected" });
            ui.separator();
            ui.label(format!("now {:.3}", sample.pressure));
            ui.separator();
            ui.label(format!("hardest {highest:.3}"));
            ui.separator();
            ui.label(format!("{readings} readings"));
            if ui.button("Clear").clicked() {
                self.strokes.clear();
                self.pressure_history.clear();
            }
        });
        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.sensitivity, 0..=100).text("sensitivity"));
            ui.add(egui::Slider::new(&mut self.maximum_width, 2.0..=60.0).text("full width"));
            ui.label(format!("gain {:.2}", self.gain()));
        });

        ui.separator();
        ui.label(egui::RichText::new("Synthetic pressure reference").strong());
        let (reference, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::hover());
        let painter = ui.painter_at(reference);
        painter.rect_filled(reference, 4.0, egui::Color32::from_gray(248));
        let step = reference.width() / PRESSURES.len() as f32;
        for (index, pressure) in PRESSURES.iter().enumerate() {
            let x = reference.left() + step * (index as f32 + 0.5);
            let width = self.width_at(*pressure);
            painter.line_segment(
                [egui::pos2(x - 18.0, reference.center().y), egui::pos2(x + 18.0, reference.center().y)],
                egui::Stroke::new(width, egui::Color32::from_rgb(48, 34, 88)),
            );
            painter.text(
                egui::pos2(x, reference.bottom() - 4.0),
                egui::Align2::CENTER_BOTTOM,
                format!("{pressure:.2}"),
                egui::FontId::proportional(11.0),
                egui::Color32::DARK_GRAY,
            );
        }

        ui.add_space(8.0);
        let (canvas, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
        let painter = ui.painter_at(canvas);
        painter.rect_filled(canvas, 0.0, egui::Color32::WHITE);
        if response.drag_started() {
            self.strokes.push(Vec::new());
            self.drawing = true;
        }
        if response.drag_stopped() {
            self.drawing = false;
        }
        if self.drawing
            && let Some(at) = response.interact_pointer_pos()
        {
            let pressure = if sample.pen { sample.pressure } else { 1.0 };
            if let Some(stroke) = self.strokes.last_mut() {
                stroke.push(Dab { at, pressure });
            }
            ui.ctx().request_repaint();
        }
        for stroke in &self.strokes {
            self.paint_stroke(&painter, stroke, egui::Color32::from_rgb(35, 28, 55));
        }
        if self.strokes.is_empty() {
            painter.text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "Draw here with light, medium, and hard pressure",
                egui::FontId::proportional(16.0),
                egui::Color32::from_gray(145),
            );
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 760.0]).with_title("NoteCraft pressure canvas"),
        ..Default::default()
    };
    eframe::run_native("notecraft-pressure-canvas", options, Box::new(|_cc| Ok(Box::<PressureCanvas>::default())))
}

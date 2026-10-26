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
        let diagnostics = self.stylus.diagnostics();
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

        let dark = ui.visuals().dark_mode;
        let page_bg = if dark { egui::Color32::from_rgb(25, 24, 31) } else { egui::Color32::from_rgb(248, 248, 250) };
        let panel_bg = if dark { egui::Color32::from_rgb(38, 36, 48) } else { egui::Color32::WHITE };
        let text = if dark { egui::Color32::from_rgb(242, 240, 248) } else { egui::Color32::from_rgb(38, 36, 45) };
        let muted = if dark { egui::Color32::from_rgb(190, 184, 204) } else { egui::Color32::from_rgb(105, 101, 115) };
        let accent = egui::Color32::from_rgb(126, 78, 196);

        egui::Frame::new().fill(page_bg).inner_margin(egui::Margin::same(16)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(10.0, 8.0);

            ui.heading(egui::RichText::new("NoteCraft pressure canvas").color(text));
            ui.label(egui::RichText::new("Compare the raw tablet reading with the rendered width. Draw with a pen, then with a mouse.").color(muted));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("PRESSURE  {:.3}", sample.pressure)).size(22.0).strong().color(text));
                ui.add(egui::ProgressBar::new(sample.pressure.clamp(0.0, 1.0)).desired_width(260.0).text(format!("{:.0}%", sample.pressure * 100.0)));
            });
            let (meter, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), egui::Sense::hover());
            let painter = ui.painter_at(meter);
            painter.rect_filled(meter, 6.0, panel_bg);
            let filled =
                egui::Rect::from_min_max(meter.min, egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.bottom()));
            painter.rect_filled(filled, 6.0, accent);
            painter.text(meter.left_top() + egui::vec2(10.0, 7.0), egui::Align2::LEFT_TOP, "raw pressure 0", egui::FontId::proportional(11.0), muted);
            painter.text(meter.right_top() - egui::vec2(10.0, -7.0), egui::Align2::RIGHT_TOP, "1.0 maximum", egui::FontId::proportional(11.0), muted);
            painter.circle_filled(egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.center().y), 8.0, panel_bg);
            painter.circle_stroke(
                egui::pos2(meter.left() + meter.width() * sample.pressure.clamp(0.0, 1.0), meter.center().y),
                8.0,
                egui::Stroke::new(2.0, accent),
            );
            ui.label(egui::RichText::new("Recent pressure").strong().color(text));
            let (history, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), egui::Sense::hover());
            let painter = ui.painter_at(history);
            painter.rect_filled(history, 6.0, panel_bg);
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
                painter.add(egui::Shape::line(points, egui::Stroke::new(2.0, accent)));
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(if sample.pen { "Pressure pen detected" } else { "No pressure pen detected" }).color(if sample.pen {
                    egui::Color32::from_rgb(60, 170, 105)
                } else {
                    egui::Color32::from_rgb(210, 115, 70)
                }));
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
            ui.label(
                egui::RichText::new(format!(
                    "Events {} | pointer {} | mouse {} | buttons {} | touch {} | force {} | no force {}",
                    diagnostics.events,
                    diagnostics.pointer_moved,
                    diagnostics.mouse_moved,
                    diagnostics.pointer_buttons,
                    diagnostics.touch_events,
                    diagnostics.touch_with_force,
                    diagnostics.touch_without_force,
                ))
                .small()
                .color(muted),
            );
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.sensitivity, 0..=100).text("sensitivity"));
                ui.add(egui::Slider::new(&mut self.maximum_width, 2.0..=60.0).text("full width"));
                ui.label(format!("gain {:.2}", self.gain()));
            });

            ui.separator();
            ui.label(egui::RichText::new("Synthetic pressure reference").strong().color(text));
            let (reference, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 70.0), egui::Sense::hover());
            let painter = ui.painter_at(reference);
            painter.rect_filled(reference, 6.0, panel_bg);
            let step = reference.width() / PRESSURES.len() as f32;
            for (index, pressure) in PRESSURES.iter().enumerate() {
                let x = reference.left() + step * (index as f32 + 0.5);
                let width = self.width_at(*pressure);
                painter.line_segment(
                    [egui::pos2(x - 18.0, reference.center().y), egui::pos2(x + 18.0, reference.center().y)],
                    egui::Stroke::new(width, text),
                );
                painter.text(
                    egui::pos2(x, reference.bottom() - 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    format!("{pressure:.2}"),
                    egui::FontId::proportional(11.0),
                    muted,
                );
            }

            ui.add_space(8.0);
            let (canvas, response) = ui.allocate_exact_size(ui.available_size(), egui::Sense::drag());
            let painter = ui.painter_at(canvas);
            painter.rect_filled(canvas, 0.0, egui::Color32::from_rgb(255, 255, 255));
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
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 760.0]).with_title("NoteCraft pressure canvas"),
        ..Default::default()
    };
    eframe::run_native("notecraft-pressure-canvas", options, Box::new(|_cc| Ok(Box::<PressureCanvas>::default())))
}

//! Small code-drawn locale markers. These avoid depending on platform emoji fonts.

use egui::{Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

/// Draws a representative country flag; pseudo-locales get a neutral X marker.
pub fn paint(ui: &mut Ui, code: &str) {
    let (rect, _) = ui.allocate_exact_size(vec2(20.0, 14.0), Sense::hover());
    paint_at(ui, rect, code);
}

pub fn paint_at(ui: &Ui, rect: Rect, code: &str) {
    let p = ui.painter();
    let c = |hex: u32| Color32::from_rgb(((hex >> 16) & 255) as u8, ((hex >> 8) & 255) as u8, (hex & 255) as u8);
    let white = Color32::WHITE;
    let red = c(0xD32F2F);
    let blue = c(0x1565C0);
    let green = c(0x168447);
    let dark = c(0x202020);
    let pale = c(0xF5F2E9);
    let stripe = |index: usize, color: Color32, count: usize| {
        let h = rect.height() / count as f32;
        let r = Rect::from_min_max(pos2(rect.left(), rect.top() + h * index as f32), pos2(rect.right(), rect.top() + h * (index + 1) as f32));
        p.rect_filled(r, 0.0, color);
    };
    match code.to_ascii_lowercase().as_str() {
        "de" => {
            stripe(0, dark, 3);
            stripe(1, red, 3);
            stripe(2, c(0xFBC02D), 3);
        }
        "en" | "en-xa" => {
            p.rect_filled(rect, 0.0, c(0xB22234));
            for i in 0..6 {
                let y = rect.top() + rect.height() * (i as f32 * 2.0 + 1.0) / 13.0;
                p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(rect.height() / 13.0, white));
            }
            p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * 0.46, rect.top() + rect.height() * 0.58)), 0.0, c(0x3C3B6E));
        }
        "es" => {
            stripe(0, red, 3);
            stripe(1, c(0xFFC400), 3);
            stripe(2, red, 3);
        }
        "fa" => {
            stripe(0, green, 3);
            stripe(1, white, 3);
            stripe(2, red, 3);
        }
        "fr" => {
            for i in 0..3 {
                let w = rect.width() / 3.0;
                p.rect_filled(
                    Rect::from_min_max(pos2(rect.left() + w * i as f32, rect.top()), pos2(rect.left() + w * (i + 1) as f32, rect.bottom())),
                    0.0,
                    [blue, white, red][i],
                );
            }
        }
        "he" => {
            p.rect_filled(rect, 0.0, white);
            for y in [0.22, 0.72] {
                let yy = rect.top() + rect.height() * y;
                p.line_segment([pos2(rect.left() + 2.0, yy), pos2(rect.right() - 2.0, yy)], Stroke::new(2.0, blue));
            }
        }
        "hi" => {
            stripe(0, c(0xFF9933), 3);
            stripe(1, white, 3);
            stripe(2, green, 3);
            p.circle_filled(rect.center(), 2.1, blue);
        }
        "ja" => {
            p.rect_filled(rect, 0.0, white);
            p.circle_filled(rect.center(), 3.4, red);
        }
        "ko" => {
            p.rect_filled(rect, 0.0, white);
            p.circle_filled(rect.center() + egui::vec2(0.0, -1.2), 2.4, red);
            p.circle_filled(rect.center() + egui::vec2(0.0, 1.2), 2.4, blue);
        }
        "ru" => {
            stripe(0, white, 3);
            stripe(1, c(0x0039A6), 3);
            stripe(2, red, 3);
        }
        "ur" => {
            p.rect_filled(rect, 0.0, green);
            p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * 0.23, rect.bottom())), 0.0, white);
            p.circle_filled(pos2(rect.left() + rect.width() * 0.68, rect.center().y), 3.2, white);
            p.circle_filled(pos2(rect.left() + rect.width() * 0.72, rect.center().y - 1.0), 2.6, green);
        }
        "zh" => {
            p.rect_filled(rect, 0.0, red);
            p.circle_filled(pos2(rect.left() + 5.0, rect.top() + 4.0), 1.9, c(0xFFDE00));
        }
        "ar" => {
            stripe(0, green, 3);
            stripe(1, white, 3);
            stripe(2, dark, 3);
            p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.left() + rect.width() * 0.24, rect.bottom())), 0.0, red);
        }
        "ar-xb" => {
            p.rect_filled(rect, 0.0, pale);
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "X", egui::FontId::proportional(11.0), blue);
        }
        _ => {
            p.rect_filled(rect, 0.0, pale);
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "?", egui::FontId::proportional(10.0), dark);
        }
    }
    p.rect_stroke(rect, 1.5, Stroke::new(0.7, c(0x707070)), egui::StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_demo_and_pseudo_locale_has_a_flag_mapping() {
        for code in ["ar", "de", "en", "es", "fa", "fr", "he", "hi", "ja", "ko", "ru", "ur", "zh", "en-XA", "ar-XB"] {
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| paint(ui, code));
            });
            assert!(!output.shapes.is_empty(), "{code} should paint a flag marker");
            output.textures_delta.clear();
        }
    }
}

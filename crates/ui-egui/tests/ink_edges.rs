//! How a stroke's edges are drawn on the screen.

use egui::vec2;
use egui_kittest::Harness;
use notecraft_ink::{Dab, Rgb, Stroke, Tool};

/// A diagonal stroke 3 points wide, drawn as the page draws ink, at one pixel per point.
fn render_diagonal() -> image::RgbaImage {
    let mut stroke = Stroke::new(Tool::Pen, Rgb(0, 0, 0), 3.0);
    for i in 0..60 {
        let t = i as f32 / 59.0;
        stroke.add(Dab::new(20.0 + 200.0 * t, 20.0 + 37.0 * t, 0.8));
    }
    let mut h = Harness::builder().with_size(vec2(240.0, 80.0)).with_pixels_per_point(1.0).build_ui(move |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::WHITE);
        notecraft_ui_egui::inkpad::paint_stroke(ui.painter(), egui::Pos2::ZERO, &stroke);
    });
    h.run();
    h.render().expect("an offscreen renderer")
}

/// The edges of a stroke fade into the paper, as egui's own lines do, rather than stepping from
/// ink to paper a whole pixel at a time. Ink was a bare triangle mesh, which egui does not smooth,
/// so a slanted line looked coarse and stair-stepped.
#[test]
fn a_stroke_has_smooth_edges() {
    let img = render_diagonal();
    let shade = |p: &image::Rgba<u8>| (u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])) / 3;
    let ink = img.pixels().filter(|p| shade(p) < 40).count();
    let between = img.pixels().filter(|p| (40..215).contains(&shade(p))).count();
    assert!(ink > 300, "the stroke is drawn: {ink} ink pixels");
    // Measured: about 290 in-between pixels with the edges smoothed, about a dozen without.
    assert!(between > 150, "the edges are smoothed: {between} in-between pixels beside {ink} ink pixels");
}

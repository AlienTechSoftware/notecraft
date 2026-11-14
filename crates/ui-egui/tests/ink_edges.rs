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

fn render_straight(
    tool: Tool,
    background: egui::Color32,
    pixels_per_point: f32,
    start: egui::Pos2,
    end: egui::Pos2,
    copies: usize,
) -> image::RgbaImage {
    let mut stroke = Stroke::new(tool, Rgb(255, 221, 51), 32.0);
    stroke.add(Dab::new(start.x, start.y, 1.0));
    stroke.add(Dab::new(end.x, end.y, 1.0));
    let mut h = Harness::builder().with_size(vec2(240.0, 240.0)).with_pixels_per_point(pixels_per_point).build_ui(move |ui| {
        ui.painter().rect_filled(ui.max_rect(), 0.0, background);
        for _ in 0..copies {
            notecraft_ui_egui::inkpad::paint_stroke(ui.painter(), egui::Pos2::ZERO, &stroke);
        }
    });
    h.run();
    h.render().expect("an offscreen renderer")
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn pixel(image: &image::RgbaImage, at: egui::Pos2, pixels_per_point: f32) -> [u8; 4] {
    image.get_pixel((at.x * pixels_per_point).round() as u32, (at.y * pixels_per_point).round() as u32).0
}

fn same_coverage(actual: [u8; 4], expected: [u8; 4], context: &str) {
    assert!(actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1), "{context}: {actual:?} should match {expected:?}");
}

/// A single translucent stroke must not paint its own inward cap halves twice.
/// Sample away from the antialiased boundary, on both paper colours and display scales.
#[test]
fn one_highlighter_stroke_has_uniform_opacity_through_its_round_ends() {
    let directions = [
        ("right", egui::pos2(60.0, 120.0), egui::pos2(180.0, 120.0)),
        ("left", egui::pos2(180.0, 120.0), egui::pos2(60.0, 120.0)),
        ("down", egui::pos2(120.0, 60.0), egui::pos2(120.0, 180.0)),
        ("up", egui::pos2(120.0, 180.0), egui::pos2(120.0, 60.0)),
        ("diagonal", egui::pos2(60.0, 80.0), egui::pos2(180.0, 160.0)),
        ("diagonal-reversed", egui::pos2(180.0, 160.0), egui::pos2(60.0, 80.0)),
        ("short", egui::pos2(118.0, 120.0), egui::pos2(122.0, 120.0)),
    ];
    for (paper, background) in [("light", egui::Color32::WHITE), ("dark", egui::Color32::from_gray(24))] {
        for scale in [1.0, 2.0] {
            for tool in [Tool::Pen, Tool::Highlighter] {
                for (direction, start, end) in directions {
                    let image = render_straight(tool, background, scale, start, end, 1);
                    let centre = pixel(&image, start + (end - start) / 2.0, scale);
                    if tool == Tool::Pen {
                        same_coverage(centre, [255, 221, 51, 255], "opaque pen control");
                    } else {
                        let (lo, hi) = if paper == "light" { (51, 255) } else { (24, 51) };
                        assert!((lo + 1..hi).contains(&centre[2]), "a highlighter remains translucent: {centre:?}");
                    }
                    if let Ok(dir) = std::env::var("NOTECRAFT_UI_VISUAL_DIR") {
                        let dir = std::path::PathBuf::from(dir);
                        std::fs::create_dir_all(&dir).expect("visual directory");
                        image.save(dir.join(format!("highlighter-cap-{paper}-{scale}-{tool:?}-{direction}.png"))).expect("diagnostic picture");
                    }
                    let along = (end - start).normalized();
                    for (name, at) in [
                        ("start inside", start + along * 5.0),
                        ("end inside", end - along * 5.0),
                        ("start outside", start - along * 10.0),
                        ("end outside", end + along * 10.0),
                        ("start seam", start),
                        ("end seam", end),
                    ] {
                        same_coverage(pixel(&image, at, scale), centre, &format!("{paper}/{scale}/{tool:?}/{direction}/{name}"));
                    }
                }
            }
        }
    }
}

#[test]
fn separate_highlighter_strokes_still_build_up_ink() {
    let (start, end) = (egui::pos2(60.0, 120.0), egui::pos2(180.0, 120.0));
    let centre = egui::pos2(120.0, 120.0);
    let once = pixel(&render_straight(Tool::Highlighter, egui::Color32::WHITE, 1.0, start, end, 1), centre, 1.0);
    let twice = pixel(&render_straight(Tool::Highlighter, egui::Color32::WHITE, 1.0, start, end, 2), centre, 1.0);
    assert!(twice[2] < once[2], "separate strokes still composite normally: {once:?} then {twice:?}");
    assert!(twice[2] > 51, "two highlighter strokes are still translucent: {twice:?}");
}

#[test]
fn round_cap_edges_keep_their_antialiasing_fringe_at_both_display_scales() {
    for (scale, xs, ys, minimum) in [(1.0, 38..60, 98..142, 10), (2.0, 76..120, 196..284, 20)] {
        let image = render_straight(Tool::Pen, egui::Color32::WHITE, scale, egui::pos2(60.0, 120.0), egui::pos2(180.0, 120.0), 1);
        // Only the outward half of the starting cap: neither the ribbon nor its long edges.
        let mut between = 0;
        for x in xs {
            for y in ys.clone() {
                if (52..255).contains(&image.get_pixel(x, y)[2]) {
                    between += 1;
                }
            }
        }
        assert!(between > minimum, "the round cap has a physical-pixel fringe at {scale}x: {between} partly covered pixels");
    }
}

//! Real Stroke -> SVG -> existing SVG-loader regressions. No window or test doubles.
use notecraft_ink::{Dab, Drawing, Rgb, Stroke, Tool, svg};

#[test]
fn saved_svg_keeps_smoothed_ink_outside_the_raw_reading_bounds() {
    let mut stroke = Stroke::new(Tool::Pen, Rgb::BLACK, 4.0);
    for (x, y) in [(10.0, 0.0), (110.0, 0.0), (111.0, 0.0), (210.0, 200.0)] {
        stroke.add(Dab::new(x, y, 1.0));
    }
    let mut drawing = Drawing::new(240.0, 240.0);
    drawing.add(stroke);
    let mut expanded = drawing.clone();
    expanded.set_view_box(-10.0, -30.0, 260.0, 280.0);
    for flavour in [svg::Flavour::Plain, svg::Flavour::Editable] {
        let text = svg::write(&drawing, flavour);
        let control_text = svg::write(&expanded, flavour);
        assert_eq!(text.split_once("<title>").map(|p| p.1), control_text.split_once("<title>").map(|p| p.1), "the control changes only the viewport");
        let control = egui_extras::image::load_svg_bytes(control_text.as_bytes(), &Default::default()).expect("control raster");
        let image = egui_extras::image::load_svg_bytes(text.as_bytes(), &Default::default()).expect("saved raster");
        let (i, witness) = control
            .pixels
            .iter()
            .enumerate()
            .filter(|(i, _)| expanded.view_y + (*i / control.size[0]) as f32 + 0.5 < -3.0)
            .max_by_key(|(_, p)| p.a())
            .expect("overshoot raster pixels");
        assert!(witness.a() > 128, "the expanded control must contain clearly painted overshoot");
        let x = expanded.view_x + (i % control.size[0]) as f32 + 0.5;
        let y = expanded.view_y + (i / control.size[0]) as f32 + 0.5;
        let px = ((x - drawing.view_x) * image.size[0] as f32 / drawing.width).floor() as isize;
        let py = ((y - drawing.view_y) * image.size[1] as f32 / drawing.height).floor() as isize;
        let alpha = if px >= 0 && py >= 0 && px < image.size[0] as isize && py < image.size[1] as isize {
            image.pixels[py as usize * image.size[0] + px as usize].a()
        } else {
            0
        };
        assert!(alpha > 128, "{flavour:?}: saved viewport clips visible curve at ({x}, {y}), alpha={alpha}");
    }
}

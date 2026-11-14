use notecraft_ink::{Dab, Drawing, Rgb, Stroke, Tool, pick, shape, svg};

fn curved(tool: Tool, turn: usize, reverse: bool) -> Stroke {
    let mut stroke = Stroke::new(tool, Rgb::BLACK, 4.0);
    for (x, y, pressure) in [(10.0, 0.0, 0.2), (110.0, 0.0, 1.0), (111.0, 0.0, 0.4), (210.0, 200.0, 0.8)] {
        let (x, y) = match turn {
            0 => (x, y),
            1 => (-y, x),
            2 => (-x, -y),
            _ => (y, -x),
        };
        stroke.add(Dab::new(x, y, pressure));
    }
    if reverse {
        stroke.dabs.reverse();
    }
    stroke
}

fn contains(bounds: (f32, f32, f32, f32), point: (f32, f32)) -> bool {
    point.0 >= bounds.0 - 0.0001 && point.1 >= bounds.1 - 0.0001 && point.0 <= bounds.2 + 0.0001 && point.1 <= bounds.3 + 0.0001
}

#[test]
fn smoothed_bounds_include_visible_ink_and_selection_in_every_direction() {
    for tool in Tool::ALL {
        for turn in 0..4 {
            for reverse in [false, true] {
                let stroke = curved(tool, turn, reverse);
                let bounds = stroke.bounds().expect("stroke bounds");
                let ring = shape::outline(&stroke);
                let mut drawing = Drawing::new(240.0, 240.0);
                drawing.add(stroke);
                let selected = pick::bounds(&drawing, &[0]).expect("selection bounds");
                assert_eq!(drawing.bounds(), Some(bounds));
                assert_eq!(selected, bounds);
                let viewport = (drawing.view_x, drawing.view_y, drawing.view_x + drawing.width, drawing.view_y + drawing.height);
                for point in ring {
                    assert!(contains(bounds, point), "{tool:?} turn={turn} reverse={reverse}: {point:?} outside stroke/selection bounds {bounds:?}");
                    assert!(contains(viewport, point), "{point:?} outside saved viewport {viewport:?}");
                }
            }
        }
    }
}

#[test]
fn smoothed_bounds_preserve_full_width_padding_and_existing_sheet_area() {
    for count in [1, 2, 3] {
        let mut stroke = Stroke::new(Tool::Pen, Rgb::BLACK, 40.0);
        for x in [50.0, 100.0, 150.0].into_iter().take(count) {
            stroke.add(Dab::new(x, 60.0, 0.0));
        }
        assert_eq!(
            stroke.bounds(),
            Some((30.0, 40.0, 50.0 * count as f32 + 20.0, 80.0)),
            "keep selected maximum-width padding even with light pressure"
        );
    }
    let mut drawing = Drawing::new(600.0, 800.0);
    drawing.set_view_box(-30.0, -40.0, 600.0, 800.0);
    drawing.add(curved(Tool::Pen, 0, false));
    assert_eq!(
        (drawing.view_x, drawing.view_y, drawing.width, drawing.height),
        (-30.0, -40.0, 600.0, 800.0),
        "bounds must not shrink an existing sheet"
    );
}

#[test]
fn smoothed_bounds_svg_round_trip_keeps_readings_and_a_stable_viewport() {
    let stroke = curved(Tool::Pen, 0, false);
    let mut drawing = Drawing::new(240.0, 240.0);
    drawing.add(stroke.clone());
    assert!(drawing.view_y < -10.0, "the viewport includes the smoothed lobe above the raw readings");
    let text = svg::write(&drawing, svg::Flavour::Editable);
    let (back, flat) = svg::read(&text).expect("saved drawing");
    assert_eq!(flat, 0);
    assert_eq!(back.strokes, vec![stroke]);
    assert_eq!(svg::write(&back, svg::Flavour::Editable), text, "rereading must not keep growing or move the viewport");
}

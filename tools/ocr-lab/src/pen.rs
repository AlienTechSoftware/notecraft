//! The pen for digital drawings. The tools, colours and widths are the app's own: the rack and the
//! tool kinds come from `notecraft-ink`, so a pen here is the same pen as on a page in the app. The
//! lab paints the pen onto a raster picture, which is what OCR reads.

use image::RgbaImage;
use notecraft_ink::Tool;
use notecraft_ink::rack::{self, Nib};

/// The pens the lab offers, and which one is in hand.
pub struct Pen {
    pub rack: Vec<Nib>,
    pub selected: usize,
}

impl Pen {
    /// The app's starting rack, with its first pen in hand.
    pub fn new() -> Pen {
        Pen { rack: rack::fresh(), selected: 0 }
    }

    /// The nib in hand. The rack is never empty, so this always has one.
    pub fn nib(&self) -> Nib {
        self.rack.get(self.selected).copied().unwrap_or_else(|| rack::new_nib(Tool::Pen))
    }
}

/// How much the ribbon's sensitivity widens a stroke. The same rule as the app's: neutral at 50, growing
/// to four times the nib's width at 100, and no thinner below 50.
pub fn sensitivity_width(sensitivity: u8) -> f32 {
    if sensitivity <= 50 { 1.0 } else { 1.0 + 3.0 * (f32::from(sensitivity - 50) / 50.0) }
}

/// The gain the ribbon's sensitivity gives the pen, as the app computes it: 1 at the neutral 50, and
/// turned down or up toward the ink crate's limits.
pub fn gain(sensitivity: u8) -> f32 {
    let share = f32::from(sensitivity.min(100)) / 50.0;
    if share <= 1.0 {
        notecraft_ink::MIN_GAIN + (1.0 - notecraft_ink::MIN_GAIN) * share
    } else {
        1.0 + (notecraft_ink::MAX_GAIN - 1.0) * (share - 1.0)
    }
}

/// The stroke a pen lays down at this sensitivity: the nib's width times `sensitivity_width`, with the gain.
pub fn stroke(nib: Nib, sensitivity: u8) -> notecraft_ink::Stroke {
    notecraft_ink::Stroke::with_gain(nib.tool, nib.colour, nib.width() * sensitivity_width(sensitivity), gain(sensitivity))
}

/// The nib to paint with for a stroke at this pressure: the width the app draws at that pressure.
pub fn drawn_nib(stroke: &notecraft_ink::Stroke, pressure: f32) -> Nib {
    let width = notecraft_ink::shape::width_at(stroke, pressure);
    Nib::new(stroke.tool, stroke.colour, (width * 10.0).round() as u16)
}

/// Paints a line from `from` to `to` with `nib`, in picture pixels. Pressure is full throughout,
/// since a mouse has none. The tool's opacity sets how much of each pixel the nib covers.
pub fn line(image: &mut RgbaImage, from: [f32; 2], to: [f32; 2], nib: Nib) {
    let steps = (to[0] - from[0]).abs().max((to[1] - from[1]).abs()).ceil().max(1.0) as i32;
    for s in 0..=steps {
        let t = s as f32 / steps as f32;
        let x = from[0] + (to[0] - from[0]) * t;
        let y = from[1] + (to[1] - from[1]) * t;
        disc(image, x, y, nib);
    }
}

/// Rubs out along a line from `from` to `to`, back to white, which is the blank sheet's colour. The
/// eraser is a round nib of `radius` picture pixels.
pub fn erase_line(image: &mut RgbaImage, from: [f32; 2], to: [f32; 2], radius: f32) {
    // A nib's width is in tenths of a point, so a radius of r pixels is a width of 20r tenths.
    let tenths = (radius * 20.0).round().clamp(f32::from(rack::MIN_TENTHS), f32::from(rack::MAX_TENTHS)) as u16;
    line(image, from, to, Nib::new(Tool::Pen, notecraft_ink::Rgb(255, 255, 255), tenths));
}

/// Paints one round dab of the nib's width, blending its colour over what is there.
fn disc(image: &mut RgbaImage, x: f32, y: f32, nib: Nib) {
    let radius = (nib.width() / 2.0).max(0.5);
    let alpha = nib.tool.opacity();
    let [r, g, b] = [nib.colour.0, nib.colour.1, nib.colour.2];
    let (w, h) = (image.width() as i32, image.height() as i32);
    let reach = radius.ceil() as i32;
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            let (px, py) = (x as i32 + dx, y as i32 + dy);
            let inside = ((px as f32 + 0.5) - x).powi(2) + ((py as f32 + 0.5) - y).powi(2) <= radius * radius;
            if !inside || px < 0 || py < 0 || px >= w || py >= h {
                continue;
            }
            let dst = image.get_pixel_mut(px as u32, py as u32);
            for (channel, value) in dst.0.iter_mut().take(3).zip([r, g, b]) {
                *channel = (f32::from(value) * alpha + f32::from(*channel) * (1.0 - alpha)).round() as u8;
            }
            dst.0[3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_ink::Rgb;

    #[test]
    fn a_pen_paints_its_colour_and_a_highlighter_blends_over_what_is_there() {
        let mut image = RgbaImage::from_pixel(20, 20, image::Rgba([255, 255, 255, 255]));
        line(&mut image, [5.0, 10.0], [15.0, 10.0], Nib::new(Tool::Pen, Rgb(0, 0, 0), 30));
        assert_eq!(image.get_pixel(10, 10).0, [0, 0, 0, 255], "a pen is solid");
        line(&mut image, [5.0, 3.0], [15.0, 3.0], Nib::new(Tool::Highlighter, Rgb(255, 0, 0), 140));
        let tinted = image.get_pixel(10, 3).0;
        assert!(tinted[0] > 0 && tinted[1] > 0 && tinted[1] < 255, "a highlighter lets the white show through: {tinted:?}");
    }

    #[test]
    fn a_stroke_comes_back_from_the_svg_the_lab_writes() {
        let mut drawing = notecraft_ink::Drawing::new(100.0, 100.0);
        let mut stroke = notecraft_ink::Stroke::new(Tool::Pen, Rgb(0, 0, 0), 3.0);
        stroke.add(notecraft_ink::Dab::new(10.0, 10.0, 1.0));
        stroke.add(notecraft_ink::Dab::new(60.0, 40.0, 1.0));
        assert!(drawing.add(stroke), "a stroke with dabs is kept");
        let text = notecraft_ink::svg::write(&drawing, notecraft_ink::svg::Flavour::Editable);
        let (back, _) = notecraft_ink::svg::read(&text).expect("the SVG reads back");
        assert!(!back.is_empty(), "the stroke is there after the round trip");
    }

    #[test]
    fn sensitivity_widens_the_nib_to_four_times_at_the_top_and_is_neutral_at_fifty() {
        assert_eq!(sensitivity_width(0), 1.0);
        assert_eq!(sensitivity_width(50), 1.0);
        assert_eq!(sensitivity_width(100), 4.0);
        let nib = Nib::new(Tool::Pen, Rgb(0, 0, 0), 30);
        assert_eq!(stroke(nib, 100).width, 12.0, "3.0 points becomes 12.0 points");
        assert_eq!(gain(50), 1.0, "neutral sensitivity leaves the pen as the pen gave it");
    }

    /// Counts the picture's rows that are inked along a vertical line through its middle.
    fn inked_rows(image: &RgbaImage) -> usize {
        let x = image.width() / 2;
        (0..image.height()).filter(|&y| image.get_pixel(x, y).0[0] < 128).count()
    }

    // P0 checks, run only when asked: `cargo test -p notecraft-ocr-lab -- --ignored`. They keep the
    // ribbon's sensitivity and pressure behaviour from regressing.

    #[test]
    #[ignore = "P0 check, run with: cargo test -p notecraft-ocr-lab -- --ignored"]
    fn p0_sensitivity_thickens_the_line_on_the_picture() {
        let nib = Nib::new(Tool::Pen, Rgb(0, 0, 0), 30);
        let mut neutral = RgbaImage::from_pixel(60, 40, image::Rgba([255, 255, 255, 255]));
        let mut maximum = neutral.clone();
        line(&mut neutral, [5.0, 20.0], [55.0, 20.0], drawn_nib(&stroke(nib, 50), 1.0));
        line(&mut maximum, [5.0, 20.0], [55.0, 20.0], drawn_nib(&stroke(nib, 100), 1.0));
        let (thin, thick) = (inked_rows(&neutral), inked_rows(&maximum));
        assert!(thick >= 3 * thin, "at sensitivity 100 a 3 pt pen must be at least 3x as thick: {thin} rows at 50, {thick} at 100");
    }

    #[test]
    #[ignore = "P0 check, run with: cargo test -p notecraft-ocr-lab -- --ignored"]
    fn p0_a_light_press_writes_thinner_than_a_full_press() {
        let nib = Nib::new(Tool::Pen, Rgb(0, 0, 0), 60);
        let s = stroke(nib, 50);
        let mut light = RgbaImage::from_pixel(60, 40, image::Rgba([255, 255, 255, 255]));
        let mut full = light.clone();
        line(&mut light, [5.0, 20.0], [55.0, 20.0], drawn_nib(&s, 0.2));
        line(&mut full, [5.0, 20.0], [55.0, 20.0], drawn_nib(&s, 1.0));
        assert!(inked_rows(&light) < inked_rows(&full), "a light press must draw thinner");
    }

    #[test]
    #[ignore = "P0 check, run with: cargo test -p notecraft-ocr-lab -- --ignored"]
    fn p0_the_gain_is_neutral_at_fifty_and_turned_up_at_the_top() {
        assert_eq!(gain(50), 1.0);
        assert!(gain(100) > 1.0, "the top of the control turns the pen up");
        assert!(gain(0) < 1.0, "the bottom turns it down");
    }

    #[test]
    fn the_pen_in_hand_comes_from_the_rack() {
        let mut pen = Pen::new();
        assert_eq!(pen.nib(), rack::fresh()[0]);
        pen.selected = 99;
        assert_eq!(pen.nib().tool, Tool::Pen, "a selection past the rack falls back to a pen");
    }
}

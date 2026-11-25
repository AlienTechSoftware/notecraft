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
    fn the_pen_in_hand_comes_from_the_rack() {
        let mut pen = Pen::new();
        assert_eq!(pen.nib(), rack::fresh()[0]);
        pen.selected = 99;
        assert_eq!(pen.nib().tool, Tool::Pen, "a selection past the rack falls back to a pen");
    }
}

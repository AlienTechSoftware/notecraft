//! Paints what egui tessellated into an image, on the CPU (issue #168).
//!
//! Printing draws the note with the window's own drawing code into a context of its own, so the
//! sheets show what the page shows: words, tables, pictures, boxes and ink. That context has no
//! GPU behind it: these few functions put its triangles into pixels the way egui's own renderers
//! do, with premultiplied colours blended in gamma space and textures sampled bilinearly.

use egui::epaint::{ClippedPrimitive, Primitive, TextureId};
use egui::{Color32, ColorImage, ImageData};
use std::collections::HashMap;

/// The textures the painted frames asked for, kept up to date from each frame's changes.
#[derive(Default)]
pub struct Textures(HashMap<TextureId, ColorImage>);

impl Textures {
    /// Applies a frame's new and changed textures. A patch is written into the texture it
    /// belongs to; a texture never seen whole is skipped until it is.
    pub fn apply(&mut self, changes: &egui::TexturesDelta) {
        for (id, delta) in changes.set.iter().flat_map(|(id, deltas)| deltas.iter().map(move |d| (id, d))) {
            let ImageData::Color(image) = &delta.image;
            match delta.pos {
                None => {
                    self.0.insert(*id, (**image).clone());
                }
                Some([x, y]) => {
                    let Some(whole) = self.0.get_mut(id) else { continue };
                    let [w, h] = image.size;
                    for row in 0..h {
                        for col in 0..w {
                            if x + col < whole.size[0] && y + row < whole.size[1] {
                                whole.pixels[(y + row) * whole.size[0] + x + col] = image.pixels[row * w + col];
                            }
                        }
                    }
                }
            }
        }
    }
}

/// An image being painted: premultiplied RGBA, one `[f32; 4]` per pixel in `0.0..=1.0`.
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    /// A canvas of opaque white, the colour of paper.
    pub fn white(width: usize, height: usize) -> Canvas {
        Canvas { width, height, pixels: vec![[1.0; 4]; width * height] }
    }

    /// Paints `primitives`, whose positions are in points, at `pixels_per_point`.
    pub fn paint(&mut self, primitives: &[ClippedPrimitive], textures: &Textures, pixels_per_point: f32) {
        for primitive in primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else { continue };
            let texture = textures.0.get(&mesh.texture_id);
            let clip = primitive.clip_rect;
            let clip = [
                (clip.min.x * pixels_per_point).floor().max(0.0) as i64,
                (clip.min.y * pixels_per_point).floor().max(0.0) as i64,
                ((clip.max.x * pixels_per_point).ceil() as i64).min(self.width as i64),
                ((clip.max.y * pixels_per_point).ceil() as i64).min(self.height as i64),
            ];
            if clip[0] >= clip[2] || clip[1] >= clip[3] {
                continue;
            }
            for [i0, i1, i2] in mesh.indices.as_chunks::<3>().0 {
                let corner = |i: u32| mesh.vertices.get(i as usize);
                let (Some(a), Some(b), Some(c)) = (corner(*i0), corner(*i1), corner(*i2)) else { continue };
                self.triangle([a, b, c], texture, pixels_per_point, clip);
            }
        }
    }

    fn triangle(&mut self, v: [&egui::epaint::Vertex; 3], texture: Option<&ColorImage>, ppp: f32, clip: [i64; 4]) {
        let mut v = v;
        let mut p = v.map(|v| (v.pos.x * ppp, v.pos.y * ppp));
        let edge = |a: (f32, f32), b: (f32, f32), c: (f32, f32)| (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
        let mut area = edge(p[0], p[1], p[2]);
        if area.abs() < 1e-6 || !area.is_finite() {
            return;
        }
        // One winding for every triangle, so the tie-break below means the same in each.
        if area < 0.0 {
            p.swap(1, 2);
            v.swap(1, 2);
            area = -area;
        }
        let x0 = (p[0].0.min(p[1].0).min(p[2].0).floor() as i64).max(clip[0]);
        let x1 = (p[0].0.max(p[1].0).max(p[2].0).ceil() as i64).min(clip[2]);
        let y0 = (p[0].1.min(p[1].1).min(p[2].1).floor() as i64).max(clip[1]);
        let y1 = (p[0].1.max(p[1].1).max(p[2].1).ceil() as i64).min(clip[3]);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        // A pixel centre exactly on an edge belongs to one of the two triangles that share the
        // edge, never both: the edge runs opposite ways in the two, and only one way owns it.
        let owns = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            dy > 0.0 || (dy == 0.0 && dx < 0.0)
        };
        let owned = [owns(p[1], p[2]), owns(p[2], p[0]), owns(p[0], p[1])];
        let colours = v.map(|v| rgba(v.color));
        // A mesh of one colour with no texture to speak of, as most fills and strokes are, is
        // painted without sampling.
        let plain = texture.is_none_or(|t| t.size == [1, 1]) || v.iter().all(|v| v.uv == egui::epaint::WHITE_UV);
        for y in y0..y1 {
            let py = y as f32 + 0.5;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let e = [edge(p[1], p[2], (px, py)), edge(p[2], p[0], (px, py)), edge(p[0], p[1], (px, py))];
                if (0..3).any(|k| e[k] < 0.0 || (e[k] == 0.0 && !owned[k])) {
                    continue;
                }
                let (w0, w1, w2) = (e[0] / area, e[1] / area, e[2] / area);
                let mut colour = [0.0f32; 4];
                for (k, c) in colour.iter_mut().enumerate() {
                    *c = w0 * colours[0][k] + w1 * colours[1][k] + w2 * colours[2][k];
                }
                if !plain && let Some(texture) = texture {
                    let u = w0 * v[0].uv.x + w1 * v[1].uv.x + w2 * v[2].uv.x;
                    let t = w0 * v[0].uv.y + w1 * v[1].uv.y + w2 * v[2].uv.y;
                    let texel = sample(texture, u, t);
                    for k in 0..4 {
                        colour[k] *= texel[k];
                    }
                }
                let alpha = colour[3];
                if alpha <= 0.0 {
                    continue;
                }
                let at = y as usize * self.width + x as usize;
                let dst = &mut self.pixels[at];
                for k in 0..4 {
                    dst[k] = (colour[k] + dst[k] * (1.0 - alpha)).clamp(0.0, 1.0);
                }
            }
        }
    }

    /// The canvas as opaque RGB bytes, over white.
    pub fn into_rgb(self) -> RgbImage {
        let mut bytes = Vec::with_capacity(self.width * self.height * 3);
        for [r, g, b, a] in self.pixels {
            for c in [r, g, b] {
                bytes.push(((c + (1.0 - a)).clamp(0.0, 1.0) * 255.0).round() as u8);
            }
        }
        RgbImage { width: self.width, height: self.height, bytes }
    }
}

/// An opaque picture, three bytes a pixel, rows top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub bytes: Vec<u8>,
}

impl RgbImage {
    pub fn white(width: usize, height: usize) -> RgbImage {
        RgbImage { width, height, bytes: vec![255; width * height * 3] }
    }

    /// Whether row `y` is the colour of the paper all the way across: a place to start a new sheet
    /// that cuts through nothing.
    pub fn row_is_blank(&self, y: usize) -> bool {
        let start = y * self.width * 3;
        self.bytes.get(start..start + self.width * 3).is_some_and(|row| row.iter().all(|b| *b >= 250))
    }

    /// Copies rows `rows` of `from` onto this image with their top left corner at `at`.
    pub fn put(&mut self, from: &RgbImage, rows: std::ops::Range<usize>, at: (usize, usize)) {
        for (i, y) in rows.enumerate() {
            let ty = at.1 + i;
            if ty >= self.height || y >= from.height {
                break;
            }
            let width = from.width.min(self.width.saturating_sub(at.0));
            let src = &from.bytes[y * from.width * 3..(y * from.width + width) * 3];
            let dst_start = (ty * self.width + at.0) * 3;
            self.bytes[dst_start..dst_start + width * 3].copy_from_slice(src);
        }
    }

    /// A smaller copy, `factor` times narrower and shorter, each pixel the average of the ones it
    /// covers: for the preview.
    pub fn shrunk(&self, factor: usize) -> RgbImage {
        let factor = factor.max(1);
        let (w, h) = (self.width.div_ceil(factor), self.height.div_ceil(factor));
        let mut out = RgbImage::white(w, h);
        for y in 0..h {
            for x in 0..w {
                let mut sum = [0u32; 3];
                let mut n = 0u32;
                for sy in y * factor..((y + 1) * factor).min(self.height) {
                    for sx in x * factor..((x + 1) * factor).min(self.width) {
                        let i = (sy * self.width + sx) * 3;
                        for (k, total) in sum.iter_mut().enumerate() {
                            *total += u32::from(self.bytes[i + k]);
                        }
                        n += 1;
                    }
                }
                let i = (y * w + x) * 3;
                for (k, total) in sum.iter().enumerate() {
                    out.bytes[i + k] = (total / n.max(1)) as u8;
                }
            }
        }
        out
    }
}

fn rgba(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_array();
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]
}

/// The texture's colour at `(u, v)`, between its four nearest texels.
fn sample(texture: &ColorImage, u: f32, v: f32) -> [f32; 4] {
    let [w, h] = texture.size;
    if w == 0 || h == 0 {
        return [1.0; 4];
    }
    let x = (u * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| rgba(texture.pixels[y * w + x]);
    let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    let mut out = [0.0; 4];
    for k in 0..4 {
        out[k] = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn painted(shapes: Vec<egui::Shape>, size: (usize, usize)) -> RgbImage {
        let ctx = egui::Context::default();
        let mut tex = Textures::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0 as f32, size.1 as f32))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            for shape in shapes.clone() {
                ui.painter().add(shape);
            }
        });
        tex.apply(&out.textures_delta);
        out.textures_delta.clear();
        let prims = ctx.tessellate(out.shapes, 1.0);
        let mut canvas = Canvas::white(size.0, size.1);
        canvas.paint(&prims, &tex, 1.0);
        canvas.into_rgb()
    }

    #[test]
    fn a_filled_rectangle_is_painted_where_it_is_and_nowhere_else() {
        let rect = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(30.0, 20.0));
        let image = painted(vec![egui::Shape::rect_filled(rect, 0.0, Color32::from_rgb(200, 0, 0))], (40, 30));
        let at = |x: usize, y: usize| &image.bytes[(y * image.width + x) * 3..(y * image.width + x) * 3 + 3];
        assert_eq!(at(20, 15), &[200, 0, 0]);
        assert_eq!(at(5, 5), &[255, 255, 255]);
        assert_eq!(at(35, 25), &[255, 255, 255]);
        assert!(image.row_is_blank(2) && !image.row_is_blank(15));
    }

    #[test]
    fn a_half_clear_colour_is_blended_with_the_paper() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(8.0, 8.0));
        let image = painted(vec![egui::Shape::rect_filled(rect, 0.0, Color32::from_black_alpha(128))], (8, 8));
        let grey = image.bytes[(4 * 8 + 4) * 3];
        assert!((120..=135).contains(&grey), "half black over white is mid grey: {grey}");
    }

    #[test]
    fn text_puts_ink_on_the_paper() {
        let ctx = egui::Context::default();
        let mut tex = Textures::default();
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(200.0, 40.0))), ..Default::default() };
        let mut out = ctx.run_ui(input, |ui| {
            ui.painter().text(egui::pos2(4.0, 4.0), egui::Align2::LEFT_TOP, "Printed", egui::FontId::proportional(20.0), Color32::BLACK);
        });
        tex.apply(&out.textures_delta);
        out.textures_delta.clear();
        let prims = ctx.tessellate(out.shapes, 1.0);
        let mut canvas = Canvas::white(200, 40);
        canvas.paint(&prims, &tex, 1.0);
        let image = canvas.into_rgb();
        let dark = image.bytes.chunks(3).filter(|p| p[0] < 100).count();
        assert!(dark > 50, "the letters are drawn: {dark} dark pixels");
    }

    #[test]
    fn shrinking_averages_and_putting_places_rows() {
        let mut big = RgbImage::white(4, 4);
        big.bytes[..6].copy_from_slice(&[0, 0, 0, 0, 0, 0]);
        let small = big.shrunk(2);
        assert_eq!((small.width, small.height), (2, 2));
        assert_eq!(small.bytes[0], 127);
        let mut sheet = RgbImage::white(6, 6);
        sheet.put(&big, 0..1, (1, 2));
        assert_eq!(&sheet.bytes[(2 * 6 + 1) * 3..(2 * 6 + 1) * 3 + 3], &[0, 0, 0]);
        assert!(sheet.row_is_blank(0));
    }
}

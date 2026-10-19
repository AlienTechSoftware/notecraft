//! Draws a formula from `notecraft-math` with egui.
//!
//! The maths crate lays a formula out as plain shapes: glyphs of the KaTeX faces, runs of
//! text in the window's own font, lines, rectangles and paths. This module paints them. The
//! KaTeX faces are registered with egui under the name `katex-<face>` (see `fonts.rs`), so a
//! glyph is drawn by naming its face. A formula is laid out once and kept until its text or
//! its size changes, so a page of equations costs nothing per frame.

use std::collections::HashMap;
use std::sync::Arc;

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Galley, Mesh, Painter, Pos2, Response, Sense, Stroke, Ui, vec2};
use notecraft_math::{Cmd, Metrics, Shape, Typeset, typeset};

struct EguiMetrics<'a> {
    painter: &'a Painter,
}

impl Metrics for EguiMetrics<'_> {
    fn width(&self, text: &str, size: f32, _italic: bool) -> f32 {
        self.painter.layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE).size().x
    }
}

/// How many laid out formulas are kept. Past this the cache starts again, which is cheap: a
/// formula costs a fraction of a millisecond.
const CACHE_LIMIT: usize = 256;

type Key = (String, u32, bool);
type Cache = HashMap<Key, Arc<Typeset>>;

/// Lays out `tex` at `size` points with the UI's fonts, as a display equation or in a line of text.
pub fn measure(ui: &Ui, tex: &str, size: f32, display: bool) -> Arc<Typeset> {
    let key: Key = (tex.to_owned(), size.to_bits(), display);
    let id = egui::Id::new("notecraft-math-cache");
    if let Some(hit) = ui.ctx().data_mut(|d| d.get_temp_mut_or_default::<Cache>(id).get(&key).cloned()) {
        return hit;
    }
    let laid = Arc::new(typeset(tex, size, display, &EguiMetrics { painter: ui.painter() }));
    ui.ctx().data_mut(|d| {
        let cache = d.get_temp_mut_or_default::<Cache>(id);
        if cache.len() >= CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(key, laid.clone());
    });
    laid
}

fn colour(own: Option<[u8; 4]>, default: Color32) -> Color32 {
    own.map_or(default, |[r, g, b, a]| Color32::from_rgba_unmultiplied(r, g, b, a))
}

/// The distance from the top of a one line galley to its baseline.
fn baseline(galley: &Galley, size: f32) -> f32 {
    galley.rows.first().map_or(0.8 * size, |row| row.pos.y + row.glyphs.first().map_or(0.8 * size, |g| g.pos.y))
}

fn paint(painter: &Painter, origin: Pos2, t: &Typeset, default: Color32) {
    // The KaTeX faces are registered when the window installs its fonts, and egui takes them up
    // at the start of the next frame, not the one that asked. Asking egui for a family it does
    // not have yet is a panic, so it is asked what it has now; until the faces are there a glyph
    // is drawn in the ordinary font: wrong shapes for a frame, but a formula and not a crash.
    let katex = painter.fonts(|f| f.definitions().families.contains_key(&FontFamily::Name("katex-Main-Regular".into())));
    for shape in &t.shapes {
        match shape {
            Shape::Glyph { x, y, face, ch, size, color } => {
                let family = if katex { FontFamily::Name(format!("katex-{face}").into()) } else { FontFamily::Proportional };
                let c = colour(*color, default);
                let galley = painter.layout_no_wrap(ch.to_string(), FontId::new(*size, family), c);
                let top = origin.y + y - baseline(&galley, *size);
                painter.galley(Pos2::new(origin.x + x, top), galley, c);
            }
            Shape::Text { x, y, text, size, italic, color } => {
                let c = colour(*color, default);
                let mut job = LayoutJob::default();
                job.append(text, 0.0, TextFormat { font_id: FontId::proportional(*size), color: c, italics: *italic, ..TextFormat::default() });
                let galley = painter.layout_job(job);
                let top = origin.y + y - baseline(&galley, *size);
                painter.galley(Pos2::new(origin.x + x, top), galley, c);
            }
            Shape::Line { from, to, thickness, dashed, color } => {
                let stroke = Stroke::new(*thickness, colour(*color, default));
                let (a, b) = (Pos2::new(origin.x + from.0, origin.y + from.1), Pos2::new(origin.x + to.0, origin.y + to.1));
                if *dashed {
                    painter.extend(egui::Shape::dashed_line(&[a, b], stroke, 3.0 * thickness.max(1.0), 2.0 * thickness.max(1.0)));
                } else {
                    painter.line_segment([a, b], stroke);
                }
            }
            Shape::Rect { x, y, width, height, color } => {
                let min = Pos2::new(origin.x + x, origin.y + y);
                painter.rect_filled(egui::Rect::from_min_size(min, vec2(*width, *height)), 0.0, colour(*color, default));
            }
            Shape::Path { x, y, cmds, fill, stroke, color } => {
                let c = colour(*color, default);
                for points in subpaths(Pos2::new(origin.x + x, origin.y + y), cmds) {
                    if *fill {
                        fill_polygon(painter, &points, c);
                    } else if points.len() > 1 {
                        painter.add(egui::Shape::line(points, Stroke::new(stroke.max(0.5), c)));
                    }
                }
            }
        }
    }
}

/// A path's steps as lists of points, curves cut into short straight pieces.
fn subpaths(origin: Pos2, cmds: &[Cmd]) -> Vec<Vec<Pos2>> {
    const STEPS: usize = 12;
    let at = |x: f32, y: f32| Pos2::new(origin.x + x, origin.y + y);
    let mut out: Vec<Vec<Pos2>> = Vec::new();
    let mut now: Vec<Pos2> = Vec::new();
    for cmd in cmds {
        match *cmd {
            Cmd::Move(x, y) => {
                if now.len() > 1 {
                    out.push(std::mem::take(&mut now));
                } else {
                    now.clear();
                }
                now.push(at(x, y));
            }
            Cmd::Line(x, y) => now.push(at(x, y)),
            Cmd::Quad(x1, y1, x, y) => {
                let Some(&p0) = now.last() else { continue };
                let (p1, p2) = (at(x1, y1), at(x, y));
                for i in 1..=STEPS {
                    let t = i as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    now.push(Pos2::new(u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x, u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y));
                }
            }
            Cmd::Cubic(x1, y1, x2, y2, x, y) => {
                let Some(&p0) = now.last() else { continue };
                let (p1, p2, p3) = (at(x1, y1), at(x2, y2), at(x, y));
                for i in 1..=STEPS {
                    let t = i as f32 / STEPS as f32;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    now.push(Pos2::new(a * p0.x + b * p1.x + c * p2.x + d * p3.x, a * p0.y + b * p1.y + c * p2.y + d * p3.y));
                }
            }
            Cmd::Close => {
                if let Some(&first) = now.first() {
                    now.push(first);
                }
            }
        }
    }
    if now.len() > 1 {
        out.push(now);
    }
    out
}

/// Fills a polygon that need not be convex, by cutting off one ear at a time. A radical or a
/// tall brace is not convex, and egui fills only convex shapes. A polygon that cannot be cut
/// (it crosses itself) is filled as it stands.
fn fill_polygon(painter: &Painter, points: &[Pos2], color: Color32) {
    // Without points that repeat the one before them: they add nothing and stop an ear being found.
    let mut pts: Vec<Pos2> = Vec::with_capacity(points.len());
    for p in points {
        if pts.last().is_none_or(|q| (q.x - p.x).abs() > 1e-3 || (q.y - p.y).abs() > 1e-3) {
            pts.push(*p);
        }
    }
    while pts.len() > 1 && pts.first().zip(pts.last()).is_some_and(|(a, b)| (a.x - b.x).abs() <= 1e-3 && (a.y - b.y).abs() <= 1e-3) {
        pts.pop();
    }
    if pts.len() < 3 {
        return;
    }
    let area: f32 = (0..pts.len()).map(|i| (pts[i].x * pts[(i + 1) % pts.len()].y) - (pts[(i + 1) % pts.len()].x * pts[i].y)).sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let cross = |a: Pos2, b: Pos2, c: Pos2| ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)) * sign;
    let inside = |p: Pos2, a: Pos2, b: Pos2, c: Pos2| cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0;
    let mut index: Vec<usize> = (0..pts.len()).collect();
    let mut mesh = Mesh::default();
    for p in &pts {
        mesh.colored_vertex(*p, color);
    }
    let mut guard = index.len() * index.len() + 8;
    while index.len() > 3 && guard > 0 {
        guard -= 1;
        let n = index.len();
        let mut cut = None;
        for i in 0..n {
            let (a, b, c) = (index[(i + n - 1) % n], index[i], index[(i + 1) % n]);
            let turn = cross(pts[a], pts[b], pts[c]);
            if turn.abs() <= 1e-3 {
                // Three points on a line: the middle one is no corner. Drop it, no triangle.
                cut = Some((i, a, b, c, false));
                break;
            }
            if turn < 0.0 {
                continue;
            }
            if index.iter().any(|&o| o != a && o != b && o != c && inside(pts[o], pts[a], pts[b], pts[c])) {
                continue;
            }
            cut = Some((i, a, b, c, true));
            break;
        }
        let Some((i, a, b, c, ear)) = cut else {
            // Not a simple polygon: fill it whole rather than leave a hole.
            painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
            return;
        };
        if ear {
            mesh.add_triangle(a as u32, b as u32, c as u32);
        }
        index.remove(i);
    }
    if let [a, b, c] = index[..] {
        mesh.add_triangle(a as u32, b as u32, c as u32);
    }
    painter.add(egui::Shape::mesh(mesh));
}

/// An inline formula, as wide as it needs to be.
pub fn inline(ui: &mut Ui, tex: &str, size: f32, color: Color32) -> Response {
    let l = measure(ui, tex, size, false);
    let (rect, resp) = ui.allocate_exact_size(vec2(l.width + 2.0, l.height()), Sense::hover());
    paint(ui.painter(), rect.min + vec2(1.0, 0.0), &l, color);
    resp.on_hover_text(tex)
}

/// A display formula, centred in the available width.
pub fn display(ui: &mut Ui, tex: &str, size: f32, color: Color32) -> Response {
    let l = measure(ui, tex, size, true);
    let full = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(full.max(l.width), l.height() + 4.0), Sense::hover());
    let x = rect.min.x + ((rect.width() - l.width) / 2.0).max(0.0);
    paint(ui.painter(), Pos2::new(x, rect.min.y + 2.0), &l, color);
    resp.on_hover_text(tex)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context(with_fonts: bool) -> egui::Context {
        let ctx = egui::Context::default();
        if with_fonts {
            crate::fonts::install(&ctx);
        }
        ctx
    }

    /// Runs one frame that draws `tex` and returns what it laid out.
    fn frame(ctx: &egui::Context, tex: &str, display: bool) -> Arc<Typeset> {
        let mut out = None;
        let mut run = ctx.run_ui(egui::RawInput::default(), |ui| {
            let t = measure(ui, tex, 22.0, display);
            paint(ui.painter(), Pos2::ZERO, &t, Color32::BLACK);
            out = Some(t);
        });
        run.textures_delta.clear();
        out.unwrap_or_default()
    }

    #[test]
    fn a_formula_drawn_before_the_fonts_are_in_is_drawn_and_does_not_panic() {
        let t = frame(&context(false), r"\frac{a}{b} + \sqrt{x}", true);
        assert!(!t.shapes.is_empty());
    }

    #[test]
    fn every_face_an_equation_can_ask_for_is_registered() {
        let fonts = crate::fonts::definitions();
        let ctx = context(true);
        let mut faces = std::collections::BTreeSet::new();
        for tex in [
            r"\frac{a}{b} + \sqrt{x} + \sum_{i=1}^n \int_0^1 \left[ \frac{1}{\frac{1}{x}} \right]",
            r"\begin{pmatrix} a & b \\ c & d \end{pmatrix} \mathbb{R} \mathcal{L} \mathfrak{g} \mathscr{S} \mathbf{x} \mathsf{y} \mathtt{z}",
            r"\hat{x} \vec{v} \underbrace{a}_{b} \binom{n}{k} \left\{ \left( \left[ \frac{\frac{a}{b}}{c} \right] \right) \right\}",
            r"\alpha \beta \Gamma \hbar \nabla \partial \infty \forall \exists \leq \neq \approx \mapsto",
        ] {
            let t = frame(&ctx, tex, true);
            assert_eq!(t.engine, notecraft_math::Engine::Ratex, "{tex}");
            for s in &t.shapes {
                if let Shape::Glyph { face, .. } = s {
                    faces.insert(face.clone());
                }
            }
        }
        assert!(faces.len() >= 5, "the formulas used too few faces to mean much: {faces:?}");
        for face in &faces {
            let family = FontFamily::Name(format!("katex-{face}").into());
            assert!(fonts.families.contains_key(&family), "the face {face} is drawn by equations and is not registered");
        }
    }

    #[test]
    fn every_equation_the_gallery_offers_is_drawn_in_full() {
        let ctx = context(true);
        for (name, tex) in crate::ribbon::EQUATIONS {
            let t = frame(&ctx, tex, false);
            assert_eq!(t.engine, notecraft_math::Engine::Ratex, "{name}: {tex}");
            assert!(t.width > 0.0 && t.height() > 0.0, "{name} has no size");
            assert!(!t.shapes.iter().any(|s| matches!(s, Shape::Text { text, .. } if text.contains('\\'))), "{name} left raw TeX in the picture");
        }
    }

    #[test]
    fn a_formula_is_laid_out_once_and_kept() {
        let ctx = context(true);
        let (a, b) = (frame(&ctx, "x^2 + y^2", true), frame(&ctx, "x^2 + y^2", true));
        assert!(Arc::ptr_eq(&a, &b), "the second frame took the first one's layout");
        let c = frame(&ctx, "x^2 + y^2", false);
        assert!(!Arc::ptr_eq(&a, &c), "inline and display are different layouts");
    }
}

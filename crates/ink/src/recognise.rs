//! Draw > Automatic Shapes (issue #294): a hand-drawn line, rectangle, ellipse or triangle
//! is turned into a clean one when the pen is lifted.
//!
//! Recognition is deliberately cautious. A stroke becomes a shape only when it is close to
//! that shape all the way round; anything in doubt stays exactly as it was drawn, so
//! handwriting is never mistaken for a box. The shape keeps the stroke's tool, colour, width
//! and gain, and sits where the hand drew it. It is ordinary ink after that, so it saves in
//! both page formats like any other stroke, and one Undo brings back what the hand drew.
//!
//! Supported: a straight line, a rectangle with sides along the page, an ellipse or circle
//! with axes along the page, and a triangle of any shape.

use crate::{Dab, Stroke, shapes};

/// What a stroke was recognised as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Recognised {
    Line { from: (f32, f32), to: (f32, f32) },
    Rectangle { left: f32, top: f32, right: f32, bottom: f32 },
    Ellipse { left: f32, top: f32, right: f32, bottom: f32 },
    Triangle([(f32, f32); 3]),
}

impl Recognised {
    /// The name, for scripts and the status line.
    pub fn id(&self) -> &'static str {
        match self {
            Recognised::Line { .. } => "line",
            Recognised::Rectangle { .. } => "rectangle",
            Recognised::Ellipse { .. } => "ellipse",
            Recognised::Triangle(_) => "triangle",
        }
    }
}

/// A stroke must be at least this long, in points, to be read as a shape: a tick or a dot
/// is writing.
const LEAST: f32 = 24.0;

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    if len2 <= f32::EPSILON {
        return dist(p, a);
    }
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / len2).clamp(0.0, 1.0);
    dist(p, (a.0 + vx * t, a.1 + vy * t))
}

/// What the stroke looks like, if it is plainly one of the supported shapes.
pub fn recognise(stroke: &Stroke) -> Option<Recognised> {
    let pts: Vec<(f32, f32)> = stroke.dabs.iter().map(|d| (d.x, d.y)).collect();
    let (first, last) = (*pts.first()?, *pts.last()?);
    if pts.len() < 4 {
        return None;
    }
    let length: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    if length < LEAST {
        return None;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (first.0, first.1, first.0, first.1);
    for &(x, y) in &pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    let diagonal = (x1 - x0).hypot(y1 - y0);

    // A line: every reading close to the straight line from end to end, and the stroke not
    // much longer than that line (a scribble back and forth along one line is not a line).
    let chord = dist(first, last);
    if chord >= LEAST {
        let off = pts.iter().map(|&p| to_segment(p, first, last)).fold(0.0_f32, f32::max);
        if off <= (chord * 0.05).max(2.5) && length <= chord * 1.12 {
            return Some(Recognised::Line { from: first, to: last });
        }
    }

    // Everything else is closed: the pen comes back near where it started.
    if chord > diagonal * 0.2 || x1 - x0 < LEAST / 2.0 || y1 - y0 < LEAST / 2.0 {
        return None;
    }

    // A rectangle: every reading near the box's edge, and the corners actually visited.
    let near_edge = |&(x, y): &(f32, f32)| (x - x0).abs().min((x - x1).abs()).min((y - y0).abs()).min((y - y1).abs());
    let slack = (diagonal * 0.06).max(3.0);
    let edge_off = pts.iter().map(near_edge).fold(0.0_f32, f32::max);
    let corners = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)];
    let visited = corners.iter().all(|&c| pts.iter().any(|&p| dist(p, c) <= diagonal * 0.12));
    if edge_off <= slack && visited {
        return Some(Recognised::Rectangle { left: x0, top: y0, right: x1, bottom: y1 });
    }

    // An ellipse in that box: every reading near the ellipse the box holds, and it goes
    // most of the way round.
    let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
    let radial = |&(x, y): &(f32, f32)| {
        let r = (((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2)).sqrt();
        (r - 1.0).abs()
    };
    let worst = pts.iter().map(radial).fold(0.0_f32, f32::max);
    let turned = {
        let mut total = 0.0_f32;
        let angle = |&(x, y): &(f32, f32)| ((y - cy) / ry).atan2((x - cx) / rx);
        for w in pts.windows(2) {
            let mut d = angle(&w[1]) - angle(&w[0]);
            if d > std::f32::consts::PI {
                d -= std::f32::consts::TAU;
            } else if d < -std::f32::consts::PI {
                d += std::f32::consts::TAU;
            }
            total += d;
        }
        total.abs()
    };
    if worst <= 0.2 && turned >= std::f32::consts::TAU * 0.85 {
        return Some(Recognised::Ellipse { left: x0, top: y0, right: x1, bottom: y1 });
    }

    // A triangle: the closed path comes down to three corners, and every reading is near
    // one of the three sides.
    let corners = simplify_closed(&pts, (diagonal * 0.08).max(3.0));
    if corners.len() == 3 {
        let tri = [corners[0], corners[1], corners[2]];
        let off = pts
            .iter()
            .map(|&p| to_segment(p, tri[0], tri[1]).min(to_segment(p, tri[1], tri[2])).min(to_segment(p, tri[2], tri[0])))
            .fold(0.0_f32, f32::max);
        let shortest = dist(tri[0], tri[1]).min(dist(tri[1], tri[2])).min(dist(tri[2], tri[0]));
        if off <= slack && shortest >= diagonal * 0.2 {
            return Some(Recognised::Triangle(tri));
        }
    }
    None
}

/// The corners of a closed path, by Ramer-Douglas-Peucker from the point furthest from the
/// start: points within `tolerance` of the line between two kept corners are dropped.
fn simplify_closed(pts: &[(f32, f32)], tolerance: f32) -> Vec<(f32, f32)> {
    let Some(&start) = pts.first() else { return Vec::new() };
    let far = pts.iter().enumerate().max_by(|a, b| dist(*a.1, start).total_cmp(&dist(*b.1, start))).map_or(0, |(i, _)| i);
    let mut out = Vec::new();
    rdp(&pts[..=far], tolerance, &mut out);
    out.pop();
    let mut back: Vec<(f32, f32)> = pts[far..].to_vec();
    back.push(start);
    rdp(&back, tolerance, &mut out);
    out.pop();
    // A corner the start happened to sit on a straight side is not a corner.
    let n = out.len();
    if n >= 4 {
        let keep: Vec<(f32, f32)> = (0..n)
            .filter(|&i| {
                let (a, p, b) = (out[(i + n - 1) % n], out[i], out[(i + 1) % n]);
                to_segment(p, a, b) > tolerance
            })
            .map(|i| out[i])
            .collect();
        return keep;
    }
    out
}

fn rdp(pts: &[(f32, f32)], tolerance: f32, out: &mut Vec<(f32, f32)>) {
    let (Some(&a), Some(&b)) = (pts.first(), pts.last()) else { return };
    let mut best = (0, 0.0_f32);
    for (i, &p) in pts.iter().enumerate().take(pts.len().saturating_sub(1)).skip(1) {
        let d = to_segment(p, a, b);
        if d > best.1 {
            best = (i, d);
        }
    }
    if best.1 > tolerance {
        rdp(&pts[..=best.0], tolerance, out);
        out.pop();
        rdp(&pts[best.0..], tolerance, out);
    } else {
        out.push(a);
        out.push(b);
    }
}

/// The stroke redrawn as `shape`: the same tool, colour, width and gain, with an even
/// pressure taken from the hand's average.
pub fn redraw(stroke: &Stroke, shape: Recognised) -> Stroke {
    #[allow(clippy::cast_precision_loss)]
    let pressure = (stroke.dabs.iter().map(|d| d.pressure).sum::<f32>() / stroke.dabs.len().max(1) as f32).clamp(0.1, 1.0);
    let dabs: Vec<Dab> = match shape {
        Recognised::Line { from, to } => shapes::trace(shapes::Shape::Line, from, to, pressure),
        Recognised::Rectangle { left, top, right, bottom } => shapes::trace(shapes::Shape::Rectangle, (left, top), (right, bottom), pressure),
        Recognised::Ellipse { left, top, right, bottom } => shapes::trace(shapes::Shape::Ellipse, (left, top), (right, bottom), pressure),
        Recognised::Triangle(t) => shapes::polygon(&[t[0], t[1], t[2], t[0]], pressure),
    };
    // Straight from the tracer: a shape's corners are repeated on purpose to stay sharp,
    // which `Stroke::add` would drop as readings that say nothing new.
    Stroke { dabs, ..stroke.clone() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rgb, Tool};

    fn hand(points: &[(f32, f32)]) -> Stroke {
        let mut s = Stroke::new(Tool::Pen, Rgb(200, 30, 30), 2.5);
        for (i, &(x, y)) in points.iter().enumerate() {
            // A little tremor, as a hand has.
            let wobble = [0.0, 0.8, -0.6, 0.5, -0.9][i % 5];
            s.dabs.push(Dab::new(x + wobble * 0.5, y + wobble, 0.6));
        }
        s
    }

    fn walk(corners: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        for w in corners.windows(2) {
            let n = (dist(w[0], w[1]) / step).ceil().max(1.0) as usize;
            for k in 0..n {
                let t = k as f32 / n as f32;
                out.push((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
            }
        }
        out.push(*corners.last().unwrap_or(&(0.0, 0.0)));
        out
    }

    #[test]
    fn plain_shapes_are_recognised() {
        assert!(matches!(recognise(&hand(&walk(&[(10.0, 10.0), (200.0, 60.0)], 4.0))), Some(Recognised::Line { .. })));
        let boxed = walk(&[(50.0, 50.0), (250.0, 50.0), (250.0, 150.0), (50.0, 150.0), (52.0, 54.0)], 4.0);
        assert!(matches!(recognise(&hand(&boxed)), Some(Recognised::Rectangle { .. })), "{:?}", recognise(&hand(&boxed)));
        let round: Vec<(f32, f32)> = (0..=72)
            .map(|i| {
                let a = i as f32 / 72.0 * std::f32::consts::TAU * 1.02;
                (150.0 + 90.0 * a.cos(), 100.0 + 50.0 * a.sin())
            })
            .collect();
        assert!(matches!(recognise(&hand(&round)), Some(Recognised::Ellipse { .. })), "{:?}", recognise(&hand(&round)));
        let tri = walk(&[(100.0, 20.0), (180.0, 160.0), (20.0, 150.0), (98.0, 24.0)], 4.0);
        assert!(matches!(recognise(&hand(&tri)), Some(Recognised::Triangle(_))), "{:?}", recognise(&hand(&tri)));
    }

    #[test]
    fn writing_and_unclear_strokes_stay_as_they_were() {
        // A short tick.
        assert_eq!(recognise(&hand(&[(0.0, 0.0), (4.0, 6.0), (12.0, -8.0), (14.0, -9.0)])), None);
        // A wave: neither line nor closed.
        let wave: Vec<(f32, f32)> = (0..80).map(|i| (i as f32 * 3.0, 20.0 * (i as f32 * 0.3).sin())).collect();
        assert_eq!(recognise(&hand(&wave)), None);
        // A loop that does not close.
        let open: Vec<(f32, f32)> = (0..=40)
            .map(|i| {
                let a = i as f32 / 40.0 * std::f32::consts::PI * 1.3;
                (100.0 + 60.0 * a.cos(), 100.0 + 60.0 * a.sin())
            })
            .collect();
        assert_eq!(recognise(&hand(&open)), None);
        // A pentagon is not supported, so it is left alone.
        let five: Vec<(f32, f32)> = (0..=5)
            .map(|i| {
                let a = i as f32 / 5.0 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
                (100.0 + 80.0 * a.cos(), 100.0 + 80.0 * a.sin())
            })
            .collect();
        assert_eq!(recognise(&hand(&walk(&five, 4.0))), None);
        // A back-and-forth scribble along one line is not a line.
        assert_eq!(recognise(&hand(&walk(&[(0.0, 0.0), (100.0, 0.0), (10.0, 0.0), (100.0, 0.0)], 4.0))), None);
    }

    #[test]
    fn a_redrawn_shape_keeps_the_pen_and_the_place() {
        let s = hand(&walk(&[(50.0, 50.0), (250.0, 50.0), (250.0, 150.0), (50.0, 150.0), (52.0, 54.0)], 4.0));
        let shape = recognise(&s).expect("rectangle");
        let r = redraw(&s, shape);
        assert_eq!((r.tool, r.colour, r.width, r.gain), (s.tool, s.colour, s.width, s.gain));
        let (bx0, by0, bx1, by1) = r.bounds().expect("bounds");
        let (sx0, sy0, sx1, sy1) = s.bounds().expect("bounds");
        assert!((bx0 - sx0).abs() < 2.0 && (by0 - sy0).abs() < 2.0 && (bx1 - sx1).abs() < 2.0 && (by1 - sy1).abs() < 2.0);
        assert!(r.dabs.iter().all(|d| (d.pressure - 0.6).abs() < 1e-4), "an even line at the hand's pressure");
    }
}

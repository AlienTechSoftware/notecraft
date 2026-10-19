//! The shape a stroke makes on the page.
//!
//! A pen reports a scatter of points. Joining them with straight lines gives handwriting
//! that looks like a seismograph, so the readings are first smoothed into a curve
//! ([`centre`]), and then the curve is given width: at each point the ribbon is pushed out
//! on both sides by half the width there, which is what the pressure decides. Walking the
//! left side forwards and the right side back gives one closed polygon ([`outline`]) that
//! any painter can fill and that SVG can hold as a path.
//!
//! All of it is plain arithmetic on points, so it is tested without a window and draws the
//! same everywhere.

use crate::{Dab, Stroke};

/// How far apart the points of the smoothed curve are, in points. Smaller is smoother and
/// slower; at a third of a point the joins are invisible at any zoom a page is read at.
const STEP: f32 = 0.34;

/// How many points make the half-circle at each end of a stroke.
const CAP: usize = 6;

/// The smoothed middle of a stroke: the readings run through a Catmull-Rom spline, which
/// passes through every one of them rather than near it, and resampled evenly.
///
/// A stroke of one reading comes back as that reading: a dot is a dot.
pub fn centre(stroke: &Stroke) -> Vec<Dab> {
    let dabs = &stroke.dabs;
    if dabs.len() < 3 {
        return dabs.clone();
    }
    let mut out = Vec::new();
    let at = |i: isize| -> Dab {
        let last = dabs.len() as isize - 1;
        let i = i.clamp(0, last).max(0) as usize;
        dabs.get(i).copied().unwrap_or(Dab { x: 0.0, y: 0.0, pressure: 1.0 })
    };
    for i in 0..dabs.len().saturating_sub(1) {
        let (p0, p1, p2, p3) = (at(i as isize - 1), at(i as isize), at(i as isize + 1), at(i as isize + 2));
        // One point every STEP along this piece, from however long it is end to end.
        let span = ((p2.x - p1.x).powi(2) + (p2.y - p1.y).powi(2)).sqrt();
        let steps = ((span / STEP).ceil() as usize).clamp(1, 400);
        for s in 0..steps {
            #[allow(clippy::cast_precision_loss)]
            let t = s as f32 / steps as f32;
            out.push(spline(p0, p1, p2, p3, t));
        }
    }
    if let Some(last) = dabs.last() {
        out.push(*last);
    }
    out
}

/// Catmull-Rom between `p1` and `p2`, with `p0` and `p3` giving the curve its lean.
/// Pressure rides along the same curve, so it swells and fades as smoothly as the line.
fn spline(p0: Dab, p1: Dab, p2: Dab, p3: Dab, t: f32) -> Dab {
    let (t2, t3) = (t * t, t * t * t);
    let f = |a: f32, b: f32, c: f32, d: f32| {
        0.5 * ((2.0 * b) + (-a + c) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (-a + 3.0 * b - 3.0 * c + d) * t3)
    };
    Dab {
        x: f(p0.x, p1.x, p2.x, p3.x),
        y: f(p0.y, p1.y, p2.y, p3.y),
        pressure: f(p0.pressure, p1.pressure, p2.pressure, p3.pressure).clamp(0.0, 1.0),
    }
}

/// How wide the stroke is where the pen was pressed this hard. Every tool lays down
/// something however lightly it is held, which is its floor; a highlighter's floor is its
/// whole width, because a chisel tip does not thin.
pub fn width_at(stroke: &Stroke, pressure: f32) -> f32 {
    let floor = stroke.tool.floor();
    let share = floor + (1.0 - floor) * pressure.clamp(0.0, 1.0);
    (stroke.width * share).max(crate::MIN_WIDTH)
}

/// The two edges of the ribbon, left and right of the smoothed middle, point for point
/// with it. A painter that fills triangles wants these; SVG wants them joined up, which is
/// what [`outline`] does. Both come from here, so the file and the screen agree.
pub fn sides(stroke: &Stroke) -> (Vec<(f32, f32)>, Vec<(f32, f32)>) {
    let line = centre(stroke);
    let mut left: Vec<(f32, f32)> = Vec::with_capacity(line.len());
    let mut right: Vec<(f32, f32)> = Vec::with_capacity(line.len());
    for (i, d) in line.iter().enumerate() {
        // The direction here is the way the line is going, taken from the points either
        // side so a corner leans the right way.
        let before = line.get(i.saturating_sub(1)).copied().unwrap_or(*d);
        let after = line.get(i + 1).copied().unwrap_or(*d);
        let (dx, dy) = (after.x - before.x, after.y - before.y);
        let len = (dx * dx + dy * dy).sqrt();
        // Two readings in the same place say nothing about direction; the ends are round
        // anyway, so across the page is as good a guess as any.
        let (nx, ny) = if len > f32::EPSILON { (-dy / len, dx / len) } else { (0.0, 1.0) };
        let half = width_at(stroke, d.pressure) / 2.0;
        left.push((d.x + nx * half, d.y + ny * half));
        right.push((d.x - nx * half, d.y - ny * half));
    }
    (left, right)
}

/// The closed polygon to fill for one stroke: down one side of the ribbon, round the end,
/// back up the other, round the start. Empty for a stroke with no readings.
pub fn outline(stroke: &Stroke) -> Vec<(f32, f32)> {
    let line = centre(stroke);
    let Some(first) = line.first().copied() else { return Vec::new() };
    if line.len() == 1 {
        return circle(first.x, first.y, width_at(stroke, first.pressure) / 2.0);
    }
    let (left, right) = sides(stroke);
    let last = line.last().copied().unwrap_or(first);
    let mut out = left;
    out.extend(half_circle(last, width_at(stroke, last.pressure) / 2.0, line.get(line.len().saturating_sub(2)).copied().unwrap_or(first)));
    out.extend(right.into_iter().rev());
    out.extend(half_circle(first, width_at(stroke, first.pressure) / 2.0, line.get(1).copied().unwrap_or(last)));
    out
}

/// A ring of points around a dot, for a stroke that never moved.
fn circle(x: f32, y: f32, r: f32) -> Vec<(f32, f32)> {
    let n = CAP * 2;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let a = std::f32::consts::TAU * i as f32 / n as f32;
            (x + r * a.cos(), y + r * a.sin())
        })
        .collect()
}

/// The round end of a stroke: a half turn around `at`, starting on the side the ribbon
/// arrived on. `from` is the point before it, which says which way the pen was going.
fn half_circle(at: Dab, r: f32, from: Dab) -> Vec<(f32, f32)> {
    let (dx, dy) = (at.x - from.x, at.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > f32::EPSILON { (dx / len, dy / len) } else { (1.0, 0.0) };
    // Start at the left edge (the normal) and swing half a turn to the right edge.
    let start = (-uy).atan2(ux);
    (0..=CAP)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let a = start - std::f32::consts::PI * i as f32 / CAP as f32;
            (at.x + r * a.cos(), at.y + r * a.sin())
        })
        .collect()
}

/// Whether a circle of `radius` at (`x`, `y`) touches the stroke: the eraser's question,
/// and the one a click on a drawing asks. Measured against the readings as drawn, with the
/// width of the line taken in, so a fat stroke is easier to catch than a hair.
pub fn touches(stroke: &Stroke, x: f32, y: f32, radius: f32) -> bool {
    let reach = |d: &Dab| radius + width_at(stroke, d.pressure) / 2.0;
    let mut dabs = stroke.dabs.iter();
    let Some(mut prev) = dabs.next() else { return false };
    if (prev.x - x).hypot(prev.y - y) <= reach(prev) {
        return true;
    }
    for d in dabs {
        if distance_to_segment(x, y, prev, d) <= reach(d) {
            return true;
        }
        prev = d;
    }
    false
}

/// How far (`x`, `y`) is from the line between two readings.
fn distance_to_segment(x: f32, y: f32, a: &Dab, b: &Dab) -> f32 {
    let (vx, vy) = (b.x - a.x, b.y - a.y);
    let len2 = vx * vx + vy * vy;
    if len2 <= f32::EPSILON {
        return (a.x - x).hypot(a.y - y);
    }
    let t = (((x - a.x) * vx + (y - a.y) * vy) / len2).clamp(0.0, 1.0);
    (a.x + vx * t - x).hypot(a.y + vy * t - y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dab, Rgb, Stroke, Tool};

    fn drawn(tool: Tool, points: &[(f32, f32, f32)]) -> Stroke {
        let mut s = Stroke::new(tool, Rgb::BLACK, 6.0);
        for (x, y, p) in points {
            s.dabs.push(Dab::new(*x, *y, *p));
        }
        s
    }

    #[test]
    fn a_smoothed_line_passes_through_every_reading() {
        let s = drawn(Tool::Pen, &[(0.0, 0.0, 1.0), (20.0, 14.0, 1.0), (40.0, 0.0, 1.0), (60.0, 14.0, 1.0)]);
        let line = centre(&s);
        assert!(line.len() > s.dabs.len() * 4, "it was resampled finely: {} points", line.len());
        for d in &s.dabs {
            let nearest = line.iter().map(|c| (c.x - d.x).hypot(c.y - d.y)).fold(f32::MAX, f32::min);
            assert!(nearest < 0.5, "the curve goes through ({}, {}); nearest point was {nearest}", d.x, d.y);
        }
        // And it never wanders far from the readings it was made from.
        for c in &line {
            let nearest = s.dabs.iter().map(|d| (c.x - d.x).hypot(c.y - d.y)).fold(f32::MAX, f32::min);
            assert!(nearest < 20.0, "the curve stays near the ink: {nearest}");
        }
    }

    #[test]
    fn a_dot_is_a_dot_and_two_readings_are_a_line() {
        let dot = drawn(Tool::Pen, &[(5.0, 5.0, 1.0)]);
        assert_eq!(centre(&dot).len(), 1);
        let ring = outline(&dot);
        assert_eq!(ring.len(), CAP * 2, "a dot is drawn as a ring");
        for (x, y) in &ring {
            let r = (x - 5.0).hypot(y - 5.0);
            assert!((r - 3.0).abs() < 0.01, "every point of the ring is half a width out: {r}");
        }
        assert_eq!(centre(&drawn(Tool::Pen, &[(0.0, 0.0, 1.0), (9.0, 0.0, 1.0)])).len(), 2);
    }

    #[test]
    fn pressing_harder_makes_a_wider_stroke() {
        let pen = drawn(Tool::Pen, &[(0.0, 0.0, 0.1)]);
        assert!(width_at(&pen, 1.0) > width_at(&pen, 0.1), "the pen swells");
        assert!(width_at(&pen, 0.0) > 0.0, "and still marks the page at nothing");
        assert_eq!(width_at(&pen, 1.0), 6.0, "at full pressure it is the width it was given");

        let marker = drawn(Tool::Highlighter, &[(0.0, 0.0, 0.1)]);
        assert_eq!(width_at(&marker, 0.1), width_at(&marker, 1.0), "a highlighter does not thin");

        let pencil = drawn(Tool::Pencil, &[(0.0, 0.0, 0.1)]);
        assert!(width_at(&pencil, 0.2) > width_at(&pen, 0.2), "a pencil varies less than a pen");
    }

    #[test]
    fn an_outline_is_a_closed_ribbon_as_wide_as_the_pressure() {
        // A straight line drawn lightly at one end and hard at the other.
        let s = drawn(Tool::Pen, &[(0.0, 50.0, 0.1), (40.0, 50.0, 0.5), (80.0, 50.0, 1.0)]);
        let ring = outline(&s);
        assert!(ring.len() > 20, "the ribbon has many points: {}", ring.len());

        // Its height at the light end is smaller than at the heavy end.
        let spread = |near: f32| {
            let ys: Vec<f32> = ring.iter().filter(|(x, _)| (x - near).abs() < 3.0).map(|(_, y)| *y).collect();
            let hi = ys.iter().copied().fold(f32::MIN, f32::max);
            let lo = ys.iter().copied().fold(f32::MAX, f32::min);
            hi - lo
        };
        let (light, heavy) = (spread(5.0), spread(75.0));
        assert!(heavy > light * 1.5, "the hard end is much fatter: {light} then {heavy}");
        assert!((heavy - 6.0).abs() < 0.6, "and the hard end is about the full width: {heavy}");

        // Every point of the ribbon is within half a width of the line it was drawn along.
        for (x, y) in &ring {
            assert!((0.0..=80.0).contains(&x.clamp(0.0, 80.0)));
            assert!((y - 50.0).abs() <= 3.2, "the ribbon stays beside the line: {y}");
        }
    }

    #[test]
    fn the_eraser_catches_what_it_passes_over_and_nothing_else() {
        let s = drawn(Tool::Pen, &[(0.0, 0.0, 1.0), (100.0, 0.0, 1.0)]);
        assert!(touches(&s, 50.0, 0.0, 1.0), "straight through the middle");
        assert!(touches(&s, 50.0, 3.5, 1.0), "and along its edge, because the line has width");
        assert!(!touches(&s, 50.0, 40.0, 1.0), "well clear of it");
        assert!(!touches(&s, -40.0, 0.0, 1.0), "before it starts");
        assert!(touches(&s, -2.0, 0.0, 3.0), "but a fat eraser reaches the end of it");
        assert!(!touches(&Stroke::default(), 0.0, 0.0, 100.0), "a stroke with no readings is not there");
    }

    #[test]
    fn a_stroke_of_junk_still_makes_a_shape_and_does_not_hang() {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 3.0);
        for _ in 0..40 {
            s.dabs.push(Dab::new(7.0, 7.0, 1.0));
        }
        let ring = outline(&s);
        assert!(!ring.is_empty(), "readings all in one place still draw something");
        assert!(ring.iter().all(|(x, y)| x.is_finite() && y.is_finite()), "and nothing came out as a non-number");
    }
}

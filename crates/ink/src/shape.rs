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
    // The pen's answer to pressure, bent by the stroke's gain: above 1 a light hand
    // already lays down a broad line, below 1 it takes a firm press. A gain of 1 leaves
    // the reading exactly as the pen gave it.
    let felt = if (stroke.gain - 1.0).abs() < f32::EPSILON { pressure.clamp(0.0, 1.0) } else { pressure.clamp(0.0, 1.0).powf(1.0 / stroke.gain) };
    let share = floor + (1.0 - floor) * felt;
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
    // Where the readings give no direction, `sides` assumed the pen was going across the
    // page, left to right; each end assumes the same, so a tap still closes into a dot.
    out.extend(half_circle(
        last,
        width_at(stroke, last.pressure) / 2.0,
        line.get(line.len().saturating_sub(2)).copied().unwrap_or(first),
        (1.0, 0.0),
    ));
    out.extend(right.into_iter().rev());
    out.extend(half_circle(first, width_at(stroke, first.pressure) / 2.0, line.get(1).copied().unwrap_or(last), (-1.0, 0.0)));
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
/// arrived on. `from` is the point before it, which says which way the pen was going;
/// `otherwise` is the way out to assume when `from` is in the same place.
fn half_circle(at: Dab, r: f32, from: Dab, otherwise: (f32, f32)) -> Vec<(f32, f32)> {
    let (dx, dy) = (at.x - from.x, at.y - from.y);
    let len = (dx * dx + dy * dy).sqrt();
    let (ux, uy) = if len > f32::EPSILON { (dx / len, dy / len) } else { otherwise };
    // Start at the left edge, which is where `sides` put it: the normal (-uy, ux), the
    // heading turned a quarter anticlockwise. Its angle is atan2(ux, -uy). Swinging half a
    // turn clockwise from there passes through the heading itself, the tip, and ends on the
    // right edge. At the start of a stroke `from` is the point after, so the heading points
    // back out of the stroke and its "left" is the ribbon's right edge, which is where the
    // outline has just arrived.
    let start = ux.atan2(-uy);
    (0..=CAP)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let a = start - std::f32::consts::PI * i as f32 / CAP as f32;
            (at.x + r * a.cos(), at.y + r * a.sin())
        })
        .collect()
}

/// Whether a circle of `radius` at (`x`, `y`) touches the stroke: the eraser's question,
/// and the one a click on a drawing asks. The saved outline includes the smoothed curve,
/// changing pressure and round ends, so a hit follows the ink that is actually visible.
pub fn touches(stroke: &Stroke, x: f32, y: f32, radius: f32) -> bool {
    if let Some((x0, y0, x1, y1)) = curve_bounds(stroke) {
        let rounding = x0.abs().max(y0.abs()).max(x1.abs()).max(y1.abs()).max(1.0) * f32::EPSILON * 8.0;
        let edge = width_at(stroke, 1.0) / 2.0 + radius.max(0.0) + rounding;
        if x < x0 - edge || y < y0 - edge || x > x1 + edge || y > y1 + edge {
            return false;
        }
    }
    let ring = outline(stroke);
    if encloses(&ring, x, y) {
        return true;
    }
    ring.iter().zip(ring.iter().cycle().skip(1)).any(|(&a, &b)| distance_to_segment(x, y, a, b) <= radius)
}

/// A Catmull-Rom segment stays inside the hull of its equivalent Bezier controls.
/// Unlike the raw readings' box, this also contains the curve's overshoot. Rejecting
/// distant eraser movements here avoids building every stroke's outline each frame.
fn curve_bounds(stroke: &Stroke) -> Option<(f32, f32, f32, f32)> {
    let first = stroke.dabs.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for (i, pair) in stroke.dabs.windows(2).enumerate() {
        let (p1, p2) = (pair[0], pair[1]);
        let p0 = stroke.dabs.get(i.saturating_sub(1)).copied().unwrap_or(p1);
        let p3 = stroke.dabs.get(i + 2).copied().unwrap_or(p2);
        for (x, y) in
            [(p1.x + (p2.x - p0.x) / 6.0, p1.y + (p2.y - p0.y) / 6.0), (p2.x - (p3.x - p1.x) / 6.0, p2.y - (p3.y - p1.y) / 6.0), (p2.x, p2.y)]
        {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    Some((x0, y0, x1, y1))
}

/// How far (`x`, `y`) is from one edge of the ink outline.
fn distance_to_segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let len2 = vx * vx + vy * vy;
    if len2 <= f32::EPSILON {
        return (a.0 - x).hypot(a.1 - y);
    }
    let t = (((x - a.0) * vx + (y - a.1) * vy) / len2).clamp(0.0, 1.0);
    (a.0 + vx * t - x).hypot(a.1 + vy * t - y)
}

/// Whether the outline covers a point under SVG's default nonzero winding rule.
pub(crate) fn encloses(ring: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut winding = 0;
    for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
        let cross = (b.0 - a.0) * (y - a.1) - (x - a.0) * (b.1 - a.1);
        if a.1 <= y && b.1 > y && cross > 0.0 {
            winding += 1;
        } else if a.1 > y && b.1 <= y && cross < 0.0 {
            winding -= 1;
        }
    }
    winding != 0
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

        // Turning the pen up lays down more ink for the same press, and down less; full
        // pressure is the pen's full width whatever it is set to.
        let eager = crate::Stroke::with_gain(Tool::Pen, Rgb::BLACK, 6.0, 2.5);
        let shy = crate::Stroke::with_gain(Tool::Pen, Rgb::BLACK, 6.0, 0.4);
        assert!(width_at(&eager, 0.3) > width_at(&pen, 0.3), "turned up, a light press draws more");
        assert!(width_at(&shy, 0.3) < width_at(&pen, 0.3), "turned down, it draws less");
        for s in [&eager, &shy] {
            assert!((width_at(s, 1.0) - 6.0).abs() < 0.01, "a full press is the full width either way");
            assert!(width_at(s, 0.0) > 0.0, "and nothing still marks the page");
        }
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

    /// Issue #217: the reproduction from the report. A wide straight stroke's saved outline
    /// covers whole round lobes at both ends, not lobes with a diagonal chord cut off them.
    #[test]
    fn both_ends_of_a_wide_stroke_are_whole_round_lobes() {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 40.0);
        s.dabs.push(Dab::new(40.0, 40.0, 1.0));
        s.dabs.push(Dab::new(160.0, 40.0, 1.0));
        let ring = outline(&s);
        // Inside each round end, in all four corners of it, and right out at its tip.
        for (x, y) in [(26.0, 30.0), (26.0, 50.0), (21.0, 40.0), (174.0, 30.0), (174.0, 50.0), (179.0, 40.0)] {
            assert!(encloses(&ring, x, y), "({x}, {y}) is inside a round end of the stroke");
        }
        // And nothing beyond the radius is painted.
        for (x, y) in [(15.0, 40.0), (185.0, 40.0), (25.0, 24.0), (175.0, 56.0)] {
            assert!(!encloses(&ring, x, y), "({x}, {y}) is outside the stroke");
        }
    }

    /// Issue #217: whichever way the pen went and however hard each end was pressed, each
    /// round end starts where the ribbon edge it follows stops, finishes where the next edge
    /// begins, and swings outward, away from the stroke, the whole way round.
    #[test]
    fn the_round_ends_join_the_ribbon_and_bulge_outward_in_every_direction() {
        let lines: [((f32, f32), (f32, f32)); 6] = [
            ((40.0, 40.0), (160.0, 40.0)), // left to right
            ((160.0, 40.0), (40.0, 40.0)), // right to left
            ((50.0, 20.0), (50.0, 140.0)), // down the page
            ((50.0, 140.0), (50.0, 20.0)), // up the page
            ((10.0, 10.0), (90.0, 70.0)),  // a diagonal
            ((90.0, 130.0), (10.0, 20.0)), // the other diagonal, backwards
        ];
        for ((ax, ay), (bx, by)) in lines {
            for (pa, pb) in [(1.0, 1.0), (0.2, 1.0), (1.0, 0.3)] {
                let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 30.0);
                s.dabs.push(Dab::new(ax, ay, pa));
                s.dabs.push(Dab::new((ax + bx) / 2.0, (ay + by) / 2.0, (pa + pb) / 2.0));
                s.dabs.push(Dab::new(bx, by, pb));
                let what = format!("({ax}, {ay}) at {pa} to ({bx}, {by}) at {pb}");

                let line = centre(&s);
                let (left, right) = sides(&s);
                let ring = outline(&s);
                let n = line.len();
                assert_eq!(ring.len(), 2 * n + 2 * (CAP + 1), "{what}: two edges and two ends");
                let end_cap = &ring[n..n + CAP + 1];
                let start_cap = &ring[2 * n + CAP + 1..];

                let near = |p: (f32, f32), q: (f32, f32)| (p.0 - q.0).hypot(p.1 - q.1) < 0.01;
                assert!(near(end_cap[0], left[n - 1]), "{what}: the far end starts on the left edge: {:?} vs {:?}", end_cap[0], left[n - 1]);
                assert!(near(end_cap[CAP], right[n - 1]), "{what}: and finishes on the right edge: {:?} vs {:?}", end_cap[CAP], right[n - 1]);
                assert!(near(start_cap[0], right[0]), "{what}: the near end starts on the right edge: {:?} vs {:?}", start_cap[0], right[0]);
                assert!(near(start_cap[CAP], left[0]), "{what}: and finishes on the left edge: {:?} vs {:?}", start_cap[CAP], left[0]);

                // Outward is the way the pen was going at the far end, and back the way it
                // came at the near one.
                let (len, ux, uy) = {
                    let (dx, dy) = (bx - ax, by - ay);
                    let len = dx.hypot(dy);
                    (len, dx / len, dy / len)
                };
                assert!(len > 0.0);
                let first = line[0];
                let last = line[n - 1];
                for (cap, at, (ox, oy)) in [(end_cap, last, (ux, uy)), (start_cap, first, (-ux, -uy))] {
                    let r = width_at(&s, at.pressure) / 2.0;
                    for &(x, y) in cap {
                        assert!(((x - at.x).hypot(y - at.y) - r).abs() < 0.01, "{what}: every point of an end is a radius out");
                        let ahead = (x - at.x) * ox + (y - at.y) * oy;
                        assert!(ahead > -0.01, "{what}: no point of an end swings back over the stroke: {ahead}");
                    }
                    let tip = cap[CAP / 2];
                    assert!(near(tip, (at.x + r * ox, at.y + r * oy)), "{what}: the middle of an end is its tip: {tip:?}");
                }
            }
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
    fn hit_testing_follows_the_visible_pressure_width_in_either_direction() {
        for reverse in [false, true] {
            let mut stroke = Stroke::new(Tool::Pen, Rgb::BLACK, 40.0);
            stroke.add(Dab::new(0.0, 40.0, 0.0));
            stroke.add(Dab::new(100.0, 40.0, 1.0));
            if reverse {
                stroke.dabs.reverse();
            }
            let ring = outline(&stroke);
            assert!(!encloses(&ring, 10.0, 54.0), "query is outside the visible narrow part");
            assert!(encloses(&ring, 90.0, 54.0), "query is inside the visible thick part");
            assert!(!touches(&stroke, 10.0, 54.0, 3.0), "eraser stays clear of narrow ink, reverse={reverse}");
            assert!(touches(&stroke, 90.0, 54.0, 3.0), "eraser reaches thick ink, reverse={reverse}");
            let mut drawing = crate::Drawing::new(150.0, 90.0);
            drawing.add(stroke);
            assert_eq!(crate::pick::at(&drawing, 10.0, 54.0, 3.0), None, "outside pointer does not select ink");
            assert_eq!(crate::pick::at(&drawing, 90.0, 54.0, 3.0), Some(0), "inside pointer selects ink");
            assert_eq!(drawing.erase_at(10.0, 54.0, 3.0), 0, "outside eraser preserves stroke");
            assert_eq!(drawing.erase_at(90.0, 54.0, 3.0), 1, "inside eraser removes stroke");
        }
    }

    #[test]
    fn hits_include_the_smoothed_curve_dots_caps_and_eraser_radius() {
        let mut curve = drawn(Tool::Pen, &[(0.0, 0.0, 1.0), (40.0, 40.0, 1.0), (80.0, 0.0, 1.0)]);
        curve.width = 1.0;
        let line = centre(&curve);
        let at = line.iter().min_by(|a, b| (a.x - 20.0).abs().total_cmp(&(b.x - 20.0).abs())).expect("curve point");
        assert!(encloses(&outline(&curve), at.x, at.y), "smoothed centre is visible ink");
        assert!(touches(&curve, at.x, at.y, 0.0), "hit follows the curve rather than raw chords");

        for count in [1, 5] {
            let dot = drawn(Tool::Highlighter, &vec![(20.0, 20.0, 1.0); count]);
            assert!(touches(&dot, 20.0, 20.0, 0.0), "stationary readings remain a dot");
            assert!(touches(&dot, 24.0, 20.0, 1.1), "eraser radius reaches the dot edge");
            assert!(!touches(&dot, 25.0, 20.0, 1.0), "eraser clear of dot preserves it");
        }
        let line = drawn(Tool::Highlighter, &[(20.0, 20.0, 1.0), (80.0, 20.0, 1.0)]);
        assert!(touches(&line, 16.0, 20.0, 1.1), "eraser reaches the round start cap");
        assert!(touches(&line, 84.0, 20.0, 1.1), "eraser reaches the round end cap");
        assert!(!touches(&line, 85.0, 20.0, 1.0), "eraser remains outside the round cap");
    }

    #[test]
    fn outline_containment_uses_nonzero_winding_for_retraced_loops() {
        let twice = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0)];
        assert!(encloses(&twice, 10.0, 10.0), "same-direction loops remain filled as in SVG");
        assert!(!encloses(&twice, 30.0, 10.0), "outside remains empty");
        let cancelling = [(0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (0.0, 0.0), (0.0, 20.0), (20.0, 20.0), (20.0, 0.0)];
        assert!(!encloses(&cancelling, 10.0, 10.0), "opposite winding cancels");
    }

    #[test]
    fn hit_rejection_preserves_visible_spline_overshoot() {
        let mut stroke = drawn(Tool::Pen, &[(0.0, 0.0, 1.0), (100.0, 0.0, 1.0), (101.0, 0.0, 1.0), (200.0, 200.0, 1.0)]);
        stroke.width = 1.0;
        let curve = centre(&stroke);
        let ring = outline(&stroke);
        let low = curve.iter().find(|point| point.y < -5.0 && encloses(&ring, point.x, point.y)).expect("visible spline overshoot beyond raw bounds");
        assert!(touches(&stroke, low.x, low.y, 0.0), "conservative rejection retains overshoot");
        assert!(!touches(&stroke, -100.0, -100.0, 3.0), "distant miss remains a miss");
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
        // A tap is many readings in one place; it is a whole dot, not half of one.
        for (x, y) in [(8.3, 7.0), (5.7, 7.0), (7.0, 8.3), (7.0, 5.7)] {
            assert!(encloses(&ring, x, y), "({x}, {y}) is inside the dot");
        }
    }
}

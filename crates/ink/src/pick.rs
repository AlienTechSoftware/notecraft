//! Picking ink up: which strokes a pointer or a lasso has caught, and what can be done to
//! them once they are held.
//!
//! Ink is a flat list of strokes in the file, and a selection is never written down: it is
//! what someone is holding at this moment, like a hand on a sheet of paper. So everything
//! here takes the strokes it works on by their place in that list, and none of it knows
//! about a window.

use crate::{Drawing, Stroke, shape};

/// The box around the strokes at these places, with the width of each line taken in.
/// `None` when nothing is held, or when nothing held is really there.
pub fn bounds(drawing: &Drawing, held: &[usize]) -> Option<(f32, f32, f32, f32)> {
    held.iter()
        .filter_map(|i| drawing.strokes.get(*i))
        .filter_map(Stroke::bounds)
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// The topmost stroke under a point, which is the last one drawn there: ink laid down later
/// sits on top, so that is what a hand reaches first.
pub fn at(drawing: &Drawing, x: f32, y: f32, radius: f32) -> Option<usize> {
    drawing.strokes.iter().enumerate().rev().find(|(_, s)| shape::touches(s, x, y, radius)).map(|(i, _)| i)
}

const CLOSING_TOUCH_FLOOR: f32 = 2.0;
const CLOSING_TRAVEL: f32 = 8.0;
const OPEN_LASSO_MAJORITY: f32 = 0.65;

/// Every stroke caught by a lasso, allowing an open `C`-shaped lasso when `allow_partial_majority`
/// is reflected by `inside` (at least 65% of readings and the stroke's centre enclosed, or all
/// readings enclosed).
pub fn inside(drawing: &Drawing, loop_: &[(f32, f32)]) -> Vec<usize> {
    inside_with(drawing, loop_, false)
}

/// Every stroke caught by a lasso. When `require_closed_loop` is true, the gesture path must
/// close back onto itself (`closes_into_loop`) and every reading of the stroke must be inside;
/// when false, an open `C`-bracket is accepted and a stroke is caught when all readings are
/// enclosed or when at least 65% of readings and its bounding-box centre are enclosed.
pub fn inside_with(drawing: &Drawing, loop_: &[(f32, f32)], require_closed_loop: bool) -> Vec<usize> {
    if loop_.len() < 3 {
        return Vec::new();
    }
    if require_closed_loop && !closes_into_loop(loop_, 4.0) {
        return Vec::new();
    }
    if !require_closed_loop && !is_open_or_closed_lasso(loop_, 4.0) {
        return Vec::new();
    }
    drawing.strokes.iter().enumerate().filter(|(_, s)| stroke_inside_lasso(s, loop_, !require_closed_loop)).map(|(i, _)| i).collect()
}

fn stroke_inside_lasso(stroke: &Stroke, loop_: &[(f32, f32)], allow_partial_majority: bool) -> bool {
    if stroke.dabs.is_empty() || loop_.len() < 3 {
        return false;
    }
    let enclosed = stroke.dabs.iter().filter(|d| encloses(loop_, d.x, d.y)).count();
    if enclosed == stroke.dabs.len() {
        return true;
    }
    if !allow_partial_majority {
        return false;
    }
    let Some(first) = stroke.dabs.first() else {
        return false;
    };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for d in &stroke.dabs {
        x0 = x0.min(d.x);
        y0 = y0.min(d.y);
        x1 = x1.max(d.x);
        y1 = y1.max(d.y);
    }
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    #[allow(clippy::cast_precision_loss)]
    let fraction = enclosed as f32 / stroke.dabs.len() as f32;
    fraction >= OPEN_LASSO_MAJORITY && encloses(loop_, cx, cy)
}

/// Whether the lasso path is either a closed loop or a valid open `C`-bracket gesture.
pub fn is_open_or_closed_lasso(loop_: &[(f32, f32)], touch: f32) -> bool {
    if loop_.len() < 3 {
        return false;
    }
    if closes_into_loop(loop_, touch) {
        return true;
    }
    let travelled: f32 = loop_.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
    if travelled < (touch * 3.0).max(24.0) {
        return false;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (loop_[0].0, loop_[0].1, loop_[0].0, loop_[0].1);
    for &(x, y) in loop_ {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x1 - x0) >= 8.0 && (y1 - y0) >= 8.0
}

/// Whether the gesture path closes back onto itself.
pub fn closes_into_loop(loop_: &[(f32, f32)], touch: f32) -> bool {
    if loop_.len() < 4 {
        return false;
    }
    let reach = touch.max(CLOSING_TOUCH_FLOOR);
    let mut travelled = Vec::with_capacity(loop_.len());
    travelled.push(0.0_f32);
    let mut total = 0.0_f32;
    for w in loop_.windows(2) {
        total += (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        travelled.push(total);
    }
    let apart = reach * CLOSING_TRAVEL;
    for j in 2..(loop_.len() - 1) {
        let (c, d) = (loop_[j], loop_[j + 1]);
        for i in 0..(j - 1) {
            if travelled[j] - travelled[i + 1] > apart && segments_meet(loop_[i], loop_[i + 1], c, d, reach) {
                return true;
            }
        }
    }
    let (first, last) = (loop_[0], loop_[loop_.len() - 1]);
    let end_gap = (last.0 - first.0).hypot(last.1 - first.1);
    end_gap <= reach && total >= reach * 3.0
}

fn point_to_segment_sq(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len_sq = dx * dx + dy * dy;
    if len_sq <= f32::EPSILON {
        let (px, py) = (p.0 - a.0, p.1 - a.1);
        return px * px + py * py;
    }
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len_sq).clamp(0.0, 1.0);
    let (nx, ny) = (a.0 + t * dx, a.1 + t * dy);
    let (px, py) = (p.0 - nx, p.1 - ny);
    px * px + py * py
}

fn segments_meet(a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32), reach: f32) -> bool {
    let side = |u: (f32, f32), v: (f32, f32), p: (f32, f32)| (v.0 - u.0) * (p.1 - u.1) - (v.1 - u.1) * (p.0 - u.0);
    if side(a, b, c) * side(a, b, d) < 0.0 && side(c, d, a) * side(c, d, b) < 0.0 {
        return true;
    }
    let reach_sq = reach * reach;
    point_to_segment_sq(a, c, d) <= reach_sq
        || point_to_segment_sq(b, c, d) <= reach_sq
        || point_to_segment_sq(c, a, b) <= reach_sq
        || point_to_segment_sq(d, a, b) <= reach_sq
}

/// Every stroke a rectangle (two corners, in any order) touches: a reading inside it, or the
/// line between two readings crossing it. A selection box takes ink this way, as it takes the
/// containers it touches.
pub fn touching(drawing: &Drawing, rect: (f32, f32, f32, f32)) -> Vec<usize> {
    let (x0, x1) = (rect.0.min(rect.2), rect.0.max(rect.2));
    let (y0, y1) = (rect.1.min(rect.3), rect.1.max(rect.3));
    let inside = |x: f32, y: f32| x >= x0 && x <= x1 && y >= y0 && y <= y1;
    // Liang-Barsky: whether the segment from a to b passes through the rectangle.
    let crosses = |(ax, ay): (f32, f32), (bx, by): (f32, f32)| {
        let (dx, dy) = (bx - ax, by - ay);
        let (mut t0, mut t1) = (0.0_f32, 1.0_f32);
        for (p, q) in [(-dx, ax - x0), (dx, x1 - ax), (-dy, ay - y0), (dy, y1 - ay)] {
            if p == 0.0 {
                if q < 0.0 {
                    return false;
                }
            } else {
                let r = q / p;
                if p < 0.0 {
                    t0 = t0.max(r);
                } else {
                    t1 = t1.min(r);
                }
                if t0 > t1 {
                    return false;
                }
            }
        }
        true
    };
    drawing
        .strokes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.dabs.iter().any(|d| inside(d.x, d.y)) || s.dabs.windows(2).any(|w| crosses((w[0].x, w[0].y), (w[1].x, w[1].y))))
        .map(|(i, _)| i)
        .collect()
}

/// Whether a closed path goes round a point,/// Whether a closed path goes round a point, by counting the times a ray leaving the point
/// crosses it: an odd number means inside. The path is treated as closed whether or not its
/// last point meets its first, because a hand drawing a loop rarely closes it exactly.
pub fn encloses(loop_: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let mut j = loop_.len() - 1;
    for i in 0..loop_.len() {
        let (xi, yi) = loop_[i];
        let (xj, yj) = loop_[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Moves the strokes at these places by (`dx`, `dy`).
pub fn move_by(drawing: &mut Drawing, held: &[usize], dx: f32, dy: f32) {
    if !dx.is_finite() || !dy.is_finite() {
        return;
    }
    for i in held {
        if let Some(s) = drawing.strokes.get_mut(*i) {
            for d in &mut s.dabs {
                d.x += dx;
                d.y += dy;
            }
        }
    }
}

/// Stretches them about (`ox`, `oy`). The line's own width follows the smaller of the two
/// factors, so a stroke made half as wide does not keep a fat edge.
pub fn scale_about(drawing: &mut Drawing, held: &[usize], ox: f32, oy: f32, fx: f32, fy: f32) {
    let sane = |f: f32| if f.is_finite() { f.clamp(0.05, 20.0) } else { 1.0 };
    let (fx, fy) = (sane(fx), sane(fy));
    for i in held {
        if let Some(s) = drawing.strokes.get_mut(*i) {
            for d in &mut s.dabs {
                d.x = ox + (d.x - ox) * fx;
                d.y = oy + (d.y - oy) * fy;
            }
            s.width = (s.width * fx.min(fy)).clamp(crate::MIN_WIDTH, crate::MAX_WIDTH);
        }
    }
}

/// A quarter turn or a mirror of held ink (issue #544).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// A quarter turn clockwise, as the page is seen.
    Right,
    /// A quarter turn anticlockwise.
    Left,
    /// Mirrored left to right.
    FlipHorizontal,
    /// Mirrored top to bottom.
    FlipVertical,
}

/// Turns or mirrors the strokes at these places as one group about (`ox`, `oy`), which the
/// caller gives as the middle of the box round them, so the group stays where it was. Each
/// stroke keeps its pen, colour, width, pressure readings and place in the order.
pub fn turn_about(drawing: &mut Drawing, held: &[usize], ox: f32, oy: f32, turn: Turn) {
    if !ox.is_finite() || !oy.is_finite() {
        return;
    }
    for i in held {
        if let Some(s) = drawing.strokes.get_mut(*i) {
            for d in &mut s.dabs {
                let (dx, dy) = (d.x - ox, d.y - oy);
                // The page's y grows downward, so a clockwise turn takes right to down.
                let (nx, ny) = match turn {
                    Turn::Right => (-dy, dx),
                    Turn::Left => (dy, -dx),
                    Turn::FlipHorizontal => (-dx, dy),
                    Turn::FlipVertical => (dx, -dy),
                };
                d.x = ox + nx;
                d.y = oy + ny;
            }
        }
    }
}

/// Takes the strokes at these places out, and gives back how many went. Places that are not
/// there are ignored, so a stale selection cannot remove the wrong ink.
pub fn remove(drawing: &mut Drawing, held: &[usize]) -> usize {
    let mut going: Vec<usize> = held.iter().copied().filter(|i| *i < drawing.strokes.len()).collect();
    going.sort_unstable();
    going.dedup();
    for i in going.iter().rev() {
        drawing.strokes.remove(*i);
    }
    going.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dab, Rgb, Stroke, Tool};

    /// Issue #544: held ink turns and mirrors as one group about the middle of its box, keeps
    /// its width and order, and four quarter turns or two mirrors put it back.
    #[test]
    fn held_ink_turns_and_mirrors_about_its_middle() {
        let mut d = Drawing::new(200.0, 200.0);
        d.add(line(10.0, 50.0, 20.0));
        d.add(line(10.0, 30.0, 40.0));
        d.add(line(100.0, 120.0, 100.0));
        let before = d.clone();
        let held = [0, 1];
        let (x0, y0, x1, y1) = bounds(&d, &held).expect("box");
        let (ox, oy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        turn_about(&mut d, &held, ox, oy, Turn::Right);
        let (a0, b0, a1, b1) = bounds(&d, &held).expect("box");
        assert!(((a0 + a1) / 2.0 - ox).abs() < 0.01 && ((b0 + b1) / 2.0 - oy).abs() < 0.01, "the middle stays put");
        assert!(((a1 - a0) - (y1 - y0)).abs() < 0.01 && ((b1 - b0) - (x1 - x0)).abs() < 0.01, "width and height swap");
        // The long line ran left to right at the top; turned clockwise it runs top to bottom on the right.
        let first = &d.strokes[0].dabs;
        assert!((first[0].x - first[first.len() - 1].x).abs() < 0.01 && first[0].y < first[first.len() - 1].y);
        assert_eq!(d.strokes[2], before.strokes[2], "ink not held is left alone");
        assert_eq!(d.strokes[0].width, before.strokes[0].width);
        for _ in 0..3 {
            turn_about(&mut d, &held, ox, oy, Turn::Right);
        }
        for (a, b) in d.strokes.iter().zip(&before.strokes) {
            for (p, q) in a.dabs.iter().zip(&b.dabs) {
                assert!((p.x - q.x).abs() < 0.01 && (p.y - q.y).abs() < 0.01, "four turns put it back");
            }
        }
        turn_about(&mut d, &held, ox, oy, Turn::Left);
        turn_about(&mut d, &held, ox, oy, Turn::Right);
        turn_about(&mut d, &held, ox, oy, Turn::FlipHorizontal);
        let first = &d.strokes[0].dabs;
        assert!(first[0].x > first[first.len() - 1].x, "mirrored left to right: {first:?}");
        turn_about(&mut d, &held, ox, oy, Turn::FlipHorizontal);
        turn_about(&mut d, &held, ox, oy, Turn::FlipVertical);
        assert!((d.strokes[1].dabs[0].y - (oy + (oy - 40.0))).abs() < 0.01, "mirrored top to bottom");
        turn_about(&mut d, &held, ox, oy, Turn::FlipVertical);
        for (a, b) in d.strokes.iter().zip(&before.strokes) {
            for (p, q) in a.dabs.iter().zip(&b.dabs) {
                assert!((p.x - q.x).abs() < 0.01 && (p.y - q.y).abs() < 0.01);
            }
        }
    }

    /// A straight stroke from (x0, y) to (x1, y).
    fn line(x0: f32, x1: f32, y: f32) -> Stroke {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        s.dabs.push(Dab::new(x0, y, 1.0));
        s.dabs.push(Dab::new((x0 + x1) / 2.0, y, 1.0));
        s.dabs.push(Dab::new(x1, y, 1.0));
        s
    }

    fn sheet() -> Drawing {
        let mut d = Drawing::new(400.0, 300.0);
        d.add(line(10.0, 60.0, 20.0));
        d.add(line(10.0, 60.0, 50.0));
        d.add(line(200.0, 300.0, 200.0));
        d
    }

    /// A selection box takes every stroke it touches, as it takes every container it touches:
    /// any part inside, or a line passing right through it between two readings.
    #[test]
    fn a_rectangle_takes_every_stroke_it_touches() {
        let d = sheet();
        assert_eq!(touching(&d, (0.0, 0.0, 30.0, 30.0)), vec![0], "the end of the first line is inside");
        assert_eq!(touching(&d, (0.0, 10.0, 100.0, 60.0)), vec![0, 1], "both short lines");
        assert_eq!(touching(&d, (240.0, 190.0, 260.0, 210.0)), vec![2], "the long line passes through, no reading inside");
        assert_eq!(touching(&d, (100.0, 100.0, 150.0, 150.0)), Vec::<usize>::new(), "nothing there");
        assert_eq!(touching(&d, (30.0, 60.0, 0.0, 0.0)), vec![0, 1], "corners in any order");
    }

    #[test]
    fn the_box_round_what_is_held_takes_in_the_width_of_the_lines() {
        let d = sheet();
        assert_eq!(bounds(&d, &[]), None, "nothing held, no box");
        assert_eq!(bounds(&d, &[99]), None, "nor for ink that is not there");
        let (x0, y0, x1, y1) = bounds(&d, &[0, 1]).expect("a box");
        assert!((x0 - 9.0).abs() < 0.01 && (x1 - 61.0).abs() < 0.01, "a point either side for the line's width: {x0} {x1}");
        assert!((y0 - 19.0).abs() < 0.01 && (y1 - 51.0).abs() < 0.01, "{y0} {y1}");
    }

    #[test]
    fn a_point_picks_up_the_stroke_under_it_and_the_topmost_when_they_overlap() {
        let mut d = sheet();
        assert_eq!(at(&d, 30.0, 20.0, 1.0), Some(0));
        assert_eq!(at(&d, 30.0, 50.0, 1.0), Some(1));
        assert_eq!(at(&d, 30.0, 120.0, 1.0), None, "nothing there");

        // Ink laid down later sits on top, so that is what a hand reaches first.
        d.add(line(10.0, 60.0, 20.0));
        assert_eq!(at(&d, 30.0, 20.0, 1.0), Some(3), "the newer one");
    }

    #[test]
    fn a_lasso_takes_what_is_wholly_inside_it_and_leaves_the_rest() {
        let d = sheet();
        let round_both = [(0.0, 0.0), (80.0, 0.0), (80.0, 70.0), (0.0, 70.0)];
        assert_eq!(inside(&d, &round_both), vec![0, 1]);

        let round_first = [(0.0, 0.0), (80.0, 0.0), (80.0, 35.0), (0.0, 35.0)];
        assert_eq!(inside(&d, &round_first), vec![0]);

        // A loop that only clips the end of a stroke does not take it: a lasso thrown
        // round two words must not drag in the long line passing through them.
        let clips = [(0.0, 0.0), (30.0, 0.0), (30.0, 35.0), (0.0, 35.0)];
        assert!(inside(&d, &clips).is_empty(), "half a stroke is not a stroke");

        for silly in [vec![], vec![(0.0, 0.0)], vec![(0.0, 0.0), (10.0, 10.0)]] {
            assert!(inside(&d, &silly).is_empty(), "a loop needs three points to be a loop");
        }
    }

    #[test]
    fn a_loop_encloses_what_is_in_it_however_it_was_drawn() {
        let square = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)];
        assert!(encloses(&square, 5.0, 5.0));
        assert!(!encloses(&square, 15.0, 5.0));
        assert!(!encloses(&square, 5.0, -5.0));
        // A hand never closes a loop exactly, and it must not have to.
        let open = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (0.0, 1.0)];
        assert!(encloses(&open, 5.0, 5.0));
    }

    #[test]
    fn held_ink_moves_and_nothing_else_does() {
        let mut d = sheet();
        let before = d.strokes[2].clone();
        move_by(&mut d, &[0], 100.0, 5.0);
        assert!((d.strokes[0].dabs[0].x - 110.0).abs() < 0.01);
        assert!((d.strokes[0].dabs[0].y - 25.0).abs() < 0.01);
        assert_eq!(d.strokes[2], before, "the ink that was not held stayed put");

        // A move that is not a number does nothing rather than losing the ink.
        let now = d.strokes[0].clone();
        move_by(&mut d, &[0], f32::NAN, 1.0);
        assert_eq!(d.strokes[0], now);
    }

    #[test]
    fn held_ink_stretches_about_the_corner_it_is_dragged_from() {
        let mut d = sheet();
        scale_about(&mut d, &[0], 10.0, 20.0, 2.0, 1.0);
        assert!((d.strokes[0].dabs[0].x - 10.0).abs() < 0.01, "the corner it was pulled from stays put");
        assert!((d.strokes[0].dabs[2].x - 110.0).abs() < 0.01, "and the far end goes twice as far");
        assert!((d.strokes[0].dabs[2].y - 20.0).abs() < 0.01, "with nothing happening the other way");
        assert!((d.strokes[0].width - 2.0).abs() < 0.01, "a stroke stretched one way keeps its weight");

        let mut d = sheet();
        scale_about(&mut d, &[0], 0.0, 0.0, 0.5, 0.5);
        assert!((d.strokes[0].width - 1.0).abs() < 0.01, "made half the size, it is drawn half as heavily");
    }

    #[test]
    fn deleting_held_ink_takes_the_right_strokes_and_survives_a_stale_hold() {
        let mut d = sheet();
        let third = d.strokes[2].clone();
        assert_eq!(remove(&mut d, &[0, 1]), 2);
        assert_eq!(d.strokes.len(), 1);
        assert_eq!(d.strokes[0], third, "the one that was not held is the one that is left");

        // A hold naming ink that has already gone, or the same ink twice, is survivable.
        assert_eq!(remove(&mut d, &[0, 0, 7]), 1);
        assert!(d.strokes.is_empty());
        assert_eq!(remove(&mut d, &[3]), 0);
    }

    #[test]
    fn open_c_bracket_selects_in_lasso_tool_but_requires_closure_when_requested() {
        let mut d = Drawing::new(400.0, 300.0);
        // Stroke centered at x=95, y=40 with 5 dabs; 4 of 5 (80%) inside the open C-bracket.
        let mut mostly_inside = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        for x in [85.0, 90.0, 95.0, 100.0, 130.0] {
            mostly_inside.dabs.push(Dab::new(x, 40.0, 1.0));
        }
        d.add(mostly_inside);

        // Open 3-sided C-bracket around [75..125, 10..70] (open on the right).
        let open_bracket = [(125.0, 10.0), (75.0, 10.0), (75.0, 70.0), (125.0, 70.0)];
        assert_eq!(inside(&d, &open_bracket), vec![0]);
        assert_eq!(inside_with(&d, &open_bracket, false), vec![0]);
        assert!(inside_with(&d, &open_bracket, true).is_empty(), "strict loop mode rejects open C-bracket");

        // Closed loop selects wholly enclosed strokes even when closure is required.
        let mut wholly_inside = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        for x in [85.0, 95.0, 110.0] {
            wholly_inside.dabs.push(Dab::new(x, 50.0, 1.0));
        }
        d.add(wholly_inside);
        let closed_loop = [(65.0, 5.0), (140.0, 5.0), (140.0, 80.0), (65.0, 80.0), (65.0, 5.0)];
        assert_eq!(inside_with(&d, &closed_loop, true), vec![0, 1]);
    }
}

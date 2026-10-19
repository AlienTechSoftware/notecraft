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

/// Every stroke drawn wholly inside a lasso. Wholly, not partly: a loop thrown round two
/// words should not also drag in the long line that happens to pass through it.
pub fn inside(drawing: &Drawing, loop_: &[(f32, f32)]) -> Vec<usize> {
    if loop_.len() < 3 {
        return Vec::new();
    }
    drawing
        .strokes
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.dabs.is_empty() && s.dabs.iter().all(|d| encloses(loop_, d.x, d.y)))
        .map(|(i, _)| i)
        .collect()
}

/// Whether a closed path goes round a point, by counting the times a ray leaving the point
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
}

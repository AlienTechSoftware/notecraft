//! Collect All (issue #23): pulls what has drifted apart on the page's open canvas back
//! together, boxes and ink alike.
//!
//! The page is looked at as bands. Along each axis, the stretches of the page that hold nothing
//! at all, from one edge of the page to the other, are the gaps; every gap wider than `gap` is
//! closed down to `gap`, and the empty margin before the first thing is closed down to `gap` as
//! well. Nothing else moves: two things that touched or overlapped still do, everything keeps
//! its order across and down the page, and since only empty bands are taken out, nothing ends
//! up on top of anything it was not already on top of.

use egui::{Rect, Vec2, vec2};

/// How far each of `items` (rectangles in page points) moves, in the same order.
pub fn gather(items: &[Rect], gap: f32) -> Vec<Vec2> {
    let gap = if gap.is_finite() { gap.max(0.0) } else { 0.0 };
    let xs = axis(&items.iter().map(|r| (r.min.x, r.max.x)).collect::<Vec<_>>(), gap);
    let ys = axis(&items.iter().map(|r| (r.min.y, r.max.y)).collect::<Vec<_>>(), gap);
    xs.into_iter().zip(ys).map(|(x, y)| vec2(x, y)).collect()
}

/// Along one axis: how far each span moves when every empty stretch wider than `gap` is
/// closed to `gap`, the stretch before the first span included.
fn axis(spans: &[(f32, f32)], gap: f32) -> Vec<f32> {
    let clean: Vec<(f32, f32)> = spans.iter().map(|&(a, b)| if a.is_finite() && b.is_finite() { (a.min(b), a.max(b)) } else { (0.0, 0.0) }).collect();
    // The occupied stretches, merged where they touch or overlap.
    let mut order: Vec<usize> = (0..clean.len()).collect();
    order.sort_by(|&a, &b| clean[a].0.total_cmp(&clean[b].0));
    let mut bands: Vec<(f32, f32)> = Vec::new();
    for &i in &order {
        let (a, b) = clean[i];
        match bands.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => bands.push((a, b)),
        }
    }
    // Where each band starts once the gaps before it are closed.
    let mut shift = Vec::with_capacity(bands.len());
    let mut end = 0.0_f32;
    for (n, &(a, b)) in bands.iter().enumerate() {
        let before = if n == 0 { a.max(0.0) } else { a - end };
        let start = if n == 0 { a.min(gap).max(a.min(0.0)) } else { end + before.min(gap) };
        shift.push(start - a);
        end = start + (b - a);
    }
    clean.iter().map(|&(a, _)| bands.iter().position(|&(lo, hi)| a >= lo && a <= hi).and_then(|n| shift.get(n).copied()).unwrap_or(0.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn r(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(pos2(x0, y0), pos2(x1, y1))
    }

    fn moved(items: &[Rect], gap: f32) -> Vec<Rect> {
        items.iter().zip(gather(items, gap)).map(|(r, d)| r.translate(d)).collect()
    }

    #[test]
    fn things_far_apart_come_together_and_keep_their_order() {
        let items = [r(0.0, 0.0, 100.0, 50.0), r(2000.0, 3000.0, 2100.0, 3040.0)];
        let after = moved(&items, 24.0);
        assert_eq!(after[0], items[0], "the first thing, at the corner, stays");
        assert_eq!(after[1], r(124.0, 74.0, 224.0, 114.0), "the far one comes to within the gap, across and down");
    }

    #[test]
    fn small_gaps_and_overlaps_are_left_as_they_are() {
        let items = [r(0.0, 0.0, 100.0, 100.0), r(110.0, 20.0, 200.0, 80.0), r(50.0, 90.0, 150.0, 140.0)];
        assert!(gather(&items, 24.0).iter().all(|d| *d == Vec2::ZERO), "nothing is more than the gap apart");
    }

    #[test]
    fn a_drawing_that_drifted_off_to_the_side_comes_back_beside_the_rest() {
        // A note at the top, and a sketch of two strokes far to the right of it.
        let items = [r(40.0, 20.0, 360.0, 200.0), r(5000.0, 60.0, 5100.0, 120.0), r(5080.0, 100.0, 5200.0, 180.0)];
        let after = moved(&items, 24.0);
        assert_eq!(after[0], r(24.0, 20.0, 344.0, 200.0), "the margin before the first thing closes to the gap");
        assert_eq!(after[1].min.x, 344.0 + 24.0, "the sketch comes to the gap's width beside the note");
        assert_eq!(after[2].min.x - after[1].min.x, 80.0, "and its strokes keep their places to each other");
        assert_eq!((after[1].min.y, after[2].min.y), (60.0, 100.0), "nothing needed to move down the page");
    }

    #[test]
    fn nothing_ends_up_on_anything_it_was_not_on_before() {
        let items = [
            r(0.0, 0.0, 50.0, 50.0),
            r(900.0, 0.0, 950.0, 50.0),
            r(0.0, 900.0, 50.0, 950.0),
            r(900.0, 900.0, 950.0, 950.0),
            r(400.0, 400.0, 600.0, 600.0),
        ];
        let after = moved(&items, 10.0);
        for i in 0..after.len() {
            for j in (i + 1)..after.len() {
                assert!(!after[i].intersects(after[j]) || items[i].intersects(items[j]), "{i} and {j}: {:?} {:?}", after[i], after[j]);
            }
        }
        // The middle one is still between the others, both ways.
        assert!(after[4].min.x > after[0].max.x && after[4].max.x < after[1].min.x);
        assert!(after[4].min.y > after[0].max.y && after[4].max.y < after[2].min.y);
    }

    #[test]
    fn nothing_and_junk_move_nothing() {
        assert!(gather(&[], 24.0).is_empty());
        let junk = [r(f32::NAN, 0.0, 10.0, 10.0), r(0.0, 0.0, 10.0, 10.0)];
        assert!(gather(&junk, 24.0).iter().all(|d| d.x.is_finite() && d.y.is_finite()));
        assert_eq!(gather(&[r(500.0, 500.0, 510.0, 510.0)], f32::NAN), vec![vec2(-500.0, -500.0)], "a junk gap is no gap");
    }
}

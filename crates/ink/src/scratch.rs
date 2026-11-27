//! The scratch-out gesture (issue #569): a quick zigzag back and forth over ink rubs that ink out,
//! as OneNote's does, when it is turned on.
//!
//! It is told from writing by its shape and by what it is over, and only when it is plainly both:
//! many passes back and forth along one line, the pen's path several times as long as the ground
//! it covers, and existing strokes lying mostly under it. A scribble that is not clearly that is
//! ink like any other, and is kept.

use crate::{Drawing, Stroke, shape};

/// The fewest passes back and forth a scratch makes along its line.
const MIN_PASSES: usize = 5;

/// Whether a stroke is shaped like a scratch-out: at least five passes back and forth along its
/// longer side, each going a good part of the way, its path at least three times as long as
/// that side, and big enough to be meant.
pub fn is_scratch(stroke: &Stroke) -> bool {
    let dabs = &stroke.dabs;
    if dabs.len() < 8 {
        return false;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for d in dabs {
        x0 = x0.min(d.x);
        y0 = y0.min(d.y);
        x1 = x1.max(d.x);
        y1 = y1.max(d.y);
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let along_x = w >= h;
    let extent = w.max(h);
    if extent < 8.0 {
        return false;
    }
    let path: f32 = dabs.windows(2).map(|p| p[0].distance_to(p[1])).sum();
    if path < extent * 3.0 {
        return false;
    }
    // Passes along the longer side: runs one way that go a good part of the way, so a tremor
    // is no pass. The last run counts like the others.
    let reach = extent * 0.4;
    let coord = |i: usize| if along_x { dabs[i].x } else { dabs[i].y };
    let (mut passes, mut direction, mut start) = (0usize, 0i8, coord(0));
    for i in 1..dabs.len() {
        let step = coord(i) - coord(i - 1);
        let now = if step > 0.0 {
            1
        } else if step < 0.0 {
            -1
        } else {
            0
        };
        if now == 0 {
            continue;
        }
        if direction != 0 && now != direction {
            if (coord(i - 1) - start).abs() >= reach {
                passes += 1;
            }
            start = coord(i - 1);
        }
        direction = now;
    }
    if (coord(dabs.len() - 1) - start).abs() >= reach {
        passes += 1;
    }
    passes >= MIN_PASSES
}

/// Points along a stroke's path no more than `step` apart, its readings among them.
fn along(stroke: &Stroke, step: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for pair in stroke.dabs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let n = ((a.distance_to(b) / step).ceil() as usize).clamp(1, 2048);
        for k in 0..n {
            let t = k as f32 / n as f32;
            out.push((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    if let Some(last) = stroke.dabs.last() {
        out.push((last.x, last.y));
    }
    out
}

/// Whether more than half a stroke's length lies inside, however its readings are spaced: each
/// line between two readings is weighed by its length. A dot is inside or not.
fn mostly_inside(stroke: &Stroke, inside: &dyn Fn((f32, f32)) -> bool) -> bool {
    if let [only] = stroke.dabs.as_slice() {
        return inside((only.x, only.y));
    }
    let (mut within, mut total) = (0.0_f32, 0.0_f32);
    for pair in stroke.dabs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let len = a.distance_to(b);
        let n = ((len / 0.5).ceil() as usize).clamp(1, 4096);
        let hit = (0..n)
            .filter(|k| {
                let t = (*k as f32 + 0.5) / n as f32;
                inside((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t))
            })
            .count();
        within += len * hit as f32 / n as f32;
        total += len;
    }
    if total == 0.0 {
        // Readings all in one place (a press whose pressure changed): a dot.
        return stroke.dabs.first().is_some_and(|d| inside((d.x, d.y)));
    }
    within * 2.0 > total
}

/// The strokes a scratch rubs out: those its path crosses that lie mostly inside the ground it
/// covered, measured along their length, so a long line it only clips at one end is left alone.
/// Empty when the scratch is over no ink, which then stays as a scribble.
pub fn scratched(drawing: &Drawing, scratch: &Stroke) -> Vec<usize> {
    let margin = scratch.width.max(1.0);
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for d in &scratch.dabs {
        x0 = x0.min(d.x - margin);
        y0 = y0.min(d.y - margin);
        x1 = x1.max(d.x + margin);
        y1 = y1.max(d.y + margin);
    }
    let inside = |(x, y): (f32, f32)| x >= x0 && x <= x1 && y >= y0 && y <= y1;
    let path = along(scratch, 1.0);
    drawing
        .strokes
        .iter()
        .enumerate()
        .filter(|(_, s)| mostly_inside(s, &inside) && path.iter().any(|(x, y)| shape::touches(s, *x, *y, scratch.width / 2.0 + 1.0)))
        .map(|(i, _)| i)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Dab, Rgb, Tool};

    fn stroke(points: &[(f32, f32)]) -> Stroke {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        s.dabs = points.iter().map(|&(x, y)| Dab::new(x, y, 0.5)).collect();
        s
    }

    /// A zigzag back and forth: `passes` runs across `width`, dropping a little each time.
    fn zigzag(x: f32, y: f32, width: f32, passes: usize) -> Stroke {
        let mut points = Vec::new();
        for pass in 0..=passes {
            let at = if pass % 2 == 0 { x } else { x + width };
            points.push((at, y + pass as f32 * 2.0));
            points.push((at + if pass % 2 == 0 { 1.0 } else { -1.0 }, y + pass as f32 * 2.0 + 0.5));
        }
        stroke(&points)
    }

    /// Issue #569: a zigzag of many passes is a scratch; a line, a loop, a letter-like squiggle of
    /// a few turns and a tiny scribble are not.
    #[test]
    fn a_scratch_is_told_from_writing() {
        assert!(is_scratch(&zigzag(0.0, 0.0, 60.0, 6)), "six passes back and forth");
        assert!(!is_scratch(&zigzag(0.0, 0.0, 60.0, 3)), "a W is not a scratch");
        assert!(!is_scratch(&zigzag(0.0, 0.0, 60.0, 4)), "four passes are not five");
        assert!(is_scratch(&zigzag(0.0, 0.0, 60.0, 5)), "five are");
        assert!(!is_scratch(&stroke(&(0..20).map(|i| (i as f32 * 5.0, 0.0)).collect::<Vec<_>>())), "a line");
        let circle: Vec<(f32, f32)> = (0..40).map(|i| (i as f32 * 0.157).sin_cos()).map(|(s, c)| (30.0 * c, 30.0 * s)).collect();
        assert!(!is_scratch(&stroke(&circle)), "a loop");
        assert!(!is_scratch(&zigzag(0.0, 0.0, 4.0, 8)), "too small to mean it");
    }

    /// The strokes rubbed out are those the scratch crosses and mostly covers; a long line it
    /// clips at one end, and ink elsewhere, are left.
    #[test]
    fn a_scratch_rubs_out_only_the_ink_under_it() {
        let mut d = Drawing::new(400.0, 400.0);
        d.add(stroke(&[(10.0, 6.0), (50.0, 6.0)])); // under it
        d.add(stroke(&[(55.0, 4.0), (300.0, 4.0)])); // a long line it clips
        d.add(stroke(&[(10.0, 200.0), (50.0, 200.0)])); // elsewhere
        let scratch = zigzag(0.0, 0.0, 60.0, 6);
        assert_eq!(scratched(&d, &scratch), vec![0]);
        // Readings packed close under it and one far away: judged by length, it is mostly out.
        let mut d2 = Drawing::new(400.0, 400.0);
        let mut points: Vec<(f32, f32)> = (0..=80).map(|i| (10.0 + i as f32 * 0.5, 6.0)).collect();
        points.push((150.0, 6.0));
        d2.add(stroke(&points));
        assert!(scratched(&d2, &scratch).is_empty(), "most of its length is outside");
        // A dot pressed in one place, its pressure changing: two readings, no length.
        let mut d3 = Drawing::new(400.0, 400.0);
        let mut dot = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        dot.dabs = vec![Dab::new(30.0, 6.0, 0.2), Dab::new(30.0, 6.0, 0.9)];
        d3.add(dot);
        assert_eq!(scratched(&d3, &scratch), vec![0], "a dot under it goes");
        let empty = Drawing::new(400.0, 400.0);
        assert!(scratched(&empty, &scratch).is_empty(), "over nothing it stays a scribble");
    }
}

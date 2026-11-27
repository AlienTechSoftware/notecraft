//! The scratch-out gesture (issue #569): a quick zigzag back and forth over ink rubs that ink out,
//! as OneNote's does, when it is turned on.
//!
//! It is told from writing by its shape and by what it is over, and only when it is plainly both:
//! many passes back and forth along one line, the pen's path several times as long as the ground
//! it covers, and existing strokes lying mostly under it. A scribble that is not clearly that is
//! ink like any other, and is kept.

use crate::{Drawing, Stroke, shape};

/// The fewest turns back a scratch makes along its line: five passes.
const MIN_TURNS: usize = 4;

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
    // Turns back along the longer side, counting only runs that go a good part of the way, so a
    // tremor is no turn.
    let reach = extent * 0.4;
    let coord = |i: usize| if along_x { dabs[i].x } else { dabs[i].y };
    let (mut turns, mut direction, mut anchor) = (0usize, 0i8, coord(0));
    let mut run = 0.0_f32;
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
        if direction == 0 {
            direction = now;
        } else if now != direction {
            // A turn counts once the run before it went far enough.
            if run >= reach {
                turns += 1;
            }
            direction = now;
            anchor = coord(i - 1);
        }
        run = (coord(i) - anchor).abs();
    }
    turns >= MIN_TURNS
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
        .filter(|(_, s)| {
            let points = along(s, 1.0);
            let under = points.iter().filter(|p| inside(**p)).count();
            !points.is_empty() && under * 2 > points.len() && path.iter().any(|(x, y)| shape::touches(s, *x, *y, scratch.width / 2.0 + 1.0))
        })
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
        let empty = Drawing::new(400.0, 400.0);
        assert!(scratched(&empty, &scratch).is_empty(), "over nothing it stays a scribble");
    }
}

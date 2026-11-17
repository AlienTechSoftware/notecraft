//! Draw > Ink Replay (issue #295): the page's ink drawn again, stroke by stroke, in the
//! order it was drawn.
//!
//! The ink file keeps its strokes in the order they were drawn, in both page formats, and
//! that order is all the replay uses. It keeps no clock times: no stroke has ever been saved
//! with one, so older pages and new ones replay the same way, each stroke at an even speed
//! with a short pause between strokes, rather than with invented timings. Replay only paints;
//! it never changes the drawing, the page or undo.

use notecraft_ink::{Drawing, Stroke};

/// How fast the pen moves in a replay at normal speed, in page points a second.
pub const PACE: f32 = 260.0;
/// The pause between two strokes, counted as this many points of drawing.
pub const GAP: f32 = 40.0;
/// The speeds offered.
pub const SPEEDS: [f32; 4] = [0.5, 1.0, 2.0, 4.0];

/// A replay in progress.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Replay {
    /// How far through the replay it is, in points of drawing (pauses included).
    pub at: f32,
    pub playing: bool,
    /// One of [`SPEEDS`].
    pub speed: f32,
}

impl Default for Replay {
    fn default() -> Self {
        Self { at: 0.0, playing: true, speed: 1.0 }
    }
}

/// How long a stroke takes to draw, in points along its readings (at least one).
fn length(stroke: &Stroke) -> f32 {
    stroke.dabs.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum::<f32>().max(1.0)
}

/// Where each stroke starts in the replay, and where the replay ends.
fn starts(drawing: &Drawing) -> (Vec<f32>, f32) {
    let mut at = 0.0;
    let mut out = Vec::with_capacity(drawing.strokes.len());
    for stroke in &drawing.strokes {
        out.push(at);
        at += length(stroke) + GAP;
    }
    (out, at)
}

/// What the replay shows at `at`: how many strokes are drawn whole, and the part of the next
/// one drawn so far.
pub fn shown(drawing: &Drawing, at: f32) -> (usize, Option<Stroke>) {
    let (starts, _) = starts(drawing);
    for (i, (stroke, start)) in drawing.strokes.iter().zip(&starts).enumerate() {
        let into = at - start;
        if into < length(stroke) {
            if into <= 0.0 {
                return (i, None);
            }
            let mut part = Stroke { dabs: Vec::new(), ..stroke.clone() };
            let mut gone = 0.0;
            for (k, dab) in stroke.dabs.iter().enumerate() {
                if k > 0 {
                    let prev = stroke.dabs[k - 1];
                    gone += (dab.x - prev.x).hypot(dab.y - prev.y);
                }
                if gone > into {
                    break;
                }
                part.dabs.push(*dab);
            }
            return (i, Some(part));
        }
    }
    (drawing.strokes.len(), None)
}

impl Replay {
    /// Moves the replay on by `seconds`, when it is playing. Stops at the end.
    pub fn tick(&mut self, drawing: &Drawing, seconds: f32) {
        if !self.playing {
            return;
        }
        let (_, end) = starts(drawing);
        self.at = (self.at + seconds.max(0.0) * PACE * self.speed).min(end);
        if self.at >= end {
            self.playing = false;
        }
    }

    /// Whether the replay has drawn everything.
    pub fn finished(&self, drawing: &Drawing) -> bool {
        self.at >= starts(drawing).1
    }

    /// Back to the start, playing.
    pub fn restart(&mut self) {
        self.at = 0.0;
        self.playing = true;
    }

    /// Play from where it is, or from the start once it has finished; or pause.
    pub fn toggle(&mut self, drawing: &Drawing) {
        if self.playing {
            self.playing = false;
        } else {
            if self.finished(drawing) {
                self.at = 0.0;
            }
            self.playing = true;
        }
    }

    /// To the end of the next stroke not yet drawn whole, paused there.
    pub fn next(&mut self, drawing: &Drawing) {
        let (whole, _) = shown(drawing, self.at);
        let (starts, end) = starts(drawing);
        self.at = starts.get(whole + 1).map_or(end, |s| s - GAP / 2.0);
        self.playing = false;
    }

    /// Back to the end of the stroke before the last one drawn, paused there.
    pub fn previous(&mut self, drawing: &Drawing) {
        let (whole, part) = shown(drawing, self.at);
        let (starts, _) = starts(drawing);
        let target = if part.is_some() { whole } else { whole.saturating_sub(1) };
        self.at = if target == 0 { 0.0 } else { starts.get(target).map_or(0.0, |s| s - GAP / 2.0) };
        self.playing = false;
    }

    /// The next speed in [`SPEEDS`], round to the first.
    pub fn faster(&mut self) {
        let i = SPEEDS.iter().position(|s| (s - self.speed).abs() < 1e-3).unwrap_or(1);
        self.speed = SPEEDS[(i + 1) % SPEEDS.len()];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_ink::{Dab, Rgb, Tool};

    fn line(x: f32, len: f32) -> Stroke {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 2.0);
        for k in 0..=10 {
            s.dabs.push(Dab::new(x, k as f32 * len / 10.0, 0.5));
        }
        s
    }

    fn drawing() -> Drawing {
        let mut d = Drawing::new(800.0, 600.0);
        d.strokes = vec![line(10.0, 100.0), line(20.0, 50.0), line(30.0, 200.0)];
        d
    }

    #[test]
    fn strokes_come_back_in_the_order_they_were_drawn() {
        let d = drawing();
        assert_eq!(shown(&d, 0.0).0, 0);
        let (whole, part) = shown(&d, 50.0);
        assert_eq!(whole, 0);
        let part = part.expect("half of the first stroke");
        assert_eq!(part.dabs.len(), 6, "up to halfway along it");
        assert_eq!(shown(&d, 100.0 + GAP + 1.0).0, 1, "the first is whole, the second has begun");
        assert_eq!(shown(&d, 1.0e6), (3, None));
    }

    #[test]
    fn playing_pausing_stepping_and_speed() {
        let d = drawing();
        let mut r = Replay::default();
        r.tick(&d, 0.1);
        assert!((r.at - PACE * 0.1).abs() < 1e-3);
        r.toggle(&d);
        assert!(!r.playing);
        let at = r.at;
        r.tick(&d, 1.0);
        assert_eq!(r.at, at, "paused does not move");
        r.next(&d);
        assert_eq!(shown(&d, r.at).0, 1, "next shows the first stroke whole");
        r.next(&d);
        assert_eq!(shown(&d, r.at).0, 2);
        r.previous(&d);
        assert_eq!(shown(&d, r.at).0, 1);
        r.previous(&d);
        assert_eq!(r.at, 0.0);
        r.faster();
        assert_eq!(r.speed, 2.0);
        r.playing = true;
        r.tick(&d, 100.0);
        assert!(r.finished(&d) && !r.playing, "it stops at the end");
        r.toggle(&d);
        assert_eq!((r.at, r.playing), (0.0, true), "play after the end starts again");
    }

    #[test]
    fn an_empty_drawing_replays_as_nothing() {
        let d = Drawing::new(10.0, 10.0);
        let mut r = Replay::default();
        r.tick(&d, 1.0);
        assert!(r.finished(&d));
        assert_eq!(shown(&d, 0.0), (0, None));
    }
}

//! The page keeps moving for a moment after a swipe on a touchpad, as it does in a browser.
//!
//! A touchpad reports how far the fingers moved and, when they leave the pad, that the swipe has
//! ended. Without anything more the page stops dead the moment the fingers lift, so getting across
//! a page takes swipe after swipe. A fling carries on at the speed the fingers were going and
//! slows to a stop, so a quick flick goes a long way and a slow drag stops where it is let go.

use egui::Vec2;
use std::collections::VecDeque;

/// How far back the finger moves count towards the speed at lift-off, in seconds. A swipe that
/// stopped before the fingers lifted does not fling.
const WINDOW: f64 = 0.1;
/// How quickly a fling slows: its speed falls by `e` every `1 / FRICTION` seconds, so it travels
/// `speed / FRICTION` in all.
const FRICTION: f32 = 3.0;
/// Below this speed, in points a second, a swipe does not fling and a fling stops.
const SLOWEST: f32 = 60.0;
/// The fastest a fling starts, in points a second, so a stray huge sample cannot throw the page
/// off into the distance.
const FASTEST: f32 = 9000.0;

/// The swipe being made and the fling it left behind.
#[derive(Debug, Default)]
pub struct Fling {
    /// The finger moves of the swipe, with when each came: the newest last.
    moves: VecDeque<(f64, Vec2)>,
    /// How fast the page is carrying on, in points a second.
    velocity: Vec2,
    /// When the fling last moved the page.
    last: Option<f64>,
}

impl Fling {
    /// The fingers moved the page by `delta` at `now`. A fling still going stops: the fingers have
    /// the page again.
    pub fn finger(&mut self, now: f64, delta: Vec2) {
        self.stop();
        self.moves.push_back((now, delta));
        while self.moves.front().is_some_and(|(t, _)| now - t > WINDOW) {
            self.moves.pop_front();
        }
    }

    /// The fingers left the pad at `now`: the page carries on at the speed they were going.
    pub fn lift(&mut self, now: f64) {
        let moves: Vec<(f64, Vec2)> = self.moves.drain(..).filter(|(t, _)| now - t <= WINDOW).collect();
        let (Some(first), Some(last)) = (moves.first(), moves.last()) else { return };
        // The time the moves took: from the first to the last, and one more frame for the first.
        let frame = if moves.len() > 1 { (last.0 - first.0) / (moves.len() - 1) as f64 } else { 1.0 / 60.0 };
        #[allow(clippy::cast_possible_truncation)]
        let span = ((last.0 - first.0) + frame).max(1.0 / 240.0) as f32;
        let travelled = moves.iter().fold(Vec2::ZERO, |sum, (_, d)| sum + *d);
        let velocity = travelled / span;
        if velocity.length() < SLOWEST {
            return;
        }
        self.velocity = velocity * (velocity.length().min(FASTEST) / velocity.length());
        self.last = Some(now);
    }

    /// Stops the page where it is: a click, a pinch, or a new swipe.
    pub fn stop(&mut self) {
        self.velocity = Vec2::ZERO;
        self.last = None;
    }

    /// Whether the page is still carrying on.
    pub fn moving(&self) -> bool {
        self.last.is_some()
    }

    /// How far the page carries on between the last frame and `now`.
    pub fn step(&mut self, now: f64) -> Vec2 {
        let Some(last) = self.last else { return Vec2::ZERO };
        #[allow(clippy::cast_possible_truncation)]
        let dt = ((now - last).max(0.0) as f32).min(0.1);
        self.last = Some(now);
        // The speed falls off smoothly; this is the distance it covers while it does.
        let fade = (-FRICTION * dt).exp();
        let moved = self.velocity * (1.0 - fade) / FRICTION;
        self.velocity *= fade;
        if self.velocity.length() < SLOWEST {
            self.stop();
        }
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Swipes `per_frame` points a frame for `frames` frames at 60 a second, then lifts.
    fn swipe(f: &mut Fling, per_frame: Vec2, frames: usize) -> f64 {
        let mut now = 0.0;
        for _ in 0..frames {
            now += 1.0 / 60.0;
            f.finger(now, per_frame);
        }
        now += 1.0 / 60.0;
        f.lift(now);
        now
    }

    /// Runs the fling to its end, and says how far it went.
    fn coast(f: &mut Fling, mut now: f64) -> Vec2 {
        let mut total = Vec2::ZERO;
        for _ in 0..600 {
            now += 1.0 / 60.0;
            total += f.step(now);
        }
        total
    }

    #[test]
    fn a_quick_swipe_carries_on_the_way_it_went() {
        let mut f = Fling::default();
        let now = swipe(&mut f, egui::vec2(0.0, -20.0), 8);
        assert!(f.moving());
        let went = coast(&mut f, now);
        // 1200 points a second, slowing by FRICTION: about 400 points further.
        assert!(went.y < -300.0 && went.y > -450.0, "it went {went:?}");
        assert!(went.x.abs() < 0.01);
        assert!(!f.moving(), "and it stops");
    }

    #[test]
    fn a_slow_drag_stops_where_it_is_let_go() {
        let mut f = Fling::default();
        swipe(&mut f, egui::vec2(0.5, 0.0), 8);
        assert!(!f.moving());
    }

    #[test]
    fn a_swipe_that_paused_before_lifting_does_not_fling() {
        let mut f = Fling::default();
        let now = swipe(&mut f, egui::vec2(20.0, 0.0), 0);
        for i in 0..8 {
            f.finger(now + f64::from(i) / 60.0, egui::vec2(20.0, 0.0));
        }
        f.lift(now + 0.5);
        assert!(!f.moving());
    }

    #[test]
    fn touching_again_stops_a_fling() {
        let mut f = Fling::default();
        let now = swipe(&mut f, egui::vec2(30.0, 0.0), 6);
        assert!(f.step(now + 1.0 / 60.0).x > 0.0);
        f.finger(now + 2.0 / 60.0, Vec2::ZERO);
        assert!(!f.moving());
        assert_eq!(f.step(now + 3.0 / 60.0), Vec2::ZERO);
    }
}

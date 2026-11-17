//! Draw > Ruler (issue #293): a straightedge laid on the page.
//!
//! The ruler is something the user holds, not page content: it is never written to a file.
//! A stroke begun just outside one of its long edges is laid along that edge, so a line
//! drawn against it comes out straight, and the ink is ordinary ink after that. All of it is
//! arithmetic in page points, tested here without a window.

/// How long the ruler is, in points (about 20 cm).
pub const LENGTH: f32 = 567.0;
/// How wide it is, in points.
pub const WIDTH: f32 = 46.0;
/// How far outside an edge a pen may land and still be drawn along it, in points.
pub const REACH: f32 = 14.0;
/// The round handle that turns the ruler sits this far in from its right-hand end.
pub const HANDLE_IN: f32 = 18.0;
/// The handle's radius, in points.
pub const HANDLE_R: f32 = 10.0;

/// Which long edge a stroke is drawn along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// The edge above the middle when the ruler lies flat.
    Upper,
    /// The edge below it.
    Lower,
}

/// Where the ruler lies: its middle, in page points, and how far it is turned, in degrees
/// clockwise on screen (y grows down the page).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ruler {
    pub x: f32,
    pub y: f32,
    pub angle: f32,
}

impl Ruler {
    /// A ruler lying flat with its middle at (`x`, `y`).
    pub fn at(x: f32, y: f32) -> Self {
        Self { x, y, angle: 0.0 }
    }

    fn axis(&self) -> (f32, f32) {
        let a = self.angle.to_radians();
        (a.cos(), a.sin())
    }

    /// A page point in the ruler's own terms: how far along it from the middle, and how
    /// far across (negative towards the upper edge).
    pub fn to_local(&self, x: f32, y: f32) -> (f32, f32) {
        let (c, s) = self.axis();
        let (dx, dy) = (x - self.x, y - self.y);
        (dx * c + dy * s, -dx * s + dy * c)
    }

    /// Back from the ruler's terms to the page.
    pub fn to_page(&self, along: f32, across: f32) -> (f32, f32) {
        let (c, s) = self.axis();
        (self.x + along * c - across * s, self.y + along * s + across * c)
    }

    /// The ruler's four corners on the page, clockwise from the upper left.
    pub fn corners(&self) -> [(f32, f32); 4] {
        let (h, w) = (LENGTH / 2.0, WIDTH / 2.0);
        [self.to_page(-h, -w), self.to_page(h, -w), self.to_page(h, w), self.to_page(-h, w)]
    }

    /// Whether the point is on the ruler, where a press holds it rather than drawing.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (along, across) = self.to_local(x, y);
        along.abs() <= LENGTH / 2.0 && across.abs() <= WIDTH / 2.0
    }

    /// The middle of the round handle that turns the ruler.
    pub fn handle(&self) -> (f32, f32) {
        self.to_page(LENGTH / 2.0 - HANDLE_IN, 0.0)
    }

    /// Whether the point is on the turning handle.
    pub fn on_handle(&self, x: f32, y: f32) -> bool {
        let (hx, hy) = self.handle();
        (x - hx).hypot(y - hy) <= HANDLE_R + 2.0
    }

    /// The edge a pen landing here draws along, if it landed close enough outside one.
    /// `reach` is in page points (smaller when the page is zoomed in, so it stays the same
    /// on screen).
    pub fn edge_near(&self, x: f32, y: f32, reach: f32) -> Option<Edge> {
        let (along, across) = self.to_local(x, y);
        if along.abs() > LENGTH / 2.0 {
            return None;
        }
        let w = WIDTH / 2.0;
        if (-w - reach..-w).contains(&across) {
            Some(Edge::Upper)
        } else if across > w && across <= w + reach {
            Some(Edge::Lower)
        } else {
            None
        }
    }

    /// Where a reading is drawn when the pen runs along `edge`: on the line half the pen's
    /// `width` outside the edge, so the ink touches the ruler and does not run under it, and
    /// no further along than the ruler is long.
    pub fn along(&self, edge: Edge, x: f32, y: f32, width: f32) -> (f32, f32) {
        let (along, _) = self.to_local(x, y);
        let off = WIDTH / 2.0 + width.max(0.0) / 2.0;
        let across = match edge {
            Edge::Upper => -off,
            Edge::Lower => off,
        };
        self.to_page(along.clamp(-LENGTH / 2.0, LENGTH / 2.0), across)
    }

    /// Turns the ruler by `degrees` (clockwise on screen), kept within a half turn either way.
    pub fn turn(&mut self, degrees: f32) {
        self.angle = normal(self.angle + degrees);
    }

    /// Turns the ruler about its middle so that its handle points at (`x`, `y`).
    pub fn point_at(&mut self, x: f32, y: f32) {
        let (dx, dy) = (x - self.x, y - self.y);
        if dx.hypot(dy) > 1.0 {
            self.angle = normal(dy.atan2(dx).to_degrees());
        }
    }

    /// The angle shown to the user: whole degrees from 0 to 179, as a protractor reads a
    /// line (a ruler turned half round lies along the same line).
    pub fn shown_degrees(&self) -> u16 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let d = (-self.angle).rem_euclid(180.0).round() as u16;
        d % 180
    }
}

fn normal(degrees: f32) -> f32 {
    let d = degrees.rem_euclid(360.0);
    if d > 180.0 { d - 360.0 } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn page_and_ruler_terms_agree_at_any_angle() {
        for angle in [0.0, 30.0, -45.0, 90.0, 179.0] {
            let r = Ruler { x: 200.0, y: 300.0, angle };
            for (x, y) in [(200.0, 300.0), (10.0, 20.0), (450.0, 280.0)] {
                let (a, c) = r.to_local(x, y);
                assert!(close(r.to_page(a, c), (x, y)), "angle {angle}");
            }
        }
    }

    #[test]
    fn a_press_on_the_ruler_holds_it_and_one_just_outside_an_edge_draws_along_it() {
        let r = Ruler::at(300.0, 200.0);
        assert!(r.contains(300.0, 200.0));
        assert!(r.contains(300.0 - LENGTH / 2.0 + 1.0, 200.0 + WIDTH / 2.0 - 1.0));
        assert!(!r.contains(300.0, 200.0 - WIDTH / 2.0 - 1.0));
        assert_eq!(r.edge_near(300.0, 200.0 - WIDTH / 2.0 - 4.0, REACH), Some(Edge::Upper));
        assert_eq!(r.edge_near(300.0, 200.0 + WIDTH / 2.0 + 4.0, REACH), Some(Edge::Lower));
        assert_eq!(r.edge_near(300.0, 200.0 - WIDTH / 2.0 - REACH - 1.0, REACH), None, "too far off to mean the ruler");
        assert_eq!(r.edge_near(300.0, 200.0, REACH), None, "on the ruler is holding it, not drawing");
        assert_eq!(r.edge_near(300.0 + LENGTH, 200.0 - WIDTH / 2.0 - 4.0, REACH), None, "past its end");
        assert!(r.on_handle(r.handle().0, r.handle().1));
    }

    #[test]
    fn a_shaky_hand_along_a_turned_ruler_draws_a_straight_line() {
        let r = Ruler { x: 100.0, y: 100.0, angle: 30.0 };
        let width = 4.0;
        let wobbly: Vec<(f32, f32)> =
            (0..20).map(|i| r.to_page(-100.0 + i as f32 * 10.0, -WIDTH / 2.0 - 3.0 + if i % 2 == 0 { 2.0 } else { -1.5 })).collect();
        let drawn: Vec<(f32, f32)> = wobbly.iter().map(|&(x, y)| r.along(Edge::Upper, x, y, width)).collect();
        for &(x, y) in &drawn {
            let (_, across) = r.to_local(x, y);
            assert!((across + WIDTH / 2.0 + width / 2.0).abs() < 1e-3, "every point is on the one line beside the edge");
        }
        // And it follows the pen along the ruler.
        let first = r.to_local(drawn[0].0, drawn[0].1).0;
        let last = r.to_local(drawn[19].0, drawn[19].1).0;
        assert!((first + 100.0).abs() < 1e-3 && (last - 90.0).abs() < 1e-3);
        // No further than the ruler is long.
        let past = r.along(Edge::Lower, r.to_page(LENGTH, 0.0).0, r.to_page(LENGTH, 0.0).1, width);
        assert!((r.to_local(past.0, past.1).0 - LENGTH / 2.0).abs() < 1e-3);
    }

    #[test]
    fn turning_shows_the_angle_a_protractor_would() {
        let mut r = Ruler::at(0.0, 0.0);
        assert_eq!(r.shown_degrees(), 0);
        r.turn(-30.0);
        assert_eq!(r.shown_degrees(), 30, "turned anticlockwise on screen reads 30");
        r.turn(-180.0);
        assert_eq!(r.shown_degrees(), 30, "half round lies on the same line");
        r.turn(400.0);
        assert!(r.angle > -180.0 && r.angle <= 180.0);
        r.point_at(0.0, -50.0);
        assert_eq!(r.shown_degrees(), 90);
        r.point_at(50.0, 0.0);
        assert_eq!(r.shown_degrees(), 0);
    }
}

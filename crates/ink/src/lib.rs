//! Ink: what a pen leaves on a page.
//!
//! A [`Stroke`] is the readings of one press of the pen to its lift: where the tip was and
//! how hard it was pressed at each reading. A [`Drawing`] is a sheet of them. Nothing here
//! draws anything or knows about a window; the shape of a stroke is handed out as a polygon
//! ([`shape`]) that any painter can fill, and as the SVG it is stored as ([`svg`]).
//!
//! ## Why strokes are stored twice over
//!
//! SVG has no variable-width stroke. The W3C SVG working group has had the request open
//! since 2009 and the answer in every editor is the same: make a filled outline of the
//! ribbon and draw that. So a saved drawing holds the outline, which any browser or editor
//! renders correctly, with no idea it was ever a pen.
//!
//! But an outline is a dead end. Once a stroke is only its outline it cannot be re-smoothed,
//! re-weighted, or partly rubbed out, because the pen's own readings are gone. So the
//! readings are kept too, in our own XML namespace on the same path element, where other
//! programs ignore them and we can read the stroke back exactly. The owner asked for both
//! and both is what a file carries: [`svg::Flavour::Editable`] writes the readings beside
//! the outline, [`svg::Flavour::Plain`] leaves them out for a drawing that is being sent
//! somewhere else.
//!
//! This is the shape of OneNote's own ink too: its exported ink is InkML, where pressure
//! travels on a channel of its own beside the coordinates. We are not reading its files
//! here; the resemblance is in the idea, which is older than either of us.

#![doc(test(attr(deny(warnings))))]

pub mod shape;
pub mod svg;

/// One reading of the pen: where the tip was, and how hard it was pressed there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dab {
    pub x: f32,
    pub y: f32,
    /// 0 to 1. A mouse, and a pen on a platform that reports no pressure, is 1 throughout.
    pub pressure: f32,
}

impl Dab {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self { x, y, pressure }.sane()
    }

    /// Replaces anything that is not a finite number, and puts pressure back in range. Ink
    /// arrives from a device driver and from files, so nothing here trusts its input.
    pub fn sane(self) -> Self {
        let n = |v: f32| if v.is_finite() { v.clamp(-1.0e6, 1.0e6) } else { 0.0 };
        Self { x: n(self.x), y: n(self.y), pressure: if self.pressure.is_finite() { self.pressure.clamp(0.0, 1.0) } else { 1.0 } }
    }

    fn distance_to(self, other: Dab) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// What was drawing. The nibs OneNote offers under its Add button, and what each one
/// means here. They differ in two things only: how see-through the ink is, and how much of
/// the width the lightest touch still lays down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    /// Opaque, and the width follows the pressure.
    #[default]
    Pen,
    /// See-through and flat: a highlighter does not thin when pressed lightly, and it is
    /// drawn under the words rather than over them.
    Highlighter,
    /// Opaque and thin, with less of a swell than the pen.
    Pencil,
    /// A split nib: it answers pressure more than a pen does, so a light hand gives a
    /// hairline and a firm one a broad stroke.
    FountainPen,
    /// A loaded brush: the widest range of all, nearly nothing at a touch.
    BrushPen,
}

impl Tool {
    /// Every nib, in the order the Draw tab offers them.
    pub const ALL: [Tool; 5] = [Tool::Pen, Tool::Pencil, Tool::FountainPen, Tool::BrushPen, Tool::Highlighter];

    pub fn id(self) -> &'static str {
        match self {
            Tool::Pen => "pen",
            Tool::Highlighter => "highlighter",
            Tool::Pencil => "pencil",
            Tool::FountainPen => "fountain-pen",
            Tool::BrushPen => "brush-pen",
        }
    }

    pub fn parse(id: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.id() == id.trim())
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pen => "Pen",
            Tool::Highlighter => "Highlighter",
            Tool::Pencil => "Pencil",
            Tool::FountainPen => "Fountain Pen",
            Tool::BrushPen => "Brush Pen",
        }
    }

    /// How see-through the ink is, 0 to 1.
    pub fn opacity(self) -> f32 {
        match self {
            Tool::Pen | Tool::Pencil | Tool::FountainPen | Tool::BrushPen => 1.0,
            Tool::Highlighter => 0.4,
        }
    }

    /// How much of the width the lightest touch still lays down. A highlighter is a flat
    /// chisel and draws its whole width however it is held; a brush, at the other end,
    /// leaves almost nothing until it is pressed.
    pub fn floor(self) -> f32 {
        match self {
            Tool::Pen => 0.25,
            Tool::Pencil => 0.55,
            Tool::Highlighter => 1.0,
            Tool::FountainPen => 0.15,
            Tool::BrushPen => 0.08,
        }
    }
}

/// A colour as it is written in a file: red, green, blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0x20, 0x20, 0x20);

    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Reads `#rrggbb` or `#rgb`. Anything else is `None`.
    pub fn parse(text: &str) -> Option<Rgb> {
        let digits = text.trim().strip_prefix('#')?;
        let byte = |s: &str| u8::from_str_radix(s, 16).ok();
        match digits.len() {
            6 => Some(Rgb(byte(digits.get(0..2)?)?, byte(digits.get(2..4)?)?, byte(digits.get(4..6)?)?)),
            3 => {
                let double = |s: &str| byte(s).map(|v| v * 17);
                Some(Rgb(double(digits.get(0..1)?)?, double(digits.get(1..2)?)?, double(digits.get(2..3)?)?))
            }
            _ => None,
        }
    }
}

/// One press of the pen to its lift.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub tool: Tool,
    pub colour: Rgb,
    /// The width in points at full pressure. The pressure of each reading scales it.
    pub width: f32,
    pub dabs: Vec<Dab>,
}

impl Default for Stroke {
    fn default() -> Self {
        Self { tool: Tool::Pen, colour: Rgb::BLACK, width: 3.0, dabs: Vec::new() }
    }
}

/// The smallest and largest a stroke may be asked to be, in points. A stroke outside this
/// is a file that has been meddled with, or a slider that got away.
pub const MIN_WIDTH: f32 = 0.4;
pub const MAX_WIDTH: f32 = 80.0;

/// No stroke may hold more readings than this. A pen reports about 200 a second, so this is
/// several minutes of one unbroken line, and it stops a damaged file eating the window.
pub const MAX_DABS: usize = 20_000;

/// Nor may a drawing hold more strokes than this.
pub const MAX_STROKES: usize = 20_000;

impl Stroke {
    pub fn new(tool: Tool, colour: Rgb, width: f32) -> Self {
        Self { tool, colour, width: Self::sane_width(width), dabs: Vec::new() }
    }

    fn sane_width(width: f32) -> f32 {
        if width.is_finite() { width.clamp(MIN_WIDTH, MAX_WIDTH) } else { 3.0 }
    }

    /// Adds a reading, unless it is so close to the last one that it would only add noise.
    /// Returns whether it was kept.
    pub fn add(&mut self, dab: Dab) -> bool {
        let dab = dab.sane();
        if self.dabs.len() >= MAX_DABS {
            return false;
        }
        // A hair's breadth apart is the same place, but a change in pressure is worth
        // keeping even when the pen has not moved: that is a press, and it shows.
        if let Some(last) = self.dabs.last() {
            let still = last.distance_to(dab) < 0.35;
            let same_press = (last.pressure - dab.pressure).abs() < 0.02;
            if still && same_press {
                return false;
            }
        }
        self.dabs.push(dab);
        true
    }

    /// Puts a stroke read from a file back in its senses: a sane width, no silly readings,
    /// and not more of them than anything will draw.
    pub fn sane(mut self) -> Self {
        self.width = Self::sane_width(self.width);
        self.dabs.truncate(MAX_DABS);
        for d in &mut self.dabs {
            *d = d.sane();
        }
        self
    }

    pub fn is_empty(&self) -> bool {
        self.dabs.is_empty()
    }

    /// The box the ink covers, with the width of the stroke taken into account.
    /// `None` for a stroke with no readings.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let first = self.dabs.first()?;
        let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
        for d in &self.dabs {
            x0 = x0.min(d.x);
            y0 = y0.min(d.y);
            x1 = x1.max(d.x);
            y1 = y1.max(d.y);
        }
        let edge = self.width / 2.0;
        Some((x0 - edge, y0 - edge, x1 + edge, y1 + edge))
    }
}

/// A sheet of ink: the strokes on it, in the order they were drawn.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// How big the sheet is, in points. The strokes are placed inside it.
    pub width: f32,
    pub height: f32,
    pub strokes: Vec<Stroke>,
}

impl Drawing {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width: width.max(1.0), height: height.max(1.0), strokes: Vec::new() }
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.iter().all(Stroke::is_empty)
    }

    /// Adds a stroke, unless it has nothing in it or the sheet is already full.
    pub fn add(&mut self, stroke: Stroke) -> bool {
        if stroke.is_empty() || self.strokes.len() >= MAX_STROKES {
            return false;
        }
        self.strokes.push(stroke.sane());
        true
    }

    /// Takes the last stroke off, as Undo does. Returns whether there was one.
    pub fn undo(&mut self) -> bool {
        self.strokes.pop().is_some()
    }

    /// Rubs out every stroke the eraser passes over, and says how many went. `radius` is
    /// how wide the eraser is.
    pub fn erase_at(&mut self, x: f32, y: f32, radius: f32) -> usize {
        let before = self.strokes.len();
        self.strokes.retain(|s| !shape::touches(s, x, y, radius));
        before - self.strokes.len()
    }

    /// The box all the ink covers, which may be larger than the sheet if a stroke ran over
    /// the edge. `None` for a drawing with no ink in it.
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        self.strokes.iter().filter_map(Stroke::bounds).reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_goes_to_a_file_and_comes_back() {
        assert_eq!(Rgb(0x20, 0x20, 0x20).hex(), "#202020");
        assert_eq!(Rgb::parse("#202020"), Some(Rgb(0x20, 0x20, 0x20)));
        assert_eq!(Rgb::parse("#f0a"), Some(Rgb(0xff, 0x00, 0xaa)), "the short form doubles each digit");
        for junk in ["", "#", "202020", "#2020", "#gggggg", "#20202020", "  "] {
            assert_eq!(Rgb::parse(junk), None, "{junk:?} is not a colour");
        }
        // Every colour survives the round trip.
        for c in [Rgb(0, 0, 0), Rgb(255, 255, 255), Rgb(1, 128, 254)] {
            assert_eq!(Rgb::parse(&c.hex()), Some(c));
        }
    }

    #[test]
    fn a_tool_knows_how_it_marks_the_page() {
        for t in Tool::ALL {
            assert_eq!(Tool::parse(t.id()), Some(t), "{} round trips", t.label());
            assert!((0.0..=1.0).contains(&t.opacity()));
            assert!((0.0..=1.0).contains(&t.floor()));
        }
        assert_eq!(Tool::parse("crayon"), None);
        assert_eq!(Tool::ALL.len(), 5, "every nib is offered");
        // The nibs are ordered by how much they answer the pen: a brush most, a
        // highlighter not at all.
        assert!(Tool::BrushPen.floor() < Tool::FountainPen.floor());
        assert!(Tool::FountainPen.floor() < Tool::Pen.floor());
        assert!(Tool::Pen.floor() < Tool::Pencil.floor());
        assert!(Tool::Pencil.floor() < Tool::Highlighter.floor());
        assert!(Tool::Highlighter.opacity() < Tool::Pen.opacity(), "a highlighter is see-through");
        assert_eq!(Tool::Highlighter.floor(), 1.0, "and it is a flat chisel, so pressure does not thin it");
    }

    #[test]
    fn readings_that_say_nothing_new_are_dropped() {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 3.0);
        assert!(s.add(Dab::new(10.0, 10.0, 0.5)));
        assert!(!s.add(Dab::new(10.1, 10.0, 0.5)), "the pen has not moved and is not pressing differently");
        assert!(s.add(Dab::new(10.1, 10.0, 0.9)), "but a change of pressure in one place is a press");
        assert!(s.add(Dab::new(30.0, 10.0, 0.9)), "and so is moving");
        assert_eq!(s.dabs.len(), 3);
    }

    #[test]
    fn nothing_mad_gets_into_a_stroke() {
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, f32::NAN);
        assert_eq!(s.width, 3.0, "a width that is not a number falls back");
        assert_eq!(Stroke::new(Tool::Pen, Rgb::BLACK, 1.0e9).width, MAX_WIDTH);
        assert_eq!(Stroke::new(Tool::Pen, Rgb::BLACK, 0.0).width, MIN_WIDTH);
        s.add(Dab::new(f32::NAN, f32::INFINITY, f32::NAN));
        let d = s.dabs.first().copied().unwrap_or(Dab::new(9.0, 9.0, 9.0));
        assert_eq!((d.x, d.y, d.pressure), (0.0, 0.0, 1.0), "a junk reading is replaced, not kept");

        // A stroke cannot grow without end.
        let mut long = Stroke::default();
        for i in 0..(MAX_DABS + 50) {
            long.add(Dab::new(i as f32, 0.0, 1.0));
        }
        assert_eq!(long.dabs.len(), MAX_DABS);
    }

    #[test]
    fn a_drawing_holds_strokes_and_gives_them_up_again() {
        let mut d = Drawing::new(200.0, 100.0);
        assert!(d.is_empty());
        assert!(!d.add(Stroke::default()), "a stroke with no readings is not ink");

        let mut s = Stroke::new(Tool::Pen, Rgb(255, 0, 0), 4.0);
        s.add(Dab::new(10.0, 10.0, 1.0));
        s.add(Dab::new(50.0, 10.0, 1.0));
        assert!(d.add(s));
        assert!(!d.is_empty());
        assert_eq!(d.bounds(), Some((8.0, 8.0, 52.0, 12.0)), "the box takes in the width of the line");

        assert_eq!(d.erase_at(30.0, 10.0, 2.0), 1, "the eraser passed over it");
        assert!(d.is_empty());
        assert!(!d.undo(), "and there is nothing left to take back");
    }

    #[test]
    fn a_sheet_is_never_nothing_wide() {
        let d = Drawing::new(0.0, -5.0);
        assert!(d.width >= 1.0 && d.height >= 1.0);
    }
}

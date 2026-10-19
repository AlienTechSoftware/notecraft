//! The rack of pens: which tools are on it, in what order, and how it is written down.
//!
//! OneNote's Drawing Tools are a rack a person arranges: pens can be added, taken off,
//! moved along, and each one's kind, colour and thickness changed. So the rack is not a
//! fixed list in the code but something the window saves and reads back, and this is the
//! part of it that has nothing to do with drawing a window: what a nib is, what the rack
//! starts as, and how it survives being written to a settings file.

use crate::{Rgb, Tool};

/// One tool on the rack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Nib {
    pub tool: Tool,
    pub colour: Rgb,
    /// Width at full pressure, in tenths of a point, so a nib can be compared and written
    /// down without a floating point number in the way.
    pub tenths: u16,
}

/// The thinnest and thickest a nib may be, in tenths of a point.
pub const MIN_TENTHS: u16 = 4;
pub const MAX_TENTHS: u16 = 400;

/// No rack may be longer than this: it has to fit on a ribbon, and a damaged settings file
/// must not be able to fill one.
pub const MAX_NIBS: usize = 24;

impl Nib {
    pub fn new(tool: Tool, colour: Rgb, tenths: u16) -> Nib {
        Nib { tool, colour, tenths: tenths.clamp(MIN_TENTHS, MAX_TENTHS) }
    }

    /// Width in points, which is what a stroke is drawn with.
    pub fn width(self) -> f32 {
        f32::from(self.tenths) / 10.0
    }

    /// `kind:#rrggbb:tenths`.
    pub fn write(self) -> String {
        format!("{}:{}:{}", self.tool.id(), self.colour.hex(), self.tenths)
    }

    /// Reads one back. Anything it cannot make sense of is `None`, so a damaged line costs
    /// that nib and not the whole rack.
    pub fn parse(text: &str) -> Option<Nib> {
        let mut parts = text.trim().split(':');
        let tool = Tool::parse(parts.next()?)?;
        let colour = Rgb::parse(parts.next()?)?;
        let tenths: u16 = parts.next()?.trim().parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Nib::new(tool, colour, tenths))
    }
}

/// The rack a new notebook starts with: four pens, a pencil, two highlighters, in the
/// order OneNote racks them.
pub fn fresh() -> Vec<Nib> {
    vec![
        Nib::new(Tool::Pen, Rgb(0x20, 0x20, 0x20), 30),
        Nib::new(Tool::Pen, Rgb(0xd0, 0x21, 0x21), 30),
        Nib::new(Tool::Pen, Rgb(0x18, 0x55, 0xc4), 30),
        Nib::new(Tool::Pen, Rgb(0x16, 0x7f, 0x3d), 30),
        Nib::new(Tool::Pencil, Rgb(0x55, 0x55, 0x55), 18),
        Nib::new(Tool::Highlighter, Rgb(0xff, 0xdd, 0x33), 140),
        Nib::new(Tool::Highlighter, Rgb(0x6f, 0xe3, 0x9a), 140),
    ]
}

/// What a newly added tool of each kind looks like before it is changed.
pub fn new_nib(tool: Tool) -> Nib {
    match tool {
        Tool::Highlighter => Nib::new(tool, Rgb(0xff, 0xdd, 0x33), 140),
        Tool::Pencil => Nib::new(tool, Rgb(0x55, 0x55, 0x55), 18),
        Tool::BrushPen => Nib::new(tool, Rgb(0x20, 0x20, 0x20), 60),
        Tool::FountainPen => Nib::new(tool, Rgb(0x1b, 0x2a, 0x6b), 26),
        Tool::Pen => Nib::new(tool, Rgb(0x20, 0x20, 0x20), 30),
    }
}

/// The whole rack as one line, for a settings file.
pub fn write(rack: &[Nib]) -> String {
    rack.iter().map(|n| n.write()).collect::<Vec<_>>().join("|")
}

/// Reads it back. An empty or unreadable line gives the rack a notebook starts with,
/// rather than no pens at all, because a person with no pens cannot draw their way out.
pub fn parse(text: &str) -> Vec<Nib> {
    let nibs: Vec<Nib> = text.split('|').filter_map(Nib::parse).take(MAX_NIBS).collect();
    if nibs.is_empty() { fresh() } else { nibs }
}

/// Moves the nib at `from` one place along, and says where it ended up. A nib at either
/// end stays where it is.
pub fn shift(rack: &mut [Nib], from: usize, left: bool) -> usize {
    if left && from > 0 {
        rack.swap(from, from - 1);
        return from - 1;
    }
    if !left && from + 1 < rack.len() {
        rack.swap(from, from + 1);
        return from + 1;
    }
    from
}

/// Takes the nib at `at` off the rack, unless it is the last one: a rack with no pens on
/// it would leave nothing to draw with and no way back.
pub fn remove(rack: &mut Vec<Nib>, at: usize) -> bool {
    if rack.len() <= 1 || at >= rack.len() {
        return false;
    }
    rack.remove(at);
    true
}

/// The colours the tool editor offers, as OneNote's grid does: four rows of colour and one
/// of greys. Picking from a small, fixed set keeps two people's notes looking alike.
pub const PALETTE: [[Rgb; 6]; 5] = [
    [Rgb(0x20, 0x20, 0x20), Rgb(0x5a, 0x5a, 0x5a), Rgb(0x8c, 0x8c, 0x8c), Rgb(0xbd, 0xbd, 0xbd), Rgb(0xe4, 0xe4, 0xe4), Rgb(0xff, 0xff, 0xff)],
    [Rgb(0x8c, 0x12, 0x12), Rgb(0xd0, 0x21, 0x21), Rgb(0xe8, 0x62, 0x1f), Rgb(0xf2, 0x9d, 0x12), Rgb(0xff, 0xdd, 0x33), Rgb(0xfa, 0xf0, 0x8a)],
    [Rgb(0x0d, 0x4c, 0x27), Rgb(0x16, 0x7f, 0x3d), Rgb(0x3f, 0xb5, 0x50), Rgb(0x6f, 0xe3, 0x9a), Rgb(0x19, 0x8f, 0x8f), Rgb(0x66, 0xd9, 0xd9)],
    [Rgb(0x10, 0x2a, 0x6b), Rgb(0x18, 0x55, 0xc4), Rgb(0x3b, 0x8d, 0xf0), Rgb(0x86, 0xc5, 0xff), Rgb(0x4a, 0x2a, 0x8f), Rgb(0x8b, 0x63, 0xd6)],
    [Rgb(0x6b, 0x1b, 0x4a), Rgb(0xb0, 0x2b, 0x7a), Rgb(0xe8, 0x5a, 0xa8), Rgb(0xf7, 0xa8, 0xcd), Rgb(0x6a, 0x45, 0x2a), Rgb(0xa9, 0x7c, 0x50)],
];

/// The thicknesses the tool editor offers as dots, in tenths of a point, thinnest first.
pub const THICKNESSES: [u16; 6] = [4, 10, 20, 35, 60, 140];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nib_goes_to_a_file_and_comes_back() {
        for nib in fresh() {
            assert_eq!(Nib::parse(&nib.write()), Some(nib), "{}", nib.write());
        }
        assert_eq!(Nib::parse("pen:#202020:30"), Some(Nib::new(Tool::Pen, Rgb(0x20, 0x20, 0x20), 30)));
        for junk in ["", "pen", "pen:#202020", "crayon:#202020:30", "pen:nonsense:30", "pen:#202020:lots", "pen:#202020:30:extra"] {
            assert_eq!(Nib::parse(junk), None, "{junk:?} is not a nib");
        }
    }

    #[test]
    fn the_whole_rack_survives_a_settings_file() {
        let before = fresh();
        let line = write(&before);
        assert_eq!(parse(&line), before);
        assert!(!line.contains('\n'), "it is one line: {line}");

        // A damaged nib costs that nib, not the rack.
        let hurt = parse("pen:#202020:30|nonsense|highlighter:#ffdd33:140");
        assert_eq!(hurt.len(), 2);
        assert_eq!(hurt[1].tool, Tool::Highlighter);

        // Nothing readable at all gives the rack a notebook starts with: a person with no
        // pens cannot draw their way out of having no pens.
        for empty in ["", "   ", "nonsense", "|||"] {
            assert_eq!(parse(empty), fresh(), "{empty:?}");
        }
        // And a file that tries to fill the ribbon is cut short.
        let many = vec!["pen:#202020:30"; MAX_NIBS + 20].join("|");
        assert_eq!(parse(&many).len(), MAX_NIBS);
    }

    #[test]
    fn a_nibs_width_is_kept_in_range() {
        assert_eq!(Nib::new(Tool::Pen, Rgb::BLACK, 0).tenths, MIN_TENTHS);
        assert_eq!(Nib::new(Tool::Pen, Rgb::BLACK, 9999).tenths, MAX_TENTHS);
        assert!((Nib::new(Tool::Pen, Rgb::BLACK, 30).width() - 3.0).abs() < 0.001);
    }

    #[test]
    fn nibs_move_along_the_rack_and_stop_at_the_ends() {
        let mut rack = fresh();
        let red = rack[1];
        assert_eq!(shift(&mut rack, 1, true), 0);
        assert_eq!(rack[0], red, "it moved left");
        assert_eq!(shift(&mut rack, 0, true), 0, "and stops at the end");
        assert_eq!(rack[0], red);
        assert_eq!(shift(&mut rack, 0, false), 1, "back the other way");
        let last = rack.len() - 1;
        assert_eq!(shift(&mut rack, last, false), last, "and stops at that end too");
    }

    #[test]
    fn the_last_pen_cannot_be_taken_off() {
        let mut rack = fresh();
        let was = rack.len();
        assert!(remove(&mut rack, 0));
        assert_eq!(rack.len(), was - 1);
        assert!(!remove(&mut rack, 99), "nor one that is not there");
        while rack.len() > 1 {
            assert!(remove(&mut rack, 0));
        }
        assert!(!remove(&mut rack, 0), "the last one stays, so there is always something to draw with");
        assert_eq!(rack.len(), 1);
    }

    #[test]
    fn every_kind_can_be_added_and_the_palette_is_sound() {
        for tool in Tool::ALL {
            let nib = new_nib(tool);
            assert_eq!(nib.tool, tool);
            assert!(nib.tenths >= MIN_TENTHS && nib.tenths <= MAX_TENTHS);
        }
        // No colour twice, so the grid never offers the same thing in two places.
        let mut seen: Vec<Rgb> = PALETTE.iter().flatten().copied().collect();
        let total = seen.len();
        seen.sort_by_key(|c| (c.0, c.1, c.2));
        seen.dedup();
        assert_eq!(seen.len(), total, "every colour in the grid is its own");
        assert!(THICKNESSES.windows(2).all(|w| w[0] < w[1]), "the dots go thin to thick");
        assert!(THICKNESSES.iter().all(|t| (MIN_TENTHS..=MAX_TENTHS).contains(t)));
    }
}

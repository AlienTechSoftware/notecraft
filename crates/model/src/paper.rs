//! A page's paper (issue #269): the sheet size it is written for, which way the sheet is
//! turned, and its margins, the way OneNote's View > Paper Size sets them.
//!
//! A page with no paper is "Auto": it grows as far as its content does, which is how every
//! page was before. A page given a paper still holds everything it held; the sheets are drawn
//! as boundaries behind the content, and nothing beyond them is clipped or moved.
//!
//! In the page file the paper is one line, the same in both formats and on every platform:
//! the size, the turn, then the margins in millimetres, top, right, bottom and left, written
//! with a full stop for the decimal point. `a4 portrait 20,20,20,20`, or for a size of one's
//! own `custom:200x300 landscape 10,10,10,10`.

/// The sheet sizes on offer, each with its width and height in millimetres, portrait.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Size {
    Letter,
    Legal,
    Tabloid,
    Executive,
    A3,
    A4,
    A5,
    A6,
    B4,
    B5,
    Index3x5,
    Index4x6,
    Index5x8,
    /// A size of one's own, in millimetres, as given (portrait or not).
    Custom(f32, f32),
}

impl Size {
    /// The standard sizes, in the order the menu lists them.
    pub const STANDARD: [Size; 13] = [
        Size::Letter,
        Size::Legal,
        Size::Tabloid,
        Size::Executive,
        Size::A3,
        Size::A4,
        Size::A5,
        Size::A6,
        Size::B4,
        Size::B5,
        Size::Index3x5,
        Size::Index4x6,
        Size::Index5x8,
    ];

    pub fn id(self) -> String {
        match self {
            Size::Letter => "letter".into(),
            Size::Legal => "legal".into(),
            Size::Tabloid => "tabloid".into(),
            Size::Executive => "executive".into(),
            Size::A3 => "a3".into(),
            Size::A4 => "a4".into(),
            Size::A5 => "a5".into(),
            Size::A6 => "a6".into(),
            Size::B4 => "b4".into(),
            Size::B5 => "b5".into(),
            Size::Index3x5 => "index3x5".into(),
            Size::Index4x6 => "index4x6".into(),
            Size::Index5x8 => "index5x8".into(),
            Size::Custom(w, h) => format!("custom:{}x{}", number(w), number(h)),
        }
    }

    /// What the menu calls it.
    pub fn label(self) -> &'static str {
        match self {
            Size::Letter => "Letter",
            Size::Legal => "Legal",
            Size::Tabloid => "Tabloid",
            Size::Executive => "Executive",
            Size::A3 => "A3",
            Size::A4 => "A4",
            Size::A5 => "A5",
            Size::A6 => "A6",
            Size::B4 => "B4",
            Size::B5 => "B5",
            Size::Index3x5 => "Index Card (3 x 5 in)",
            Size::Index4x6 => "Index Card (4 x 6 in)",
            Size::Index5x8 => "Index Card (5 x 8 in)",
            Size::Custom(..) => "Custom",
        }
    }

    /// Width and height in millimetres, the short side first for a standard size.
    pub fn mm(self) -> (f32, f32) {
        match self {
            Size::Letter => (215.9, 279.4),
            Size::Legal => (215.9, 355.6),
            Size::Tabloid => (279.4, 431.8),
            Size::Executive => (184.15, 266.7),
            Size::A3 => (297.0, 420.0),
            Size::A4 => (210.0, 297.0),
            Size::A5 => (148.0, 210.0),
            Size::A6 => (105.0, 148.0),
            Size::B4 => (250.0, 353.0),
            Size::B5 => (176.0, 250.0),
            Size::Index3x5 => (76.2, 127.0),
            Size::Index4x6 => (101.6, 152.4),
            Size::Index5x8 => (127.0, 203.2),
            Size::Custom(w, h) => (w, h),
        }
    }

    pub fn parse(text: &str) -> Option<Size> {
        let text = text.trim().to_ascii_lowercase();
        if let Some(dims) = text.strip_prefix("custom:") {
            let (w, h) = dims.split_once('x')?;
            let (w, h) = (w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?);
            return (in_range(w) && in_range(h)).then_some(Size::Custom(w, h));
        }
        Size::STANDARD.into_iter().find(|s| s.id() == text)
    }
}

/// The smallest and largest side a sheet may have, in millimetres: a sticky note to a poster.
pub const MIN_SIDE: f32 = 20.0;
pub const MAX_SIDE: f32 = 2000.0;

fn in_range(mm: f32) -> bool {
    mm.is_finite() && (MIN_SIDE..=MAX_SIDE).contains(&mm)
}

/// A number as the file writes it: no more than two decimals, no trailing zeros.
fn number(v: f32) -> String {
    let s = format!("{:.2}", v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// A page's paper: its size, whether it is turned on its side, and its margins.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paper {
    pub size: Size,
    pub landscape: bool,
    /// Top, right, bottom, left, in millimetres.
    pub margins: [f32; 4],
}

impl Paper {
    /// The margins a new sheet starts with: three quarters of an inch all round, which leaves
    /// room to write even on an index card.
    pub const DEFAULT_MARGINS: [f32; 4] = [19.05, 19.05, 19.05, 19.05];

    pub fn new(size: Size) -> Paper {
        Paper { size, landscape: false, margins: Paper::DEFAULT_MARGINS }
    }

    /// Width and height of the sheet as it lies, in millimetres.
    pub fn sheet_mm(self) -> (f32, f32) {
        let (w, h) = self.size.mm();
        if self.landscape {
            (w.max(h), w.min(h))
        } else if matches!(self.size, Size::Custom(..)) {
            (w, h)
        } else {
            (w.min(h), w.max(h))
        }
    }

    /// The margins, each held so that some of the sheet is left to write on.
    pub fn margins_mm(self) -> [f32; 4] {
        let (w, h) = self.sheet_mm();
        let [t, r, b, l] = self.margins.map(|m| if m.is_finite() { m.max(0.0) } else { 0.0 });
        let fit = |a: f32, b: f32, side: f32| {
            let room = (side - 10.0).max(0.0);
            if a + b <= room { (a, b) } else { (a * room / (a + b), b * room / (a + b)) }
        };
        let (t, b) = fit(t, b, h);
        let (l, r) = fit(l, r, w);
        [t, r, b, l]
    }

    /// The line written in the page file.
    pub fn write(self) -> String {
        let m = self.margins_mm().map(number).join(",");
        format!("{} {} {m}", self.size.id(), if self.landscape { "landscape" } else { "portrait" })
    }

    /// Reads [`Paper::write`]. A line that does not read is `None`, which is Auto: a
    /// hand-edited file cannot make a page unreadable.
    pub fn parse(text: &str) -> Option<Paper> {
        let mut words = text.split_whitespace();
        let size = Size::parse(words.next()?)?;
        let mut paper = Paper::new(size);
        for word in words {
            match word.to_ascii_lowercase().as_str() {
                "landscape" => paper.landscape = true,
                "portrait" => paper.landscape = false,
                other => {
                    let parts: Vec<f32> = other.split(',').filter_map(|p| p.trim().parse::<f32>().ok()).collect();
                    match parts.as_slice() {
                        [all] => paper.margins = [*all; 4],
                        [t, r, b, l] => paper.margins = [*t, *r, *b, *l],
                        _ => {}
                    }
                }
            }
        }
        paper.margins = paper.margins_mm();
        Some(paper)
    }
}

/// Screen points in a millimetre at 100% zoom: the 96 points to an inch every desktop uses.
pub const POINTS_PER_MM: f32 = 96.0 / 25.4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_size_round_trips_and_junk_is_auto() {
        for s in Size::STANDARD {
            assert_eq!(Size::parse(&s.id()), Some(s));
            let (w, h) = s.mm();
            assert!(w < h, "{} is listed portrait", s.label());
        }
        assert_eq!(Size::parse("custom:200x300"), Some(Size::Custom(200.0, 300.0)));
        assert_eq!(Size::parse("custom:1x300"), None, "too small to write on");
        assert_eq!(Size::parse("custom:axb"), None);
        assert_eq!(Paper::parse(""), None);
        assert_eq!(Paper::parse("auto"), None);
        assert_eq!(Paper::parse("folio portrait 10,10,10,10"), None);
    }

    #[test]
    fn a_paper_line_reads_back_the_same_everywhere() {
        let p = Paper { size: Size::A4, landscape: true, margins: [20.0, 15.5, 20.0, 15.5] };
        assert_eq!(p.write(), "a4 landscape 20,15.5,20,15.5");
        assert_eq!(Paper::parse(&p.write()), Some(p));
        assert_eq!(p.sheet_mm(), (297.0, 210.0), "turned on its side");
        let mine = Paper { size: Size::Custom(150.0, 90.0), landscape: false, margins: [5.0; 4] };
        assert_eq!(mine.write(), "custom:150x90 portrait 5,5,5,5");
        assert_eq!(Paper::parse(&mine.write()), Some(mine));
        assert_eq!(mine.sheet_mm(), (150.0, 90.0), "a size of one's own is taken as given");
        assert_eq!(Paper::parse("letter").map(|p| p.margins), Some(Paper::DEFAULT_MARGINS));
        assert_eq!(Paper::parse("letter portrait 12").map(|p| p.margins), Some([12.0; 4]));
    }

    #[test]
    fn margins_always_leave_room_to_write() {
        let p = Paper { size: Size::A6, landscape: false, margins: [100.0, 80.0, 100.0, 80.0] };
        let [t, r, b, l] = p.margins_mm();
        let (w, h) = p.sheet_mm();
        assert!(t + b <= h - 10.0 + 0.01 && l + r <= w - 10.0 + 0.01, "{t} {r} {b} {l}");
        assert!(Paper { margins: [-5.0, f32::NAN, 3.0, 3.0], ..p }.margins_mm().iter().all(|m| *m >= 0.0));
    }
}

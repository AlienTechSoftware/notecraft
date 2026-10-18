//! The colour a notebook, section group or section can carry, the way OneNote colours its
//! notebook covers and section tabs. A fixed, named set: a name survives a theme change and
//! stays readable in both light and dark, which a free hex value would not.

/// The colours a container can be given. `None` means the theme's own colour for that kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeColor {
    Purple,
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Magenta,
    Gray,
}

impl NodeColor {
    pub const ALL: [NodeColor; 9] = [
        NodeColor::Purple,
        NodeColor::Blue,
        NodeColor::Teal,
        NodeColor::Green,
        NodeColor::Yellow,
        NodeColor::Orange,
        NodeColor::Red,
        NodeColor::Magenta,
        NodeColor::Gray,
    ];

    pub fn id(self) -> &'static str {
        match self {
            NodeColor::Purple => "purple",
            NodeColor::Blue => "blue",
            NodeColor::Teal => "teal",
            NodeColor::Green => "green",
            NodeColor::Yellow => "yellow",
            NodeColor::Orange => "orange",
            NodeColor::Red => "red",
            NodeColor::Magenta => "magenta",
            NodeColor::Gray => "gray",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            NodeColor::Purple => "Purple",
            NodeColor::Blue => "Blue",
            NodeColor::Teal => "Teal",
            NodeColor::Green => "Green",
            NodeColor::Yellow => "Yellow",
            NodeColor::Orange => "Orange",
            NodeColor::Red => "Red",
            NodeColor::Magenta => "Magenta",
            NodeColor::Gray => "Gray",
        }
    }

    /// The colour on a light surface and on a dark one. A front end picks by its mode and may
    /// still adjust for contrast; these are the starting points.
    pub fn rgb(self, dark: bool) -> (u8, u8, u8) {
        let (light, night) = match self {
            NodeColor::Purple => ((0x7A, 0x45, 0xB8), (0xB0, 0x85, 0xE8)),
            NodeColor::Blue => ((0x1A, 0x5F, 0xD0), (0x8F, 0xB8, 0xFF)),
            NodeColor::Teal => ((0x0F, 0x82, 0x8C), (0x5A, 0xCF, 0xD8)),
            NodeColor::Green => ((0x1E, 0x8A, 0x44), (0x6C, 0xD0, 0x8A)),
            NodeColor::Yellow => ((0xA8, 0x74, 0x00), (0xF2, 0xC9, 0x4C)),
            NodeColor::Orange => ((0xC2, 0x5A, 0x11), (0xFF, 0xA8, 0x62)),
            NodeColor::Red => ((0xC0, 0x39, 0x2B), (0xFF, 0x8A, 0x80)),
            NodeColor::Magenta => ((0xA8, 0x27, 0x7A), (0xF0, 0x8A, 0xCE)),
            NodeColor::Gray => ((0x5B, 0x64, 0x72), (0xB4, 0xBC, 0xC8)),
        };
        if dark { night } else { light }
    }

    /// Reads a stored value. Unknown text is `None`, so a hand-edited file cannot break a tree.
    pub fn parse(text: &str) -> Option<NodeColor> {
        let want = text.trim().to_ascii_lowercase();
        NodeColor::ALL.into_iter().find(|c| c.id() == want)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_junk_is_ignored() {
        for c in NodeColor::ALL {
            assert_eq!(NodeColor::parse(c.id()), Some(c));
            assert_eq!(NodeColor::parse(&c.label().to_uppercase()), Some(c));
            assert!(!c.label().is_empty());
        }
        assert_eq!(NodeColor::parse("  Blue  "), Some(NodeColor::Blue));
        assert_eq!(NodeColor::parse("chartreuse"), None);
        assert_eq!(NodeColor::parse(""), None);
        assert_eq!(NodeColor::parse("#ff0000"), None);
    }

    #[test]
    fn every_colour_differs_in_both_modes() {
        for dark in [false, true] {
            let all: Vec<(u8, u8, u8)> = NodeColor::ALL.iter().map(|c| c.rgb(dark)).collect();
            for (i, a) in all.iter().enumerate() {
                for b in all.iter().skip(i + 1) {
                    assert_ne!(a, b, "two colours are the same (dark: {dark})");
                }
            }
        }
        assert_ne!(NodeColor::Red.rgb(false), NodeColor::Red.rgb(true), "the dark variant is lighter");
    }
}

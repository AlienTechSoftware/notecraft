//! Which paragraphs of a page are justified: stretched to fill the column.
//!
//! Justification is not in the words. CommonMark has no alignment at all, and an HTML page
//! could carry a `style` on the paragraph but would then say something the Markdown copy
//! cannot, so the two formats would stop being the same page. It is layout, like the boxes
//! a page is arranged in and the width its table columns are drawn at, so it is kept where
//! those are: in the page's own metadata, as a line that names each justified paragraph by
//! its position among the page's blocks.
//!
//! Nothing is written into the text itself. A justified Arabic paragraph is stretched by
//! kashida when it is drawn (`notecraft_text::kashida`), and the tatweels live only in
//! the drawing: the page file keeps the words the writer typed, so another program reading
//! the Markdown sees plain sentences and a page saved and reopened comes back the same.
//!
//! The weakness is the one the table column widths have. A page whose blocks are rearranged
//! outside NoteCraft may find a number against the wrong paragraph. The worst that does is
//! justify a paragraph the writer did not ask for, which one click puts right; nothing a
//! page says is lost.

/// How many paragraphs are remembered. A page with more than this is not something a hand
/// marked, so the rest are drawn ragged.
const MAX_BLOCKS: usize = 500;

/// How hard a justified paragraph is stretched.
///
/// Word has had these three since at least 2010, under the names Justify Low, Justify
/// Medium and Justify High, and they decide how many kashidas a line may take. A writer of
/// Arabic already knows them by those names, so NoteCraft uses the same three and the same
/// words; `docs/rtl-in-word-and-onenote.md` has the comparison.
///
/// What each one allows is typography, not page data, so it is decided in
/// `notecraft_text::kashida` and not here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Strength {
    /// The fewest kashidas: only the joins that take elongation best, and one apiece. A
    /// line may stop short of the margin rather than stretch a join that would look wrong.
    Low,
    /// The middle setting, and what a paragraph gets when the page does not say.
    #[default]
    Medium,
    /// As many as the line will take, down to the joins of last resort.
    High,
}

impl Strength {
    /// The name the page file keeps, and the one Word uses.
    pub fn id(self) -> &'static str {
        match self {
            Strength::Low => "low",
            Strength::Medium => "medium",
            Strength::High => "high",
        }
    }

    /// Reads one back. Anything else is `None`, and the caller falls back to the middle.
    pub fn of(text: &str) -> Option<Strength> {
        match text.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Strength::Low),
            "medium" => Some(Strength::Medium),
            "high" => Some(Strength::High),
            _ => None,
        }
    }

    /// All three, weakest first, for a menu.
    pub const ALL: [Strength; 3] = [Strength::Low, Strength::Medium, Strength::High];
}

/// The paragraphs of a page that are justified, by their position among its blocks, each
/// with the strength it is stretched to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Justified {
    /// Sorted by block and without repeats, so the written line is the same every time.
    blocks: Vec<(usize, Strength)>,
}

impl Justified {
    /// True when the block at `block` is justified.
    pub fn has(&self, block: usize) -> bool {
        self.strength_of(block).is_some()
    }

    /// How hard the block at `block` is stretched, or `None` when it is not justified.
    pub fn strength_of(&self, block: usize) -> Option<Strength> {
        let at = self.blocks.binary_search_by_key(&block, |(at, _)| *at).ok()?;
        self.blocks.get(at).map(|(_, s)| *s)
    }

    /// Justifies a block at `strength`, or, with `None`, takes the justification off.
    /// Asking again at a different strength changes it.
    pub fn set(&mut self, block: usize, strength: Option<Strength>) {
        match (self.blocks.binary_search_by_key(&block, |(at, _)| *at), strength) {
            (Err(at), Some(s)) if self.blocks.len() < MAX_BLOCKS => self.blocks.insert(at, (block, s)),
            (Ok(at), Some(s)) => {
                if let Some(have) = self.blocks.get_mut(at) {
                    have.1 = s;
                }
            }
            (Ok(at), None) => {
                self.blocks.remove(at);
            }
            _ => {}
        }
    }

    /// True when no paragraph on the page is justified, which is what a page says until one
    /// is asked for.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The justified blocks and their strengths, in order.
    pub fn blocks(&self) -> &[(usize, Strength)] {
        &self.blocks
    }
}

/// Reads the line a page keeps: the block numbers, separated by commas, each with an
/// optional strength after a colon. `4, 11:high, 15:low` is three justified paragraphs.
///
/// A number on its own is the middle strength, which is both the sensible default and what
/// every page written before the strengths existed says. Anything that does not read as a
/// number is dropped, and a strength that does not read as one of the three falls back to
/// the middle, because justification is a convenience and a page must open whatever its
/// metadata says.
pub fn parse(text: &str) -> Justified {
    let mut blocks: Vec<(usize, Strength)> = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (number, strength) = match part.split_once(':') {
            Some((n, s)) => (n.trim(), Strength::of(s).unwrap_or_default()),
            None => (part, Strength::default()),
        };
        let Ok(at) = number.parse::<usize>() else { continue };
        if blocks.iter().any(|(have, _)| *have == at) {
            continue;
        }
        blocks.push((at, strength));
    }
    blocks.sort_by_key(|(at, _)| *at);
    blocks.truncate(MAX_BLOCKS);
    Justified { blocks }
}

/// Writes them back as that line. The middle strength writes the number alone, so a page
/// that never asked for a strength keeps the shorter line it had. An empty set writes
/// nothing, which takes the line out.
pub fn write(justified: &Justified) -> String {
    justified
        .blocks
        .iter()
        .map(|(at, s)| if *s == Strength::Medium { at.to_string() } else { format!("{at}:{}", s.id()) })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_justified_paragraphs_survive_the_round_trip() {
        let line = "0, 4:low, 11:high";
        let marked = parse(line);
        assert_eq!(marked.strength_of(0), Some(Strength::Medium), "a number on its own is the middle strength");
        assert_eq!(marked.strength_of(4), Some(Strength::Low));
        assert_eq!(marked.strength_of(11), Some(Strength::High));
        assert_eq!(marked.strength_of(1), None, "a paragraph nobody marked is ragged");
        assert!(marked.has(0) && !marked.has(1));
        assert_eq!(write(&marked), line);
        assert!(parse("").is_empty());
        assert_eq!(write(&Justified::default()), "", "nothing marked writes no line at all");
    }

    /// A page written before the strengths existed says `4, 11`, and must keep saying it.
    #[test]
    fn a_page_from_before_the_strengths_reads_and_writes_the_same_line() {
        let line = "4, 11";
        let marked = parse(line);
        assert_eq!(marked.blocks(), [(4, Strength::Medium), (11, Strength::Medium)]);
        assert_eq!(write(&marked), line, "the middle strength writes the number alone");
    }

    #[test]
    fn a_click_marks_one_paragraph_and_leaves_the_rest() {
        let mut marked = Justified::default();
        marked.set(3, Some(Strength::Medium));
        marked.set(1, Some(Strength::High));
        assert_eq!(marked.blocks(), [(1, Strength::High), (3, Strength::Medium)], "kept in block order");
        // Asking again at another strength changes that paragraph and adds no second entry.
        marked.set(1, Some(Strength::Low));
        assert_eq!(marked.blocks(), [(1, Strength::Low), (3, Strength::Medium)]);
        marked.set(1, None);
        assert_eq!(marked.blocks(), [(3, Strength::Medium)]);
        marked.set(9, None);
        assert_eq!(marked.blocks(), [(3, Strength::Medium)], "unjustifying a ragged paragraph does nothing");
    }

    #[test]
    fn the_three_strengths_are_word_s_three() {
        assert_eq!(Strength::ALL.map(Strength::id), ["low", "medium", "high"]);
        for s in Strength::ALL {
            assert_eq!(Strength::of(s.id()), Some(s), "{s:?} reads back");
        }
        assert_eq!(Strength::of("HIGH"), Some(Strength::High), "the case in the file does not matter");
        assert_eq!(Strength::of("sideways"), None);
        assert_eq!(Strength::default(), Strength::Medium);
    }

    #[test]
    fn nonsense_in_the_page_is_dropped_not_obeyed() {
        assert!(parse("sideways").is_empty());
        assert!(parse(",,,").is_empty());
        assert_eq!(parse("2, bad, 5").blocks(), [(2, Strength::Medium), (5, Strength::Medium)], "what can be read is kept");
        assert_eq!(parse("5, 2, 5").blocks(), [(2, Strength::Medium), (5, Strength::Medium)], "out of order and repeated reads as one set");
        assert_eq!(parse("-1, 3").blocks(), [(3, Strength::Medium)], "a block before the first one is not a block");
        assert_eq!(parse("3:sideways").strength_of(3), Some(Strength::Medium), "a strength nobody knows falls back to the middle");
        assert_eq!(parse("3:").strength_of(3), Some(Strength::Medium), "and so does an empty one");
        let crowd: String = (0..MAX_BLOCKS + 50).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        assert_eq!(parse(&crowd).blocks().len(), MAX_BLOCKS);
    }
}

//! Which paragraphs of a page are justified, stretched to fill the column, and which are
//! aligned to the left, the centre or the right (issue #258).
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

/// Which edge a paragraph's lines are set against, when it was given one (issue #258). A
/// paragraph given none starts its lines where its words start: at the left when they read
/// left to right and at the right when they read right to left. `Left` and `Right` are the
/// sides of the page whichever way the words read, as Word and OneNote have them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    /// The name the page file keeps.
    pub fn id(self) -> &'static str {
        match self {
            Align::Left => "left",
            Align::Center => "center",
            Align::Right => "right",
        }
    }

    /// Reads one back; anything else is `None`.
    pub fn of(text: &str) -> Option<Align> {
        match text.trim().to_ascii_lowercase().as_str() {
            "left" => Some(Align::Left),
            "center" | "centre" => Some(Align::Center),
            "right" => Some(Align::Right),
            _ => None,
        }
    }

    /// All three, in the order of the menu.
    pub const ALL: [Align; 3] = [Align::Left, Align::Center, Align::Right];
}

/// How one paragraph is laid out: stretched to fill the column, or set against one edge or
/// the middle. A paragraph is one or the other, as in Word, where Justify is the fourth of
/// the alignment buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Justify(Strength),
    Align(Align),
}

impl Mark {
    /// How hard it is stretched, when it is justified.
    pub fn strength(self) -> Option<Strength> {
        match self {
            Mark::Justify(s) => Some(s),
            Mark::Align(_) => None,
        }
    }

    /// The edge its lines are set against, when it was aligned.
    pub fn align(self) -> Option<Align> {
        match self {
            Mark::Align(a) => Some(a),
            Mark::Justify(_) => None,
        }
    }
}

/// The most points any one of the three spacings may be. A page that says more is read at
/// this, because a gap taller than a screen is a mistake in the file, not a layout.
pub const MAX_POINTS: u16 = 200;

/// The room a paragraph takes beyond what the page gives every paragraph (issue #275), in
/// points: space before it, space after it, and the least height of each of its lines. Nothing
/// is the page's own spacing, and a paragraph given nothing keeps no entry at all.
///
/// Like the alignment, it is layout and not words, so it is kept in the page's metadata and
/// the text is never touched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Spacing {
    /// Extra room above the paragraph.
    pub before: u16,
    /// Extra room below it.
    pub after: u16,
    /// The least height of a line, in points. Nothing (0) leaves the line at the height its
    /// font gives it; a larger number never makes a line shorter than that.
    pub line: u16,
}

impl Spacing {
    /// True when it asks for nothing, which is what a paragraph without an entry has.
    pub fn is_none(self) -> bool {
        self == Spacing::default()
    }

    /// The same with every part held to [`MAX_POINTS`].
    pub fn clamped(self) -> Spacing {
        Spacing { before: self.before.min(MAX_POINTS), after: self.after.min(MAX_POINTS), line: self.line.min(MAX_POINTS) }
    }
}

/// The paragraphs of a page that are justified or aligned, by their position among its
/// blocks, each with how, and the ones given room of their own.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Justified {
    /// Sorted by block and without repeats, so the written line is the same every time.
    blocks: Vec<(usize, Mark)>,
    /// The paragraphs given spacing (issue #275), sorted the same way. Never holds a `Spacing`
    /// that asks for nothing.
    spacing: Vec<(usize, Spacing)>,
}

impl Justified {
    /// True when the block at `block` is justified.
    pub fn has(&self, block: usize) -> bool {
        self.strength_of(block).is_some()
    }

    /// How the block at `block` is laid out, or `None` when it was given nothing.
    pub fn mark_of(&self, block: usize) -> Option<Mark> {
        let at = self.blocks.binary_search_by_key(&block, |(at, _)| *at).ok()?;
        self.blocks.get(at).map(|(_, m)| *m)
    }

    /// How hard the block at `block` is stretched, or `None` when it is not justified.
    pub fn strength_of(&self, block: usize) -> Option<Strength> {
        self.mark_of(block).and_then(Mark::strength)
    }

    /// The edge the block at `block` is set against, or `None` when it was not aligned.
    pub fn align_of(&self, block: usize) -> Option<Align> {
        self.mark_of(block).and_then(Mark::align)
    }

    /// Justifies a block at `strength`, or, with `None`, takes the justification off.
    /// Asking again at a different strength changes it. Justifying an aligned paragraph
    /// replaces its alignment.
    pub fn set(&mut self, block: usize, strength: Option<Strength>) {
        if strength.is_none() && self.align_of(block).is_some() {
            return;
        }
        self.set_mark(block, strength.map(Mark::Justify));
    }

    /// Aligns a block, or, with `None`, gives it back the alignment its words have. Aligning
    /// a justified paragraph stops it being justified.
    pub fn set_align(&mut self, block: usize, align: Option<Align>) {
        if align.is_none() && self.strength_of(block).is_some() {
            return;
        }
        self.set_mark(block, align.map(Mark::Align));
    }

    /// Gives a block `mark`, or, with `None`, takes whatever it had off.
    pub fn set_mark(&mut self, block: usize, mark: Option<Mark>) {
        match (self.blocks.binary_search_by_key(&block, |(at, _)| *at), mark) {
            (Err(at), Some(m)) if self.blocks.len() < MAX_BLOCKS => self.blocks.insert(at, (block, m)),
            (Ok(at), Some(m)) => {
                if let Some(have) = self.blocks.get_mut(at) {
                    have.1 = m;
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
        self.blocks.is_empty() && self.spacing.is_empty()
    }

    /// The room the block at `block` was given, or `None` when it was given none.
    pub fn spacing_of(&self, block: usize) -> Option<Spacing> {
        let at = self.spacing.binary_search_by_key(&block, |(at, _)| *at).ok()?;
        self.spacing.get(at).map(|(_, s)| *s)
    }

    /// Gives a block `spacing`, or, with `None` or a spacing that asks for nothing, takes what
    /// it had off. Every part is held to [`MAX_POINTS`].
    pub fn set_spacing(&mut self, block: usize, spacing: Option<Spacing>) {
        let spacing = spacing.map(Spacing::clamped).filter(|s| !s.is_none());
        match (self.spacing.binary_search_by_key(&block, |(at, _)| *at), spacing) {
            (Err(at), Some(s)) if self.spacing.len() < MAX_BLOCKS => self.spacing.insert(at, (block, s)),
            (Ok(at), Some(s)) => {
                if let Some(have) = self.spacing.get_mut(at) {
                    have.1 = s;
                }
            }
            (Ok(at), None) => {
                self.spacing.remove(at);
            }
            _ => {}
        }
    }

    /// Every block given spacing, and what it was given, in order.
    pub fn spacings(&self) -> &[(usize, Spacing)] {
        &self.spacing
    }

    /// The justified blocks and their strengths, in order.
    pub fn blocks(&self) -> Vec<(usize, Strength)> {
        self.blocks.iter().filter_map(|(at, m)| m.strength().map(|s| (*at, s))).collect()
    }

    /// Every block given a layout, and what it was given, in order.
    pub fn marks(&self) -> &[(usize, Mark)] {
        &self.blocks
    }

    /// A block was put into the page at `at`. What is kept for a paragraph at or after it moves
    /// down one with it, so it stays on the same paragraph. True when anything changed.
    pub fn insert_block(&mut self, at: usize) -> bool {
        let mut moved = false;
        for block in self.blocks.iter_mut().map(|(b, _)| b).chain(self.spacing.iter_mut().map(|(b, _)| b)) {
            if *block >= at {
                *block += 1;
                moved = true;
            }
        }
        moved
    }

    /// The blocks in `gone` were taken off the page. A mark on one of them goes, and a mark
    /// on a paragraph after them moves up with it, so it stays on the same paragraph. True
    /// when anything changed.
    pub fn remove_blocks(&mut self, gone: std::ops::Range<usize>) -> bool {
        let before = (self.blocks.clone(), self.spacing.clone());
        let n = gone.len();
        self.blocks.retain(|(at, _)| !gone.contains(at));
        self.spacing.retain(|(at, _)| !gone.contains(at));
        for at in self.blocks.iter_mut().map(|(b, _)| b).chain(self.spacing.iter_mut().map(|(b, _)| b)) {
            if *at >= gone.end {
                *at -= n;
            }
        }
        (self.blocks.clone(), self.spacing.clone()) != before
    }
}

/// Reads the line a page keeps: the block numbers, separated by commas, each with an
/// optional strength after a colon. `4, 11:high, 15:low` is three justified paragraphs, and
/// `2:center, 6:right` two aligned ones.
///
/// A number on its own is the middle strength, which is both the sensible default and what
/// every page written before the strengths existed says. Anything that does not read as a
/// number is dropped, and a strength that does not read as one of the three falls back to
/// the middle, because justification is a convenience and a page must open whatever its
/// metadata says.
pub fn parse(text: &str) -> Justified {
    let mut blocks: Vec<(usize, Mark)> = Vec::new();
    let mut spacing: Vec<(usize, Spacing)> = Vec::new();
    for part in text.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        // Spacing (issue #275) is an entry that starts with `s`: `s4=6/12/0` gives block 4 six
        // points before, twelve after and no least line height. A reader from before it
        // existed finds no block number there and skips the entry, so the justified and aligned
        // paragraphs of the same line still read the way they always did.
        if let Some(rest) = part.strip_prefix('s') {
            if let Some((at, spaces)) = rest.split_once('=').and_then(|(n, v)| Some((n.trim().parse::<usize>().ok()?, spaces_of(v)?)))
                && !spacing.iter().any(|(have, _)| *have == at)
                && !spaces.is_none()
            {
                spacing.push((at, spaces));
            }
            continue;
        }
        let (number, strength) = match part.split_once(':') {
            Some((n, s)) => (n.trim(), Align::of(s).map_or_else(|| Mark::Justify(Strength::of(s).unwrap_or_default()), Mark::Align)),
            None => (part, Mark::Justify(Strength::default())),
        };
        let Ok(at) = number.parse::<usize>() else { continue };
        if blocks.iter().any(|(have, _)| *have == at) {
            continue;
        }
        blocks.push((at, strength));
    }
    blocks.sort_by_key(|(at, _)| *at);
    blocks.truncate(MAX_BLOCKS);
    spacing.sort_by_key(|(at, _)| *at);
    spacing.truncate(MAX_BLOCKS);
    Justified { blocks, spacing }
}

/// Reads `before/after/line`, each a whole number of points. Anything else is `None`.
fn spaces_of(text: &str) -> Option<Spacing> {
    let mut parts = text.split('/').map(|p| p.trim().parse::<u16>());
    let (before, after, line) = (parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?);
    parts.next().is_none().then(|| Spacing { before, after, line }.clamped())
}

/// Writes them back as that line. The middle strength writes the number alone, so a page
/// that never asked for a strength keeps the shorter line it had. An empty set writes
/// nothing, which takes the line out.
pub fn write(justified: &Justified) -> String {
    justified
        .blocks
        .iter()
        .map(|(at, m)| match m {
            Mark::Justify(Strength::Medium) => at.to_string(),
            Mark::Justify(s) => format!("{at}:{}", s.id()),
            Mark::Align(a) => format!("{at}:{}", a.id()),
        })
        .chain(justified.spacing.iter().map(|(at, s)| format!("s{at}={}/{}/{}", s.before, s.after, s.line)))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_follows_its_paragraph_when_blocks_before_it_go() {
        let mut marked = parse("0, 3:low, 7:high");
        assert!(marked.remove_blocks(1..3));
        assert_eq!(write(&marked), "0, 1:low, 5:high");
        // A mark on a block that went goes with it.
        assert!(marked.remove_blocks(1..2));
        assert_eq!(write(&marked), "0, 4:high");
        assert!(!marked.remove_blocks(9..12));
        // A block put in before a paragraph moves its mark down; one after the last moves none.
        assert!(marked.insert_block(2));
        assert_eq!(write(&marked), "0, 5:high");
        assert!(!marked.insert_block(6));
    }

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

    /// Issue #258: an aligned paragraph is kept in the same line as a justified one, and a
    /// paragraph is one or the other.
    #[test]
    fn aligned_paragraphs_survive_the_round_trip_and_replace_justification() {
        let line = "1:center, 3, 5:right, 8:left, 9:high";
        let marked = parse(line);
        assert_eq!(marked.align_of(1), Some(Align::Center));
        assert_eq!(marked.align_of(5), Some(Align::Right));
        assert_eq!(marked.align_of(8), Some(Align::Left));
        assert_eq!(marked.strength_of(1), None, "a centred paragraph is not justified");
        assert_eq!(marked.blocks(), [(3, Strength::Medium), (9, Strength::High)], "blocks() lists only the justified");
        assert_eq!(write(&marked), line);
        assert_eq!(parse("2:CENTRE").align_of(2), Some(Align::Center), "either spelling, any case");

        let mut m = Justified::default();
        m.set(4, Some(Strength::Low));
        m.set_align(4, Some(Align::Center));
        assert_eq!((m.strength_of(4), m.align_of(4)), (None, Some(Align::Center)), "aligning stops justifying");
        m.set(4, None);
        assert_eq!(m.align_of(4), Some(Align::Center), "taking justification off an aligned paragraph leaves it aligned");
        m.set(4, Some(Strength::High));
        assert_eq!((m.strength_of(4), m.align_of(4)), (Some(Strength::High), None), "justifying replaces the alignment");
        m.set_align(4, None);
        assert_eq!(m.strength_of(4), Some(Strength::High), "taking alignment off a justified paragraph leaves it justified");
        m.set_align(6, Some(Align::Right));
        assert!(m.insert_block(5));
        assert_eq!(m.align_of(7), Some(Align::Right), "an alignment follows its paragraph");
        m.set_align(7, None);
        assert_eq!(write(&m), "4:high");
        assert_eq!(Align::ALL.map(Align::id), ["left", "center", "right"]);
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

    #[test]
    fn spacing_is_kept_on_the_same_line_and_survives_the_round_trip() {
        let line = "2:center, 5, s2=6/12/0, s9=0/0/18";
        let marked = parse(line);
        assert_eq!(marked.spacing_of(2), Some(Spacing { before: 6, after: 12, line: 0 }));
        assert_eq!(marked.spacing_of(9), Some(Spacing { before: 0, after: 0, line: 18 }));
        assert_eq!(marked.spacing_of(5), None, "a paragraph that was never given room has none");
        assert_eq!(marked.align_of(2), Some(Align::Center), "the alignment on the same line still reads");
        assert_eq!(marked.strength_of(5), Some(Strength::Medium));
        assert_eq!(write(&marked), line, "and writes back the way it came");
        assert_eq!(marked.blocks(), [(5, Strength::Medium)], "spacing is not mistaken for justification");
    }

    /// A reader from before spacing existed finds no block number in `s2=6/12/0` and skips it,
    /// so a page with spacing opens there with the justification it always had.
    #[test]
    fn an_entry_of_spacing_is_not_a_block_number() {
        for entry in ["s2=6/12/0", "s9=0/0/18"] {
            assert!(entry.parse::<usize>().is_err(), "{entry}: an older reader drops it");
            assert!(!entry.contains(':'), "{entry}: nor does it read as a strength or an alignment");
        }
        assert_eq!(parse("s2=6/12/0").marks(), &[], "the new reader keeps it as spacing and as nothing else");
    }

    #[test]
    fn spacing_that_is_wrong_or_asks_for_nothing_is_dropped() {
        let marked = parse("s1=0/0/0, s2=6/12, s3=a/b/c, s4=1/2/3/4, s=1/1/1, sx=1/1/1, s5=-1/0/0, s6=9/9/9, s6=1/1/1, s7=999/0/5");
        assert_eq!(marked.spacings(), &[(6, Spacing { before: 9, after: 9, line: 9 }), (7, Spacing { before: MAX_POINTS, after: 0, line: 5 })]);
        let mut set = Justified::default();
        set.set_spacing(3, Some(Spacing { before: 7, after: 0, line: 0 }));
        assert!(!set.is_empty());
        set.set_spacing(3, Some(Spacing::default()));
        assert!(set.is_empty(), "asking for nothing takes the entry off");
        set.set_spacing(3, Some(Spacing { before: 900, after: 1, line: 1 }));
        assert_eq!(set.spacing_of(3).map(|s| s.before), Some(MAX_POINTS));
        set.set_spacing(3, None);
        assert_eq!(set.spacing_of(3), None);
    }

    #[test]
    fn spacing_follows_its_paragraph_when_blocks_come_and_go() {
        let mut marked = parse("1:right, s1=4/0/0, s6=0/9/0");
        assert!(marked.insert_block(0));
        assert_eq!(write(&marked), "2:right, s2=4/0/0, s7=0/9/0");
        assert!(marked.remove_blocks(2..3));
        assert_eq!(write(&marked), "s6=0/9/0", "the spacing of a removed paragraph goes with it");
        assert!(!marked.remove_blocks(20..22));
        assert!(marked.insert_block(6));
        assert_eq!(write(&marked), "s7=0/9/0");
        assert!(marked.remove_blocks(7..8));
        assert!(marked.is_empty());
    }
}

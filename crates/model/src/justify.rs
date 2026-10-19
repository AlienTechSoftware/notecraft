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

/// The paragraphs of a page that are justified, by their position among its blocks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Justified {
    /// Sorted and without repeats, so the written line is the same every time.
    blocks: Vec<usize>,
}

impl Justified {
    /// True when the block at `block` is justified.
    pub fn has(&self, block: usize) -> bool {
        self.blocks.binary_search(&block).is_ok()
    }

    /// Marks a block as justified, or takes the mark off again.
    pub fn set(&mut self, block: usize, on: bool) {
        match (self.blocks.binary_search(&block), on) {
            (Err(at), true) if self.blocks.len() < MAX_BLOCKS => self.blocks.insert(at, block),
            (Ok(at), false) => {
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

    /// The justified blocks, in order.
    pub fn blocks(&self) -> &[usize] {
        &self.blocks
    }
}

/// Reads the line a page keeps: the block numbers, separated by commas.
///
/// Anything that does not read as a number is dropped, because justification is a
/// convenience and a page must open whatever its metadata says.
pub fn parse(text: &str) -> Justified {
    let mut blocks: Vec<usize> = text.split(',').map(str::trim).filter(|p| !p.is_empty()).filter_map(|p| p.parse::<usize>().ok()).collect();
    blocks.sort_unstable();
    blocks.dedup();
    blocks.truncate(MAX_BLOCKS);
    Justified { blocks }
}

/// Writes them back as that line. An empty set writes nothing, which takes the line out.
pub fn write(justified: &Justified) -> String {
    justified.blocks.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_justified_paragraphs_survive_the_round_trip() {
        let line = "0, 4, 11";
        let marked = parse(line);
        assert!(marked.has(0) && marked.has(4) && marked.has(11));
        assert!(!marked.has(1), "a paragraph nobody marked is ragged");
        assert_eq!(write(&marked), line);
        assert!(parse("").is_empty());
        assert_eq!(write(&Justified::default()), "", "nothing marked writes no line at all");
    }

    #[test]
    fn a_click_marks_one_paragraph_and_leaves_the_rest() {
        let mut marked = Justified::default();
        marked.set(3, true);
        marked.set(1, true);
        assert_eq!(marked.blocks(), [1, 3], "kept in order, so the line is the same every time");
        marked.set(3, true);
        assert_eq!(marked.blocks(), [1, 3], "asking twice marks it once");
        marked.set(1, false);
        assert_eq!(marked.blocks(), [3]);
        marked.set(9, false);
        assert_eq!(marked.blocks(), [3], "taking the mark off a paragraph that had none does nothing");
    }

    #[test]
    fn nonsense_in_the_page_is_dropped_not_obeyed() {
        assert!(parse("sideways").is_empty());
        assert!(parse(",,,").is_empty());
        assert_eq!(parse("2, bad, 5").blocks(), [2, 5], "what can be read is kept");
        assert_eq!(parse("5, 2, 5").blocks(), [2, 5], "out of order and repeated reads as one set");
        assert_eq!(parse("-1, 3").blocks(), [3], "a block before the first one is not a block");
        let crowd: String = (0..MAX_BLOCKS + 50).map(|n| n.to_string()).collect::<Vec<_>>().join(",");
        assert_eq!(parse(&crowd).blocks().len(), MAX_BLOCKS);
    }
}

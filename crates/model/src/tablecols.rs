//! How wide the columns of a table are drawn.
//!
//! A column width is not part of the table. A CommonMark table has no room for one, and an
//! HTML table could carry it but would then say something the Markdown copy cannot, so the
//! two formats would stop being the same page. Widths are layout, like the boxes a page is
//! arranged in, so they are kept where those are: in the page's own metadata, as a line
//! that names the table by its position among the page's blocks.
//!
//! A page whose blocks are rearranged outside NoteCraft may find a width against the wrong
//! table. The worst that does is draw a table in the wrong widths, which a drag puts right;
//! nothing a page says is lost, which is why this layer is allowed to be a guess.

/// The narrowest and widest a column may be dragged.
pub const MIN_WIDTH: f32 = 40.0;
pub const MAX_WIDTH: f32 = 900.0;

/// How many tables and columns are kept. A page with more than this is not something a
/// hand arranged, so the rest are drawn at the usual width.
const MAX_TABLES: usize = 200;
const MAX_COLUMNS: usize = 64;

/// The widths every table on a page was given, each against the block it belongs to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widths {
    /// Sorted by block, so the written line is the same every time.
    tables: Vec<(usize, Vec<f32>)>,
}

impl Widths {
    /// The widths of the table at block `block`, or `None` when it was never dragged.
    pub fn of(&self, block: usize) -> Option<&[f32]> {
        self.tables.iter().find(|(at, _)| *at == block).map(|(_, w)| w.as_slice())
    }

    /// Sets one column's width, leaving the others as they are. `all` is what the table's
    /// columns are at now, so a table dragged for the first time keeps the look it had.
    pub fn set(&mut self, block: usize, column: usize, width: f32, all: &[f32]) {
        if column >= all.len() || all.len() > MAX_COLUMNS || self.tables.len() >= MAX_TABLES && self.of(block).is_none() {
            return;
        }
        let mut widths: Vec<f32> = all.iter().map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH)).collect();
        widths[column] = width.clamp(MIN_WIDTH, MAX_WIDTH);
        match self.tables.iter_mut().find(|(at, _)| *at == block) {
            Some((_, have)) => *have = widths,
            None => {
                self.tables.push((block, widths));
                self.tables.sort_by_key(|(at, _)| *at);
            }
        }
    }

    /// True when no table has been given widths of its own.
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// A block was put into the page at `at`. What is kept for a table at or after it moves
    /// down one with it, so it stays on the same table. True when anything changed.
    pub fn insert_block(&mut self, at: usize) -> bool {
        let mut moved = false;
        for (block, _) in &mut self.tables {
            if *block >= at {
                *block += 1;
                moved = true;
            }
        }
        moved
    }

    /// A column was put into the table at block `block`, at column `at`. The columns from
    /// there on keep their widths, moved one along, and the new one is `width` wide, so a
    /// width stays with the column it was dragged for (issue #154). True when anything
    /// changed; a table never dragged has nothing kept and stays that way.
    pub fn insert_column(&mut self, block: usize, at: usize, width: f32) -> bool {
        let Some((_, widths)) = self.tables.iter_mut().find(|(b, _)| *b == block) else { return false };
        if widths.len() >= MAX_COLUMNS {
            return false;
        }
        widths.insert(at.min(widths.len()), width.clamp(MIN_WIDTH, MAX_WIDTH));
        true
    }

    /// Column `at` was taken out of the table at block `block`. Its width goes with it and
    /// the columns after it keep theirs. True when anything changed.
    pub fn remove_column(&mut self, block: usize, at: usize) -> bool {
        let Some((_, widths)) = self.tables.iter_mut().find(|(b, _)| *b == block) else { return false };
        if at >= widths.len() {
            return false;
        }
        widths.remove(at);
        true
    }

    /// The blocks in `gone` were taken off the page. A width kept for one of them goes, and
    /// a width kept for a table after them moves up with it, so it stays on the same table.
    /// True when anything changed.
    pub fn remove_blocks(&mut self, gone: std::ops::Range<usize>) -> bool {
        let before = self.tables.clone();
        let n = gone.len();
        self.tables.retain(|(at, _)| !gone.contains(at));
        for (at, _) in &mut self.tables {
            if *at >= gone.end {
                *at -= n;
            }
        }
        self.tables != before
    }
}

/// Reads the line a page keeps: `block:w,w,w`, one table after another, `;` between them.
///
/// Anything that does not read as that is dropped, because a width is a convenience and a
/// page must open whatever its metadata says.
pub fn parse(text: &str) -> Widths {
    let mut tables: Vec<(usize, Vec<f32>)> = Vec::new();
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()).take(MAX_TABLES) {
        let Some((at, list)) = part.split_once(':') else { continue };
        let Ok(at) = at.trim().parse::<usize>() else { continue };
        let widths: Vec<f32> = list
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .take(MAX_COLUMNS)
            .filter_map(|w| w.parse::<f32>().ok())
            .map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH))
            .collect();
        if widths.is_empty() || tables.iter().any(|(have, _)| *have == at) {
            continue;
        }
        tables.push((at, widths));
    }
    tables.sort_by_key(|(at, _)| *at);
    Widths { tables }
}

/// Writes them back as that line. An empty set writes nothing, which takes the line out.
pub fn write(widths: &Widths) -> String {
    widths
        .tables
        .iter()
        .map(|(at, w)| {
            let list: Vec<String> = w.iter().map(|w| format!("{}", w.round() as i64)).collect();
            format!("{at}:{}", list.join(","))
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_width_follows_its_table_when_blocks_before_it_go() {
        let mut widths = parse("1:100,100; 6:120,200");
        assert!(widths.remove_blocks(2..4));
        assert_eq!(write(&widths), "1:100,100; 4:120,200");
        // A width kept for a block that went goes with it.
        assert!(widths.remove_blocks(1..2));
        assert_eq!(write(&widths), "3:120,200");
        // Blocks after the last table move nothing.
        assert!(!widths.remove_blocks(8..10));
        assert_eq!(write(&widths), "3:120,200");
        // A block put in before the table moves it down; one put in after it does not.
        assert!(widths.insert_block(3));
        assert_eq!(write(&widths), "4:120,200");
        assert!(!widths.insert_block(5));
        assert_eq!(write(&widths), "4:120,200");
    }

    #[test]
    fn widths_survive_the_round_trip() {
        let line = "0:160,240; 7:120,120,300";
        let widths = parse(line);
        assert_eq!(widths.of(0), Some([160.0, 240.0].as_slice()));
        assert_eq!(widths.of(7), Some([120.0, 120.0, 300.0].as_slice()));
        assert_eq!(widths.of(3), None, "a table nobody dragged has no widths of its own");
        assert_eq!(write(&widths), line);
    }

    /// Issue #154: a column put in or taken out keeps every other column's width on the
    /// column it was dragged for.
    #[test]
    fn widths_follow_their_columns_when_a_column_goes_in_or_out() {
        let mut widths = parse("1:210,290; 4:100,120");
        assert!(widths.insert_column(1, 0, 160.0), "a column at the front");
        assert_eq!(widths.of(1), Some([160.0, 210.0, 290.0].as_slice()));
        assert!(widths.insert_column(1, 3, 150.0), "and one at the end");
        assert_eq!(widths.of(1), Some([160.0, 210.0, 290.0, 150.0].as_slice()));
        assert!(widths.remove_column(1, 1));
        assert_eq!(widths.of(1), Some([160.0, 290.0, 150.0].as_slice()));
        assert_eq!(widths.of(4), Some([100.0, 120.0].as_slice()), "another table is left alone");

        // A table nobody dragged keeps nothing, before or after.
        assert!(!widths.insert_column(2, 0, 160.0));
        assert!(!widths.remove_column(2, 0));
        assert_eq!(widths.of(2), None);
        assert!(!widths.remove_column(4, 9), "a column that is not there");
    }

    #[test]
    fn a_drag_changes_one_column_and_leaves_the_rest() {
        let mut widths = Widths::default();
        widths.set(2, 1, 220.0, &[160.0, 160.0, 160.0]);
        assert_eq!(widths.of(2), Some([160.0, 220.0, 160.0].as_slice()));
        widths.set(2, 0, 90.0, &[160.0, 220.0, 160.0]);
        assert_eq!(widths.of(2), Some([90.0, 220.0, 160.0].as_slice()));

        // A column cannot be dragged away to nothing, or out past the far edge.
        widths.set(2, 0, 1.0, &[90.0, 220.0, 160.0]);
        assert_eq!(widths.of(2).map(|w| w[0]), Some(MIN_WIDTH));
        widths.set(2, 0, 5000.0, &[40.0, 220.0, 160.0]);
        assert_eq!(widths.of(2).map(|w| w[0]), Some(MAX_WIDTH));

        // A column that is not there is not invented.
        widths.set(2, 9, 100.0, &[40.0, 220.0, 160.0]);
        assert_eq!(widths.of(2).map(<[f32]>::len), Some(3));
    }

    #[test]
    fn nonsense_in_the_page_is_dropped_not_obeyed() {
        assert!(parse("").is_empty());
        assert!(parse("sideways").is_empty());
        assert!(parse("2:").is_empty(), "a table with no widths is no entry");
        assert_eq!(parse("1:bad,200").of(1), Some([200.0].as_slice()), "what can be read is kept");
        assert_eq!(parse("1:100; 1:200").of(1), Some([100.0].as_slice()), "the first entry for a table wins");
    }
}

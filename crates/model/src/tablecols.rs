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
    fn widths_survive_the_round_trip() {
        let line = "0:160,240; 7:120,120,300";
        let widths = parse(line);
        assert_eq!(widths.of(0), Some([160.0, 240.0].as_slice()));
        assert_eq!(widths.of(7), Some([120.0, 120.0, 300.0].as_slice()));
        assert_eq!(widths.of(3), None, "a table nobody dragged has no widths of its own");
        assert_eq!(write(&widths), line);
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

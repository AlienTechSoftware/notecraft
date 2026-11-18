//! How the tables of a page are laid out: how wide their columns are drawn, whether their
//! borders are, and which way the words in each cell are aligned and what colour is behind them.
//!
//! None of it is part of the table. A CommonMark table has no room for a width, and an
//! HTML table could carry it but would then say something the Markdown copy cannot, so the
//! two formats would stop being the same page. Layout is kept where the boxes a page is
//! arranged in are kept: in the page's own metadata, as a line that names each table by its
//! position among the page's blocks.
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

/// How many cells of one table keep an alignment of their own. A table with more than this
/// many aligned cells was not aligned by hand, cell by cell.
const MAX_CELLS: usize = 4096;

/// How many merged cells one table keeps (issue #298).
const MAX_SPANS: usize = 1024;

/// Why Table > Merge Cells said no (issue #298).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MergeRefusal {
    /// One cell is already one cell: there is nothing to merge it with.
    OneCell,
    /// The cells picked cut through a merged cell. Merging them would leave part of it
    /// outside the new one, which is not a rectangle any more.
    CutsAMergedCell,
    /// The table keeps no more merged cells.
    TooMany,
}

/// Which way the words in a table cell are aligned (issue #170). A cell with none is aligned
/// the way the table's words always were, to the start of the line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    fn letter(self) -> char {
        match self {
            Align::Left => 'l',
            Align::Center => 'c',
            Align::Right => 'r',
        }
    }

    fn from_letter(c: &str) -> Option<Align> {
        match c.trim() {
            "l" => Some(Align::Left),
            "c" => Some(Align::Center),
            "r" => Some(Align::Right),
            _ => None,
        }
    }
}

/// What one cell was given: an alignment for its words, a colour behind them, or both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    align: Option<Align>,
    /// The cell's shading (issue #169), one of the named colours a page or a section can carry,
    /// so it reads in both light and dark themes.
    shade: Option<crate::NodeColor>,
}

impl Cell {
    fn is_bare(self) -> bool {
        self.align.is_none() && self.shade.is_none()
    }
}

/// What one table was given: the widths of its columns, which may be none, whether its
/// borders are hidden, and the cells given an alignment or a shading of their own. A cell is
/// `(row, column)`, with the heading as row 0, the way the editor counts them.
#[derive(Clone, Debug, PartialEq)]
struct Layout {
    block: usize,
    widths: Vec<f32>,
    borderless: bool,
    /// Sorted by cell, so the written line is the same every time.
    cells: Vec<((usize, usize), Cell)>,
    /// Merged cells (issue #298): the top-left cell of each, with how many rows and columns
    /// it takes in. Never 1 by 1, never overlapping, sorted by cell.
    spans: Vec<((usize, usize), (usize, usize))>,
}

impl Layout {
    fn new(block: usize) -> Layout {
        Layout { block, widths: Vec::new(), borderless: false, cells: Vec::new(), spans: Vec::new() }
    }

    /// True when it says nothing the table would not do anyway, so it need not be kept.
    fn is_bare(&self) -> bool {
        self.widths.is_empty() && !self.borderless && self.cells.is_empty() && self.spans.is_empty()
    }
}

/// The rows and columns a merged cell takes in, as `(first row, first column, last row, last column)`.
fn reach(((r, c), (h, w)): ((usize, usize), (usize, usize))) -> (usize, usize, usize, usize) {
    (r, c, r + h - 1, c + w - 1)
}

/// Where a merged cell is after a row or a column was put in at `at`: one longer when `at`
/// falls inside it, one further on when it is at or after it.
fn grow((start, len): (usize, usize), at: usize) -> (usize, usize) {
    if at <= start {
        (start + 1, len)
    } else if at < start + len {
        (start, len + 1)
    } else {
        (start, len)
    }
}

/// Where a merged cell is after row or column `at` was taken out: one shorter when it took
/// `at` in, one back when it came after it, gone when `at` was all it had.
fn shrink((start, len): (usize, usize), at: usize) -> Option<(usize, usize)> {
    if at < start {
        Some((start - 1, len))
    } else if at < start + len {
        (len > 1).then_some((start, len - 1))
    } else {
        Some((start, len))
    }
}

/// The layout every table on a page was given, each against the block it belongs to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Widths {
    /// Sorted by block, so the written line is the same every time.
    tables: Vec<Layout>,
}

impl Widths {
    fn layout(&self, block: usize) -> Option<&Layout> {
        self.tables.iter().find(|l| l.block == block)
    }

    fn put(&mut self, layout: Layout) {
        self.tables.push(layout);
        self.tables.sort_by_key(|l| l.block);
    }

    /// Drops what no longer says anything, so a table set back to how it was keeps no line.
    fn tidy(&mut self) {
        self.tables.retain(|l| !l.is_bare());
    }

    fn cell(&self, block: usize, cell: (usize, usize)) -> Option<Cell> {
        self.layout(block).and_then(|l| l.cells.iter().find(|(at, _)| *at == cell).map(|(_, c)| *c))
    }

    /// How the words in cell `(row, column)` of the table at block `block` are aligned, when
    /// the cell was given an alignment of its own.
    pub fn align(&self, block: usize, cell: (usize, usize)) -> Option<Align> {
        self.cell(block, cell).and_then(|c| c.align)
    }

    /// The colour behind cell `(row, column)` of the table at block `block`, when it was given
    /// one (issue #169).
    pub fn shade(&self, block: usize, cell: (usize, usize)) -> Option<crate::NodeColor> {
        self.cell(block, cell).and_then(|c| c.shade)
    }

    /// Aligns the words in `cells` of the table at block `block`, or with `None` puts them
    /// back to the usual alignment. True when that changed anything.
    pub fn set_align(&mut self, block: usize, cells: &[(usize, usize)], align: Option<Align>) -> bool {
        self.change_cells(block, cells, align.is_some(), |c| c.align = align)
    }

    /// Shades `cells` of the table at block `block` with `shade`, or with `None` takes their
    /// colour away. Their words and alignment stay. True when that changed anything.
    pub fn set_shade(&mut self, block: usize, cells: &[(usize, usize)], shade: Option<crate::NodeColor>) -> bool {
        self.change_cells(block, cells, shade.is_some(), |c| c.shade = shade)
    }

    /// Changes what `cells` keep by `change`. `adds` says whether it can give a cell something,
    /// so a table with nothing kept is not given an entry only to take nothing off it.
    fn change_cells(&mut self, block: usize, cells: &[(usize, usize)], adds: bool, change: impl Fn(&mut Cell)) -> bool {
        if self.layout(block).is_none() {
            if !adds || self.tables.len() >= MAX_TABLES {
                return false;
            }
            self.put(Layout::new(block));
        }
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return false };
        let before = layout.cells.clone();
        for at in cells {
            let mut cell = layout.cells.iter().find(|(have, _)| have == at).map(|(_, c)| *c).unwrap_or_default();
            change(&mut cell);
            layout.cells.retain(|(have, _)| have != at);
            if !cell.is_bare() && layout.cells.len() < MAX_CELLS {
                layout.cells.push((*at, cell));
            }
        }
        layout.cells.sort_by_key(|(at, _)| *at);
        let changed = layout.cells != before;
        self.tidy();
        changed
    }

    /// A row was put into the table at block `block`, at row `at`. The rows from there on keep
    /// their cells' alignments, moved down one. True when anything changed.
    pub fn insert_row(&mut self, block: usize, at: usize) -> bool {
        let spans = self.move_spans(block, |((r, c), (h, w))| {
            let (r, h) = grow((r, h), at);
            Some(((r, c), (h, w)))
        });
        self.move_cells(block, |(r, c)| Some((if r >= at { r + 1 } else { r }, c))) || spans
    }

    /// Row `at` was taken out of the table at block `block`. Its cells' alignments go with it,
    /// and the rows after it keep theirs. True when anything changed.
    pub fn remove_row(&mut self, block: usize, at: usize) -> bool {
        let spans = self.move_spans(block, |((r, c), (h, w))| shrink((r, h), at).map(|(r, h)| ((r, c), (h, w))));
        spans
            | self.move_cells(block, |(r, c)| match r.cmp(&at) {
                std::cmp::Ordering::Less => Some((r, c)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some((r - 1, c)),
            })
    }

    /// The rows of the table at block `block` were put in a new order: `order[new] = old`, both
    /// counting the heading as row 0. Each row's cells keep their alignments. True when anything
    /// changed.
    /// A cell merged across rows cannot follow its rows to two places, so it is split; the
    /// words it held stay in its first cell, and nothing is lost (issue #298). A cell merged
    /// along one row goes with that row.
    pub fn reorder_rows(&mut self, block: usize, order: &[usize]) -> bool {
        let spans = self.move_spans(block, |((r, c), (h, w))| (h == 1).then(|| ((order.iter().position(|old| *old == r).unwrap_or(r), c), (h, w))));
        self.move_cells(block, |(r, c)| Some((order.iter().position(|old| *old == r).unwrap_or(r), c))) || spans
    }

    /// Moves, stretches or drops the merged cells of one table, by `to`. A merged cell that
    /// comes out 1 by 1 is an ordinary cell again and is not kept.
    fn move_spans(&mut self, block: usize, to: impl Fn(((usize, usize), (usize, usize))) -> Option<((usize, usize), (usize, usize))>) -> bool {
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return false };
        let before = layout.spans.clone();
        layout.spans = before.iter().filter_map(|s| to(*s)).filter(|(_, (h, w))| *h * *w > 1).collect();
        layout.spans.sort_by_key(|(at, _)| *at);
        let changed = layout.spans != before;
        self.tidy();
        changed
    }

    /// The merged cell that cell `(row, column)` of the table at block `block` is part of:
    /// its top-left cell, and how many rows and columns it takes in (issue #298).
    pub fn span(&self, block: usize, cell: (usize, usize)) -> Option<((usize, usize), (usize, usize))> {
        self.layout(block)?.spans.iter().copied().find(|s| {
            let (r0, c0, r1, c1) = reach(*s);
            (r0..=r1).contains(&cell.0) && (c0..=c1).contains(&cell.1)
        })
    }

    /// Every merged cell of the table at block `block`, top-left cell first.
    pub fn spans(&self, block: usize) -> &[((usize, usize), (usize, usize))] {
        self.layout(block).map_or(&[], |l| l.spans.as_slice())
    }

    /// The cell that holds the words for `cell`: the top-left of the merged cell it is part
    /// of, or `cell` itself.
    pub fn owner(&self, block: usize, cell: (usize, usize)) -> (usize, usize) {
        self.span(block, cell).map_or(cell, |(at, _)| at)
    }

    /// True when `cell` is covered by a merged cell whose words are kept in another cell.
    pub fn covered(&self, block: usize, cell: (usize, usize)) -> bool {
        self.owner(block, cell) != cell
    }

    /// The smallest block of cells that holds both `a` and `b` and cuts through no merged cell.
    pub fn whole_block(&self, block: usize, a: (usize, usize), b: (usize, usize)) -> ((usize, usize), (usize, usize)) {
        let (mut r0, mut c0, mut r1, mut c1) = (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
        // A merged cell reaching out of the block makes it bigger, which may take in another.
        loop {
            let mut grew = false;
            for s in self.spans(block) {
                let (sr0, sc0, sr1, sc1) = reach(*s);
                let meets = sr0 <= r1 && sr1 >= r0 && sc0 <= c1 && sc1 >= c0;
                if meets && (sr0 < r0 || sc0 < c0 || sr1 > r1 || sc1 > c1) {
                    (r0, c0, r1, c1) = (r0.min(sr0), c0.min(sc0), r1.max(sr1), c1.max(sc1));
                    grew = true;
                }
            }
            if !grew {
                return ((r0, c0), (r1, c1));
            }
        }
    }

    /// Merges the cells from `a` to `b` (any two corners) of the table at block `block` into
    /// one (issue #298). Merged cells wholly inside become part of it. The words are the
    /// caller's to move: this keeps only which cells are one.
    pub fn merge(&mut self, block: usize, a: (usize, usize), b: (usize, usize)) -> Result<((usize, usize), (usize, usize)), MergeRefusal> {
        let (r0, c0, r1, c1) = (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
        if r0 == r1 && c0 == c1 {
            return Err(MergeRefusal::OneCell);
        }
        let inside = |s: &((usize, usize), (usize, usize))| {
            let (sr0, sc0, sr1, sc1) = reach(*s);
            sr0 >= r0 && sc0 >= c0 && sr1 <= r1 && sc1 <= c1
        };
        let meets = |s: &((usize, usize), (usize, usize))| {
            let (sr0, sc0, sr1, sc1) = reach(*s);
            sr0 <= r1 && sr1 >= r0 && sc0 <= c1 && sc1 >= c0
        };
        if self.spans(block).iter().any(|s| meets(s) && !inside(s)) {
            return Err(MergeRefusal::CutsAMergedCell);
        }
        if self.layout(block).is_none() {
            if self.tables.len() >= MAX_TABLES {
                return Err(MergeRefusal::TooMany);
            }
            self.put(Layout::new(block));
        }
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return Err(MergeRefusal::TooMany) };
        layout.spans.retain(|s| !inside(s));
        if layout.spans.len() >= MAX_SPANS {
            return Err(MergeRefusal::TooMany);
        }
        let span = ((r0, c0), (r1 - r0 + 1, c1 - c0 + 1));
        layout.spans.push(span);
        layout.spans.sort_by_key(|(at, _)| *at);
        Ok(span)
    }

    /// Splits the merged cell that `cell` is part of back into its cells (issue #298). True
    /// when there was one.
    pub fn split(&mut self, block: usize, cell: (usize, usize)) -> bool {
        let Some(span) = self.span(block, cell) else { return false };
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return false };
        layout.spans.retain(|s| *s != span);
        self.tidy();
        true
    }

    /// Moves or drops the aligned cells of one table, by `to`.
    fn move_cells(&mut self, block: usize, to: impl Fn((usize, usize)) -> Option<(usize, usize)>) -> bool {
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return false };
        let before = layout.cells.clone();
        layout.cells = before.iter().filter_map(|(cell, a)| to(*cell).map(|moved| (moved, *a))).collect();
        layout.cells.sort_by_key(|(at, _)| *at);
        let changed = layout.cells != before;
        self.tidy();
        changed
    }

    /// The widths of the table at block `block`, or `None` when it was never dragged.
    pub fn of(&self, block: usize) -> Option<&[f32]> {
        self.layout(block).map(|l| l.widths.as_slice()).filter(|w| !w.is_empty())
    }

    /// True when the borders of the table at block `block` were hidden. A table has them
    /// until that is asked for, and only that table is told (issue #161).
    pub fn borders_hidden(&self, block: usize) -> bool {
        self.layout(block).is_some_and(|l| l.borderless)
    }

    /// Hides the borders of the table at block `block`, or draws them again. True when that
    /// changed anything. A table that has neither hidden borders nor widths of its own keeps
    /// nothing, so showing them again takes its entry out of the page.
    pub fn set_borders(&mut self, block: usize, hidden: bool) -> bool {
        match self.tables.iter().position(|l| l.block == block) {
            Some(at) if self.tables[at].borderless == hidden => false,
            Some(at) => {
                self.tables[at].borderless = hidden;
                self.tidy();
                true
            }
            None if hidden && self.tables.len() < MAX_TABLES => {
                self.put(Layout { borderless: true, ..Layout::new(block) });
                true
            }
            None => false,
        }
    }

    /// Sets one column's width, leaving the others as they are. `all` is what the table's
    /// columns are at now, so a table dragged for the first time keeps the look it had.
    pub fn set(&mut self, block: usize, column: usize, width: f32, all: &[f32]) {
        if column >= all.len() || all.len() > MAX_COLUMNS || self.tables.len() >= MAX_TABLES && self.layout(block).is_none() {
            return;
        }
        let mut widths: Vec<f32> = all.iter().map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH)).collect();
        widths[column] = width.clamp(MIN_WIDTH, MAX_WIDTH);
        match self.tables.iter_mut().find(|l| l.block == block) {
            Some(have) => have.widths = widths,
            None => self.put(Layout { widths, ..Layout::new(block) }),
        }
    }

    /// True when no table has been given a layout of its own.
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }

    /// A block was put into the page at `at`. What is kept for a table at or after it moves
    /// down one with it, so it stays on the same table. True when anything changed.
    pub fn insert_block(&mut self, at: usize) -> bool {
        let mut moved = false;
        for layout in &mut self.tables {
            if layout.block >= at {
                layout.block += 1;
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
        // An aligned cell keeps its alignment, moved along with its column (issue #170).
        let spans = self.move_spans(block, |((r, c), (h, w))| {
            let (c, w) = grow((c, w), at);
            Some(((r, c), (h, w)))
        });
        let cells = self.move_cells(block, |(r, c)| Some((r, if c >= at { c + 1 } else { c }))) || spans;
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return cells };
        if layout.widths.is_empty() || layout.widths.len() >= MAX_COLUMNS {
            return cells;
        }
        let at = at.min(layout.widths.len());
        layout.widths.insert(at, width.clamp(MIN_WIDTH, MAX_WIDTH));
        true
    }

    /// Column `at` was taken out of the table at block `block`. Its width goes with it and
    /// the columns after it keep theirs. True when anything changed.
    pub fn remove_column(&mut self, block: usize, at: usize) -> bool {
        let spans = self.move_spans(block, |((r, c), (h, w))| shrink((c, w), at).map(|(c, w)| ((r, c), (h, w))));
        let cells = spans
            | self.move_cells(block, |(r, c)| match c.cmp(&at) {
                std::cmp::Ordering::Less => Some((r, c)),
                std::cmp::Ordering::Equal => None,
                std::cmp::Ordering::Greater => Some((r, c - 1)),
            });
        let Some(layout) = self.tables.iter_mut().find(|l| l.block == block) else { return cells };
        if at >= layout.widths.len() {
            return cells;
        }
        layout.widths.remove(at);
        self.tidy();
        true
    }

    /// The blocks in `gone` were taken off the page. What is kept for one of them goes, and
    /// what is kept for a table after them moves up with it, so it stays on the same table.
    /// True when anything changed.
    pub fn remove_blocks(&mut self, gone: std::ops::Range<usize>) -> bool {
        let before = self.tables.clone();
        let n = gone.len();
        self.tables.retain(|l| !gone.contains(&l.block));
        for layout in &mut self.tables {
            if layout.block >= gone.end {
                layout.block -= n;
            }
        }
        self.tables != before
    }
}

/// Reads the line a page keeps: `block:w,w,w`, one table after another, `;` between them. A
/// `!` after the widths says the table's borders are hidden, and may stand alone: `3:!`.
///
/// The cells of a table given something of their own follow as a part of their own,
/// `block@row.column=a,row.column=a`. `a` is an alignment, `l`, `c` or `r` (issue #170), then a
/// shading, `~` and a colour's name (issue #169): `1.2=c~yellow`, or `1.2=~yellow` for a cell
/// with only a colour. The part has no `:` in it, so a NoteCraft from before it skips it and
/// keeps the widths.
///
/// Anything that does not read as that is dropped, because layout is a convenience and a
/// page must open whatever its metadata says.
pub fn parse(text: &str) -> Widths {
    let mut tables: Vec<Layout> = Vec::new();
    let mut aligned: Vec<(usize, Vec<((usize, usize), Cell)>)> = Vec::new();
    let mut merged: Vec<(usize, Vec<((usize, usize), (usize, usize))>)> = Vec::new();
    for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()).take(MAX_TABLES * 3) {
        // Merged cells (issue #298): `block+row.column=RxC`. No `:` and no `@`, so a NoteCraft
        // from before reads past it. Not `#`, which front matter reads as a comment.
        if !part.contains(':')
            && !part.contains('@')
            && let Some((at, list)) = part.split_once('+')
        {
            let Ok(at) = at.trim().parse::<usize>() else { continue };
            if merged.iter().any(|(have, _)| *have == at) {
                continue;
            }
            let mut spans: Vec<((usize, usize), (usize, usize))> = Vec::new();
            for entry in list.split(',').take(MAX_SPANS) {
                let read = || -> Option<((usize, usize), (usize, usize))> {
                    let (cell, size) = entry.split_once('=')?;
                    let (r, c) = cell.trim().split_once('.')?;
                    let (h, w) = size.trim().split_once('x')?;
                    let span = ((r.trim().parse().ok()?, c.trim().parse().ok()?), (h.trim().parse().ok()?, w.trim().parse().ok()?));
                    let ((_, _), (h, w)): ((usize, usize), (usize, usize)) = span;
                    (h >= 1 && w >= 1 && h * w > 1 && h <= 10_000 && w <= MAX_COLUMNS).then_some(span)
                };
                let Some(span) = read() else { continue };
                // Two merged cells that overlap cannot both be: the first one read is kept.
                let (r0, c0, r1, c1) = reach(span);
                let clash = spans.iter().any(|s| {
                    let (sr0, sc0, sr1, sc1) = reach(*s);
                    sr0 <= r1 && sr1 >= r0 && sc0 <= c1 && sc1 >= c0
                });
                if !clash {
                    spans.push(span);
                }
            }
            if !spans.is_empty() {
                spans.sort_by_key(|(cell, _)| *cell);
                merged.push((at, spans));
            }
            continue;
        }
        if !part.contains(':')
            && let Some((at, list)) = part.split_once('@')
        {
            let Ok(at) = at.trim().parse::<usize>() else { continue };
            if aligned.iter().any(|(have, _)| *have == at) {
                continue;
            }
            let cells: Vec<((usize, usize), Cell)> = list
                .split(',')
                .take(MAX_CELLS)
                .filter_map(|entry| {
                    let (cell, what) = entry.split_once('=')?;
                    let (r, c) = cell.trim().split_once('.')?;
                    let (a, shade) = match what.split_once('~') {
                        Some((a, shade)) => (a, Some(crate::NodeColor::parse(shade)?)),
                        None => (what, None),
                    };
                    let align = if a.trim().is_empty() { None } else { Some(Align::from_letter(a)?) };
                    let kept = Cell { align, shade };
                    (!kept.is_bare()).then_some(((r.trim().parse().ok()?, c.trim().parse().ok()?), kept))
                })
                .collect();
            if !cells.is_empty() {
                aligned.push((at, cells));
            }
            continue;
        }
        let Some((at, list)) = part.split_once(':') else { continue };
        let Ok(at) = at.trim().parse::<usize>() else { continue };
        let (list, borderless) = match list.trim_end().strip_suffix('!') {
            Some(list) => (list, true),
            None => (list, false),
        };
        let widths: Vec<f32> = list
            .split(',')
            .map(str::trim)
            .filter(|w| !w.is_empty())
            .take(MAX_COLUMNS)
            .filter_map(|w| w.parse::<f32>().ok())
            .map(|w| w.clamp(MIN_WIDTH, MAX_WIDTH))
            .collect();
        if widths.is_empty() && !borderless || tables.iter().any(|have| have.block == at) {
            continue;
        }
        tables.push(Layout { widths, borderless, ..Layout::new(at) });
    }
    for (at, mut cells) in aligned {
        if tables.len() >= MAX_TABLES && !tables.iter().any(|l| l.block == at) {
            continue;
        }
        cells.sort_by_key(|(cell, _)| *cell);
        cells.dedup_by_key(|(cell, _)| *cell);
        match tables.iter_mut().find(|l| l.block == at) {
            Some(layout) => layout.cells = cells,
            None => tables.push(Layout { cells, ..Layout::new(at) }),
        }
    }
    for (at, spans) in merged {
        if tables.len() >= MAX_TABLES && !tables.iter().any(|l| l.block == at) {
            continue;
        }
        match tables.iter_mut().find(|l| l.block == at) {
            Some(layout) => layout.spans = spans,
            None => tables.push(Layout { spans, ..Layout::new(at) }),
        }
    }
    tables.sort_by_key(|l| l.block);
    Widths { tables }
}

/// Writes them back as that line. An empty set writes nothing, which takes the line out.
pub fn write(widths: &Widths) -> String {
    let mut parts = Vec::new();
    for l in &widths.tables {
        if !l.widths.is_empty() || l.borderless {
            let list: Vec<String> = l.widths.iter().map(|w| format!("{}", w.round() as i64)).collect();
            parts.push(format!("{}:{}{}", l.block, list.join(","), if l.borderless { "!" } else { "" }));
        }
        if !l.cells.is_empty() {
            let list: Vec<String> = l
                .cells
                .iter()
                .map(|((r, c), cell)| {
                    let a = cell.align.map(|a| a.letter().to_string()).unwrap_or_default();
                    let shade = cell.shade.map(|s| format!("~{}", s.id())).unwrap_or_default();
                    format!("{r}.{c}={a}{shade}")
                })
                .collect();
            parts.push(format!("{}@{}", l.block, list.join(",")));
        }
        if !l.spans.is_empty() {
            let list: Vec<String> = l.spans.iter().map(|((r, c), (h, w))| format!("{r}.{c}={h}x{w}")).collect();
            parts.push(format!("{}+{}", l.block, list.join(",")));
        }
    }
    parts.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #161: hiding the borders is a fact about one table, kept with it.
    #[test]
    fn borders_are_hidden_for_one_table_and_leave_the_others() {
        let mut widths = parse("1:100,100; 6:120,200");
        assert!(!widths.borders_hidden(1) && !widths.borders_hidden(6), "every table has its borders until one is hidden");
        assert!(widths.set_borders(1, true));
        assert!(widths.borders_hidden(1));
        assert!(!widths.borders_hidden(6), "another table is left alone");
        assert_eq!(widths.of(1), Some([100.0, 100.0].as_slice()), "the widths stay");
        assert_eq!(write(&widths), "1:100,100!; 6:120,200");
        assert!(!widths.set_borders(1, true), "hiding what is hidden changes nothing");
        assert!(widths.set_borders(1, false));
        assert_eq!(write(&widths), "1:100,100; 6:120,200");
    }

    #[test]
    fn a_table_with_hidden_borders_and_no_dragged_widths_is_kept() {
        let mut widths = Widths::default();
        assert!(widths.set_borders(3, true));
        assert!(!widths.is_empty());
        assert_eq!(widths.of(3), None, "no widths of its own, so the usual ones are drawn");
        assert_eq!(write(&widths), "3:!");
        assert_eq!(parse("3:!"), widths, "and it reads back");
        // Showing them again leaves nothing to keep, so the line goes away.
        assert!(widths.set_borders(3, false));
        assert!(widths.is_empty());
        assert_eq!(write(&widths), "");
        assert!(!widths.set_borders(3, false), "showing what is shown changes nothing");
    }

    #[test]
    fn hidden_borders_follow_their_table_and_survive_width_changes() {
        let mut widths = parse("2:!; 5:100,100!");
        assert!(widths.insert_block(3));
        assert!(widths.borders_hidden(2) && widths.borders_hidden(6), "a block put in after the first table moves only the second");
        assert!(widths.remove_blocks(0..1));
        assert!(widths.borders_hidden(1) && widths.borders_hidden(5));
        // A drag on a table whose borders are hidden keeps them hidden.
        widths.set(5, 0, 150.0, &[100.0, 100.0]);
        assert_eq!(write(&widths), "1:!; 5:150,100!");
        // A column put into a table with no widths of its own invents none.
        assert!(!widths.insert_column(1, 0, 160.0));
        assert_eq!(widths.of(1), None);
        // The table goes and its borders go with it.
        assert!(widths.remove_blocks(1..2));
        assert_eq!(write(&widths), "4:150,100!");
    }

    #[test]
    fn a_flag_without_a_table_is_read_without_harm() {
        assert!(parse("!").is_empty());
        assert!(parse("x:!").is_empty());
        assert_eq!(parse("1:bad!").of(1), None);
        assert!(parse("1:bad!").borders_hidden(1), "the flag is read even when the widths are not");
        assert!(parse("1:bad,200").of(1).is_some() && !parse("1:bad,200").borders_hidden(1));
    }

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

    /// Issue #170: a cell's alignment is kept with its table, and written as a part a reader
    /// from before alignment skips.
    #[test]
    fn cell_alignment_is_kept_and_read_back() {
        let mut widths = parse("2:100,100");
        assert_eq!(widths.align(2, (1, 0)), None, "a cell is aligned the usual way until told");
        assert!(widths.set_align(2, &[(1, 0), (0, 1)], Some(Align::Center)));
        assert!(widths.set_align(5, &[(2, 2)], Some(Align::Right)), "a table with nothing kept yet");
        assert_eq!(widths.align(2, (1, 0)), Some(Align::Center));
        assert_eq!(widths.align(2, (1, 1)), None, "only the cells asked for");
        let line = write(&widths);
        assert_eq!(line, "2:100,100; 2@0.1=c,1.0=c; 5@2.2=r");
        assert_eq!(parse(&line), widths, "and it reads back");
        assert!(!widths.set_align(2, &[(1, 0)], Some(Align::Center)), "aligning what is aligned changes nothing");
        // The usual alignment again: the cell keeps nothing, and a table left with nothing goes.
        assert!(widths.set_align(5, &[(2, 2)], None));
        assert!(widths.set_align(2, &[(1, 0), (0, 1)], None));
        assert_eq!(write(&widths), "2:100,100");
        assert!(!widths.set_align(7, &[(0, 0)], None), "nothing to take off a table with nothing kept");
        assert!(widths.of(7).is_none() && !widths.borders_hidden(7));
    }

    #[test]
    fn an_older_reader_keeps_the_widths_of_an_aligned_table() {
        // What a NoteCraft from before alignment does with the line: a part without a `:` is
        // skipped, so the widths and the borders are read as they were.
        let line = "3:120,200!; 3@1.1=r";
        let older: Vec<&str> = line.split(';').map(str::trim).filter(|p| p.contains(':')).collect();
        assert_eq!(older, vec!["3:120,200!"]);
        let widths = parse(line);
        assert_eq!(widths.of(3), Some([120.0, 200.0].as_slice()));
        assert!(widths.borders_hidden(3));
        assert_eq!(widths.align(3, (1, 1)), Some(Align::Right));
        // The aligned part may come first.
        assert_eq!(parse("3@1.1=r; 3:120,200!"), widths);
    }

    #[test]
    fn aligned_cells_follow_their_rows_and_columns() {
        let mut widths = Widths::default();
        widths.set_align(1, &[(0, 0), (1, 1), (2, 2)], Some(Align::Right));
        // A row put in above row 1 moves rows 1 and 2 down.
        assert!(widths.insert_row(1, 1));
        assert_eq!(write(&widths), "1@0.0=r,2.1=r,3.2=r");
        // Row 2 goes, and its cell with it; row 3 moves up.
        assert!(widths.remove_row(1, 2));
        assert_eq!(write(&widths), "1@0.0=r,2.2=r");
        // A column put in at the front moves every cell along; one taken out takes its cells.
        assert!(widths.insert_column(1, 0, 160.0));
        assert_eq!(write(&widths), "1@0.1=r,2.3=r", "a table with no widths gains none");
        assert!(widths.remove_column(1, 1));
        assert_eq!(write(&widths), "1@2.2=r");
        // Rows put in a new order take their cells: row 2 is now row 1.
        assert!(widths.reorder_rows(1, &[0, 2, 1]));
        assert_eq!(write(&widths), "1@1.2=r");
        // The table moves with its block, and goes with it.
        assert!(widths.insert_block(0));
        assert_eq!(widths.align(2, (1, 2)), Some(Align::Right));
        assert!(widths.remove_blocks(2..3));
        assert!(widths.is_empty());
    }

    #[test]
    fn hiding_and_showing_borders_keeps_the_alignment() {
        let mut widths = Widths::default();
        widths.set_align(4, &[(1, 0)], Some(Align::Center));
        assert!(widths.set_borders(4, true));
        assert!(widths.set_borders(4, false), "showing the borders again");
        assert_eq!(widths.align(4, (1, 0)), Some(Align::Center), "does not drop the table's alignment");
        assert_eq!(write(&widths), "4@1.0=c");
    }

    /// Issue #169: a cell's shading is kept beside its alignment, follows the cell, and goes
    /// without taking the alignment with it.
    #[test]
    fn cell_shading_is_kept_beside_alignment() {
        use crate::NodeColor;
        let mut widths = parse("2:100,100");
        assert_eq!(widths.shade(2, (1, 0)), None, "a cell has no colour until given one");
        assert!(widths.set_shade(2, &[(1, 0), (1, 1)], Some(NodeColor::Yellow)));
        assert!(widths.set_align(2, &[(1, 1)], Some(Align::Right)));
        assert_eq!(write(&widths), "2:100,100; 2@1.0=~yellow,1.1=r~yellow");
        assert_eq!(parse(&write(&widths)), widths, "and it reads back");
        assert_eq!(widths.shade(2, (1, 1)), Some(NodeColor::Yellow));
        assert_eq!(widths.align(2, (1, 1)), Some(Align::Right));
        assert!(!widths.set_shade(2, &[(1, 0)], Some(NodeColor::Yellow)), "the same colour again changes nothing");
        // Another colour replaces it; No Colour takes it away and leaves the alignment.
        assert!(widths.set_shade(2, &[(1, 1)], Some(NodeColor::Blue)));
        assert!(widths.set_shade(2, &[(1, 0), (1, 1)], None));
        assert_eq!(write(&widths), "2:100,100; 2@1.1=r");
        // A shaded cell follows its row like an aligned one, and a table can have only colours.
        assert!(widths.set_shade(6, &[(2, 0)], Some(NodeColor::Green)));
        assert!(widths.insert_row(6, 1));
        assert_eq!(widths.shade(6, (3, 0)), Some(NodeColor::Green));
        assert!(!widths.set_shade(9, &[(0, 0)], None), "nothing to take off a table with nothing kept");
    }

    #[test]
    fn nonsense_alignment_is_dropped_not_obeyed() {
        assert!(parse("2@").is_empty());
        assert!(parse("x@1.1=c").is_empty());
        assert!(parse("2@1.1=sideways").is_empty());
        assert!(parse("2@1=c").is_empty());
        assert_eq!(parse("2@1.1=c,bad,0.0=r").align(2, (0, 0)), Some(Align::Right), "what can be read is kept");
        assert_eq!(parse("2@1.1=c; 2@1.1=r").align(2, (1, 1)), Some(Align::Center), "the first entry for a table wins");
        assert!(parse("2@1.1=~").is_empty(), "a shading with no colour is nothing");
        assert!(parse("2@1.1=~plaid").is_empty(), "nor is a colour with no name it knows");
        assert!(parse("2@1.1=").is_empty());
        assert_eq!(parse("2@1.1=c~plaid,0.0=~red").shade(2, (0, 0)), Some(crate::NodeColor::Red));
    }

    #[test]
    fn nonsense_in_the_page_is_dropped_not_obeyed() {
        assert!(parse("").is_empty());
        assert!(parse("sideways").is_empty());
        assert!(parse("2:").is_empty(), "a table with no widths is no entry");
        assert_eq!(parse("1:bad,200").of(1), Some([200.0].as_slice()), "what can be read is kept");
        assert_eq!(parse("1:100; 1:200").of(1), Some([100.0].as_slice()), "the first entry for a table wins");
    }

    /// Issue #298: a merged cell is kept with its table, read back the same, and skipped by a
    /// NoteCraft that knows nothing of it.
    #[test]
    fn merged_cells_are_written_and_read_back() {
        let mut widths = parse("2:100,120,140");
        assert_eq!(widths.merge(2, (1, 2), (0, 1)), Ok(((0, 1), (2, 2))), "any two corners make the same block");
        assert_eq!(write(&widths), "2:100,120,140; 2+0.1=2x2");
        assert_eq!(parse(&write(&widths)), widths);
        // Front matter cuts a value at `#`, so the line must not need one.
        assert_eq!(parse(&crate::kv::unquote(&write(&widths))), widths, "survives being read from front matter");
        assert_eq!(widths.owner(2, (1, 2)), (0, 1));
        assert!(widths.covered(2, (1, 1)) && !widths.covered(2, (0, 1)) && !widths.covered(2, (0, 0)));
        assert_eq!(widths.of(2), Some([100.0, 120.0, 140.0].as_slice()), "the widths stay");
        // Junk, a 1 by 1 and an overlap are dropped; the page still opens.
        let odd = parse("4+0.0=1x1,0.0=1x2,0.1=2x1,x.y=2x2,3.3=0x4");
        assert_eq!(odd.spans(4), &[((0, 0), (1, 2))]);
        assert!(widths.split(2, (1, 1)), "split from any of its cells");
        assert!(widths.spans(2).is_empty());
        assert!(!widths.split(2, (1, 1)), "nothing left to split");
        assert_eq!(write(&widths), "2:100,120,140");
    }

    #[test]
    fn a_merge_that_cuts_through_a_merged_cell_is_refused_and_one_that_holds_it_takes_it_in() {
        let mut widths = Widths::default();
        assert_eq!(widths.merge(1, (1, 1), (1, 1)), Err(MergeRefusal::OneCell));
        assert!(widths.is_empty(), "a refused merge keeps nothing");
        assert_eq!(widths.merge(1, (1, 1), (2, 2)), Ok(((1, 1), (2, 2))));
        assert_eq!(widths.merge(1, (0, 0), (1, 1)), Err(MergeRefusal::CutsAMergedCell));
        assert_eq!(widths.whole_block(1, (0, 0), (1, 1)), ((0, 0), (2, 2)), "the block a drag picks grows to hold it");
        assert_eq!(widths.merge(1, (0, 0), (2, 3)), Ok(((0, 0), (3, 4))));
        assert_eq!(widths.spans(1), &[((0, 0), (3, 4))], "the one inside became part of the new one");
    }

    #[test]
    fn merged_cells_follow_rows_and_columns_put_in_and_taken_out() {
        let mut widths = Widths::default();
        widths.merge(3, (1, 1), (2, 2)).expect("merge");
        assert!(widths.insert_row(3, 2), "a row inside makes it taller");
        assert_eq!(widths.spans(3), &[((1, 1), (3, 2))]);
        assert!(widths.insert_row(3, 0));
        assert_eq!(widths.spans(3), &[((2, 1), (3, 2))], "a row above moves it down");
        assert!(widths.insert_column(3, 2, 100.0));
        assert_eq!(widths.spans(3), &[((2, 1), (3, 3))], "a column inside makes it wider");
        assert!(widths.remove_column(3, 0));
        assert_eq!(widths.spans(3), &[((2, 0), (3, 3))], "a column before it taken out moves it back");
        assert!(widths.remove_row(3, 2));
        assert!(widths.remove_row(3, 2));
        assert_eq!(widths.spans(3), &[((2, 0), (1, 3))]);
        assert!(widths.remove_column(3, 0) && widths.remove_column(3, 0));
        assert!(widths.spans(3).is_empty(), "1 by 1 is an ordinary cell again");
        assert!(widths.is_empty());
    }

    #[test]
    fn sorting_keeps_a_cell_merged_along_its_row_and_splits_one_across_rows() {
        let mut widths = Widths::default();
        widths.merge(0, (1, 0), (1, 1)).expect("merge");
        widths.merge(0, (2, 2), (3, 2)).expect("merge");
        // Rows 1, 2, 3 become 3, 1, 2.
        assert!(widths.reorder_rows(0, &[0, 3, 1, 2]));
        assert_eq!(widths.spans(0), &[((2, 0), (1, 2))]);
    }
}

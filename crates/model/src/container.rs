//! Containers: the level between a page and its paragraphs.
//!
//! A page in OneNote is not one column of text. It is a sheet with boxes on it, each holding
//! paragraphs, pictures and tables, each placed where the writer put it. NoteCraft calls
//! those containers, and this is what one is: a place on the page, a width, and how many of
//! the page's blocks belong to it.
//!
//! **Why only a count.** The blocks themselves stay where they are, in the page's own HTML or
//! Markdown, in reading order. A container says "the next three blocks are mine". So a page
//! read by anything else, or edited in another program, is still the same document in the
//! same order; the boxes are a layer over it, kept in the page's metadata. If the counts and
//! the blocks ever disagree, [`fit`] puts everything in one container rather than losing a
//! word.

/// One box on the page.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Container {
    /// Points from the left edge of the page.
    pub x: f32,
    /// Points from the top of the page.
    pub y: f32,
    /// How wide the box is, in points.
    pub width: f32,
    /// How many of the page's blocks, in order, are in this box.
    pub blocks: usize,
}

/// The narrowest a container may be, so one cannot be dragged into nothing.
pub const MIN_WIDTH: f32 = 120.0;

impl Container {
    pub fn new(x: f32, y: f32, width: f32, blocks: usize) -> Container {
        Container { x: x.max(0.0), y: y.max(0.0), width: width.max(MIN_WIDTH), blocks }
    }
}

/// Reads the line a page keeps its containers on: `x,y,width,blocks` per box, separated by
/// semicolons. Anything that does not parse is skipped rather than guessed at.
pub fn parse(text: &str) -> Vec<Container> {
    text.split(';')
        .filter_map(|part| {
            let mut fields = part.split(',').map(str::trim);
            let x: f32 = fields.next()?.parse().ok()?;
            let y: f32 = fields.next()?.parse().ok()?;
            let width: f32 = fields.next()?.parse().ok()?;
            let blocks: usize = fields.next()?.parse().ok()?;
            if fields.next().is_some() || !x.is_finite() || !y.is_finite() || !width.is_finite() {
                return None;
            }
            Some(Container::new(x, y, width, blocks))
        })
        .collect()
}

/// Writes them back out the same way. Whole points: a page is not a drawing program, and
/// the shorter line is the one a person can read in the file.
pub fn write(containers: &[Container]) -> String {
    containers
        .iter()
        .map(|c| format!("{},{},{},{}", c.x.round() as i64, c.y.round() as i64, c.width.round() as i64, c.blocks))
        .collect::<Vec<_>>()
        .join("; ")
}

/// The containers to use for a page with `blocks` blocks in it.
///
/// This is where honesty about a count-based layer lives. If the containers account for
/// exactly the blocks there are, they are used as they are. If they do not, because the page
/// was edited somewhere else, everything goes into one container and nothing is lost.
pub fn fit(containers: &[Container], blocks: usize, width: f32) -> Vec<Container> {
    let counted: usize = containers.iter().map(|c| c.blocks).sum();
    if !containers.is_empty() && counted == blocks {
        return containers.to_vec();
    }
    vec![Container::new(0.0, 0.0, width, blocks)]
}

/// Which container a block belongs to, and which block of that container it is.
pub fn place(containers: &[Container], block: usize) -> Option<(usize, usize)> {
    let mut first = 0;
    for (i, c) in containers.iter().enumerate() {
        if block < first + c.blocks {
            return Some((i, block - first));
        }
        first += c.blocks;
    }
    None
}

/// The range of blocks in container `at`.
pub fn blocks_of(containers: &[Container], at: usize) -> std::ops::Range<usize> {
    let first: usize = containers.iter().take(at).map(|c| c.blocks).sum();
    let count = containers.get(at).map(|c| c.blocks).unwrap_or(0);
    first..first + count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_of_containers_round_trips() {
        let boxes = vec![Container::new(0.0, 0.0, 420.0, 3), Container::new(460.0, 24.0, 300.0, 2)];
        let line = write(&boxes);
        assert_eq!(line, "0,0,420,3; 460,24,300,2");
        assert_eq!(parse(&line), boxes);
    }

    #[test]
    fn nonsense_is_skipped_not_guessed_at() {
        assert!(parse("").is_empty());
        assert!(parse("garbage").is_empty());
        assert!(parse("1,2,3").is_empty(), "four fields or nothing");
        assert!(parse("1,2,3,4,5").is_empty());
        assert!(parse("x,y,z,w").is_empty());
        assert!(parse("1,2,NaN,4").is_empty(), "a box of no width is not a box");
        // One good box among the rubbish is still read.
        assert_eq!(parse("junk; 10,20,300,2").len(), 1);
    }

    #[test]
    fn a_box_has_a_floor_under_its_size() {
        let c = Container::new(-50.0, -10.0, 4.0, 1);
        assert_eq!((c.x, c.y), (0.0, 0.0), "a box stays on the page");
        assert_eq!(c.width, MIN_WIDTH, "and stays wide enough to write in");
    }

    #[test]
    fn containers_that_do_not_match_the_page_are_not_trusted() {
        let two = vec![Container::new(0.0, 0.0, 400.0, 2), Container::new(420.0, 0.0, 200.0, 1)];
        assert_eq!(fit(&two, 3, 700.0), two, "three blocks, three accounted for");
        // The page gained a block somewhere else: everything goes in one box instead.
        let fitted = fit(&two, 4, 700.0);
        assert_eq!(fitted.len(), 1);
        assert_eq!(fitted.first().map(|c| (c.blocks, c.width)), Some((4, 700.0)));
        // No containers at all is the same thing: one box holding the page.
        assert_eq!(fit(&[], 2, 700.0), vec![Container::new(0.0, 0.0, 700.0, 2)]);
    }

    #[test]
    fn a_block_knows_which_box_it_is_in() {
        let boxes = vec![Container::new(0.0, 0.0, 400.0, 2), Container::new(420.0, 0.0, 200.0, 3)];
        assert_eq!(place(&boxes, 0), Some((0, 0)));
        assert_eq!(place(&boxes, 1), Some((0, 1)));
        assert_eq!(place(&boxes, 2), Some((1, 0)));
        assert_eq!(place(&boxes, 4), Some((1, 2)));
        assert_eq!(place(&boxes, 5), None, "there is no sixth block");
        assert_eq!(blocks_of(&boxes, 0), 0..2);
        assert_eq!(blocks_of(&boxes, 1), 2..5);
        assert_eq!(blocks_of(&boxes, 9), 5..5);
    }
}

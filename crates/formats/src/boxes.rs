//! Boxes in the page's own text: each container is an element in the body.
//!
//! A page is a sheet of boxes ([`notecraft_model::Container`]): a place, a width, and the blocks
//! each one holds. The blocks stay in reading order, and the boxes are written around them as
//! elements, so the page's text says what the window shows, and any other program that opens
//! the file can see the structure:
//!
//! ```text
//! HTML     <div class="nc-box" data-x="40" data-y="20" data-width="320"
//!               style="position:absolute;left:40px;top:20px;width:320px">
//!            <h1>Meeting</h1><p>Agenda</p>
//!          </div>
//! Markdown <div class="nc-box" data-x="40" data-y="20" data-width="320">
//!
//!          # Meeting
//!
//!          Agenda
//!
//!          </div>
//! ```
//!
//! This is how OneNote's own HTML export lays a page out too: one positioned `div` per outline.
//! The `data-` attributes are what NoteCraft reads; the `style` is for a browser, so a page opens
//! looking like the window. In Markdown the blank lines after the opening tag and before the
//! closing one keep the inside ordinary Markdown.
//!
//! A page that was never arranged has no boxes in its text and is read as one column. Older pages
//! kept their boxes in the head or front matter; the store still reads those.

use crate::markdown::{self, Block};
use crate::{html, render};
use notecraft_model::{Container, PageFormat};

/// The class a box's element carries.
pub const CLASS: &str = "nc-box";

/// How wide a box is when text turns up outside any box and one has to be made for it.
const STRAY_WIDTH: f32 = 360.0;

/// One run of a body: a box, or text outside any box.
enum Piece {
    Stray(String),
    Box { x: f32, y: f32, width: f32, inner: String },
}

/// Reads a page body. The boxes are `None` when the body has none in its text: it is one column.
pub fn read(format: PageFormat, body: &str) -> (Vec<Block>, Option<Vec<Container>>) {
    let pieces = match format {
        PageFormat::Html => html_pieces(body),
        PageFormat::Cmgfm => markdown_pieces(body),
    };
    if !pieces.iter().any(|p| matches!(p, Piece::Box { .. })) {
        return (plain(format, body), None);
    }
    let mut blocks: Vec<Block> = Vec::new();
    let mut boxes: Vec<Container> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Box { x, y, width, inner } => {
                let inside = plain(format, &inner);
                boxes.push(Container::new(x, y, width, inside.len()));
                blocks.extend(inside);
            }
            Piece::Stray(text) => {
                let loose = plain(format, &text);
                if loose.is_empty() {
                    continue;
                }
                // Words outside every box (a hand edit) belong with the box before them, or
                // get one of their own at the top left when there is none.
                match boxes.last_mut() {
                    Some(last) => last.blocks += loose.len(),
                    None => boxes.push(Container::new(0.0, 0.0, STRAY_WIDTH, loose.len())),
                }
                blocks.extend(loose);
            }
        }
    }
    (blocks, Some(boxes))
}

/// Writes a page body. With `boxes`, each is an element around its blocks; without, the blocks
/// are written plain, one column. Boxes whose counts do not add up to the blocks are not
/// trusted, and the page is written plain rather than losing a block.
pub fn write(format: PageFormat, blocks: &[Block], boxes: Option<&[Container]>) -> String {
    let Some(boxes) = boxes.filter(|b| !b.is_empty() && b.iter().map(|c| c.blocks).sum::<usize>() == blocks.len()) else {
        return render_plain(format, blocks);
    };
    let mut at = 0;
    let mut out: Vec<String> = Vec::with_capacity(boxes.len());
    for c in boxes {
        let inside = blocks.get(at..at + c.blocks).unwrap_or_default();
        at += c.blocks;
        out.push(match format {
            PageFormat::Html => html_box(c, &render::blocks_to_html(inside)),
            PageFormat::Cmgfm => markdown_box(c, &render::blocks_to_markdown(inside)),
        });
    }
    let mut text = out.join("\n");
    if format == PageFormat::Cmgfm {
        text.push('\n');
    }
    text
}

/// True when the body has boxes written in its text.
pub fn has_boxes(format: PageFormat, body: &str) -> bool {
    read(format, body).1.is_some()
}

fn plain(format: PageFormat, text: &str) -> Vec<Block> {
    match format {
        PageFormat::Html => markdown::parse(&html::to_markdown(text)),
        PageFormat::Cmgfm => markdown::parse(text),
    }
}

fn render_plain(format: PageFormat, blocks: &[Block]) -> String {
    match format {
        PageFormat::Html => render::blocks_to_html(blocks),
        PageFormat::Cmgfm => render::blocks_to_markdown(blocks),
    }
}

/// Whole points: a page is not a drawing program, and the shorter number is the one a person
/// can read in the file.
fn pt(v: f32) -> i64 {
    v.round() as i64
}

fn html_box(c: &Container, inside: &str) -> String {
    let (x, y, w) = (pt(c.x), pt(c.y), pt(c.width));
    let open = format!(
        "<div class=\"{CLASS}\" data-x=\"{x}\" data-y=\"{y}\" data-width=\"{w}\" style=\"position:absolute;left:{x}px;top:{y}px;width:{w}px\">"
    );
    if inside.trim().is_empty() { format!("{open}</div>") } else { format!("{open}\n{}\n</div>", inside.trim_matches('\n')) }
}

fn markdown_box(c: &Container, inside: &str) -> String {
    let open = format!("<div class=\"{CLASS}\" data-x=\"{}\" data-y=\"{}\" data-width=\"{}\">", pt(c.x), pt(c.y), pt(c.width));
    if inside.trim().is_empty() { format!("{open}\n\n</div>\n") } else { format!("{open}\n\n{}\n\n</div>\n", inside.trim_matches('\n')) }
}

/// The value of an attribute in a tag, as a number. Missing or unreadable is zero.
fn number(tag: &str, name: &str) -> f32 {
    let key = format!("{name}=\"");
    let Some(start) = tag.find(&key).map(|i| i + key.len()) else { return 0.0 };
    let value = tag.get(start..).and_then(|rest| rest.split('"').next()).unwrap_or("");
    value.trim().parse::<f32>().ok().filter(|v| v.is_finite()).unwrap_or(0.0)
}

fn is_box_tag(tag: &str) -> bool {
    tag.starts_with("<div") && tag.contains(CLASS)
}

/// The next opening tag of a box in `text`: where it starts and where it ends (after the `>`).
fn find_box_open(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(found) = text.get(from..).and_then(|t| t.find("<div")) {
        let start = from + found;
        let end = start + text.get(start..)?.find('>')? + 1;
        let tag = text.get(start..end)?;
        // `<div` and not `<divider`.
        let after = tag.as_bytes().get(4).copied().unwrap_or(b'>');
        if (after == b'>' || after.is_ascii_whitespace()) && is_box_tag(tag) {
            return Some((start, end));
        }
        from = end;
    }
    None
}

/// Where the box that was just opened closes: the length of what is inside it and of the closing
/// tag. Boxes are not nested, but other `div`s inside one are counted so the right tag closes it.
fn matching_close(after: &str) -> Option<(usize, usize)> {
    let mut depth = 1usize;
    let mut at = 0;
    while let Some(found) = after.get(at..).and_then(|t| t.find('<')) {
        let lt = at + found;
        let rest = after.get(lt..)?;
        let tag_len = rest.find('>').map(|i| i + 1).unwrap_or(rest.len());
        if let Some(name) = rest.strip_prefix("</div") {
            if name.starts_with('>') || name.starts_with(|c: char| c.is_ascii_whitespace()) {
                depth -= 1;
                if depth == 0 {
                    return Some((lt, tag_len));
                }
            }
        } else if let Some(name) = rest.strip_prefix("<div")
            && (name.starts_with('>') || name.starts_with(|c: char| c.is_ascii_whitespace()))
        {
            depth += 1;
        }
        at = lt + tag_len.max(1);
    }
    None
}

fn html_pieces(body: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut rest = body;
    let mut stray = String::new();
    while let Some((start, end)) = find_box_open(rest) {
        stray.push_str(rest.get(..start).unwrap_or(""));
        let tag = rest.get(start..end).unwrap_or("");
        let after = rest.get(end..).unwrap_or("");
        let (inner_len, close_len) = matching_close(after).unwrap_or((after.len(), 0));
        if !stray.trim().is_empty() {
            pieces.push(Piece::Stray(std::mem::take(&mut stray)));
        }
        stray.clear();
        pieces.push(Piece::Box {
            x: number(tag, "data-x"),
            y: number(tag, "data-y"),
            width: number(tag, "data-width"),
            inner: after.get(..inner_len).unwrap_or("").to_string(),
        });
        rest = after.get(inner_len + close_len..).unwrap_or("");
    }
    stray.push_str(rest);
    if !stray.trim().is_empty() {
        pieces.push(Piece::Stray(stray));
    }
    pieces
}

fn fence(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("```") || t.starts_with("~~~")
}

fn markdown_pieces(body: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut stray: Vec<&str> = Vec::new();
    // The box being collected: its opening tag and the lines so far.
    let mut open: Option<(String, Vec<&str>)> = None;
    let mut in_fence = false;
    let flush = |stray: &mut Vec<&str>, pieces: &mut Vec<Piece>| {
        let text = stray.join("\n");
        stray.clear();
        if !text.trim().is_empty() {
            pieces.push(Piece::Stray(text));
        }
    };
    let finish = |tag: &str, lines: &[&str], pieces: &mut Vec<Piece>| {
        pieces.push(Piece::Box { x: number(tag, "data-x"), y: number(tag, "data-y"), width: number(tag, "data-width"), inner: lines.join("\n") });
    };
    for line in body.split('\n') {
        let t = line.trim();
        match open.as_mut() {
            None => {
                if !in_fence && is_box_tag(t) && t.ends_with('>') {
                    flush(&mut stray, &mut pieces);
                    open = Some((t.to_string(), Vec::new()));
                    continue;
                }
                if fence(line) {
                    in_fence = !in_fence;
                }
                stray.push(line);
            }
            Some((tag, lines)) => {
                if !in_fence && t == "</div>" {
                    finish(tag, lines, &mut pieces);
                    open = None;
                    continue;
                }
                if fence(line) {
                    in_fence = !in_fence;
                }
                lines.push(line);
            }
        }
    }
    // A box that was never closed keeps what was collected for it.
    if let Some((tag, lines)) = open {
        finish(&tag, &lines, &mut pieces);
    }
    flush(&mut stray, &mut pieces);
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD: &str = "# Meeting\n\nAgenda for today\n\n- one\n- two\n\nNotes go here\n";

    fn blocks() -> Vec<Block> {
        markdown::parse(MD)
    }

    fn two_boxes(n: usize) -> Vec<Container> {
        vec![Container::new(40.0, 20.0, 320.0, 2), Container::new(380.0, 50.0, 300.0, n - 2)]
    }

    #[test]
    fn a_body_with_no_boxes_is_one_column_and_says_so() {
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let body = render_plain(format, &blocks());
            let (read_blocks, boxes) = read(format, &body);
            assert_eq!(read_blocks, blocks(), "{format:?}: the same blocks");
            assert_eq!(boxes, None, "{format:?}: and no boxes");
            assert!(!has_boxes(format, &body));
        }
    }

    #[test]
    fn boxes_are_written_as_elements_and_read_back_in_both_formats() {
        let b = blocks();
        let boxes = two_boxes(b.len());
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let body = write(format, &b, Some(&boxes));
            assert_eq!(body.matches("class=\"nc-box\"").count(), 2, "{format:?}: one element per box:\n{body}");
            assert!(body.contains("data-x=\"40\"") && body.contains("data-width=\"300\""), "{format:?}: the place is in the text:\n{body}");
            let (read_blocks, read_boxes) = read(format, &body);
            assert_eq!(read_blocks, b, "{format:?}: the blocks come back");
            assert_eq!(read_boxes, Some(boxes.clone()), "{format:?}: and the boxes");
        }
    }

    #[test]
    fn an_html_box_is_positioned_so_a_browser_shows_the_arrangement() {
        let body = write(PageFormat::Html, &blocks(), Some(&two_boxes(blocks().len())));
        assert!(body.contains("position:absolute;left:40px;top:20px;width:320px"), "{body}");
        assert!(body.contains("<h1>Meeting</h1>"), "the words are ordinary HTML inside it:\n{body}");
    }

    #[test]
    fn a_markdown_box_keeps_ordinary_markdown_inside_it() {
        let body = write(PageFormat::Cmgfm, &blocks(), Some(&two_boxes(blocks().len())));
        assert!(body.starts_with("<div class=\"nc-box\" data-x=\"40\" data-y=\"20\" data-width=\"320\">\n\n# Meeting\n"), "{body}");
        assert!(body.contains("\n\n</div>\n"), "blank lines before the closing tag:\n{body}");
        assert!(body.contains("- one\n- two"), "lists are lists:\n{body}");
    }

    #[test]
    fn an_empty_box_survives() {
        let b = markdown::parse("only\n");
        let boxes = vec![Container::new(0.0, 0.0, 300.0, 1), Container::new(340.0, 0.0, 200.0, 0)];
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let (read_blocks, read_boxes) = read(format, &write(format, &b, Some(&boxes)));
            assert_eq!(read_blocks, b, "{format:?}");
            assert_eq!(read_boxes, Some(boxes.clone()), "{format:?}: the empty box is still a box");
        }
    }

    #[test]
    fn a_single_box_is_still_a_box() {
        let b = blocks();
        let boxes = vec![Container::new(10.0, 10.0, 500.0, b.len())];
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            assert_eq!(read(format, &write(format, &b, Some(&boxes))).1, Some(boxes.clone()), "{format:?}");
        }
    }

    #[test]
    fn words_outside_every_box_join_the_box_before_them() {
        let html = "<p>loose first</p><div class=\"nc-box\" data-x=\"50\" data-y=\"60\" data-width=\"300\"><p>in the box</p></div><p>loose after</p>";
        let (blocks, boxes) = read(PageFormat::Html, html);
        let boxes = boxes.unwrap_or_default();
        assert_eq!(blocks.len(), 3, "nothing is lost: {blocks:?}");
        assert_eq!(boxes.len(), 2, "the words before the first box got a box of their own");
        assert_eq!((boxes[0].x, boxes[0].blocks), (0.0, 1));
        assert_eq!((boxes[1].x, boxes[1].y, boxes[1].blocks), (50.0, 60.0, 2), "the words after join the box before them");

        let md = "loose first\n\n<div class=\"nc-box\" data-x=\"50\" data-y=\"60\" data-width=\"300\">\n\nin the box\n\n</div>\n\nloose after\n";
        let (blocks, boxes) = read(PageFormat::Cmgfm, md);
        assert_eq!(blocks.len(), 3);
        assert_eq!(boxes.map(|b| b.iter().map(|c| c.blocks).collect::<Vec<_>>()), Some(vec![1, 2]));
    }

    #[test]
    fn a_code_block_that_looks_like_a_box_is_only_code() {
        let b = markdown::parse("before\n\n```html\n<div class=\"nc-box\" data-x=\"1\" data-y=\"1\" data-width=\"200\">\n</div>\n```\n\nafter\n");
        let boxes = vec![Container::new(0.0, 0.0, 400.0, b.len())];
        let body = write(PageFormat::Cmgfm, &b, Some(&boxes));
        let (read_blocks, read_boxes) = read(PageFormat::Cmgfm, &body);
        assert_eq!(read_blocks, b, "the code block is intact:\n{body}");
        assert_eq!(read_boxes, Some(boxes), "and it did not make a box");
        // And the same in a page that was never arranged.
        let plain = write(PageFormat::Cmgfm, &b, None);
        assert_eq!(read(PageFormat::Cmgfm, &plain).1, None, "{plain}");
    }

    #[test]
    fn another_div_inside_a_box_does_not_end_it() {
        let html = "<div class=\"nc-box\" data-x=\"5\" data-y=\"5\" data-width=\"300\"><div><p>inner</p></div><p>after inner</p></div><p>outside</p>";
        let (blocks, boxes) = read(PageFormat::Html, html);
        assert_eq!(blocks.len(), 3, "{blocks:?}");
        assert_eq!(boxes.map(|b| b.iter().map(|c| c.blocks).collect::<Vec<_>>()), Some(vec![3]));
    }

    #[test]
    fn a_box_that_was_never_closed_keeps_its_words() {
        let (b, boxes) = read(PageFormat::Html, "<div class=\"nc-box\" data-x=\"1\" data-y=\"2\" data-width=\"300\"><p>cut off");
        assert_eq!(b.len(), 1);
        assert_eq!(boxes.map(|b| b.len()), Some(1));
        let (b, boxes) = read(PageFormat::Cmgfm, "<div class=\"nc-box\" data-x=\"1\" data-y=\"2\" data-width=\"300\">\n\ncut off\n");
        assert_eq!(b.len(), 1);
        assert_eq!(boxes.map(|b| b.len()), Some(1));
    }

    #[test]
    fn counts_that_do_not_fit_the_blocks_write_the_page_plain() {
        let b = blocks();
        let wrong = vec![Container::new(0.0, 0.0, 300.0, b.len() + 3)];
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let body = write(format, &b, Some(&wrong));
            assert!(!body.contains("nc-box"), "{format:?}: no block is dropped to make a wrong count fit:\n{body}");
            assert_eq!(read(format, &body).0, b);
        }
        assert!(!write(PageFormat::Html, &b, Some(&[])).contains("nc-box"), "no boxes at all is plain");
    }

    #[test]
    fn unreadable_places_are_zero_and_widths_are_kept_sensible() {
        let html = "<div class=\"nc-box\" data-x=\"abc\" data-width=\"5\"><p>x</p></div>";
        let boxes = read(PageFormat::Html, html).1.unwrap_or_default();
        assert_eq!((boxes[0].x, boxes[0].y), (0.0, 0.0), "garbage is zero");
        assert_eq!(boxes[0].width, notecraft_model::container::MIN_WIDTH, "a box cannot be narrower than the floor");
    }

    #[test]
    fn right_to_left_words_and_a_divider_tag_are_untouched() {
        let b = markdown::parse("مرحبا بالعالم\n\nsecond\n");
        let boxes = vec![Container::new(0.0, 0.0, 300.0, 1), Container::new(0.0, 80.0, 300.0, 1)];
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let (rb, rbx) = read(format, &write(format, &b, Some(&boxes)));
            assert_eq!(rb, b, "{format:?}");
            assert_eq!(rbx, Some(boxes.clone()));
        }
        // `<divider>` is not a div, and a div that is not a box is not one.
        assert_eq!(read(PageFormat::Html, "<divider class=\"nc-box\"></divider><p>x</p>").1, None);
        assert_eq!(read(PageFormat::Html, "<div class=\"other\"><p>x</p></div>").1, None);
    }
}

//! One OneNote page into NoteCraft blocks.
//!
//! The reader gives a page as lines of runs, pictures, files and tables. This turns them into
//! the block model both page formats are written from, and hands every picture and file to
//! the caller to store beside the page. Nothing here touches the file system.

use notecraft_formats::markdown::{Block, Span};
use notecraft_onenote::{Content, List, Page, Run, Store};

/// What the conversion could not carry over, counted for the report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Loss {
    /// Runs that were underlined. Neither format here has an underline mark.
    pub underlined_runs: usize,
    /// Runs OneNote marked as math. The file keeps no structure for them, so they are text.
    pub math_runs: usize,
    /// Pictures or files whose bytes are not in the section file.
    pub missing_data: usize,
    /// Pictures in a format a browser cannot show (for example Windows metafiles).
    pub unviewable_images: usize,
}

impl Loss {
    pub fn add(&mut self, other: Loss) {
        self.underlined_runs += other.underlined_runs;
        self.math_runs += other.math_runs;
        self.missing_data += other.missing_data;
        self.unviewable_images += other.unviewable_images;
    }
}

/// A converted page.
#[derive(Debug, Default)]
pub struct Converted {
    pub blocks: Vec<Block>,
    pub images: usize,
    pub files: usize,
    pub tables: usize,
    pub loss: Loss,
}

/// Stores a picture or file and returns the relative link to it, or `None` if it could not.
pub type Attach<'a> = &'a mut dyn FnMut(&str, &[u8]) -> Option<String>;

struct Ctx<'a, 'b> {
    store: &'a Store,
    data: &'a [u8],
    attach: Attach<'b>,
    out: Converted,
    numbers: Vec<usize>,
    next_image: usize,
}

/// Converts a page. `first_row_header` makes the first row of a table its header row, which a
/// CommonMark + GFM table needs; HTML tables do not, and a OneNote table has no header row.
pub fn convert_page(store: &Store, data: &[u8], page: &Page, first_row_header: bool, attach: Attach) -> Converted {
    let mut cx = Ctx { store, data, attach, out: Converted::default(), numbers: Vec::new(), next_image: 1 };
    for content in &page.blocks {
        cx.block(content, first_row_header);
    }
    cx.out
}

impl Ctx<'_, '_> {
    fn block(&mut self, content: &Content, first_row_header: bool) {
        match content {
            Content::Text { runs, list, depth } => {
                let spans = self.spans(runs);
                if spans.iter().all(|s| s.text.trim().is_empty()) {
                    return;
                }
                match list {
                    Some(kind) => {
                        let ordered = *kind == List::Number;
                        let depth = (*depth).min(8);
                        let number = self.number(ordered, depth);
                        self.out.blocks.push(Block::Item { ordered, number, task: None, depth, spans });
                    }
                    None => {
                        self.numbers.clear();
                        self.out.blocks.push(Block::Paragraph(spans));
                    }
                }
            }
            Content::Picture { .. } | Content::File { .. } => {
                self.numbers.clear();
                if let Some(span) = self.attachment(content) {
                    let block = match span.image.clone() {
                        Some(src) => Block::Image { alt: span.text, src },
                        None => Block::Paragraph(vec![span]),
                    };
                    self.out.blocks.push(block);
                }
            }
            Content::Table { rows } => {
                self.numbers.clear();
                self.table(rows, first_row_header);
            }
        }
    }

    /// The number of the next item at this depth of a numbered list; bullets do not count.
    fn number(&mut self, ordered: bool, depth: usize) -> usize {
        self.numbers.truncate(depth + 1);
        if !ordered {
            self.numbers.truncate(depth);
            return 1;
        }
        self.numbers.resize(depth + 1, 0);
        self.numbers[depth] += 1;
        self.numbers[depth]
    }

    fn spans(&mut self, runs: &[Run]) -> Vec<Span> {
        let mut spans = Vec::new();
        // A hyperlink can arrive as a field code in the text, `HYPERLINK "url"`, and the words
        // it applies to follow in the next run.
        let mut pending: Option<String> = None;
        for run in runs {
            let mut counted = false;
            for piece in field_pieces(&run.text) {
                let body = match piece {
                    Piece::Link(url) => {
                        pending = Some(url);
                        continue;
                    }
                    Piece::Text(t) => t,
                };
                let text = clean_text(body);
                if text.trim().is_empty() {
                    // Spaces between words stay; a bare field code leaves nothing.
                    if !text.is_empty() && pending.is_none() {
                        spans.push(Span { text, bold: run.bold, italic: run.italic, strike: run.strike, link: run.link.clone(), ..Span::default() });
                    }
                    continue;
                }
                if !counted {
                    counted = true;
                    if run.underline {
                        self.out.loss.underlined_runs += 1;
                    }
                    if run.math {
                        self.out.loss.math_runs += 1;
                    }
                }
                let link = run.link.clone().or_else(|| pending.take());
                spans.push(Span { text, bold: run.bold, italic: run.italic, strike: run.strike, link, ..Span::default() });
            }
        }
        spans
    }

    /// A picture as an image span, or a file as a link span. `None` when its bytes are gone.
    fn attachment(&mut self, content: &Content) -> Option<Span> {
        match content {
            Content::Picture { name, alt, file, ext } => {
                let Some(bytes) = file.as_deref().and_then(|r| self.store.file_bytes(self.data, r)) else {
                    self.out.loss.missing_data += 1;
                    return None;
                };
                let kind = sniff(bytes, ext);
                if !kind.viewable {
                    self.out.loss.unviewable_images += 1;
                }
                let file_name = if has_extension(name) { name.clone() } else { format!("image-{}.{}", self.next_image, kind.ext) };
                self.next_image += 1;
                let link = (self.attach)(&file_name, bytes)?;
                self.out.images += 1;
                let alt = if alt.trim().is_empty() { String::new() } else { clean_text(alt) };
                Some(Span { text: alt, image: Some(link), ..Span::default() })
            }
            Content::File { name, file, ext } => {
                let Some(bytes) = file.as_deref().and_then(|r| self.store.file_bytes(self.data, r)) else {
                    self.out.loss.missing_data += 1;
                    return None;
                };
                let mut file_name = if name.trim().is_empty() { "attachment".to_string() } else { name.clone() };
                if !has_extension(&file_name) {
                    let ext = ext.trim_start_matches('.');
                    if !ext.is_empty() {
                        file_name = format!("{file_name}.{ext}");
                    }
                }
                let link = (self.attach)(&file_name, bytes)?;
                self.out.files += 1;
                Some(Span { text: clean_text(&file_name), link: Some(link), ..Span::default() })
            }
            _ => None,
        }
    }

    fn table(&mut self, rows: &[Vec<Vec<Content>>], first_row_header: bool) {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        if width == 0 {
            return;
        }
        let mut cells: Vec<Vec<Vec<Span>>> = rows
            .iter()
            .map(|row| {
                let mut out: Vec<Vec<Span>> = row.iter().map(|cell| self.cell(cell)).collect();
                out.resize(width, Vec::new());
                out
            })
            .collect();
        if cells.iter().all(|r| r.iter().all(|c| c.is_empty())) {
            return;
        }
        let head = if first_row_header && !cells.is_empty() { cells.remove(0) } else { Vec::new() };
        self.out.tables += 1;
        self.out.blocks.push(Block::Table { head, rows: cells });
    }

    /// A cell holds lines of its own. A cell here is one run of spans, so the lines are
    /// joined by a space, and a table nested in a cell is flattened to its text.
    fn cell(&mut self, cell: &[Content]) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        for content in cell {
            let part: Vec<Span> = match content {
                Content::Text { runs, .. } => self.spans(runs),
                Content::Picture { .. } | Content::File { .. } => self.attachment(content).into_iter().collect(),
                Content::Table { rows } => rows.iter().flatten().flat_map(|c| self.cell(c)).collect(),
            };
            if part.is_empty() {
                continue;
            }
            if !spans.is_empty() {
                spans.push(Span { text: " ".to_string(), ..Span::default() });
            }
            spans.extend(part);
        }
        spans
    }
}

/// A piece of a run: words, or the address of a hyperlink field code that applies to the
/// words after it.
enum Piece<'a> {
    Text(&'a str),
    Link(String),
}

/// A run's text split at hyperlink field codes: the marker U+FDDF, `HYPERLINK`, the address in
/// quotes. A marker that is not followed by that is dropped by `clean_text` later.
fn field_pieces(text: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find('\u{FDDF}') {
        if at > 0 {
            out.push(Piece::Text(&rest[..at]));
        }
        let after = &rest[at + '\u{FDDF}'.len_utf8()..];
        let url = after
            .strip_prefix("HYPERLINK")
            .map(str::trim_start)
            .and_then(|t| t.strip_prefix('"'))
            .and_then(|t| t.find('"').map(|end| (&t[..end], &t[end + 1..])));
        match url {
            Some((url, tail)) => {
                out.push(Piece::Link(url.to_string()));
                rest = tail;
            }
            None => rest = after,
        }
    }
    if !rest.is_empty() {
        out.push(Piece::Text(rest));
    }
    out
}

/// Text with the control characters OneNote puts in runs (a vertical tab for a soft line
/// break, carriage returns, the object replacement character) turned into spaces, and runs
/// of those squeezed. A tab stays.
pub fn clean_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for c in text.chars() {
        // U+FDDF marks a field code and is not a character anyone sees.
        if c == '\u{FDDF}' {
            continue;
        }
        let odd = (c.is_control() && c != '\t') || c == '\u{2028}' || c == '\u{2029}' || c == '\u{FFFC}';
        if odd {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() && !out.ends_with(' ') && c != ' ' {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

fn has_extension(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) => !stem.is_empty() && !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()),
        None => false,
    }
}

/// What a picture's bytes say it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageKind {
    pub ext: &'static str,
    /// Whether a browser draws it.
    pub viewable: bool,
}

/// The picture format by its first bytes. The extension OneNote records is the fallback.
pub fn sniff(bytes: &[u8], recorded: &str) -> ImageKind {
    let kind = |ext, viewable| ImageKind { ext, viewable };
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        kind("png", true)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        kind("jpg", true)
    } else if bytes.starts_with(b"GIF8") {
        kind("gif", true)
    } else if bytes.starts_with(b"BM") {
        kind("bmp", true)
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        kind("webp", true)
    } else if bytes.starts_with(&[0x49, 0x49, 0x2A, 0x00]) || bytes.starts_with(&[0x4D, 0x4D, 0x00, 0x2A]) {
        kind("tif", false)
    } else if bytes.len() > 44 && bytes[0..4] == [1, 0, 0, 0] && &bytes[40..44] == b" EMF" {
        kind("emf", false)
    } else if bytes.starts_with(&[0xD7, 0xCD, 0xC6, 0x9A]) {
        kind("wmf", false)
    } else {
        match recorded.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "png" => kind("png", true),
            "jpg" | "jpeg" => kind("jpg", true),
            "gif" => kind("gif", true),
            "emf" => kind("emf", false),
            "wmf" => kind("wmf", false),
            _ => kind("bin", false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_breaks_and_object_marks_become_single_spaces() {
        assert_eq!(clean_text("one\u{b}two"), "one two");
        assert_eq!(clean_text("a\r\n\r\nb"), "a b");
        assert_eq!(clean_text("\u{FFFC}text"), "text");
        assert_eq!(clean_text("tab\there"), "tab\there");
        assert_eq!(clean_text("مرحبا\u{b}عالم"), "مرحبا عالم");
    }

    #[test]
    fn a_hyperlink_field_code_becomes_a_link_on_the_words_after_it() {
        let run = |t: &str| Run { text: t.into(), ..Run::default() };
        let store = empty_store();
        let page = Page {
            title: "t".into(),
            level: 1,
            blocks: vec![Content::Text {
                runs: vec![run("see \u{FDDF}HYPERLINK \"http://x.example/a\""), run("the page"), run(" and more")],
                list: None,
                depth: 0,
            }],
        };
        let mut attach = |_: &str, _: &[u8]| None;
        let out = convert_page(&store, &[], &page, false, &mut attach);
        let Some(Block::Paragraph(spans)) = out.blocks.first() else { panic!("a paragraph") };
        let shown: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(shown, "see the page and more");
        let linked: Vec<&str> = spans.iter().filter(|s| s.link.is_some()).map(|s| s.text.as_str()).collect();
        assert_eq!(linked, ["the page"]);
        assert_eq!(spans.iter().find_map(|s| s.link.as_deref()), Some("http://x.example/a"));
    }

    #[test]
    fn pictures_are_known_by_their_first_bytes() {
        assert_eq!(sniff(&[0x89, b'P', b'N', b'G', 0, 0], "").ext, "png");
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0], "bin").ext, "jpg");
        assert!(!sniff(&[0xD7, 0xCD, 0xC6, 0x9A, 0], "").viewable);
        assert_eq!(sniff(&[0, 1, 2], ".GIF").ext, "gif");
        assert_eq!(sniff(&[0, 1, 2], "").ext, "bin");
    }

    #[test]
    fn a_file_name_has_an_extension_only_when_it_looks_like_one() {
        assert!(has_extension("photo.png"));
        assert!(!has_extension("photo"));
        assert!(!has_extension(".png"));
        assert!(!has_extension("notes v2. final"));
    }

    #[test]
    fn numbered_items_count_per_depth_and_restart_after_other_blocks() {
        let store = empty_store();
        let item = |kind, depth| Content::Text { runs: vec![Run { text: "x".into(), ..Run::default() }], list: Some(kind), depth };
        let para = Content::Text { runs: vec![Run { text: "p".into(), ..Run::default() }], list: None, depth: 0 };
        let page = Page {
            title: "t".into(),
            level: 1,
            blocks: vec![item(List::Number, 0), item(List::Number, 0), item(List::Number, 1), item(List::Number, 0), para, item(List::Number, 0)],
        };
        let mut attach = |_: &str, _: &[u8]| None;
        let out = convert_page(&store, &[], &page, false, &mut attach);
        let numbers: Vec<usize> = out.blocks.iter().filter_map(|b| if let Block::Item { number, .. } = b { Some(*number) } else { None }).collect();
        assert_eq!(numbers, [1, 2, 1, 3, 1]);
    }

    #[test]
    fn a_table_is_padded_square_and_its_first_row_is_the_header_only_when_asked() {
        let store = empty_store();
        let cell = |t: &str| vec![Content::Text { runs: vec![Run { text: t.into(), ..Run::default() }], list: None, depth: 0 }];
        let page = Page { title: "t".into(), level: 1, blocks: vec![Content::Table { rows: vec![vec![cell("a"), cell("b")], vec![cell("c")]] }] };
        let mut attach = |_: &str, _: &[u8]| None;
        let md = convert_page(&store, &[], &page, true, &mut attach);
        let Some(Block::Table { head, rows }) = md.blocks.first() else { panic!("a table") };
        assert_eq!((head.len(), rows.len(), rows[0].len()), (2, 1, 2));
        let html = convert_page(&store, &[], &page, false, &mut attach);
        let Some(Block::Table { head, rows }) = html.blocks.first() else { panic!("a table") };
        assert_eq!((head.len(), rows.len()), (0, 2));
    }

    #[test]
    fn a_picture_whose_bytes_are_missing_is_counted_not_invented() {
        let store = empty_store();
        let page = Page {
            title: "t".into(),
            level: 1,
            blocks: vec![Content::Picture {
                name: "a.png".into(),
                alt: String::new(),
                file: Some("<ifndf>{00000000-0000-0000-0000-000000000000}".into()),
                ext: String::new(),
            }],
        };
        let mut attach = |_: &str, _: &[u8]| Some("assets/x".to_string());
        let out = convert_page(&store, &[], &page, false, &mut attach);
        assert!(out.blocks.is_empty());
        assert_eq!(out.loss.missing_data, 1);
    }

    fn empty_store() -> Store {
        let data = notecraft_onenote::testing::OneNote::new().finish();
        notecraft_onenote::read(&data).expect("an empty section reads")
    }
}

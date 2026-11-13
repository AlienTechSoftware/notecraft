//! Blocks back to text: the other half of the parsers, so a page can be edited as blocks
//! and written back in its own format.
//!
//! The pair `parse` and `render` round-trips: parsing what `render` writes gives the same
//! blocks again. That property is what makes it safe to edit a page as blocks, and it is
//! what the tests check, for both formats and for hostile text.

use crate::html::escape;
use crate::markdown::{Block, Span};

/// Blocks as CommonMark + GFM text.
pub fn blocks_to_markdown(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut last_was_item = false;
    for block in blocks {
        let is_item = matches!(block, Block::Item { .. });
        // A blank line between blocks, but list items stay together.
        if !out.is_empty() && !(is_item && last_was_item) {
            out.push('\n');
        }
        match block {
            Block::Heading { level, spans } => {
                out.push_str(&"#".repeat(usize::from((*level).clamp(1, 6))));
                out.push(' ');
                out.push_str(&spans_to_markdown(spans));
                out.push('\n');
            }
            Block::Paragraph(spans) => {
                out.push_str(&escape_block_start(&spans_to_markdown(spans)));
                out.push('\n');
            }
            Block::Item { ordered, number, task, depth, spans } => {
                out.push_str(&"  ".repeat(*depth));
                if *ordered {
                    out.push_str(&format!("{number}. "));
                } else {
                    out.push_str("- ");
                }
                if let Some(done) = task {
                    out.push_str(if *done { "[x] " } else { "[ ] " });
                }
                out.push_str(&escape_task_box(&spans_to_markdown(spans)));
                out.push('\n');
            }
            Block::Quote(spans) => {
                out.push_str("> ");
                out.push_str(&spans_to_markdown(spans));
                out.push('\n');
            }
            Block::Code { lang, text } => {
                let fence = crate::markdown::code_fence(text);
                out.push_str(&fence);
                out.push_str(lang);
                out.push('\n');
                out.push_str(text);
                if !text.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str(&fence);
                out.push('\n');
            }
            Block::MathBlock(tex) => {
                out.push_str("$$\n");
                out.push_str(tex.trim_end_matches('\n'));
                out.push_str("\n$$\n");
            }
            Block::Table { head, rows } => {
                out.push_str(&row_to_markdown(head));
                out.push_str(&format!("|{}\n", " --- |".repeat(head.len().max(1))));
                for r in rows {
                    out.push_str(&row_to_markdown(r));
                }
            }
            Block::Rule => out.push_str("---\n"),
            Block::Image { alt, src, width, height } => {
                if width.is_some() || height.is_some() {
                    // Escaped, so a quote in the text cannot end the attribute (issue #109).
                    let mut img_tag = format!("<img src=\"{}\" alt=\"{}\"", escape(src), escape(alt));
                    if let Some(w) = width {
                        img_tag.push_str(&format!(" width=\"{}\"", *w as i32));
                    }
                    if let Some(h) = height {
                        img_tag.push_str(&format!(" height=\"{}\"", *h as i32));
                    }
                    img_tag.push_str(">\n");
                    out.push_str(&img_tag);
                } else {
                    out.push_str(&format!("![{alt}]({})\n", destination(src)));
                }
            }
        }
        last_was_item = is_item;
    }
    out
}

/// Blocks as readable plain text for clipboard consumers that cannot accept rich content.
/// Formatting marks are omitted, while link targets remain visible when they differ from
/// their labels. This is deliberately distinct from Markdown source.
pub fn blocks_to_text(blocks: &[Block]) -> String {
    blocks.iter().map(block_to_text).collect::<Vec<_>>().join("\n")
}

fn block_to_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) => spans_to_text(spans),
        Block::Item { ordered, number, task, depth, spans } => {
            let marker = match task {
                Some(true) => "[x] ",
                Some(false) => "[ ] ",
                None if *ordered => return format!("{}{number}. {}", "  ".repeat(*depth), spans_to_text(spans)),
                None => "- ",
            };
            format!("{}{marker}{}", "  ".repeat(*depth), spans_to_text(spans))
        }
        Block::Code { text, .. } | Block::MathBlock(text) => text.trim_end_matches('\n').to_string(),
        Block::Table { head, rows } => std::iter::once(head)
            .chain(rows.iter())
            .map(|row| row.iter().map(|cell| spans_to_text(cell)).collect::<Vec<_>>().join("\t"))
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Rule => String::new(),
        Block::Image { alt, src, .. } => readable_link(alt, src),
    }
}

fn spans_to_text(spans: &[Span]) -> String {
    spans
        .iter()
        .map(|span| {
            if let Some(image) = &span.image {
                readable_link(&span.text, image)
            } else if let Some(target) = &span.link {
                readable_link(&span.text, target)
            } else {
                span.text.clone()
            }
        })
        .collect()
}

fn readable_link(label: &str, target: &str) -> String {
    if label.is_empty() || label == target { target.to_string() } else { format!("{label} ({target})") }
}

fn row_to_markdown(cells: &[Vec<Span>]) -> String {
    let mut s = String::from("|");
    for c in cells {
        s.push(' ');
        let mut literal = Literal::for_spans(c);
        let words: String = c.iter().filter(|span| plain_text(span)).map(|span| span.text.as_str()).collect();
        let lower = words.to_ascii_lowercase();
        literal.tag |= lower.contains("<br>") || lower.contains("<br ");
        s.push_str(&escape_table_cell(&table_line_breaks(&marked(c, 0, &literal))));
        s.push_str(" |");
    }
    s.push('\n');
    s
}

/// Inline spans as Markdown. The order of the marks matches what the parser reads back.
pub fn spans_to_markdown(spans: &[Span]) -> String {
    marked(spans, 0, &Literal::for_spans(spans))
}

/// Quotes the table's delimiters around an already serialized inline source.
pub(crate) fn escape_table_cell(text: &str) -> String {
    text.replace('\\', "\\\\").replace('|', "\\|")
}

/// Explicit breaks keep each table row on one source line. HTML folds bare carriage
/// returns, so the optional attribute preserves the original CSV line-ending bytes.
pub(crate) const TABLE_BREAKS: [(&str, &str); 3] = [("\r\n", "<br data-nc-break=\"crlf\">"), ("\r", "<br data-nc-break=\"cr\">"), ("\n", "<br>")];

pub(crate) fn table_line_breaks(text: &str) -> String {
    TABLE_BREAKS.iter().fold(text.to_string(), |text, (newline, tag)| text.replace(newline, tag))
}

/// Live-editor text and a character mask identifying literal punctuation. Formatting marks
/// remain editable, while persistence-only backslashes never enter the text widget.
pub fn editable_spans(spans: &[Span]) -> (String, Vec<bool>) {
    let literal = Literal { editor: true, ..Literal::default() };
    let text = marked(spans, 0, &literal);
    let mut masked = spans.to_vec();
    for span in &mut masked {
        if plain_text(span) {
            span.text = span.text.chars().map(|c| if matches!(c, '\\' | '*' | '~' | '$' | '`' | '[' | ']' | '<') { '\0' } else { c }).collect();
        }
    }
    let masked = marked(&masked, 0, &literal);
    let mask = text.chars().zip(masked.chars()).map(|(a, b)| a != b).collect();
    (text, mask)
}

/// The marks in the order they are opened: bold outermost, so `***x***` reads back the same way.
const MARKS: [(&str, fn(&Span) -> bool, fn(&mut Span)); 3] = [
    ("**", |s| s.bold && plain_text(s), |s| s.bold = false),
    ("*", |s| s.italic && plain_text(s), |s| s.italic = false),
    ("~~", |s| s.strike && plain_text(s), |s| s.strike = false),
];

/// Pictures, maths and code carry no emphasis of their own.
fn plain_text(s: &Span) -> bool {
    s.image.is_none() && !s.math && !s.code
}

/// Writes `spans` with the marks from `level` on. Neighbours that share a mark are written under one
/// pair of it (`**a *b* c**`, not `**a **` `***b***` `** c**`), and the white space at the edge of a
/// run goes outside its marks, which is what CommonMark needs for them to open and close.
fn marked(spans: &[Span], level: usize, literal: &Literal) -> String {
    let Some((marks, has, clear)) = MARKS.get(level) else {
        return spans.iter().map(|s| leaf(s, literal)).collect();
    };
    let mut out = String::new();
    let mut i = 0;
    while i < spans.len() {
        let on = spans.get(i).is_some_and(has);
        let mut j = i;
        while spans.get(j).is_some_and(|s| has(s) == on) {
            j += 1;
        }
        let run = spans.get(i..j).unwrap_or_default();
        if on {
            let inner: Vec<Span> = run
                .iter()
                .cloned()
                .map(|mut s| {
                    clear(&mut s);
                    s
                })
                .collect();
            let text = marked(&inner, level + 1, literal);
            let core = text.trim();
            if core.is_empty() {
                out.push_str(&text);
            } else {
                let lead = text.len() - text.trim_start().len();
                let trail = text.len() - text.trim_end().len();
                out.push_str(text.get(..lead).unwrap_or_default());
                out.push_str(&format!("{marks}{core}{marks}"));
                out.push_str(text.get(text.len() - trail..).unwrap_or_default());
            }
        } else {
            out.push_str(&marked(run, level + 1, literal));
        }
        i = j;
    }
    out
}

/// One span with nothing but its code, picture, maths or link written round it.
/// Inline code written so it reads back as the same text (issue #107). The delimiter is one
/// backtick more than the longest run inside, and a space pads each side when the code starts
/// or ends with a backtick, or starts and ends with a space, which the reader takes off again.
pub(crate) fn code_span(text: &str) -> String {
    let mut longest = 0;
    let mut run = 0;
    for c in text.chars() {
        run = if c == '`' { run + 1 } else { 0 };
        longest = longest.max(run);
    }
    let ticks = "`".repeat(longest + 1);
    let pad = text.starts_with('`') || text.ends_with('`') || (text.starts_with(' ') && text.ends_with(' ') && text.trim() != "");
    let pad = if pad { " " } else { "" };
    format!("{ticks}{pad}{text}{pad}{ticks}")
}

fn leaf(s: &Span, literal: &Literal) -> String {
    let styled = |text: String| inline_style(s, inline_marks(s, text));
    if let Some(src) = &s.image {
        let image = format!("![{}]({})", s.text, destination(src));
        let image = match &s.link {
            Some(target) => format!("[{image}]({})", destination(target)),
            None => image,
        };
        return styled(image);
    }
    if s.math {
        return styled(format!("${}$", s.text));
    }
    let text = if s.code { code_span(&s.text) } else { literal.escape(&s.text) };
    let text = match &s.link {
        Some(target) => format!("[{text}]({})", destination(target)),
        None => text,
    };
    styled(text)
}

/// A link or picture address as it is written between `(` and `)`. Balanced parentheses are
/// left as they are, as in `a(b).pdf`; when they do not pair up, each is written `\(` or `\)`
/// so the reader does not end the address at the wrong one (issue #108).
pub(crate) fn destination(target: &str) -> String {
    let mut depth = 0i32;
    let balanced = target.chars().all(|c| {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        depth >= 0
    }) && depth == 0;
    if balanced { target.to_string() } else { target.replace('(', "\\(").replace(')', "\\)") }
}

/// Which delimiters a run of spans must quote so its words read back as words.
///
/// The inline parser only treats `\\` before punctuation, `` ` ``, `*`, `~~`, `$`, `[`/`]` and `![`
/// as syntax, and each of those only forms markup when it has a partner (a second `$`, a closing
/// `*`, a mark written for a styled neighbour, a link's `](`). A delimiter with no possible
/// partner in the run is left alone, so ordinary text (`a_b`, `x < y`, `$5`, `2*3`) is written
/// and edited exactly as typed, and only text that would otherwise turn into markup gains a
/// backslash.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Literal {
    star: bool,
    tilde: bool,
    dollar: bool,
    tick: bool,
    bracket: bool,
    tag: bool,
    editor: bool,
}

impl Literal {
    /// Quote every delimiter: for text whose neighbours are not known, such as words inserted
    /// into the middle of a line that is written already.
    const ALL: Literal = Literal { star: true, tilde: true, dollar: true, tick: true, bracket: true, tag: true, editor: false };

    /// The quoting needed by `spans` written together as one line.
    fn for_spans(spans: &[Span]) -> Literal {
        let words = || spans.iter().filter(|s| plain_text(s)).flat_map(|s| s.text.chars());
        let count = |c: char| words().filter(|&x| x == c).count();
        let emphasis = spans.iter().any(|s| plain_text(s) && (s.bold || s.italic));
        let strike = spans.iter().any(|s| plain_text(s) && s.strike);
        let brackets = count('[') + count(']');
        let linked = spans.iter().any(|s| s.link.is_some() || s.image.is_some());
        // A bracket only starts a link when a `](` follows it; marks never fall between the two.
        let joined: String = words().collect();
        Literal {
            star: count('*') >= 2 || (count('*') >= 1 && emphasis),
            tilde: count('~') >= 2 || (count('~') >= 1 && strike),
            dollar: count('$') >= 2 || (count('$') >= 1 && spans.iter().any(|s| s.math)),
            tick: count('`') >= 2 || (count('`') >= 1 && spans.iter().any(|s| s.code)),
            editor: false,
            tag: ["<u>", "</u>", "<sub>", "</sub>", "<sup>", "</sup>", "<span ", "</span>"]
                .iter()
                .any(|tag| joined.to_ascii_lowercase().contains(tag)),
            bracket: (count('[') >= 1 && joined.contains("](")) || (brackets >= 1 && linked),
        }
    }

    /// How many characters each character of `text` takes when written: 2 if it is quoted.
    fn widths(&self, text: &str) -> Vec<usize> {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                let quoted = match c {
                    // A backslash escapes the punctuation after it. One at the end of a run could
                    // meet a mark or a neighbour's punctuation, so it is quoted too.
                    '\\' => chars.get(i + 1).is_none_or(|n| n.is_ascii_punctuation()),
                    '<' => self.tag,
                    '*' => self.star,
                    '~' => self.tilde,
                    '$' => self.dollar,
                    '`' => self.tick,
                    '[' | ']' => self.bracket,
                    _ => false,
                };
                1 + usize::from(quoted)
            })
            .collect()
    }

    /// `text` with the delimiters this run needs quoted.
    fn escape(&self, text: &str) -> String {
        if self.editor {
            return text.to_string();
        }
        let mut out = String::with_capacity(text.len());
        for (c, width) in text.chars().zip(self.widths(text)) {
            if width == 2 {
                out.push('\\');
            }
            out.push(c);
        }
        out
    }
}

/// Quote literal inline text whose neighbours are not known, so none of it reads as markup.
pub(crate) fn escape_markdown(text: &str) -> String {
    Literal::ALL.escape(text)
}

/// Words at the start of a list item that read like a task box (`[x] shipped`) stay words.
pub(crate) fn escape_task_box(text: &str) -> String {
    if ["[ ] ", "[x] ", "[X] "].iter().any(|m| text.starts_with(m)) { format!("\\{text}") } else { text.into() }
}

/// A paragraph must not turn into a heading, list, quote, rule or picture when serialized.
pub(crate) fn escape_block_start(text: &str) -> String {
    let t = text.trim_start();
    let rule = t.trim_end();
    let hashes = t.bytes().take_while(|c| *c == b'#').count();
    let heading = (1..=6).contains(&hashes) && t.as_bytes().get(hashes) == Some(&b' ');
    let digits = t.bytes().take_while(u8::is_ascii_digit).count();
    let ordered = digits > 0 && t.get(digits..).is_some_and(|s| s.starts_with(". ") || s.starts_with(") "));
    let bullet = ["- ", "+ ", "* "].iter().any(|m| t.starts_with(m));
    let ruled = rule.len() >= 3 && b"-_*".iter().any(|m| rule.bytes().all(|c| c == *m));
    let picture = rule.starts_with("<img ") && rule.ends_with('>');
    // A line that is a `div` tag would open or close a box around the page's blocks.
    let tag = rule.starts_with("<div") || rule.starts_with("</div");
    if heading || bullet || ordered || ruled || picture || tag || t.starts_with('>') {
        let at = text.len() - t.len() + if ordered { digits } else { 0 };
        let mut out = text.to_owned();
        out.insert(at, '\\');
        out
    } else {
        text.into()
    }
}

fn inline_marks(s: &Span, mut text: String) -> String {
    if s.underline {
        text = format!("<u>{text}</u>");
    }
    if let Some(script) = s.script {
        let tag = match script {
            crate::markdown::Script::Subscript => "sub",
            crate::markdown::Script::Superscript => "sup",
        };
        text = format!("<{tag}>{text}</{tag}>");
    }
    text
}

fn inline_style(s: &Span, text: String) -> String {
    let mut styles = Vec::new();
    if let Some(family) = &s.font_family {
        styles.push(format!("font-family: {}", escape(&crate::markdown::font_family_css(family))));
    }
    if let Some(size) = s.font_size {
        styles.push(format!("font-size: {size}pt"));
    }
    if styles.is_empty() { text } else { format!("<span style=\"{}\">{text}</span>", styles.join("; ")) }
}

/// Blocks as the HTML body of a page: the inside of `<body>`, one block per line.
pub fn blocks_to_html(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut at = 0;
    while let Some(block) = blocks.get(at) {
        if let Block::Item { ordered, depth, .. } = block {
            render_html_list(blocks, &mut at, *depth, *ordered, &mut out);
            continue;
        }
        match block {
            Block::Heading { level, spans } => {
                let l = (*level).clamp(1, 6);
                out.push_str(&format!("<h{l}>{}</h{l}>\n", spans_to_html(spans)));
            }
            Block::Paragraph(spans) => out.push_str(&format!("<p>{}</p>\n", spans_to_html(spans))),
            Block::Item { .. } => {}
            Block::Quote(spans) => out.push_str(&format!("<blockquote>{}</blockquote>\n", spans_to_html(spans))),
            Block::Code { lang, text } => {
                // `class="diagram"` is how a page says "this is a Mermaid diagram"; the
                // reader turns it back into a `mermaid` fence, so the pair agrees.
                let class = match lang.as_str() {
                    "" => String::new(),
                    "mermaid" => " class=\"diagram\"".to_string(),
                    other => format!(" class=\"{}\"", escape(other)),
                };
                out.push_str(&format!("<pre{class}>{}</pre>\n", escape(text.trim_end_matches('\n'))));
            }
            Block::MathBlock(tex) => out.push_str(&format!("<p class=\"math\">{}</p>\n", escape(tex.trim_end_matches('\n')))),
            Block::Table { head, rows } => {
                out.push_str("<table>\n");
                if !head.is_empty() {
                    out.push_str("<tr>");
                    for c in head {
                        out.push_str(&format!("<th>{}</th>", spans_to_html_with_breaks(c, true)));
                    }
                    out.push_str("</tr>\n");
                }
                for r in rows {
                    out.push_str("<tr>");
                    for c in r {
                        out.push_str(&format!("<td>{}</td>", spans_to_html_with_breaks(c, true)));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table>\n");
            }
            Block::Rule => out.push_str("<hr>\n"),
            Block::Image { alt, src, width, height } => {
                let mut img_tag = format!("<img src=\"{}\" alt=\"{}\"", escape(src), escape(alt));
                if let Some(w) = width {
                    img_tag.push_str(&format!(" width=\"{}\"", *w as i32));
                }
                if let Some(h) = height {
                    img_tag.push_str(&format!(" height=\"{}\"", *h as i32));
                }
                img_tag.push('>');
                out.push_str(&format!("<p>{}</p>\n", img_tag));
            }
        }
        at += 1;
    }
    out
}

/// Writes one contiguous list level, nesting child items inside their parent list item.
fn render_html_list(blocks: &[Block], at: &mut usize, depth: usize, ordered: bool, out: &mut String) {
    out.push_str(if ordered { "<ol>\n" } else { "<ul>\n" });
    while let Some(Block::Item { ordered: item_ordered, task, depth: item_depth, spans, .. }) = blocks.get(*at) {
        if *item_depth != depth || *item_ordered != ordered {
            break;
        }
        let box_text = match task {
            Some(true) => "&#9745; ",
            Some(false) => "&#9744; ",
            None => "",
        };
        out.push_str(&format!("<li>{box_text}{}", spans_to_html(spans)));
        *at += 1;
        if let Some(Block::Item { depth: child_depth, ordered: child_ordered, .. }) = blocks.get(*at)
            && *child_depth > depth
        {
            render_html_list(blocks, at, depth + 1, *child_ordered, out);
        }
        out.push_str("</li>\n");
    }
    out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
}

/// Inline spans as HTML.
pub fn spans_to_html(spans: &[Span]) -> String {
    spans_to_html_with_breaks(spans, false)
}

/// Table cell line breaks are content, rather than whitespace between HTML words.
fn spans_to_html_with_breaks(spans: &[Span], cell: bool) -> String {
    let mut out = String::new();
    for s in spans {
        if let Some(src) = &s.image {
            let mut image = format!("<img src=\"{}\" alt=\"{}\">", escape(src), escape(&s.text));
            image = inline_style(s, inline_marks(s, image));
            match &s.link {
                Some(target) => out.push_str(&format!("<a href=\"{}\">{image}</a>", escape(target))),
                None => out.push_str(&image),
            }
            continue;
        }
        if s.math {
            let mut math = format!("<span class=\"math\">${}$</span>", escape(&s.text));
            math = inline_style(s, inline_marks(s, math));
            out.push_str(&math);
            continue;
        }
        let mut text = escape(&s.text);
        if cell && !s.code {
            text = table_line_breaks(&text);
        }
        if s.code {
            text = format!("<code>{text}</code>");
        } else {
            if s.strike {
                text = format!("<s>{text}</s>");
            }
            if s.italic {
                text = format!("<i>{text}</i>");
            }
            if s.bold {
                text = format!("<b>{text}</b>");
            }
        }
        let text = match &s.link {
            Some(target) => format!("<a href=\"{}\">{text}</a>", escape(target)),
            None => text,
        };
        let text = inline_style(s, inline_marks(s, text));
        out.push_str(&text);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::html;
    use crate::markdown::parse;

    #[test]
    fn linked_images_keep_the_outer_attachment_target() {
        let spans = crate::markdown::parse_inline("[![brief.docx](assets/brief.docx.png) brief.docx](assets/brief.docx)");
        assert_eq!(spans_to_markdown(&spans), "[![brief.docx](assets/brief.docx.png)](assets/brief.docx)[ brief.docx](assets/brief.docx)");
        assert_eq!(
            spans_to_html(&spans),
            "<a href=\"assets/brief.docx\"><img src=\"assets/brief.docx.png\" alt=\"brief.docx\"></a><a href=\"assets/brief.docx\"> brief.docx</a>"
        );
    }

    /// A code block whose lines are numbered says so in its language, in both formats, and keeps its words.
    #[test]
    fn numbered_code_is_the_same_block_in_both_formats() {
        let blocks = vec![
            Block::Code {
                lang: "123|rust".into(),
                text: "fn a() {}
fn b() {}"
                    .into(),
            },
            Block::Code { lang: "123|".into(), text: "plain".into() },
            Block::Code { lang: "rust".into(), text: "x".into() },
        ];
        let md = blocks_to_markdown(&blocks);
        assert!(
            md.contains("```123|rust")
                && md.contains(
                    "```123|
"
                ),
            "{md}"
        );
        assert_eq!(parse(&md), blocks, "CMGFM reads back the same");
        let html = blocks_to_html(&blocks);
        assert!(html.contains("<pre class=\"123|rust\">"), "{html}");
        assert_eq!(parse(&html::to_markdown(&html)), blocks, "HTML reads back the same");
        assert_eq!(crate::markdown::code_lang("123|rust"), (true, "rust"));
        assert_eq!(crate::markdown::code_lang("rust"), (false, "rust"));
        assert_eq!(crate::markdown::code_lang("123|"), (true, ""));
    }

    /// Neighbours that share a mark are written under one pair of it, with the white space outside.
    #[test]
    fn neighbours_that_share_a_mark_are_written_under_one_pair() {
        let bold = |t: &str| Span { text: t.into(), bold: true, ..Default::default() };
        let both = |t: &str| Span { text: t.into(), bold: true, italic: true, ..Default::default() };
        let spans = vec![bold("one "), both("two"), bold(" three")];
        assert_eq!(spans_to_markdown(&spans), "**one *two* three**");
        assert_eq!(spans_to_markdown(&[bold("x "), Span { text: "y".into(), ..Default::default() }]), "**x** y");
        // Both marks on one run, and read back as both.
        let spans = crate::markdown::parse_inline("***x***");
        assert_eq!((spans.len(), spans[0].bold, spans[0].italic, spans[0].text.as_str()), (1, true, true, "x"));
        // Whatever is written says the same thing when it is read and written again.
        for md in ["**one *two* three**", "**a** b *c*", "~~**a** b~~", "**[a](x) b**"] {
            let once = spans_to_markdown(&crate::markdown::parse_inline(md));
            assert_eq!(spans_to_markdown(&crate::markdown::parse_inline(&once)), once, "{md}");
        }
    }

    /// The property that makes block editing safe: what is written parses back the same.
    fn round_trips(markdown: &str) {
        let first = parse(markdown);
        let written = blocks_to_markdown(&first);
        let second = parse(&written);
        assert_eq!(first, second, "\n--- source ---\n{markdown}\n--- written ---\n{written}");
    }

    #[test]
    fn markdown_round_trips() {
        for text in [
            "# Title\n\nA paragraph with **bold**, *italic*, ~~gone~~ and `code`.\n",
            "## Two\n\n- one\n- two\n  - nested\n\n1. first\n2. second\n",
            "- [ ] open\n- [x] done\n",
            "> a quote\n\n---\n\n![a picture](assets/p.png)\n",
            "```rust\nlet x = 1;\n```\n",
            "```mermaid\ngraph TD;\n  A-->B;\n```\n",
            "$$\nx = \\frac{1}{2}\n$$\n",
            "Inline $E = mc^2$ and a [link](page.md).\n",
            "| A | B |\n| --- | --- |\n| 1 | 2 |\n",
            "",
            "just words\n",
        ] {
            round_trips(text);
        }
    }

    #[test]
    fn html_round_trips_through_blocks() {
        // A page written by NoteCraft comes back the same after a trip through the blocks.
        for body in [
            "<h1>Title</h1>\n<p>Words with <b>bold</b> and <i>slant</i>.</p>\n",
            "<ul>\n<li>one</li>\n<li>two</li>\n</ul>\n",
            "<ul>\n<li>parent<ul>\n<li>nested</li>\n</ul></li>\n<li>sibling</li>\n</ul>\n",
            "<ol>\n<li>first</li>\n<li>second</li>\n</ol>\n",
            "<p>A <a href=\"x.md\">link</a> and <code>code</code>.</p>\n",
            "<p class=\"math\">x^2 +\ny^2</p>\n",
            "<blockquote>Quality is not negotiable.</blockquote>\n",
            "<table>\n<tr><th>Phase</th><th>Status</th></tr>\n<tr><td>Format</td><td>done</td></tr>\n</table>\n",
            "<p><img src=\"assets/koala.png\" alt=\"A koala\"></p>\n",
            "<hr>\n",
        ] {
            let blocks = parse(&html::to_markdown(body));
            let written = blocks_to_html(&blocks);
            let again = parse(&html::to_markdown(&written));
            assert_eq!(blocks, again, "\n--- body ---\n{body}\n--- written ---\n{written}");
        }
    }

    /// Issue #106: code that holds a fence line of its own stays one block, in both formats.
    /// The fence written round it is longer than any run of backticks starting one of its
    /// lines, and only a fence at least as long as the opening one closes the block.
    #[test]
    fn code_holding_a_fence_stays_one_block() {
        let tick = "`";
        for text in [
            format!("before\n{}\nafter", tick.repeat(3)),
            format!("{}md\nnested\n{}", tick.repeat(3), tick.repeat(3)),
            format!("  {}\n{}python\nx\n{}", tick.repeat(5), tick.repeat(4), tick.repeat(4)),
            format!("inline {} only", tick.repeat(3)),
        ] {
            let original = vec![Block::Code { lang: "markdown".into(), text: text.clone() }];
            let saved = blocks_to_markdown(&original);
            assert_eq!(parse(&saved), original, "CMGFM:\n{saved}");
            let html = blocks_to_html(&original);
            let reread = html::to_markdown(&html);
            assert_eq!(parse(&reread), original, "HTML:\n{html}\n--- read as ---\n{reread}");
        }
        // A plain block keeps the three backticks it always had.
        assert_eq!(
            blocks_to_markdown(&[Block::Code { lang: "rust".into(), text: "let x = 1;".into() }]),
            format!("{0}rust\nlet x = 1;\n{0}\n", tick.repeat(3))
        );
        // A shorter fence inside a longer one is code, and a closing fence with words after
        // it is not a closing fence.
        let four = tick.repeat(4);
        let three = tick.repeat(3);
        assert_eq!(
            parse(&format!("{four}\n{three}\n{three}js\n{four}\n")),
            vec![Block::Code { lang: String::new(), text: format!("{three}\n{three}js") }]
        );
    }

    /// Issue #107: inline code holding backticks, or starting or ending with one or with a
    /// space, comes back as the same single span in both formats.
    #[test]
    fn inline_code_holding_backticks_stays_one_span() {
        let tick = "`";
        for text in [
            format!("a{tick}b"),
            format!("a{0}{0}b{0}c", tick),
            format!("{tick}start"),
            format!("end{tick}"),
            tick.to_string(),
            " padded ".to_string(),
            "  two  ".to_string(),
            " ".to_string(),
            "plain".to_string(),
        ] {
            let original = vec![Block::Paragraph(vec![
                Span { text: "see ".into(), ..Span::default() },
                Span { text: text.clone(), code: true, ..Span::default() },
                Span { text: " here".into(), ..Span::default() },
            ])];
            let saved = blocks_to_markdown(&original);
            assert_eq!(parse(&saved), original, "CMGFM {text:?}:\n{saved}");
            // HTML folds a space at the edge of the code into the space before or after it, as a
            // browser shows it, so those are only kept in CMGFM.
            if text.starts_with(' ') || text.ends_with(' ') {
                continue;
            }
            let html = blocks_to_html(&original);
            let reread = html::to_markdown(&html);
            assert_eq!(parse(&reread), original, "HTML {text:?}:\n{html}\n--- read as ---\n{reread}");
        }
        // Plain code keeps its one backtick each side.
        assert_eq!(
            blocks_to_markdown(&[Block::Paragraph(vec![Span { text: "x".into(), code: true, ..Span::default() }])]),
            format!("{tick}x{tick}\n")
        );
        // A run with no partner of the same length is text.
        let lone = parse(&format!("a {0}{0} b {0} c\n", tick));
        assert!(matches!(&lone[..], [Block::Paragraph(spans)] if spans.iter().all(|s| !s.code)), "{lone:?}");
    }

    /// Issue #108: a link or picture address with parentheses in it is kept whole, in both
    /// formats, whether its parentheses pair up or not.
    #[test]
    fn link_addresses_with_parentheses_are_kept_whole() {
        let parsed = parse("[file](https://example.test/a(b).pdf)\n");
        let link = |target: &str| vec![Block::Paragraph(vec![Span { text: "file".into(), link: Some(target.into()), ..Span::default() }])];
        assert_eq!(parsed, link("https://example.test/a(b).pdf"), "a balanced pair stays in the address");
        assert_eq!(
            parse("[file](a\\(b.pdf) after\n")[0],
            Block::Paragraph(vec![
                Span { text: "file".into(), link: Some("a(b.pdf".into()), ..Span::default() },
                Span { text: " after".into(), ..Span::default() },
            ]),
            "an escaped parenthesis does not count"
        );

        for target in ["https://example.test/a(b).pdf", "assets/a(b).pdf", "x/(a(b)c).md", "open(.pdf", "close).pdf", ")(", "w/Foo_(bar)"] {
            let original = link(target);
            let saved = blocks_to_markdown(&original);
            assert_eq!(parse(&saved), original, "CMGFM {target:?}:\n{saved}");
            let html = blocks_to_html(&original);
            let reread = html::to_markdown(&html);
            assert_eq!(parse(&reread), original, "HTML {target:?}:\n{html}\n--- read as ---\n{reread}");

            let picture = vec![Block::Image { alt: "pic".into(), src: target.into(), width: None, height: None }];
            assert_eq!(parse(&blocks_to_markdown(&picture)), picture, "picture {target:?}");
            assert_eq!(parse(&html::to_markdown(&blocks_to_html(&picture))), picture, "HTML picture {target:?}");
        }
        assert_eq!(blocks_to_markdown(&link("a(b).pdf")), "[file](a(b).pdf)\n", "balanced parentheses are written as they are");

        // A link written before quoting existed, to a Windows folder, keeps its last backslash,
        // even with a parenthesis later in the line.
        assert_eq!(parse("[file](C:\\Notes\\)\n"), link("C:\\Notes\\"));
        assert_eq!(
            parse("[file](C:\\Notes\\) and (more) text\n")[0],
            Block::Paragraph(vec![
                Span { text: "file".into(), link: Some("C:\\Notes\\".into()), ..Span::default() },
                Span { text: " and (more) text".into(), ..Span::default() },
            ])
        );
    }

    /// Issue #109: a resized picture whose alt text or address holds a quote, an ampersand or
    /// an angle bracket keeps all of it, in both formats.
    #[test]
    fn a_resized_picture_keeps_quotes_in_its_alt_text() {
        for (alt, src) in
            [("a\"b", "assets/a.png"), ("Tom & \"Jerry\" <3", "assets/t&j.png"), ("it's", "assets/q\"uote.png"), ("&amp; literal", "assets/a.png")]
        {
            let original = vec![Block::Image { alt: alt.into(), src: src.into(), width: Some(100.0), height: Some(80.0) }];
            let md = blocks_to_markdown(&original);
            assert_eq!(parse(&md), original, "CMGFM:\n{md}");
            let html = blocks_to_html(&original);
            let reread = html::to_markdown(&html);
            assert_eq!(parse(&reread), original, "HTML:\n{html}\n--- read as ---\n{reread}");
        }
        let md = blocks_to_markdown(&[Block::Image { alt: "a\"b".into(), src: "assets/a.png".into(), width: Some(100.0), height: None }]);
        assert_eq!(md, "<img src=\"assets/a.png\" alt=\"a&quot;b\" width=\"100\">\n");
    }

    #[test]
    fn html_escapes_everything_that_matters() {
        let blocks = parse("A <script> & \"quotes\" in **bold**\n");
        let html = blocks_to_html(&blocks);
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp;"));
        assert!(!html.contains("<script>"));
        // A quote in an address is escaped, so it cannot end the attribute and start another.
        let link = blocks_to_html(&parse("[x](\"onerror=alert(1))\n"));
        assert!(link.contains("href=\"&quot;onerror=alert(1)\""), "{link}");
        assert!(!link.contains("\"onerror"), "{link}");
    }

    #[test]
    fn lists_open_and_close_once() {
        let blocks = parse("- a\n- b\n\n1. c\n\ntext\n");
        let html = blocks_to_html(&blocks);
        assert_eq!(html.matches("<ul>").count(), 1);
        assert_eq!(html.matches("</ul>").count(), 1);
        assert_eq!(html.matches("<ol>").count(), 1);
        assert_eq!(html.matches("</ol>").count(), 1);
        assert!(html.ends_with("<p>text</p>\n"), "{html}");
        // A list at the very end is still closed.
        assert!(blocks_to_html(&parse("- a\n")).ends_with("</ul>\n"));
    }

    #[test]
    fn hostile_blocks_do_not_break_the_writers() {
        let odd = vec![
            Block::Heading { level: 9, spans: vec![Span { text: "deep".into(), ..Default::default() }] },
            Block::Item { ordered: true, number: 0, task: Some(true), depth: 40, spans: Vec::new() },
            Block::Code { lang: "a\"b".into(), text: String::new() },
            Block::Table { head: Vec::new(), rows: vec![Vec::new()] },
            Block::MathBlock(String::new()),
            Block::Paragraph(Vec::new()),
        ];
        let md = blocks_to_markdown(&odd);
        let html = blocks_to_html(&odd);
        assert!(md.starts_with("###### deep"), "{md}");
        assert!(!html.is_empty());
        // Whatever comes out must parse without panicking.
        let _ = parse(&md);
        let _ = parse(&html::to_markdown(&html));
    }

    #[test]
    fn the_writers_agree_on_the_text() {
        let blocks = parse("# T\n\nA **b** c\n\n- x\n");
        let md = blocks_to_markdown(&blocks);
        let html = blocks_to_html(&blocks);
        for word in ["T", "A", "b", "c", "x"] {
            assert!(md.contains(word) && html.contains(word), "{word} is missing");
        }
    }

    #[test]
    fn clipboard_plain_text_omits_markup_but_keeps_link_targets() {
        let blocks = parse("# Title\n\nA **bold** word and [a link](https://example.com).\n\n- item\n");
        assert_eq!(blocks_to_text(&blocks), "Title\nA bold word and a link (https://example.com).\n- item");
        let html = blocks_to_html(&blocks);
        assert!(html.contains("<h1>Title</h1>"), "{html}");
        assert!(html.contains("<b>bold</b>"), "{html}");
        assert!(html.contains("<a href=\"https://example.com\">a link</a>"), "{html}");
    }

    #[test]
    fn inline_font_size_round_trips_through_cmgfm_and_html() {
        let source = "<span style=\"font-size: 18pt\">larger text</span>";
        let blocks = parse(source);

        let markdown = blocks_to_markdown(&blocks);
        assert_eq!(parse(&markdown), blocks, "CMGFM retains the size");

        let html = blocks_to_html(&blocks);
        assert!(html.contains("<span style=\"font-size: 18pt\">larger text</span>"), "{html}");
        assert_eq!(parse(&html::to_markdown(&html)), blocks, "HTML retains the size");
    }

    #[test]
    fn font_family_underline_and_scripts_round_trip_in_both_formats() {
        let source = "<span style=\"font-family: DejaVu Sans; font-size: 18pt\"><u>underlined</u> H<sub>2</sub>O x<sup>2</sup></span>";
        let blocks = parse(source);
        assert_eq!(blocks_to_text(&blocks), "underlined H2O x2", "formatting tags are not visible page text");
        let Block::Paragraph(spans) = &blocks[0] else { panic!("expected a paragraph") };
        assert!(spans.iter().all(|span| span.font_family.as_deref() == Some("DejaVu Sans") && span.font_size == Some(18)));
        assert!(spans.iter().any(|span| span.text == "underlined" && span.underline));
        assert!(spans.iter().any(|span| span.text == "2" && span.script == Some(crate::markdown::Script::Subscript)));
        assert!(spans.iter().any(|span| span.text == "2" && span.script == Some(crate::markdown::Script::Superscript)));
        let markdown = blocks_to_markdown(&blocks);
        let html = blocks_to_html(&blocks);

        assert!(markdown.contains("font-family: DejaVu Sans"), "CMGFM preserves the font family: {markdown}");
        assert!(markdown.contains("<u>") && markdown.contains("<sub>") && markdown.contains("<sup>"), "CMGFM preserves the marks: {markdown}");
        assert!(html.contains("font-family: DejaVu Sans"), "HTML preserves the font family: {html}");
        assert!(html.contains("<u>") && html.contains("<sub>") && html.contains("<sup>"), "HTML preserves the marks: {html}");
        assert_eq!(parse(&markdown), blocks, "CMGFM round-trips the formatting");
        assert_eq!(parse(&html::to_markdown(&html)), blocks, "HTML round-trips the formatting");
    }

    #[test]
    fn font_family_names_with_punctuation_round_trip_safely() {
        for name in [
            "Noto Sans (Arabic)",
            "O'Brien & Sons",
            "Family; color: red",
            "Family \"A\"",
            "Family <one>",
            "Font\\'X",
            "3D Fonts",
            "Font.Name",
            "東京",
        ] {
            let blocks = vec![Block::Paragraph(vec![Span { text: "words".into(), font_family: Some(name.into()), ..Span::default() }])];
            let markdown = blocks_to_markdown(&blocks);
            let html = blocks_to_html(&blocks);
            assert_eq!(parse(&markdown), blocks, "CMGFM round trip: {name:?}: {markdown}");
            assert_eq!(parse(&html::to_markdown(&html)), blocks, "HTML round trip: {name:?}: {html}");
        }
    }

    #[test]
    fn nested_font_styles_preserve_unselected_marks_and_unicode() {
        let source = "<span style=\"font-family: Noto Sans; font-size: 18pt\">A **<u>bold</u>** <span style=\"font-size: 12pt\">مرحبا<sub>2</sub></span></span>";
        let blocks = parse(source);
        assert_eq!(blocks_to_text(&blocks), "A bold مرحبا2");
        let markdown = blocks_to_markdown(&blocks);
        let html = blocks_to_html(&blocks);
        assert_eq!(parse(&markdown), blocks, "CMGFM round-trips nested formatting: {markdown}");
        assert_eq!(parse(&html::to_markdown(&html)), blocks, "HTML round-trips nested formatting: {html}");
    }
}

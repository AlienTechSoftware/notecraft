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
                out.push_str(&spans_to_markdown(spans));
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
                out.push_str(&spans_to_markdown(spans));
                out.push('\n');
            }
            Block::Quote(spans) => {
                out.push_str("> ");
                out.push_str(&spans_to_markdown(spans));
                out.push('\n');
            }
            Block::Code { lang, text } => {
                out.push_str("```");
                out.push_str(lang);
                out.push('\n');
                out.push_str(text);
                if !text.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n");
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
            Block::Image { alt, src } => out.push_str(&format!("![{alt}]({src})\n")),
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
        Block::Image { alt, src } => readable_link(alt, src),
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
        s.push_str(&spans_to_markdown(c).replace('|', "\\|"));
        s.push_str(" |");
    }
    s.push('\n');
    s
}

/// Inline spans as Markdown. The order of the marks matches what the parser reads back.
pub fn spans_to_markdown(spans: &[Span]) -> String {
    marked(spans, 0)
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
fn marked(spans: &[Span], level: usize) -> String {
    let Some((marks, has, clear)) = MARKS.get(level) else {
        return spans.iter().map(leaf).collect();
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
            let text = marked(&inner, level + 1);
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
            out.push_str(&marked(run, level + 1));
        }
        i = j;
    }
    out
}

/// One span with nothing but its code, picture, maths or link written round it.
fn leaf(s: &Span) -> String {
    if let Some(src) = &s.image {
        let image = format!("![{}]({src})", s.text);
        return match &s.link {
            Some(target) => format!("[{image}]({target})"),
            None => image,
        };
    }
    if s.math {
        return format!("${}$", s.text);
    }
    let text = if s.code { format!("`{}`", s.text) } else { s.text.clone() };
    match &s.link {
        Some(target) => format!("[{text}]({target})"),
        None => text,
    }
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
                        out.push_str(&format!("<th>{}</th>", spans_to_html(c)));
                    }
                    out.push_str("</tr>\n");
                }
                for r in rows {
                    out.push_str("<tr>");
                    for c in r {
                        out.push_str(&format!("<td>{}</td>", spans_to_html(c)));
                    }
                    out.push_str("</tr>\n");
                }
                out.push_str("</table>\n");
            }
            Block::Rule => out.push_str("<hr>\n"),
            Block::Image { alt, src } => out.push_str(&format!("<p><img src=\"{}\" alt=\"{}\"></p>\n", escape(src), escape(alt))),
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
    let mut out = String::new();
    for s in spans {
        if let Some(src) = &s.image {
            let image = format!("<img src=\"{}\" alt=\"{}\">", escape(src), escape(&s.text));
            match &s.link {
                Some(target) => out.push_str(&format!("<a href=\"{}\">{image}</a>", escape(target))),
                None => out.push_str(&image),
            }
            continue;
        }
        if s.math {
            out.push_str(&format!("<span class=\"math\">${}$</span>", escape(&s.text)));
            continue;
        }
        let mut text = escape(&s.text);
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
        match &s.link {
            Some(target) => out.push_str(&format!("<a href=\"{}\">{text}</a>", escape(target))),
            None => out.push_str(&text),
        }
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

    #[test]
    fn html_escapes_everything_that_matters() {
        let blocks = parse("A <script> & \"quotes\" in **bold**\n");
        let html = blocks_to_html(&blocks);
        assert!(html.contains("&lt;script&gt;"), "{html}");
        assert!(html.contains("&amp;"));
        assert!(!html.contains("<script>"));
        let link = parse("[x](\"onerror=alert(1))\n");
        assert!(!blocks_to_html(&link).contains("onerror=alert(1)\""));
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
}

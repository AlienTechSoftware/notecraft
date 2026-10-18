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
    let mut out = String::new();
    for s in spans {
        if let Some(src) = &s.image {
            out.push_str(&format!("![{}]({src})", s.text));
            continue;
        }
        if s.math {
            out.push_str(&format!("${}$", s.text));
            continue;
        }
        let mut text = s.text.clone();
        if s.code {
            text = format!("`{text}`");
        } else {
            // Bold before italic, so `***x***` reads back the same way.
            if s.strike {
                text = format!("~~{text}~~");
            }
            if s.italic {
                text = format!("*{text}*");
            }
            if s.bold {
                text = format!("**{text}**");
            }
        }
        match &s.link {
            Some(target) => out.push_str(&format!("[{text}]({target})")),
            None => out.push_str(&text),
        }
    }
    out
}

/// Blocks as the HTML body of a page: the inside of `<body>`, one block per line.
pub fn blocks_to_html(blocks: &[Block]) -> String {
    let mut out = String::new();
    let mut open_list: Option<bool> = None; // Some(ordered)
    for block in blocks {
        // Close a list when the next block is not an item of the same kind.
        let wanted = match block {
            Block::Item { ordered, .. } => Some(*ordered),
            _ => None,
        };
        if open_list != wanted
            && let Some(ordered) = open_list.take()
        {
            out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
        }
        match block {
            Block::Heading { level, spans } => {
                let l = (*level).clamp(1, 6);
                out.push_str(&format!("<h{l}>{}</h{l}>\n", spans_to_html(spans)));
            }
            Block::Paragraph(spans) => out.push_str(&format!("<p>{}</p>\n", spans_to_html(spans))),
            Block::Item { ordered, task, spans, .. } => {
                if open_list.is_none() {
                    out.push_str(if *ordered { "<ol>\n" } else { "<ul>\n" });
                    open_list = Some(*ordered);
                }
                let box_text = match task {
                    Some(true) => "&#9745; ",
                    Some(false) => "&#9744; ",
                    None => "",
                };
                out.push_str(&format!("<li>{box_text}{}</li>\n", spans_to_html(spans)));
            }
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
            Block::MathBlock(tex) => out.push_str(&format!("<p class=\"math\">$$ {} $$</p>\n", escape(tex.trim()))),
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
    }
    if let Some(ordered) = open_list {
        out.push_str(if ordered { "</ol>\n" } else { "</ul>\n" });
    }
    out
}

/// Inline spans as HTML.
pub fn spans_to_html(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        if let Some(src) = &s.image {
            out.push_str(&format!("<img src=\"{}\" alt=\"{}\">", escape(src), escape(&s.text)));
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
            "<ol>\n<li>first</li>\n<li>second</li>\n</ol>\n",
            "<p>A <a href=\"x.md\">link</a> and <code>code</code>.</p>\n",
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
}

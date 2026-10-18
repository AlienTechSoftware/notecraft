//! A CommonMark + GFM subset parsed into blocks that a front end can draw.
//!
//! Supported blocks: headings, paragraphs, bullet, numbered and task list items
//! (nested by indentation), block quotes, fenced code (a `mermaid` fence is kept as
//! code with its language), math blocks (`$$ ... $$`), tables, rules and standalone
//! images. Supported inline: bold, italic, strikethrough, code, links, images and
//! `$math$`. Not supported: reference links, HTML inside Markdown, nested block
//! quotes and lists inside quotes, setext headings, footnotes.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub math: bool,
    /// Link target, if this span is part of a link.
    pub link: Option<String>,
    /// Image source, if this span is an inline image (the text is the alt text).
    pub image: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading { level: u8, spans: Vec<Span> },
    Paragraph(Vec<Span>),
    Item { ordered: bool, number: usize, task: Option<bool>, depth: usize, spans: Vec<Span> },
    Quote(Vec<Span>),
    Code { lang: String, text: String },
    MathBlock(String),
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    Rule,
    Image { alt: String, src: String },
}

#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
}

/// Parses inline Markdown into styled spans.
pub fn parse_inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    inline(&chars, Style::default(), None, 0)
}

const MAX_DEPTH: usize = 8;

fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (from..=chars.len() - pat.len()).find(|&i| chars.get(i..i + pat.len()) == Some(pat))
}

fn slice(chars: &[char], a: usize, b: usize) -> &[char] {
    chars.get(a..b).unwrap_or(&[])
}

fn push_text(out: &mut Vec<Span>, buf: &mut String, st: Style, link: &Option<String>) {
    if !buf.is_empty() {
        out.push(Span { text: std::mem::take(buf), bold: st.bold, italic: st.italic, strike: st.strike, link: link.clone(), ..Span::default() });
    }
}

fn inline(chars: &[char], st: Style, link: Option<String>, depth: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        let next = chars.get(i + 1).copied();
        match c {
            '\\' if next.is_some() => {
                if let Some(n) = next {
                    buf.push(n);
                }
                i += 2;
            }
            '`' => {
                if let Some(end) = find(chars, i + 1, &['`']) {
                    push_text(&mut out, &mut buf, st, &link);
                    let code: String = slice(chars, i + 1, end).iter().collect();
                    out.push(Span {
                        text: code,
                        code: true,
                        bold: st.bold,
                        italic: st.italic,
                        strike: st.strike,
                        link: link.clone(),
                        ..Span::default()
                    });
                    i = end + 1;
                } else {
                    buf.push(c);
                    i += 1;
                }
            }
            '*' | '~' if next == Some(c) && depth < MAX_DEPTH => {
                let pat = [c, c];
                match find(chars, i + 2, &pat) {
                    Some(end) if end > i + 2 => {
                        push_text(&mut out, &mut buf, st, &link);
                        let mut inner = st;
                        if c == '*' {
                            inner.bold = true;
                        } else {
                            inner.strike = true;
                        }
                        out.extend(inline(slice(chars, i + 2, end), inner, link.clone(), depth + 1));
                        i = end + 2;
                    }
                    _ => {
                        buf.push(c);
                        i += 1;
                    }
                }
            }
            '*' if depth < MAX_DEPTH && next.is_some_and(|n| !n.is_whitespace()) => match find(chars, i + 1, &['*']) {
                Some(end) if end > i + 1 => {
                    push_text(&mut out, &mut buf, st, &link);
                    let mut inner = st;
                    inner.italic = true;
                    out.extend(inline(slice(chars, i + 1, end), inner, link.clone(), depth + 1));
                    i = end + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            },
            '$' => match find(chars, i + 1, &['$']) {
                Some(end) if end > i + 1 && !slice(chars, i + 1, end).contains(&'\n') => {
                    push_text(&mut out, &mut buf, st, &link);
                    out.push(Span { text: slice(chars, i + 1, end).iter().collect(), math: true, link: link.clone(), ..Span::default() });
                    i = end + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            },
            '!' if next == Some('[') => match parse_link(chars, i + 1) {
                Some((alt, url, after)) => {
                    push_text(&mut out, &mut buf, st, &link);
                    out.push(Span { text: alt, image: Some(url), ..Span::default() });
                    i = after;
                }
                None => {
                    buf.push(c);
                    i += 1;
                }
            },
            '[' if depth < MAX_DEPTH => match parse_link(chars, i) {
                Some((label, url, after)) => {
                    push_text(&mut out, &mut buf, st, &link);
                    let label_chars: Vec<char> = label.chars().collect();
                    out.extend(inline(&label_chars, st, Some(url), depth + 1));
                    i = after;
                }
                None => {
                    buf.push(c);
                    i += 1;
                }
            },
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    push_text(&mut out, &mut buf, st, &link);
    out
}

/// `[label](url)` starting at `start` (the `[`). Returns label, url and the index after `)`.
fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    let close = find(chars, start + 1, &[']'])?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find(chars, close + 2, &[')'])?;
    let label: String = slice(chars, start + 1, close).iter().collect();
    let url: String = slice(chars, close + 2, end).iter().collect::<String>().trim().to_string();
    if url.contains(char::is_whitespace) {
        return None;
    }
    Some((label, url, end + 1))
}

fn is_table_separator(line: &str) -> bool {
    let t = line.trim();
    t.contains('-') && t.contains('|') && t.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

fn cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn list_marker(line: &str) -> Option<(bool, usize, usize, &str)> {
    // Returns (ordered, number, indent width, rest after the marker).
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(m) {
            return Some((false, 0, indent, rest));
        }
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let after = t.get(digits.len()..).unwrap_or("");
        if let Some(rest) = after.strip_prefix(". ") {
            return Some((true, digits.parse().unwrap_or(1), indent, rest));
        }
    }
    None
}

fn starts_block(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('#') || t.starts_with("```") || t.starts_with('>') || t.starts_with("$$") || list_marker(line).is_some()
}

/// Parses a Markdown page body into blocks. Never panics.
pub fn parse(src: &str) -> Vec<Block> {
    let text = src.replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines.get(i).copied().unwrap_or("");
        let t = line.trim();
        if t.is_empty() {
            i += 1;
        } else if let Some(fence) = t.strip_prefix("```") {
            let lang = fence.trim().to_string();
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && !lines.get(i).is_some_and(|l| l.trim_start().starts_with("```")) {
                body.push(lines.get(i).copied().unwrap_or(""));
                i += 1;
            }
            i += 1;
            out.push(Block::Code { lang, text: body.join("\n") });
        } else if t == "$$" {
            let mut body = Vec::new();
            i += 1;
            while i < lines.len() && lines.get(i).map(|l| l.trim()) != Some("$$") {
                body.push(lines.get(i).copied().unwrap_or(""));
                i += 1;
            }
            i += 1;
            out.push(Block::MathBlock(body.join(" ").trim().to_string()));
        } else if t.starts_with('#') && t.trim_start_matches('#').starts_with(' ') && t.chars().take_while(|&c| c == '#').count() <= 6 {
            let level = t.chars().take_while(|&c| c == '#').count();
            let rest = t.trim_start_matches('#').trim();
            out.push(Block::Heading { level: u8::try_from(level).unwrap_or(6), spans: parse_inline(rest) });
            i += 1;
        } else if t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_')) {
            out.push(Block::Rule);
            i += 1;
        } else if t.starts_with('>') {
            let mut parts = Vec::new();
            while i < lines.len() && lines.get(i).is_some_and(|l| l.trim_start().starts_with('>')) {
                parts.push(lines.get(i).copied().unwrap_or("").trim_start().trim_start_matches('>').trim().to_string());
                i += 1;
            }
            out.push(Block::Quote(parse_inline(&parts.join(" "))));
        } else if line.contains('|') && lines.get(i + 1).is_some_and(|l| is_table_separator(l)) {
            let head: Vec<Vec<Span>> = cells(line).iter().map(|c| parse_inline(c)).collect();
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines.get(i).is_some_and(|l| l.contains('|') && !l.trim().is_empty()) {
                rows.push(cells(lines.get(i).copied().unwrap_or("")).iter().map(|c| parse_inline(c)).collect());
                i += 1;
            }
            out.push(Block::Table { head, rows });
        } else if let Some((ordered, number, indent, rest)) = list_marker(line) {
            let (task, rest) = if let Some(r) = rest.strip_prefix("[ ] ") {
                (Some(false), r)
            } else if let Some(r) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
                (Some(true), r)
            } else {
                (None, rest)
            };
            out.push(Block::Item { ordered, number, task, depth: indent / 2, spans: parse_inline(rest) });
            i += 1;
        } else {
            let mut parts = vec![t.to_string()];
            i += 1;
            while i < lines.len() {
                let next = lines.get(i).copied().unwrap_or("");
                if next.trim().is_empty() || starts_block(next) || (next.contains('|') && lines.get(i + 1).is_some_and(|l| is_table_separator(l))) {
                    break;
                }
                parts.push(next.trim().to_string());
                i += 1;
            }
            let joined = parts.join(" ");
            let spans = parse_inline(&joined);
            // A line that is only an image becomes an image block.
            if let [only] = spans.as_slice()
                && let Some(src) = &only.image
            {
                out.push(Block::Image { alt: only.text.clone(), src: src.clone() });
                continue;
            }
            out.push(Block::Paragraph(spans));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn inline_styles() {
        let s = parse_inline("a **bold** b *it* c ~~gone~~ d `code` e");
        assert_eq!(text_of(&s), "a bold b it c gone d code e");
        assert!(s.iter().any(|x| x.bold && x.text == "bold"));
        assert!(s.iter().any(|x| x.italic && x.text == "it"));
        assert!(s.iter().any(|x| x.strike && x.text == "gone"));
        assert!(s.iter().any(|x| x.code && x.text == "code"));
    }

    #[test]
    fn nested_styles_combine() {
        let s = parse_inline("**bold and *italic* inside**");
        assert!(s.iter().any(|x| x.bold && x.italic && x.text == "italic"));
    }

    #[test]
    fn links_images_and_math() {
        let s = parse_inline("see [the **site**](https://x.test) and ![pic](assets/a.png) with $E=mc^2$");
        assert!(s.iter().any(|x| x.link.as_deref() == Some("https://x.test") && x.bold && x.text == "site"));
        assert!(s.iter().any(|x| x.image.as_deref() == Some("assets/a.png") && x.text == "pic"));
        assert!(s.iter().any(|x| x.math && x.text == "E=mc^2"));
    }

    #[test]
    fn unmatched_markers_stay_literal() {
        assert_eq!(text_of(&parse_inline("2 * 3 and a_b and $5 and [x] and `open")), "2 * 3 and a_b and $5 and [x] and `open");
        assert_eq!(text_of(&parse_inline("**never closed")), "**never closed");
        assert_eq!(text_of(&parse_inline("\\*not italic\\*")), "*not italic*");
    }

    #[test]
    fn headings_and_paragraphs() {
        let b = parse("# One\n\ntwo\nlines\n\n###### six\n####### seven\n");
        assert!(matches!(&b[0], Block::Heading { level: 1, .. }));
        assert!(matches!(&b[1], Block::Paragraph(s) if text_of(s) == "two lines"));
        assert!(matches!(&b[2], Block::Heading { level: 6, .. }));
        assert!(matches!(&b[3], Block::Paragraph(_)), "seven hashes is a paragraph");
    }

    #[test]
    fn lists_and_tasks() {
        let b = parse("- a\n- [ ] todo\n- [x] done\n  - nested\n1. one\n2. two\n");
        assert!(matches!(&b[0], Block::Item { ordered: false, task: None, depth: 0, .. }));
        assert!(matches!(&b[1], Block::Item { task: Some(false), .. }));
        assert!(matches!(&b[2], Block::Item { task: Some(true), .. }));
        assert!(matches!(&b[3], Block::Item { depth: 1, .. }));
        assert!(matches!(&b[4], Block::Item { ordered: true, number: 1, .. }));
        assert!(matches!(&b[5], Block::Item { ordered: true, number: 2, .. }));
    }

    #[test]
    fn code_math_quote_rule() {
        let b = parse("```mermaid\ngraph TD;\n  A-->B;\n```\n\n$$\nE = mc^2\n$$\n\n> quoted\n> text\n\n---\n");
        assert!(matches!(&b[0], Block::Code { lang, text } if lang == "mermaid" && text.contains("A-->B")));
        assert!(matches!(&b[1], Block::MathBlock(m) if m == "E = mc^2"));
        assert!(matches!(&b[2], Block::Quote(s) if text_of(s) == "quoted text"));
        assert!(matches!(&b[3], Block::Rule));
    }

    #[test]
    fn tables() {
        let b = parse("| a | b |\n|---|:-:|\n| 1 | **2** |\n| 3 | 4 |\n\nafter");
        match &b[0] {
            Block::Table { head, rows } => {
                assert_eq!(head.len(), 2);
                assert_eq!(rows.len(), 2);
                assert!(rows[0][1].iter().any(|s| s.bold));
            }
            other => panic!("expected a table, got {other:?}"),
        }
        assert!(matches!(&b[1], Block::Paragraph(_)));
    }

    #[test]
    fn a_lone_image_is_an_image_block() {
        let b = parse("![alt text](assets/x.png)\n");
        assert_eq!(b, vec![Block::Image { alt: "alt text".into(), src: "assets/x.png".into() }]);
    }

    #[test]
    fn crlf_and_unclosed_fences() {
        assert_eq!(parse("# T\r\n\r\ntext\r\n").len(), 2);
        let b = parse("```\nno end\nmore");
        assert!(matches!(&b[0], Block::Code { text, .. } if text.contains("more")));
    }

    #[test]
    fn hostile_input_never_panics() {
        let samples = [
            "",
            "\n\n\n",
            "#",
            "# ",
            "|",
            "| |\n|-|",
            "**",
            "[](",
            "![](",
            "[a](b c)",
            "$$",
            "```",
            "- ",
            "1. ",
            "> ",
            "\u{0}\u{1}",
            "é**é*é~~é",
            &"*".repeat(500),
            &"[".repeat(300),
            &"**a ".repeat(200),
            &"> ".repeat(100),
        ];
        for s in samples {
            let _ = parse(s);
            let _ = parse_inline(s);
        }
    }
}

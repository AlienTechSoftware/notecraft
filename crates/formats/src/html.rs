//! HTML pages: reading and replacing the body, page metadata, escaping, and a
//! conversion to a Markdown-like text that previews can draw.

/// The text between `<body ...>` and `</body>`, or the whole text if there is no body.
pub fn inner(text: &str) -> String {
    let Some(start) = body_start(text) else {
        return text.to_string();
    };
    let end = text.rfind("</body>").unwrap_or(text.len()).max(start);
    text.get(start..end).unwrap_or("").trim_matches('\n').to_string()
}

fn body_start(text: &str) -> Option<usize> {
    let i = text.find("<body")?;
    let j = text.get(i..)?.find('>')?;
    Some(i + j + 1)
}

/// Replaces the body of an HTML page, keeping the head. If the text has no body, a
/// complete minimal page is made around `body`.
pub fn replace_inner(text: &str, body: &str) -> String {
    match body_start(text) {
        Some(start) => {
            let end = text.rfind("</body>").unwrap_or(text.len()).max(start);
            format!("{}\n{}\n{}", text.get(..start).unwrap_or(""), body.trim_matches('\n'), text.get(end..).unwrap_or(""))
        }
        None => format!("<!doctype html>\n<html><head><meta charset=\"utf-8\"></head>\n<body>\n{body}\n</body></html>\n"),
    }
}

/// Sets `<meta name="NAME" content="VALUE">` in the head, adding it if missing.
pub fn set_meta(text: &str, name: &str, value: &str) -> String {
    let marker = format!("name=\"{name}\" content=\"");
    if let Some(i) = text.find(&marker) {
        let vstart = i + marker.len();
        if let Some(vend) = text.get(vstart..).and_then(|t| t.find('"')) {
            return format!("{}{}{}", text.get(..vstart).unwrap_or(""), value, text.get(vstart + vend..).unwrap_or(""));
        }
    }
    let tag = format!("<meta name=\"{name}\" content=\"{value}\">");
    match text.find("</head>") {
        Some(i) => format!("{}{tag}\n{}", text.get(..i).unwrap_or(""), text.get(i..).unwrap_or("")),
        None => text.to_string(),
    }
}

/// The content of `<meta name="NAME" content="...">`, if present.
pub fn meta(text: &str, name: &str) -> Option<String> {
    let marker = format!("name=\"{name}\" content=\"");
    let start = text.find(&marker)? + marker.len();
    let end = text.get(start..)?.find('"')?;
    text.get(start..start + end).map(unescape)
}

/// The text of `<title>...</title>`.
pub fn title(text: &str) -> Option<String> {
    between(text, "<title>", "</title>").map(|t| unescape(&t))
}

/// Replaces the text between `start` and `end` markers. Unchanged if either is missing.
pub fn replace_between(text: &str, start: &str, end: &str, with: &str) -> String {
    if let Some(s) = text.find(start) {
        let s2 = s + start.len();
        if let Some(e) = text.get(s2..).and_then(|t| t.find(end)) {
            return format!("{}{with}{}", text.get(..s2).unwrap_or(""), text.get(s2 + e..).unwrap_or(""));
        }
    }
    text.to_string()
}

pub fn between(text: &str, start: &str, end: &str) -> Option<String> {
    let s = text.find(start)? + start.len();
    let e = text.get(s..)?.find(end)? + s;
    text.get(s..e).map(str::to_string)
}

pub fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub fn unescape(s: &str) -> String {
    let named = s.replace("&nbsp;", " ").replace("&quot;", "\"").replace("&gt;", ">").replace("&lt;", "<");
    // Numeric references (&#9744; and &#x2610;), then &amp; last so "&amp;#9744;" stays literal.
    let mut out = String::with_capacity(named.len());
    let mut rest = named.as_str();
    while let Some(i) = rest.find("&#") {
        out.push_str(rest.get(..i).unwrap_or(""));
        let tail = rest.get(i + 2..).unwrap_or("");
        let end = tail.find(';').filter(|&e| e > 0 && e <= 8);
        let decoded = end.and_then(|e| {
            let digits = tail.get(..e)?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse::<u32>().ok()?,
            };
            char::from_u32(code)
        });
        match (decoded, end) {
            (Some(c), Some(e)) => {
                out.push(c);
                rest = tail.get(e + 1..).unwrap_or("");
            }
            _ => {
                out.push_str("&#");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out.replace("&amp;", "&")
}

/// Plain text of an HTML fragment: tags removed, entities decoded.
pub fn strip_tags(html: &str) -> String {
    let (mut out, mut in_tag) = (String::new(), false);
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    unescape(&out)
}

/// Converts an HTML fragment to Markdown-like text, so previews of both formats can
/// share one renderer. Handles headings, paragraphs, line breaks, lists (nested by
/// indentation), emphasis, code, preformatted blocks, links, images, block quotes,
/// rules, tables, and the NoteCraft markers for equations (`span.math`) and diagrams
/// (`pre.diagram`). Unknown tags are skipped and their text kept. Scripts and styles
/// are dropped.
/// Tags the block model can hold. Everything NoteCraft writes uses only these.
const KNOWN_TAGS: &[&str] = &[
    "html",
    "head",
    "body",
    "meta",
    "title",
    "link",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "p",
    "br",
    "hr",
    "ul",
    "ol",
    "li",
    "blockquote",
    "pre",
    "code",
    "table",
    "thead",
    "tbody",
    "tr",
    "th",
    "td",
    "img",
    "a",
    "b",
    "strong",
    "i",
    "em",
    "s",
    "del",
    "u",
    "span",
    "div",
];

/// The first tag in `html` that the block model cannot hold, if there is one. A page that
/// has one is shown but not written on, because writing it back would drop that markup.
pub fn unsupported_tag(html: &str) -> Option<String> {
    let mut rest = html;
    while let Some(at) = rest.find('<') {
        let after = rest.get(at + 1..)?;
        let end = after.find('>')?;
        let raw = after.get(..end).unwrap_or("");
        rest = after.get(end + 1..).unwrap_or("");
        if raw.starts_with('!') {
            continue;
        }
        let name: String = raw.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_lowercase();
        if name.is_empty() {
            continue;
        }
        if !KNOWN_TAGS.contains(&name.as_str()) {
            return Some(name);
        }
    }
    None
}

pub fn to_markdown(html: &str) -> String {
    let mut w = Writer::default();
    let chars: Vec<char> = html.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        if c == '<' {
            let Some(close) = chars.get(i..).and_then(|s| s.iter().position(|&x| x == '>')) else {
                w.text(&chars.get(i..).map(|s| s.iter().collect::<String>()).unwrap_or_default());
                break;
            };
            let raw: String = chars.get(i + 1..i + close).map(|s| s.iter().collect()).unwrap_or_default();
            i += close + 1;
            if raw.starts_with("!--") {
                continue;
            }
            let (name, closing, attrs) = parse_tag(&raw);
            if (name == "script" || name == "style") && !closing {
                // Skip to the matching close tag.
                let needle = format!("</{name}");
                let rest: String = chars.get(i..).map(|s| s.iter().collect()).unwrap_or_default();
                match rest.to_lowercase().find(&needle) {
                    Some(p) => i += rest.get(..p).map(|s| s.chars().count()).unwrap_or(0),
                    None => i = chars.len(),
                }
                continue;
            }
            w.tag(&name, closing, &attrs);
        } else {
            let start = i;
            while i < chars.len() && chars.get(i).copied() != Some('<') {
                i += 1;
            }
            let t: String = chars.get(start..i).map(|s| s.iter().collect()).unwrap_or_default();
            w.text(&unescape(&t));
        }
    }
    w.finish()
}

fn parse_tag(raw: &str) -> (String, bool, String) {
    let raw = raw.trim();
    let (closing, body) = match raw.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, raw),
    };
    let mut parts = body.splitn(2, |c: char| c.is_whitespace());
    let name = parts.next().unwrap_or("").trim_end_matches('/').to_lowercase();
    (name, closing, parts.next().unwrap_or("").to_string())
}

fn attr(attrs: &str, key: &str) -> Option<String> {
    let lower = attrs.to_lowercase();
    let marker = format!("{key}=\"");
    let start = lower.find(&marker)? + marker.len();
    let end = attrs.get(start..)?.find('"')?;
    attrs.get(start..start + end).map(unescape)
}

#[derive(Default)]
struct Writer {
    out: String,
    lists: Vec<(bool, usize)>, // (ordered, counter)
    in_pre: bool,
    pre_diagram: bool,
    /// Inside a `<div class="mermaid">`, which Mermaid's own pages use instead of a `pre`.
    div_diagram: bool,
    in_math: bool,
    in_math_block: bool,
    href: Vec<Option<String>>,
    row: Option<Vec<String>>,
    cell: String,
    in_cell: bool,
    rows_in_table: usize,
    quote: bool,
}

impl Writer {
    fn push(&mut self, s: &str) {
        if self.in_cell {
            self.cell.push_str(s);
        } else {
            self.out.push_str(s);
        }
    }

    fn block_break(&mut self) {
        if self.in_cell {
            return;
        }
        let trimmed = self.out.trim_end_matches([' ', '\t']).len();
        self.out.truncate(trimmed);
        // Opening a paragraph inside a blockquote follows the quote's empty `> ` prefix.
        // It is already a valid start for that paragraph; breaking it here creates empty
        // quote blocks around the actual text.
        if self.quote && self.out.lines().last() == Some(">") {
            return;
        }
        if !self.out.is_empty() && !self.out.ends_with("\n\n") {
            if self.out.ends_with('\n') {
                self.out.push('\n');
            } else {
                self.out.push_str("\n\n");
            }
        }
        if self.quote {
            self.out.push_str("> ");
        }
    }

    fn line_break(&mut self) {
        if self.in_cell {
            self.cell.push(' ');
        } else if !self.out.ends_with('\n') {
            self.out.push('\n');
        }
    }

    fn text(&mut self, t: &str) {
        if self.in_pre || self.in_math_block {
            let s = t.to_string();
            self.push(&s);
            return;
        }
        let mut s = String::new();
        let mut last_space =
            if self.in_cell { self.cell.ends_with(' ') } else { self.out.ends_with(' ') || self.out.ends_with('\n') || self.out.is_empty() };
        for c in t.chars() {
            if c.is_whitespace() {
                if !last_space {
                    s.push(' ');
                    last_space = true;
                }
            } else {
                s.push(c);
                last_space = false;
            }
        }
        if self.in_math {
            // Keep the TeX as written between dollar signs.
            self.push(&s);
        } else {
            self.push(&s);
        }
    }

    fn tag(&mut self, name: &str, closing: bool, attrs: &str) {
        match (name, closing) {
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                self.block_break();
                let level = name.get(1..2).and_then(|d| d.parse::<usize>().ok()).unwrap_or(1);
                self.push(&format!("{} ", "#".repeat(level)));
            }
            // A div with the class `mermaid` holds a diagram, as a pre does. Its text is the source.
            ("div", false) if !self.in_pre && attrs.to_lowercase().contains("mermaid") => {
                self.block_break();
                self.push("```mermaid\n");
                self.in_pre = true;
                self.pre_diagram = true;
                self.div_diagram = true;
            }
            ("div", true) if self.div_diagram => {
                self.div_diagram = false;
                self.in_pre = false;
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.out.push_str("```");
                self.block_break();
            }
            ("p", true) if self.in_math_block => {
                self.in_math_block = false;
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.out.push_str("$$");
                self.block_break();
            }
            ("h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" | "div" | "blockquote", true) => {
                if name == "blockquote" {
                    self.quote = false;
                    // A paragraph end leaves a quote prefix ready for another paragraph.
                    // Drop that unused prefix when the blockquote itself ends.
                    if self.out.ends_with("> ") {
                        self.out.truncate(self.out.len().saturating_sub(2));
                    }
                }
                self.block_break();
            }
            ("p", false) if attrs.to_lowercase().contains("class=\"math\"") => {
                self.block_break();
                self.push("$$\n");
                self.in_math_block = true;
            }
            ("p" | "div", false) => self.block_break(),
            ("blockquote", false) => {
                self.quote = true;
                self.block_break();
            }
            ("br", _) => self.line_break(),
            ("hr", _) => {
                self.block_break();
                self.push("---");
                self.block_break();
            }
            ("ul" | "ol", false) => {
                if !self.lists.is_empty() {
                    let trimmed = self.out.trim_end_matches([' ', '\t']).len();
                    self.out.truncate(trimmed);
                }
                self.lists.push((name == "ol", 0));
                if !self.out.is_empty() && !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
            }
            ("ul" | "ol", true) => {
                self.lists.pop();
                if self.lists.is_empty() {
                    self.block_break();
                }
            }
            ("li", false) => {
                if !self.out.ends_with('\n') && !self.out.is_empty() {
                    self.out.push('\n');
                }
                let depth = self.lists.len().saturating_sub(1);
                let marker = match self.lists.last_mut() {
                    Some((true, n)) => {
                        *n += 1;
                        format!("{n}. ")
                    }
                    _ => "- ".to_string(),
                };
                self.out.push_str(&format!("{}{marker}", "  ".repeat(depth)));
            }
            ("li", true) => {
                let trimmed = self.out.trim_end_matches([' ', '\t']).len();
                self.out.truncate(trimmed);
                self.line_break();
            }
            ("b" | "strong", _) => self.push("**"),
            ("i" | "em", _) => self.push("*"),
            ("s" | "del" | "strike", _) => self.push("~~"),
            ("code", _) if !self.in_pre => self.push("`"),
            ("pre", false) => {
                self.block_break();
                // `diagram` is how NoteCraft has always written one; `mermaid` is how Mermaid's own
                // pages and most other tools do, so a page from elsewhere is read the same.
                let class = attr(attrs, "class").unwrap_or_default();
                self.pre_diagram = class.eq_ignore_ascii_case("diagram") || class.eq_ignore_ascii_case("mermaid");
                let language = if self.pre_diagram { "mermaid" } else { class.as_str() };
                self.push(&format!("```{language}\n"));
                self.in_pre = true;
            }
            ("pre", true) => {
                self.in_pre = false;
                if !self.out.ends_with('\n') {
                    self.out.push('\n');
                }
                self.out.push_str("```");
                self.block_break();
            }
            ("span", false) if attrs.to_lowercase().contains("math") => self.in_math = true,
            ("span", true) => self.in_math = false,
            ("a", false) => {
                let href = attr(attrs, "href");
                self.push("[");
                self.href.push(href);
            }
            ("a", true) => {
                let href = self.href.pop().flatten().unwrap_or_default();
                self.push(&format!("]({href})"));
            }
            ("img", _) => {
                let alt = attr(attrs, "alt").unwrap_or_default();
                let src = attr(attrs, "src").unwrap_or_default();
                self.push(&format!("![{alt}]({src})"));
            }
            ("video" | "audio", false) => {
                let src = attr(attrs, "src").unwrap_or_default();
                self.push(&format!("![{name}]({src})"));
            }
            ("table", false) => {
                self.block_break();
                self.rows_in_table = 0;
            }
            ("table", true) => self.block_break(),
            ("tr", false) => self.row = Some(Vec::new()),
            ("tr", true) => {
                if let Some(cells) = self.row.take() {
                    self.out.push_str(&format!("| {} |\n", cells.join(" | ")));
                    if self.rows_in_table == 0 {
                        self.out.push_str(&format!("|{}\n", "---|".repeat(cells.len().max(1))));
                    }
                    self.rows_in_table += 1;
                }
            }
            ("td" | "th", false) => {
                self.in_cell = true;
                self.cell.clear();
            }
            ("td" | "th", true) => {
                self.in_cell = false;
                let cell = self.cell.trim().to_string();
                if let Some(row) = self.row.as_mut() {
                    row.push(cell);
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> String {
        let mut s = self
            .out
            .lines()
            .map(|l| {
                // A ballot box starts a to-do, whether it is a paragraph of its own or a
                // list item: "&#9744; buy milk" and "<li>&#9744; buy milk</li>" both mean
                // the same thing, and NoteCraft writes the second one.
                let (prefix, rest) = match l.find(['\u{2610}', '\u{2611}', '\u{2612}']) {
                    Some(at) if l.get(..at).is_some_and(|p| p.trim_end().is_empty() || p.trim_end().ends_with('-')) => {
                        (l.get(..at).unwrap_or(""), l.get(at..).unwrap_or(""))
                    }
                    _ => return l.to_string(),
                };
                let indent = prefix.trim_end_matches("- ").trim_end_matches('-');
                if let Some(rest) = rest.strip_prefix("\u{2610} ") {
                    format!("{indent}- [ ] {rest}")
                } else if let Some(rest) = rest.strip_prefix("\u{2611} ").or_else(|| rest.strip_prefix("\u{2612} ")) {
                    format!("{indent}- [x] {rest}")
                } else {
                    l.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        while s.ends_with('\n') || s.ends_with(' ') {
            s.pop();
        }
        s.push('\n');
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>A &amp; B</title>\n<meta name=\"notecraft:id\" content=\"x1\">\n</head>\n<body>\n<h1>A &amp; B</h1>\n</body>\n</html>\n";

    #[test]
    fn reads_and_replaces_the_body() {
        assert_eq!(inner(PAGE), "<h1>A &amp; B</h1>");
        let out = replace_inner(PAGE, "<p>new</p>");
        assert!(out.contains("<title>A &amp; B</title>"));
        assert_eq!(inner(&out), "<p>new</p>");
    }

    #[test]
    fn text_without_a_body_is_made_into_a_page() {
        assert_eq!(inner("<p>x</p>"), "<p>x</p>");
        assert!(replace_inner("", "<p>x</p>").contains("<body>"));
    }

    #[test]
    fn meta_and_title() {
        assert_eq!(title(PAGE).as_deref(), Some("A & B"));
        assert_eq!(meta(PAGE, "notecraft:id").as_deref(), Some("x1"));
        let updated = set_meta(PAGE, "notecraft:updated", "now");
        assert_eq!(meta(&updated, "notecraft:updated").as_deref(), Some("now"));
        let again = set_meta(&updated, "notecraft:updated", "later");
        assert_eq!(meta(&again, "notecraft:updated").as_deref(), Some("later"));
        assert_eq!(again.matches("notecraft:updated").count(), 1);
    }

    #[test]
    fn replace_between_is_safe_when_markers_are_missing() {
        assert_eq!(replace_between("abc", "<x>", "</x>", "n"), "abc");
        assert_eq!(replace_between("a<x>b</x>c", "<x>", "</x>", "n"), "a<x>n</x>c");
    }

    #[test]
    fn numeric_character_references_are_decoded() {
        assert_eq!(unescape("&#9744; to do"), "\u{2610} to do");
        assert_eq!(unescape("&#x2611; done &amp; dusted"), "\u{2611} done & dusted");
        assert_eq!(unescape("&amp;#9744;"), "&#9744;", "an escaped ampersand stays literal");
        for junk in ["&#", "&#;", "&#x;", "&#99999999999;", "&#12345678901;", "&#abc;", "x&#", "&#9744"] {
            let _ = unescape(junk);
        }
        assert_eq!(unescape("&#9744"), "&#9744", "no closing semicolon: left as it is");
    }

    #[test]
    fn to_do_lines_become_task_items() {
        let md = to_markdown("<p>&#9744; write the proposal</p><p>&#9745; ship it</p>");
        assert!(md.contains("- [ ] write the proposal"), "{md}");
        assert!(md.contains("- [x] ship it"), "{md}");
    }

    #[test]
    fn escaping_round_trips() {
        let s = "a < b & \"c\" > d";
        assert_eq!(unescape(&escape(s)), s);
    }

    #[test]
    fn strips_tags() {
        assert_eq!(strip_tags("<p>Hello <b>big</b> world</p>").split_whitespace().collect::<Vec<_>>().join(" "), "Hello big world");
    }

    #[test]
    fn converts_blocks_and_inline_markup() {
        let md = to_markdown("<h1>Title</h1><p>One <b>bold</b> and <i>italic</i> and <code>code</code>.</p><ul><li>a</li><li>b</li></ul>");
        assert!(md.contains("# Title"), "{md}");
        assert!(md.contains("One **bold** and *italic* and `code`."), "{md}");
        assert!(md.contains("- a\n- b"), "{md}");
    }

    #[test]
    fn converts_ordered_and_nested_lists() {
        let md = to_markdown("<ol><li>x</li><li>y<ul><li>z</li></ul></li></ol>");
        assert!(md.contains("1. x"), "{md}");
        assert!(md.contains("2. y"), "{md}");
        assert!(md.contains("  - z"), "{md}");
    }

    #[test]
    fn converts_links_images_and_tables() {
        let md = to_markdown(
            "<p><a href=\"https://x.test\">site</a> <img src=\"assets/a.png\" alt=\"pic\"></p><table><tr><th>A</th><th>B</th></tr><tr><td>1</td><td>2</td></tr></table>",
        );
        assert!(md.contains("[site](https://x.test)"), "{md}");
        assert!(md.contains("![pic](assets/a.png)"), "{md}");
        assert!(md.contains("| A | B |\n|---|---|\n| 1 | 2 |"), "{md}");
    }

    #[test]
    fn keeps_equations_and_diagrams() {
        let md = to_markdown("<span class=\"math\">$ E = mc^2 $</span><pre class=\"diagram\">graph TD; A--&gt;B;</pre>");
        assert!(md.contains("$ E = mc^2 $"), "{md}");
        assert!(md.contains("```mermaid\ngraph TD; A-->B;\n```"), "{md}");
    }

    #[test]
    fn a_diagram_is_read_from_a_pre_or_a_div_by_either_name() {
        let want = "```mermaid\ngraph TD; A-->B;\n```";
        for html in [
            "<pre class=\"diagram\">graph TD; A--&gt;B;</pre>",
            "<pre class=\"mermaid\">graph TD; A--&gt;B;</pre>",
            "<div class=\"mermaid\">graph TD; A--&gt;B;</div>",
            "<p>before</p><div class=\"mermaid\">graph TD; A--&gt;B;</div><p>after</p>",
        ] {
            let md = to_markdown(html);
            assert!(md.contains(want), "{html} gave {md}");
        }
        let md = to_markdown("<p>before</p><div class=\"mermaid\">graph TD; A--&gt;B;</div><p>after</p>");
        assert!(md.contains("before") && md.contains("after"), "the words round it are kept: {md}");
        // An ordinary div is still just a block of text.
        let md = to_markdown("<div class=\"note\">plain words</div>");
        assert!(!md.contains("mermaid") && md.contains("plain words"), "{md}");
    }

    #[test]
    fn drops_scripts_and_styles_and_survives_junk() {
        let md = to_markdown("<p>a</p><script>alert(1)</script><style>p{}</style><p>b</p>");
        assert!(!md.contains("alert"), "{md}");
        assert!(md.contains('a') && md.contains('b'));
        for junk in ["<", "<<>>", "<p", "</p></p></div>", "<li>x", "&", "<script>never closed"] {
            let _ = to_markdown(junk);
        }
    }
}

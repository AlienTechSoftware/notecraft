//! A CommonMark + GFM subset parsed into blocks that a front end can draw.
//!
//! Supported blocks: headings, paragraphs, bullet, numbered and task list items
//! (nested by indentation), block quotes, fenced code (a `mermaid` fence is kept as
//! code with its language), math blocks (`$$ ... $$`), tables, rules and standalone
//! images. Supported inline: bold, italic, strikethrough, code, links, images and
//! `$math$`, and NoteCraft font-size spans. Other HTML is not supported. Also not
//! supported: reference links, nested block quotes and lists inside quotes, setext
//! headings, footnotes.

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub script: Option<Script>,
    pub code: bool,
    pub math: bool,
    /// Font size in points, carried by a NoteCraft inline `<span>`.
    pub font_size: Option<u8>,
    pub font_family: Option<String>,
    /// Link target, if this span is part of a link.
    pub link: Option<String>,
    /// Image source, if this span is an inline image (the text is the alt text).
    pub image: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Script {
    Subscript,
    Superscript,
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
    Image { alt: String, src: String, width: Option<f32>, height: Option<f32> },
}

/// What a code block's language starts with when its lines are numbered: `123|rust`, or `123|` for no language.
/// It is the block's own info string, so both page formats carry it the way they carry any language: the fence
/// in CMGFM, the class of the `pre` in HTML. A page read by something that knows nothing of it shows the code
/// under an odd language name, and loses nothing.
pub const NUMBERED: &str = "123|";

/// A code block's language split into whether its lines are numbered and the language itself.
pub fn code_lang(lang: &str) -> (bool, &str) {
    match lang.strip_prefix(NUMBERED) {
        Some(rest) => (true, rest),
        None => (false, lang),
    }
}

#[derive(Clone, Default)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    script: Option<Script>,
    font_size: Option<u8>,
    font_family: Option<String>,
}

/// Parses inline Markdown into styled spans.
pub fn parse_inline(text: &str) -> Vec<Span> {
    let chars: Vec<char> = text.chars().collect();
    inline(&chars, Style::default(), None, 0)
}

const MAX_DEPTH: usize = 8;

// CommonMark consumes a backslash only before ASCII punctuation.
fn escapes(chars: &[char], at: usize) -> bool {
    chars.get(at) == Some(&'\\') && chars.get(at + 1).is_some_and(char::is_ascii_punctuation)
}

fn find(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    let mut at = from;
    while !pat.is_empty() && at + pat.len() <= chars.len() {
        if escapes(chars, at) {
            at += 2;
        } else if chars.get(at..at + pat.len()) == Some(pat) {
            return Some(at);
        } else {
            at += 1;
        }
    }
    None
}

// Backslashes stay literal inside code and legacy link destinations.
fn find_raw(chars: &[char], from: usize, pat: &[char]) -> Option<usize> {
    if pat.is_empty() || chars.len() < pat.len() {
        return None;
    }
    (from..=chars.len() - pat.len()).find(|&i| chars.get(i..i + pat.len()) == Some(pat))
}

/// Where the run of exactly `run` backticks that closes a code span starts, looking from
/// `from`. A longer or shorter run is part of the code.
fn closing_ticks(chars: &[char], from: usize, run: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '`' {
            let len = chars[i..].iter().take_while(|&&c| c == '`').count();
            if len == run {
                return Some(i);
            }
            i += len;
        } else {
            i += 1;
        }
    }
    None
}

fn slice(chars: &[char], a: usize, b: usize) -> &[char] {
    chars.get(a..b).unwrap_or(&[])
}

fn push_text(out: &mut Vec<Span>, buf: &mut String, st: &Style, link: &Option<String>) {
    if !buf.is_empty() {
        out.push(Span {
            text: std::mem::take(buf),
            bold: st.bold,
            italic: st.italic,
            strike: st.strike,
            underline: st.underline,
            script: st.script,
            font_size: st.font_size,
            font_family: st.font_family.clone(),
            link: link.clone(),
            ..Span::default()
        });
    }
}

fn span_open_end(chars: &[char], start: usize) -> Option<usize> {
    let remaining = chars.get(start..)?;
    let end = remaining.iter().position(|c| *c == '>')?;
    let tag: String = remaining.get(..=end)?.iter().collect();
    tag.to_ascii_lowercase().starts_with("<span ").then_some(start + end + 1)
}

pub fn valid_font_family(name: &str) -> bool {
    !name.is_empty() && name.trim() == name && name.len() <= 128 && !name.chars().any(char::is_control)
}

pub(crate) fn font_family_css(name: &str) -> String {
    let simple = name.split(' ').all(|word| {
        let mut chars = word.chars();
        chars.next().is_some_and(|c| c.is_alphabetic() || c == '_') && chars.all(|c| c.is_alphanumeric() || matches!(c, '-' | '_'))
    });
    if simple { name.to_string() } else { format!("'{}'", name.replace('\\', "\\\\").replace('\'', "\\'")) }
}

pub(crate) fn font_family_from_css(value: &str) -> Option<String> {
    let value = value.trim();
    let name = if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        let mut name = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                let escaped = chars.next()?;
                if !matches!(escaped, '\\' | '\'') {
                    return None;
                }
                name.push(escaped);
            } else if c == '\'' {
                return None;
            } else {
                name.push(c);
            }
        }
        name
    } else {
        if !value.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.')) {
            return None;
        }
        value.to_string()
    };
    valid_font_family(&name).then_some(name)
}

pub(crate) fn style_declarations(style: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (at, c) in style.char_indices() {
        if quoted && c == '\\' && !escaped {
            escaped = true;
            continue;
        }
        if c == '\'' && !escaped {
            quoted = !quoted;
        } else if c == ';' && !quoted {
            result.push(&style[start..at]);
            start = at + 1;
        }
        escaped = false;
    }
    result.push(&style[start..]);
    result
}

fn font_style_open(chars: &[char], start: usize) -> Option<(Option<u8>, Option<String>, usize)> {
    let end = span_open_end(chars, start)?;
    let tag: String = chars.get(start..end)?.iter().collect();
    let lower = tag.to_ascii_lowercase();
    let style_start = lower.find("style=\"")? + "style=\"".len();
    let style_end = lower.get(style_start..)?.find('"')? + style_start;
    let style = crate::html::unescape(tag.get(style_start..style_end)?);
    let mut size = None;
    let mut family = None;
    for declaration in style_declarations(&style).into_iter().filter(|part| !part.trim().is_empty()) {
        let (property, value) = declaration.split_once(':')?;
        let value = value.trim();
        if property.trim().eq_ignore_ascii_case("font-size") {
            let points = value.strip_suffix("pt")?.parse::<u8>().ok()?;
            if !(8..=72).contains(&points) {
                return None;
            }
            size = Some(points);
        } else if property.trim().eq_ignore_ascii_case("font-family") {
            family = Some(font_family_from_css(value)?);
        } else {
            return None;
        }
    }
    (size.is_some() || family.is_some()).then_some((size, family, end))
}

fn font_size_close(chars: &[char], start: usize) -> Option<usize> {
    let close = ['<', '/', 's', 'p', 'a', 'n', '>'];
    let mut depth = 1usize;
    let mut at = start;
    while at < chars.len() {
        if escapes(chars, at) {
            at += 2;
            continue;
        }
        if chars.get(at..at + close.len()) == Some(close.as_slice()) {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                return Some(at);
            }
            at += close.len();
        } else if let Some(after) = span_open_end(chars, at) {
            depth += 1;
            at = after;
        } else {
            at += 1;
        }
    }
    None
}

fn inline_tag_close(chars: &[char], start: usize, open: &[char], close: &[char]) -> Option<usize> {
    let mut depth = 1usize;
    let mut at = start;
    while at < chars.len() {
        if escapes(chars, at) {
            at += 2;
            continue;
        }
        if chars.get(at..at + close.len()) == Some(close) {
            depth -= 1;
            if depth == 0 {
                return Some(at);
            }
            at += close.len();
        } else if chars.get(at..at + open.len()) == Some(open) {
            depth += 1;
            at += open.len();
        } else {
            at += 1;
        }
    }
    None
}

fn inline(chars: &[char], st: Style, link: Option<String>, depth: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars.get(i).copied().unwrap_or(' ');
        let next = chars.get(i + 1).copied();
        if c == '<'
            && depth < MAX_DEPTH
            && let Some((font_size, font_family, content_start)) = font_style_open(chars, i)
            && let Some(content_end) = font_size_close(chars, content_start)
        {
            push_text(&mut out, &mut buf, &st, &link);
            let mut sized = st.clone();
            if font_size.is_some() {
                sized.font_size = font_size;
            }
            if font_family.is_some() {
                sized.font_family = font_family;
            }
            out.extend(inline(slice(chars, content_start, content_end), sized, link.clone(), depth + 1));
            i = content_end + "</span>".chars().count();
            continue;
        }
        if c == '<' && depth < MAX_DEPTH {
            let remaining: String = chars[i..].iter().collect();
            let tag = [("<u>", "</u>", None), ("<sub>", "</sub>", Some(Script::Subscript)), ("<sup>", "</sup>", Some(Script::Superscript))]
                .into_iter()
                .find(|(open, _, _)| remaining.starts_with(open));
            if let Some((open, close, script)) = tag {
                let open_len = open.chars().count();
                let opening: Vec<char> = open.chars().collect();
                let closing: Vec<char> = close.chars().collect();
                if let Some(end) = inline_tag_close(chars, i + open_len, &opening, &closing) {
                    push_text(&mut out, &mut buf, &st, &link);
                    let mut inner = st.clone();
                    if let Some(script) = script {
                        inner.script = Some(script);
                    } else {
                        inner.underline = true;
                    }
                    out.extend(inline(slice(chars, i + open_len, end), inner, link.clone(), depth + 1));
                    i = end + closing.len();
                    continue;
                }
            }
        }
        match c {
            '\\' if escapes(chars, i) => {
                if let Some(n) = next {
                    buf.push(n);
                }
                i += 2;
            }
            '`' => {
                // A code span opens with a run of backticks and closes at the next run of
                // exactly as many, so a backtick inside it is code (issue #107). A run with no
                // partner is literal text.
                let run = chars[i..].iter().take_while(|&&c| c == '`').count();
                if let Some(end) = closing_ticks(chars, i + run, run) {
                    push_text(&mut out, &mut buf, &st, &link);
                    let code: String = slice(chars, i + run, end).iter().collect();
                    // One space each side is padding when both are there, as CommonMark has it,
                    // so a span can start or end with a backtick.
                    let code = match code.strip_prefix(' ').and_then(|c| c.strip_suffix(' ')) {
                        Some(inner) if !code.chars().all(|c| c == ' ') => inner.to_string(),
                        _ => code,
                    };
                    out.push(Span {
                        text: code,
                        code: true,
                        bold: st.bold,
                        italic: st.italic,
                        strike: st.strike,
                        underline: st.underline,
                        script: st.script,
                        font_size: st.font_size,
                        font_family: st.font_family.clone(),
                        link: link.clone(),
                        ..Span::default()
                    });
                    i = end + run;
                } else {
                    buf.extend(std::iter::repeat_n('`', run));
                    i += run;
                }
            }
            // `***x***` is bold and italic together, which is how both marks on one run are written.
            '*' if next == Some('*')
                && chars.get(i + 2) == Some(&'*')
                && depth < MAX_DEPTH
                && find(chars, i + 3, &['*', '*', '*']).is_some_and(|end| end > i + 3) =>
            {
                if let Some(end) = find(chars, i + 3, &['*', '*', '*']) {
                    push_text(&mut out, &mut buf, &st, &link);
                    let inner = Style { bold: true, italic: true, ..st.clone() };
                    out.extend(inline(slice(chars, i + 3, end), inner, link.clone(), depth + 1));
                    i = end + 3;
                }
            }
            '*' | '~' if next == Some(c) && depth < MAX_DEPTH => {
                let pat = [c, c];
                match find(chars, i + 2, &pat) {
                    Some(end) if end > i + 2 => {
                        push_text(&mut out, &mut buf, &st, &link);
                        let mut inner = st.clone();
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
                    push_text(&mut out, &mut buf, &st, &link);
                    let mut inner = st.clone();
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
                    push_text(&mut out, &mut buf, &st, &link);
                    out.push(Span {
                        text: slice(chars, i + 1, end).iter().collect(),
                        math: true,
                        font_size: st.font_size,
                        font_family: st.font_family.clone(),
                        underline: st.underline,
                        script: st.script,
                        link: link.clone(),
                        ..Span::default()
                    });
                    i = end + 1;
                }
                _ => {
                    buf.push(c);
                    i += 1;
                }
            },
            '!' if next == Some('[') => match parse_link(chars, i + 1) {
                Some((alt, url, after)) => {
                    push_text(&mut out, &mut buf, &st, &link);
                    // Preserve the surrounding link when an image is nested in one. This is
                    // how attachment cards (`[![icon](icon) name](file)`) retain their target.
                    out.push(Span {
                        text: alt,
                        image: Some(url),
                        font_size: st.font_size,
                        font_family: st.font_family.clone(),
                        underline: st.underline,
                        script: st.script,
                        link: link.clone(),
                        ..Span::default()
                    });
                    i = after;
                }
                None => {
                    buf.push(c);
                    i += 1;
                }
            },
            '[' if depth < MAX_DEPTH => match parse_link(chars, i) {
                Some((label, url, after)) => {
                    push_text(&mut out, &mut buf, &st, &link);
                    let label_chars: Vec<char> = label.chars().collect();
                    out.extend(inline(&label_chars, st.clone(), Some(url), depth + 1));
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
    push_text(&mut out, &mut buf, &st, &link);
    out
}

/// `[label](url)` starting at `start` (the `[`). Returns label, url and the index after `)`.
fn parse_link(chars: &[char], start: usize) -> Option<(String, String, usize)> {
    // Find the matching label bracket, not the `]` belonging to an image nested
    // inside the label (`[![icon](icon) name](file)`).
    let mut depth = 0usize;
    let mut escaped = false;
    let close = (start + 1..chars.len()).find(|&i| {
        if escaped {
            escaped = false;
            return false;
        }
        if escapes(chars, i) {
            escaped = true;
            return false;
        }
        match chars.get(i) {
            Some('[') => {
                depth += 1;
                false
            }
            Some(']') if depth > 0 => {
                depth -= 1;
                false
            }
            Some(']') => true,
            _ => false,
        }
    })?;
    if chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = find_raw(chars, close + 2, &[')'])?;
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
    let border = t.ends_with('|') && t[..t.len() - 1].chars().rev().take_while(|c| *c == '\\').count() % 2 == 0;
    let t = if border { &t[..t.len() - 1] } else { t };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        // Table quoting is an outer layer, including inside code, maths and destinations.
        if c == '\\' && chars.peek().is_some_and(|c| matches!(c, '\\' | '|')) {
            if let Some(next) = chars.next() {
                cell.push(next);
            }
        } else if c == '|' {
            cells.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
    }
    cells.push(cell.trim().to_string());
    cells
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

fn parse_attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_lowercase();
    let marker = format!("{name}=\"");
    let start = lower.find(&marker)? + marker.len();
    let end = tag.get(start..)?.find('"')?;
    tag.get(start..start + end).map(str::to_string)
}

fn try_parse_img_block(line: &str) -> Option<Block> {
    let trimmed = line.trim();
    if !trimmed.starts_with("<img ") || !trimmed.ends_with('>') {
        return None;
    }
    let src = parse_attr(trimmed, "src")?;
    let alt = parse_attr(trimmed, "alt").unwrap_or_default();
    let width = parse_attr(trimmed, "width").and_then(|w| w.parse::<f32>().ok()).filter(|v| v.is_finite());
    let height = parse_attr(trimmed, "height").and_then(|h| h.parse::<f32>().ok()).filter(|v| v.is_finite());
    Some(Block::Image { alt, src, width, height })
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
        } else if let Some(img_block) = try_parse_img_block(line) {
            out.push(img_block);
            i += 1;
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
                out.push(Block::Image { alt: only.text.clone(), src: src.clone(), width: None, height: None });
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

    #[test]
    fn nested_attachment_image_keeps_file_link() {
        let spans = parse_inline("[![ ](assets/file.docx.png) file.docx](assets/file.docx)");
        let image = spans.iter().find(|span| span.image.is_some()).expect("image span");
        assert_eq!(image.image.as_deref(), Some("assets/file.docx.png"));
        assert_eq!(image.link.as_deref(), Some("assets/file.docx"));
    }

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
    fn font_size_spans_round_trip_and_nest() {
        let source = "<span style=\"font-size: 18pt\">outer <span style=\"font-size: 12pt\">inner</span> outer</span>";
        let spans = parse_inline(source);
        assert!(spans.iter().any(|span| span.text == "outer " && span.font_size == Some(18)));
        assert!(spans.iter().any(|span| span.text == "inner" && span.font_size == Some(12)));
        assert!(spans.iter().any(|span| span.text == " outer" && span.font_size == Some(18)));
        assert_eq!(parse_inline(&crate::render::spans_to_markdown(&spans)), spans);

        let invalid = parse_inline("<span style=\"font-size: 73pt\">too large</span>");
        assert_eq!(text_of(&invalid), "<span style=\"font-size: 73pt\">too large</span>");
        assert!(invalid.iter().all(|span| span.font_size.is_none()));
    }

    #[test]
    fn nested_identical_inline_tags_preserve_all_text_and_marks() {
        for tag in ["u", "sub", "sup"] {
            let source = format!("<{tag}>before <{tag}>inside</{tag}> after</{tag}>");
            let spans = parse_inline(&source);
            assert_eq!(text_of(&spans), "before inside after", "{tag}");
            for span in &spans {
                match tag {
                    "u" => assert!(span.underline),
                    "sub" => assert_eq!(span.script, Some(Script::Subscript)),
                    "sup" => assert_eq!(span.script, Some(Script::Superscript)),
                    _ => {}
                }
            }
        }
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
        assert_eq!(b, vec![Block::Image { alt: "alt text".into(), src: "assets/x.png".into(), width: None, height: None }]);
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

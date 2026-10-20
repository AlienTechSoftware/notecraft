//! Syntax colours for the code view: which part of a file is a tag, a name, a string, a comment.
//!
//! Each format has a small hand-written tokenizer. They read text and say, as byte ranges, what
//! kind of thing each stretch is; nothing here draws, so it is tested without a window. A
//! tokenizer never fails and never panics: text it does not understand is left plain, and a
//! half-typed file (an open tag, a string with no end) is coloured as far as it goes.
//!
//! This is for reading a file, not for checking it. The colours do not claim the text is valid.

use std::ops::Range;

/// The formats the code view knows how to colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Markdown,
    Html,
    /// SVG and other XML.
    Xml,
    Json,
    Toml,
    Yaml,
    Mermaid,
    Plain,
}

impl Lang {
    pub fn label(self) -> &'static str {
        match self {
            Lang::Markdown => "Markdown",
            Lang::Html => "HTML",
            Lang::Xml => "XML",
            Lang::Json => "JSON",
            Lang::Toml => "TOML",
            Lang::Yaml => "YAML",
            Lang::Mermaid => "Mermaid",
            Lang::Plain => "Plain text",
        }
    }

    /// The format a fence or a file extension names, if it is one of ours.
    pub fn named(name: &str) -> Option<Lang> {
        match name.trim().to_ascii_lowercase().as_str() {
            "md" | "markdown" | "cmgfm" => Some(Lang::Markdown),
            "html" | "htm" | "xhtml" => Some(Lang::Html),
            "svg" | "xml" | "xsl" | "rss" | "atom" => Some(Lang::Xml),
            "json" | "jsonc" | "json5" => Some(Lang::Json),
            "toml" => Some(Lang::Toml),
            "yml" | "yaml" => Some(Lang::Yaml),
            "mmd" | "mermaid" => Some(Lang::Mermaid),
            _ => None,
        }
    }

    /// The format of a file by its name.
    pub fn of_file(name: &str) -> Lang {
        name.rsplit_once('.').and_then(|(_, ext)| Lang::named(ext)).unwrap_or(Lang::Plain)
    }
}

/// What a stretch of text is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    /// A tag name, or a key that opens a block.
    Tag,
    /// An attribute name, or a key.
    Attr,
    Str,
    Comment,
    Number,
    /// A word the language reserves.
    Keyword,
    /// Brackets, arrows, separators.
    Punct,
    Heading,
    Emphasis,
    /// Code in a text file.
    Code,
    Link,
    /// A declaration, a front matter fence, an entity.
    Meta,
}

/// One coloured stretch: byte offsets into the text, on character boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub class: Class,
}

/// Files bigger than this are shown plain: colouring is re-done as they are typed in.
pub const MAX_COLOURED: usize = 160 * 1024;

/// The coloured stretches of `text`, in order and not overlapping. What is not in one is plain.
pub fn spans(lang: Lang, text: &str) -> Vec<Span> {
    if text.len() > MAX_COLOURED {
        return Vec::new();
    }
    let mut out = Vec::new();
    match lang {
        Lang::Markdown => markdown(text, 0, &mut out),
        Lang::Html => markup(text, 0, &mut out),
        Lang::Xml => markup(text, 0, &mut out),
        Lang::Json => json(text, 0, &mut out),
        Lang::Toml => toml(text, 0, &mut out),
        Lang::Yaml => yaml(text, 0, &mut out),
        Lang::Mermaid => mermaid(text, 0, &mut out),
        Lang::Plain => {}
    }
    // Sorted, and cut so that none overlaps the one before: the layout trusts that.
    out.sort_by_key(|s| (s.range.start, s.range.end));
    let mut clean: Vec<Span> = Vec::with_capacity(out.len());
    for mut s in out {
        if let Some(last) = clean.last()
            && s.range.start < last.range.end
        {
            s.range.start = last.range.end;
        }
        if s.range.start < s.range.end && text.is_char_boundary(s.range.start) && text.is_char_boundary(s.range.end) {
            clean.push(s);
        }
    }
    clean
}

fn push(out: &mut Vec<Span>, base: usize, start: usize, end: usize, class: Class) {
    if start < end {
        out.push(Span { range: base + start..base + end, class });
    }
}

// ---------------------------------------------------------------------------------- markup

fn markup(text: &str, base: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        if b[i] == b'<' {
            if text[i..].starts_with("<!--") {
                let end = text[i + 4..].find("-->").map_or(n, |e| i + 4 + e + 3);
                push(out, base, i, end, Class::Comment);
                i = end;
                continue;
            }
            if text[i..].starts_with("<![CDATA[") {
                let end = text[i..].find("]]>").map_or(n, |e| i + e + 3);
                push(out, base, i, end, Class::Str);
                i = end;
                continue;
            }
            if text[i..].starts_with("<!") || text[i..].starts_with("<?") {
                let end = text[i..].find('>').map_or(n, |e| i + e + 1);
                push(out, base, i, end, Class::Meta);
                i = end;
                continue;
            }
            let name_at = if b.get(i + 1) == Some(&b'/') { i + 2 } else { i + 1 };
            if b.get(name_at).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
                i = tag(text, i, name_at, base, out);
                continue;
            }
        }
        if b[i] == b'&'
            && let Some(semi) = text[i..].find(';').filter(|s| *s <= 10 && *s > 1)
            && text[i + 1..i + semi].bytes().all(|c| c.is_ascii_alphanumeric() || c == b'#')
        {
            push(out, base, i, i + semi + 1, Class::Meta);
            i += semi + 1;
            continue;
        }
        i += 1;
    }
}

/// A tag from its `<` to its `>`, or the end of the text. Returns where it stopped.
fn tag(text: &str, open: usize, name_at: usize, base: usize, out: &mut Vec<Span>) -> usize {
    let b = text.as_bytes();
    let n = b.len();
    push(out, base, open, name_at, Class::Punct);
    let mut i = name_at;
    while i < n && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/') {
        i += 1;
    }
    push(out, base, name_at, i, Class::Tag);
    while i < n {
        match b[i] {
            b' ' | b'\t' | b'\r' | b'\n' => i += 1,
            b'>' => {
                push(out, base, i, i + 1, Class::Punct);
                return i + 1;
            }
            b'/' => {
                push(out, base, i, i + 1, Class::Punct);
                i += 1;
            }
            b'=' => {
                push(out, base, i, i + 1, Class::Punct);
                i += 1;
                while i < n && matches!(b[i], b' ' | b'\t') {
                    i += 1;
                }
                if i < n && (b[i] == b'"' || b[i] == b'\'') {
                    let q = b[i];
                    let end = text[i + 1..].find(q as char).map_or(n, |e| i + 1 + e + 1);
                    push(out, base, i, end, Class::Str);
                    i = end;
                } else {
                    let start = i;
                    while i < n && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>') {
                        i += 1;
                    }
                    push(out, base, start, i, Class::Str);
                }
            }
            _ => {
                let start = i;
                while i < n && !matches!(b[i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/' | b'=') {
                    i += 1;
                }
                if i == start {
                    i += 1;
                } else {
                    push(out, base, start, i, Class::Attr);
                }
            }
        }
    }
    n
}

// ------------------------------------------------------------------------------------ json

fn json(text: &str, base: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        match b[i] {
            b'"' => {
                let mut j = i + 1;
                while j < n && b[j] != b'"' && b[j] != b'\n' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = (j + 1).min(n);
                let after = text[end..].trim_start_matches([' ', '\t']);
                let key = after.starts_with(':');
                push(out, base, i, end, if key { Class::Attr } else { Class::Str });
                i = end;
            }
            b'/' if text[i..].starts_with("//") => {
                let end = text[i..].find('\n').map_or(n, |e| i + e);
                push(out, base, i, end, Class::Comment);
                i = end;
            }
            b'/' if text[i..].starts_with("/*") => {
                let end = text[i + 2..].find("*/").map_or(n, |e| i + 2 + e + 2);
                push(out, base, i, end, Class::Comment);
                i = end;
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                push(out, base, i, i + 1, Class::Punct);
                i += 1;
            }
            c if c == b'-' || c.is_ascii_digit() => {
                let start = i;
                i += 1;
                while i < n && (b[i].is_ascii_digit() || matches!(b[i], b'.' | b'e' | b'E' | b'+' | b'-')) {
                    i += 1;
                }
                push(out, base, start, i, Class::Number);
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < n && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                if matches!(&text[start..i], "true" | "false" | "null") {
                    push(out, base, start, i, Class::Keyword);
                }
            }
            _ => i += 1,
        }
    }
}

// ---------------------------------------------------------------------------- toml and yaml

/// The end of the line that starts at `from`, excluding its newline.
fn eol(text: &str, from: usize) -> usize {
    text[from..].find('\n').map_or(text.len(), |e| from + e)
}

/// A value on one line: strings, numbers, words, and a comment at its end.
fn value(text: &str, start: usize, end: usize, base: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    let mut i = start;
    while i < end {
        match b[i] {
            b'"' | b'\'' => {
                let q = b[i];
                let mut j = i + 1;
                while j < end && b[j] != q {
                    j += if b[j] == b'\\' && q == b'"' { 2 } else { 1 };
                }
                let stop = (j + 1).min(end);
                push(out, base, i, stop, Class::Str);
                i = stop;
            }
            b'#' if i == start || matches!(b[i - 1], b' ' | b'\t') => {
                push(out, base, i, end, Class::Comment);
                return;
            }
            c if c.is_ascii_digit() || (c == b'-' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) => {
                let s = i;
                i += 1;
                while i < end && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'.' | b'_' | b':' | b'-' | b'+')) {
                    i += 1;
                }
                push(out, base, s, i, Class::Number);
            }
            c if c.is_ascii_alphabetic() => {
                let s = i;
                while i < end && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if matches!(&text[s..i], "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~") {
                    push(out, base, s, i, Class::Keyword);
                }
            }
            b'[' | b']' | b'{' | b'}' | b',' => {
                push(out, base, i, i + 1, Class::Punct);
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn toml(text: &str, base: usize, out: &mut Vec<Span>) {
    let mut at = 0;
    while at < text.len() {
        let end = eol(text, at);
        let line = &text[at..end];
        let trimmed = line.trim_start();
        let lead = line.len() - trimmed.len();
        if trimmed.starts_with('#') {
            push(out, base, at + lead, end, Class::Comment);
        } else if trimmed.starts_with('[') {
            let close = trimmed.rfind(']').map_or(trimmed.len(), |c| c + 1);
            push(out, base, at + lead, at + lead + close, Class::Tag);
            value(text, at + lead + close, end, base, out);
        } else if let Some(eq) = trimmed.find('=') {
            push(out, base, at + lead, at + lead + eq, Class::Attr);
            push(out, base, at + lead + eq, at + lead + eq + 1, Class::Punct);
            value(text, at + lead + eq + 1, end, base, out);
        }
        at = end + 1;
    }
}

fn yaml(text: &str, base: usize, out: &mut Vec<Span>) {
    let mut at = 0;
    while at < text.len() {
        let end = eol(text, at);
        let line = &text[at..end];
        let trimmed = line.trim_start();
        let mut lead = line.len() - trimmed.len();
        if trimmed.starts_with('#') {
            push(out, base, at + lead, end, Class::Comment);
        } else if trimmed == "---" || trimmed == "..." {
            push(out, base, at + lead, end, Class::Meta);
        } else {
            let mut rest = trimmed;
            if let Some(after) = rest.strip_prefix("- ") {
                push(out, base, at + lead, at + lead + 1, Class::Punct);
                lead += rest.len() - after.len();
                rest = after;
            }
            // `key: value`, where the colon is followed by a space or the end of the line.
            let colon = rest.char_indices().find(|(i, c)| *c == ':' && rest[i + 1..].chars().next().is_none_or(char::is_whitespace)).map(|(i, _)| i);
            match colon {
                Some(c) if !rest.starts_with(['"', '\'']) || rest[..c].ends_with(['"', '\'']) => {
                    push(out, base, at + lead, at + lead + c, Class::Attr);
                    push(out, base, at + lead + c, at + lead + c + 1, Class::Punct);
                    value(text, at + lead + c + 1, end, base, out);
                }
                _ => value(text, at + lead, end, base, out),
            }
        }
        at = end + 1;
    }
}

// -------------------------------------------------------------------------------- mermaid

const MERMAID_WORDS: &[&str] = &[
    "graph",
    "flowchart",
    "sequenceDiagram",
    "classDiagram",
    "stateDiagram",
    "stateDiagram-v2",
    "erDiagram",
    "gantt",
    "pie",
    "mindmap",
    "timeline",
    "journey",
    "xychart-beta",
    "quadrantChart",
    "sankey-beta",
    "gitGraph",
    "title",
    "section",
    "subgraph",
    "end",
    "participant",
    "actor",
    "class",
    "direction",
    "dateFormat",
    "axisFormat",
    "x-axis",
    "y-axis",
    "bar",
    "line",
    "note",
    "Note",
    "loop",
    "alt",
    "else",
    "opt",
    "par",
    "and",
    "rect",
    "activate",
    "deactivate",
    "autonumber",
    "showData",
    "style",
    "classDef",
    "linkStyle",
    "click",
    "TD",
    "TB",
    "BT",
    "LR",
    "RL",
    "over",
    "of",
    "left",
    "right",
    "quadrant-1",
    "quadrant-2",
    "quadrant-3",
    "quadrant-4",
    "interpolate",
];

fn mermaid(text: &str, base: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    let n = b.len();
    let mut i = 0;
    while i < n {
        match b[i] {
            b'%' if b.get(i + 1) == Some(&b'%') => {
                let end = eol(text, i);
                push(out, base, i, end, Class::Comment);
                i = end;
            }
            b'"' => {
                let end = text[i + 1..].find('"').map_or(eol(text, i), |e| i + 1 + e + 1);
                push(out, base, i, end, Class::Str);
                i = end;
            }
            b'[' | b'(' | b'{' | b'>' if matches!(b[i], b'[' | b'(' | b'{') => {
                // A node's text, up to the bracket that closes it on this line.
                let close = match b[i] {
                    b'[' => ']',
                    b'(' => ')',
                    _ => '}',
                };
                let line_end = eol(text, i);
                let end = text[i + 1..line_end].rfind(close).map_or(line_end, |e| i + 1 + e + 1);
                push(out, base, i, i + 1, Class::Punct);
                push(out, base, i + 1, end.saturating_sub(1).max(i + 1), Class::Str);
                push(out, base, end.saturating_sub(1).max(i + 1), end, Class::Punct);
                i = end;
            }
            b'|' => {
                let line_end = eol(text, i);
                if let Some(e) = text[i + 1..line_end].find('|') {
                    push(out, base, i, i + 1 + e + 1, Class::Str);
                    i += e + 2;
                } else {
                    i += 1;
                }
            }
            b'-' | b'=' | b'.' | b'~' | b'<' | b'>' | b'o' | b'x'
                if matches!(b[i], b'-' | b'=' | b'~') && text[i..].starts_with(['-', '=', '~']) && arrow_len(&text[i..]) >= 2 =>
            {
                let len = arrow_len(&text[i..]);
                push(out, base, i, i + len, Class::Punct);
                i += len;
            }
            c if c.is_ascii_alphabetic() => {
                let start = i;
                while i < n && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'_' | b'-')) {
                    i += 1;
                }
                if MERMAID_WORDS.contains(&&text[start..i]) {
                    push(out, base, start, i, Class::Keyword);
                }
            }
            c if c.is_ascii_digit() => {
                let start = i;
                while i < n && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                push(out, base, start, i, Class::Number);
            }
            _ => i += 1,
        }
    }
}

/// How many bytes of an arrow or link start `s`: `-->`, `---`, `==>`, `-.->`, `->>`, `--x`.
fn arrow_len(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() && matches!(b[i], b'-' | b'=' | b'.' | b'~') {
        i += 1;
    }
    if i >= 2 || (i == 1 && b.get(1) == Some(&b'>')) {
        while i < b.len() && matches!(b[i], b'>' | b'x' | b'o' | b'|') && b[i] != b'|' {
            i += 1;
            if b.get(i) != Some(&b'>') {
                break;
            }
        }
        i
    } else {
        0
    }
}

// ------------------------------------------------------------------------------- markdown

fn markdown(text: &str, base: usize, out: &mut Vec<Span>) {
    let mut at = 0;
    let mut fence: Option<(char, usize, Option<Lang>, usize)> = None; // marker, length, language, where the content starts
    let mut first = true;
    // Front matter: `---` on the first line, through the next `---`.
    if text.starts_with("---\n") || text.starts_with("---\r\n") {
        let open_end = eol(text, 0);
        let mut scan = open_end + 1;
        while scan <= text.len() {
            let end = eol(text, scan);
            if text[scan..end].trim_end() == "---" {
                push(out, base, 0, open_end, Class::Meta);
                yaml(&text[open_end + 1..scan], base + open_end + 1, out);
                push(out, base, scan, end, Class::Meta);
                at = end + 1;
                first = false;
                break;
            }
            if end >= text.len() {
                break;
            }
            scan = end + 1;
        }
    }
    let _ = first;
    while at < text.len() {
        let end = eol(text, at);
        let line = &text[at..end];
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if let Some((marker, len, lang, content)) = fence {
            let closes =
                indent <= 3 && trimmed.chars().take_while(|c| *c == marker).count() >= len && trimmed.trim_start_matches(marker).trim().is_empty();
            if closes {
                if let Some(l) = lang {
                    sub(text, content, at, l, base, out);
                } else {
                    push(out, base, content, at, Class::Code);
                }
                push(out, base, at, end, Class::Meta);
                fence = None;
            }
            at = end + 1;
            continue;
        }
        let marker = trimmed.chars().next().filter(|c| *c == '`' || *c == '~');
        if let Some(m) = marker.filter(|_| indent <= 3) {
            let len = trimmed.chars().take_while(|c| *c == m).count();
            if len >= 3 {
                let info = trimmed[len..].trim();
                let word = info.split_whitespace().next().unwrap_or("");
                push(out, base, at + indent, at + indent + len, Class::Meta);
                push(out, base, at + indent + len + (trimmed[len..].len() - trimmed[len..].trim_start().len()), end, Class::Keyword);
                fence = Some((m, len, Lang::named(word), end + 1));
                at = end + 1;
                continue;
            }
        }
        block_line(text, at, end, indent, base, out);
        at = end + 1;
    }
    // A fence never closed: what follows it is still code.
    if let Some((_, _, lang, content)) = fence {
        let stop = text.len();
        if content < stop {
            match lang {
                Some(l) => sub(text, content, stop, l, base, out),
                None => push(out, base, content, stop, Class::Code),
            }
        }
    }
}

/// Colours the text between two offsets as another language, in place.
fn sub(text: &str, from: usize, to: usize, lang: Lang, base: usize, out: &mut Vec<Span>) {
    let to = to.min(text.len());
    if from >= to || !text.is_char_boundary(from) || !text.is_char_boundary(to) {
        return;
    }
    let part = &text[from..to];
    match lang {
        Lang::Html | Lang::Xml => markup(part, base + from, out),
        Lang::Json => json(part, base + from, out),
        Lang::Toml => toml(part, base + from, out),
        Lang::Yaml => yaml(part, base + from, out),
        Lang::Mermaid => mermaid(part, base + from, out),
        Lang::Markdown | Lang::Plain => push(out, base, from, to, Class::Code),
    }
}

/// One line of Markdown outside a fence.
fn block_line(text: &str, at: usize, end: usize, indent: usize, base: usize, out: &mut Vec<Span>) {
    let line = &text[at..end];
    let trimmed = &line[indent..];
    let start = at + indent;
    if trimmed.starts_with('#') {
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes <= 6 && trimmed[hashes..].starts_with([' ', '\t']) || trimmed.len() == hashes {
            push(out, base, start, end, Class::Heading);
            return;
        }
    }
    if trimmed.len() >= 3 && trimmed.chars().all(|c| matches!(c, '-' | '*' | '_' | ' ')) && trimmed.chars().filter(|c| *c != ' ').count() >= 3 {
        push(out, base, start, end, Class::Punct);
        return;
    }
    if trimmed.starts_with("$$") {
        push(out, base, start, end, Class::Keyword);
        return;
    }
    let mut from = start;
    let mut body = trimmed;
    while let Some(rest) = body.strip_prefix('>') {
        push(out, base, from, from + 1, Class::Punct);
        from += 1 + (rest.len() - rest.trim_start_matches(' ').len());
        body = rest.trim_start_matches(' ');
    }
    // A list marker, and a task box after it.
    let marker_len = if body.starts_with(['-', '*', '+']) && body[1..].starts_with(' ') {
        Some(1)
    } else {
        let digits = body.chars().take_while(char::is_ascii_digit).count();
        (digits > 0 && body[digits..].starts_with(['.', ')']) && body[digits + 1..].starts_with(' ')).then_some(digits + 1)
    };
    if let Some(m) = marker_len {
        push(out, base, from, from + m, Class::Punct);
        from += m + 1;
        body = &body[m + 1..];
        for task in ["[ ]", "[x]", "[X]"] {
            if body.starts_with(task) {
                push(out, base, from, from + 3, Class::Keyword);
                from += 3;
                body = &body[3..];
                break;
            }
        }
    }
    if body.starts_with('|') {
        let mut off = 0;
        for (i, c) in body.char_indices() {
            if c == '|' {
                push(out, base, from + i, from + i + 1, Class::Punct);
            }
            off = i;
        }
        let _ = off;
    }
    inline(text, from, end, base, out);
}

/// Inline Markdown on one line: code, emphasis, links, maths, comments, tags.
fn inline(text: &str, from: usize, end: usize, base: usize, out: &mut Vec<Span>) {
    let b = text.as_bytes();
    let mut i = from;
    while i < end {
        match b[i] {
            b'`' => {
                let run = b[i..end].iter().take_while(|c| **c == b'`').count();
                let closing = "`".repeat(run);
                match text[i + run..end].find(&closing) {
                    Some(e) => {
                        push(out, base, i, i + run + e + run, Class::Code);
                        i += run + e + run;
                    }
                    None => i += run,
                }
            }
            b'*' | b'_' => {
                let m = b[i];
                let run = b[i..end].iter().take_while(|c| **c == m).count();
                // Emphasis opens on a marker with a word after it, and closes on the same one.
                if run <= 3
                    && b.get(i + run).is_some_and(|c| !c.is_ascii_whitespace())
                    && (i == from || !b[i - 1].is_ascii_alphanumeric() || m == b'*')
                {
                    let closing = String::from_utf8(vec![m; run]).unwrap_or_default();
                    if let Some(e) = text[i + run..end].find(&closing).filter(|e| *e > 0) {
                        push(out, base, i, i + run + e + run, Class::Emphasis);
                        i += run + e + run;
                        continue;
                    }
                }
                i += run;
            }
            b'~' if text[i..end].starts_with("~~") => match text[i + 2..end].find("~~") {
                Some(e) => {
                    push(out, base, i, i + 2 + e + 2, Class::Comment);
                    i += 2 + e + 2;
                }
                None => i += 2,
            },
            b'$' if b.get(i + 1).is_some_and(|c| !c.is_ascii_whitespace() && *c != b'$') => match text[i + 1..end].find('$') {
                Some(e) if e > 0 => {
                    push(out, base, i, i + 1 + e + 1, Class::Keyword);
                    i += 1 + e + 1;
                }
                _ => i += 1,
            },
            b'[' => {
                // `[text](target)`: the text is the link, the target a string.
                if let Some(close) = text[i + 1..end].find("](") {
                    let t_end = i + 1 + close;
                    if let Some(paren) = text[t_end + 2..end].find(')') {
                        push(out, base, i, t_end + 1, Class::Link);
                        push(out, base, t_end + 1, t_end + 2, Class::Punct);
                        push(out, base, t_end + 2, t_end + 2 + paren, Class::Str);
                        push(out, base, t_end + 2 + paren, t_end + 3 + paren, Class::Punct);
                        i = t_end + 3 + paren;
                        continue;
                    }
                }
                i += 1;
            }
            b'!' if b.get(i + 1) == Some(&b'[') => {
                push(out, base, i, i + 1, Class::Punct);
                i += 1;
            }
            b'<' if text[i..end].starts_with("<!--") => {
                let e = text[i..end].find("-->").map_or(end, |e| i + e + 3);
                push(out, base, i, e, Class::Comment);
                i = e;
            }
            b'<' if b.get(i + 1).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'/') => {
                // A tag in the line, up to its `>`.
                if let Some(close) = text[i..end].find('>') {
                    markup(&text[i..i + close + 1], base + i, out);
                    i += close + 1;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(lang: Lang, text: &str) -> Vec<(String, Class)> {
        spans(lang, text).into_iter().map(|s| (text[s.range].to_string(), s.class)).collect()
    }

    fn has(lang: Lang, text: &str, what: &str, class: Class) -> bool {
        classes(lang, text).iter().any(|(t, c)| t == what && *c == class)
    }

    #[test]
    fn a_file_is_coloured_as_what_its_name_says() {
        assert_eq!(Lang::of_file("index.md"), Lang::Markdown);
        assert_eq!(Lang::of_file("index.html"), Lang::Html);
        assert_eq!(Lang::of_file("ink.svg"), Lang::Xml);
        assert_eq!(Lang::of_file("assets/data.JSON"), Lang::Json);
        assert_eq!(Lang::of_file("_node.toml"), Lang::Toml);
        assert_eq!(Lang::of_file("a.yml"), Lang::Yaml);
        assert_eq!(Lang::of_file("flow.mmd"), Lang::Mermaid);
        assert_eq!(Lang::of_file("notes.txt"), Lang::Plain);
        assert_eq!(Lang::of_file("noextension"), Lang::Plain);
    }

    #[test]
    fn html_tags_attributes_strings_comments_and_entities() {
        let html = "<!-- note --><div class=\"nc-box\" data-x=3><p>a &amp; b</p><br/></div>";
        assert!(has(Lang::Html, html, "<!-- note -->", Class::Comment));
        assert!(has(Lang::Html, html, "div", Class::Tag));
        assert!(has(Lang::Html, html, "class", Class::Attr));
        assert!(has(Lang::Html, html, "\"nc-box\"", Class::Str));
        assert!(has(Lang::Html, html, "data-x", Class::Attr));
        assert!(has(Lang::Html, html, "3", Class::Str), "an unquoted value");
        assert!(has(Lang::Html, html, "&amp;", Class::Meta));
        assert!(has(Lang::Html, html, "br", Class::Tag));
    }

    #[test]
    fn svg_and_xml_declarations_and_namespaces() {
        let svg = "<?xml version=\"1.0\"?>\n<svg xmlns:nc=\"https://notecraft.org/ns/ink\"><path nc:tool=\"pen\" d=\"M0 0\"/></svg>";
        assert!(has(Lang::Xml, svg, "<?xml version=\"1.0\"?>", Class::Meta));
        assert!(has(Lang::Xml, svg, "svg", Class::Tag));
        assert!(has(Lang::Xml, svg, "xmlns:nc", Class::Attr));
        assert!(has(Lang::Xml, svg, "nc:tool", Class::Attr));
        assert!(has(Lang::Xml, svg, "\"pen\"", Class::Str));
    }

    #[test]
    fn a_half_typed_tag_is_coloured_as_far_as_it_goes_and_does_not_panic() {
        for text in ["<div class=\"unclosed", "<p", "<", "<!--", "<![CDATA[x", "<a href=", "&", "&amp", "<div =", "<é class=\"ü\">"] {
            let _ = spans(Lang::Html, text);
        }
        assert!(has(Lang::Html, "<div class=\"unclosed", "\"unclosed", Class::Str));
    }

    #[test]
    fn json_keys_strings_numbers_words_and_punctuation() {
        let j = "{\"name\": \"x\", \"n\": -1.5e3, \"ok\": true, \"none\": null, \"list\": [1, 2]}";
        assert!(has(Lang::Json, j, "\"name\"", Class::Attr), "a key");
        assert!(has(Lang::Json, j, "\"x\"", Class::Str), "a value");
        assert!(has(Lang::Json, j, "-1.5e3", Class::Number));
        assert!(has(Lang::Json, j, "true", Class::Keyword));
        assert!(has(Lang::Json, j, "null", Class::Keyword));
        assert!(has(Lang::Json, j, "{", Class::Punct));
    }

    #[test]
    fn toml_tables_keys_values_and_comments() {
        let t = "# a node\n[folder]\ncolor = \"teal\"  # why\norder = 10\nopen = true\n";
        assert!(has(Lang::Toml, t, "# a node", Class::Comment));
        assert!(has(Lang::Toml, t, "[folder]", Class::Tag));
        assert!(has(Lang::Toml, t, "color ", Class::Attr));
        assert!(has(Lang::Toml, t, "\"teal\"", Class::Str));
        assert!(has(Lang::Toml, t, "# why", Class::Comment));
        assert!(has(Lang::Toml, t, "10", Class::Number));
        assert!(has(Lang::Toml, t, "true", Class::Keyword));
    }

    #[test]
    fn yaml_keys_values_lists_and_comments() {
        let y = "title: \"Format ideas\"\norder: 10\ntags: []\n# note\n- item: yes\n";
        assert!(has(Lang::Yaml, y, "title", Class::Attr));
        assert!(has(Lang::Yaml, y, "\"Format ideas\"", Class::Str));
        assert!(has(Lang::Yaml, y, "10", Class::Number));
        assert!(has(Lang::Yaml, y, "# note", Class::Comment));
        assert!(has(Lang::Yaml, y, "yes", Class::Keyword));
    }

    #[test]
    fn markdown_front_matter_headings_lists_links_and_code() {
        let md = "---\nid: abc\ntitle: \"T\"\n---\n\n# Heading\n\n- [ ] to do\n- **bold** and *it* and `code` and [link](https://x.y)\n> quoted\n\n$$\nx^2\n$$\n";
        assert!(has(Lang::Markdown, md, "---", Class::Meta));
        assert!(has(Lang::Markdown, md, "id", Class::Attr), "front matter is YAML");
        assert!(has(Lang::Markdown, md, "# Heading", Class::Heading));
        assert!(has(Lang::Markdown, md, "[ ]", Class::Keyword));
        assert!(has(Lang::Markdown, md, "**bold**", Class::Emphasis));
        assert!(has(Lang::Markdown, md, "*it*", Class::Emphasis));
        assert!(has(Lang::Markdown, md, "`code`", Class::Code));
        assert!(has(Lang::Markdown, md, "[link]", Class::Link));
        assert!(has(Lang::Markdown, md, "https://x.y", Class::Str));
        assert!(has(Lang::Markdown, md, ">", Class::Punct));
        assert!(has(Lang::Markdown, md, "$$", Class::Keyword));
    }

    #[test]
    fn a_fenced_block_is_coloured_as_its_own_language() {
        let md = "text\n\n```mermaid\nflowchart TD\n  A[Start] --> B\n```\n\n```json\n{\"a\": 1}\n```\n\n```\nplain code\n```\n";
        assert!(has(Lang::Markdown, md, "flowchart", Class::Keyword), "a mermaid fence is Mermaid");
        assert!(has(Lang::Markdown, md, "-->", Class::Punct));
        assert!(has(Lang::Markdown, md, "\"a\"", Class::Attr), "a json fence is JSON");
        assert!(has(Lang::Markdown, md, "plain code\n", Class::Code), "an untyped fence is code");
    }

    #[test]
    fn an_unclosed_fence_is_still_code_and_nothing_hangs() {
        let md = "```json\n{\"a\": 1}\n";
        assert!(has(Lang::Markdown, md, "\"a\"", Class::Attr));
        let _ = spans(Lang::Markdown, "```");
        let _ = spans(Lang::Markdown, "---\nunclosed front matter");
    }

    #[test]
    fn mermaid_keywords_arrows_labels_and_comments() {
        let m = "flowchart TD\n  %% a note\n  A[Start] -->|yes| B{Done?}\n  B ==> C((End))\n  pie\n  \"HTML\" : 62\n";
        assert!(has(Lang::Mermaid, m, "flowchart", Class::Keyword));
        assert!(has(Lang::Mermaid, m, "TD", Class::Keyword));
        assert!(has(Lang::Mermaid, m, "%% a note", Class::Comment));
        assert!(has(Lang::Mermaid, m, "-->", Class::Punct));
        assert!(has(Lang::Mermaid, m, "|yes|", Class::Str));
        assert!(has(Lang::Mermaid, m, "==>", Class::Punct));
        assert!(has(Lang::Mermaid, m, "\"HTML\"", Class::Str));
        assert!(has(Lang::Mermaid, m, "62", Class::Number));
    }

    #[test]
    fn spans_are_in_order_do_not_overlap_and_sit_on_character_boundaries() {
        let samples: [(Lang, &str); 6] = [
            (Lang::Markdown, "---\ntitle: \"ü\"\n---\n# é\n- **ñ** `ç` [ü](ö)\n```html\n<p class=\"ä\">ß</p>\n```\n"),
            (Lang::Html, "<p class=\"é\">ü &amp; ö <!-- ñ --></p>"),
            (Lang::Xml, "<?xml?><svg a=\"ü\"><b/></svg>"),
            (Lang::Json, "{\"é\": [1, \"ü\", null]}"),
            (Lang::Toml, "# é\n[ü]\nk = \"ñ\" # ö\n"),
            (Lang::Mermaid, "flowchart LR\n  A[é] --> B[ü]\n  %% ñ\n"),
        ];
        for (lang, text) in samples {
            let spans = spans(lang, text);
            assert!(!spans.is_empty(), "{lang:?} found nothing in {text:?}");
            let mut last = 0;
            for s in &spans {
                assert!(s.range.start >= last, "{lang:?}: overlap at {:?} after {last}", s.range);
                assert!(s.range.start < s.range.end && s.range.end <= text.len(), "{lang:?}: {:?}", s.range);
                assert!(text.is_char_boundary(s.range.start) && text.is_char_boundary(s.range.end), "{lang:?}: {:?}", s.range);
                last = s.range.end;
            }
        }
    }

    #[test]
    fn odd_text_never_panics() {
        let odd = [
            "",
            "\n",
            "\u{0}",
            "\u{202e}x",
            "\"",
            "'",
            "[",
            "[[",
            "{",
            "#",
            "###### ",
            "> > > ",
            "1.",
            "- ",
            "|",
            "||",
            "`",
            "``",
            "$",
            "$$",
            "*",
            "**",
            "_",
            "~~",
            "<",
            "</",
            "&;",
            "%%",
            "-->",
            "-",
            "=",
            "...",
        ];
        for lang in [Lang::Markdown, Lang::Html, Lang::Xml, Lang::Json, Lang::Toml, Lang::Yaml, Lang::Mermaid, Lang::Plain] {
            for text in odd {
                let _ = spans(lang, text);
                let long = text.repeat(500);
                let _ = spans(lang, &long);
            }
        }
    }

    #[test]
    fn a_file_too_big_is_left_plain() {
        let big = "a".repeat(MAX_COLOURED + 1);
        assert!(spans(Lang::Json, &format!("{{\"k\": \"{big}\"}}")).is_empty());
    }
}

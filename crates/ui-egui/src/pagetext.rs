//! Formatted editing: the page area shows the text the way it reads, not as code.
//!
//! The editor is one text box, and this module gives egui the layout for it: headings in
//! heading type, bold and links in their own colour, quotes and markers dimmed, code and
//! equations in their own style. The markers themselves stay in the text (they are what the
//! file holds) but they are dimmed, so the eye follows the words, not the syntax.
//!
//! This is the step before a full editor that hides the markers altogether; it keeps every
//! caret, selection and undo behaviour of a plain text box, so nothing is lost on the way.

use crate::theme::Tokens;
use egui::FontId;
use egui::text::{LayoutJob, TextFormat};
use notecraft_model::PageFormat;

/// One piece of the page text and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    /// Ordinary words.
    Body,
    /// A heading, 1 to 3 (deeper headings use 3).
    Heading(u8),
    Bold,
    Italic,
    /// Inline code, a fenced block, or anything inside an HTML tag.
    Code,
    /// A list bullet, a heading's hashes, a quote mark: part of the syntax.
    Marker,
    Quote,
    Link,
    Math,
}

impl Style {
    fn format(self, base: f32, t: &Tokens) -> TextFormat {
        let body = FontId::proportional(base);
        match self {
            Style::Body => TextFormat { font_id: body, color: t.text, ..Default::default() },
            Style::Heading(level) => {
                let size = match level {
                    1 => base * 1.75,
                    2 => base * 1.4,
                    _ => base * 1.18,
                };
                TextFormat { font_id: FontId::proportional(size), color: t.strong, ..Default::default() }
            }
            Style::Bold => TextFormat { font_id: body, color: t.strong, ..Default::default() },
            Style::Italic => TextFormat { font_id: body, color: t.text, italics: true, ..Default::default() },
            Style::Code => TextFormat { font_id: FontId::monospace(base * 0.92), color: t.text, background: t.code_bg, ..Default::default() },
            Style::Marker => TextFormat { font_id: body, color: t.muted, ..Default::default() },
            Style::Quote => TextFormat { font_id: body, color: t.muted, italics: true, ..Default::default() },
            Style::Link => TextFormat { font_id: body, color: t.link, underline: egui::Stroke::new(1.0, t.link), ..Default::default() },
            Style::Math => TextFormat { font_id: FontId::proportional(base), color: t.accent, italics: true, ..Default::default() },
        }
    }
}

/// Splits `text` into styled pieces. Pure, so it is tested without a window.
pub fn pieces(format: PageFormat, text: &str) -> Vec<(std::ops::Range<usize>, Style)> {
    let mut out: Vec<(std::ops::Range<usize>, Style)> = Vec::new();
    match format {
        PageFormat::Cmgfm => markdown_pieces(text, &mut out),
        PageFormat::Html => html_pieces(text, &mut out),
    }
    merge(out, text.len())
}

/// Fills the gaps with body text and drops empty pieces, so the result covers the text once.
fn merge(mut pieces: Vec<(std::ops::Range<usize>, Style)>, len: usize) -> Vec<(std::ops::Range<usize>, Style)> {
    pieces.retain(|(r, _)| r.start < r.end && r.end <= len);
    pieces.sort_by_key(|(r, _)| r.start);
    let mut out = Vec::with_capacity(pieces.len() * 2 + 1);
    let mut at = 0usize;
    for (range, style) in pieces {
        if range.start < at {
            continue; // overlapping: the first one wins
        }
        if range.start > at {
            out.push((at..range.start, Style::Body));
        }
        at = range.end;
        out.push((range, style));
    }
    if at < len {
        out.push((at..len, Style::Body));
    }
    out
}

fn markdown_pieces(text: &str, out: &mut Vec<(std::ops::Range<usize>, Style)>) {
    let mut fenced = false;
    for (start, line) in line_spans(text) {
        let end = start + line.len();
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        // A tag marker at the start of a line is syntax, so it is dimmed like the rest.
        if let (Some(tag), _) = notecraft_model::Tag::split(trimmed) {
            let marker = tag.marker();
            let len = marker.len().min(trimmed.len());
            out.push((start + indent..start + indent + len, Style::Marker));
            inline(text, start + indent + len, end, out);
            continue;
        }
        if trimmed.starts_with("```") {
            fenced = !fenced;
            out.push((start..end, Style::Code));
            continue;
        }
        if fenced {
            out.push((start..end, Style::Code));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            out.push((start + indent..start + indent + 1, Style::Marker));
            out.push((start + indent + 1..start + indent + 1 + rest.len(), Style::Quote));
            continue;
        }
        let hashes = trimmed.bytes().take_while(|b| *b == b'#').count();
        if hashes > 0 && hashes <= 6 && trimmed.as_bytes().get(hashes) == Some(&b' ') {
            out.push((start + indent..start + indent + hashes + 1, Style::Marker));
            let level = hashes.min(3) as u8;
            out.push((start + indent + hashes + 1..end, Style::Heading(level)));
            continue;
        }
        // A list marker, a numbered marker, or a task box.
        if let Some(marker) = list_marker(trimmed) {
            out.push((start + indent..start + indent + marker, Style::Marker));
            inline(text, start + indent + marker, end, out);
            continue;
        }
        inline(text, start, end, out);
    }
}

/// The length of a list marker at the start of `s`, including the space and any task box.
fn list_marker(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut n = match bytes.first() {
        Some(b'-' | b'*' | b'+') if bytes.get(1) == Some(&b' ') => 2,
        _ => {
            let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
            if digits > 0 && matches!(bytes.get(digits), Some(b'.' | b')')) && bytes.get(digits + 1) == Some(&b' ') { digits + 2 } else { 0 }
        }
    };
    if n == 0 {
        return None;
    }
    let rest = s.get(n..).unwrap_or("");
    for box_text in ["[ ] ", "[x] ", "[X] "] {
        if rest.starts_with(box_text) {
            n += box_text.len();
            break;
        }
    }
    Some(n)
}

/// Inline styles inside one line: code, bold, italic, links and maths.
fn inline(text: &str, from: usize, to: usize, out: &mut Vec<(std::ops::Range<usize>, Style)>) {
    let line = text.get(from..to).unwrap_or("");
    let bytes = line.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let rest = line.get(i..).unwrap_or("");
        let tag_end = rest.find('>').map(|n| n + 1);
        if rest.starts_with("<span ")
            && tag_end.is_some_and(|end| {
                let tag = rest.get(..end).unwrap_or("");
                tag_font_size(tag).is_some() || tag_font_family(tag).is_some()
            })
        {
            let end = tag_end.unwrap_or(0);
            out.push((from + i..from + i + end, Style::Marker));
            i += end;
            continue;
        }
        if rest.starts_with("</span>") {
            out.push((from + i..from + i + "</span>".len(), Style::Marker));
            i += "</span>".len();
            continue;
        }
        // The marks themselves are dimmed and the words inside them carry the style, so the
        // text reads as formatted text with the marks out of the way.
        let emphasis = |mark: &str, style: Style, out: &mut Vec<(std::ops::Range<usize>, Style)>| -> Option<usize> {
            let len = wrapped(rest, mark, mark)?;
            let m = mark.len();
            out.push((from + i..from + i + m, Style::Marker));
            out.push((from + i + m..from + i + len - m, style));
            out.push((from + i + len - m..from + i + len, Style::Marker));
            Some(len)
        };
        if let Some(len) = emphasis("`", Style::Code, out) {
            i += len;
        } else if let Some(len) = emphasis("$", Style::Math, out) {
            i += len;
        } else if let Some(len) = emphasis("**", Style::Bold, out) {
            i += len;
        } else if let Some(len) = emphasis("*", Style::Italic, out) {
            i += len;
        } else if rest.starts_with('[')
            && let Some(len) = link_len(rest)
        {
            out.push((from + i..from + i + len, Style::Link));
            i += len;
        } else {
            i += rest.chars().next().map(char::len_utf8).unwrap_or(1);
        }
    }
}

/// The length of `open ... close` at the start of `s`, if it closes on the same line.
fn wrapped(s: &str, open: &str, close: &str) -> Option<usize> {
    let rest = s.strip_prefix(open)?;
    if rest.starts_with(' ') || rest.is_empty() {
        return None;
    }
    let end = rest.find(close)?;
    if rest.get(..end)?.contains('\n') { None } else { Some(open.len() + end + close.len()) }
}

/// The target of the link the character `at` (a character index, as a caret counts) sits in,
/// if it sits in one. `[words](target "title")` gives `target`; angle brackets around it are
/// dropped. Pure, so a click can be tested without a window.
pub fn link_at(text: &str, at: usize) -> Option<String> {
    let byte = text.char_indices().nth(at).map(|(b, _)| b)?;
    let (range, _) = inline_pieces(text).into_iter().find(|(r, s)| *s == Style::Link && r.contains(&byte))?;
    href_of(text.get(range)?)
}

/// The target written inside a `[words](target)` link.
fn href_of(link: &str) -> Option<String> {
    let open = link.find("](")? + 2;
    let inside = link.get(open..link.len().checked_sub(1)?)?.trim();
    // `<...>` may hold spaces, as CommonMark allows; a bare target ends at the first one.
    let target = match inside.strip_prefix('<') {
        Some(rest) => rest.split('>').next().unwrap_or(""),
        None => inside.split_whitespace().next().unwrap_or(""),
    };
    if target.is_empty() { None } else { Some(target.to_string()) }
}

/// The length of a `[text](target)` link at the start of `s`.
fn link_len(s: &str) -> Option<usize> {
    let close = s.find("](")?;
    let end = s.get(close..)?.find(')')? + close + 1;
    if s.get(..end)?.contains('\n') { None } else { Some(end) }
}

fn tag_font_size(tag: &str) -> Option<u8> {
    let lower = tag.to_ascii_lowercase();
    if !lower.starts_with("<span ") {
        return None;
    }
    let style_start = lower.find("style=\"")? + "style=\"".len();
    let style_end = lower.get(style_start..)?.find('"')? + style_start;
    lower.get(style_start..style_end)?.split(';').find_map(|declaration| {
        let (property, value) = declaration.split_once(':')?;
        if !property.trim().eq("font-size") {
            return None;
        }
        let points = value.trim().strip_suffix("pt")?.parse::<u8>().ok()?;
        (8..=72).contains(&points).then_some(points)
    })
}

fn tag_font_family(tag: &str) -> Option<String> {
    let source = format!("{tag}x</span>");
    let spans = notecraft_formats::markdown::parse_inline(&source);
    let span = spans.first()?;
    (spans.len() == 1 && span.text == "x").then(|| span.font_family.clone()).flatten()
}

fn font_family_ranges(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    let mut out = Vec::new();
    let mut spans: Vec<Option<(String, usize)>> = Vec::new();
    let mut at = 0;
    while let Some(start) = text.get(at..).and_then(|rest| rest.find('<')).map(|n| at + n) {
        let Some(end) = text.get(start..).and_then(|rest| rest.find('>')).map(|n| start + n + 1) else { break };
        let tag = text.get(start..end).unwrap_or("");
        if tag.eq_ignore_ascii_case("</span>") {
            if let Some((family, content_start)) = spans.pop().flatten() {
                out.push((content_start..start, family));
            }
        } else if tag.to_ascii_lowercase().starts_with("<span ") {
            spans.push(tag_font_family(tag).map(|family| (family, end)));
        }
        at = end;
    }
    out
}

fn font_size_ranges(text: &str) -> Vec<(std::ops::Range<usize>, u8)> {
    let mut out = Vec::new();
    let mut spans: Vec<Option<(u8, usize)>> = Vec::new();
    let mut at = 0usize;
    while let Some(start) = text.get(at..).and_then(|rest| rest.find('<')).map(|n| at + n) {
        let Some(end) = text.get(start..).and_then(|rest| rest.find('>')).map(|n| start + n + 1) else {
            break;
        };
        let tag = text.get(start..end).unwrap_or("");
        if tag.eq_ignore_ascii_case("</span>") {
            if let Some((size, content_start)) = spans.pop().flatten() {
                out.push((content_start..start, size));
            }
        } else if tag.to_ascii_lowercase().starts_with("<span ") {
            spans.push(tag_font_size(tag).map(|size| (size, end)));
        }
        at = end;
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InlineTag {
    Underline,
    Subscript,
    Superscript,
}

fn inline_tag_ranges(
    text: &str,
    pieces: &[(std::ops::Range<usize>, Style)],
) -> (Vec<std::ops::Range<usize>>, Vec<(std::ops::Range<usize>, InlineTag)>) {
    let mut markers = Vec::new();
    let mut ranges = Vec::new();
    let mut open: Vec<(InlineTag, usize, usize)> = Vec::new();
    let mut at = 0;
    while let Some(start) = text.get(at..).and_then(|rest| rest.find('<')).map(|n| at + n) {
        let Some(end) = text.get(start..).and_then(|rest| rest.find('>')).map(|n| start + n + 1) else { break };
        let escaped = text.as_bytes().get(..start).unwrap_or_default().iter().rev().take_while(|&&byte| byte == b'\\').count() % 2 == 1;
        if escaped || pieces.iter().any(|(range, style)| matches!(style, Style::Code | Style::Math) && range.start <= start && end <= range.end) {
            at = end;
            continue;
        }
        let tag = text.get(start..end).unwrap_or("");
        let (kind, closing) = match tag {
            "<u>" => (InlineTag::Underline, false),
            "</u>" => (InlineTag::Underline, true),
            "<sub>" => (InlineTag::Subscript, false),
            "</sub>" => (InlineTag::Subscript, true),
            "<sup>" => (InlineTag::Superscript, false),
            "</sup>" => (InlineTag::Superscript, true),
            _ => {
                at = end;
                continue;
            }
        };
        if closing {
            if open.last().is_some_and(|(active, _, _)| *active == kind)
                && let Some((_, opening, content)) = open.pop()
            {
                markers.push(opening..content);
                markers.push(start..end);
                ranges.push((content..start, kind));
            }
        } else {
            open.push((kind, start, end));
        }
        at = end;
    }
    (markers, ranges)
}

/// HTML: the tags are the syntax, so they are dimmed and the text between them reads normally.
fn html_pieces(text: &str, out: &mut Vec<(std::ops::Range<usize>, Style)>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut heading: Option<(u8, usize)> = None;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            let end = text.get(i..).and_then(|s| s.find('>')).map(|n| i + n + 1).unwrap_or(bytes.len());
            let tag = text.get(i..end).unwrap_or("").to_ascii_lowercase();
            out.push((i..end, Style::Marker));
            // Text inside a heading tag is drawn as a heading until it closes.
            if let Some(level) = tag.strip_prefix("<h").and_then(|t| t.as_bytes().first().copied()).and_then(|b| (b as char).to_digit(10)) {
                heading = Some((level.min(3) as u8, end));
            } else if tag.starts_with("</h")
                && let Some((level, from)) = heading.take()
            {
                out.push((from..i, Style::Heading(level)));
            }
            i = end;
        } else {
            i += text.get(i..).and_then(|s| s.chars().next()).map(char::len_utf8).unwrap_or(1);
        }
    }
}

fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for line in text.split_inclusive('\n') {
        let without = line.strip_suffix('\n').unwrap_or(line);
        let without = without.strip_suffix('\r').unwrap_or(without);
        out.push((at, without));
        at += line.len();
    }
    out
}

/// The styled pieces of one line of inline text: no headings, no list markers, just the
/// marks inside the words. Used by the block editor, where the block already knows what it is.
pub fn inline_pieces(text: &str) -> Vec<(std::ops::Range<usize>, Style)> {
    let mut out = Vec::new();
    inline(text, 0, text.len(), &mut out);
    merge(out, text.len())
}

/// Which edge a line hangs from: the right one when it reads right to left. A line with
/// no letters of its own follows `page`, the direction the whole page was given.
pub fn halign(text: &str, page: Option<notecraft_model::Dir>) -> egui::Align {
    if notecraft_model::direction_in(text, page).is_rtl() { egui::Align::RIGHT } else { egui::Align::LEFT }
}

/// The layout for one block being written in: inline marks styled, in the block's own size.
pub fn inline_job(text: &str, base: f32, strong: bool, t: &Tokens, wrap_width: f32, page: Option<notecraft_model::Dir>) -> LayoutJob {
    inline_job_inner(text, base, strong, t, wrap_width, page, None)
}

pub fn inline_job_with_fonts(
    ctx: &egui::Context,
    text: &str,
    base: f32,
    strong: bool,
    t: &Tokens,
    wrap_width: f32,
    page: Option<notecraft_model::Dir>,
) -> LayoutJob {
    let families = ctx.fonts(|fonts| fonts.definitions().families.keys().cloned().collect::<std::collections::BTreeSet<_>>());
    inline_job_inner(text, base, strong, t, wrap_width, page, Some(&families))
}

fn inline_job_inner(
    text: &str,
    base: f32,
    strong: bool,
    t: &Tokens,
    wrap_width: f32,
    page: Option<notecraft_model::Dir>,
    available: Option<&std::collections::BTreeSet<egui::FontFamily>>,
) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: egui::text::TextWrapping { max_width: wrap_width, ..Default::default() },
        halign: halign(text, page),
        ..Default::default()
    };
    let font_sizes = font_size_ranges(text);
    let font_families = font_family_ranges(text);
    let pieces = inline_pieces(text);
    let (tag_markers, tag_ranges) = inline_tag_ranges(text, &pieces);
    for (range, style) in pieces {
        let mut boundaries = vec![range.start, range.end];
        for extra in font_sizes
            .iter()
            .map(|(r, _)| r)
            .chain(font_families.iter().map(|(r, _)| r))
            .chain(tag_ranges.iter().map(|(r, _)| r))
            .chain(tag_markers.iter())
        {
            if extra.start > range.start && extra.start < range.end {
                boundaries.push(extra.start);
            }
            if extra.end > range.start && extra.end < range.end {
                boundaries.push(extra.end);
            }
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for pair in boundaries.windows(2) {
            let (Some(start), Some(end)) = (pair.first(), pair.get(1)) else { continue };
            let Some(part) = text.get(*start..*end) else { continue };
            let font_size =
                font_sizes.iter().filter(|(r, _)| r.start <= *start && r.end >= *end).min_by_key(|(r, _)| r.end - r.start).map(|(_, size)| *size);
            let marker = style != Style::Code && style != Style::Math && tag_markers.iter().any(|r| r.start <= *start && r.end >= *end);
            let mut format = if marker { Style::Marker } else { style }.format(base, t);
            if !marker
                && !matches!(style, Style::Marker | Style::Code | Style::Math)
                && let Some((_, name)) = font_families.iter().filter(|(r, _)| r.start <= *start && r.end >= *end).min_by_key(|(r, _)| r.len())
                && let Some(available) = available
            {
                let family = crate::fonts::named_family(name);
                if available.contains(&family) {
                    format.font_id.family = family;
                }
            }
            if let Some(points) = font_size {
                format.font_id.size = base * f32::from(points) / 11.0;
            }
            if !marker && style != Style::Code && style != Style::Math {
                let active = tag_ranges.iter().filter(|(r, _)| r.start <= *start && r.end >= *end);
                let mut script = None;
                for (r, tag) in active {
                    if *tag == InlineTag::Underline {
                        format.underline = egui::Stroke::new(1.0, format.color);
                    } else if script.as_ref().is_none_or(|(size, _)| r.len() < *size) {
                        script = Some((r.len(), *tag));
                    }
                }
                if let Some((_, tag)) = script {
                    format.font_id.size *= 0.7;
                    format.valign = if tag == InlineTag::Superscript { egui::Align::TOP } else { egui::Align::BOTTOM };
                }
            }
            if strong && style == Style::Body {
                format.color = t.strong;
            }
            job.append(part, 0.0, format);
        }
    }
    if job.sections.is_empty() {
        let mut format = Style::Body.format(base, t);
        if strong {
            format.color = t.strong;
        }
        job.append("", 0.0, format);
    }
    job
}

/// The layout egui draws: the pieces above, turned into one job.
pub fn job(format: PageFormat, text: &str, base: f32, t: &Tokens, wrap_width: f32, page: Option<notecraft_model::Dir>) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: egui::text::TextWrapping { max_width: wrap_width, ..Default::default() },
        halign: halign(text, page),
        ..Default::default()
    };
    for (range, style) in pieces(format, text) {
        if let Some(part) = text.get(range) {
            job.append(part, 0.0, style.format(base, t));
        }
    }
    if job.sections.is_empty() {
        job.append("", 0.0, Style::Body.format(base, t));
    }
    job
}

/// A colour that is never the page's own text colour, for the test that checks the styles
/// actually differ.
pub fn is_plain(style: Style) -> bool {
    style == Style::Body
}

#[cfg(test)]
mod tests {
    use super::*;

    fn styled(format: PageFormat, text: &str) -> Vec<(String, Style)> {
        pieces(format, text).into_iter().map(|(r, s)| (text.get(r).unwrap_or("").to_string(), s)).collect()
    }

    fn covers(format: PageFormat, text: &str) {
        let joined: String = pieces(format, text).into_iter().filter_map(|(r, _)| text.get(r)).collect();
        assert_eq!(joined, text, "the pieces must cover the text exactly once");
    }

    #[test]
    fn headings_are_headings_and_the_hashes_are_dimmed() {
        let text = "# Title\n\nSome words\n";
        assert_eq!(
            styled(PageFormat::Cmgfm, text),
            vec![("# ".to_string(), Style::Marker), ("Title".to_string(), Style::Heading(1)), ("\n\nSome words\n".to_string(), Style::Body),]
        );
        covers(PageFormat::Cmgfm, text);
        assert!(matches!(styled(PageFormat::Cmgfm, "## Two").get(1), Some((_, Style::Heading(2)))));
        assert!(matches!(styled(PageFormat::Cmgfm, "##### Five").get(1), Some((_, Style::Heading(3)))));
        // A hash without a space is not a heading.
        assert_eq!(styled(PageFormat::Cmgfm, "#tag"), vec![("#tag".to_string(), Style::Body)]);
    }

    /// The words carry the style and the marks around them are dimmed, so the page reads as
    /// formatted text rather than as source.
    #[test]
    fn a_click_in_a_link_gives_its_target_and_one_beside_it_gives_none() {
        let text = "Go to [the page](../page-b/ \"Page B\") now, or [the web](<https://x.test/a b>).";
        let at = |needle: &str| text.find(needle).map(|b| text[..b].chars().count()).unwrap_or(usize::MAX);
        assert_eq!(link_at(text, at("the page")), Some("../page-b/".to_string()));
        assert_eq!(link_at(text, at("page-b")), Some("../page-b/".to_string()), "the target itself is part of the link");
        assert_eq!(link_at(text, at("[the web")), Some("https://x.test/a b".to_string()), "angle brackets are dropped and may hold a space");
        assert_eq!(link_at(text, at("Go")), None);
        assert_eq!(link_at(text, at("now")), None);
        assert_eq!(link_at(text, text.chars().count()), None, "past the end is nowhere");
        assert_eq!(link_at("مرحبا [هنا](../هنا/) و", 7), Some("../هنا/".to_string()), "characters, not bytes");
        assert_eq!(link_at("[empty]()", 1), None);
        assert_eq!(link_at("[unclosed](x", 1), None);
    }

    #[test]
    fn bold_italic_code_links_and_maths() {
        let text = "a **bold** and *slanted* and `code` and [link](x.md) and $x^2$ end";
        let got = styled(PageFormat::Cmgfm, text);
        let find = |s: &str| got.iter().find(|(t, _)| t == s).map(|(_, st)| *st);
        assert_eq!(find("bold"), Some(Style::Bold));
        assert_eq!(find("slanted"), Some(Style::Italic));
        assert_eq!(find("code"), Some(Style::Code));
        assert_eq!(find("[link](x.md)"), Some(Style::Link));
        assert_eq!(find("x^2"), Some(Style::Math));
        assert_eq!(got.iter().filter(|(t, s)| t == "**" && *s == Style::Marker).count(), 2, "both bold marks are dimmed");
        assert_eq!(got.iter().filter(|(t, s)| t == "*" && *s == Style::Marker).count(), 2);
        assert_eq!(got.iter().filter(|(t, s)| t == "`" && *s == Style::Marker).count(), 2);
        covers(PageFormat::Cmgfm, text);
    }

    #[test]
    fn a_font_size_span_changes_the_editor_text_size() {
        let text = "<span style=\"font-size: 18pt\">large</span>";
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let job = inline_job(text, 11.0, false, &tokens, 1000.0, Some(notecraft_model::Dir::Ltr));
        let section = job.sections.iter().find(|section| section.format.font_id.size > 11.0).expect("sized text section");
        assert_eq!(section.format.font_id.size, 18.0);
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn editor_uses_installed_family_and_keeps_markers_dimmed() {
        let mut database = fontdb::Database::new();
        database.load_font_data(crate::fonts::BUNDLED[0].1.to_vec());
        let catalog = crate::fonts::system::Catalog::from_database(database);
        let family = catalog.families().first().expect("fixture family");
        let mut definitions = crate::fonts::definitions();
        let key = catalog.install(family, &mut definitions).expect("install fixture");
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions);
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let text = format!("<span style=\"font-family: {family}; font-size: 18pt\">Hello</span>");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let job = inline_job_with_fonts(ui.ctx(), &text, 11.0, false, &tokens, 1000.0, None);
            let word = job.sections.iter().find(|s| text.get(s.byte_range.start.0..s.byte_range.end.0) == Some("Hello")).expect("word");
            assert_eq!(word.format.font_id.family, key);
            assert_eq!(word.format.font_id.size, 18.0);
            assert_eq!(job.sections[0].format.color, tokens.muted);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn editor_draws_nested_underline_and_scripts_without_styling_their_tags() {
        let text = "<u>under <sup>مرحبا</sup> end</u> plain <sub>2</sub>";
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let job = inline_job(text, 20.0, false, &tokens, 1000.0, Some(notecraft_model::Dir::Ltr));
        let format = |part: &str| {
            job.sections
                .iter()
                .find(|section| text.get(section.byte_range.start.0..section.byte_range.end.0) == Some(part))
                .map(|section| &section.format)
                .expect("section")
        };
        assert_ne!(format("under ").underline, egui::Stroke::NONE);
        assert_ne!(format("مرحبا").underline, egui::Stroke::NONE);
        assert_eq!(format("مرحبا").font_id.size, 14.0);
        assert_eq!(format("مرحبا").valign, egui::Align::TOP);
        assert_eq!(format("2").font_id.size, 14.0);
        assert_eq!(format("2").valign, egui::Align::BOTTOM);
        assert_eq!(format(" plain ").underline, egui::Stroke::NONE);
        for tag in ["<u>", "<sup>", "</sup>", "</u>", "<sub>", "</sub>"] {
            assert_eq!(format(tag).color, tokens.muted);
            assert_eq!(format(tag).underline, egui::Stroke::NONE);
        }
    }

    #[test]
    fn editor_keeps_combined_marks_and_does_not_format_literal_code_tags() {
        let text = "**<u>bold</u>** `<u>`plain`</u>` <span style=\"font-size: 20pt\"><sub>2</sub></span>";
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let job = inline_job(text, 11.0, false, &tokens, 1000.0, Some(notecraft_model::Dir::Ltr));
        let format = |part: &str| {
            job.sections
                .iter()
                .find(|section| text.get(section.byte_range.start.0..section.byte_range.end.0) == Some(part))
                .map(|section| &section.format)
                .expect("section")
        };
        assert_eq!(format("bold").color, tokens.strong);
        assert_ne!(format("bold").underline, egui::Stroke::NONE);
        assert_eq!(format("plain").underline, egui::Stroke::NONE);
        assert_eq!(format("2").font_id.size, 14.0);
    }

    #[test]
    fn editor_does_not_format_escaped_tags() {
        let text = "\\<u>plain\\</u>";
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let job = inline_job(text, 11.0, false, &tokens, 1000.0, Some(notecraft_model::Dir::Ltr));
        assert!(job.sections.iter().all(|section| section.format.underline == egui::Stroke::NONE));
    }

    /// A tag marker is syntax: dimmed, with the words after it reading normally.
    #[test]
    fn a_tag_marker_is_dimmed() {
        let got = styled(PageFormat::Cmgfm, "#idea Good idea\n");
        assert_eq!(got.first().map(|(t, s)| (t.as_str(), *s)), Some(("#idea ", Style::Marker)));
        assert!(got.iter().any(|(t, s)| t.contains("Good idea") && *s == Style::Body));
        covers(PageFormat::Cmgfm, "#idea Good idea\n");
        // A word that only looks like a tag is ordinary text.
        let plain = styled(PageFormat::Cmgfm, "#ideas later\n");
        assert_eq!(plain.first().map(|(_, s)| *s), Some(Style::Body));
    }

    #[test]
    fn lists_quotes_and_fences() {
        let list = styled(PageFormat::Cmgfm, "- one\n1. two\n- [ ] task\n");
        assert_eq!(list.first().map(|(t, s)| (t.as_str(), *s)), Some(("- ", Style::Marker)));
        assert!(list.iter().any(|(t, s)| t == "1. " && *s == Style::Marker));
        assert!(list.iter().any(|(t, s)| t == "- [ ] " && *s == Style::Marker));
        let quote = styled(PageFormat::Cmgfm, "> quoted\n");
        assert_eq!(quote.first().map(|(_, s)| *s), Some(Style::Marker));
        assert_eq!(quote.get(1).map(|(_, s)| *s), Some(Style::Quote));
        let fence = styled(PageFormat::Cmgfm, "```rust\nlet x = 1;\n```\nafter\n");
        assert_eq!(fence.iter().filter(|(_, s)| *s == Style::Code).count(), 3, "the fence and the code inside it");
        assert!(fence.iter().any(|(t, s)| t.contains("after") && *s == Style::Body));
        covers(PageFormat::Cmgfm, "```rust\nlet x = 1;\n```\nafter\n");
    }

    #[test]
    fn html_tags_are_dimmed_and_headings_stay_headings() {
        let text = "<h1>Title</h1><p>Words</p>";
        let got = styled(PageFormat::Html, text);
        assert_eq!(got.first().map(|(t, s)| (t.as_str(), *s)), Some(("<h1>", Style::Marker)));
        assert!(got.iter().any(|(t, s)| t == "Title" && *s == Style::Heading(1)));
        assert!(got.iter().any(|(t, s)| t == "Words" && *s == Style::Body));
        covers(PageFormat::Html, text);
        // An unclosed tag does not run away.
        covers(PageFormat::Html, "<p>text");
        covers(PageFormat::Html, "<<<");
    }

    #[test]
    fn hostile_text_is_covered_exactly_once() {
        for text in [
            "",
            "\n\n\n",
            "**unclosed",
            "`",
            "$",
            "[half](",
            "- ",
            "#",
            "######## too deep",
            "> ",
            "```",
            "a\r\nb\r\n",
            "\u{1F600} emoji **bold**",
            "mixed \u{4e2d}\u{6587} *text*",
        ] {
            covers(PageFormat::Cmgfm, text);
            covers(PageFormat::Html, text);
        }
        let long = "word ".repeat(5000);
        covers(PageFormat::Cmgfm, &long);
    }

    #[test]
    fn the_styles_really_differ() {
        let t = crate::theme::Theme::DEFAULT.tokens();
        let body = Style::Body.format(15.0, &t);
        for style in [Style::Heading(1), Style::Bold, Style::Italic, Style::Code, Style::Marker, Style::Quote, Style::Link, Style::Math] {
            let f = style.format(15.0, &t);
            assert!(
                f.color != body.color || f.font_id != body.font_id || f.italics != body.italics || f.background != body.background,
                "{style:?} looks like body text"
            );
            assert!(!is_plain(style));
        }
        assert!(is_plain(Style::Body));
    }

    #[test]
    fn the_job_holds_every_character() {
        let t = crate::theme::Theme::DEFAULT.tokens();
        let text = "# Title\n\n- one **two**\n";
        let laid = job(PageFormat::Cmgfm, text, 15.0, &t, 400.0, Some(notecraft_model::Dir::Ltr));
        assert_eq!(laid.text, text);
        assert!(laid.sections.len() > 3);
        let empty = job(PageFormat::Cmgfm, "", 15.0, &t, 400.0, Some(notecraft_model::Dir::Ltr));
        assert_eq!(empty.sections.len(), 1, "an empty page still has one section to type into");
    }
}

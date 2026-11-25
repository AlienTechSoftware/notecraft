//! Source ranges of supported local links and media. Export can rewrite destinations
//! without serializing the surrounding page or changing its rich text and layout.

use notecraft_model::PageFormat;
use std::ops::Range;

pub struct Reference {
    pub range: Range<usize>,
    pub target: String,
    html: bool,
}

impl Reference {
    pub fn encode(&self, target: &str) -> String {
        if self.html { crate::html::escape(target).replace('\'', "&#39;") } else { target.to_string() }
    }
}

/// HTML href/src/poster/data attributes and CMGFM inline links/images, including raw HTML.
/// Code examples and comments do not become export dependencies.
pub fn in_text(format: PageFormat, text: &str) -> Vec<Reference> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = if format == PageFormat::Cmgfm {
        crate::frontmatter::split_front(text).0.len() + if text.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 }
    } else {
        0
    };
    let mut fence: Option<(u8, usize)> = None;
    // The end of the CMGFM block that holds `i` (cached until `i` reaches it), and the next
    // '>' at or after `i`. Both keep a scan that fails from being repeated over the whole rest
    // of the page from every backtick or `<letter` fragment.
    let mut block = 0..0;
    let mut next_gt = 0usize;
    while i < bytes.len() {
        if format == PageFormat::Cmgfm && (i == 0 || bytes[i - 1] == b'\n') {
            let end = text[i..].find('\n').map_or(bytes.len(), |n| i + n + 1);
            let trimmed = text[i..end].trim_start_matches(' ');
            let spaces = text[i..end].len() - trimmed.len();
            if spaces <= 3 && trimmed.as_bytes().first().is_some_and(|b| matches!(b, b'`' | b'~')) {
                let ch = trimmed.as_bytes()[0];
                let run = trimmed.bytes().take_while(|b| *b == ch).count();
                if run >= 3 {
                    match fence {
                        Some((was, count)) if was == ch && run >= count && trimmed[run..].trim().is_empty() => fence = None,
                        None => fence = Some((ch, run)),
                        _ => {}
                    }
                    i = end;
                    continue;
                }
            }
            // Native CMGFM writes two spaces per list level. Those lines are
            // list items, not code, even at a depth of two or more.
            if fence.is_some() || (spaces >= 4 && crate::markdown::list_marker(trimmed).is_none()) {
                i = end;
                continue;
            }
        }
        if format == PageFormat::Cmgfm && bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|b| **b == b'`').count();
            let marker = "`".repeat(run);
            if !block.contains(&i) {
                block = i..block_end(text, i);
            }
            // A code span ends in its own block. An unmatched run is literal backticks.
            i += run + text[i + run..block.end.max(i + run)].find(&marker).map_or(0, |end| end + run);
            continue;
        }
        if format == PageFormat::Cmgfm && bytes[i] == b'\\' {
            i = (i + 2).min(bytes.len());
            while i < bytes.len() && !text.is_char_boundary(i) {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'<' {
            if text[i..].starts_with("<!--") {
                i = text[i + 4..].find("-->").map_or(bytes.len(), |n| i + 4 + n + 3);
                continue;
            }
            if format == PageFormat::Cmgfm {
                // A comparison such as "budget < actual" is ordinary text.
                // Don't search across subsequent links for its closing angle.
                let name = i + 1 + usize::from(bytes.get(i + 1) == Some(&b'/'));
                if !bytes.get(name).is_some_and(u8::is_ascii_alphabetic) {
                    i += 1;
                    continue;
                }
            }
            let limit = if format == PageFormat::Cmgfm {
                if !block.contains(&i) {
                    block = i..block_end(text, i);
                }
                if next_gt <= i {
                    next_gt = text[i + 1..].find('>').map_or(bytes.len(), |n| i + 1 + n);
                }
                block.end
            } else {
                bytes.len()
            };
            // Raw HTML in CMGFM does not run past its block. With no '>' left in the block, this
            // `<` is text, so the next one is not searched for either.
            let found = if next_gt < limit || format == PageFormat::Html { tag_end(&bytes[..limit], i + 1) } else { None };
            let Some(end) = found else {
                if format == PageFormat::Html {
                    break;
                }
                i += 1;
                continue;
            };
            let tag = &text[i + 1..end];
            let name = tag.split_ascii_whitespace().next().unwrap_or("").trim_end_matches('/').to_ascii_lowercase();
            if matches!(name.as_str(), "pre" | "code" | "script") {
                let close = format!("</{name}>");
                i = text[end + 1..].to_ascii_lowercase().find(&close).map_or(bytes.len(), |n| end + 1 + n + close.len());
                continue;
            }
            attributes(text, i + 1, end, &mut out);
            i = end + 1;
            continue;
        }
        if format == PageFormat::Cmgfm && bytes[i..].starts_with(b"](") {
            let mut start = i + 2;
            while bytes.get(start).is_some_and(u8::is_ascii_whitespace) {
                start += 1;
            }
            let angled = bytes.get(start) == Some(&b'<');
            if angled {
                start += 1;
            }
            let mut end = start;
            let mut depth = 0usize;
            while end < bytes.len() {
                match bytes[end] {
                    b'\\' => {
                        end = (end + 2).min(bytes.len());
                        continue;
                    }
                    b'>' if angled => break,
                    b'(' if !angled => depth += 1,
                    b')' if !angled && depth == 0 => break,
                    b')' if !angled => depth -= 1,
                    b if !angled && b.is_ascii_whitespace() => break,
                    _ => {}
                }
                end += 1;
            }
            if end > start && end < bytes.len() {
                let raw = text.get(start..end).unwrap_or("");
                let mut target = String::new();
                let mut chars = raw.chars().peekable();
                while let Some(ch) = chars.next() {
                    if ch == '\\' && chars.peek().is_some_and(char::is_ascii_punctuation) {
                        if let Some(next) = chars.next() {
                            target.push(next);
                        }
                    } else {
                        target.push(ch);
                    }
                }
                out.push(Reference { range: start..end, target, html: false });
                i = end;
                continue;
            }
        }
        i += 1;
        // All slicing positions must be UTF-8 boundaries, including after escaped text.
        while i < bytes.len() && !text.is_char_boundary(i) {
            i += 1;
        }
    }
    out
}

/// Where the CMGFM block holding byte `i` ends: the start of the first later line that is blank
/// or starts a new block (a list item, heading, quote or fence), or the end of the text.
fn block_end(text: &str, i: usize) -> usize {
    let mut at = text[i..].find('\n').map_or(text.len(), |n| i + n + 1);
    while at < text.len() {
        let line = &text[at..text[at..].find('\n').map_or(text.len(), |n| at + n)];
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || crate::markdown::list_marker(line).is_some()
            || trimmed.starts_with('#')
            || trimmed.starts_with('>')
            || trimmed.starts_with("```")
            || trimmed.starts_with("~~~")
        {
            return at;
        }
        at += line.len() + 1;
    }
    text.len()
}

fn tag_end(bytes: &[u8], mut i: usize) -> Option<usize> {
    let mut quote = None;
    while let Some(&ch) = bytes.get(i) {
        match (quote, ch) {
            (Some(q), c) if q == c => quote = None,
            (None, b'\'' | b'"') => quote = Some(ch),
            (None, b'>') => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

fn attributes(text: &str, mut i: usize, end: usize, out: &mut Vec<Reference>) {
    let bytes = text.as_bytes();
    while i < end && !bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    while i < end {
        while i < end && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        let name_start = i;
        while i < end && !bytes[i].is_ascii_whitespace() && !matches!(bytes[i], b'=' | b'/') {
            i += 1;
        }
        let name = text.get(name_start..i).unwrap_or("").to_ascii_lowercase();
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if bytes.get(i) != Some(&b'=') {
            if i == name_start {
                i += 1;
            }
            continue;
        }
        i += 1;
        while i < end && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        let quote = bytes.get(i).copied().filter(|ch| matches!(ch, b'\'' | b'"'));
        if quote.is_some() {
            i += 1;
        }
        let start = i;
        while i < end
            && match quote {
                Some(q) => bytes[i] != q,
                None => !bytes[i].is_ascii_whitespace(),
            }
        {
            i += 1;
        }
        let stop = i;
        if quote.is_some() {
            i += 1;
        }
        if matches!(name.as_str(), "href" | "src" | "poster" | "data") {
            out.push(Reference { range: start..stop, target: crate::html::unescape(text.get(start..stop).unwrap_or("")), html: true });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_not_page_content_and_unicode_escapes_do_not_split_utf8() {
        let text = "---\ntitle: '[caption](missing)'\n---\n\\日本語 [real](assets/pic.png)";
        let refs = in_text(PageFormat::Cmgfm, text);
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["assets/pic.png"]);
        assert_eq!(&text[refs[0].range.clone()], "assets/pic.png");
        let refs = in_text(PageFormat::Cmgfm, "[name](assets/\\日本語.png)");
        assert_eq!(refs[0].target, "assets/\\日本語.png", "only ASCII punctuation consumes a Markdown escape");
    }

    #[test]
    fn markdown_references_follow_literal_comparisons_and_nested_list_items() {
        let text = "Budget < actual\n\n[Appendix](../appendix/)\n- Goal\n  - Step\n    - [Details](../details/)\n    1. [Checklist](../checklist/)\n\n    [Indented example](missing)\n";
        let references = in_text(PageFormat::Cmgfm, text);
        assert_eq!(references.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["../appendix/", "../details/", "../checklist/"]);
    }

    #[test]
    fn destinations_keep_source_ranges_without_treating_code_as_content() {
        let text = "<!-- <img src='secret'> --><p><A HREF = 'notecraft:/book/page?id=42#anchor'>go</A><img src=assets/pic.png></p><pre><img src='example'></pre>";
        let refs = in_text(PageFormat::Html, text);
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["notecraft:/book/page?id=42#anchor", "assets/pic.png"]);
        assert_eq!(&text[refs[0].range.clone()], refs[0].target);
        let text = "`[example](missing)`\n```\n![code](secret)\n```\n![日本語](assets/a(b).png) [link](<../other page/>)\n";
        let refs = in_text(PageFormat::Cmgfm, text);
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["assets/a(b).png", "../other page/"]);
    }

    #[test]
    fn a_code_span_stays_in_its_block_so_later_links_are_kept() {
        // #494: the backtick in the first item used to pair with the one in the last line,
        // hiding both links between them.
        let text = "- Run `make\n  - [Setup](../setup/)\n\n[Guide](../guide/)\n\nUse `x` and a lone ` here.\n";
        let refs = in_text(PageFormat::Cmgfm, text);
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["../setup/", "../guide/"]);
        // A code span may still continue onto the next line of its paragraph.
        let refs = in_text(PageFormat::Cmgfm, "Type `a\n[b](hidden)` then [c](../c/)\n");
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["../c/"]);
    }

    #[test]
    fn unclosed_tag_fragments_do_not_rescan_the_page() {
        // #495: every `<letter` without a '>' used to scan to the end of the text.
        let fragments = "Vec<T and Option<U, ".repeat(20_000);
        let text = format!("{fragments}\n\n[Kept](../kept/) <img src='assets/a.png'>\n");
        let started = std::time::Instant::now();
        let refs = in_text(PageFormat::Cmgfm, &text);
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["../kept/", "assets/a.png"]);
        assert!(started.elapsed() < std::time::Duration::from_secs(2), "took {:?}", started.elapsed());
        // A raw tag can still span lines inside its paragraph, but not a blank line.
        let refs = in_text(PageFormat::Cmgfm, "<a\nhref='../one/'>one</a>\n\n<b\n\n<img src='../two.png'>\n");
        assert_eq!(refs.iter().map(|r| r.target.as_str()).collect::<Vec<_>>(), ["../one/", "../two.png"]);
    }
}

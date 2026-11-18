//! Retains formatting while the editor works on the words a reader sees. The spans are the
//! document; `text` is their visible projection (an inline image is one object character,
//! every other span is its own words). Edits arrive as visible text and are folded back into
//! the spans, so marks, links, fonts and literal punctuation survive without ever being
//! shown.
//!
//! `literal` marks which visible characters must stay words: everything that was already in
//! the block. Characters the user types arrive unmarked, so Markdown they type still parses
//! (`**bold**` becomes bold) while stored literal punctuation (`*literal*`) never does —
//! even when a formatting wrap is put around it. Storage quoting belongs to the formats
//! crate, never the widget.

use std::ops::Range;

use notecraft_formats::markdown::{Block, Span, parse_inline};
use notecraft_formats::render::editable_spans;
use notecraft_model::Tag;

use super::{FontSelection, INLINE_IMAGE_CHAR, InlineFormat, block_text, format_span_range, paint_span_range, spans_of, spans_to_editor};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineEdit {
    spans: Vec<Span>,
    text: String,
    literal: Vec<bool>,
    /// True while `spans`/`text` are the caret's view: the tag marker was taken off the
    /// front and goes back on when the block is written.
    caret: bool,
}

/// The words one span shows while editing: an image is one object character, everything
/// else is its own text (code ticks, link targets, maths dollars and style tags are markup,
/// not words).
fn visible_piece(span: &Span) -> String {
    if span.image.is_some() { INLINE_IMAGE_CHAR.to_string() } else { span.text.clone() }
}

fn visible_text(spans: &[Span]) -> String {
    spans.iter().map(visible_piece).collect()
}

/// How many visible characters a span covers.
fn visible_len(span: &Span) -> usize {
    if span.image.is_some() { 1 } else { span.text.chars().count() }
}

/// A span that carries no formatting of its own: its words could still be read as Markdown.
fn plain(span: &Span) -> bool {
    span.image.is_none()
        && !span.math
        && !span.code
        && !span.bold
        && !span.italic
        && !span.strike
        && !span.underline
        && span.script.is_none()
        && span.font_size.is_none()
        && span.font_family.is_none()
        && span.colour.is_none()
        && span.highlight.is_none()
        && span.link.is_none()
}

/// Characters that can open or close inline markup; a stored one of these at the edge of
/// an edit may be the other half of syntax the user just typed.
fn is_markup_char(c: char) -> bool {
    matches!(c, '*' | '_' | '~' | '$' | '[' | ']' | '<' | '>' | '`' | '!' | '\\')
}

/// Adjacent spans with the same formatting are one run to the reader and to the writer.
fn merge(spans: Vec<Span>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        match out.last_mut() {
            Some(last)
                if last.image.is_none()
                    && span.image.is_none()
                    && Span { text: String::new(), ..last.clone() } == Span { text: String::new(), ..span.clone() } =>
            {
                last.text.push_str(&span.text);
            }
            _ => out.push(span),
        }
    }
    out
}

/// The spans covering visible characters `range`, edge spans trimmed to the range. An image
/// is atomic: it is in whole or not at all.
fn take_spans(spans: &[Span], range: Range<usize>) -> Vec<Span> {
    let (start, end) = (range.start, range.end);
    if start >= end {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 0usize;
    for span in spans {
        let len = visible_len(span);
        let next = at + len;
        if next > start && at < end {
            if span.image.is_some() {
                out.push(span.clone());
            } else {
                let a = start.saturating_sub(at).min(span.text.chars().count());
                let b = end.saturating_sub(at).min(span.text.chars().count());
                if a < b {
                    let chars: Vec<char> = span.text.chars().collect();
                    out.push(Span { text: chars[a..b].iter().collect(), ..span.clone() });
                }
            }
        }
        at = next;
        if at >= end {
            break;
        }
    }
    out
}

/// Fill the fields a fragment does not set itself from the span the caret sits in, the way
/// typing inside a bold or sized run continues it. Explicit marks in the fragment win.
fn inherit(span: &mut Span, donor: Option<&Span>) {
    let Some(donor) = donor.filter(|d| d.image.is_none()) else { return };
    span.bold |= donor.bold;
    span.italic |= donor.italic;
    span.strike |= donor.strike;
    span.underline |= donor.underline;
    if span.script.is_none() {
        span.script = donor.script;
    }
    span.code |= donor.code;
    span.math |= donor.math;
    if span.font_size.is_none() {
        span.font_size = donor.font_size;
    }
    if span.font_family.is_none() {
        span.font_family = donor.font_family.clone();
    }
    if span.colour.is_none() {
        span.colour = donor.colour.clone();
    }
    if span.highlight.is_none() {
        span.highlight = donor.highlight.clone();
    }
    if span.link.is_none() {
        span.link = donor.link.clone();
    }
}

/// Which old character each new character continues, by longest common subsequence, so a
/// stored literal character copied into an edit (a wrap puts marks around it) keeps its
/// protection while the marks around it are free to parse.
fn lcs_align(old: &[char], new: &[char]) -> Vec<Option<usize>> {
    let (n, m) = (old.len(), new.len());
    let mut out = vec![None; m];
    if n == 0 || m == 0 || n.saturating_mul(m) > 1_000_000 {
        return out;
    }
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if old[i] == new[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] && dp[i][j] == dp[i + 1][j + 1] + 1 {
            out[j] = Some(i);
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// The link a visible character position sits in: where it starts and ends in visible
/// characters, its words and its address. For the Link dialog and Ctrl+click.
pub(super) fn link_span(spans: &[Span], at: usize) -> Option<(usize, usize, String, String)> {
    let mut pos = 0usize;
    let len = spans.iter().map(visible_len).sum::<usize>();
    let at = at.min(len.saturating_sub(1));
    for span in spans {
        let len = visible_len(span);
        let next = pos + len;
        if at < next {
            let href = span.link.clone().filter(|_| span.image.is_none())?;
            return Some((pos, next, span.text.clone(), href));
        }
        pos = next;
    }
    None
}

impl InlineEdit {
    pub(super) fn from_spans(spans: &[Span]) -> Self {
        let text = visible_text(spans);
        Self { spans: spans.to_vec(), literal: vec![true; text.chars().count()], text, caret: false }
    }

    pub(super) fn new(block: &Block, caret: bool) -> Self {
        let fallback = [Span { text: block_text(block), ..Default::default() }];
        let mut result = Self::from_spans(spans_of(block).unwrap_or(&fallback));
        result.caret = caret;
        if caret {
            // The tag marker is drawn as a mark beside the line, not as words.
            let whole = spans_to_editor(&result.spans);
            let prefix = whole.chars().count() - Tag::split(&whole).1.chars().count();
            result.spans = take_spans(&result.spans, prefix..usize::MAX);
            result.text = visible_text(&result.spans);
            result.literal = vec![true; result.text.chars().count()];
        }
        result
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// The spans the visible text stands for.
    pub(super) fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Fold an edited visible text back into spans.
    ///
    /// The unchanged head and tail keep their spans. The changed middle is reparsed as
    /// inline Markdown over the run of unmarked characters around it — the characters the
    /// user has been typing — so `**bold**` becomes bold while stored `*literal*` stars,
    /// which are marked, stay literal even inside a formatting wrap. When the run replaces
    /// most of what was there (a selection typed over), the new characters are all fresh
    /// input and parse as Markdown outright. The run then inherits the style of the span
    /// it landed in, so typing inside a sized or coloured run continues it. `text` itself
    /// may still carry markup the parse consumed (a wrap puts `<span ...>` around the
    /// selection); the stored text is always the canonical visible projection of the spans.
    pub(super) fn parse(&mut self, text: &str) -> Vec<Span> {
        let old = std::mem::take(&mut self.text);
        if old == text {
            self.text = old;
            return self.spans.clone();
        }
        let old_chars: Vec<char> = old.chars().collect();
        let new_chars: Vec<char> = text.chars().collect();
        let prefix = old_chars.iter().zip(&new_chars).take_while(|(a, b)| a == b).count();
        let suffix = old_chars[prefix..].iter().rev().zip(new_chars[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        let old_end = old_chars.len().saturating_sub(suffix);
        let new_end = new_chars.len().saturating_sub(suffix);
        // Old characters the run absorbed at its edges: they parse as fresh markup input.
        let mut freed = vec![false; old_chars.len()];
        // The hot run: the unmarked characters around the edit, in both texts. Everything
        // outside it is stored words and keeps its spans and its marks. When the common
        // head and tail cannot be told apart (they overlap), the whole line is the run.
        let (run_start, run_end, new_run_end, whole) = if prefix > old_end || prefix > new_end {
            (0, old_chars.len(), new_chars.len(), true)
        } else {
            let (mut run_start, mut run_end) = (prefix, old_end);
            while run_start > 0 && !self.literal.get(run_start - 1).copied().unwrap_or(true) {
                run_start -= 1;
            }
            while run_end < old_chars.len() && !self.literal.get(run_end).copied().unwrap_or(true) {
                run_end += 1;
            }
            let mut new_run_end = new_chars.len().saturating_sub(old_chars.len() - run_end).min(new_chars.len());
            // A stored character that coincides with markup the edit put at the run's edge
            // is part of the edit, not an unchanged neighbour: `**bold**` typed where
            // `*literal*` stood must see both stars, or `*bold*` parses as italic between
            // leftover stars. The absorbed character is freed from literal protection.
            while run_start > 0
                && self.literal.get(run_start - 1).copied().unwrap_or(false)
                && new_chars.get(run_start) == old_chars.get(run_start - 1)
                && is_markup_char(old_chars[run_start - 1])
            {
                run_start -= 1;
                freed[run_start] = true;
            }
            while run_end < old_chars.len()
                && new_run_end > run_start
                && self.literal.get(run_end).copied().unwrap_or(false)
                && new_chars.get(new_run_end - 1) == old_chars.get(run_end)
                && is_markup_char(old_chars[run_end])
            {
                run_end += 1;
                new_run_end += 1;
                freed[run_end - 1] = true;
            }
            // In the new text the same run ends where the marked suffix resumes.
            (run_start, run_end, new_run_end.min(new_chars.len()), false)
        };
        let old_run = &old_chars[run_start..run_end];
        let new_run = &new_chars[run_start..new_run_end];
        let aligned = lcs_align(old_run, new_run);
        // A run that replaced most of the old words is fresh input: nothing in it continues
        // a stored character, so nothing keeps literal protection. That is what lets
        // `**bold**` typed over a selection become bold instead of `*` + italic + `*`.
        let matched: std::collections::HashSet<usize> = aligned.iter().flatten().copied().collect();
        let fresh = matched.len() * 2 < old_run.len();
        // Where the run lands: the span holding its first character, for inherited style.
        let mut at = 0usize;
        let mut donor = None;
        if !whole {
            for (index, span) in self.spans.iter().enumerate() {
                let len = visible_len(span);
                if run_start < at + len || (run_start == at + len && index + 1 == self.spans.len()) {
                    donor = Some(span.clone());
                    break;
                }
                at += len;
            }
        }
        // The images the run's object characters stand for, in document order.
        let image_pool: Vec<Span> = self.spans.iter().filter(|s| s.image.is_some()).cloned().collect();
        let mut image_at = 0usize;
        // Project the run: characters that continue a marked old character become private
        // markers so the parser keeps them words; everything else is free to be Markdown.
        let mut markers =
            ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{ffffd}').chain('\u{100000}'..='\u{10fffd}').filter(|c| !text.contains(*c));
        // Characters continuing a stored marked character stay literal for good; the rest
        // are fresh input the parser may read as Markdown.
        let stored: Vec<bool> = new_run
            .iter()
            .enumerate()
            .map(|(i, _)| !fresh && aligned[i].map(|j| run_start + j).is_some_and(|g| self.literal.get(g).copied().unwrap_or(false) && !freed[g]))
            .collect();
        // A mark with no partner yet is still being typed: `**bold*` must stay words until
        // the closing stars arrive, or the half-formed pair italicises "bold" and the
        // canonical text loses the stars the next keystroke needs. These stay pending, so
        // the completed `**bold**` parses as bold on the next edit.
        let mut typed = vec![false; new_run.len()];
        for mark in ['*', '_', '~'] {
            if new_run.iter().zip(&stored).filter(|(c, s)| !**s && **c == mark).count() % 2 == 1 {
                for (t, c) in typed.iter_mut().zip(new_run) {
                    if *c == mark {
                        *t = true;
                    }
                }
            }
        }
        let marked: Vec<bool> = stored.iter().zip(&typed).map(|(s, t)| *s || *t).collect();
        let mut projected = String::new();
        let mut restore: Vec<(char, char, bool)> = Vec::new();
        for (i, c) in new_run.iter().enumerate() {
            if marked[i] {
                let marker = markers.next().unwrap_or(*c);
                projected.push(marker);
                restore.push((marker, *c, stored[i]));
            } else {
                projected.push(*c);
            }
        }
        let fragment = parse_inline(&projected);
        let mut spans = take_spans(&self.spans, 0..run_start);
        let mut literal = self.literal[..run_start.min(self.literal.len())].to_vec();
        for span in fragment {
            // Whether the parser gave this run marks of its own, decided before the donor's
            // style is inherited: inherited formatting does not finish pending syntax.
            let finished = !plain(&span);
            if span.image.is_some() {
                let mut span = span;
                inherit(&mut span, donor.as_ref());
                literal.push(true);
                spans.push(span);
                continue;
            }
            let mut pending = span.clone();
            inherit(&mut pending, donor.as_ref());
            let mut buf = String::new();
            let mut flags: Vec<bool> = Vec::new();
            let flush = |buf: &mut String, flags: &mut Vec<bool>, spans: &mut Vec<Span>, literal: &mut Vec<bool>| {
                if buf.is_empty() {
                    return;
                }
                spans.push(Span { text: std::mem::take(buf), ..pending.clone() });
                literal.append(flags);
            };
            for c in span.text.chars() {
                if let Some((_, original, kept)) = restore.iter().find(|(m, _, _)| *m == c) {
                    if *original == INLINE_IMAGE_CHAR {
                        flush(&mut buf, &mut flags, &mut spans, &mut literal);
                        match image_pool.get(image_at) {
                            Some(image) => {
                                image_at += 1;
                                spans.push(image.clone());
                                literal.push(true);
                            }
                            None => {
                                buf.push(*original);
                                flags.push(true);
                            }
                        }
                    } else {
                        buf.push(*original);
                        // A stored character stays literal; one protected only because its
                        // mark was still being typed finishes with the run around it.
                        flags.push(*kept || finished);
                    }
                } else if c == INLINE_IMAGE_CHAR {
                    flush(&mut buf, &mut flags, &mut spans, &mut literal);
                    match image_pool.get(image_at) {
                        Some(image) => {
                            image_at += 1;
                            spans.push(image.clone());
                            literal.push(true);
                        }
                        None => {
                            buf.push(c);
                            flags.push(true);
                        }
                    }
                } else {
                    buf.push(c);
                    flags.push(finished);
                }
            }
            flush(&mut buf, &mut flags, &mut spans, &mut literal);
        }
        spans.extend(take_spans(&self.spans, run_end..usize::MAX));
        literal.extend_from_slice(&self.literal[run_end.min(self.literal.len())..]);
        self.adopt_with_literal(spans, literal);
        self.spans.clone()
    }

    /// [`adopt`], keeping the per-character literal flags the edit computed.
    fn adopt_with_literal(&mut self, spans: Vec<Span>, mut literal: Vec<bool>) {
        self.spans = merge(spans);
        self.text = visible_text(&self.spans);
        if literal.len() != self.text.chars().count() {
            literal = vec![true; self.text.chars().count()];
        }
        self.literal = literal;
    }

    /// The serialized-source entry point kept for callers that still hand over `block_text`:
    /// tags, direction marks and block-kind changes arrive as serialized text. Literal
    /// punctuation lines up with the mask the spans' own serialization reports.
    pub(super) fn parse_source(&self, text: &str) -> Vec<Span> {
        let source = spans_to_editor(&self.spans);
        let (source_chars, text_chars): (Vec<char>, Vec<char>) = (source.chars().collect(), text.chars().collect());
        let prefix = source_chars.iter().zip(&text_chars).take_while(|(a, b)| a == b).count();
        let suffix = source_chars[prefix..].iter().rev().zip(text_chars[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        let (_, mask) = editable_spans(&self.spans);
        let mut literal = vec![false; text_chars.len()];
        let n = prefix.min(mask.len());
        literal[..n].copy_from_slice(&mask[..n]);
        let tail_start = text_chars.len() - suffix;
        let source_tail = source_chars.len() - suffix;
        for i in 0..suffix {
            if let Some(m) = mask.get(source_tail + i) {
                literal[tail_start + i] = *m;
            }
        }
        let mut markers =
            ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{ffffd}').chain('\u{100000}'..='\u{10fffd}').filter(|c| !text.contains(*c));
        let mut projected = String::new();
        let mut restore = Vec::new();
        for (i, c) in text_chars.iter().enumerate() {
            if literal.get(i).copied().unwrap_or(false) {
                let marker = markers.next().unwrap_or(*c);
                projected.push(marker);
                restore.push((marker, *c));
            } else {
                projected.push(*c);
            }
        }
        let mut spans = parse_inline(&projected);
        if !restore.is_empty() {
            for span in &mut spans {
                span.text = span.text.chars().map(|c| restore.iter().find(|(m, _)| *m == c).map_or(c, |(_, original)| *original)).collect();
            }
        }
        spans
    }

    pub(super) fn slice(&self, range: Range<usize>) -> Self {
        let spans = take_spans(&self.spans, range.clone());
        let text = visible_text(&spans);
        let literal = self.literal.get(range.start..range.end).map(<[bool]>::to_vec).unwrap_or_else(|| vec![true; text.chars().count()]);
        Self { spans, text, literal, caret: self.caret }
    }

    pub(super) fn join(&mut self, other: Self) {
        self.spans.extend(other.spans);
        self.spans = merge(std::mem::take(&mut self.spans));
        self.text = visible_text(&self.spans);
        self.literal.extend(other.literal);
        if self.literal.len() != self.text.chars().count() {
            self.literal = vec![true; self.text.chars().count()];
        }
    }

    /// [`format_span_range`] on the visible selection: the words change marks, the text the
    /// caret edits does not.
    pub(super) fn format(&mut self, _text: &str, selection: (usize, usize), format: InlineFormat) -> Option<(String, (usize, usize))> {
        let mut spans = self.spans.clone();
        if !format_span_range(&mut spans, selection.0.min(selection.1)..selection.0.max(selection.1), format) {
            return Some((self.text.clone(), selection));
        }
        self.spans = merge(spans);
        self.text = visible_text(&self.spans);
        Some((self.text.clone(), selection))
    }

    /// [`paint_span_range`] on the visible selection.
    pub(super) fn paint(&mut self, _text: &str, selection: (usize, usize), style: &Span) -> Option<(String, (usize, usize))> {
        let mut spans = self.spans.clone();
        if !paint_span_range(&mut spans, selection.0.min(selection.1)..selection.0.max(selection.1), style) {
            return Some((self.text.clone(), selection));
        }
        self.spans = merge(spans);
        self.text = visible_text(&self.spans);
        Some((self.text.clone(), selection))
    }

    /// The formatting at the caret or of its selection, read off the spans directly.
    pub(super) fn style(&self, _text: &str, selection: (usize, usize)) -> Option<Span> {
        let (lo, hi) = (selection.0.min(selection.1), selection.0.max(selection.1));
        let len = self.text.chars().count();
        if hi > len {
            return None;
        }
        let at = if lo == hi { lo.saturating_sub(1).min(len.saturating_sub(1)) } else { lo };
        let mut pos = 0usize;
        for span in &self.spans {
            let len = visible_len(span);
            if at < pos + len {
                if span.image.is_some() || span.math {
                    return None;
                }
                return Some(Span { text: String::new(), link: None, image: None, math: false, ..span.clone() });
            }
            pos += len;
        }
        self.spans.last().and_then(|span| {
            (!span.text.is_empty() && span.image.is_none() && !span.math).then(|| Span {
                text: String::new(),
                link: None,
                image: None,
                math: false,
                ..span.clone()
            })
        })
    }

    pub(super) fn fonts(&self, _text: &str, selection: (usize, usize)) -> Option<FontSelection> {
        let (lo, hi) = (selection.0.min(selection.1), selection.0.max(selection.1));
        let len = self.text.chars().count();
        if hi > len {
            return None;
        }
        let spans = if lo == hi {
            let at = lo.saturating_sub(1).min(len.saturating_sub(1));
            let mut pos = 0usize;
            self.spans
                .iter()
                .find(|span| {
                    let l = visible_len(span);
                    let hit = at < pos + l;
                    pos += l;
                    hit
                })
                .cloned()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            take_spans(&self.spans, lo..hi)
        };
        let spans: Vec<Span> = spans.into_iter().filter(|s| !s.text.is_empty() && s.image.is_none()).collect();
        (!spans.is_empty()).then(|| FontSelection::from_spans(&spans))
    }

    /// The visible character position of a serialized-source character position, for the
    /// across-block selection that still counts bytes of `block_text`.
    pub(super) fn visible_pos_of_source(&self, source_pos: usize) -> usize {
        let mut source = 0usize;
        let mut visible = 0usize;
        for span in &self.spans {
            let ser = spans_to_editor(std::slice::from_ref(span));
            let ser_len = ser.chars().count();
            let vis_len = visible_len(span);
            if span.image.is_some() {
                if source_pos <= source {
                    return visible;
                }
                if source_pos >= source + ser_len {
                    source += ser_len;
                    visible += vis_len;
                    continue;
                }
                return visible + usize::from(source_pos > source);
            }
            // The text sits inside the marks; positions inside the marks clamp to the edge.
            let open = ser.find(&span.text).map(|byte| ser[..byte].chars().count()).unwrap_or(0);
            let text_len = span.text.chars().count();
            if source_pos < source + open {
                return visible;
            }
            if source_pos <= source + open + text_len {
                return visible + (source_pos - source - open).min(text_len);
            }
            source += ser_len;
            visible += vis_len;
        }
        visible
    }

    /// The serialized-source character position of a visible character position.
    pub(super) fn source_pos_of_visible(&self, visible_pos: usize) -> usize {
        let mut source = 0usize;
        let mut visible = 0usize;
        for span in &self.spans {
            let ser = spans_to_editor(std::slice::from_ref(span));
            let ser_len = ser.chars().count();
            let vis_len = visible_len(span);
            if visible_pos <= visible + vis_len || visible + vis_len >= self.text.chars().count() {
                if span.image.is_some() {
                    return source + if visible_pos > visible { ser_len } else { 0 };
                }
                let open = ser.find(&span.text).map(|byte| ser[..byte].chars().count()).unwrap_or(0);
                let text_len = span.text.chars().count();
                let inside = (visible_pos - visible).min(text_len);
                return source + open + inside;
            }
            source += ser_len;
            visible += vis_len;
        }
        source
    }

    /// The spans covering a visible range, for the clipboard.
    pub(super) fn selected(&mut self, _text: &str, range: Range<usize>) -> Option<Vec<Span>> {
        let spans = take_spans(&self.spans, range);
        (!spans.is_empty()).then_some(spans)
    }

    /// Write the spans back into a block without reparsing: the text already is the
    /// canonical visible projection, so the formatting `format`/`paint` applied is kept
    /// exactly, and the tag marker goes back on the front for the caret's view.
    fn commit(&self, block: &mut Block) {
        let mut spans = self.spans.clone();
        if self.caret
            && let Some(tag) = Tag::split(&block_text(block)).0
        {
            spans.insert(0, Span { text: tag.marker(), ..Default::default() });
        }
        let spans = merge(spans);
        match block {
            Block::Heading { spans: value, .. } | Block::Paragraph(value) | Block::Quote(value) | Block::Item { spans: value, .. } => *value = spans,
            Block::Code { text: value, .. } | Block::MathBlock(value) => *value = self.text.clone(),
            Block::Image { alt, .. } => *alt = self.text.clone(),
            Block::Table { .. } | Block::Rule => {}
        }
    }

    /// Replace the visible characters `range` with `text` parsed as inline markup, keeping
    /// the spans and literal marks on both sides. For dialog edits (links, video) whose
    /// replacement may carry markup the caret's text cannot show — and whose visible words
    /// may be identical to what was there, which a text diff could never tell apart.
    pub(super) fn replace(&mut self, range: Range<usize>, text: &str) {
        let len = self.text.chars().count();
        let (a, b) = (range.start.min(range.end).min(len), range.start.max(range.end).min(len));
        // The span the replacement lands in lends its style, the way typing inherits it.
        let mut at = 0usize;
        let mut donor = None;
        for (index, span) in self.spans.iter().enumerate() {
            let l = visible_len(span);
            if a < at + l || (a == at + l && index + 1 == self.spans.len()) {
                donor = Some(span.clone());
                break;
            }
            at += l;
        }
        let mut spans = take_spans(&self.spans, 0..a);
        let mut literal = self.literal[..a.min(self.literal.len())].to_vec();
        for mut span in parse_inline(text) {
            if span.image.is_some() {
                literal.push(true);
                spans.push(span);
                continue;
            }
            let finished = !plain(&span);
            // The replacement's own markup decides the link: plain words must be able to
            // take a link off, which inheriting the donor's address would prevent.
            let link = span.link.clone();
            inherit(&mut span, donor.as_ref());
            span.link = link;
            literal.extend(std::iter::repeat_n(finished, span.text.chars().count()));
            spans.push(span);
        }
        spans.extend(take_spans(&self.spans, b..usize::MAX));
        literal.extend_from_slice(&self.literal[b.min(self.literal.len())..]);
        self.adopt_with_literal(spans, literal);
    }

    pub(super) fn write(&mut self, block: &mut Block, text: &str) {
        if text == self.text {
            self.commit(block);
            return;
        }
        // The tag is not part of the visible text; it goes back on before parsing so it
        // lands in the words exactly where the block already carries it.
        let tag = Tag::split(&block_text(block)).0;
        let text = match tag {
            Some(tag) => format!("{}{text}", tag.marker()),
            None => text.to_owned(),
        };
        let spans = self.parse(&text);
        match block {
            Block::Heading { spans: value, .. } | Block::Paragraph(value) | Block::Quote(value) | Block::Item { spans: value, .. } => *value = spans,
            Block::Code { text: value, .. } | Block::MathBlock(value) => *value = text.to_owned(),
            Block::Image { alt, .. } => *alt = text.to_owned(),
            Block::Table { .. } | Block::Rule => {}
        }
    }
}

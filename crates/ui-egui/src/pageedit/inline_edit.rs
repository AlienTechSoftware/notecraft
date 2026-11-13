//! Retains literal punctuation and atomic images while the existing Markdown-style editor
//! edits its visible source. Storage quoting belongs to the formats crate, never the widget.

use std::collections::BTreeMap;
use std::ops::Range;

use notecraft_formats::markdown::{Block, Span, parse_inline};
use notecraft_formats::render::editable_spans;
use notecraft_model::Tag;

use super::{FontSelection, INLINE_IMAGE_CHAR, InlineFormat, block_text, format_source_selection, selection_fonts, spans_to_editor};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineEdit {
    text: String,
    literal: Vec<bool>,
    images: Vec<(usize, Span)>,
}

#[derive(Clone)]
enum Atom {
    Literal(char),
    Image(Span),
}

type Projection = (String, Vec<(char, Atom)>);

impl InlineEdit {
    pub(super) fn from_spans(spans: &[Span]) -> Self {
        let (text, literal) = editable_spans(spans);
        Self { text, literal, images: Vec::new() }
    }

    pub(super) fn new(block: &Block, caret: bool) -> Self {
        let fallback = [Span { text: block_text(block), ..Default::default() }];
        let spans = super::spans_of(block).unwrap_or(&fallback);
        let mut result = Self::from_spans(spans);
        if caret {
            let prefix = result.text.chars().count() - Tag::split(&result.text).1.chars().count();
            result = result.slice(prefix..result.literal.len());
            let mut after = 0;
            for span in spans.iter().filter(|span| span.image.is_some()) {
                let source = spans_to_editor(std::slice::from_ref(span));
                let found = result.text.get(after..).and_then(|rest| {
                    rest.match_indices(&source).map(|(relative, _)| after + relative).find(|start| {
                        let a = result.text[..*start].chars().count();
                        !result.literal[a..a + source.chars().count()].iter().any(|literal| *literal)
                    })
                });
                if let Some(start) = found {
                    let a = result.text[..start].chars().count();
                    let b = a + source.chars().count();
                    result.replace_metadata(a..b, 1);
                    result.text.replace_range(start..start + source.len(), &INLINE_IMAGE_CHAR.to_string());
                    result.images.push((a, span.clone()));
                    after = start + INLINE_IMAGE_CHAR.len_utf8();
                }
            }
        }
        result
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    /// Where each inline image sits in the caret's text, with the span it stands for.
    pub(super) fn images(&self) -> &[(usize, Span)] {
        &self.images
    }

    /// The range of a block's drawn words that editable-text characters `range` cover,
    /// as bytes of the drawn text. The drawn text is what the preview lays out: the
    /// marks and the tag marker are not in it, so a byte of the editable text is not a
    /// byte of it. The covered characters are found by marking them in the editable
    /// text, parsing it back the way the caret does, and counting the drawn characters
    /// up to the marks.
    pub(super) fn drawn_range(&mut self, range: Range<usize>) -> Option<(usize, usize)> {
        let text = self.text.clone();
        let (projected, mapping) = self.project(&text)?;
        let chars: Vec<char> = projected.chars().collect();
        let (start, end) = (range.start.min(chars.len()), range.end.min(chars.len()));
        if start >= end {
            return None;
        }
        let mut markers =
            ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{ffffd}').chain('\u{100000}'..='\u{10fffd}').filter(|c| !projected.contains(*c));
        let (open, close) = (markers.next()?, markers.next()?);
        let mut marked = String::new();
        for at in 0..=chars.len() {
            if at == start {
                marked.push(open);
            }
            if at == end {
                marked.push(close);
            }
            if let Some(c) = chars.get(at) {
                marked.push(*c);
            }
        }
        let spans = Self::restore_spans(parse_inline(&marked), &mapping);
        if spans.iter().any(|s| s.math) {
            return None;
        }
        let mut drawn = String::new();
        let mut at = 0usize;
        let (mut a, mut b) = (None, None);
        for span in spans.iter().filter(|s| s.image.is_none()) {
            for c in span.text.chars() {
                if c == open {
                    a = Some(at);
                } else if c == close {
                    b = Some(at);
                } else {
                    drawn.push(c);
                    at += 1;
                }
            }
        }
        let (Some(a), Some(b)) = (a, b) else { return None };
        // The tag marker is drawn as a mark beside the line, not as words.
        let prefix = drawn.chars().count() - Tag::split(&drawn).1.chars().count();
        let real = drawn.char_indices().nth(prefix).map(|(byte, _)| &drawn[byte..]).unwrap_or("");
        let byte_at = |char_at: usize| {
            let char_at = char_at.saturating_sub(prefix);
            real.char_indices().nth(char_at).map(|(byte, _)| byte).unwrap_or(real.len())
        };
        Some((byte_at(a), byte_at(b)))
    }

    pub(super) fn slice(&self, range: Range<usize>) -> Self {
        let start = range.start.min(self.literal.len());
        let end = range.end.min(self.literal.len()).max(start);
        Self {
            text: self.text.chars().skip(start).take(end - start).collect(),
            literal: self.literal[start..end].to_vec(),
            images: self.images.iter().filter(|(at, _)| *at >= start && *at < end).map(|(at, span)| (at - start, span.clone())).collect(),
        }
    }

    pub(super) fn join(&mut self, other: Self) {
        let offset = self.literal.len();
        self.text.push_str(&other.text);
        self.literal.extend(other.literal);
        self.images.extend(other.images.into_iter().map(|(at, span)| (at + offset, span)));
    }

    fn replace_metadata(&mut self, range: Range<usize>, inserted: usize) {
        self.literal.splice(range.clone(), std::iter::repeat_n(false, inserted));
        self.images.retain_mut(|(at, _)| {
            if *at >= range.end {
                *at = *at - (range.end - range.start) + inserted;
                true
            } else {
                *at < range.start
            }
        });
    }

    fn update(&mut self, text: &str) {
        if self.text == text {
            return;
        }
        let before: Vec<char> = self.text.chars().collect();
        let after: Vec<char> = text.chars().collect();
        let prefix = before.iter().zip(&after).take_while(|(a, b)| a == b).count();
        let suffix = before[prefix..].iter().rev().zip(after[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
        self.replace_metadata(prefix..before.len() - suffix, after.len() - prefix - suffix);
        self.text = text.to_owned();
    }

    /// Use the widget's actual replacement range, including equal characters at its edges.
    /// A text diff alone cannot distinguish replacing '*literal*' from retaining its stars.
    pub(super) fn update_at(&mut self, text: &str, before_selection: (usize, usize), after_selection: (usize, usize)) {
        let before: Vec<char> = self.text.chars().collect();
        let after: Vec<char> = text.chars().collect();
        let mut a = before_selection.0.min(before_selection.1).min(before.len());
        let mut b = before_selection.0.max(before_selection.1).min(before.len());
        if a == b && after.len() < before.len() {
            a = after_selection.0.min(after_selection.1).min(before.len());
            b = (a + before.len() - after.len()).min(before.len());
        }
        if let Some(inserted) = after.len().checked_add(b - a).and_then(|len| len.checked_sub(before.len()))
            && after.get(..a) == before.get(..a)
            && after.get(a + inserted..) == before.get(b..)
        {
            self.replace_metadata(a..b, inserted);
            self.text = text.to_owned();
            return;
        }
        self.update(text);
    }

    // A temporary single-character projection lets existing parsing/formatting keep its
    // character offsets. Choose markers absent from user text, and restore before output.
    fn project(&mut self, text: &str) -> Option<Projection> {
        self.update(text);
        let mut mapping = Vec::new();
        let mut available =
            ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{ffffd}').chain('\u{100000}'..='\u{10fffd}').filter(|c| !text.contains(*c));
        let mut projected = String::new();
        for (at, (c, literal)) in text.chars().zip(&self.literal).enumerate() {
            let image = self.images.iter().find(|(index, _)| *index == at).map(|(_, span)| span);
            if let Some(image) = image {
                let marker = available.next()?;
                mapping.push((marker, Atom::Image(image.clone())));
                projected.push(marker);
            } else if *literal {
                if let Some((marker, _)) = mapping.iter().find(|(_, atom)| matches!(atom, Atom::Literal(original) if *original == c)) {
                    projected.push(*marker);
                } else {
                    let marker = available.next()?;
                    mapping.push((marker, Atom::Literal(c)));
                    projected.push(marker);
                }
            } else {
                projected.push(c);
            }
        }
        Some((projected, mapping))
    }

    fn restored_char(c: char, mapping: &[(char, Atom)]) -> char {
        match mapping.iter().find(|(marker, _)| *marker == c).map(|(_, atom)| atom) {
            Some(Atom::Literal(original)) => *original,
            Some(Atom::Image(_)) => INLINE_IMAGE_CHAR,
            None => c,
        }
    }

    fn restore(text: &str, mapping: &[(char, Atom)]) -> String {
        text.chars().map(|c| Self::restored_char(c, mapping)).collect()
    }

    fn restore_spans(spans: Vec<Span>, mapping: &[(char, Atom)]) -> Vec<Span> {
        let mut result = Vec::new();
        for span in spans {
            if span.text.is_empty() {
                result.push(span);
                continue;
            }
            let mut text = String::new();
            for c in span.text.chars() {
                if let Some((_, Atom::Image(image))) = mapping.iter().find(|(marker, _)| *marker == c) {
                    if !text.is_empty() {
                        result.push(Span { text: std::mem::take(&mut text), ..span.clone() });
                    }
                    result.push(image.clone());
                } else {
                    text.push(Self::restored_char(c, mapping));
                }
            }
            if !text.is_empty() {
                result.push(Span { text, ..span });
            }
        }
        result
    }

    // Collision-free fallback for text containing every private-use marker.
    fn escaped_source(&self) -> String {
        let mut source = String::new();
        for (at, (c, literal)) in self.text.chars().zip(&self.literal).enumerate() {
            if let Some((_, image)) = self.images.iter().find(|(index, _)| *index == at) {
                source.push_str(&notecraft_formats::render::spans_to_markdown(std::slice::from_ref(image)));
            } else {
                if *literal {
                    source.push('\\');
                }
                source.push(c);
            }
        }
        source
    }

    fn parse_tagged(&mut self, text: &str, tag: Option<Tag>) -> Vec<Span> {
        if let Some((projected, mapping)) = self.project(text) {
            let source = tag.map_or(projected.clone(), |tag| Tag::set(&projected, Some(tag)));
            Self::restore_spans(parse_inline(&source), &mapping)
        } else {
            let source = self.escaped_source();
            parse_inline(&tag.map_or(source.clone(), |tag| Tag::set(&source, Some(tag))))
        }
    }

    /// Select from the complete projected line so surrounding formatting still applies.
    /// Markers are absent from the input and are discarded before anything leaves this helper.
    pub(super) fn selected(&mut self, text: &str, range: Range<usize>) -> Option<Vec<Span>> {
        let (projected, mapping) = self.project(text)?;
        let chars: Vec<char> = projected.chars().collect();
        let start = range.start.min(chars.len());
        let end = range.end.min(chars.len());
        if start >= end {
            return None;
        }
        let mut markers = ('\u{e000}'..='\u{f8ff}').chain('\u{f0000}'..='\u{ffffd}').filter(|c| !projected.contains(*c));
        let open = markers.next()?;
        let close = markers.next()?;
        let mut marked = String::new();
        for at in 0..=chars.len() {
            if at == start {
                marked.push(open);
            }
            if at == end {
                marked.push(close);
            }
            if let Some(c) = chars.get(at) {
                marked.push(*c);
            }
        }
        let mut selected = Vec::new();
        let mut inside = false;
        let mut opened = false;
        let mut closed = false;
        for span in parse_inline(&marked) {
            let mut text = String::new();
            for c in span.text.chars() {
                if c == open {
                    inside = true;
                    opened = true;
                } else if c == close {
                    inside = false;
                    closed = true;
                } else if inside {
                    text.push(c);
                }
            }
            if !text.is_empty() {
                selected.push(Span { text, ..span });
            }
        }
        let spans = Self::restore_spans(selected, &mapping);
        (opened && closed && !spans.is_empty()).then_some(spans)
    }

    pub(super) fn parse(&mut self, text: &str) -> Vec<Span> {
        self.parse_tagged(text, None)
    }

    pub(super) fn write(&mut self, block: &mut Block, text: &str) {
        let tag = Tag::split(&block_text(block)).0;
        let spans = self.parse_tagged(text, tag);
        match block {
            Block::Heading { spans: value, .. } | Block::Paragraph(value) | Block::Quote(value) | Block::Item { spans: value, .. } => *value = spans,
            Block::Code { text: value, .. } | Block::MathBlock(value) => *value = text.to_owned(),
            Block::Image { alt, .. } => *alt = text.to_owned(),
            Block::Table { .. } | Block::Rule => {}
        }
    }

    pub(super) fn format(&mut self, text: &str, selection: (usize, usize), format: InlineFormat) -> Option<(String, (usize, usize))> {
        if text.contains(INLINE_IMAGE_CHAR) {
            return None;
        }
        let (projected, mapping) = self.project(text)?;
        let (formatted, selection) = format_source_selection(&projected, selection, format)?;
        self.literal = formatted.chars().map(|c| mapping.iter().any(|(marker, _)| *marker == c)).collect();
        self.text = Self::restore(&formatted, &mapping);
        Some((self.text.clone(), selection))
    }

    /// [`super::paint_source_selection`] on the line's own text, as [`Self::format`] marks it.
    pub(super) fn paint(&mut self, text: &str, selection: (usize, usize), style: &Span) -> Option<(String, (usize, usize))> {
        if text.contains(INLINE_IMAGE_CHAR) {
            return None;
        }
        let (projected, mapping) = self.project(text)?;
        let (painted, selection) = super::paint_source_selection(&projected, selection, style)?;
        self.literal = painted.chars().map(|c| mapping.iter().any(|(marker, _)| *marker == c)).collect();
        self.text = Self::restore(&painted, &mapping);
        Some((self.text.clone(), selection))
    }

    /// [`super::style_at`] on the line's own text.
    pub(super) fn style(&self, text: &str, selection: (usize, usize)) -> Option<Span> {
        if text.contains(INLINE_IMAGE_CHAR) {
            return None;
        }
        let (projected, _) = self.clone().project(text)?;
        super::style_at(&projected, selection)
    }

    pub(super) fn fonts(&self, text: &str, selection: (usize, usize)) -> Option<FontSelection> {
        if text.contains(INLINE_IMAGE_CHAR) {
            return None;
        }
        let (projected, _) = self.clone().project(text)?;
        selection_fonts(&projected, selection)
    }

    pub(super) fn layout(&self, text: &str, layout: impl FnOnce(&str) -> egui::text::LayoutJob) -> egui::text::LayoutJob {
        let Some((projected, mapping)) = self.clone().project(text) else {
            return layout(text);
        };
        let mut job = layout(&projected);
        let restored = Self::restore(&job.text, &mapping);
        let offsets: BTreeMap<usize, usize> = job
            .text
            .char_indices()
            .map(|(byte, _)| byte)
            .chain([job.text.len()])
            .zip(restored.char_indices().map(|(byte, _)| byte).chain([restored.len()]))
            .collect();
        for section in &mut job.sections {
            section.byte_range.start.0 = offsets.get(&section.byte_range.start.0).copied().unwrap_or(restored.len());
            section.byte_range.end.0 = offsets.get(&section.byte_range.end.0).copied().unwrap_or(restored.len());
        }
        job.text = restored;
        job
    }
}

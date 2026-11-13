//! The page, written on directly: a caret in the page itself, like OneNote and Word.
//!
//! The page is a list of blocks. The block under the caret is a text box drawn in that
//! block's own typography; every other block is drawn finished. Typing changes the block,
//! Enter starts the next one, Backspace at the start joins it to the one above, and Tab
//! indents a list item. Nothing shows the page's markup: the source is a separate pane
//! behind the Code button.
//!
//! The document is the blocks. After every change they are written back to the page's own
//! format (`notecraft_formats::render`), so the file on disk stays HTML or Markdown.

use crate::pagetext;
use crate::preview;
use crate::theme::Tokens;
use egui::{Key, Sense, Ui, vec2};
use notecraft_formats::markdown::{Block, Span, parse_inline};
mod inline_edit;
pub use inline_edit::InlineEdit;

fn spans_to_editor(spans: &[Span]) -> String {
    notecraft_formats::render::editable_spans(spans).0
}

fn spans_of(block: &Block) -> Option<&[Span]> {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => Some(spans),
        _ => None,
    }
}

use notecraft_model::Tag;

/// Where a selection that a right click collapsed is to be put back: `(block, primary, secondary)`.
/// Set by the container, taken by the block's text box on the next frame. A text box moves its
/// selection to the pointer the moment any button goes down, so a menu that acts on the words
/// (Cut, Copy, Bold) would find none selected unless they are put back.
pub fn restore_selection_id() -> egui::Id {
    egui::Id::new("restore-selection")
}

/// What the caret is in, and the text it is editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Caret {
    /// The block the caret is in.
    pub block: usize,
    /// That block's text while it is being edited (inline marks included, nothing else).
    pub text: String,
    /// Ask egui for the keyboard on the next frame.
    pub focus: bool,
    /// What is selected inside the block, as character positions. Equal when nothing is
    /// selected: that is where a mark or a snippet goes.
    pub selection: (usize, usize),
    /// Where the caret should land when the block takes the keyboard: the character the
    /// click was on. `None` puts it at the end.
    pub place: Option<usize>,
    /// Which cell of a table the caret is in, as (row, column), where row 0 is the
    /// heading. `None` for every other kind of block.
    pub cell: Option<(usize, usize)>,
    /// The far corner of a block of cells dragged across, with `cell` as the near one.
    /// `None` while one cell is being written in, which is the ordinary case.
    pub span: Option<(usize, usize)>,
    /// Literal/image identity for this visit to the editor, independent of storage quoting.
    pub inline: Option<InlineEdit>,
}

impl Caret {
    fn ensure_inline(&mut self, block: &Block) -> &mut InlineEdit {
        self.inline.get_or_insert_with(|| {
            self.cell.and_then(|cell| cell_spans(block, cell)).map(InlineEdit::from_spans).unwrap_or_else(|| InlineEdit::new(block, true))
        })
    }

    pub fn write(&mut self, block: &mut Block) {
        let text = self.text.clone();
        if let Some(cell) = self.cell {
            let spans = self.ensure_inline(block).parse(&text);
            if let Some(value) = cell_spans_mut(block, cell) {
                *value = spans;
            }
        } else {
            self.ensure_inline(block).write(block, &text);
        }
    }

    pub fn parse(&self, block: &Block, text: &str) -> Vec<Span> {
        self.inline.clone().unwrap_or_else(|| InlineEdit::new(block, true)).parse(text)
    }

    /// A clipboard fragment from the semantic line, including literal/image identity.
    pub fn copied_blocks(&self, block: &Block) -> Option<Vec<Block>> {
        let a = self.selection.0.min(self.selection.1);
        let b = self.selection.0.max(self.selection.1);
        if a == b {
            return None;
        }
        match block {
            Block::Code { lang, .. } => Some(vec![Block::Code { lang: lang.clone(), text: self.text.chars().skip(a).take(b - a).collect() }]),
            Block::MathBlock(_) => Some(vec![Block::MathBlock(self.text.chars().skip(a).take(b - a).collect())]),
            _ => {
                let mut caret = self.clone();
                let spans = caret.ensure_inline(block).selected(&self.text, a..b)?;
                Some(vec![Block::Paragraph(spans)])
            }
        }
    }

    /// Clear only the active text/cell and hydrate the same editing target afterwards.
    pub fn clear_marks(&mut self, block: &mut Block) {
        self.write(block);
        if let Some(cell) = self.cell {
            if let Some(spans) = cell_spans_mut(block, cell) {
                clear_span_marks(spans);
            }
            self.text = cell_text(block, cell).unwrap_or_default();
        } else {
            clear_marks(block);
            self.text = caret_text(block);
        }
        self.inline = None;
        self.selection = (0, self.text.chars().count());
    }

    pub fn format(&mut self, block: &Block, format: InlineFormat) -> Option<(String, (usize, usize))> {
        let (text, selection) = (self.text.clone(), self.selection);
        self.ensure_inline(block).format(&text, selection, format)
    }

    pub fn fonts(&self, block: &Block) -> Option<FontSelection> {
        self.inline.clone().unwrap_or_else(|| InlineEdit::new(block, true)).fonts(&self.text, self.selection)
    }

    /// The formatting at the caret or of its selection, for the Format Painter.
    pub fn style(&self, block: &Block) -> Option<Span> {
        self.inline.clone().unwrap_or_else(|| InlineEdit::new(block, true)).style(&self.text, self.selection)
    }

    /// Gives the selected words `style`, for the Format Painter.
    pub fn paint(&mut self, block: &Block, style: &Span) -> Option<(String, (usize, usize))> {
        let (text, selection) = (self.text.clone(), self.selection);
        self.ensure_inline(block).paint(&text, selection, style)
    }
}

/// The block of cells a caret covers, as (top-left, bottom-right), or `None` when it is
/// in one cell. Both corners are inclusive, and they come back sorted however the drag
/// went: up and to the left is the same block as down and to the right.
pub fn cell_range(caret: &Caret) -> Option<((usize, usize), (usize, usize))> {
    let (from, to) = (caret.cell?, caret.span?);
    Some(((from.0.min(to.0), from.1.min(to.1)), (from.0.max(to.0), from.1.max(to.1))))
}

/// Which way a selection reaches from the cell the caret is in, for the Table tab's
/// Select buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSpan {
    /// The whole table, heading and all.
    Whole,
    /// The caret's row.
    Row,
    /// The caret's column.
    Column,
}

/// The two corners of what a Select button picks, given the table and where the caret is.
pub fn span_of(block: &Block, cell: (usize, usize), what: TableSpan) -> Option<((usize, usize), (usize, usize))> {
    let (rows, columns) = table_size(block)?;
    let (last_row, last_column) = (rows.max(1) - 1, columns.max(1) - 1);
    Some(match what {
        TableSpan::Whole => ((0, 0), (last_row, last_column)),
        TableSpan::Row => ((cell.0, 0), (cell.0, last_column)),
        TableSpan::Column => ((0, cell.1), (last_row, cell.1)),
    })
}

/// The character of `text` a click at `x` lands on, given how the block is drawn. Used so a
/// click puts the caret where the pointer is, the way it does in any editor.
pub fn char_at(
    ui: &Ui,
    text: &str,
    size: f32,
    strong: bool,
    t: &Tokens,
    width: f32,
    offset: egui::Vec2,
    page: Option<notecraft_model::Dir>,
) -> usize {
    let job = pagetext::inline_job_with_fonts(ui.ctx(), text, size, strong, t, width.max(1.0), page);
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let chars = text.chars().count();
    // Two things have to be undone for a right-to-left line, both measured in
    // `tests/arabic_layout.rs`:
    //
    // 1. egui lays such a row out to the LEFT of the origin, so its x runs from minus the
    //    row width up to zero, while the pointer offset given here starts at zero.
    // 2. The index egui answers with counts glyphs, not characters, and a shaped Arabic
    //    word has more glyphs than characters (a mark above a letter is its own glyph with
    //    no width). Counting only the glyphs that advance gives the character back.
    //
    // What is left is the visual position: in a line that reads right to left, the leftmost
    // character is the last one, so the count is taken from the other end.
    let rtl = galley.rect.max.x <= 0.0 && galley.rect.width() > 0.0;
    let probe = if rtl { egui::vec2(offset.x - galley.rect.width(), offset.y) } else { offset };
    let hit = galley.cursor_from_pos(probe).index.0;
    let visual = galley.rows.iter().flat_map(|r| r.glyphs.iter()).take(hit).filter(|g| g.advance_width > 0.0).count();
    if rtl { chars.saturating_sub(visual) } else { visual.min(chars) }
}

/// Puts `marks` around the selection, or takes them away again when they are already there.
/// Returns the new text and the new selection.
/// Puts the selection inside a directional isolate, so that run reads the way the writer
/// says whatever surrounds it ([UAX #9]: RLI or LRI, then PDI). With nothing selected the
/// pair goes in at the caret and the caret lands between them, so what is typed next is
/// inside it. Asking again for a run that already has one takes it off.
///
/// The characters are invisible and are part of the text, so the run keeps its direction in
/// any other editor too. What NoteCraft cannot do yet is draw a mixed line in the right
/// order; that needs the bidi algorithm (`docs/gaps.md`).
pub fn isolate(text: &str, selection: (usize, usize), dir: notecraft_model::Dir) -> (String, (usize, usize)) {
    const PDI: char = '\u{2069}';
    const RLI: char = '\u{2067}';
    const LRI: char = '\u{2066}';
    let open = if dir.is_rtl() { RLI } else { LRI };
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (selection.0.min(selection.1).min(chars.len()), selection.0.max(selection.1).min(chars.len()));
    let before: String = chars.get(..a).unwrap_or(&[]).iter().collect();
    let middle: String = chars.get(a..b).unwrap_or(&[]).iter().collect();
    let after: String = chars.get(b..).unwrap_or(&[]).iter().collect();
    // Already isolated, either way round: take it off, so the same button is the way back.
    if middle.starts_with([RLI, LRI]) && middle.ends_with(PDI) && middle.chars().count() >= 2 {
        let inner: String = middle.chars().skip(1).take(middle.chars().count() - 2).collect();
        let len = inner.chars().count();
        return (format!("{before}{inner}{after}"), (a, a + len));
    }
    let len = middle.chars().count();
    (format!("{before}{open}{middle}{PDI}{after}"), (a + 1, a + 1 + len))
}

/// An inline formatting change over visible character offsets. Images occupy one offset.
#[derive(Clone, Debug)]
pub enum InlineFormat {
    Bold,
    Italic,
    Strike,
    Underline,
    Subscript,
    Superscript,
    Code,
    Size(u8),
    Family(String),
}

/// Formats text in a visible-character range without splitting atomic images or maths.
/// Returns false when no formattable text was selected or nothing changed.
pub fn format_span_range(spans: &mut Vec<Span>, range: std::ops::Range<usize>, format: InlineFormat) -> bool {
    use notecraft_formats::markdown::Script;
    if range.start >= range.end {
        return false;
    }
    let selected = |span: &Span, start: usize| {
        let len = if span.image.is_some() { 1 } else { span.text.chars().count() };
        (start < range.end && start.saturating_add(len) > range.start && span.image.is_none() && !span.math && !span.text.is_empty(), len)
    };
    let active = |span: &Span| match &format {
        InlineFormat::Bold => span.bold,
        InlineFormat::Italic => span.italic,
        InlineFormat::Strike => span.strike,
        InlineFormat::Underline => span.underline,
        InlineFormat::Subscript => span.script == Some(Script::Subscript),
        InlineFormat::Superscript => span.script == Some(Script::Superscript),
        InlineFormat::Code => span.code,
        InlineFormat::Size(size) => span.font_size == Some(*size),
        InlineFormat::Family(family) => span.font_family.as_ref() == Some(family),
    };
    let mut at = 0usize;
    let mut any = false;
    let mut all = true;
    for span in spans.iter() {
        let (hit, len) = selected(span, at);
        if hit {
            any = true;
            all &= active(span);
        }
        at += len;
    }
    if !any || (all && matches!(format, InlineFormat::Size(_) | InlineFormat::Family(_))) {
        return false;
    }
    let mut output = Vec::new();
    at = 0;
    for span in std::mem::take(spans) {
        let (hit, len) = selected(&span, at);
        if hit {
            let a = range.start.saturating_sub(at).min(len);
            let b = range.end.saturating_sub(at).min(len);
            let first = span.text.char_indices().nth(a).map_or(span.text.len(), |(i, _)| i);
            let last = span.text.char_indices().nth(b).map_or(span.text.len(), |(i, _)| i);
            for (text, change) in [(&span.text[..first], false), (&span.text[first..last], true), (&span.text[last..], false)] {
                if text.is_empty() {
                    continue;
                }
                let mut piece = Span { text: text.to_owned(), ..span.clone() };
                if change {
                    match &format {
                        InlineFormat::Bold => piece.bold = !all,
                        InlineFormat::Italic => piece.italic = !all,
                        InlineFormat::Strike => piece.strike = !all,
                        InlineFormat::Underline => piece.underline = !all,
                        InlineFormat::Subscript => piece.script = (!all).then_some(Script::Subscript),
                        InlineFormat::Superscript => piece.script = (!all).then_some(Script::Superscript),
                        InlineFormat::Code => piece.code = !all,
                        InlineFormat::Size(size) => piece.font_size = Some(*size),
                        InlineFormat::Family(family) => piece.font_family = Some(family.clone()),
                    }
                }
                output.push(piece);
            }
        } else {
            output.push(span);
        }
        at += len;
    }
    *spans = output;
    true
}

fn span_characters(spans: &[Span]) -> Option<Vec<(char, Span)>> {
    let mut result = Vec::new();
    for span in spans {
        if span.image.is_some() || span.math {
            return None;
        }
        let style = Span { text: String::new(), ..span.clone() };
        result.extend(span.text.chars().map(|character| (character, style.clone())));
    }
    Some(result)
}

fn source_offset(text: &str, offset: usize, marker: char, original: &[(char, Span)]) -> Option<usize> {
    let (probe, _) = insert_at_caret(text, (offset, offset), &marker.to_string());
    let mut characters = span_characters(&parse_inline(&probe))?;
    let at = characters.iter().position(|(character, _)| *character == marker)?;
    characters.remove(at);
    (characters == original).then_some(at)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum FontValue<T> {
    #[default]
    Unavailable,
    Mixed,
    Value(Option<T>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FontSelection {
    pub family: FontValue<String>,
    pub size: FontValue<u8>,
}

impl FontSelection {
    pub fn merge(&mut self, other: Self) {
        fn merge_value<T: PartialEq>(current: &mut FontValue<T>, other: FontValue<T>) {
            match (&*current, &other) {
                (FontValue::Value(a), FontValue::Value(b)) if a == b => {}
                (FontValue::Unavailable, _) | (_, FontValue::Unavailable) => *current = FontValue::Unavailable,
                _ => *current = FontValue::Mixed,
            }
        }
        merge_value(&mut self.family, other.family);
        merge_value(&mut self.size, other.size);
    }

    pub fn from_spans<'a>(spans: impl IntoIterator<Item = &'a Span>) -> Self {
        let mut result = Self::default();
        for span in spans {
            if span.text.is_empty() || span.image.is_some() {
                continue;
            }
            if span.code || span.math {
                return Self::default();
            }
            fn add<T: PartialEq>(current: &mut FontValue<T>, value: Option<T>) {
                match current {
                    FontValue::Unavailable => *current = FontValue::Value(value),
                    FontValue::Value(first) if *first != value => *current = FontValue::Mixed,
                    _ => {}
                }
            }
            add(&mut result.family, span.font_family.clone());
            add(&mut result.size, span.font_size);
        }
        result
    }
}

/// Finds font attributes at a caret or a safely mapped source selection.
pub fn selection_fonts(text: &str, selection: (usize, usize)) -> Option<FontSelection> {
    if text.contains(INLINE_IMAGE_CHAR) || selection.0.max(selection.1) > text.chars().count() {
        return None;
    }
    let marker = ('\u{e000}'..='\u{f8ff}').find(|c| !text.contains(*c))?;
    let original = span_characters(&parse_inline(text))?;
    let lo = selection.0.min(selection.1);
    let hi = selection.0.max(selection.1);
    let start = source_offset(text, lo, marker, &original)?;
    if lo == hi {
        let (probe, _) = insert_at_caret(text, (lo, lo), &marker.to_string());
        let spans = parse_inline(&probe);
        let span = spans.iter().find(|span| span.text.contains(marker))?;
        return Some(FontSelection::from_spans([span]));
    }
    let end = source_offset(text, hi, marker, &original)?;
    if start >= end {
        return None;
    }
    let selected: Vec<Span> = original[start..end].iter().map(|(character, span)| Span { text: character.to_string(), ..span.clone() }).collect();
    Some(FontSelection::from_spans(&selected))
}

fn insert_span_marker(spans: &mut [Span], offset: usize, marker: char, after_boundary: bool) -> Option<()> {
    let mut at = 0;
    for span in spans {
        let len = span.text.chars().count();
        if offset < at + len || (offset == at + len && !after_boundary) {
            let byte = span.text.char_indices().nth(offset - at).map_or(span.text.len(), |(byte, _)| byte);
            span.text.insert(byte, marker);
            return Some(());
        }
        at += len;
    }
    None
}

/// The inline formatting the Format Painter picks up (issue #165): of the first selected
/// character, or of the character before the caret (after it at the start of the line).
/// Only the formatting, not the words, link or picture.
pub fn style_at(text: &str, selection: (usize, usize)) -> Option<Span> {
    if text.contains(INLINE_IMAGE_CHAR) || selection.0.max(selection.1) > text.chars().count() {
        return None;
    }
    let marker = ('\u{e000}'..='\u{f8ff}').find(|c| !text.contains(*c))?;
    let original = span_characters(&parse_inline(text))?;
    let lo = selection.0.min(selection.1);
    // A caret in the middle of markup (between the stars of `***Big***`) is at no character of
    // its own: the nearest place that is one, after it first, is.
    let len = text.chars().count();
    let at = (0..=len).find_map(|d| {
        [lo.checked_add(d).filter(|p| *p <= len), lo.checked_sub(d)].into_iter().flatten().find_map(|p| source_offset(text, p, marker, &original))
    })?;
    let from = if lo == selection.0.max(selection.1) && at > 0 { at - 1 } else { at };
    let (_, span) = original.get(from).or_else(|| original.last())?;
    Some(Span { text: String::new(), link: None, image: None, math: false, ..span.clone() })
}

/// Gives the words in `range` (visible characters) the formatting of `style`: every mark, the
/// script, the size and the font, each set to what `style` has, so what was not on it comes
/// off. Words, links and pictures stay as they are. Returns whether anything changed.
pub fn paint_span_range(spans: &mut Vec<Span>, range: std::ops::Range<usize>, style: &Span) -> bool {
    if range.start >= range.end {
        return false;
    }
    let before = spans.clone();
    let mut output = Vec::new();
    let mut at = 0usize;
    for span in std::mem::take(spans) {
        let len = if span.image.is_some() { 1 } else { span.text.chars().count() };
        let hit = at < range.end && at + len > range.start && span.image.is_none() && !span.math && !span.text.is_empty();
        if hit {
            let a = range.start.saturating_sub(at).min(len);
            let b = range.end.saturating_sub(at).min(len);
            let first = span.text.char_indices().nth(a).map_or(span.text.len(), |(i, _)| i);
            let last = span.text.char_indices().nth(b).map_or(span.text.len(), |(i, _)| i);
            for (text, change) in [(&span.text[..first], false), (&span.text[first..last], true), (&span.text[last..], false)] {
                if text.is_empty() {
                    continue;
                }
                let mut piece = Span { text: text.to_owned(), ..span.clone() };
                if change {
                    piece.bold = style.bold;
                    piece.italic = style.italic;
                    piece.strike = style.strike;
                    piece.underline = style.underline;
                    piece.script = style.script;
                    piece.code = style.code;
                    piece.font_size = style.font_size;
                    piece.font_family = style.font_family.clone();
                }
                output.push(piece);
            }
        } else {
            output.push(span);
        }
        at += len;
    }
    *spans = output;
    *spans != before
}

/// [`paint_span_range`] on a selection of a line's editable text, the way
/// [`format_source_selection`] marks one: `None` when the selection cannot be mapped to the
/// words unambiguously or the words would not survive.
pub fn paint_source_selection(text: &str, selection: (usize, usize), style: &Span) -> Option<(String, (usize, usize))> {
    if text.contains(INLINE_IMAGE_CHAR) || selection.0 == selection.1 || selection.0.max(selection.1) > text.chars().count() {
        return None;
    }
    let marker = ('\u{e000}'..='\u{f8ff}').find(|c| !text.contains(*c))?;
    let mut spans = parse_inline(text);
    let original = span_characters(&spans)?;
    let a = source_offset(text, selection.0.min(selection.1), marker, &original)?;
    let b = source_offset(text, selection.0.max(selection.1), marker, &original)?;
    if a >= b {
        return None;
    }
    if !paint_span_range(&mut spans, a..b, style) {
        return Some((text.to_string(), selection));
    }
    let wanted = span_characters(&spans)?;
    // The caret goes after the painted words, wherever the new markup puts them.
    insert_span_marker(&mut spans, b, marker, false)?;
    let marked = spans_to_editor(&spans);
    let end = marked[..marked.find(marker)?].chars().count();
    let result = marked.replace(marker, "");
    if span_characters(&parse_inline(&result))? != wanted {
        return None;
    }
    Some((result, (end, end)))
}

/// Formats a source selection only when its endpoints map unambiguously to text and the
/// resulting serialization preserves every character and its formatting.
pub fn format_source_selection(text: &str, selection: (usize, usize), format: InlineFormat) -> Option<(String, (usize, usize))> {
    if text.contains(INLINE_IMAGE_CHAR) || selection.0 == selection.1 || selection.0.max(selection.1) > text.chars().count() {
        return None;
    }
    let marker = ('\u{e000}'..='\u{f8ff}').find(|c| !text.contains(*c))?;
    let mut spans = parse_inline(text);
    let original = span_characters(&spans)?;
    let a = source_offset(text, selection.0.min(selection.1), marker, &original)?;
    let b = source_offset(text, selection.0.max(selection.1), marker, &original)?;
    if a >= b {
        return None;
    }
    if !format_span_range(&mut spans, a..b, format) {
        return Some((text.to_string(), selection));
    }
    let wanted = span_characters(&spans)?;
    let end_marker = ('\u{e000}'..='\u{f8ff}').find(|c| *c != marker && !text.contains(*c))?;
    insert_span_marker(&mut spans, b, end_marker, false)?;
    insert_span_marker(&mut spans, a, marker, true)?;
    let serialized = spans_to_editor(&spans);
    let start_byte = serialized.find(marker)?;
    let end_byte = serialized.find(end_marker)?;
    if end_byte < start_byte {
        return None;
    }
    let start = serialized[..start_byte].chars().count();
    let end = serialized[..end_byte].chars().count().saturating_sub(1);
    let result = serialized.replace([marker, end_marker], "");
    if span_characters(&parse_inline(&result))? != wanted {
        return None;
    }
    Some((result, if selection.0 <= selection.1 { (start, end) } else { (end, start) }))
}

/// Bold, italic, strikethrough or code on every line of words in `range`, as a toggle: when all of
/// those words already have the mark it comes off, otherwise it goes on all of them. Done on the
/// spans, not on the text, so words that are partly marked already come out right (`a **b** c`
/// becomes `**a b c**`, not `**a **b** c**`). Returns false when the range has no words to mark.
pub fn toggle_mark(blocks: &mut [Block], range: std::ops::Range<usize>, mark: &str) -> bool {
    use notecraft_formats::markdown::Script;
    if !matches!(mark, "bold" | "italic" | "strike" | "underline" | "subscript" | "superscript" | "code") {
        log::warn!("unknown inline mark requested: {mark}");
        return false;
    }
    let flag = |s: &mut Span| -> bool {
        match mark {
            "bold" => s.bold,
            "italic" => s.italic,
            "strike" => s.strike,
            "underline" => s.underline,
            "subscript" => s.script == Some(Script::Subscript),
            "superscript" => s.script == Some(Script::Superscript),
            "code" => s.code,
            _ => false,
        }
    };
    let set = |s: &mut Span, on: bool| match mark {
        "bold" => s.bold = on,
        "italic" => s.italic = on,
        "strike" => s.strike = on,
        "underline" => s.underline = on,
        "subscript" => s.script = on.then_some(Script::Subscript),
        "superscript" => s.script = on.then_some(Script::Superscript),
        "code" => s.code = on,
        _ => {}
    };
    let mut targets: Vec<&mut Vec<Span>> = Vec::new();
    for b in blocks.get_mut(range).unwrap_or_default() {
        if let Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } = b {
            targets.push(spans);
        }
    }
    let mut all = true;
    let mut any = false;
    for spans in targets.iter_mut() {
        for s in spans.iter_mut().filter(|s| s.image.is_none() && !s.math && !s.text.is_empty()) {
            any = true;
            all &= flag(s);
        }
    }
    if !any {
        return false;
    }
    for spans in targets {
        for s in spans.iter_mut().filter(|s| s.image.is_none() && !s.math) {
            set(s, !all);
        }
        // Neighbours that now look alike are one run, or each would be written with its own marks.
        let mut merged: Vec<Span> = Vec::new();
        for s in std::mem::take(spans) {
            match merged.last_mut() {
                Some(last)
                    if last.image.is_none()
                        && s.image.is_none()
                        && !last.math
                        && !s.math
                        && Span { text: String::new(), ..last.clone() } == Span { text: String::new(), ..s.clone() } =>
                {
                    last.text.push_str(&s.text)
                }
                _ => merged.push(s),
            }
        }
        *spans = merged;
    }
    true
}

pub fn wrap_selection(text: &str, selection: (usize, usize), marks: &str) -> (String, (usize, usize)) {
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (selection.0.min(selection.1).min(chars.len()), selection.0.max(selection.1).min(chars.len()));
    let (before, middle, after): (String, String, String) = (
        chars.get(..a).unwrap_or(&[]).iter().collect(),
        chars.get(a..b).unwrap_or(&[]).iter().collect(),
        chars.get(b..).unwrap_or(&[]).iter().collect(),
    );
    let n = marks.chars().count();
    // Already marked: take the marks off, so the same button is the way back.
    if middle.starts_with(marks) && middle.ends_with(marks) && middle.chars().count() >= n * 2 {
        let inner: String = middle.chars().skip(n).take(middle.chars().count() - 2 * n).collect();
        let len = inner.chars().count();
        return (format!("{before}{inner}{after}"), (a, a + len));
    }
    if before.ends_with(marks) && after.starts_with(marks) {
        let before: String = before.chars().take(before.chars().count() - n).collect();
        let after: String = after.chars().skip(n).collect();
        let len = middle.chars().count();
        let start = a - n;
        return (format!("{before}{middle}{after}"), (start, start + len));
    }
    let len = middle.chars().count();
    (format!("{before}{marks}{middle}{marks}{after}"), (a + n, a + n + len))
}

/// Wraps the selected characters with a pair of different delimiters and keeps them selected.
pub fn wrap_selection_with(text: &str, selection: (usize, usize), open: &str, close: &str) -> (String, (usize, usize)) {
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (selection.0.min(selection.1).min(chars.len()), selection.0.max(selection.1).min(chars.len()));
    let (before, middle, after): (String, String, String) = (
        chars.get(..a).unwrap_or(&[]).iter().collect(),
        chars.get(a..b).unwrap_or(&[]).iter().collect(),
        chars.get(b..).unwrap_or(&[]).iter().collect(),
    );
    let start = a + open.chars().count();
    let end = start + middle.chars().count();
    (format!("{before}{open}{middle}{close}{after}"), (start, end))
}

/// Puts `insert` where the caret is, replacing whatever is selected. Returns the new text
/// and where the caret ends up.
pub fn insert_at_caret(text: &str, selection: (usize, usize), insert: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (selection.0.min(selection.1).min(chars.len()), selection.0.max(selection.1).min(chars.len()));
    let before: String = chars.get(..a).unwrap_or(&[]).iter().collect();
    let after: String = chars.get(b..).unwrap_or(&[]).iter().collect();
    (format!("{before}{insert}{after}"), a + insert.chars().count())
}

/// Makes the block at `at` a heading, a list item, a quote or ordinary text, keeping its
/// words. Returns false when there is nothing to change.
pub fn set_kind(blocks: &mut [Block], at: usize, kind: &str) -> bool {
    let Some(block) = blocks.get(at) else { return false };
    if !is_one_line_of_words(block) {
        return false;
    }
    // Code stays code with its words as they are; only whether its lines are numbered changes.
    if let (Block::Code { lang, text }, "codeblock" | "codenumbers") = (block, kind)
        && block_kind(block) != kind
    {
        let language = notecraft_formats::markdown::code_lang(lang).1.to_string();
        let lang = if kind == "codenumbers" { format!("{}{language}", notecraft_formats::markdown::NUMBERED) } else { language };
        let text = text.clone();
        if let Some(b) = blocks.get_mut(at) {
            *b = Block::Code { lang, text };
        }
        return true;
    }
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans.clone(),
        _ => vec![Span { text: block_text(block), ..Default::default() }],
    };
    let new = match kind {
        "h1" => Block::Heading { level: 1, spans },
        "h2" => Block::Heading { level: 2, spans },
        "h3" => Block::Heading { level: 3, spans },
        "h4" => Block::Heading { level: 4, spans },
        "h5" => Block::Heading { level: 5, spans },
        "h6" => Block::Heading { level: 6, spans },
        "quote" => Block::Quote(spans),
        "codeblock" => Block::Code { lang: String::new(), text: spans.iter().map(|s| s.text.as_str()).collect() },
        "codenumbers" => {
            Block::Code { lang: notecraft_formats::markdown::NUMBERED.to_string(), text: spans.iter().map(|s| s.text.as_str()).collect() }
        }
        "bullet" => Block::Item { ordered: false, number: 0, task: None, depth: 0, spans },
        "number" => Block::Item { ordered: true, number: 1, task: None, depth: 0, spans },
        "todo" => Block::Item { ordered: false, number: 0, task: Some(false), depth: 0, spans },
        _ => Block::Paragraph(spans),
    };
    // Asking for what it already is turns it back into ordinary text, like a toggle.
    let same = std::mem::discriminant(&new) == std::mem::discriminant(block) && block_kind(block) == kind;
    if let Some(b) = blocks.get_mut(at) {
        *b = if same {
            Block::Paragraph(spans_of(&new).map(<[Span]>::to_vec).unwrap_or_else(|| vec![Span { text: block_text(&new), ..Default::default() }]))
        } else {
            new
        };
    }
    renumber(blocks);
    true
}

/// Puts a tag on the line the caret is in, or takes it off. To Do is a task list item;
/// every other tag is a marker word at the start of the line ([`notecraft_model::Tag`]).
/// Asking for the tag that is already there removes it, the way OneNote toggles a tag.
pub fn set_tag(blocks: &mut [Block], at: usize, tag: Tag) -> bool {
    let Some(block) = blocks.get(at) else { return false };
    if !is_writable(block) {
        return false;
    }
    if tag == Tag::ToDo {
        let already = matches!(block, Block::Item { task: Some(_), .. });
        let mut inline = InlineEdit::new(block, true);
        let text = inline.text().to_owned();
        let spans = inline.parse(&text);
        if let Some(b) = blocks.get_mut(at) {
            *b = if already { Block::Paragraph(spans) } else { Block::Item { ordered: false, number: 0, task: Some(false), depth: 0, spans } };
        }
        renumber(blocks);
        return true;
    }
    let text = block_text(block);
    let (current, _) = Tag::split(&text);
    let next = if current == Some(tag) { None } else { Some(tag) };
    let text = Tag::set(&text, next);
    if let Some(b) = blocks.get_mut(at) {
        set_block_text(b, &text);
    }
    true
}

/// Which way the paragraph in block `at` reads: a mark at its start, or `None` to let the
/// words decide again. False if there is no such block or it holds no words.
pub fn set_direction(blocks: &mut [Block], at: usize, dir: Option<notecraft_model::Dir>) -> bool {
    set_direction_over(blocks, at..=at, dir)
}

/// The same over a run of blocks, which is what selecting several paragraphs and asking for
/// a direction means: there is no direction of its own on a block or an outline, it simply
/// goes on every paragraph inside the selection. One place to store it, one rule to read it.
pub fn set_direction_over(blocks: &mut [Block], range: std::ops::RangeInclusive<usize>, dir: Option<notecraft_model::Dir>) -> bool {
    let mut done = false;
    for at in range {
        let Some(block) = blocks.get(at) else { continue };
        if !is_writable(block) {
            continue;
        }
        let text = notecraft_model::text::set_direction(&block_text(block), dir);
        if let Some(b) = blocks.get_mut(at) {
            set_block_text(b, &text);
            done = true;
        }
    }
    done
}

/// The tag on a block, if it has one, and the words without its marker.
pub fn tag_of(block: &Block) -> (Option<Tag>, String) {
    if matches!(block, Block::Item { task: Some(_), .. }) {
        return (Some(Tag::ToDo), block_text(block));
    }
    let text = block_text(block);
    let (tag, rest) = Tag::split(&text);
    (tag, rest.to_string())
}

/// Ticks or clears the to-do box on a line. Returns false when the line has no box.
pub fn set_task(blocks: &mut [Block], at: usize, done: bool) -> bool {
    match blocks.get_mut(at) {
        Some(Block::Item { task: Some(state), .. }) => {
            *state = done;
            true
        }
        _ => false,
    }
}

/// More or less indent on a list item. Returns false when the block is not one.
pub fn indent(blocks: &mut [Block], at: usize, deeper: bool) -> bool {
    let Some(Block::Item { depth, .. }) = blocks.get_mut(at) else { return false };
    *depth = if deeper { (*depth + 1).min(8) } else { depth.saturating_sub(1) };
    renumber(blocks);
    true
}

/// Removes inline formatting without interpreting literal text as Markdown source.
pub fn clear_marks(block: &mut Block) {
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans,
        _ => return,
    };
    clear_span_marks(spans);
}

fn clear_span_marks(spans: &mut Vec<Span>) {
    let mut cleared: Vec<Span> = Vec::new();
    for span in std::mem::take(spans) {
        if span.image.is_some() || span.math {
            cleared.push(span);
        } else if let Some(last) = cleared.last_mut().filter(|last| last.image.is_none() && !last.math) {
            last.text.push_str(&span.text);
        } else {
            cleared.push(Span { text: span.text, ..Default::default() });
        }
    }
    *spans = cleared;
}

/// The same words without their marks: no bold, italic, strikethrough or code.
pub fn without_marks(text: &str) -> String {
    let spans = parse_inline(text);
    spans
        .iter()
        .map(|s| match &s.image {
            Some(src) => format!("![{}]({src})", s.text),
            None if s.math => format!("${}$", s.text),
            None => s.text.clone(),
        })
        .collect()
}

/// Puts a new block of its own kind at `at`: a table or a diagram, for now.
pub fn insert_block(blocks: &mut Vec<Block>, at: usize, what: &str) -> bool {
    let cell = |text: &str| parse_inline(text);
    let block = match what {
        "table" => Block::Table {
            head: vec![cell("A"), cell("B"), cell("C")],
            rows: vec![vec![cell("1"), cell("2"), cell("3")], vec![cell(""), cell(""), cell("")]],
        },
        "diagram" => Block::Code { lang: "mermaid".into(), text: "graph TD;\n  A-->B;".into() },
        "rule" => Block::Rule,
        _ => return false,
    };
    let at = at.min(blocks.len());
    blocks.insert(at, block);
    true
}

/// Every file in the page's own folder that the page names: a picture, an attachment, a
/// drawing. Used to tell a file in use from an orphan.
///
/// Only names that stay inside the folder count. A web address, or a path that climbs out
/// with `..`, is not one of the page's files.
pub fn links_in(blocks: &[Block]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut keep = |target: &str| {
        let target = target.trim();
        let ours = !target.is_empty() && !target.contains("://") && !target.starts_with('/') && !target.split(['/', '\\']).any(|p| p == "..");
        if ours && !found.iter().any(|f| f == target) {
            found.push(target.to_string());
        }
    };
    for block in blocks {
        match block {
            Block::Image { src, .. } => keep(src),
            Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => {
                for span in spans {
                    if let Some(link) = &span.link {
                        keep(link);
                    }
                    if let Some(src) = &span.image {
                        keep(src);
                    }
                }
            }
            Block::Table { head, rows } => {
                for cell in head.iter().chain(rows.iter().flatten()) {
                    for span in cell {
                        if let Some(link) = &span.link {
                            keep(link);
                        }
                        if let Some(src) = &span.image {
                            keep(src);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// Whether a block is a heading that only repeats the page's own title.
///
/// A page carries its title in its head and the window draws it above the words. An older
/// page, or one written by hand, may also open with that title as a heading; drawing both
/// shows the page its own name twice. The file is left exactly as it is - the code view
/// shows it - and the live view draws the heading only when there is no title area to
/// carry it.
pub fn is_title_echo(blocks: &[Block], i: usize, title: &str) -> bool {
    if i != 0 || title.trim().is_empty() {
        return false;
    }
    match blocks.first() {
        Some(Block::Heading { level: 1, spans }) => {
            let words: String = spans.iter().map(|s| s.text.as_str()).collect();
            words.trim() == title.trim()
        }
        _ => false,
    }
}

/// The page's date stamp: the line under the title that says when the page was started.
///
/// It is drawn as a stamp, quiet and grey, the way OneNote draws the date under a page
/// title, rather than as an ordinary paragraph. It is still a paragraph in the file, so
/// anything can read it; this only says which one it is.
pub fn is_page_stamp(blocks: &[Block], i: usize) -> bool {
    // The first block of a page, or the second when the page opens with its own heading.
    let after_heading = i == 1 && matches!(blocks.first(), Some(Block::Heading { level: 1, .. }));
    if !(i == 0 || after_heading) {
        return false;
    }
    // The words themselves, not the marks round them: a page written before the stamp was
    // plain has its date in italics, and it is still the date.
    match blocks.get(i) {
        Some(Block::Paragraph(spans)) => {
            let words: String = spans.iter().map(|s| s.text.as_str()).collect();
            notecraft_model::time::is_long_date(&words)
        }
        _ => false,
    }
}

/// The text the caret writes in a block, with the stamp's old italics taken off.
///
/// A page written before the stamp was plain has its date wrapped in italics, and the
/// marks would be shown as marks the moment the caret landed there. They come off, and
/// the line is saved plain from then on.
pub(crate) fn text_for_caret(blocks: &[Block], i: usize) -> String {
    let Some(block) = blocks.get(i) else { return String::new() };
    if is_page_stamp(blocks, i) {
        let Block::Paragraph(spans) = block else { return caret_text(block) };
        return spans.iter().map(|s| s.text.as_str()).collect();
    }
    caret_text(block)
}

/// Puts an empty table of `rows` by `columns` at `at`, as the size picker asks for.
///
/// The first row is the heading, because a table here always has one: picking four by
/// five gives a heading and four rows under it, which is five rows of cells in all.
pub fn insert_table(blocks: &mut Vec<Block>, at: usize, rows: usize, columns: usize) -> bool {
    let (rows, columns) = (rows.clamp(1, 20), columns.clamp(1, 20));
    let row = || vec![Vec::new(); columns];
    let block = Block::Table { head: row(), rows: (1..rows).map(|_| row()).collect() };
    blocks.insert(at.min(blocks.len()), block);
    true
}

/// The name [`set_kind`] uses for what a block is now.
pub fn block_kind(block: &Block) -> &'static str {
    match block {
        Block::Heading { level: 1, .. } => "h1",
        Block::Heading { level: 2, .. } => "h2",
        Block::Heading { level: 3, .. } => "h3",
        Block::Heading { level: 4, .. } => "h4",
        Block::Heading { level: 5, .. } => "h5",
        Block::Heading { .. } => "h6",
        Block::Quote(_) => "quote",
        Block::Code { lang, .. } if notecraft_formats::markdown::code_lang(lang).0 => "codenumbers",
        Block::Code { .. } => "codeblock",
        Block::Item { task: Some(_), .. } => "todo",
        Block::Item { ordered: true, .. } => "number",
        Block::Item { .. } => "bullet",
        _ => "p",
    }
}

/// What a frame of the editor did, for the caller to act on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Changed {
    /// The blocks changed and the page needs writing back.
    pub blocks: bool,
    /// The caret moved to another block.
    pub caret: bool,
    /// A link was clicked: its target as written in the page, for the window to follow.
    pub link: Option<String>,
    /// A table column was dragged: the page's own widths need writing back.
    pub columns: bool,
    /// A keyboard navigation request to select a whole-block embedded object.
    pub select_object: Option<(usize, String)>,
    /// A formatting gesture owned by the focused page text widget, for the controller.
    pub mark: Option<&'static str>,
}

/// Consume before TextEdit handles Command+B/I/U as cursor movement or text deletion.
/// A retained page caret does not own input when search, title or another field has focus.
fn formatting_key(ui: &Ui, id: egui::Id) -> Option<&'static str> {
    if !ui.is_enabled() || !ui.memory(|memory| memory.has_focus(id)) {
        return None;
    }
    [(Key::B, "bold"), (Key::I, "italic"), (Key::U, "underline")]
        .into_iter()
        .find_map(|(key, mark)| ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, key)).then_some(mark))
}

/// The text of a block as it is edited: the inline marks only.
pub fn block_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans_to_editor(spans),
        Block::Code { text, .. } => text.clone(),
        Block::MathBlock(tex) => tex.clone(),
        Block::Image { alt, .. } => alt.clone(),
        Block::Table { .. } | Block::Rule => String::new(),
    }
}

/// The single editor character used for an inline image. It keeps an image atomic while
/// the TextEdit caret moves through the surrounding words.
const INLINE_IMAGE_CHAR: char = '\u{fffc}';

fn inline_image_arrow_target(text: &str, at: usize, right: bool) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    if right && chars.get(at) == Some(&INLINE_IMAGE_CHAR) {
        Some(at + 1)
    } else if !right && at > 0 && chars.get(at - 1) == Some(&INLINE_IMAGE_CHAR) {
        Some(at - 1)
    } else {
        None
    }
}

fn inline_image_navigation_edge(text: &str, range: egui::text::CCursorRange, right: bool) -> Option<usize> {
    let a = range.primary.index.0.min(range.secondary.index.0);
    let b = range.primary.index.0.max(range.secondary.index.0);
    if b == a + 1 && text.chars().nth(a) == Some(INLINE_IMAGE_CHAR) { Some(if right { b } else { a }) } else { None }
}

/// Clicking under the page writes at its end: in the last empty block if there is one,
/// else in a new one. The canvas of containers uses it for the room under the boxes.
pub fn click_below(blocks: &mut Vec<Block>, caret: &mut Option<Caret>) -> bool {
    let mut changed = Changed::default();
    apply(Action::ClickBelow, blocks, caret, &mut changed);
    changed.blocks
}

/// A table, with every cell clickable and the one the caret is in written in directly.
///
/// Until now a table was drawn and nothing more: the page showed it and refused to let
/// anyone in, which the owner reasonably read as the page being a viewer rather than an
/// editor. Each cell is its own little text box now, Tab walks along the row and on to
/// the next, and what is typed goes into the block as it is typed.
/// How wide a table column is drawn, whatever the page or the box it sits in is doing.
///
/// A column used to take a share of whatever room was going, so widening a container
/// stretched the table with it. OneNote does not: a table is its own size and stays that
/// size while the paper around it changes, so that is what this is.
pub(crate) const COLUMN_W: f32 = 160.0;

/// Room round the words inside a cell, inside its border lines.
const PAD_X: f32 = 7.0;
const PAD_Y: f32 = 4.0;

/// How near a border the pointer must be to take hold of it.
const GRAB: f32 = 4.0;

#[allow(clippy::too_many_arguments)]
fn table(
    ui: &mut Ui,
    block: &Block,
    i: usize,
    caret: &mut Option<Caret>,
    t: &Tokens,
    page: Option<notecraft_model::Dir>,
    borders: bool,
    cols: &mut notecraft_model::tablecols::Widths,
    changed: &mut Changed,
    zoom: f32,
) -> Option<Action> {
    let Block::Table { head, rows } = block else { return None };
    let mut action = None;
    let here = caret.as_ref().filter(|c| c.block == i).and_then(|c| c.cell);
    // Read before any cell is drawn: a text box takes Tab for itself as it is shown, so
    // asking afterwards never sees it, and Tab did nothing in a table at all.
    let (tab, shift, escape, rub) = ui.input(|i| {
        (i.key_pressed(Key::Tab), i.modifiers.shift, i.key_pressed(Key::Escape), i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
    });
    // A block of cells dragged across. While one is picked the cells are read, not
    // written in: a text box in the middle of a selection would take the keyboard and
    // the Delete that is meant for the whole block.
    let picked = caret.as_ref().filter(|c| c.block == i).and_then(cell_range);
    // Keyed off which cell the caret is in, not off whether the text box still has the
    // keyboard: egui moves focus on Tab before any widget is drawn, so by the time the
    // cell is shown it has already lost focus and would never see its own Tab.
    if picked.is_some() && rub {
        action = Some(Action::ClearCells(i));
    }
    if let Some(cell) = here {
        if tab {
            // Tab off the last cell adds a row and lands in it, the way OneNote grows a
            // table as it is filled in. Shift+Tab off the front leaves instead: going
            // back from the first cell has nowhere to go.
            action = Some(match (next_cell(block, cell, shift), shift) {
                (Some(to), _) => Action::EnterCell(i, to),
                (None, false) => Action::AddRow(i),
                (None, true) => Action::Leave,
            });
        } else if escape {
            action = Some(Action::Leave);
        }
    }
    let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
    // What each column is wide: what this page remembers of this table, or the usual
    // width for a column nobody has dragged.
    let widths: Vec<f32> = (0..columns)
        .map(|c| {
            cols.of(i)
                .and_then(|w| w.get(c).copied())
                .unwrap_or(COLUMN_W)
                .clamp(notecraft_model::tablecols::MIN_WIDTH, notecraft_model::tablecols::MAX_WIDTH)
        })
        .collect();
    let screen_widths: Vec<f32> = widths.iter().map(|w| w * zoom).collect();

    // Laid out by hand rather than with a grid, because a grid gives every column the
    // same width and these are each their own. Rows butt up against each other, and the
    // room round the words is inside the cell, so the lines can be drawn on the joins.
    let mut tops = Vec::with_capacity(rows.len() + 1);
    let left = ui.cursor().min.x;
    let bottom;
    {
        let mut ui = ui.new_child(egui::UiBuilder::new().max_rect(ui.available_rect_before_wrap()).layout(*ui.layout()));
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
        for r in 0..=rows.len() {
            tops.push(ui.cursor().min.y);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
                for (c, &w) in screen_widths.iter().enumerate() {
                    let cell = (r, c);
                    ui.allocate_ui_with_layout(vec2(w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        ui.set_min_width(w);
                        ui.set_max_width(w);
                        let pad_x = PAD_X * zoom;
                        let pad_y = PAD_Y * zoom;
                        let inner = (w - 2.0 * pad_x).max(8.0 * zoom);
                        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(pad_x as i8, pad_y as i8)).show(ui, |ui| {
                            ui.set_min_width(inner);
                            ui.set_max_width(inner);
                            // An empty cell is a line high, as a cell with words in it is, so
                            // there is something to click (issue #147).
                            ui.set_min_height(ui.fonts_mut(|f| f.row_height(&egui::FontId::proportional(14.5 * zoom))));
                            if here == Some(cell) && picked.is_none() {
                                let Some(caret) = caret.as_mut() else { return };
                                caret.ensure_inline(block);
                                let previous_selection = caret.selection;
                                let id = ui.make_persistent_id(("table-cell", i, r, c));
                                changed.mark = formatting_key(ui, id);
                                let at = (i, caret.text.clone(), caret.selection);
                                let out = steady_blink(ui, (at.0, &at.1, at.2), |ui| {
                                    egui::TextEdit::singleline(&mut caret.text)
                                        .id(id)
                                        .frame(egui::Frame::NONE)
                                        .desired_width(inner)
                                        .font(egui::FontId::proportional(14.5 * zoom))
                                        .show(ui)
                                });
                                if caret.focus {
                                    out.response.request_focus();
                                    caret.focus = false;
                                }
                                if let Some(range) = out.cursor_range {
                                    caret.selection = (range.primary.index.0, range.secondary.index.0);
                                }
                                if out.response.changed() {
                                    if let Some(inline) = caret.inline.as_mut() {
                                        inline.update_at(&caret.text, previous_selection, caret.selection);
                                    }
                                    action = Some(Action::SetCell(i, cell, caret.text.clone()));
                                }
                            } else {
                                let strong = r == 0;
                                let colour = if strong { t.strong } else { ui.style().visuals.text_color() };
                                let spans = cell_spans(block, cell).unwrap_or_default();
                                let job = preview::job(ui, spans, 14.5 * zoom, colour, t, page);
                                let resp = ui.add(egui::Label::new(job).sense(Sense::click()).selectable(false).wrap());
                                if resp.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
                                }
                                if resp.clicked() {
                                    action = Some(Action::EnterCell(i, cell));
                                }
                            }
                        });
                    });
                }
            });
        }
        bottom = ui.cursor().min.y;
        let used = egui::Rect::from_min_max(egui::pos2(left, tops[0]), egui::pos2(left + screen_widths.iter().sum::<f32>(), bottom));
        ui.advance_cursor_after_rect(used);
    }
    ui.advance_cursor_after_rect(egui::Rect::from_min_max(egui::pos2(left, tops[0]), egui::pos2(left + screen_widths.iter().sum::<f32>(), bottom)));

    // The lines of the table, which are also where a cell begins and ends.
    let edge_x: Vec<f32> = (0..=columns).map(|c| left + screen_widths.iter().take(c).sum::<f32>()).collect();
    let mut edge_y: Vec<f32> = tops.clone();
    edge_y.push(bottom);
    let cell_rect = |r: usize, c: usize| match (edge_x.get(c), edge_x.get(c + 1), edge_y.get(r), edge_y.get(r + 1)) {
        (Some(x0), Some(x1), Some(y0), Some(y1)) => Some(egui::Rect::from_min_max(egui::pos2(*x0, *y0), egui::pos2(*x1, *y1))),
        _ => None,
    };
    // Every cell, as a rectangle that touches its neighbours: a pointer anywhere in the
    // table is in exactly one cell, borders and gaps included.
    let mut where_ = Vec::with_capacity((rows.len() + 1) * columns);
    for r in 0..=rows.len() {
        for c in 0..columns {
            if let Some(rect) = cell_rect(r, c) {
                where_.push(((r, c), rect));
            }
        }
    }

    // A drag from one cell to another picks the block between them. The cell the drag
    // started in is remembered across frames, because the pointer may be over any cell by
    // the time it is let go, and a press on its own is still only a click.
    let anchor_id = ui.make_persistent_id(("table-drag", i));
    let on_border =
        ui.ctx().pointer_latest_pos().is_some_and(|p| edge_x.iter().skip(1).any(|x| (p.x - x).abs() <= GRAB) && p.y >= edge_y[0] && p.y <= bottom);
    let (pressed, down, at) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.interact_pos()));
    let over = at.and_then(|p| where_.iter().find(|(_, r)| r.contains(p)).map(|(c, _)| *c));
    if pressed && !on_border {
        ui.memory_mut(|m| match over {
            Some(cell) => {
                m.data.insert_temp(anchor_id, cell);
            }
            None => {
                m.data.remove_temp::<(usize, usize)>(anchor_id);
            }
        });
    }
    if down
        && let Some(from) = ui.memory(|m| m.data.get_temp::<(usize, usize)>(anchor_id))
        && let Some(to) = over
        && to != from
    {
        action = Some(Action::Select(i, from, to));
    }

    // The picked block is washed over in the accent colour, the way a selection is shown
    // anywhere else, and the words stay readable underneath.
    if let Some((top_left, bottom_right)) = picked {
        // One rectangle over the lot, not one per cell: washes that overlap darken where
        // they meet, and the seams make a block of cells look like separate bands.
        let mut whole: Option<egui::Rect> = None;
        for (cell, rect) in &where_ {
            let inside = (top_left.0..=bottom_right.0).contains(&cell.0) && (top_left.1..=bottom_right.1).contains(&cell.1);
            if inside {
                whole = Some(match whole {
                    Some(w) => w.union(*rect),
                    None => *rect,
                });
            }
        }
        if let Some(whole) = whole {
            ui.painter().rect_filled(whole, 0.0, t.accent.gamma_multiply(0.22));
        }
    }

    // The borders themselves, which a table has unless they are turned off: OneNote draws
    // a plain grey line round every cell, and a table with none reads as loose columns of
    // words rather than as a table.
    if borders && let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (edge_x.first(), edge_x.last(), edge_y.first(), edge_y.last()) {
        let line = egui::Stroke::new(1.0 * zoom, t.table_line);
        let paint = ui.painter();
        for x in &edge_x {
            paint.vline(*x, y0..=y1, line);
        }
        for y in &edge_y {
            paint.hline(x0..=x1, *y, line);
        }
    }

    // The borders are handles as well as lines: a drag on one changes the width of the
    // column to its left, the way a column is resized in OneNote or Word. Added after
    // the cells so the few pixels round a line answer to the drag and not to the words.
    let mut on_handle = false;
    for c in 0..columns {
        let Some(&x) = edge_x.get(c + 1) else { continue };
        let handle = egui::Rect::from_min_max(egui::pos2(x - GRAB * zoom, edge_y[0]), egui::pos2(x + GRAB * zoom, bottom));
        let grip = ui.interact(handle, ui.make_persistent_id(("table-column", i, c)), Sense::drag());
        if grip.hovered() || grip.dragged() {
            on_handle = true;
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if grip.dragged() {
            let want = widths[c] + grip.drag_delta().x / zoom;
            cols.set(i, c, want, &widths);
            changed.columns = true;
        }
        // While a column is being dragged its line is drawn in the accent colour, so it
        // is plain which one is moving.
        if grip.dragged() {
            ui.painter().vline(x, edge_y[0]..=bottom, egui::Stroke::new(1.5 * zoom, t.accent));
        }
    }
    if on_handle {
        // A press on a border is a resize, not the start of a block of cells.
        ui.memory_mut(|m| m.data.remove_temp::<(usize, usize)>(anchor_id));
    }

    // A click anywhere in a cell goes into it, not only a click on its words: an empty cell
    // has no words to click, and the space after a short word is most of the cell (issue
    // #147). Not on a border, which is a resize; not in the cell already being written in,
    // whose text box places the caret itself; and not when something is drawn over the table.
    let clicked = ui.input(|i| i.pointer.primary_clicked());
    if action.is_none()
        && clicked
        && !on_border
        && !on_handle
        && let (Some(p), Some(cell)) = (at, over)
        && ui.clip_rect().contains(p)
        && ui.ctx().layer_id_at(p) == Some(ui.layer_id())
        && (here != Some(cell) || picked.is_some())
    {
        action = Some(Action::EnterCell(i, cell));
    }

    ui.add_space(6.0);
    action
}

/// egui blinks a text cursor from the last time the field itself was clicked or typed in. A field
/// NoteCraft puts the caret in (a click on open paper starting a box, an arrow from the line
/// above) has no such time, so its cursor could stay dark for up to half a blink after the click,
/// and the click seemed slow to take. The blink is kept here instead, from when the caret came to
/// where it is (`at`: its block, words and selection) or last changed there.
fn steady_blink<R>(ui: &mut Ui, at: (usize, &str, (usize, usize)), show: impl FnOnce(&mut Ui) -> R) -> R {
    use std::hash::{Hash, Hasher};
    let cursor = ui.visuals().text_cursor.clone();
    if !cursor.blink {
        return show(ui);
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    at.hash(&mut hasher);
    let key = hasher.finish();
    let id = egui::Id::new("caret-blink");
    let now = ui.input(|i| i.time);
    let since = match ui.data(|d| d.get_temp::<(u64, f64)>(id)) {
        Some((k, from)) if k == key => now - from,
        _ => {
            ui.data_mut(|d| d.insert_temp(id, (key, now)));
            0.0
        }
    };
    let (lit, wake) = blink_phase(since, cursor.on_duration, cursor.off_duration);
    {
        let own = &mut ui.visuals_mut().text_cursor;
        own.blink = false;
        if !lit {
            own.stroke.color = egui::Color32::TRANSPARENT;
        }
    }
    ui.ctx().request_repaint_after_secs(wake);
    let out = show(ui);
    ui.visuals_mut().text_cursor = cursor;
    out
}

/// Whether a cursor placed `since` seconds ago is lit, blinking `on` seconds lit and `off` dark,
/// and how long until that changes.
fn blink_phase(since: f64, on: f32, off: f32) -> (bool, f32) {
    let total = (on + off).max(0.01);
    let t = (since.max(0.0) % f64::from(total)) as f32;
    if t < on { (true, on - t) } else { (false, total - t) }
}

/// A light frame around the block the caret is in, the way a container is outlined in
/// OneNote, so it is plain which block is being written in and where it ends. Blocks that
/// are only being read get nothing, so a page at rest stays quiet.
fn frame_block(ui: &Ui, top: f32, editing: bool, t: &Tokens, zoom: f32) {
    if !editing {
        return;
    }
    let bottom = ui.cursor().min.y;
    let rect = egui::Rect::from_min_max(
        egui::pos2(ui.max_rect().min.x - 6.0 * zoom, top - 3.0 * zoom),
        egui::pos2(ui.max_rect().max.x + 6.0 * zoom, bottom + 1.0 * zoom),
    );
    ui.painter().rect_stroke(rect, 2.0 * zoom, egui::Stroke::new(1.0 * zoom, t.faint), egui::StrokeKind::Inside);
}

/// The text the caret writes in: the words of the block without the tag marker, because
/// the marker is drawn as a mark beside the line and showing it as words again the moment
/// the caret arrives makes the line jump.
pub fn caret_text(block: &Block) -> String {
    InlineEdit::new(block, true).text().to_owned()
}

/// Puts the caret's text back into a block, keeping the tag the block already carries.
pub fn set_caret_text(block: &mut Block, text: &str) {
    InlineEdit::new(block, true).write(block, text);
}

/// Puts edited text back into a block, keeping what the block is.
pub fn set_block_text(block: &mut Block, text: &str) {
    let spans = InlineEdit::new(block, false).parse(text);
    match block {
        Block::Heading { spans: s, .. } | Block::Paragraph(s) | Block::Quote(s) | Block::Item { spans: s, .. } => *s = spans,
        Block::Code { text: t, .. } => *t = text.to_string(),
        Block::MathBlock(tex) => *tex = text.to_string(),
        Block::Image { alt, .. } => *alt = text.to_string(),
        Block::Table { .. } | Block::Rule => {}
    }
}

/// True for a block the caret can sit in. A table, a rule and a picture are moved and
/// deleted as a whole for now; writing in them comes with the next step (see gaps.md).
pub fn is_writable(block: &Block) -> bool {
    !matches!(block, Block::Rule | Block::Image { .. })
}

fn contains_inline_image(block: &Block) -> bool {
    matches!(
        block,
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. }
            if spans.iter().any(|span| span.image.is_some())
    )
}

/// Finds the next block that can own the caret, skipping whole-block objects such as images.
/// A caret must never be left on a block that has no editor to render it.
fn adjacent_writable(blocks: &[Block], from: usize, down: bool, shown_title: &str) -> Option<usize> {
    let mut at = from;
    loop {
        at = if down { at.checked_add(1)? } else { at.checked_sub(1)? };
        let block = blocks.get(at)?;
        if !is_title_echo(blocks, at, shown_title) && is_writable(block) {
            return Some(at);
        }
    }
}

pub(crate) fn previous_writable_block(blocks: &[Block], from: usize, shown_title: &str) -> Option<usize> {
    adjacent_writable(blocks, from, false, shown_title)
}

pub(crate) fn next_writable_block(blocks: &[Block], from: usize, shown_title: &str) -> Option<usize> {
    adjacent_writable(blocks, from, true, shown_title)
}

fn image_boundary_id() -> egui::Id {
    egui::Id::new("caret-before-whole-block-image")
}

fn next_whole_block_image(blocks: &[Block], from: usize, shown_title: &str) -> Option<usize> {
    let image = from.checked_add(1)?;
    (blocks.get(image).is_some_and(|block| matches!(block, Block::Image { .. })) && !is_title_echo(blocks, image, shown_title)).then_some(image)
}

fn previous_whole_block_image(blocks: &[Block], from: usize, shown_title: &str) -> Option<usize> {
    let image = from.checked_sub(1)?;
    (blocks.get(image).is_some_and(|block| matches!(block, Block::Image { .. })) && !is_title_echo(blocks, image, shown_title)).then_some(image)
}

/// True for a block that is one run of words and can therefore be turned into another
/// kind of one: a heading into a quote, a paragraph into a list item.
///
/// Not the same question as whether the caret goes in it. A table takes the caret now,
/// cell by cell, but asking for "Heading 1" while one is selected must not turn the whole
/// table into a heading and lose it - which is what `set_kind` would have done for as
/// long as it asked `is_writable` instead.
pub fn is_one_line_of_words(block: &Block) -> bool {
    matches!(block, Block::Heading { .. } | Block::Paragraph(_) | Block::Quote(_) | Block::Item { .. } | Block::Code { .. } | Block::MathBlock(_))
}

/// The words of one cell, as they are written: (0, c) is a heading cell.
fn cell_spans(block: &Block, (row, column): (usize, usize)) -> Option<&[Span]> {
    let Block::Table { head, rows } = block else { return None };
    if row == 0 { head.get(column).map(Vec::as_slice) } else { rows.get(row - 1)?.get(column).map(Vec::as_slice) }
}

fn cell_spans_mut(block: &mut Block, (row, column): (usize, usize)) -> Option<&mut Vec<Span>> {
    let Block::Table { head, rows } = block else { return None };
    if row == 0 { head.get_mut(column) } else { rows.get_mut(row - 1)?.get_mut(column) }
}

pub fn cell_text(block: &Block, cell: (usize, usize)) -> Option<String> {
    Some(spans_to_editor(cell_spans(block, cell)?))
}

pub fn set_cell_text(block: &mut Block, cell: (usize, usize), text: &str) {
    if let Some(spans) = cell_spans_mut(block, cell) {
        *spans = InlineEdit::from_spans(spans).parse(text);
    }
}

/// A link written `[text](address)` in a line's editable text that the caret or selection
/// (`lo..hi`, in characters) is in: where it starts and ends, in characters, its text and its
/// address. A picture (`![...](...)`) is not a link. For the Link dialog (issue #163).
pub fn link_at(text: &str, lo: usize, hi: usize) -> Option<(usize, usize, String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '[' && (i == 0 || chars[i - 1] != '!') {
            let close = (i + 1..chars.len()).find(|&j| chars[j] == ']' || chars[j] == '[');
            if let Some(close) = close.filter(|&j| chars[j] == ']')
                && chars.get(close + 1) == Some(&'(')
                && let Some(end) = (close + 2..chars.len()).find(|&j| chars[j] == ')')
            {
                let end = end + 1;
                if i <= lo && hi <= end {
                    let words: String = chars[i + 1..close].iter().collect();
                    let address: String = chars[close + 2..end - 1].iter().collect();
                    return Some((i, end, words, address));
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// `[text](address)` as the line's editable text writes a link. Brackets in the words would
/// end the link early, and a space or bracket in the address would end the address, so those
/// are left out of the words and escaped in the address. Words left empty show the address.
pub fn link_markup(text: &str, address: &str) -> String {
    let address = address.trim();
    let words: String = text.trim().chars().filter(|c| !matches!(c, '[' | ']')).collect();
    let words = if words.is_empty() { address.to_string() } else { words };
    let address = address.replace(' ', "%20").replace('(', "%28").replace(')', "%29");
    format!("[{words}]({address})")
}

/// Whether an address may be linked to: something to go to, not a script to run.
pub fn linkable_address(address: &str) -> bool {
    let address = address.trim();
    if address.is_empty() || address.chars().any(char::is_control) {
        return false;
    }
    let scheme = address.split_once(':').map(|(s, _)| s.to_ascii_lowercase());
    !matches!(scheme.as_deref(), Some("javascript" | "vbscript" | "data"))
}

/// Sorts a table's rows by one column, as OneNote's Table > Sort does (issue #171). Whole rows
/// move, so every cell stays with its row and keeps its formatting. The heading row is never
/// sorted: it stays on top. `rows` limits the sort to the body rows from one table row to
/// another (table rows count the heading as 0), for a picked block of rows; `None` sorts them
/// all.
///
/// Cells are compared by their words. Two cells that both read as numbers (`1,200`, `-3.5`,
/// `$40`, `12%`) compare as numbers; a number sorts before words; words compare without
/// regard to case. Empty cells go last whichever way the sort runs, and rows that compare
/// equal keep their order. Returns whether any row moved.
pub fn sort_table(block: &mut Block, column: usize, rows: Option<(usize, usize)>, descending: bool) -> bool {
    let Block::Table { rows: body, .. } = block else { return false };
    if body.is_empty() {
        return false;
    }
    // The body rows to sort, as indexes into `body`.
    let (first, last) = match rows {
        Some((a, b)) => (a.min(b).max(1) - 1, a.max(b).max(1) - 1),
        None => (0, body.len() - 1),
    };
    let last = last.min(body.len() - 1);
    if first >= last {
        return false;
    }
    let key = |row: &Vec<Vec<Span>>| -> SortKey {
        let words: String = row.get(column).map(|spans| spans.iter().map(|s| s.text.as_str()).collect()).unwrap_or_default();
        SortKey::of(words.trim())
    };
    let before: Vec<Vec<Vec<Span>>> = body[first..=last].to_vec();
    let mut keyed: Vec<(SortKey, Vec<Vec<Span>>)> = before.iter().map(|row| (key(row), row.clone())).collect();
    keyed.sort_by(|(a, _), (b, _)| a.compare(b, descending));
    let sorted: Vec<Vec<Vec<Span>>> = keyed.into_iter().map(|(_, row)| row).collect();
    if sorted == before {
        return false;
    }
    body.splice(first..=last, sorted);
    true
}

/// What a cell is sorted by.
#[derive(Debug, Clone, PartialEq)]
enum SortKey {
    Number(f64),
    Words(String),
    Empty,
}

impl SortKey {
    fn of(words: &str) -> SortKey {
        if words.is_empty() {
            return SortKey::Empty;
        }
        // A number with what usually comes with one: a sign, a currency sign, thousands commas,
        // a percent sign after it.
        let bare: String = words.trim_start_matches(['$', '€', '£', '¥']).trim_end_matches('%').chars().filter(|c| *c != ',').collect();
        match bare.parse::<f64>() {
            Ok(n) if n.is_finite() => SortKey::Number(n),
            _ => SortKey::Words(words.to_lowercase()),
        }
    }

    /// The order of two keys. Empty cells are last both ways; the rest turn round for a
    /// descending sort.
    fn compare(&self, other: &SortKey, descending: bool) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let rank = |k: &SortKey| match k {
            SortKey::Number(_) => 0,
            SortKey::Words(_) => 1,
            SortKey::Empty => 2,
        };
        let order = match (self, other) {
            (SortKey::Empty, SortKey::Empty) => return Ordering::Equal,
            (SortKey::Empty, _) => return Ordering::Greater,
            (_, SortKey::Empty) => return Ordering::Less,
            (SortKey::Number(a), SortKey::Number(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (SortKey::Words(a), SortKey::Words(b)) => a.cmp(b),
            (a, b) => rank(a).cmp(&rank(b)),
        };
        if descending { order.reverse() } else { order }
    }
}

/// How many cells a table has, as (rows including the heading, columns).
pub fn table_size(block: &Block) -> Option<(usize, usize)> {
    let Block::Table { head, rows } = block else { return None };
    let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    Some((rows.len() + 1, columns))
}

/// The cell after this one, reading along each row and on to the next. `None` at the end
/// of the table, which is where Tab stops.
pub fn next_cell(block: &Block, cell: (usize, usize), back: bool) -> Option<(usize, usize)> {
    let (rows, columns) = table_size(block)?;
    if rows == 0 || columns == 0 {
        return None;
    }
    let at = cell.0 * columns + cell.1;
    let to = if back { at.checked_sub(1)? } else { at + 1 };
    if to >= rows * columns {
        return None;
    }
    Some((to / columns, to % columns))
}

/// What the Table tab asks for, against the cell the caret is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEdit {
    /// A row above the caret's row, or below it.
    InsertRow(bool),
    /// A column to the left of the caret's column, or to the right.
    InsertColumn(bool),
    /// The caret's row. The heading cannot go: a table without one is not one.
    DeleteRow,
    /// The caret's column. The last column cannot go.
    DeleteColumn,
}

/// Changes a table's shape. Returns where the caret should be afterwards, or `None` when
/// nothing could be done - which is how the rules say no: the heading row stays, and a
/// table keeps at least one column.
pub fn edit_table(block: &mut Block, cell: (usize, usize), what: TableEdit) -> Option<(usize, usize)> {
    let Block::Table { head, rows } = block else { return None };
    let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0)).max(1);
    // Every row the same width, so an edit cannot leave a ragged table behind.
    head.resize(columns, Vec::new());
    for row in rows.iter_mut() {
        row.resize(columns, Vec::new());
    }
    let (r, c) = cell;
    match what {
        TableEdit::InsertRow(above) => {
            // Above the heading means directly under it: the heading is the table's top.
            let at = if above { r.max(1) - 1 } else { r };
            let at = at.min(rows.len());
            rows.insert(at, vec![Vec::new(); columns]);
            Some((at + 1, c))
        }
        TableEdit::InsertColumn(left) => {
            let at = (if left { c } else { c + 1 }).min(columns);
            head.insert(at, Vec::new());
            for row in rows.iter_mut() {
                row.insert(at, Vec::new());
            }
            Some((r, at))
        }
        TableEdit::DeleteRow => {
            if r == 0 || rows.is_empty() {
                return None;
            }
            rows.remove(r - 1);
            Some((r.min(rows.len()), c))
        }
        TableEdit::DeleteColumn => {
            if columns <= 1 {
                return None;
            }
            let at = c.min(columns - 1);
            head.remove(at);
            for row in rows.iter_mut() {
                row.remove(at);
            }
            Some((r, at.min(columns - 2)))
        }
    }
}

/// The type size of a block, so the text box matches the finished look.
fn size_of(block: &Block) -> (f32, bool) {
    match block {
        Block::Heading { level, .. } => (
            match level {
                1 => 28.0,
                2 => 22.0,
                3 => 18.0,
                _ => 16.0,
            },
            true,
        ),
        // As big as the finished block draws it (preview.rs), or the text jumps when the caret arrives.
        Block::Code { .. } | Block::MathBlock(_) => (14.5, false),
        _ => (14.5, false),
    }
}

/// A new, empty block to follow `after`: another list item in a list, a paragraph elsewhere.
pub fn next_block(after: &Block) -> Block {
    match after {
        Block::Item { ordered, number, task, depth, .. } => {
            Block::Item { ordered: *ordered, number: *number + 1, task: task.map(|_| false), depth: *depth, spans: Vec::new() }
        }
        _ => Block::Paragraph(Vec::new()),
    }
}

/// Renumbers ordered list items so a new item in the middle does not repeat a number.
pub fn renumber(blocks: &mut [Block]) {
    let mut counters: Vec<usize> = Vec::new();
    for block in blocks.iter_mut() {
        match block {
            Block::Item { ordered, number, depth, .. } => {
                counters.resize(depth.saturating_add(1), 0);
                if let Some(c) = counters.get_mut(*depth) {
                    *c += 1;
                    if *ordered {
                        *number = *c;
                    }
                }
                counters.truncate(*depth + 1);
            }
            _ => counters.clear(),
        }
    }
}

/// Draws the page and lets the caret work in it. Returns what changed.
pub fn show(
    ui: &mut Ui,
    blocks: &mut Vec<Block>,
    caret: &mut Option<Caret>,
    page_dir: &std::path::Path,
    t: &Tokens,
    page: Option<notecraft_model::Dir>,
    borders: bool,
    cols: &mut notecraft_model::tablecols::Widths,
    justified: &notecraft_model::justify::Justified,
    across: &mut Option<Across>,
    shown_title: &str,
) -> Changed {
    let all = 0..blocks.len();
    let mut changed = show_blocks(ui, blocks, caret, page_dir, t, page, all, borders, cols, justified, across, shown_title, 1.0);
    // Room under the page: a click there writes at the end, the way an empty page does.
    let (_rect, below) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0_f32.min(ui.available_height().max(40.0))), Sense::click());
    if below.clicked() {
        apply(Action::ClickBelow, blocks, caret, &mut changed);
    }
    if below.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    changed
}

/// The blocks in `range`, drawn and written in. This is the body of a page, and of one
/// container on a page that has them.
#[allow(clippy::too_many_arguments)]
pub fn show_blocks(
    ui: &mut Ui,
    blocks: &mut Vec<Block>,
    caret: &mut Option<Caret>,
    page_dir: &std::path::Path,
    t: &Tokens,
    page: Option<notecraft_model::Dir>,
    range: std::ops::Range<usize>,
    borders: bool,
    cols: &mut notecraft_model::tablecols::Widths,
    // Which blocks are stretched to reach the column; see `notecraft_model::justify`.
    justified: &notecraft_model::justify::Justified,
    // A selection that runs across more than one block, grown here as the drag moves.
    across: &mut Option<Across>,
    // The title the window is already showing, so the body does not repeat it.
    shown_title: &str,
    zoom: f32,
) -> Changed {
    let mut changed = Changed::default();
    let mut action: Option<Action> = None;
    let count = blocks.len();
    // A table copied from OneNote, Word or Excel arrives as plain text with tabs between
    // cells. Caught here, before the text box puts it all on one line.
    if let Some(at) = caret.as_ref().map(|c| c.block) {
        let pastes: Vec<String> = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Paste(text) => Some(text.clone()),
                    _ => None,
                })
                .collect()
        });
        if let Some(text) = pastes.first() {
            let blocks_in = blocks_to_paste(text);
            if !blocks_in.is_empty() {
                // Take it out of the queue, so the text box does not paste it as well.
                ui.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                action = Some(Action::Paste(at, blocks_in));
            }
        }
    }
    // A selection across blocks is nobody's text box, so the keys that act on a selection
    // are answered here. Delete and Backspace both take it out, as they do everywhere.
    if let Some(open) = across.filter(|a| !a.inside_one_block())
        && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
    {
        let (block, at) = delete_span(blocks, open);
        let text = blocks.get(block).map(caret_text).unwrap_or_default();
        *caret = Some(Caret { block, text, focus: true, selection: (at, at), place: Some(at), cell: None, span: None, inline: None });
        *across = None;
        changed.blocks = true;
        changed.caret = true;
        return changed;
    }

    for i in range.start..range.end.min(count) {
        let Some(block) = blocks.get(i).cloned() else { continue };
        // The title area above the page is already showing this; see `is_title_echo`.
        if is_title_echo(blocks, i, shown_title) {
            continue;
        }
        // While a selection runs across blocks the page is all finished words: a text box
        // would own a selection of its own and fight with this one.
        let spanning = across.is_some_and(|a| !a.inside_one_block());
        let editing = !spanning && caret.as_ref().is_some_and(|c| c.block == i);
        // Where this block starts, so a frame can be drawn around it afterwards: the block
        // the caret is in gets one, the way a container is outlined in OneNote.
        let top = ui.cursor().min.y;
        // Except the one blank line of a box just clicked into: there the cursor alone shows
        // where the words will go, until the first is written.
        let outlined = !(range.len() == 1 && crate::preview::is_blank(&block) && caret.as_ref().is_some_and(|c| c.block == i && c.text.is_empty()));
        if matches!(block, Block::Table { .. }) {
            if let Some(what) = table(ui, &block, i, caret, t, page, borders, cols, &mut changed, zoom) {
                action = Some(what);
            }
            frame_block(ui, top, editing, t, zoom);
            continue;
        }
        if editing && is_writable(&block) {
            let (size, strong) = size_of(&block);
            let size = size * zoom;
            let Some(c) = caret.as_mut() else { continue };
            c.ensure_inline(&block);
            let previous_selection = c.selection;
            let inline = c.inline.clone().unwrap_or_default();
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, width: f32| {
                let job = inline.layout(text.as_str(), |text| pagetext::inline_job_with_fonts(ui.ctx(), text, size, strong, t, width, page));
                ui.fonts_mut(|f| f.layout_job(job))
            };
            let mut at_start = false;
            let mut selection = None;
            let mut ticked = None;
            let mut input_caret = None;
            let id = ui.make_persistent_id(("page-block", i));
            changed.mark = formatting_key(ui, id);
            // A line that reads right to left is written from the right: its bullet, box or
            // tag mark sits on the right, and the words hang from that edge.
            let align = if notecraft_model::direction_in(&c.text, page).is_rtl() { egui::Align::RIGHT } else { egui::Align::LEFT };
            let row = if align == egui::Align::RIGHT {
                egui::Layout::right_to_left(egui::Align::Min)
            } else {
                egui::Layout::left_to_right(egui::Align::Min)
            };
            // A line with right-to-left text in it is written in by NoteCraft's own caret;
            // everything else stays with egui's text box, which does it well.
            let ours = needs_our_caret(&c.text) && !matches!(block, Block::Code { .. } | Block::MathBlock(_));
            if ours {
                let mut wrote = None;
                let resp = ui
                    .with_layout(row, |ui| {
                        if let Some(done) = lead(ui, &block, t) {
                            ticked = Some(done);
                        }
                        let w = write_rtl(ui, c, size, t, id, page);
                        let resp = w.response.clone();
                        wrote = Some(w);
                        resp
                    })
                    .inner;
                let (wrote_changed, at_start) = wrote.map(|w| (w.changed, w.at_start)).unwrap_or((false, false));
                if let Some(done) = ticked {
                    action = Some(Action::SetTask(i, done));
                }
                if wrote_changed {
                    let typed = c.text.clone();
                    if let Some(b) = blocks.get_mut(i) {
                        let selection = c.selection;
                        c.ensure_inline(b).update_at(&typed, previous_selection, selection);
                        c.write(b);
                    }
                    changed.blocks = true;
                }
                let (enter, backspace, tab, shift, escape, up, down) = ui.input(|i| {
                    (
                        i.key_pressed(Key::Enter),
                        i.key_pressed(Key::Backspace),
                        i.key_pressed(Key::Tab),
                        i.modifiers.shift,
                        i.key_pressed(Key::Escape),
                        i.key_pressed(Key::ArrowUp),
                        i.key_pressed(Key::ArrowDown),
                    )
                });
                if resp.has_focus() || resp.lost_focus() {
                    if enter {
                        action = Some(Action::Split(i));
                    } else if backspace && at_start && !wrote_changed {
                        action = Some(Action::JoinBack(i));
                    } else if tab {
                        action = Some(Action::Indent(i, !shift));
                    } else if escape {
                        action = Some(Action::Leave);
                    } else if up {
                        if let Some(image) = previous_whole_block_image(blocks, i, shown_title) {
                            changed.select_object = Some((image, "image".to_string()));
                        } else {
                            action = adjacent_writable(blocks, i, false, shown_title).map(Action::Move);
                        }
                    } else if down {
                        if let Some(image) = next_whole_block_image(blocks, i, shown_title) {
                            changed.select_object = Some((image, "image".to_string()));
                        } else {
                            action = adjacent_writable(blocks, i, true, shown_title).map(Action::Move);
                        }
                    }
                }
                frame_block(ui, top, outlined, t, zoom);
                if let Some(a) = action.take() {
                    apply_keeping_widths(a, blocks, caret, &mut changed, cols);
                }
                continue;
            }
            // A diagram being edited stays drawn above its source, and follows what is typed.
            if matches!(&block, Block::Code { lang, .. } if lang == "mermaid") {
                egui::Frame::new().fill(t.editor).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::diagramview::show_zoomed(ui, &c.text, t, zoom);
                });
                ui.add_space(4.0);
            }
            let resp = ui
                .with_layout(row, |ui| {
                    if let Some(done) = lead(ui, &block, t) {
                        ticked = Some(done);
                    }
                    let multiline = matches!(block, Block::Code { .. } | Block::MathBlock(_));
                    let out = if multiline {
                        // The same box the finished block is drawn in (preview.rs), so the block does not
                        // change its look when the caret arrives in it.
                        egui::Frame::new()
                            .fill(t.code_bg)
                            .corner_radius(6.0)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                if let Block::Code { lang, .. } = &block
                                    && !notecraft_formats::markdown::code_lang(lang).1.is_empty()
                                {
                                    ui.label(egui::RichText::new(notecraft_formats::markdown::code_lang(lang).1).size(12.0 * zoom).color(t.muted));
                                }
                                let at = (i, c.text.clone(), c.selection);
                                steady_blink(ui, (at.0, &at.1, at.2), |ui| {
                                    egui::TextEdit::multiline(&mut c.text)
                                        .id(id)
                                        .frame(egui::Frame::NONE)
                                        .desired_width(ui.available_width())
                                        .horizontal_align(align)
                                        .font(egui::FontId::monospace(size))
                                        .show(ui)
                                })
                            })
                            .inner
                    } else {
                        if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id)
                            && let Some(range) = state.cursor.char_range()
                        {
                            let at = range.primary.index.0;
                            let arrow = ui.input(|input| {
                                input.events.iter().find_map(|event| match event {
                                    egui::Event::Key { key: Key::ArrowLeft, pressed: true, modifiers, .. }
                                        if !modifiers.shift && !modifiers.command && !modifiers.alt =>
                                    {
                                        Some(false)
                                    }
                                    egui::Event::Key { key: Key::ArrowRight, pressed: true, modifiers, .. }
                                        if !modifiers.shift && !modifiers.command && !modifiers.alt =>
                                    {
                                        Some(true)
                                    }
                                    _ => None,
                                })
                            });
                            if let Some(right) = arrow
                                && let Some(target) = inline_image_arrow_target(&c.text, at, right)
                            {
                                log::debug!(
                                    "inline-image caret navigation: block={} direction={} before={} target={} text_chars={} selection=({}, {}) focus={}",
                                    i,
                                    if right { "right" } else { "left" },
                                    at,
                                    target,
                                    c.text.chars().count(),
                                    range.primary.index.0,
                                    range.secondary.index.0,
                                    true,
                                );
                                state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(target))));
                                state.store(ui.ctx(), id);
                                ui.input_mut(|input| {
                                    input.events.retain(|event| {
                                        !matches!(
                                            event,
                                            egui::Event::Key {
                                                key: Key::ArrowLeft | Key::ArrowRight,
                                                pressed: true,
                                                modifiers,
                                                ..
                                            } if !modifiers.shift && !modifiers.command && !modifiers.alt
                                        )
                                    });
                                });
                            }
                        }
                        let at = (i, c.text.clone(), c.selection);
                        steady_blink(ui, (at.0, &at.1, at.2), |ui| {
                            egui::TextEdit::singleline(&mut c.text)
                                .id(id)
                                .frame(egui::Frame::NONE)
                                .desired_width(ui.available_width())
                                .horizontal_align(align)
                                .layouter(&mut layouter)
                                .show(ui)
                        })
                    };
                    let mut cursor_range = out.cursor_range;
                    let (left, right, shift) = ui.input(|input| {
                        let mut left = false;
                        let mut right = false;
                        let mut shift = false;
                        for event in &input.events {
                            if let egui::Event::Key { key, pressed: true, modifiers, .. } = event {
                                if *key == Key::ArrowLeft {
                                    left = true;
                                    shift |= modifiers.shift;
                                } else if *key == Key::ArrowRight {
                                    right = true;
                                    shift |= modifiers.shift;
                                }
                            }
                        }
                        (left, right, shift)
                    });
                    if !shift
                        && let Some(range) = cursor_range
                        && let Some(at) = if right {
                            inline_image_navigation_edge(&c.text, range, true)
                        } else if left {
                            inline_image_navigation_edge(&c.text, range, false)
                        } else {
                            None
                        }
                        && let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id)
                    {
                        let cursor = egui::text::CCursor::new(at);
                        log::debug!(
                            "inline-image caret selection correction: block={} direction={} before=({}, {}) after={} text_chars={} focus={}",
                            i,
                            if right { "right" } else { "left" },
                            range.primary.index.0,
                            range.secondary.index.0,
                            at,
                            c.text.chars().count(),
                            true,
                        );
                        cursor_range = Some(egui::text::CCursorRange::one(cursor));
                        state.cursor.set_char_range(cursor_range);
                        state.store(ui.ctx(), id);
                    }
                    if let Some(range) = cursor_range {
                        input_caret = Some(out.galley.pos_from_cursor(range.primary).translate(out.galley_pos.to_vec2()));
                    }
                    at_start = cursor_range.is_some_and(|r| r.primary.index.0 == 0 && r.secondary.index.0 == 0);
                    if let Some(r) = cursor_range {
                        selection = Some((r.primary.index.0, r.secondary.index.0));
                    }
                    out.response
                })
                .inner;
            if resp.has_focus()
                && crate::input_language::is_rtl()
                && let Some(caret) = input_caret
            {
                paint_rtl_input_marker(ui, caret);
            }
            if let Some(sel) = selection {
                c.selection = sel;
            }
            // Ctrl+click (Cmd on a Mac) on a link while writing in the block follows it; a
            // plain click only moves the caret, which is how Word and OneNote do it.
            if resp.clicked()
                && ui.input(|i| i.modifiers.command)
                && !matches!(block, Block::Code { .. } | Block::MathBlock(_))
                && let Some((a, _)) = selection
                && let Some(href) = pagetext::link_at(&c.text, a)
            {
                changed.link = Some(href);
            }
            if let Some(done) = ticked {
                action = Some(Action::SetTask(i, done));
            }
            if c.focus {
                resp.request_focus();
                c.focus = false;
                // The caret goes where the click was, not to the start of the block.
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let at = c.place.take().unwrap_or_else(|| c.text.chars().count()).min(c.text.chars().count());
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(at))));
                    state.store(ui.ctx(), id);
                    c.selection = (at, at);
                }
            }
            // Restore after focus placement, which otherwise collapses the selected range.
            let wanted = ui.data(|d| d.get_temp::<(usize, usize, usize)>(restore_selection_id()));
            if let Some((block_at, primary, secondary)) = wanted
                && block_at == i
            {
                if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                    let (a, b) = (c.text.chars().count().min(secondary), c.text.chars().count().min(primary));
                    state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(b), egui::text::CCursor::new(a))));
                    state.store(ui.ctx(), id);
                    c.selection = (b, a);
                }
                ui.data_mut(|d| d.remove::<(usize, usize, usize)>(restore_selection_id()));
            }
            if resp.changed() {
                // What was typed goes into the block at once, so the page and the file
                // follow the caret without waiting for it to leave the block.
                let typed = c.text.clone();
                if let Some(b) = blocks.get_mut(i) {
                    let selection = c.selection;
                    c.ensure_inline(b).update_at(&typed, previous_selection, selection);
                    c.write(b);
                }
                changed.blocks = true;
            }
            let (enter, backspace, tab, shift, escape, up, down, right, left) = ui.input(|i| {
                (
                    i.key_pressed(Key::Enter),
                    i.key_pressed(Key::Backspace),
                    i.key_pressed(Key::Tab),
                    i.modifiers.shift,
                    i.key_pressed(Key::Escape),
                    i.key_pressed(Key::ArrowUp),
                    i.key_pressed(Key::ArrowDown),
                    i.key_pressed(Key::ArrowRight),
                    i.key_pressed(Key::ArrowLeft),
                )
            });
            let multiline = matches!(block, Block::Code { .. } | Block::MathBlock(_));
            if resp.has_focus() || resp.lost_focus() {
                if enter && !multiline {
                    action = Some(Action::Split(i));
                } else if backspace && at_start {
                    action = Some(Action::JoinBack(i));
                } else if tab && !multiline {
                    action = Some(Action::Indent(i, !shift));
                } else if escape {
                    action = Some(Action::Leave);
                } else if up {
                    if let Some(image) = previous_whole_block_image(blocks, i, shown_title) {
                        changed.select_object = Some((image, "image".to_string()));
                    } else {
                        action = adjacent_writable(blocks, i, false, shown_title).map(Action::Move);
                    }
                } else if down {
                    if let Some(image) = next_whole_block_image(blocks, i, shown_title) {
                        changed.select_object = Some((image, "image".to_string()));
                    } else {
                        action = adjacent_writable(blocks, i, true, shown_title).map(Action::Move);
                    }
                } else if right {
                    let at_end = selection.is_some_and(|(a, b)| a == b && a == c.text.chars().count());
                    let boundary = ui.data(|data| data.get_temp::<(usize, usize)>(image_boundary_id()));
                    if boundary.is_some_and(|(before, _)| before == i) {
                        if let Some((_, image)) = boundary {
                            changed.select_object = Some((image, "image".to_string()));
                            ui.data_mut(|data| data.remove::<(usize, usize)>(image_boundary_id()));
                        }
                    } else if at_end && let Some(image) = next_whole_block_image(blocks, i, shown_title) {
                        ui.data_mut(|data| data.insert_temp(image_boundary_id(), (i, image)));
                    }
                } else if left {
                    let boundary = ui.data(|data| data.get_temp::<(usize, usize)>(image_boundary_id()));
                    if boundary.is_some_and(|(before, _)| before == i) {
                        ui.data_mut(|data| data.remove::<(usize, usize)>(image_boundary_id()));
                    }
                }
            }
            frame_block(ui, top, outlined, t, zoom);
        } else {
            // A finished block. Clicking it puts the caret in it, under the pointer, and a
            // to-do box on it can be ticked without putting the caret anywhere.
            // The date under the title is drawn as a stamp: grey and quiet, the way
            // OneNote shows it, rather than as another paragraph of the page.
            let stamp = is_page_stamp(blocks, i);
            let out = ui
                .scope(|ui| {
                    if stamp {
                        ui.style_mut().visuals.override_text_color = Some(t.muted);
                    }
                    preview::show_block_zoomed(
                        ui,
                        &block,
                        i,
                        page_dir,
                        t,
                        true,
                        page,
                        justified.strength_of(i),
                        across
                            .filter(|a| !a.inside_one_block())
                            .and_then(|a| a.covering(i, block_text(&block).len()))
                            .and_then(|(a, b)| drawn_covering(&block, a, b)),
                        ui.make_persistent_id(("page-block", i)),
                        zoom,
                    )
                })
                .inner;
            let resp = out.response;
            log::debug!(
                "page block rendered: block={} stamp={} kind={} rect=({:.1},{:.1})-({:.1},{:.1}) size=({:.1}x{:.1})",
                i,
                stamp,
                block_kind(&block),
                resp.rect.min.x,
                resp.rect.min.y,
                resp.rect.max.x,
                resp.rect.max.y,
                resp.rect.width(),
                resp.rect.height()
            );
            // Dragging across words is how a reader selects them, and until now a drag that
            // began on a paragraph the caret was not already in did nothing at all: the
            // block answered a click and nothing answered a press that moved. So the caret
            // never arrived, no selection was made, and the page looked inert under the
            // pointer. The drag is read from the pointer itself rather than by sensing it
            // on a widget: a second `interact` over the same rectangle takes the pointer
            // from the one underneath whatever it is asked to sense, and five tests about
            // following a link said so twice. So the caret lands where the press fell, and
            // `write_rtl` carries the same drag on from the same reading.
            if is_writable(&block)
                && out.task_set.is_none()
                && let Some(p) = drag_began_in(ui, resp.rect)
            {
                let at = char_under(ui, &block, &resp, p, t, page);
                if across.is_none() {
                    *across = Some(Across { from: (i, byte_of_caret(&block, at)), to: (i, byte_of_caret(&block, at)) });
                }
                action = Some(Action::MoveTo(i, Some(at)));
            }
            // The drag has reached this block from another one.
            if is_writable(&block)
                && dragging(ui)
                && let Some(p) = ui.ctx().pointer_latest_pos()
                && resp.rect.contains(p)
            {
                let at = byte_of_caret(&block, char_under(ui, &block, &resp, p, t, page));
                match across.as_mut() {
                    // Already open: this is where it reaches now, the block it began in
                    // included, so dragging back the way it came shortens it again.
                    Some(open) => open.to = (i, at),
                    // Not open yet, so the drag began inside the block the caret is in -
                    // a text box, which handles its own selection and knows nothing of the
                    // page. It has now left that block, so the selection becomes the page's
                    // and keeps the anchor the text box was holding.
                    None => {
                        if let Some(c) = caret.as_ref().filter(|c| c.block != i)
                            && let Some(anchor_block) = blocks.get(c.block)
                        {
                            *across = Some(Across { from: (c.block, byte_of_caret(anchor_block, c.selection.1)), to: (i, at) });
                        }
                    }
                }
            }
            if let Some(done) = out.task_set {
                action = Some(Action::SetTask(i, done));
            } else if (resp.clicked() || resp.double_clicked()) && is_writable(&block) {
                // A click on a link follows it; anywhere else it puts the caret there.
                let link = (!contains_inline_image(&block))
                    .then(|| resp.interact_pointer_pos().and_then(|p| link_under(ui, &block, &resp, p, t, page)))
                    .flatten();
                match link {
                    Some(href) => changed.link = Some(href),
                    None => {
                        let at = resp.interact_pointer_pos().map(|p| char_under(ui, &block, &resp, p, t, page));
                        action = Some(Action::MoveTo(i, at));
                    }
                }
            }
            if resp.hovered() && is_writable(&block) {
                let over_link = resp.hover_pos().and_then(|p| link_under(ui, &block, &resp, p, t, page)).is_some();
                ui.ctx().set_cursor_icon(if over_link { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Text });
            }
            frame_block(ui, top, false, t, zoom);
        }
    }
    if let Some(a) = action {
        apply_keeping_widths(a, blocks, caret, &mut changed, cols);
    }
    changed
}

/// A selection that runs across more than one block: where the drag began and where it has
/// reached, each a block and a byte in that block's own words.
///
/// A caret belongs to one block, which is why this is kept beside it rather than inside it.
/// While a selection like this is open no block is a text box: the whole page is drawn as
/// finished words with the bands painted behind them, and the keys that act on a selection
/// act on the whole of it. That is how OneNote and Word behave, and until 2026-10-19 a drag
/// here simply stopped at the end of the paragraph it began in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Across {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

impl Across {
    /// The two ends in page order, first then last.
    pub fn ends(&self) -> ((usize, usize), (usize, usize)) {
        if self.from <= self.to { (self.from, self.to) } else { (self.to, self.from) }
    }

    /// The blocks it touches, in page order.
    pub fn blocks(&self) -> std::ops::RangeInclusive<usize> {
        let (a, b) = self.ends();
        a.0..=b.0
    }

    /// True while both ends are in the same block, which is the ordinary selection a text
    /// box handles by itself.
    pub fn inside_one_block(&self) -> bool {
        self.from.0 == self.to.0
    }

    /// What of block `at` is covered, in bytes of that block's words. `whole` is how long
    /// those words are, for the blocks the selection runs through completely.
    pub fn covering(&self, at: usize, whole: usize) -> Option<(usize, usize)> {
        let (a, b) = self.ends();
        if at < a.0 || at > b.0 {
            return None;
        }
        let start = if at == a.0 { a.1.min(whole) } else { 0 };
        let end = if at == b.0 { b.1.min(whole) } else { whole };
        (start < end).then_some((start, end))
    }
}

/// The words a selection across blocks covers, a paragraph to a line.
///
/// A blank line between them, which is what the page itself puts between paragraphs, so
/// what is copied out pastes back as the same paragraphs.
pub fn span_text(blocks: &[Block], across: Across) -> String {
    across
        .blocks()
        .filter_map(|i| {
            let whole = blocks.get(i).map(block_text)?;
            let (a, b) = across.covering(i, whole.len())?;
            Some(whole.get(a..b)?.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A multi-block clipboard fragment without reparsing literal editor text as markup.
/// Across positions are UTF-8 byte boundaries, while InlineEdit uses character offsets.
pub fn copied_span(blocks: &[Block], across: Across) -> Option<Vec<Block>> {
    let mut result = Vec::new();
    for index in across.blocks() {
        let block = blocks.get(index)?;
        let text = block_text(block);
        let Some((a, b)) = across.covering(index, text.len()) else { continue };
        let picked = text.get(a..b)?;
        let fragment = match block {
            Block::Code { lang, .. } => Block::Code { lang: lang.clone(), text: picked.to_owned() },
            Block::MathBlock(_) => Block::MathBlock(picked.to_owned()),
            _ => {
                let a = text.get(..a)?.chars().count();
                let b = text.get(..b)?.chars().count();
                Block::Paragraph(InlineEdit::new(block, false).selected(&text, a..b)?)
            }
        };
        result.push(fragment);
    }
    (!result.is_empty()).then_some(result)
}

/// Takes out everything a selection across blocks covers and joins what is left.
///
/// What is left of the first block and what is left of the last become one paragraph, the
/// blocks between them go, and the caret lands where the two halves meet - which is what
/// deleting a selection does in every editor. Returns the block the caret belongs in and
/// the character it belongs at.
pub fn delete_span(blocks: &mut Vec<Block>, across: Across) -> (usize, usize) {
    let ((first, from), (last, to)) = across.ends();
    let head = blocks.get(first).map(block_text).unwrap_or_default();
    let tail = blocks.get(last).map(block_text).unwrap_or_default();
    let kept_head = head.get(..from.min(head.len())).unwrap_or("").to_string();
    let kept_tail = tail.get(to.min(tail.len())..).unwrap_or("").to_string();
    let at = kept_head.chars().count();
    let mut kept = blocks.get(first).map(|b| InlineEdit::new(b, false)).unwrap_or_default().slice(0..at);
    let tail_start = tail.chars().count().saturating_sub(kept_tail.chars().count());
    let end = blocks.get(last).map(|b| InlineEdit::new(b, false)).unwrap_or_default();
    kept.join(end.slice(tail_start..tail.chars().count()));
    if let Some(b) = blocks.get_mut(first) {
        let text = kept.text().to_owned();
        kept.write(b, &text);
    }
    if last > first {
        let end = last.min(blocks.len().saturating_sub(1));
        blocks.drain(first + 1..=end);
    }
    renumber(blocks);
    (first, at)
}

/// `covering`'s bytes of `block_text` as bytes of the block's drawn words, which is what
/// the preview's selection bands measure in: the editable text carries marks and the tag
/// marker the drawn line never shows.
fn drawn_covering(block: &Block, a: usize, b: usize) -> Option<(usize, usize)> {
    spans_of(block)?;
    let whole = block_text(block);
    let range = chars_of(&whole, a)..chars_of(&whole, b);
    InlineEdit::new(block, false).drawn_range(range)
}

/// Byte offset in `block_text` of caret-text character `at`.
///
/// The caret's text drops the tag marker and writes an inline image as one character,
/// while `block_text` keeps the marker and the image's markup; `at` counts the caret's
/// characters, so the markup it skipped has to be added back.
fn byte_of_caret(block: &Block, at: usize) -> usize {
    let whole = block_text(block);
    let prefix = whole.chars().count() - Tag::split(&whole).1.chars().count();
    let mut pos = at + prefix;
    let edit = InlineEdit::new(block, true);
    for (image_at, span) in edit.images() {
        if *image_at >= at {
            break;
        }
        pos += spans_to_editor(std::slice::from_ref(span)).chars().count().saturating_sub(1);
    }
    byte_of_char(&whole, pos.min(whole.chars().count()))
}

/// The byte offset of the `at`-th character of `text`, or its end.
fn byte_of_char(text: &str, at: usize) -> usize {
    text.char_indices().nth(at).map(|(byte, _)| byte).unwrap_or(text.len())
}

/// An inline format over a selection that runs across blocks: every covered block's
/// covered words take it, the way the marks do when a whole box is picked. A block that
/// holds no spans - a table, a picture, code, an equation - is left alone.
///
/// Returns the selection with its ends remapped, because formatting can change how long a
/// block's editable text is; `None` when nothing formattable was covered. The covered
/// words themselves never change, so the same words stay selected.
pub fn format_across(blocks: &mut [Block], across: Across, format: InlineFormat) -> Option<Across> {
    if across.inside_one_block() {
        return None;
    }
    let (first, last) = across.ends();
    let mut changed = false;
    // Where each edge of the selection lands in the text its block has afterwards, so the
    // stored byte offsets still point at the same words.
    let mut start_byte = None;
    let mut end_byte = None;
    for index in across.blocks() {
        let Some(block) = blocks.get(index) else { continue };
        if spans_of(block).is_none() {
            continue;
        }
        let whole = block_text(block);
        let Some((a, b)) = across.covering(index, whole.len()) else { continue };
        let (Some(a), Some(b)) = (whole.get(..a), whole.get(..b)) else { continue };
        let covered = (a.chars().count(), b.chars().count());
        // The caret's own text drops the tag marker, which is not words and must not be
        // formatted; `covered` is counted in the whole block's text, so the marker's
        // characters come off the front of the range.
        let prefix = whole.chars().count() - Tag::split(&whole).1.chars().count();
        let mut edit = InlineEdit::new(block, true);
        let text = edit.text().to_owned();
        let range = (covered.0.saturating_sub(prefix), covered.1.saturating_sub(prefix));
        let Some((formatted, selection)) = edit.format(&text, range, format.clone()) else { continue };
        let Some(block) = blocks.get_mut(index) else { continue };
        edit.write(block, &formatted);
        changed = true;
        if index == first.0 || index == last.0 {
            let now = block_text(block);
            let mark = now.chars().count() - Tag::split(&now).1.chars().count();
            let byte_at = |at: usize| byte_of_char(&now, mark + at);
            if index == first.0 {
                start_byte = Some(byte_at(selection.0.min(selection.1)));
            }
            if index == last.0 {
                end_byte = Some(byte_at(selection.0.max(selection.1)));
            }
        }
    }
    if !changed {
        return None;
    }
    let mut next = across;
    // The drag may have run upwards, so `from` is not always the first block in the page.
    if let Some(byte) = start_byte {
        if next.from.0 == first.0 { next.from.1 = byte } else { next.to.1 = byte }
    }
    if let Some(byte) = end_byte {
        if next.from.0 == last.0 { next.from.1 = byte } else { next.to.1 = byte }
    }
    Some(next)
}

/// Where a drag began, when it began inside `rect` and is still going.
///
/// Read from the pointer rather than from a widget on purpose. A drag across words starts
/// on a block that is still finished text and ends in the text box that block turns into,
/// so no one widget sees the whole of it; and a widget laid over the words to watch for it
/// takes the pointer away from the words themselves, links included. The pointer knows
/// where it was pressed and whether it has moved since, which is all either end needs.
fn dragging(ui: &Ui) -> bool {
    ui.input(|i| i.pointer.is_decidedly_dragging())
}

fn drag_began_in(ui: &Ui, rect: egui::Rect) -> Option<egui::Pos2> {
    ui.input(|i| i.pointer.press_origin().filter(|p| rect.contains(*p) && i.pointer.is_decidedly_dragging()))
}

/// What a key or a click asked for.
enum Action {
    Move(usize),
    /// Put the caret in a block at a character, from a click.
    MoveTo(usize, Option<usize>),
    /// Tick or clear the to-do box on a line.
    SetTask(usize, bool),
    Split(usize),
    JoinBack(usize),
    Indent(usize, bool),
    /// A clipboard paste that is more than a line: a table, or several paragraphs.
    Paste(usize, Vec<Block>),
    Leave,
    ClickBelow,
    /// Put the caret in a cell of a table.
    EnterCell(usize, (usize, usize)),
    /// What was typed in a cell goes into the table.
    SetCell(usize, (usize, usize), String),
    /// Add a row to the end of a table and put the caret in its first cell.
    AddRow(usize),
    /// Pick a block of cells, from one corner to the other, as a drag across them does.
    Select(usize, (usize, usize), (usize, usize)),
    /// Empty every cell of the picked block, which is what Delete does to a selection.
    ClearCells(usize),
}

/// What a pasted clipboard string should become on the page.
///
/// OneNote, Word and Excel put a table on the clipboard as plain text with a tab between
/// cells and a newline between rows. Dropping that into one paragraph loses the table, so it
/// is turned back into one here. Several lines with no tabs become several paragraphs, which
/// is the other thing a one-line paste gets wrong.
#[derive(Debug, PartialEq, Eq)]
pub enum Pasted {
    /// Leave it to the text box: it belongs inside the line.
    Inline,
    /// Rows of cells, the first row being the heading.
    Table(Vec<Vec<String>>),
    /// One paragraph per line.
    Paragraphs(Vec<String>),
    /// Markup: the richest flavour, with the table structure and the marks inside the cells.
    Markup(String),
}

/// True for text that is really HTML. The apps that copy a table put an HTML flavour on the
/// clipboard beside the plain one, headed the way Windows heads it (`Version:1.0`,
/// `StartFragment`), and it is the better one to take: it carries the cells, the marks
/// inside them, and even the direction. Reading that flavour off the clipboard needs a
/// crate egui does not have; this catches it when it reaches us as text.
fn looks_like_html(text: &str) -> bool {
    let head = text.trim_start();
    let body = match head.find('<') {
        Some(i) if head.starts_with("Version:") => head.split_at(i).1,
        _ => head,
    };
    if !body.starts_with('<') {
        return false;
    }
    let lower = body.to_ascii_lowercase();
    [
        "<table",
        "<tr",
        "<p ",
        "<p>",
        "<div",
        "<ul",
        "<ol",
        "<li",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<span",
        "<a ",
        "<b>",
        "<strong",
        "<i>",
        "<em",
        "<u>",
        "<s>",
        "<br",
        "<blockquote",
        "<pre",
        "<code",
        "<img",
        "<html",
        "<body",
    ]
    .iter()
    .any(|tag| lower.contains(tag))
}

/// The HTML inside a clipboard flavour: what is between the fragment markers if they are
/// there, else everything from the first tag on.
fn html_fragment(text: &str) -> String {
    const START: &str = "<!--StartFragment-->";
    const END: &str = "<!--EndFragment-->";
    let start = text.find(START).map(|i| i + START.len());
    let end = text.find(END);
    match (start, end) {
        (Some(a), Some(b)) if b > a => text.get(a..b).unwrap_or(text).trim().to_string(),
        _ => match text.find('<') {
            Some(i) => text.split_at(i).1.trim().to_string(),
            None => text.trim().to_string(),
        },
    }
}

pub fn pasted(text: &str) -> Pasted {
    if looks_like_html(text) {
        return Pasted::Markup(html_fragment(text));
    }
    let text = text.replace("\r\n", "\n");
    let lines: Vec<String> = text.trim_end_matches('\n').split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    if lines.len() < 2 {
        return Pasted::Inline;
    }
    if lines.iter().any(|l| l.contains('\t')) {
        let rows: Vec<Vec<String>> = lines.iter().map(|l| l.split('\t').map(|c| c.trim().to_string()).collect()).collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        // A table of one column is a list of lines, not a table.
        if width > 1 {
            let rows: Vec<Vec<String>> = rows
                .into_iter()
                .map(|mut r| {
                    r.resize(width, String::new());
                    r
                })
                .collect();
            return Pasted::Table(rows);
        }
    }
    Pasted::Paragraphs(lines)
}

thread_local! {
    /// The blocks last copied from a box in this window, with the Markdown that went on the clipboard
    /// for them. The clipboard holds only text, so a heading, a link and a diagram copied as Markdown
    /// would come back as lines of words with their marks showing; the blocks themselves come back
    /// whole.
    static COPIED: std::cell::RefCell<Option<(String, Vec<Block>)>> = const { std::cell::RefCell::new(None) };
}

fn same_clipboard_text(text: &str) -> String {
    text.replace("\r\n", "\n").trim_end().to_string()
}

/// Remembers the blocks that were put on the clipboard as `markdown`.
pub fn remember_copied(markdown: &str, blocks: &[Block]) {
    COPIED.with(|c| *c.borrow_mut() = Some((same_clipboard_text(markdown), blocks.to_vec())));
}

/// What pasting `text` puts in the page: the blocks that were copied, when `text` is still what they
/// put on the clipboard; otherwise the text read as paragraphs, a table or a fragment of HTML.
/// Nothing when it is one line of words, which goes into the line the caret is in.
pub fn blocks_to_paste(text: &str) -> Vec<Block> {
    let want = same_clipboard_text(text);
    let copied = COPIED.with(|c| c.borrow().as_ref().filter(|(t, _)| *t == want).map(|(_, b)| b.clone()));
    copied.unwrap_or_else(|| pasted_blocks(pasted(text)))
}

/// Puts `new` into the page after block `at`, or in place of it if it is empty, with the caret after
/// the last of them. This is what Ctrl+V does with several blocks, for the ribbon's Paste to use too.
pub fn paste_at(blocks: &mut Vec<Block>, caret: &mut Option<Caret>, at: usize, new: Vec<Block>) -> Changed {
    let mut changed = Changed::default();
    apply(Action::Paste(at, new), blocks, caret, &mut changed);
    changed
}

/// The blocks a pasted table or paragraph list becomes.
pub fn pasted_blocks(p: Pasted) -> Vec<Block> {
    match p {
        Pasted::Inline => Vec::new(),
        // The same path an HTML page takes when it is opened, so a pasted table keeps its
        // cells and a pasted sentence keeps its bold and its links.
        Pasted::Markup(html) => notecraft_formats::markdown::parse(&notecraft_formats::html::to_markdown(&html)),
        Pasted::Paragraphs(lines) => lines.into_iter().map(|l| Block::Paragraph(parse_inline(&l))).collect(),
        Pasted::Table(mut rows) => {
            if rows.is_empty() {
                return Vec::new();
            }
            let head = rows.remove(0).iter().map(|c| parse_inline(c)).collect();
            let rows = rows.into_iter().map(|r| r.iter().map(|c| parse_inline(c)).collect()).collect();
            vec![Block::Table { head, rows }]
        }
    }
}

/// True for a line egui cannot put a caret in: anything with right-to-left letters, where
/// egui counts glyphs as characters and the caret lands in the wrong place.
pub fn needs_our_caret(text: &str) -> bool {
    text.chars().any(|c| notecraft_model::direction(c.encode_utf8(&mut [0u8; 4])) == notecraft_model::Dir::Rtl)
}

/// A character offset turned into a byte offset in `text`, and back.
fn byte_of(text: &str, chars: usize) -> usize {
    text.char_indices().nth(chars).map(|(b, _)| b).unwrap_or(text.len())
}

fn chars_of(text: &str, byte: usize) -> usize {
    text.get(..byte.min(text.len())).map(|s| s.chars().count()).unwrap_or(0)
}

/// What one frame of writing in a right-to-left line did.
pub struct Wrote {
    pub response: egui::Response,
    pub changed: bool,
    pub at_start: bool,
}

/// Writing in a line that has right-to-left text in it.
///
/// egui's own text box cannot do it: it treats a glyph index as a character offset, so in a
/// shaped script the caret lands in the wrong place and the text appears to jump
/// (`crates/ui-egui/tests/arabic_layout.rs`). So the line is drawn by the bidi painter,
/// which puts the runs where the Unicode algorithm says, and the caret is NoteCraft's own:
/// a click asks parley which character is under the pointer, and the arrow keys step the
/// way the eye moves, not the way the bytes run. `page` is the page's direction, which the
/// line reads in when it has no mark of its own, the same as when it is not being written in.
pub fn write_rtl(ui: &mut Ui, c: &mut Caret, size: f32, t: &Tokens, id: egui::Id, page: Option<notecraft_model::Dir>) -> Wrote {
    let width = ui.available_width().max(32.0);
    let inline = c.inline.clone().unwrap_or_default();
    let job = inline.layout(&c.text, |text| pagetext::inline_job_with_fonts(ui.ctx(), text, size, false, t, width, page));
    let pieces = job.sections.into_iter().map(|section| (section.byte_range.start.0..section.byte_range.end.0, section.format)).collect::<Vec<_>>();
    let (drawn, used) = crate::bidi::runs(ui, &c.text, &pieces, size, width, page);
    // The block's own id, the same one the egui text box uses for a line without
    // right-to-left text in it, so the focus carries over when a line becomes one or stops
    // being one under the caret.
    let (rect, _) = ui.allocate_exact_size(vec2(width, used.y.max(size * 1.3)), Sense::hover());
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    // A place kept behind the words for the selection, filled in below once the drag has
    // been read. The selection is opaque, so painting it after the words hid every word it
    // covered: a drag across an Arabic line left a row of empty boxes where the sentence
    // had been, which is what a reader took for a selection that did not work. The same
    // trick, for the same reason, as the shading behind a container in `containers.rs`.
    let behind = ui.painter().add(egui::Shape::Noop);
    crate::bidi::paint(ui, rect.min, &drawn);

    if c.focus {
        c.focus = false;
        // Where the caret lands when it arrives in this line: where the click was, and
        // otherwise the end of the words, which is what egui's text box does for a line
        // without right-to-left in it and what every editor does when you step into a
        // line. Leaving it at nought instead put the caret at the START of an Arabic
        // paragraph, so Enter split the line in front of its first letter: the whole
        // sentence moved down into the new paragraph, an empty one was left above it, and
        // the next thing typed ran on to the end of the sentence instead of starting a
        // new line. That is how a page of numbered Arabic lines ends up with three of
        // them crammed into one paragraph.
        let at = c.place.take().unwrap_or_else(|| c.text.chars().count()).min(c.text.chars().count());
        c.selection = (at, at);
    }
    let wanted = ui.data(|d| d.get_temp::<(usize, usize, usize)>(restore_selection_id()));
    if let Some((block, primary, secondary)) = wanted
        && block == c.block
    {
        let len = c.text.chars().count();
        c.selection = (primary.min(len), secondary.min(len));
        ui.data_mut(|d| d.remove::<(usize, usize, usize)>(restore_selection_id()));
    }
    // The caret is in this block, so this is where the typing goes.
    if !resp.has_focus() {
        resp.request_focus();
    }
    let mut changed = false;
    let chars = c.text.chars().count();
    // The selection is kept as two byte offsets: where it was started and where it is now.
    let mut anchor = byte_of(&c.text, c.selection.1.min(chars));
    let mut at = byte_of(&c.text, c.selection.0.min(chars));

    // A click puts the caret under the pointer, which only parley can work out here. A
    // double click takes the word, and a drag extends the selection from where it began.
    //
    // The drag may have begun before this text box existed, on the finished block the
    // caret has just landed in, and egui tracks a drag against the widget that was
    // pressed. So the pointer is read directly as well: `carried` is a drag that started
    // inside this line and is still going, whoever was under it at the time.
    let carried = drag_began_in(ui, rect).and(ui.ctx().pointer_latest_pos());
    if let Some(p) = resp.interact_pointer_pos().or(carried) {
        let point = p - rect.min;
        if resp.double_clicked() {
            let word = crate::bidi::word_at(&c.text, point, size, width, page);
            anchor = word.start;
            at = word.end;
            resp.request_focus();
        } else if resp.drag_started() {
            // egui calls a drag a drag only once the pointer has moved a little way, so by
            // now it is no longer where it was pressed. The selection has to begin where
            // the press fell, not here, or it collapses to a caret the moment the drag is
            // recognised and nothing is ever selected inside a block the caret already sat
            // in. That is what the owner saw.
            let began = ui.input(|i| i.pointer.press_origin()).unwrap_or(p);
            anchor = crate::bidi::hit(&c.text, began - rect.min, size, width, page);
            at = crate::bidi::hit(&c.text, point, size, width, page);
            resp.request_focus();
        } else if resp.clicked() && !resp.dragged() {
            at = crate::bidi::hit(&c.text, point, size, width, page);
            anchor = at;
            resp.request_focus();
        } else if resp.dragged() || carried.is_some() {
            at = crate::bidi::hit(&c.text, point, size, width, page);
            resp.request_focus();
        }
    }
    if resp.has_focus() {
        // The modifier is read off the key event itself, not off the global state: a
        // scripted key (the control channel, the tests) carries its modifiers there.
        let pressed = |want: Key| -> Option<bool> {
            ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, modifiers, .. } if *key == want => Some(modifiers.shift),
                    _ => None,
                })
            })
        };
        let (left, right, home, end) = (pressed(Key::ArrowLeft), pressed(Key::ArrowRight), pressed(Key::Home), pressed(Key::End));
        let shift = [left, right, home, end].iter().flatten().any(|s| *s);
        let (left, right, home, end) = (left.is_some(), right.is_some(), home.is_some(), end.is_some());
        let delete = pressed(Key::Delete).is_some();
        let moved = left || right || home || end;
        if left {
            at = crate::bidi::step(&c.text, at, false, size, width, page);
        }
        if right {
            at = crate::bidi::step(&c.text, at, true, size, width, page);
        }
        if home {
            at = 0;
        }
        if end {
            at = c.text.len();
        }
        // Shift keeps where the selection began; moving without it drops the selection.
        if moved && !shift {
            anchor = at;
        }
        // What was typed goes in at the caret, in the text's own order.
        let typed: String = ui.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(s) => Some(s.clone()),
                    _ => None,
                })
                .collect()
        });
        if !typed.is_empty() {
            // What is selected is replaced by what is typed, as everywhere else.
            let (a, b) = (anchor.min(at), anchor.max(at));
            if a < b {
                c.text.replace_range(a..b, "");
                at = a;
            }
            c.text.insert_str(at.min(c.text.len()), &typed);
            at += typed.len();
            anchor = at;
            changed = true;
        }
        if delete && at < c.text.len() {
            let next = c.text.get(at..).and_then(|s| s.chars().next()).map(char::len_utf8).unwrap_or(0);
            c.text.replace_range(at..at + next, "");
            changed = true;
        }
        if pressed(Key::Backspace).is_some() {
            let (a, b) = (anchor.min(at), anchor.max(at));
            if a < b {
                c.text.replace_range(a..b, "");
                at = a;
                anchor = a;
                changed = true;
            } else if at > 0 {
                let prev = c.text.get(..at).and_then(|s| s.chars().next_back()).map(char::len_utf8).unwrap_or(0);
                c.text.replace_range(at - prev..at, "");
                at -= prev;
                anchor = at;
                changed = true;
            }
        }
    }
    let at = at.min(c.text.len());
    let anchor = anchor.min(c.text.len());
    c.selection = (chars_of(&c.text, at), chars_of(&c.text, anchor));

    // What is selected, painted behind the words, one rectangle per run it covers.
    if resp.has_focus() && anchor != at {
        let shapes: Vec<egui::Shape> = crate::bidi::selection(ui, &c.text, &pieces, anchor, at, size, width, page)
            .into_iter()
            .map(|r| {
                let r = egui::Rect::from_min_max(rect.min + r.min.to_vec2(), rect.min + r.max.to_vec2());
                egui::Shape::rect_filled(r, 1.0, t.selection)
            })
            .collect();
        ui.painter().set(behind, egui::Shape::Vec(shapes));
    }
    // The caret itself, drawn where parley says it belongs on the line.
    if resp.has_focus()
        && let Some(bar) = crate::bidi::caret(ui, &c.text, &pieces, at, size, width, page)
    {
        let bar = egui::Rect::from_min_max(rect.min + bar.min.to_vec2(), rect.min + bar.max.to_vec2());
        let bar = egui::Rect::from_min_size(bar.min, vec2(1.5, bar.height().max(size)));
        ui.painter().rect_filled(bar, 0.0, t.accent);
        if crate::input_language::is_rtl() {
            paint_rtl_input_marker(ui, bar);
        }
    }
    Wrote { response: resp, changed, at_start: at == 0 }
}

/// A left-pointing triangle at the foot of the caret marks an active RTL keyboard layout.
pub(crate) fn paint_rtl_input_marker(ui: &Ui, caret: egui::Rect) {
    let x = caret.center().x;
    let y = caret.bottom();
    let points = vec![egui::pos2(x - 4.0, y), egui::pos2(x + 2.0, y - 3.0), egui::pos2(x + 2.0, y + 3.0)];
    ui.painter().add(egui::Shape::convex_polygon(points, ui.visuals().text_color(), egui::Stroke::NONE));
}

fn commit(blocks: &mut [Block], caret: &mut Option<Caret>) {
    if let Some(c) = caret
        && let Some(b) = blocks.get_mut(c.block)
    {
        c.write(b);
    }
}

/// [`apply`], and the table widths the page keeps move with their tables when the action puts
/// a block in or takes one out: Enter splitting a line, Backspace joining one to the line
/// above, a paste. A width is kept against a table's place among the blocks, so without this
/// a table after the change lost its widths, or another table took them (issue #154).
fn apply_keeping_widths(
    action: Action,
    blocks: &mut Vec<Block>,
    caret: &mut Option<Caret>,
    changed: &mut Changed,
    cols: &mut notecraft_model::tablecols::Widths,
) {
    let before = blocks.len();
    // Which block the action takes out, if it does (`Some` for certain; the bool says it
    // happens only when the count drops), and where the blocks it puts in start.
    let reshape = match &action {
        Action::Split(at) => Some((None, *at + 1)),
        Action::JoinBack(at) => Some((Some((*at, true)), *at)),
        Action::Paste(at, _) => {
            // An empty line is replaced by what is pasted; otherwise it goes in after the line.
            let empty = blocks.get(*at).map(|b| block_text(b).trim().is_empty()).unwrap_or(false);
            Some(if empty { (Some((*at, false)), *at) } else { (None, *at + 1) })
        }
        _ => None,
    };
    apply(action, blocks, caret, changed);
    let Some((removed, inserted_at)) = reshape else { return };
    let now = blocks.len();
    let removed = removed.filter(|(_, only_if_fewer)| !only_if_fewer || now < before).map(|(at, _)| at);
    let inserted = (now + usize::from(removed.is_some())).saturating_sub(before);
    let mut moved = false;
    if let Some(at) = removed {
        moved |= cols.remove_blocks(at..at + 1);
    }
    for offset in 0..inserted {
        moved |= cols.insert_block(inserted_at + offset);
    }
    changed.columns |= moved;
}

fn apply(action: Action, blocks: &mut Vec<Block>, caret: &mut Option<Caret>, changed: &mut Changed) {
    commit(blocks, caret);
    match action {
        Action::Move(to) => {
            let text = text_for_caret(blocks, to);
            *caret =
                blocks.get(to).map(|_| Caret { block: to, text, focus: true, selection: (0, 0), place: None, cell: None, span: None, inline: None });
            changed.caret = true;
        }
        Action::SetTask(at, done) => {
            if set_task(blocks, at, done) {
                changed.blocks = true;
            }
        }
        Action::MoveTo(to, at) => {
            let text = text_for_caret(blocks, to);
            *caret =
                blocks.get(to).map(|_| Caret { block: to, text, focus: true, selection: (0, 0), place: at, cell: None, span: None, inline: None });
            changed.caret = true;
        }
        Action::Leave => {
            *caret = None;
            changed.caret = true;
        }
        // Enter splits the line at the caret, the way it does in Word and OneNote: what is
        // before the caret stays, what is after it moves down with the caret. At the very
        // start of a line that means an empty line above and the words drop.
        Action::Paste(at, pasted) => {
            // What was pasted goes in after the line the caret is in, unless that line is
            // empty, in which case it takes its place. The caret lands after the last of it.
            let empty = blocks.get(at).map(|b| block_text(b).trim().is_empty()).unwrap_or(false);
            let start = if empty { at } else { at + 1 };
            if empty {
                blocks.remove(at);
            }
            let n = pasted.len();
            for (i, b) in pasted.into_iter().enumerate() {
                let index = (start + i).min(blocks.len());
                blocks.insert(index, b);
            }
            renumber(blocks);
            let land = (start + n).saturating_sub(1).min(blocks.len().saturating_sub(1));
            let text = blocks.get(land).map(block_text).unwrap_or_default();
            let end = text.chars().count();
            *caret = Some(Caret { block: land, text, focus: true, selection: (end, end), place: Some(end), cell: None, span: None, inline: None });
            changed.blocks = true;
            changed.caret = true;
        }
        Action::Split(at) => {
            // Enter on an empty list item ends the list, as it does in every editor.
            if matches!(blocks.get(at), Some(Block::Item { spans, .. }) if spans.is_empty()) {
                if let Some(b) = blocks.get_mut(at) {
                    *b = Block::Paragraph(Vec::new());
                }
                *caret =
                    Some(Caret { block: at, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None, inline: None });
                renumber(blocks);
                changed.blocks = true;
                changed.caret = true;
                return;
            }
            let text = caret.as_ref().map(|c| c.text.clone()).unwrap_or_default();
            let cut = caret.as_ref().map(|c| c.selection.0.min(c.selection.1)).unwrap_or_else(|| text.chars().count());
            let cut = cut.min(text.chars().count());
            let before: String = text.chars().take(cut).collect();
            let after: String = text.chars().skip(cut).collect();
            let source = caret
                .as_ref()
                .and_then(|caret| caret.inline.clone())
                .or_else(|| blocks.get(at).map(|block| InlineEdit::new(block, true)))
                .unwrap_or_default();
            let mut left = source.slice(0..cut);
            let mut right = source.slice(cut..text.chars().count());
            let mut new = blocks.get(at).map(next_block).unwrap_or(Block::Paragraph(Vec::new()));
            if let Some(b) = blocks.get_mut(at) {
                left.write(b, &before);
                // A heading splits into a heading and a paragraph, as it does in Word.
                if matches!(b, Block::Heading { .. }) {
                    new = Block::Paragraph(Vec::new());
                }
            }
            right.write(&mut new, &after);
            blocks.insert(at + 1, new);
            renumber(blocks);
            *caret = Some(Caret {
                block: at + 1,
                text: after,
                focus: true,
                selection: (0, 0),
                place: Some(0),
                cell: None,
                span: None,
                inline: Some(right),
            });
            changed.blocks = true;
            changed.caret = true;
        }
        Action::JoinBack(at) => {
            let text = caret.as_ref().map(|c| c.text.clone()).unwrap_or_default();
            if at == 0 {
                // The first block: a heading becomes a paragraph, nothing is lost.
                if let Some(Block::Heading { .. }) = blocks.first() {
                    if let Some(b) = blocks.get_mut(0) {
                        *b = Block::Paragraph(spans_of(b).unwrap_or_default().to_vec());
                    }
                    *caret = Some(Caret { block: 0, text, focus: true, selection: (0, 0), place: None, cell: None, span: None, inline: None });
                    changed.blocks = true;
                }
                return;
            }
            let previous = blocks.get(at - 1).cloned();
            match previous {
                Some(p) if is_writable(&p) => {
                    let mut source = InlineEdit::new(&p, true);
                    let at_end = source.text().chars().count();
                    let current = caret
                        .as_ref()
                        .and_then(|caret| caret.inline.clone())
                        .or_else(|| blocks.get(at).map(|block| InlineEdit::new(block, true)))
                        .unwrap_or_default();
                    source.join(current);
                    let joined = source.text().to_owned();
                    if let Some(b) = blocks.get_mut(at - 1) {
                        source.write(b, &joined);
                    }
                    blocks.remove(at);
                    renumber(blocks);
                    *caret = Some(Caret {
                        block: at - 1,
                        text: joined,
                        focus: true,
                        selection: (0, 0),
                        place: None,
                        cell: None,
                        span: None,
                        inline: Some(source),
                    });
                    let _ = at_end;
                    changed.blocks = true;
                    changed.caret = true;
                }
                _ => {}
            }
        }
        Action::Indent(at, deeper) => {
            if let Some(Block::Item { depth, .. }) = blocks.get_mut(at) {
                *depth = if deeper { (*depth + 1).min(8) } else { depth.saturating_sub(1) };
                renumber(blocks);
                changed.blocks = true;
            }
        }
        Action::EnterCell(i, cell) => {
            let text = blocks.get(i).and_then(|b| cell_text(b, cell)).unwrap_or_default();
            *caret = Some(Caret { block: i, text, focus: true, selection: (0, 0), place: None, cell: Some(cell), span: None, inline: None });
            changed.caret = true;
        }
        Action::Select(i, from, to) => {
            let text = blocks.get(i).and_then(|b| cell_text(b, from)).unwrap_or_default();
            // No focus asked for: a block of cells is picked, not written in.
            *caret = Some(Caret { block: i, text, focus: false, selection: (0, 0), place: None, cell: Some(from), span: Some(to), inline: None });
            changed.caret = true;
        }
        Action::ClearCells(i) => {
            let Some(((r0, c0), (r1, c1))) = caret.as_ref().and_then(cell_range) else { return };
            if let Some(b) = blocks.get_mut(i) {
                for r in r0..=r1 {
                    for c in c0..=c1 {
                        if cell_text(b, (r, c)).is_some() {
                            set_cell_text(b, (r, c), "");
                        }
                    }
                }
                changed.blocks = true;
            }
        }
        Action::AddRow(i) => {
            if let Some(Block::Table { head, rows }) = blocks.get_mut(i) {
                let columns = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(1)).max(1);
                rows.push(vec![Vec::new(); columns]);
                changed.blocks = true;
                let cell = (rows.len(), 0);
                *caret = Some(Caret {
                    block: i,
                    text: String::new(),
                    focus: true,
                    selection: (0, 0),
                    place: None,
                    cell: Some(cell),
                    span: None,
                    inline: None,
                });
                changed.caret = true;
            }
        }
        Action::SetCell(i, cell, text) => {
            if let Some(b) = blocks.get_mut(i) {
                if let Some(caret) = caret.as_mut().filter(|caret| caret.block == i && caret.cell == Some(cell)) {
                    caret.text = text;
                    caret.write(b);
                } else {
                    set_cell_text(b, cell, &text);
                }
                changed.blocks = true;
            }
        }
        Action::ClickBelow => {
            let last_writable = blocks.iter().rposition(is_writable);
            match last_writable {
                // An empty last block is where the caret goes; otherwise a new one is added.
                Some(i) if blocks.get(i).map(block_text).unwrap_or_default().is_empty() => {
                    *caret = Some(Caret {
                        block: i,
                        text: String::new(),
                        focus: true,
                        selection: (0, 0),
                        place: None,
                        cell: None,
                        span: None,
                        inline: None,
                    });
                }
                _ => {
                    blocks.push(Block::Paragraph(Vec::new()));
                    *caret = Some(Caret {
                        block: blocks.len() - 1,
                        text: String::new(),
                        focus: true,
                        selection: (0, 0),
                        place: None,
                        cell: None,
                        span: None,
                        inline: None,
                    });
                    changed.blocks = true;
                }
            }
            changed.caret = true;
        }
    }
}

/// The character of a finished block under `pos`, given the area `resp` it was drawn in.
fn char_under(ui: &Ui, block: &Block, resp: &egui::Response, pos: egui::Pos2, t: &Tokens, page: Option<notecraft_model::Dir>) -> usize {
    if let Some(at) = char_under_drawn(ui, block, resp, pos, t, page) {
        return at;
    }
    let (size, strong) = size_of(block);
    let text = caret_text(block);
    char_at(ui, &text, size, strong, t, resp.rect.width(), pos - resp.rect.min - egui::vec2(lead_width(block), 0.0), page)
}

/// Where a click on a finished block lands in the text the caret writes in.
///
/// A finished block is drawn from its parsed words, with no Markdown marks in sight, while
/// the caret writes in the text that still carries them. Measuring the pointer against the
/// marked text puts the caret early, and further out after a link, whose target is long in
/// the text and invisible on the page. So the pointer is measured against what was drawn and
/// the answer is carried back over the marks. `None` when that carrying back cannot be
/// trusted, and the caller measures the old way.
fn char_under_drawn(ui: &Ui, block: &Block, resp: &egui::Response, pos: egui::Pos2, t: &Tokens, page: Option<notecraft_model::Dir>) -> Option<usize> {
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans,
        _ => return None,
    };
    if spans.iter().any(|s| s.math || s.image.is_some()) {
        return None;
    }
    let (spans, mark) = match preview::tagged(spans) {
        Some((_, stripped)) => (stripped, 21.0),
        None => (spans.clone(), 0.0),
    };
    // The walk below counts the marks span by span. That count is only right when the marks
    // it counts are the ones the caret's text really has, so the two are compared first.
    if spans_to_editor(&spans) != caret_text(block) {
        return None;
    }
    let drawn: String = spans.iter().map(|s| s.text.as_str()).collect();
    let lead = mark + lead_width(block);
    // A row that reads right to left keeps its bullet and its tag mark on the right, so its
    // words start at the left edge; a row that reads the other way has them after the lead.
    let rtl = preview::dir_of(&spans, page).is_rtl();
    let rel = pos - resp.rect.min;
    let x = if rtl { rel.x } else { rel.x - lead };
    let column = (resp.rect.width() - lead).max(32.0);
    let (size, strong) = size_of(block);
    let at = if crate::bidi::paints(&drawn, page) {
        let byte = crate::bidi::hit(&drawn, vec2(x, rel.y), size, column, page);
        drawn.get(..byte).unwrap_or(drawn.as_str()).chars().count()
    } else {
        char_at(ui, &drawn, size, strong, t, column, vec2(x, rel.y), page)
    };
    Some(over_marks(&spans, at))
}

/// A character of the drawn words, as a character of the same line written with its marks.
fn over_marks(spans: &[Span], drawn: usize) -> usize {
    let source = spans_to_editor(spans);
    if drawn >= spans.iter().map(|span| span.text.chars().count()).sum() {
        return source.chars().count();
    }
    if let Some(marker) = ('\u{e000}'..='\u{f8ff}').find(|c| !source.contains(*c)) {
        let mut marked = spans.to_vec();
        if insert_span_marker(&mut marked, drawn, marker, true).is_some() {
            let probe = spans_to_editor(&marked);
            if probe.replace(marker, "") == source
                && let Some(at) = probe.find(marker)
            {
                return probe[..at].chars().count();
            }
        }
    }
    let mut seen = 0usize;
    let mut marked = 0usize;
    for s in spans {
        let n = s.text.chars().count();
        let wrap = if s.code { 1 } else { 2 * usize::from(s.bold) + usize::from(s.italic) + 2 * usize::from(s.strike) };
        let open = usize::from(s.link.is_some()) + wrap;
        if drawn < seen + n {
            return marked + open + (drawn - seen);
        }
        seen += n;
        // A link closes with `](target)`: the bracket, the two round ones and the target.
        marked += open + n + wrap + s.link.as_ref().map_or(0, |to| 3 + to.chars().count());
    }
    marked
}

/// The link the character `at` of `block` sits in, if any, for a block egui laid out. Code
/// and equations hold no links. A block the bidi painter drew is not mapped this way (see
/// `link_under`).
fn link_in(block: &Block, at: usize, page: Option<notecraft_model::Dir>) -> Option<String> {
    if matches!(block, Block::Code { .. } | Block::MathBlock(_)) {
        return None;
    }
    let text = caret_text(block);
    if crate::bidi::paints(&text, page) {
        return None;
    }
    pagetext::link_at(&text, at)
}

/// The link under the pointer at `pos` in a finished block, if the pointer is on one. A
/// right-to-left or mixed line is asked through the painter that drew it; any other line
/// through egui's layout of its text.
pub fn link_under(ui: &Ui, block: &Block, resp: &egui::Response, pos: egui::Pos2, t: &Tokens, page: Option<notecraft_model::Dir>) -> Option<String> {
    if !is_writable(block) {
        return None;
    }
    if let Some(href) = preview::link_at_point(block, pos - resp.rect.min, resp.rect.width(), page) {
        return Some(href);
    }
    link_in(block, char_under(ui, block, resp, pos, t, page), page)
}

/// How wide the bullet or quote bar in front of a block is, so a click lands on the right
/// character of the text beside it.
fn lead_width(block: &Block) -> f32 {
    match block {
        Block::Item { depth, .. } => 8.0 + *depth as f32 * 18.0 + 22.0,
        Block::Quote(_) => 18.0,
        _ => 0.0,
    }
}

/// What goes in front of a block while it is written in: a bullet, a number, a quote bar.
/// Returns the new state of the to-do box if the user ticked it.
fn lead(ui: &mut Ui, block: &Block, t: &Tokens) -> Option<bool> {
    let mut ticked = None;
    // A tag's mark goes in front of the line while it is written in too, so the page does
    // not change shape under the caret.
    if let (Some(tag), _) = tag_of(block)
        && tag != Tag::ToDo
    {
        let (rect, resp) = ui.allocate_exact_size(vec2(21.0, 20.0), Sense::hover());
        let mark = crate::ribbon::mark_for(tag);
        crate::ribbon_icons::paint(
            &ui.painter_at(rect),
            egui::Rect::from_center_size(rect.center(), vec2(17.0, 17.0)),
            mark,
            mark.color(t.text, t.accent),
        );
        resp.on_hover_text(crate::tl!(tag.label()));
    }
    match block {
        Block::Item { ordered, number, task, depth, .. } => {
            ui.add_space(8.0 + *depth as f32 * 18.0);
            match task {
                Some(done) => {
                    let mut v = *done;
                    let response = ui.checkbox(&mut v, "");
                    let enabled = ui.is_enabled();
                    let name = crate::tl_id!("ui.to_do_2").to_string();
                    response.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::CheckBox, enabled, v, name.clone()));
                    if response.changed() {
                        ticked = Some(v);
                    }
                }
                None => {
                    // i18n-exempt: This glyph is the visual bullet marker for an unordered list.
                    ui.label(egui::RichText::new(if *ordered { format!("{number}.") } else { "\u{2022}".to_string() }).color(t.muted));
                }
            }
        }
        Block::Quote(_) => {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(crate::tl_id!("ui.text_122")).color(t.quote_bar));
        }
        _ => {}
    }
    ticked
}

/// The blocks written back in the page's own format.
pub fn to_body(format: notecraft_model::PageFormat, blocks: &[Block]) -> String {
    match format {
        notecraft_model::PageFormat::Html => notecraft_formats::render::blocks_to_html(blocks),
        notecraft_model::PageFormat::Cmgfm => notecraft_formats::render::blocks_to_markdown(blocks),
    }
}

/// Whether a page can be written on as blocks: its own text must survive the trip through
/// them. A page NoteCraft wrote always does; one with markup the block model does not know
/// would lose that markup, so it stays read-only and the source pane is offered instead.
pub fn round_trips(format: notecraft_model::PageFormat, body: &str) -> bool {
    // Markup the block model cannot hold would be dropped on the way back out.
    if format == notecraft_model::PageFormat::Html && notecraft_formats::html::unsupported_tag(body).is_some() {
        return false;
    }
    // The boxes are part of the text, so they have to come back as they went in.
    let (blocks, boxes) = notecraft_formats::boxes::read(format, body);
    let written = notecraft_formats::boxes::write(format, &blocks, boxes.as_deref());
    notecraft_formats::boxes::read(format, &written) == (blocks, boxes)
}

#[cfg(test)]
mod tests {
    /// The cursor is lit from the moment the caret is put somewhere, for the first half of each
    /// blink, and the window is woken when that changes.
    #[test]
    fn a_placed_cursor_is_lit_at_once_and_blinks_from_then() {
        assert_eq!(super::blink_phase(0.0, 0.5, 0.5), (true, 0.5));
        assert_eq!(super::blink_phase(0.25, 0.5, 0.5), (true, 0.25));
        assert_eq!(super::blink_phase(0.75, 0.5, 0.5), (false, 0.25));
        assert_eq!(super::blink_phase(1.0, 0.5, 0.5), (true, 0.5));
    }

    use super::*;
    use notecraft_model::PageFormat;

    fn blocks(md: &str) -> Vec<Block> {
        notecraft_formats::markdown::parse(md)
    }

    /// Issue #163: the Link dialog finds the link the caret is in, writes one that reads back as
    /// a link, and will not link to a script.
    #[test]
    fn links_are_found_written_and_checked() {
        let line = "see [the plan](https://example.test/a) and ![pic](assets/p.png) here";
        let (start, end, words, address) = link_at(line, 7, 7).expect("the caret is in the link");
        assert_eq!((words.as_str(), address.as_str()), ("the plan", "https://example.test/a"));
        assert_eq!(line.chars().skip(start).take(end - start).collect::<String>(), "[the plan](https://example.test/a)");
        assert!(link_at(line, 0, 2).is_none(), "not in a link");
        assert!(link_at(line, 46, 46).is_none(), "a picture is not a link");
        assert!(link_at(line, 2, 7).is_none(), "a selection running out of the link is not in it");

        assert_eq!(link_markup("the plan", " https://example.test/a b "), "[the plan](https://example.test/a%20b)");
        assert_eq!(link_markup("", "https://example.test"), "[https://example.test](https://example.test)", "no words: the address shows");
        assert_eq!(link_markup("a [b] c", "x(1)"), "[a b c](x%281%29)");
        let spans = notecraft_formats::markdown::parse(&link_markup("go [now]", "../other page/")).remove(0);
        let Block::Paragraph(spans) = spans else { panic!("a paragraph") };
        assert_eq!(spans.len(), 1, "one link: {spans:?}");
        assert_eq!(spans[0].link.as_deref(), Some("../other%20page/"));

        for ok in ["https://example.test", "mailto:a@example.test", "../page/", "notecraft:/book/s/p?id=1"] {
            assert!(linkable_address(ok), "{ok}");
        }
        for bad in ["", "  ", "javascript:alert(1)", "JavaScript:x", "data:text/html,x", "vbscript:x", "a\nb"] {
            assert!(!linkable_address(bad), "{bad:?}");
        }
    }

    /// Issue #165: the Format Painter picks up the formatting at the caret and puts exactly that
    /// on other words: what the source has goes on, what it has not comes off, the words stay.
    #[test]
    fn the_format_painter_copies_formatting_and_keeps_words() {
        let text = "**Big** plain *slanted*";
        let bold = style_at(text, (3, 3)).expect("a style in the bold word");
        assert!(bold.bold && !bold.italic && bold.text.is_empty(), "{bold:?}");
        let plain = style_at(text, (9, 9)).expect("a style in the plain word");
        assert!(!plain.bold && !plain.italic);

        // "plain" (characters 8..13 of the text) takes the bold word's formatting.
        let (painted, caret) = paint_source_selection(text, (8, 13), &bold).expect("painted");
        let spans = parse_inline(&painted);
        let words: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(words, "Big plain slanted", "the words are the same: {painted}");
        assert!(spans.iter().filter(|s| s.text.contains("plain")).all(|s| s.bold), "plain is bold now: {painted}");
        assert!(spans.iter().filter(|s| s.text.contains("slanted")).all(|s| s.italic && !s.bold), "slanted is untouched: {painted}");
        let before: String = painted.chars().take(caret.0).collect();
        assert!(before.trim_end_matches('*').ends_with("plain"), "the caret is after the painted words: {before:?}");

        // The plain formatting takes the italic off "slanted".
        let start = text.find("*slanted").expect("slanted");
        let (painted, _) = paint_source_selection(text, (start, text.chars().count()), &plain).expect("painted");
        assert!(parse_inline(&painted).iter().all(|s| !s.italic), "italic came off: {painted}");
        assert!(painted.contains("slanted"));
    }

    /// Issue #171: Table > Sort moves whole rows by one column, keeps the heading on top,
    /// compares numbers as numbers and puts empty cells last.
    #[test]
    fn a_table_sorts_whole_rows_by_one_column() {
        let mut b = blocks("| Name | Amount |\n|---|---|\n| pear | 1,200 |\n| Apple | 30 |\n|  | 5 |\n| banana | $40 |\n").remove(0);
        let column = |b: &Block, c: usize| -> Vec<String> {
            let Block::Table { rows, .. } = b else { panic!("a table") };
            rows.iter().map(|r| r[c].iter().map(|s| s.text.as_str()).collect::<String>()).collect()
        };
        assert!(sort_table(&mut b, 0, None, false));
        assert_eq!(column(&b, 0), ["Apple", "banana", "pear", ""], "words without regard to case, the empty cell last");
        assert_eq!(column(&b, 1), ["30", "$40", "1,200", "5"], "each row kept together");
        assert!(sort_table(&mut b, 0, None, true));
        assert_eq!(column(&b, 0), ["pear", "banana", "Apple", ""], "descending, the empty cell still last");

        assert!(sort_table(&mut b, 1, None, false));
        assert_eq!(column(&b, 1), ["5", "30", "$40", "1,200"], "numbers as numbers, not as words");
        let Block::Table { head, .. } = &b else { panic!("a table") };
        assert_eq!(head[0].iter().map(|s| s.text.as_str()).collect::<String>(), "Name", "the heading stays on top");
        assert!(!sort_table(&mut b, 1, None, false), "already in that order: nothing moves");

        // Only the picked rows (table rows 2 to 3, which count the heading as 0).
        assert!(sort_table(&mut b, 1, Some((2, 3)), true));
        assert_eq!(column(&b, 1), ["5", "$40", "30", "1,200"], "only the picked rows turned round");
    }

    #[test]
    fn numbers_sort_before_words() {
        let mut b = blocks("| A |\n|---|\n| zed |\n| 7 |\n| alpha |\n| -2.5 |\n").remove(0);
        assert!(sort_table(&mut b, 0, None, false));
        let Block::Table { rows, .. } = &b else { panic!("a table") };
        let got: Vec<String> = rows.iter().map(|r| r[0].iter().map(|s| s.text.as_str()).collect()).collect();
        assert_eq!(got, ["-2.5", "7", "alpha", "zed"]);
    }

    #[test]
    fn selection_fonts_follow_caret_and_mixed_unicode_text() {
        let text = "<span style=\"font-family: Example Sans; font-size: 18pt\">مرحبا</span> plain";
        let start = text.find("مرحبا").expect("text");
        let at = text[..start].chars().count();
        let selected = selection_fonts(text, (at, at + 3)).expect("selection");
        assert_eq!(selected.family, FontValue::Value(Some("Example Sans".to_string())));
        assert_eq!(selected.size, FontValue::Value(Some(18)));
        assert_eq!(selection_fonts(text, (at + 2, at + 2)), Some(selected));
        let mixed = selection_fonts(text, (0, text.chars().count())).expect("whole selection");
        assert_eq!(mixed.family, FontValue::Mixed);
        assert_eq!(mixed.size, FontValue::Mixed);
        assert!(selection_fonts("**abc**", (1, 4)).is_none());
    }

    #[test]
    fn partial_source_selection_formats_nested_mixed_unicode_without_losing_marks() {
        let text = "a **مرحبا** z";
        let (formatted, selection) = format_source_selection(text, (5, 8), InlineFormat::Underline).expect("safe selection");
        let spans = parse_inline(&formatted);
        assert_eq!(spans.iter().map(|s| s.text.as_str()).collect::<String>(), "a مرحبا z");
        assert_eq!(spans.iter().filter(|s| s.underline).map(|s| s.text.as_str()).collect::<String>(), "رحب");
        assert!(spans.iter().filter(|s| s.underline).all(|s| s.bold));
        let (off, _) = format_source_selection(&formatted, selection, InlineFormat::Underline).expect("toggle");
        assert!(parse_inline(&off).iter().all(|s| !s.underline));
    }

    #[test]
    fn partial_source_selection_refuses_markup_boundaries_and_images() {
        assert!(format_source_selection("**abc**", (1, 4), InlineFormat::Underline).is_none());
        assert!(format_source_selection("a \u{fffc} b", (0, 3), InlineFormat::Underline).is_none());
    }

    #[test]
    fn partial_source_family_and_script_preserve_other_styles_and_selection_direction() {
        let text = "a **مرحبا** z";
        let (family, selection) = format_source_selection(text, (8, 5), InlineFormat::Family("Noto Sans".into())).expect("family");
        assert!(selection.0 > selection.1);
        let (script, _) = format_source_selection(&family, selection, InlineFormat::Superscript).expect("script");
        let spans = parse_inline(&script);
        assert_eq!(
            spans.iter().filter(|s| s.script == Some(notecraft_formats::markdown::Script::Superscript)).map(|s| s.text.as_str()).collect::<String>(),
            "رحب"
        );
        assert!(spans.iter().filter(|s| s.script.is_some()).all(|s| s.bold && s.font_family.as_deref() == Some("Noto Sans")), "{script}");
    }

    #[test]
    fn formatting_a_partial_unicode_selection_preserves_other_span_properties() {
        let mut spans = parse_inline("a **مرحبا** z");
        assert!(format_span_range(&mut spans, 3..6, InlineFormat::Underline));
        assert_eq!(spans.iter().map(|s| s.text.as_str()).collect::<String>(), "a مرحبا z");
        let underlined: String = spans.iter().filter(|s| s.underline).map(|s| s.text.as_str()).collect();
        assert_eq!(underlined, "رحب");
        assert!(spans.iter().filter(|s| s.underline).all(|s| s.bold));
        assert!(format_span_range(&mut spans, 3..6, InlineFormat::Underline));
        assert!(spans.iter().all(|s| !s.underline));
        assert_eq!(spans.iter().filter(|s| s.bold).map(|s| s.text.as_str()).collect::<String>(), "مرحبا");
    }

    #[test]
    fn formatting_mixed_runs_and_scripts_uses_one_consistent_toggle() {
        use notecraft_formats::markdown::Script;
        let mut spans = parse_inline("<u>a</u>b<sub>c</sub>");
        assert!(format_span_range(&mut spans, 0..3, InlineFormat::Underline));
        assert!(spans.iter().all(|s| s.underline));
        assert!(format_span_range(&mut spans, 1..3, InlineFormat::Superscript));
        assert_eq!(spans.iter().filter(|s| s.script == Some(Script::Superscript)).map(|s| s.text.as_str()).collect::<String>(), "bc");
        assert!(format_span_range(&mut spans, 1..3, InlineFormat::Superscript));
        assert!(spans.iter().all(|s| s.script.is_none()));
    }

    #[test]
    fn sizing_and_family_only_change_selected_text_and_ignore_atomic_content() {
        let mut spans = vec![
            Span { text: "ab".into(), link: Some("target".into()), ..Span::default() },
            Span { text: "picture".into(), image: Some("image.png".into()), ..Span::default() },
            Span { text: "cd".into(), ..Span::default() },
        ];
        assert!(format_span_range(&mut spans, 1..4, InlineFormat::Size(18)));
        assert!(format_span_range(&mut spans, 1..4, InlineFormat::Family("Noto Sans".into())));
        assert_eq!(spans.iter().filter(|s| s.font_size == Some(18)).map(|s| s.text.as_str()).collect::<String>(), "bc");
        assert!(spans.iter().filter(|s| s.font_size == Some(18)).all(|s| s.font_family.as_deref() == Some("Noto Sans")));
        assert_eq!(spans.iter().find(|s| s.image.is_some()).and_then(|s| s.font_size), None);
        assert!(spans.iter().filter(|s| s.text == "a" || s.text == "b").all(|s| s.link.as_deref() == Some("target")));
        assert!(!format_span_range(&mut spans, 2..3, InlineFormat::Underline));
    }

    #[test]
    fn partial_styles_round_trip_through_both_page_formats() {
        let mut spans = parse_inline("a **مرحبا** z");
        assert!(format_span_range(&mut spans, 3..6, InlineFormat::Underline));
        assert!(format_span_range(&mut spans, 3..6, InlineFormat::Family("Noto Sans".into())));
        assert!(format_span_range(&mut spans, 3..6, InlineFormat::Size(18)));
        let original = vec![Block::Paragraph(spans)];
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let body = to_body(format, &original);
            assert_eq!(preview::blocks_for(format, &body), original, "{format:?}: {body}");
        }
    }

    /// A format over a selection that runs across blocks takes each block's covered words,
    /// leaves the rest of the page alone, and keeps the selection's ends on the same words.
    #[test]
    fn a_format_runs_across_the_blocks_a_selection_covers() {
        let words = |b: &Block| spans_of(b).unwrap_or(&[]).iter().map(|s| s.text.as_str()).collect::<String>();
        let mut b = blocks("one two\n\n#idea three four\n\nfive six\n");
        let first = block_text(&b[0]);
        let second = block_text(&b[1]);
        let across = Across { from: (0, first.len() / 2), to: (1, second.len() / 2) };
        let next = format_across(&mut b, across, InlineFormat::Size(18)).expect("formatted");
        // Both edge blocks took the size on their covered words; the block below did not.
        for (index, sized) in [(0usize, true), (1, true), (2, false)] {
            let hit = spans_of(&b[index]).unwrap_or(&[]).iter().any(|s| s.font_size == Some(18));
            assert_eq!(hit, sized, "block {index}");
        }
        // No word was lost or gained, and the tag marker is not words: it stays put.
        assert_eq!(words(&b[0]), "one two");
        assert_eq!(words(&b[1]), "#idea three four");
        assert_eq!(words(&b[2]), "five six");
        assert_eq!(tag_of(&b[1]).0, Some(Tag::Idea));
        // The remapped ends still cover words in both blocks.
        assert!(next.covering(0, block_text(&b[0]).len()).is_some());
        assert!(next.covering(1, block_text(&b[1]).len()).is_some());
        // The same selection takes and loses a mark like it does inside one block.
        let bold = format_across(&mut b, next, InlineFormat::Bold).expect("bold");
        assert!(format_across(&mut b, bold, InlineFormat::Bold).is_some());
        assert!(matches!(&b[2], Block::Paragraph(spans) if spans.iter().all(|s| !s.bold)));
    }

    /// The bands a cross-paragraph selection paints are measured in the words on the
    /// page, not in the editable text's marks: `**`, `[x](y)` and the `#tag` marker are
    /// not drawn, so a byte of one is not a byte of the other.
    #[test]
    fn a_covering_range_maps_to_the_drawn_words() {
        // A link and bold marks lengthen the editable text; the drawn words do not have them.
        let block = blocks("see [the format page](../format-ideas/) and **this** too").remove(0);
        let whole = block_text(&block);
        let drawn = "see the format page and this too";
        // From the "and" to the end of the line.
        let start = whole.match_indices("and **this**").next().expect("the words").0;
        let (a, b) = drawn_covering(&block, start, whole.len()).expect("covered");
        assert_eq!(&drawn[a..b], "and this too");
        // The whole line covers every drawn word, marks and all.
        let (a, b) = drawn_covering(&block, 0, whole.len()).expect("covered");
        assert_eq!(&drawn[a..b], drawn);

        // A tag marker is not words either.
        let block = blocks("#idea hello world").remove(0);
        let whole = block_text(&block);
        let start = whole.match_indices("world").next().expect("the words").0;
        let (a, b) = drawn_covering(&block, start, whole.len()).expect("covered");
        assert_eq!(&"hello world"[a..b], "world");
    }

    /// `Across` endpoints count caret characters; `block_text` also holds the tag
    /// marker and an image's markup, which the caret's text does not.
    #[test]
    fn a_caret_position_maps_back_into_the_block_text() {
        let plain = blocks("plain words").remove(0);
        assert_eq!(byte_of_caret(&plain, 3), byte_of_char(&block_text(&plain), 3));

        let tagged = blocks("#idea hello").remove(0);
        let whole = block_text(&tagged);
        assert_eq!(byte_of_caret(&tagged, 0), byte_of_char(&whole, "#idea ".len()));
        assert_eq!(byte_of_caret(&tagged, 5), byte_of_char(&whole, "#idea ".len() + 5));

        let imaged = blocks("before ![a picture](assets/picture.png) after").remove(0);
        // Caret position 8 is just past the image's one caret character.
        let whole = block_text(&imaged);
        assert_eq!(byte_of_caret(&imaged, 8), whole.find(" after").expect("the words"));
    }

    #[test]
    fn inline_images_are_one_logical_caret_character() {
        let mut block = blocks("before ![a picture](assets/picture.png) after").remove(0);

        assert_eq!(caret_text(&block), "before \u{fffc} after");

        set_caret_text(&mut block, "before \u{fffc} and after");
        assert_eq!(block_text(&block), "before ![a picture](assets/picture.png) and after");

        set_caret_text(&mut block, "before  after");
        assert_eq!(block_text(&block), "before  after");
    }

    #[test]
    fn arrow_navigation_crosses_an_inline_image_without_selecting_it() {
        let text = "before \u{fffc} after";
        let image = text.chars().position(|c| c == INLINE_IMAGE_CHAR).unwrap();
        let range = egui::text::CCursorRange::two(egui::text::CCursor::new(image), egui::text::CCursor::new(image + 1));

        assert_eq!(inline_image_arrow_target(text, image, true), Some(image + 1));
        assert_eq!(inline_image_arrow_target(text, image + 1, false), Some(image));
        assert_eq!(inline_image_navigation_edge(text, range, true), Some(image + 1));
        assert_eq!(inline_image_navigation_edge(text, range, false), Some(image));
    }

    #[test]
    fn vertical_navigation_skips_a_whole_block_image() {
        let blocks = blocks("one\n\n![a picture](assets/picture.png)\n\nthree");
        assert_eq!(blocks.iter().filter(|b| matches!(b, Block::Image { .. })).count(), 1);
        assert_eq!(adjacent_writable(&blocks, 0, true, ""), Some(2));
        assert_eq!(adjacent_writable(&blocks, 2, false, ""), Some(0));
    }

    #[test]
    fn whole_block_image_has_a_caret_boundary_before_it() {
        let blocks = blocks("one\n\n![a picture](assets/picture.png)\n\nthree");
        assert_eq!(next_whole_block_image(&blocks, 0, ""), Some(1));
        assert_eq!(previous_whole_block_image(&blocks, 2, ""), Some(1));
        assert_eq!(previous_writable_block(&blocks, 1, ""), Some(0));
        assert_eq!(next_writable_block(&blocks, 1, ""), Some(2));
        assert_eq!(next_whole_block_image(&blocks, 2, ""), None);
    }

    #[test]
    fn inline_image_mapping_preserves_tags() {
        let mut block = blocks("#important before ![a picture](assets/picture.png) after").remove(0);

        assert_eq!(caret_text(&block), "before \u{fffc} after");
        set_caret_text(&mut block, "before \u{fffc} now");
        assert_eq!(block_text(&block), "#important before ![a picture](assets/picture.png) now");
    }

    /// A click is measured against the words on the page; the caret lives in the text with
    /// its marks. The walk between the two is checked here against the text the renderer
    /// actually writes, character for character, so a click after a link lands in the word
    /// under the pointer and not a dozen characters before it.
    #[test]
    fn a_character_of_the_drawn_words_maps_to_the_same_one_with_the_marks() {
        let line = "see [the format page](../format-ideas/) and **this** too";
        let spans = parse_inline(line);
        let drawn: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(drawn, "see the format page and this too", "what the page shows");
        assert_eq!(spans_to_editor(&spans), line, "and what the caret writes in");

        // Every drawn character comes back as the same character in the marked line.
        let marked: Vec<char> = line.chars().collect();
        for (i, c) in drawn.chars().enumerate() {
            let at = over_marks(&spans, i);
            assert_eq!(marked.get(at), Some(&c), "drawn character {i} ({c:?}) landed on {at} in {line:?}");
        }
        // Past the last word is the end of the line, marks and all.
        assert_eq!(over_marks(&spans, drawn.chars().count()), marked.len());
        // The old way, measuring against the marked text, would have been this far out.
        let after_link = "see the format page".chars().count();
        assert_eq!(over_marks(&spans, after_link) - after_link, 20, "just past the link, the two texts are twenty characters apart");

        // A line with no marks at all maps one to one.
        let plain = parse_inline("just words");
        assert_eq!(over_marks(&plain, 5), 5);
    }

    #[test]
    fn a_table_copied_from_another_app_comes_back_as_a_table() {
        // What OneNote puts on the clipboard: a tab between cells, a newline between rows.
        let clip = "test\t\t\t\n\t\t\t\n\ttest\t\t\n";
        match pasted(clip) {
            Pasted::Table(rows) => {
                assert_eq!(rows.len(), 3, "three rows");
                assert!(rows.iter().all(|r| r.len() == 4), "every row four cells wide: {rows:?}");
                assert_eq!(rows.first().and_then(|r| r.first()).map(String::as_str), Some("test"));
                assert_eq!(rows.get(2).and_then(|r| r.get(1)).map(String::as_str), Some("test"));
            }
            other => panic!("expected a table, got {other:?}"),
        }
        let blocks = pasted_blocks(pasted(clip));
        assert!(matches!(blocks.first(), Some(Block::Table { rows, .. }) if rows.len() == 2), "a heading row and two rows: {blocks:?}");

        // Several lines with no tabs are paragraphs, not a table of one column.
        assert_eq!(pasted("one\ntwo"), Pasted::Paragraphs(vec!["one".into(), "two".into()]));
        assert_eq!(pasted_blocks(pasted("one\ntwo")).len(), 2);
        // One line, tabs or not, belongs inside the line the caret is in.
        assert_eq!(pasted("just words"), Pasted::Inline);
        assert_eq!(pasted("a\tb"), Pasted::Inline);
        assert_eq!(pasted(""), Pasted::Inline);
        assert!(pasted_blocks(Pasted::Inline).is_empty());
        // The HTML flavour is the better one, and the one OneNote puts there with the
        // Windows header. Taken whole, it keeps the cells and what is inside them.
        let clip_html = "Version:1.0\r\nStartHTML:0000000105\r\nStartFragment:0000000527\r\n<html><body>\r\n<!--StartFragment-->\
<table border=1><tr><td><p>test</p></td><td><p>&nbsp;</p></td></tr><tr><td><p>two</p></td><td><p><b>bold</b></p></td></tr></table>\
<!--EndFragment--></body></html>";
        match pasted(clip_html) {
            Pasted::Markup(html) => {
                assert!(html.starts_with("<table"), "the fragment, not the header: {html:.40}");
                assert!(!html.contains("Version:1.0"));
            }
            other => panic!("expected markup, got {other:?}"),
        }
        let blocks = pasted_blocks(pasted(clip_html));
        assert!(matches!(blocks.first(), Some(Block::Table { .. })), "a table came back: {blocks:?}");
        let inline_html = "<strong>bold</strong> <a href=\"https://example.com\">linked</a>";
        let blocks = pasted_blocks(pasted(inline_html));
        let keeps_format = matches!(blocks.first(), Some(Block::Paragraph(spans)) if spans.iter().any(|s| s.bold && s.text == "bold") && spans.iter().any(|s| s.link.as_deref() == Some("https://example.com")));
        assert!(keeps_format, "rich inline clipboard keeps emphasis and links: {blocks:?}");
        // Plain words that merely start with a bracket are not markup.
        assert_eq!(pasted("<- look at that\nand this"), Pasted::Paragraphs(vec!["<- look at that".into(), "and this".into()]));

        // Windows line endings are the usual case and must not leave stray returns.
        assert_eq!(pasted("one\r\ntwo\r\n"), Pasted::Paragraphs(vec!["one".into(), "two".into()]));
    }

    #[test]
    fn a_direction_over_several_paragraphs_goes_on_each_of_them() {
        use notecraft_model::Dir;
        let mut bs = blocks("one\n\ntwo\n\nthree\n");
        assert_eq!(bs.len(), 3);
        // Selecting a block of the page and asking for right to left marks every paragraph
        // in it, because a paragraph is the only thing that carries a direction.
        assert!(set_direction_over(&mut bs, 0..=1, Some(Dir::Rtl)));
        let marked: Vec<bool> = bs.iter().map(|b| notecraft_model::direction(&block_text(b)).is_rtl()).collect();
        assert_eq!(marked, vec![true, true, false], "the third was not in the selection");

        // Asking for auto over the same run takes them all off again.
        assert!(set_direction_over(&mut bs, 0..=2, None));
        assert!(bs.iter().all(|b| !block_text(b).starts_with('\u{200F}')), "every mark is gone");
        // A range past the end is not a panic, and changes nothing.
        assert!(!set_direction_over(&mut bs, 9..=12, Some(Dir::Rtl)));
    }

    #[test]
    fn a_block_gives_up_its_text_and_takes_it_back() {
        let mut b = blocks("# A **bold** title\n");
        let text = block_text(b.first().expect("one"));
        assert_eq!(text, "A **bold** title");
        set_block_text(b.first_mut().expect("one"), "Changed *here*");
        assert_eq!(block_text(b.first().expect("one")), "Changed *here*");
        // The block stays a heading.
        assert!(matches!(b.first(), Some(Block::Heading { level: 1, .. })));
        // A code block keeps its text as it is.
        let mut code = blocks("```rust\nlet x = 1;\n```\n");
        assert_eq!(block_text(code.first().expect("one")).trim(), "let x = 1;");
        set_block_text(code.first_mut().expect("one"), "let y = 2;");
        assert!(matches!(code.first(), Some(Block::Code { .. })));
        assert_eq!(block_text(code.first().expect("one")), "let y = 2;");
    }

    #[test]
    fn a_rule_and_a_picture_are_not_written_in_but_a_table_is() {
        for md in [
            "---
",
            "![alt](x.png)
",
        ] {
            let b = blocks(md);
            assert!(b.first().is_some_and(|b| !is_writable(b)), "{md}");
        }
        assert!(
            blocks(
                "text
"
            )
            .first()
            .is_some_and(is_writable)
        );
        assert!(
            blocks(
                "- item
"
            )
            .first()
            .is_some_and(is_writable)
        );
        // A table takes the caret now, cell by cell.
        assert!(
            blocks(
                "| a |
| --- |
| 1 |
"
            )
            .first()
            .is_some_and(is_writable),
            "a table is written in"
        );

        // It is not one line of words, though, so it cannot be turned into a heading:
        // that would throw the table away.
        for md in [
            "| a |
| --- |
| 1 |
",
            "---
",
            "![alt](x.png)
",
        ] {
            assert!(blocks(md).first().is_some_and(|b| !is_one_line_of_words(b)), "{md} keeps its kind");
        }
        for md in [
            "text
", "- item
", "# head
", "> quote
",
        ] {
            assert!(blocks(md).first().is_some_and(is_one_line_of_words), "{md} can become another kind");
        }
    }

    #[test]
    fn a_tables_shape_is_changed_round_the_caret() {
        let table = || blocks("| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n").remove(0);

        // A row below the caret's row, and the caret follows it.
        let mut b = table();
        assert_eq!(edit_table(&mut b, (1, 0), TableEdit::InsertRow(false)), Some((2, 0)));
        assert_eq!(table_size(&b), Some((4, 2)));
        assert_eq!(cell_text(&b, (2, 0)).as_deref(), Some(""), "the new row is empty");
        assert_eq!(cell_text(&b, (3, 0)).as_deref(), Some("3"), "and what was below it moved down");

        // A row above. Above the heading means under it: the heading is the top.
        let mut b = table();
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::InsertRow(true)), Some((1, 0)));
        assert_eq!(cell_text(&b, (0, 0)).as_deref(), Some("A"), "the heading is still the heading");
        assert_eq!(cell_text(&b, (1, 0)).as_deref(), Some(""));

        // Columns, either side.
        let mut b = table();
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::InsertColumn(true)), Some((0, 0)));
        assert_eq!(table_size(&b), Some((3, 3)));
        assert_eq!(cell_text(&b, (0, 1)).as_deref(), Some("A"), "what was first moved right");
        let mut b = table();
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::InsertColumn(false)), Some((0, 1)));
        assert_eq!(cell_text(&b, (0, 2)).as_deref(), Some("B"));

        // Deleting a row, and the heading refusing to go.
        let mut b = table();
        assert_eq!(edit_table(&mut b, (1, 0), TableEdit::DeleteRow), Some((1, 0)));
        assert_eq!(table_size(&b), Some((2, 2)));
        assert_eq!(cell_text(&b, (1, 0)).as_deref(), Some("3"));
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::DeleteRow), None, "a table without a heading is not a table");

        // Deleting a column, and the last one refusing to go.
        let mut b = table();
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::DeleteColumn), Some((0, 0)));
        assert_eq!(table_size(&b), Some((3, 1)));
        assert_eq!(cell_text(&b, (0, 0)).as_deref(), Some("B"));
        assert_eq!(edit_table(&mut b, (0, 0), TableEdit::DeleteColumn), None, "the last column stays");

        // Anything that is not a table says no rather than guessing.
        let mut p = blocks("just words\n").remove(0);
        assert_eq!(edit_table(&mut p, (0, 0), TableEdit::DeleteRow), None);
    }

    #[test]
    fn a_heading_that_only_repeats_the_title_is_left_to_the_title_area() {
        let b = blocks(
            "# Format ideas

words

# Format ideas
",
        );
        assert!(is_title_echo(&b, 0, "Format ideas"), "the page opens with its own name");
        assert!(is_title_echo(&b, 0, "  Format ideas  "), "spaces round the title do not matter");
        assert!(!is_title_echo(&b, 2, "Format ideas"), "only the first block; later headings are the writer's");
        assert!(!is_title_echo(&b, 1, "Format ideas"), "a paragraph is not a heading");
        assert!(!is_title_echo(&b, 0, "Something else"), "a heading that says something else stays");
        assert!(!is_title_echo(&b, 0, ""), "a page with no title hides nothing");

        // A heading two levels down is the writer's, whatever it says.
        let two = blocks(
            "## Format ideas
",
        );
        assert!(!is_title_echo(&two, 0, "Format ideas"));
    }

    #[test]
    fn the_date_under_the_title_is_a_stamp_and_never_shows_its_marks() {
        let b = blocks(
            "# Title

Monday, 19 October 2026, 05:00

ordinary words
",
        );
        assert!(is_page_stamp(&b, 1), "the line under the title is the stamp");
        assert!(!is_page_stamp(&b, 2), "and nothing else is");
        assert!(!is_page_stamp(&b, 0));

        // A page written before the stamp was plain has it in italics. The caret takes
        // the words without the marks, so clicking the line never shows a markdown page.
        let old = blocks(
            "# Title

*Monday, 19 October 2026, 05:00*
",
        );
        assert!(is_page_stamp(&old, 1));
        assert_eq!(text_for_caret(&old, 1), "Monday, 19 October 2026, 05:00");
        assert_eq!(caret_text(&old[1]), "*Monday, 19 October 2026, 05:00*", "while any other line keeps its marks");

        // A paragraph that is not a date is an ordinary paragraph wherever it sits.
        let not = blocks(
            "# Title

A note to self.
",
        );
        assert!(!is_page_stamp(&not, 1));
    }

    #[test]
    fn a_picked_size_is_the_size_of_the_table() {
        let mut blocks = blocks("words\n");
        assert!(insert_table(&mut blocks, 1, 5, 4), "four columns, five rows");
        let table = blocks.get(1).expect("the table");
        assert_eq!(table_size(table), Some((5, 4)), "the heading counts as one of the five");
        assert_eq!(cell_text(table, (0, 0)).as_deref(), Some(""), "and every cell starts empty");
        assert_eq!(cell_text(table, (4, 3)).as_deref(), Some(""));
    }

    #[test]
    fn a_block_of_cells_is_the_same_block_whichever_way_it_was_dragged() {
        let caret = |cell, span| Caret { block: 0, text: String::new(), focus: false, selection: (0, 0), place: None, cell, span, inline: None };
        assert_eq!(cell_range(&caret(Some((1, 0)), Some((2, 3)))), Some(((1, 0), (2, 3))));
        assert_eq!(cell_range(&caret(Some((2, 3)), Some((1, 0)))), Some(((1, 0), (2, 3))), "dragged back the other way");
        assert_eq!(cell_range(&caret(Some((2, 0)), Some((1, 3)))), Some(((1, 0), (2, 3))), "and across the diagonal");
        assert_eq!(cell_range(&caret(Some((1, 1)), None)), None, "one cell is not a block");
        assert_eq!(cell_range(&caret(None, None)), None);
    }

    #[test]
    fn the_select_buttons_reach_as_far_as_the_table_does() {
        let b = blocks("| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n").remove(0);
        assert_eq!(span_of(&b, (1, 1), TableSpan::Whole), Some(((0, 0), (2, 2))), "the heading is part of the table");
        assert_eq!(span_of(&b, (1, 1), TableSpan::Row), Some(((1, 0), (1, 2))));
        assert_eq!(span_of(&b, (1, 1), TableSpan::Column), Some(((0, 1), (2, 1))));
        let p = blocks("just words\n").remove(0);
        assert_eq!(span_of(&p, (0, 0), TableSpan::Whole), None, "nothing to pick outside a table");
    }

    #[test]
    fn a_ragged_table_is_squared_up_before_it_is_changed() {
        // A table read from a file can have rows of different lengths; an edit must not
        // leave it raggeder still.
        let mut b = blocks("| A | B | C |\n|---|---|---|\n| 1 |\n").remove(0);
        edit_table(&mut b, (1, 0), TableEdit::InsertRow(false));
        let (rows, columns) = table_size(&b).expect("a table");
        assert_eq!(columns, 3);
        for r in 0..rows {
            for c in 0..columns {
                assert!(cell_text(&b, (r, c)).is_some(), "every cell of {rows} by {columns} is there: {r},{c}");
            }
        }
    }

    #[test]
    fn enter_adds_the_right_kind_of_block() {
        assert!(matches!(next_block(&Block::Paragraph(Vec::new())), Block::Paragraph(_)));
        let item = Block::Item { ordered: true, number: 2, task: None, depth: 1, spans: Vec::new() };
        match next_block(&item) {
            Block::Item { ordered, number, depth, .. } => assert_eq!((ordered, number, depth), (true, 3, 1)),
            other => panic!("{other:?}"),
        }
        let task = Block::Item { ordered: false, number: 1, task: Some(true), depth: 0, spans: Vec::new() };
        match next_block(&task) {
            Block::Item { task, .. } => assert_eq!(task, Some(false), "a new to-do starts unticked"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn numbers_follow_the_list() {
        let mut b = blocks("1. one\n2. two\n3. three\n");
        b.insert(1, Block::Item { ordered: true, number: 99, task: None, depth: 0, spans: parse_inline("new") });
        renumber(&mut b);
        let numbers: Vec<usize> = b
            .iter()
            .filter_map(|x| match x {
                Block::Item { number, .. } => Some(*number),
                _ => None,
            })
            .collect();
        assert_eq!(numbers, vec![1, 2, 3, 4]);
        // A paragraph between two lists starts the numbering again.
        let mut split = blocks("1. a\n\ntext\n\n1. b\n");
        renumber(&mut split);
        let numbers: Vec<usize> = split
            .iter()
            .filter_map(|x| match x {
                Block::Item { number, .. } => Some(*number),
                _ => None,
            })
            .collect();
        assert_eq!(numbers, vec![1, 1]);
    }

    #[test]
    fn typing_is_written_back_in_the_page_format() {
        let b = blocks("# Title\n\nWords\n");
        assert_eq!(to_body(PageFormat::Cmgfm, &b), "# Title\n\nWords\n");
        let html = to_body(PageFormat::Html, &b);
        assert!(html.contains("<h1>Title</h1>") && html.contains("<p>Words</p>"), "{html}");
    }

    /// Every page NoteCraft itself writes can be written on. If one of these fails, the
    /// user sees a page that is read-only for no good reason.
    #[test]
    fn the_pages_notecraft_writes_can_be_written_on() {
        for body in
            [notecraft_engine::sample::HTML_BODY, "<h1>Untitled page</h1>\n<p class=\"page-date\"><i>Saturday, 17 October 2026, 18:29</i></p>\n"]
        {
            let before = preview::blocks_for(PageFormat::Html, body);
            let after = preview::blocks_for(PageFormat::Html, &to_body(PageFormat::Html, &before));
            let first_difference = before.iter().zip(after.iter()).find(|(a, b)| a != b);
            assert!(
                round_trips(PageFormat::Html, body),
                "this page should be writable.\nfirst block that changed:\n{first_difference:#?}\nblocks before: {}, after: {}",
                before.len(),
                after.len()
            );
        }
        assert!(round_trips(PageFormat::Cmgfm, notecraft_engine::sample::MD_BODY));
    }

    #[test]
    fn a_page_the_editor_would_damage_is_left_alone() {
        // What NoteCraft writes round-trips.
        assert!(round_trips(PageFormat::Html, "<h1>T</h1>\n<p>Words with <b>bold</b>.</p>\n"));
        assert!(round_trips(PageFormat::Cmgfm, "# T\n\n- one\n- two\n"));
        // Markup the block model does not know does not round-trip, so it is read-only.
        assert!(!round_trips(PageFormat::Html, "<p>Words</p><aside data-x=\"1\"><p>side note</p></aside>"));
        assert!(!round_trips(PageFormat::Html, "<p><span style=\"font-family: Noto Sans; color: red\">words</span></p>"));
        assert!(round_trips(PageFormat::Html, "<p><span style=\"font-family: Noto Sans; font-size: 18pt\">words</span></p>"));
    }

    /// The actions are pure state changes, so they are tested without a window.
    #[test]
    fn splitting_joining_and_indenting() {
        let mut b = blocks("- one\n- two\n");
        let mut caret =
            Some(Caret { block: 0, text: "one".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        let mut changed = Changed::default();
        apply(Action::Split(0), &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 3, "a new item after the first");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1));
        assert!(changed.blocks && changed.caret);

        // Enter in the middle of a line splits it; the caret goes with the second half.
        let mut split = blocks("one two\n");
        let mut caret =
            Some(Caret { block: 0, text: "one two".into(), focus: false, selection: (3, 3), place: None, cell: None, span: None, inline: None });
        apply(Action::Split(0), &mut split, &mut caret, &mut changed);
        assert_eq!(block_text(split.first().expect("first")), "one");
        assert_eq!(block_text(split.get(1).expect("second")), " two");
        assert_eq!(caret.as_ref().map(|c| (c.block, c.place)), Some((1, Some(0))), "the caret is at the start of the second half");

        // Enter at the very start pushes the words down and leaves an empty line above.
        let mut start = blocks("words\n");
        let mut caret =
            Some(Caret { block: 0, text: "words".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        apply(Action::Split(0), &mut start, &mut caret, &mut changed);
        assert_eq!(start.len(), 2);
        assert_eq!(block_text(start.first().expect("first")), "", "an empty line above");
        assert_eq!(block_text(start.get(1).expect("second")), "words", "the words dropped");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1), "the caret stayed with the words");

        // Enter at the end of a heading starts a paragraph, not another heading.
        let mut head = blocks("# Title\n");
        let mut caret =
            Some(Caret { block: 0, text: "Title".into(), focus: false, selection: (5, 5), place: None, cell: None, span: None, inline: None });
        apply(Action::Split(0), &mut head, &mut caret, &mut changed);
        assert!(matches!(head.first(), Some(Block::Heading { .. })));
        assert!(matches!(head.get(1), Some(Block::Paragraph(_))), "{:?}", head.get(1));

        apply(Action::Indent(1, true), &mut b, &mut caret, &mut changed);
        assert!(matches!(b.get(1), Some(Block::Item { depth: 1, .. })));
        apply(Action::Indent(1, false), &mut b, &mut caret, &mut changed);
        assert!(matches!(b.get(1), Some(Block::Item { depth: 0, .. })));

        // Enter on the empty item ends the list.
        apply(Action::Split(1), &mut b, &mut caret, &mut changed);
        assert!(matches!(b.get(1), Some(Block::Paragraph(_))), "{:?}", b.get(1));

        // Backspace at the start joins the text onto the block above.
        let mut two = blocks("first\n\nsecond\n");
        let mut caret =
            Some(Caret { block: 1, text: "second".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        apply(Action::JoinBack(1), &mut two, &mut caret, &mut changed);
        assert_eq!(two.len(), 1);
        assert_eq!(block_text(two.first().expect("one")), "firstsecond");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(0));

        // Backspace at the start of the first block turns a heading into a paragraph.
        let mut head = blocks("# Title\n");
        let mut caret =
            Some(Caret { block: 0, text: "Title".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        apply(Action::JoinBack(0), &mut head, &mut caret, &mut changed);
        assert!(matches!(head.first(), Some(Block::Paragraph(_))));
        assert_eq!(block_text(head.first().expect("one")), "Title");
    }

    /// What the ribbon's B, I and S buttons do to the words under the caret.
    #[test]
    fn marks_go_around_the_selection_and_come_off_again() {
        // Nothing selected: the marks go in and the caret sits between them.
        let (text, sel) = wrap_selection("hello", (5, 5), "**");
        assert_eq!(text, "hello****");
        assert_eq!(sel, (7, 7));
        // A selection is wrapped.
        let (text, sel) = wrap_selection("hello world", (0, 5), "**");
        assert_eq!(text, "**hello** world");
        assert_eq!((text.chars().skip(sel.0).take(sel.1 - sel.0).collect::<String>()).as_str(), "hello");
        // The same button again takes them off, whether they are inside or outside.
        assert_eq!(wrap_selection("**hello** world", (0, 9), "**").0, "hello world");
        assert_eq!(wrap_selection("**hello** world", (2, 7), "**").0, "hello world");
        // Italic and code use their own marks.
        assert_eq!(wrap_selection("x", (0, 1), "*").0, "*x*");
        assert_eq!(wrap_selection("x", (0, 1), "`").0, "`x`");
        // A selection that runs past the end does not panic.
        assert_eq!(wrap_selection("ab", (0, 99), "**").0, "**ab**");
        assert_eq!(wrap_selection("", (3, 1), "**").0, "****");
        // Characters outside the ASCII range are counted, not bytes.
        let (text, _) = wrap_selection("\u{4e2d}\u{6587}x", (0, 2), "**");
        assert_eq!(text, "**\u{4e2d}\u{6587}**x");
    }

    #[test]
    fn a_size_wrapper_keeps_its_unicode_selection() {
        let open = "<span style=\"font-size: 18pt\">";
        let (text, selection) = wrap_selection_with("aمرحبا z", (6, 1), open, "</span>");
        assert_eq!(text, format!("a{open}مرحبا</span> z"));
        assert_eq!(text.chars().skip(selection.0).take(selection.1 - selection.0).collect::<String>(), "مرحبا");
    }

    /// The two code styles are one block with its lines numbered or not; changing between them keeps the words and the language.
    #[test]
    fn code_changes_between_plain_and_numbered_and_keeps_its_words() {
        let mut b = blocks(
            "```rust
*not italic*
```
",
        );
        assert_eq!(block_kind(&b[0]), "codeblock");
        assert!(set_kind(&mut b, 0, "codenumbers"));
        assert!(matches!(&b[0], Block::Code { lang, text } if lang == "123|rust" && text == "*not italic*"), "{:?}", b[0]);
        assert_eq!(block_kind(&b[0]), "codenumbers");
        assert!(set_kind(&mut b, 0, "codeblock"));
        assert!(matches!(&b[0], Block::Code { lang, .. } if lang == "rust"));
        // The same style again is the way back to ordinary text.
        assert!(set_kind(&mut b, 0, "codeblock"));
        assert_eq!(block_kind(&b[0]), "p");
    }

    /// Bold, italic and the rest are toggles, and words that are partly marked already do not break them.
    #[test]
    fn a_mark_is_a_toggle_over_words_that_are_partly_marked() {
        let md = |b: &[Block]| notecraft_formats::render::blocks_to_markdown(b).trim().to_string();
        for (mark, open) in [("bold", "**"), ("italic", "*"), ("strike", "~~")] {
            // Partly marked: it goes on all of it, once, and the same button takes it off again.
            let mut b = blocks(
                "a **b** c
",
            );
            assert!(toggle_mark(&mut b, 0..1, mark));
            let on = md(&b);
            let spans = parse_inline(&on);
            let has = |s: &Span| match mark {
                "bold" => s.bold,
                "italic" => s.italic,
                _ => s.strike,
            };
            // (A space between two runs is written outside their marks, so it is not itself marked.)
            assert!(spans.iter().filter(|s| !s.text.trim().is_empty()).all(has), "{mark} is on all of it ({open}): {on}");
            assert_eq!(spans.iter().map(|s| s.text.as_str()).collect::<String>(), "a b c", "{mark} lost no words: {on}");
            assert!(toggle_mark(&mut b, 0..1, mark));
            // Only that mark comes off: the bold on b stays for the others.
            assert_eq!(md(&b), if mark == "bold" { "a b c" } else { "a **b** c" }, "{mark} came off everywhere");
            // Marked at both ends but not all the way through: that is not 'already marked'.
            let mut b = blocks(
                "**a** and **b**
",
            );
            assert!(toggle_mark(&mut b, 0..1, "bold"));
            assert_eq!(md(&b), "**a and b**", "everything is bold now, none lost");
        }
        // Several blocks together: all or nothing, and what is not words is left alone.
        let mut b = blocks(
            "one

**two**

```
code
```
",
        );
        assert!(toggle_mark(&mut b, 0..3, "bold"));
        assert_eq!(
            md(&b),
            "**one**

**two**

```
code
```"
        );
        assert!(toggle_mark(&mut b, 0..3, "bold"));
        assert_eq!(
            md(&b),
            "one

two

```
code
```"
        );
        let mut only_code = blocks(
            "```
code
```
",
        );
        assert!(!toggle_mark(&mut only_code, 0..1, "bold"), "nothing to mark");
    }

    #[test]
    fn underline_and_scripts_toggle_across_mixed_blocks_without_changing_other_marks() {
        use notecraft_formats::markdown::Script;
        for (mark, script) in [("underline", None), ("subscript", Some(Script::Subscript)), ("superscript", Some(Script::Superscript))] {
            let mut blocks = blocks("plain **bold**\n\n*italic*\n");
            let original = blocks.clone();
            assert!(toggle_mark(&mut blocks, 0..2, mark));
            for block in &blocks {
                let spans = match block {
                    Block::Paragraph(spans) => spans,
                    _ => panic!("paragraph"),
                };
                assert!(spans.iter().all(|span| if mark == "underline" { span.underline } else { span.script == script }), "{mark}: {spans:?}");
            }
            assert!(toggle_mark(&mut blocks, 0..2, mark));
            assert_eq!(blocks, original, "{mark} must preserve the other marks");
        }
        let mut blocks = blocks("<sub>before</sub> plain\n");
        assert!(toggle_mark(&mut blocks, 0..1, "superscript"));
        let Block::Paragraph(spans) = &blocks[0] else { panic!("paragraph") };
        assert!(spans.iter().all(|span| span.script == Some(Script::Superscript)));
    }

    /// What the ribbon's Styles and Lists buttons do to the block under the caret.
    #[test]
    fn a_block_changes_kind_and_keeps_its_words() {
        let mut b = blocks("Some words\n");
        assert!(set_kind(&mut b, 0, "h1"));
        assert!(matches!(b.first(), Some(Block::Heading { level: 1, .. })));
        assert_eq!(block_text(b.first().expect("one")), "Some words");
        assert_eq!(block_kind(b.first().expect("one")), "h1");
        // The same kind again turns it back into ordinary text.
        assert!(set_kind(&mut b, 0, "h1"));
        assert_eq!(block_kind(b.first().expect("one")), "p");
        for (kind, check) in [
            ("bullet", "bullet"),
            ("number", "number"),
            ("todo", "todo"),
            ("quote", "quote"),
            ("h2", "h2"),
            ("h3", "h3"),
            ("h4", "h4"),
            ("h5", "h5"),
            ("h6", "h6"),
            ("codeblock", "codeblock"),
            ("codenumbers", "codenumbers"),
            ("p", "p"),
        ] {
            assert!(set_kind(&mut b, 0, kind), "{kind}");
            assert_eq!(block_kind(b.first().expect("one")), check, "{kind}");
            assert_eq!(block_text(b.first().expect("one")), "Some words", "{kind} lost the words");
            if kind == check && kind != "p" {
                set_kind(&mut b, 0, "p");
            }
        }
        // A table cannot become a heading.
        let mut table = blocks("| a |\n| --- |\n| 1 |\n");
        assert!(!set_kind(&mut table, 0, "h1"));
        assert!(!set_kind(&mut table, 9, "h1"), "a block that is not there");
    }

    /// A to-do box on the page is the user's to tick, and the page is written back with it.
    #[test]
    fn a_to_do_box_can_be_ticked_and_cleared() {
        let mut b = blocks("- [ ] buy milk\n- not a to-do\n\nwords\n");
        assert!(set_task(&mut b, 0, true));
        assert!(matches!(b.first(), Some(Block::Item { task: Some(true), .. })));
        assert_eq!(to_body(PageFormat::Cmgfm, &b).lines().next(), Some("- [x] buy milk"));
        assert!(to_body(PageFormat::Html, &b).contains("&#9745;"), "the HTML page carries a ticked box");
        assert!(set_task(&mut b, 0, false));
        assert_eq!(to_body(PageFormat::Cmgfm, &b).lines().next(), Some("- [ ] buy milk"));
        // A line with no box, a block that is not a list item, and a block that is not there.
        assert!(!set_task(&mut b, 1, true));
        assert!(!set_task(&mut b, 2, true));
        assert!(!set_task(&mut b, 99, true));
    }

    #[test]
    fn text_goes_in_where_the_caret_is() {
        assert_eq!(insert_at_caret("hello world", (5, 5), " there"), ("hello there world".to_string(), 11));
        // A selection is replaced.
        assert_eq!(insert_at_caret("hello world", (0, 5), "bye"), ("bye world".to_string(), 3));
        // Backwards selections, and positions past the end, are handled.
        assert_eq!(insert_at_caret("abc", (3, 1), "X"), ("aX".to_string(), 2));
        assert_eq!(insert_at_caret("abc", (99, 99), "X"), ("abcX".to_string(), 4));
        assert_eq!(insert_at_caret("", (0, 0), "X"), ("X".to_string(), 1));
        // Characters outside the ASCII range count as one.
        assert_eq!(insert_at_caret("\u{4e2d}\u{6587}", (1, 1), "X"), ("\u{4e2d}X\u{6587}".to_string(), 2));
    }

    #[test]
    fn whole_blocks_are_inserted_where_the_caret_is() {
        let mut b = blocks("one\n\ntwo\n");
        assert!(insert_block(&mut b, 1, "table"));
        assert!(matches!(b.get(1), Some(Block::Table { .. })));
        assert_eq!(b.len(), 3);
        assert!(insert_block(&mut b, 99, "diagram"), "past the end is the end");
        assert!(matches!(b.last(), Some(Block::Code { lang, .. }) if lang == "mermaid"));
        assert!(!insert_block(&mut b, 0, "nonsense"));
        // What goes in comes back out in both formats.
        let md = to_body(PageFormat::Cmgfm, &b);
        assert!(md.contains("| A | B | C |") && md.contains("```mermaid"), "{md}");
        let html = to_body(PageFormat::Html, &b);
        assert!(html.contains("<table>") && html.contains("class=\"diagram\""), "{html}");
    }

    #[test]
    fn clicking_below_the_page_writes_at_the_end() {
        let mut b = blocks("words\n");
        let mut caret = None;
        let mut changed = Changed::default();
        apply(Action::ClickBelow, &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 2, "a new paragraph at the end");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1));
        // Clicking again lands in the empty block instead of adding another.
        apply(Action::ClickBelow, &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 2);
        // On an empty page it starts the first block.
        let mut empty: Vec<Block> = Vec::new();
        let mut caret = None;
        apply(Action::ClickBelow, &mut empty, &mut caret, &mut changed);
        assert_eq!(empty.len(), 1);
        assert_eq!(caret.map(|c| c.block), Some(0));
    }

    #[test]
    fn the_caret_never_points_outside_the_page() {
        let mut b = blocks("one\n");
        let mut caret =
            Some(Caret { block: 9, text: "junk".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        let mut changed = Changed::default();
        apply(Action::Move(5), &mut b, &mut caret, &mut changed);
        assert_eq!(caret, None, "a move to a block that is not there leaves no caret");
        let mut caret =
            Some(Caret { block: 0, text: "one".into(), focus: false, selection: (0, 0), place: None, cell: None, span: None, inline: None });
        apply(Action::JoinBack(0), &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 1);
    }

    #[test]
    fn blocks_that_were_copied_come_back_whole_and_other_text_is_read_as_text() {
        let original =
            notecraft_formats::markdown::parse("# Copy me\n\n*the date*\n\n[a link](https://example.org)\n\n```mermaid\ngraph TD;\n  A-->B;\n```\n");
        let markdown = notecraft_formats::render::blocks_to_markdown(&original);
        remember_copied(&markdown, &original);
        // What comes back is the blocks, not lines of words with their marks showing.
        assert_eq!(blocks_to_paste(&markdown), original);
        assert_eq!(blocks_to_paste(&markdown.replace('\n', "\r\n")), original, "whatever the line ends of the clipboard");
        assert!(blocks_to_paste(&markdown).iter().any(|b| matches!(b, Block::Heading { .. })), "the heading is still a heading");
        assert!(blocks_to_paste(&markdown).iter().any(|b| matches!(b, Block::Code { lang, .. } if lang == "mermaid")), "and the diagram a diagram");
        // Other text, even one that starts alike, is read as the text it is.
        let other = format!("{markdown}and more");
        assert_ne!(blocks_to_paste(&other), original);
        assert!(blocks_to_paste("one line of words").is_empty(), "one line goes into the line the caret is in");
        let lines = blocks_to_paste("first\nsecond");
        assert_eq!(lines.len(), 2, "two lines are two blocks");
    }

    #[test]
    fn pasting_blocks_takes_the_place_of_an_empty_line_and_goes_after_a_full_one() {
        let new = notecraft_formats::markdown::parse("# Title\n\nbody\n");
        let mut blocks = vec![Block::Paragraph(Vec::new())];
        let mut caret = None;
        paste_at(&mut blocks, &mut caret, 0, new.clone());
        assert_eq!(blocks.len(), 2, "the empty line is replaced by the two blocks");
        assert!(matches!(blocks[0], Block::Heading { .. }));
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1), "the caret is after the last of them");

        let mut blocks = notecraft_formats::markdown::parse("words\n");
        let mut caret = None;
        paste_at(&mut blocks, &mut caret, 0, new);
        assert_eq!(blocks.len(), 3, "after the line that has words");
        assert!(matches!(blocks[1], Block::Heading { .. }));
    }
}

#[cfg(test)]
mod regression_tests;

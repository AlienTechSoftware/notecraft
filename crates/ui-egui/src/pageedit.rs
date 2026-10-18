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
use notecraft_formats::markdown::{Block, parse_inline};
use notecraft_formats::render::spans_to_markdown;
use notecraft_model::Tag;

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
}

/// The character of `text` a click at `x` lands on, given how the block is drawn. Used so a
/// click puts the caret where the pointer is, the way it does in any editor.
pub fn char_at(ui: &Ui, text: &str, size: f32, strong: bool, t: &Tokens, width: f32, offset: egui::Vec2) -> usize {
    let job = pagetext::inline_job(text, size, strong, t, width.max(1.0));
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    galley.cursor_from_pos(offset).index.0.min(text.chars().count())
}

/// Puts `marks` around the selection, or takes them away again when they are already there.
/// Returns the new text and the new selection.
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
    if !is_writable(block) {
        return false;
    }
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans.clone(),
        _ => parse_inline(&block_text(block)),
    };
    let new = match kind {
        "h1" => Block::Heading { level: 1, spans },
        "h2" => Block::Heading { level: 2, spans },
        "h3" => Block::Heading { level: 3, spans },
        "quote" => Block::Quote(spans),
        "bullet" => Block::Item { ordered: false, number: 0, task: None, depth: 0, spans },
        "number" => Block::Item { ordered: true, number: 1, task: None, depth: 0, spans },
        "todo" => Block::Item { ordered: false, number: 0, task: Some(false), depth: 0, spans },
        _ => Block::Paragraph(spans),
    };
    // Asking for what it already is turns it back into ordinary text, like a toggle.
    let same = std::mem::discriminant(&new) == std::mem::discriminant(block) && block_kind(block) == kind;
    if let Some(b) = blocks.get_mut(at) {
        *b = if same { Block::Paragraph(parse_inline(&block_text(&new))) } else { new };
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
        let text = Tag::set(&block_text(block), None);
        let spans = parse_inline(&text);
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

/// The name [`set_kind`] uses for what a block is now.
pub fn block_kind(block: &Block) -> &'static str {
    match block {
        Block::Heading { level: 1, .. } => "h1",
        Block::Heading { level: 2, .. } => "h2",
        Block::Heading { .. } => "h3",
        Block::Quote(_) => "quote",
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
}

/// The text of a block as it is edited: the inline marks only.
pub fn block_text(block: &Block) -> String {
    match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans_to_markdown(spans),
        Block::Code { text, .. } => text.clone(),
        Block::MathBlock(tex) => tex.clone(),
        Block::Image { alt, .. } => alt.clone(),
        Block::Table { .. } | Block::Rule => String::new(),
    }
}

/// Puts edited text back into a block, keeping what the block is.
pub fn set_block_text(block: &mut Block, text: &str) {
    let spans = parse_inline(text);
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
    !matches!(block, Block::Table { .. } | Block::Rule | Block::Image { .. })
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
        Block::Code { .. } | Block::MathBlock(_) => (13.5, false),
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
pub fn show(ui: &mut Ui, blocks: &mut Vec<Block>, caret: &mut Option<Caret>, page_dir: &std::path::Path, t: &Tokens) -> Changed {
    let mut changed = Changed::default();
    let mut action: Option<Action> = None;
    let count = blocks.len();
    for i in 0..count {
        let Some(block) = blocks.get(i).cloned() else { continue };
        let editing = caret.as_ref().is_some_and(|c| c.block == i);
        if editing && is_writable(&block) {
            let (size, strong) = size_of(&block);
            let Some(c) = caret.as_mut() else { continue };
            let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, width: f32| {
                ui.fonts_mut(|f| f.layout_job(pagetext::inline_job(text.as_str(), size, strong, t, width)))
            };
            let mut at_start = false;
            let mut selection = None;
            let mut ticked = None;
            let id = ui.make_persistent_id(("page-block", i));
            let resp = ui
                .horizontal_top(|ui| {
                    if let Some(done) = lead(ui, &block, t) {
                        ticked = Some(done);
                    }
                    let multiline = matches!(block, Block::Code { .. } | Block::MathBlock(_));
                    let out = if multiline {
                        egui::TextEdit::multiline(&mut c.text)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width())
                            .font(egui::FontId::monospace(size))
                            .show(ui)
                    } else {
                        egui::TextEdit::singleline(&mut c.text)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .desired_width(ui.available_width())
                            .layouter(&mut layouter)
                            .show(ui)
                    };
                    at_start = out.cursor_range.is_some_and(|r| r.primary.index.0 == 0 && r.secondary.index.0 == 0);
                    if let Some(r) = out.cursor_range {
                        selection = Some((r.primary.index.0, r.secondary.index.0));
                    }
                    out.response
                })
                .inner;
            if let Some(sel) = selection {
                c.selection = sel;
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
            if resp.changed() {
                // What was typed goes into the block at once, so the page and the file
                // follow the caret without waiting for it to leave the block.
                let typed = c.text.clone();
                if let Some(b) = blocks.get_mut(i) {
                    set_block_text(b, &typed);
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
                } else if up && i > 0 {
                    action = Some(Action::Move(i - 1));
                } else if down && i + 1 < count {
                    action = Some(Action::Move(i + 1));
                }
            }
        } else {
            // A finished block. Clicking it puts the caret in it, under the pointer, and a
            // to-do box on it can be ticked without putting the caret anywhere.
            let out = preview::show_block(ui, &block, i, page_dir, t, true);
            let resp = out.response;
            if let Some(done) = out.task_set {
                action = Some(Action::SetTask(i, done));
            } else if resp.clicked() && is_writable(&block) {
                let (size, strong) = size_of(&block);
                let text = block_text(&block);
                let at = resp
                    .interact_pointer_pos()
                    .map(|p| char_at(ui, &text, size, strong, t, resp.rect.width(), p - resp.rect.min - egui::vec2(lead_width(&block), 0.0)));
                action = Some(Action::MoveTo(i, at));
            }
            if resp.hovered() && is_writable(&block) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
        }
    }
    // Room under the page: a click there writes at the end, the way an empty page does.
    let (rect, below) = ui.allocate_exact_size(vec2(ui.available_width(), 140.0_f32.min(ui.available_height().max(40.0))), Sense::click());
    let _ = rect;
    if below.clicked() {
        action = Some(Action::ClickBelow);
    }
    if below.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    if let Some(a) = action {
        apply(a, blocks, caret, &mut changed);
    }
    changed
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
    Leave,
    ClickBelow,
}

fn commit(blocks: &mut [Block], caret: &Option<Caret>) {
    if let Some(c) = caret
        && let Some(b) = blocks.get_mut(c.block)
    {
        set_block_text(b, &c.text);
    }
}

fn apply(action: Action, blocks: &mut Vec<Block>, caret: &mut Option<Caret>, changed: &mut Changed) {
    commit(blocks, caret);
    match action {
        Action::Move(to) => {
            *caret = blocks.get(to).map(|b| Caret { block: to, text: block_text(b), focus: true, selection: (0, 0), place: None });
            changed.caret = true;
        }
        Action::SetTask(at, done) => {
            if set_task(blocks, at, done) {
                changed.blocks = true;
            }
        }
        Action::MoveTo(to, at) => {
            *caret = blocks.get(to).map(|b| Caret { block: to, text: block_text(b), focus: true, selection: (0, 0), place: at });
            changed.caret = true;
        }
        Action::Leave => {
            *caret = None;
            changed.caret = true;
        }
        // Enter splits the line at the caret, the way it does in Word and OneNote: what is
        // before the caret stays, what is after it moves down with the caret. At the very
        // start of a line that means an empty line above and the words drop.
        Action::Split(at) => {
            // Enter on an empty list item ends the list, as it does in every editor.
            if matches!(blocks.get(at), Some(Block::Item { spans, .. }) if spans.is_empty()) {
                if let Some(b) = blocks.get_mut(at) {
                    *b = Block::Paragraph(Vec::new());
                }
                *caret = Some(Caret { block: at, text: String::new(), focus: true, selection: (0, 0), place: None });
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
            let mut new = blocks.get(at).map(next_block).unwrap_or(Block::Paragraph(Vec::new()));
            if let Some(b) = blocks.get_mut(at) {
                set_block_text(b, &before);
                // A heading splits into a heading and a paragraph, as it does in Word.
                if matches!(b, Block::Heading { .. }) {
                    new = Block::Paragraph(Vec::new());
                }
            }
            set_block_text(&mut new, &after);
            blocks.insert(at + 1, new);
            renumber(blocks);
            *caret = Some(Caret { block: at + 1, text: after, focus: true, selection: (0, 0), place: Some(0) });
            changed.blocks = true;
            changed.caret = true;
        }
        Action::JoinBack(at) => {
            let text = caret.as_ref().map(|c| c.text.clone()).unwrap_or_default();
            if at == 0 {
                // The first block: a heading becomes a paragraph, nothing is lost.
                if let Some(Block::Heading { .. }) = blocks.first() {
                    if let Some(b) = blocks.get_mut(0) {
                        *b = Block::Paragraph(parse_inline(&text));
                    }
                    *caret = Some(Caret { block: 0, text, focus: true, selection: (0, 0), place: None });
                    changed.blocks = true;
                }
                return;
            }
            let previous = blocks.get(at - 1).cloned();
            match previous {
                Some(p) if is_writable(&p) => {
                    let mut joined = block_text(&p);
                    let at_end = joined.chars().count();
                    joined.push_str(&text);
                    if let Some(b) = blocks.get_mut(at - 1) {
                        set_block_text(b, &joined);
                    }
                    blocks.remove(at);
                    renumber(blocks);
                    *caret = Some(Caret { block: at - 1, text: joined, focus: true, selection: (0, 0), place: None });
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
        Action::ClickBelow => {
            let last_writable = blocks.iter().rposition(is_writable);
            match last_writable {
                // An empty last block is where the caret goes; otherwise a new one is added.
                Some(i) if blocks.get(i).map(block_text).unwrap_or_default().is_empty() => {
                    *caret = Some(Caret { block: i, text: String::new(), focus: true, selection: (0, 0), place: None });
                }
                _ => {
                    blocks.push(Block::Paragraph(Vec::new()));
                    *caret = Some(Caret { block: blocks.len() - 1, text: String::new(), focus: true, selection: (0, 0), place: None });
                    changed.blocks = true;
                }
            }
            changed.caret = true;
        }
    }
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
        resp.on_hover_text(tag.label());
    }
    match block {
        Block::Item { ordered, number, task, depth, .. } => {
            ui.add_space(8.0 + *depth as f32 * 18.0);
            match task {
                Some(done) => {
                    let mut v = *done;
                    if ui.checkbox(&mut v, "").changed() {
                        ticked = Some(v);
                    }
                }
                None => {
                    ui.label(egui::RichText::new(if *ordered { format!("{number}.") } else { "\u{2022}".to_string() }).color(t.muted));
                }
            }
        }
        Block::Quote(_) => {
            ui.add_space(6.0);
            ui.label(egui::RichText::new("\u{2588}").color(t.quote_bar));
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
    let blocks = preview::blocks_for(format, body);
    let written = to_body(format, &blocks);
    preview::blocks_for(format, &written) == blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_model::PageFormat;

    fn blocks(md: &str) -> Vec<Block> {
        notecraft_formats::markdown::parse(md)
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
    fn tables_rules_and_pictures_are_not_written_in_yet() {
        for md in ["| a |\n| --- |\n| 1 |\n", "---\n", "![alt](x.png)\n"] {
            let b = blocks(md);
            assert!(b.first().is_some_and(|b| !is_writable(b)), "{md}");
        }
        assert!(blocks("text\n").first().is_some_and(is_writable));
        assert!(blocks("- item\n").first().is_some_and(is_writable));
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
    }

    /// The actions are pure state changes, so they are tested without a window.
    #[test]
    fn splitting_joining_and_indenting() {
        let mut b = blocks("- one\n- two\n");
        let mut caret = Some(Caret { block: 0, text: "one".into(), focus: false, selection: (0, 0), place: None });
        let mut changed = Changed::default();
        apply(Action::Split(0), &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 3, "a new item after the first");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1));
        assert!(changed.blocks && changed.caret);

        // Enter in the middle of a line splits it; the caret goes with the second half.
        let mut split = blocks("one two\n");
        let mut caret = Some(Caret { block: 0, text: "one two".into(), focus: false, selection: (3, 3), place: None });
        apply(Action::Split(0), &mut split, &mut caret, &mut changed);
        assert_eq!(block_text(split.first().expect("first")), "one");
        assert_eq!(block_text(split.get(1).expect("second")), " two");
        assert_eq!(caret.as_ref().map(|c| (c.block, c.place)), Some((1, Some(0))), "the caret is at the start of the second half");

        // Enter at the very start pushes the words down and leaves an empty line above.
        let mut start = blocks("words\n");
        let mut caret = Some(Caret { block: 0, text: "words".into(), focus: false, selection: (0, 0), place: None });
        apply(Action::Split(0), &mut start, &mut caret, &mut changed);
        assert_eq!(start.len(), 2);
        assert_eq!(block_text(start.first().expect("first")), "", "an empty line above");
        assert_eq!(block_text(start.get(1).expect("second")), "words", "the words dropped");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(1), "the caret stayed with the words");

        // Enter at the end of a heading starts a paragraph, not another heading.
        let mut head = blocks("# Title\n");
        let mut caret = Some(Caret { block: 0, text: "Title".into(), focus: false, selection: (5, 5), place: None });
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
        let mut caret = Some(Caret { block: 1, text: "second".into(), focus: false, selection: (0, 0), place: None });
        apply(Action::JoinBack(1), &mut two, &mut caret, &mut changed);
        assert_eq!(two.len(), 1);
        assert_eq!(block_text(two.first().expect("one")), "firstsecond");
        assert_eq!(caret.as_ref().map(|c| c.block), Some(0));

        // Backspace at the start of the first block turns a heading into a paragraph.
        let mut head = blocks("# Title\n");
        let mut caret = Some(Caret { block: 0, text: "Title".into(), focus: false, selection: (0, 0), place: None });
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
        for (kind, check) in
            [("bullet", "bullet"), ("number", "number"), ("todo", "todo"), ("quote", "quote"), ("h2", "h2"), ("h3", "h3"), ("p", "p")]
        {
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
        let mut caret = Some(Caret { block: 9, text: "junk".into(), focus: false, selection: (0, 0), place: None });
        let mut changed = Changed::default();
        apply(Action::Move(5), &mut b, &mut caret, &mut changed);
        assert_eq!(caret, None, "a move to a block that is not there leaves no caret");
        let mut caret = Some(Caret { block: 0, text: "one".into(), focus: false, selection: (0, 0), place: None });
        apply(Action::JoinBack(0), &mut b, &mut caret, &mut changed);
        assert_eq!(b.len(), 1);
    }
}

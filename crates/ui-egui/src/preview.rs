//! The page preview: blocks from either format, drawn with egui.
//!
//! Bold is shown in a stronger colour because the default egui fonts have no bold
//! face (adding one means adding a font file and its licence). A click on a link is
//! reported by [`show`] for the window to follow. Equations and Mermaid diagrams are drawn;
//! a diagram that cannot be drawn shows its source and says why.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, RichText, Stroke, Ui};
use notecraft_formats::html;
use notecraft_formats::markdown::{self, Block, Span};
use notecraft_model::Dir;
use notecraft_model::PageFormat;
use notecraft_model::justify::{Justified, Strength};
use std::path::Path;

/// Blocks to draw for the editable part of a page in `format`.
pub fn blocks_for(format: PageFormat, body: &str) -> Vec<Block> {
    match format {
        PageFormat::Html => markdown::parse(&html::to_markdown(body)),
        PageFormat::Cmgfm => markdown::parse(body),
    }
}

/// A `file://` address egui's file loader can read, for an attachment link like
/// `assets/pic.png` relative to the page folder. Only links under `assets/` are
/// resolved; anything else (a web address, a path that climbs out) gives `None`.
pub fn image_uri(page_dir: &Path, src: &str) -> Option<String> {
    let rel = src.strip_prefix("assets/")?;
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') || rel.starts_with('/') {
        return None;
    }
    let full = notecraft_store::tree::display_path(&page_dir.join("assets").join(rel));
    Some(file_uri(&full))
}

/// `file:///C:/dir/a%20b.png` or `file:///home/me/a%20b.png`: the form egui's file
/// loader reads on every platform (three slashes, spaces and symbols percent-encoded).
fn file_uri(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = if p.starts_with('/') { p } else { format!("/{p}") };
    let mut out = String::from("file://");
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The same formats [`job`] uses, as ranges into the line's own text, for the bidi painter.
fn pieces(spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens) -> Vec<crate::bidi::Piece> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for s in spans {
        if s.image.is_some() {
            continue;
        }
        let text = if s.math { format!("${}$", s.text) } else { s.text.clone() };
        let end = at + text.len();
        out.push((at..end, format_of(s, size, color, tokens)));
        at = end;
    }
    out
}

/// How one span is drawn.
fn format_of(s: &Span, size: f32, color: egui::Color32, tokens: &Tokens) -> TextFormat {
    let mono = s.code || s.math;
    let c = if s.link.is_some() {
        tokens.link
    } else if s.bold {
        tokens.strong
    } else {
        color
    };
    TextFormat {
        font_id: if mono { FontId::monospace(size * 0.92) } else { FontId::proportional(size) },
        color: c,
        italics: s.italic || s.math,
        underline: if s.link.is_some() { Stroke::new(1.0, c) } else { Stroke::NONE },
        strikethrough: if s.strike { Stroke::new(1.0, c) } else { Stroke::NONE },
        background: if s.code { tokens.code_bg } else { egui::Color32::TRANSPARENT },
        ..TextFormat::default()
    }
}

pub fn job(ui: &Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens, page: Option<Dir>) -> LayoutJob {
    let mut job = LayoutJob { halign: crate::pagetext::halign(&text_of(spans), page), ..LayoutJob::default() };
    job.wrap.max_width = ui.available_width();
    for s in spans {
        if s.image.is_some() {
            continue;
        }
        let mono = s.code || s.math;
        let c = if s.link.is_some() {
            tokens.link
        } else if s.bold {
            tokens.strong
        } else {
            color
        };
        let format = TextFormat {
            font_id: if mono { FontId::monospace(size * 0.92) } else { FontId::proportional(size) },
            color: c,
            italics: s.italic || s.math,
            underline: if s.link.is_some() { Stroke::new(1.0, c) } else { Stroke::NONE },
            strikethrough: if s.strike { Stroke::new(1.0, c) } else { Stroke::NONE },
            background: if s.code { tokens.code_bg } else { egui::Color32::TRANSPARENT },
            ..TextFormat::default()
        };
        let text = if s.math { format!("${}$", s.text) } else { s.text.clone() };
        job.append(&text, 0.0, format);
    }
    job
}

/// The rectangles to lay behind a selection inside a galley egui laid out: one to a row,
/// so a paragraph wrapped over three lines gets three bands and not one tall box.
///
/// `a` and `b` are bytes of the galley's own text. A row that the selection does not touch
/// gives nothing; a row it covers end to end gives the whole row.
fn selection_bands(galley: &egui::Galley, a: usize, b: usize) -> Vec<egui::Rect> {
    let text = galley.text();
    // epaint counts in characters, the page counts in bytes.
    let chars_to = |at: usize| text.get(..at.min(text.len())).map(|t| t.chars().count()).unwrap_or(0);
    let (a, b) = (chars_to(a.min(b)), chars_to(a.max(b)));
    if a == b {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut before = 0usize;
    for placed in &galley.rows {
        let mut covered: Option<(f32, f32)> = None;
        for (n, glyph) in placed.row.glyphs.iter().enumerate() {
            let at = before + n;
            if at < a || at >= b {
                continue;
            }
            let (x0, x1) = (glyph.pos.x, glyph.pos.x + glyph.advance_width);
            covered = Some(match covered {
                Some((l, r)) => (l.min(x0), r.max(x1)),
                None => (x0, x1),
            });
        }
        // A row that ends a paragraph swallowed its newline, which still counts as a
        // character of the text; otherwise the next row would start one short.
        before += placed.char_count_including_newline().0;
        if let Some((left, right)) = covered {
            let top = placed.pos.y;
            out.push(egui::Rect::from_min_max(
                egui::pos2(placed.pos.x + left, top),
                egui::pos2(placed.pos.x + right.max(left + 1.0), top + placed.row.size.y),
            ));
        }
    }
    out
}

/// Which way a block's words read, given the direction of the page they are on.
pub fn dir_of(spans: &[Span], page: Option<Dir>) -> Dir {
    notecraft_model::direction_in(&text_of(spans), page)
}

/// Draws spans as wrapped text. A paragraph with inline math flows text and formulas
/// together; one without math is a single label. Right-to-left text hangs from the right
/// edge, which is where a reader of Arabic or Hebrew starts.
///
/// With `justify`, every line of the paragraph but the last is made to reach the column.
/// Which way that is done follows the words. A paragraph that reads right to left is
/// stretched by kashida, the way a scribe lengthens the joins inside the words, so the
/// spaces stay as they were and the page keeps an even colour. A paragraph that reads left
/// to right has no such joins, so it is justified the Latin way, by widening the spaces,
/// which is what egui's own layout does.
#[allow(clippy::too_many_arguments)]
fn draw_spans(
    ui: &mut Ui,
    spans: &[Span],
    size: f32,
    color: egui::Color32,
    tokens: &Tokens,
    page: Option<Dir>,
    justify: Option<Strength>,
    // The bytes of this block's words that are selected, when a selection reaches into it
    // from a drag that crossed more than one paragraph.
    selected: Option<(usize, usize)>,
) {
    if !spans.iter().any(|s| s.math) {
        // A line that mixes the two directions is drawn by egui in the wrong order, and a
        // paragraph that reads right to left is not wrapped by it, so both go through the
        // bidi painter: parley says where each run belongs, egui still draws them, and the
        // marks inside the words are kept.
        let text = text_of(spans);
        if crate::bidi::paints(&text, page) {
            let pieces = pieces(spans, size, color, tokens);
            let width = ui.available_width();
            // Kashida is for the joins of Arabic and Persian letters. A line that mixes the
            // two directions and reads left to right has nothing to stretch, and the bidi
            // painter has no way to widen its spaces, so it is drawn ragged; see
            // `docs/gaps.md`.
            let stretch = justify.filter(|_| dir_of(spans, page).is_rtl());
            let sel = selected.map(|range| crate::bidi::Selected { range, color: tokens.selection });
            let rect = crate::bidi::show(ui, &text, &pieces, size, width, page, stretch, sel);
            let _ = rect;
            return;
        }
        let mut job = job(ui, spans, size, color, tokens, page);
        // Latin has no joins to stretch, so a left-to-right paragraph is justified the way
        // egui does it, by widening the spaces. The strength is about kashidas and has
        // nothing to say here.
        job.justify = justify.is_some();
        match selected {
            // Nothing selected here: egui's own label, which is what this has always been.
            None if !dir_of(spans, page).is_rtl() => {
                ui.label(job);
            }
            None => {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| ui.label(job));
            }
            // Something is. The galley is laid out here so the selection can go behind the
            // words rather than over them, which a label gives no way to do.
            Some((a, b)) => {
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                let (enabled, said) = (ui.is_enabled(), galley.text().to_string());
                resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &said));
                for band in selection_bands(&galley, a, b) {
                    ui.painter().rect_filled(band.translate(rect.min.to_vec2()), 1.0, tokens.selection);
                }
                ui.painter().galley(rect.min, galley, color);
            }
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut run: Vec<Span> = Vec::new();
        for s in spans {
            if s.math {
                if !run.is_empty() {
                    ui.label(job(ui, &run, size, color, tokens, page));
                    run.clear();
                }
                crate::mathview::inline(ui, &s.text, size * 1.05, color);
            } else {
                run.push(s.clone());
            }
        }
        if !run.is_empty() {
            ui.label(job(ui, &run, size, color, tokens, page));
        }
    });
}

fn text_of(spans: &[Span]) -> String {
    spans.iter().filter(|s| s.image.is_none()).map(|s| s.text.as_str()).collect()
}

/// The type size a block's words are drawn at here.
fn size_of_block(block: &Block) -> f32 {
    match block {
        Block::Heading { level, .. } => match level {
            1 => 28.0,
            2 => 22.0,
            3 => 18.0,
            _ => 16.0,
        },
        _ => 14.5,
    }
}

/// The link under a point in a block the bidi painter drew, if the point is on one. `at` is
/// measured from the block's top left corner and `width` is the block's width. `None` for a
/// block egui drew (the caller maps those through egui's own layout) and for a point on no
/// link. The lookup goes through parley, the same way the drawing did, so a click on a
/// right-to-left or mixed line lands on the word under the pointer.
pub fn link_at_point(block: &Block, at: egui::Vec2, width: f32, page: Option<Dir>) -> Option<String> {
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans,
        _ => return None,
    };
    if spans.iter().any(|s| s.math) {
        return None;
    }
    // What is drawn: the words without a tag's marker, beside the mark or the bullet.
    let (spans, mark) = match tagged(spans) {
        Some((_, stripped)) => (stripped, 21.0),
        None => (spans.clone(), 0.0),
    };
    let text = text_of(&spans);
    if !crate::bidi::paints(&text, page) {
        return None;
    }
    let lead = mark
        + match block {
            Block::Item { depth, .. } => 8.0 + *depth as f32 * 18.0 + 22.0,
            Block::Quote(_) => 18.0,
            _ => 0.0,
        };
    // A right-to-left row puts its bullet or mark on the right, so the words start at the
    // left edge; a left-to-right row has them on the left.
    let rtl = dir_of(&spans, page).is_rtl();
    let x = if rtl { at.x } else { at.x - lead };
    let column = (width - lead).max(32.0);
    let byte = crate::bidi::hit(&text, egui::vec2(x, at.y), size_of_block(block), column);
    let mut start = 0usize;
    for s in &spans {
        if s.image.is_some() {
            continue;
        }
        let end = start + s.text.len();
        if byte >= start && byte < end {
            return s.link.clone();
        }
        start = end;
    }
    None
}

/// The box a picture is fitted into, keeping its shape.
///
/// The height must not come from `ui.available_height()`. A page scrolls, so once the
/// blocks above have filled the view there is no height left, and egui fits a picture into
/// a box of no height: it drew nothing at all, and a picture low on a long page was a gap
/// where a picture should be. Measured on 2026-10-19 on the Arabic showcase page, where the
/// same picture drew at the top of one page and vanished at the bottom of another.
fn picture_box(ui: &Ui, zoom: f32) -> egui::Vec2 {
    // Parent columns already carry the page zoom; the picture's height is its own size.
    egui::vec2(ui.available_width(), 480.0 * zoom)
}

fn draw_inline_images(ui: &mut Ui, spans: &[Span], page_dir: &Path, zoom: f32) {
    for s in spans {
        if let Some(src) = &s.image {
            match image_uri(page_dir, src) {
                Some(uri) => {
                    // A linked image is the attachment-card form used by NoteCraft. Keep its
                    // generated shell icon compact; standalone images still use the picture box.
                    let size = if s.link.is_some() { egui::vec2(48.0 * zoom, 56.0 * zoom) } else { picture_box(ui, zoom) };
                    let response = ui.add(egui::Image::new(uri).fit_to_exact_size(size).sense(egui::Sense::hover()));
                    if response.hovered() {
                        log::debug!("inline image render hover: src={src:?} rect={:?} pointer={:?}", response.rect, ui.ctx().pointer_latest_pos());
                    }
                }
                None => {
                    ui.weak(format!("[image: {} ({src})]", s.text));
                }
            }
        }
    }
}

/// True for a block that is more than styled text: a picture, a table, an equation, a
/// diagram. While the page is written on as text, these are drawn below it.
pub fn is_rich(block: &Block) -> bool {
    matches!(block, Block::Image { .. } | Block::Table { .. } | Block::MathBlock(_) | Block::Code { .. })
        || matches!(block, Block::Paragraph(spans) if spans.iter().any(|s| s.math || s.image.is_some()))
}

/// Draws only the blocks [`is_rich`] picks out. They are picked out of their page, so the
/// page's justification does not follow them here; none of them is a paragraph of words.
pub fn show_rich(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Option<Dir>) {
    let rich: Vec<Block> = blocks.iter().filter(|b| is_rich(b)).cloned().collect();
    let _link = show(ui, &rich, page_dir, tokens, page, &Justified::default());
}

/// What happened in one block this frame.
pub struct BlockResponse {
    /// The area the block took, sensing clicks: this is how the caret lands in it.
    pub response: egui::Response,
    /// A to-do box the user ticked or cleared, and its new state.
    pub task_set: Option<bool>,
}

/// Draws blocks into `ui`. `page_dir` is the page's folder, used to find attachments.
/// Read only: to-do boxes are shown but not ticked (see [`show_block`]). A link is still
/// live: the target of one clicked this frame is returned, as written in the page.
pub fn show(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Option<Dir>, justified: &Justified) -> Option<String> {
    show_zoomed(ui, blocks, page_dir, tokens, page, justified, 1.0)
}

/// Draws blocks at a page zoom while keeping the surrounding window unchanged.
pub fn show_zoomed(
    ui: &mut Ui,
    blocks: &[Block],
    page_dir: &Path,
    tokens: &Tokens,
    page: Option<Dir>,
    justified: &Justified,
    zoom: f32,
) -> Option<String> {
    let mut link = None;
    for (i, block) in blocks.iter().enumerate() {
        let out = show_block_zoomed(ui, block, i, page_dir, tokens, false, page, justified.strength_of(i), None, ui.id().with(("block", i)), zoom);
        let resp = out.response;
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(href) = crate::pageedit::link_under(ui, block, &resp, p, tokens, page)
        {
            link = Some(href);
        } else if resp.hovered() && resp.hover_pos().and_then(|p| crate::pageedit::link_under(ui, block, &resp, p, tokens, page)).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    link
}

/// Draws one block and reports what happened in it. With `live` the to-do box can be
/// ticked, which is what the page editor passes. With `justified` the block's lines are
/// made to reach the column; the page says which blocks those are, and only a paragraph
/// listens (see `notecraft_model::justify`).
#[allow(clippy::too_many_arguments)]
pub fn show_block(
    ui: &mut Ui,
    block: &Block,
    i: usize,
    page_dir: &Path,
    tokens: &Tokens,
    live: bool,
    page: Option<Dir>,
    justified: Option<Strength>,
    // What of this block's words a selection reaching across paragraphs covers, in bytes.
    selected: Option<(usize, usize)>,
    // The id the block senses under. The page editor passes the one its text box uses for
    // this block, so a drag that begins on the finished block carries straight on into the
    // text box when the caret lands in it on the next frame; egui tracks a drag against the
    // id that started it, and a different id here would drop the drag on the floor.
    id: egui::Id,
) -> BlockResponse {
    show_block_zoomed(ui, block, i, page_dir, tokens, live, page, justified, selected, id, 1.0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show_block_zoomed(
    ui: &mut Ui,
    block: &Block,
    i: usize,
    page_dir: &Path,
    tokens: &Tokens,
    live: bool,
    page: Option<Dir>,
    justified: Option<Strength>,
    selected: Option<(usize, usize)>,
    id: egui::Id,
    zoom: f32,
) -> BlockResponse {
    let start = ui.cursor().min;
    let body = ui.style().visuals.text_color();
    let mut task_set = None;
    // Keep the actual painted bounds for rich-object hit testing.  The block's
    // layout row may be as wide as the page even when an image is narrower.
    let mut rendered_object_rect = None;
    {
        match block {
            Block::Heading { level, spans } => {
                let size = match level {
                    1 => 28.0,
                    2 => 22.0,
                    3 => 18.0,
                    _ => 16.0,
                } * zoom;
                ui.add_space(6.0);
                match tagged(spans) {
                    Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| draw_spans(ui, &spans, size, tokens.strong, tokens, page, None, selected)),
                    None => draw_spans(ui, spans, size, tokens.strong, tokens, page, None, selected),
                }
                ui.add_space(2.0);
            }
            Block::Paragraph(spans) => {
                // A line that starts with a tag marker shows the tag's mark, not the word.
                match tagged(spans) {
                    Some((tag, spans)) => {
                        with_tag(ui, tag, tokens, |ui| {
                            draw_inline_images(ui, &spans, page_dir, zoom);
                            draw_spans(ui, &spans, 14.5 * zoom, body, tokens, page, justified, selected);
                        });
                        ui.add_space(4.0);
                    }
                    None => {
                        draw_inline_images(ui, spans, page_dir, zoom);
                        if !text_of(spans).is_empty() || spans.iter().any(|s| s.math) {
                            draw_spans(ui, spans, 14.5 * zoom, body, tokens, page, justified, selected);
                        }
                        ui.add_space(4.0);
                    }
                }
            }
            Block::Item { ordered, number, task, depth, spans } => {
                // A right-to-left item starts at the right: its bullet or box goes there,
                // and the indent of a nested item comes off the right edge too.
                let row = if dir_of(spans, page).is_rtl() {
                    egui::Layout::right_to_left(egui::Align::Min)
                } else {
                    egui::Layout::left_to_right(egui::Align::Min)
                };
                ui.with_layout(row, |ui| {
                    ui.add_space(8.0 + *depth as f32 * 18.0);
                    match task {
                        Some(done) => {
                            let mut v = *done;
                            // On a page the user is writing, the box is theirs to tick.
                            if ui.add_enabled(live, egui::Checkbox::without_text(&mut v)).changed() {
                                task_set = Some(v);
                            }
                        }
                        None => {
                            ui.label(RichText::new(if *ordered { format!("{number}.") } else { "\u{2022}".to_string() }).color(tokens.muted));
                        }
                    }
                    ui.vertical(|ui| match tagged(spans) {
                        Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| {
                            draw_spans(ui, &spans, 14.5 * zoom, body, tokens, page, None, selected);
                            draw_inline_images(ui, &spans, page_dir, zoom);
                        }),
                        None => {
                            draw_spans(ui, spans, 14.5 * zoom, body, tokens, page, None, selected);
                            draw_inline_images(ui, spans, page_dir, zoom);
                        }
                    });
                });
            }
            Block::Quote(spans) => {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, 18.0)), 1.0, tokens.quote_bar);
                    draw_spans(ui, spans, 14.5 * zoom, tokens.muted, tokens, page, None, selected);
                });
                ui.add_space(4.0);
            }
            // A Mermaid diagram is shown as the drawing, in the page, and a click puts the caret in
            // it, which shows its source to edit. One that cannot be drawn says why and shows its
            // source instead, so nothing the page holds is ever hidden.
            Block::Code { lang, text } if lang == "mermaid" => {
                egui::Frame::new().fill(tokens.editor).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let crate::diagramview::Shown::Failed(_) = crate::diagramview::show_zoomed(ui, text, tokens, zoom) {
                        ui.label(RichText::new(text).monospace().size(14.5 * zoom));
                    }
                });
                ui.add_space(4.0);
            }
            Block::Code { lang, text } => {
                let (numbered, language) = notecraft_formats::markdown::code_lang(lang);
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if !language.is_empty() {
                        ui.label(RichText::new(language).size(12.0 * zoom).color(tokens.muted));
                    }
                    if numbered {
                        // Numbers are for reading: the block being written in shows none (pageedit.rs).
                        let lines = text.trim_end_matches('\n').split('\n').count();
                        let numbers = (1..=lines).map(|n| n.to_string()).collect::<Vec<_>>().join("\n");
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(numbers).monospace().size(14.5 * zoom).color(tokens.muted));
                            ui.label(RichText::new(text.trim_end_matches('\n')).monospace().size(14.5 * zoom));
                        });
                    } else {
                        ui.label(RichText::new(text).monospace().size(14.5 * zoom));
                    }
                });
                ui.add_space(4.0);
            }
            Block::MathBlock(tex) => {
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::mathview::display(ui, tex, 22.0 * zoom, tokens.strong);
                });
                ui.add_space(4.0);
            }
            Block::Table { head, rows } => {
                egui::Grid::new(("table", i)).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for cell in head {
                        ui.label(job(ui, cell, 14.5 * zoom, tokens.strong, tokens, page));
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(job(ui, cell, 14.5 * zoom, body, tokens, page));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            }
            Block::Rule => {
                ui.separator();
            }
            // The page names its own ink so that its words say what is on it, but the ink
            // is drawn as a layer over the whole page, not as a picture in the middle of
            // the words. Another program opening the page sees the handwriting; here the
            // line itself takes no room.
            Block::Image { src, .. } if crate::inkpad::is_page_ink(src) => {}
            Block::Image { alt, src } => {
                match image_uri(page_dir, src) {
                    Some(uri) => {
                        let response = ui.add(
                            egui::Image::new(uri)
                                .max_width(ui.available_width())
                                .max_height(480.0 * zoom)
                                .fit_to_original_size(1.0)
                                .sense(egui::Sense::hover()),
                        );
                        rendered_object_rect = Some(response.rect);
                        if response.hovered() {
                            log::debug!(
                                "image render hover: block={i} src={src:?} rect={:?} pointer={:?}",
                                response.rect,
                                ui.ctx().pointer_latest_pos()
                            );
                        }
                    }
                    None => {
                        ui.weak(format!("[image: {alt} ({src})]"));
                    }
                }
                ui.add_space(4.0);
            }
        }
    }
    // A block with no words in it is still a line the writer can click into. Drawn as nothing it
    // was a few points tall, so clicking the empty box that held it went nowhere and no caret
    // appeared: give it the height of one line, for the eye and for the pointer.
    if is_blank(block) {
        let used = ui.cursor().min.y - start.y;
        let line = match block {
            Block::Heading { level: 1, .. } => 34.0,
            Block::Heading { level: 2, .. } => 28.0,
            Block::Heading { .. } => 24.0,
            _ => 22.0,
        };
        if used < line {
            ui.add_space(line - used);
        }
    }
    let end = ui.cursor().min;
    let rect = egui::Rect::from_min_max(start, egui::pos2(start.x + ui.available_width().max(1.0), end.y.max(start.y + 1.0)));
    let response = ui.interact(rect, id, egui::Sense::click());
    if let Some(kind) = object_kind(block) {
        let clicked = response.clicked();
        let pointer = response.interact_pointer_pos();
        ui.data_mut(|data| {
            let mut hits = data.get_temp::<Vec<(usize, egui::Rect, String)>>(object_hits_id()).unwrap_or_default();
            hits.push((i, rendered_object_rect.unwrap_or(response.rect), kind.to_string()));
            data.insert_temp(object_hits_id(), hits);
            if clicked {
                data.insert_temp(object_clicked_id(), (i, kind.to_string()));
            }
        });
        if clicked {
            log::debug!("embedded object click: block={i} kind={kind} rect={:?} pointer={pointer:?}", response.rect);
        }
    }
    BlockResponse { response, task_set }
}

pub(crate) fn object_kind(block: &Block) -> Option<&'static str> {
    match block {
        Block::Image { .. } => Some("image"),
        Block::Code { lang, .. } if lang == "mermaid" => Some("diagram"),
        Block::Code { .. } => Some("code"),
        Block::MathBlock(_) => Some("math"),
        Block::Table { .. } => Some("table"),
        Block::Paragraph(spans) | Block::Item { spans, .. } | Block::Heading { spans, .. } | Block::Quote(spans)
            if spans.iter().any(|s| s.image.is_some()) =>
        {
            Some("image")
        }
        _ => None,
    }
}

pub(crate) fn object_hits_id() -> egui::Id {
    egui::Id::new("embedded-object-hits")
}

pub(crate) fn object_clicked_id() -> egui::Id {
    egui::Id::new("embedded-object-clicked")
}

/// A text block with no words, no maths and no picture in it: a blank line.
pub fn is_blank(block: &Block) -> bool {
    match block {
        Block::Paragraph(spans) | Block::Quote(spans) | Block::Heading { spans, .. } | Block::Item { spans, .. } => {
            text_of(spans).is_empty() && !spans.iter().any(|s| s.math || s.image.is_some())
        }
        _ => false,
    }
}

/// The tag at the start of a line, and the words without its marker.
pub fn tagged(spans: &[Span]) -> Option<(notecraft_model::Tag, Vec<Span>)> {
    let (tag, _) = notecraft_model::Tag::split(&text_of(spans));
    let tag = tag?;
    let marker = tag.marker();
    let mut out = spans.to_vec();
    if let Some(first) = out.first_mut() {
        if let Some(rest) = first.text.strip_prefix(&marker) {
            first.text = rest.to_string();
        } else if let Some(rest) = first.text.strip_prefix(marker.trim_end()) {
            first.text = rest.to_string();
        }
    }
    Some((tag, out))
}

/// Draws a line behind its tag mark.
fn with_tag(ui: &mut Ui, tag: notecraft_model::Tag, tokens: &Tokens, body: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(21.0, 20.0), egui::Sense::hover());
        let mark = crate::ribbon::mark_for(tag);
        crate::ribbon_icons::paint(
            &ui.painter_at(rect),
            egui::Rect::from_center_size(rect.center(), egui::vec2(17.0, 17.0)),
            mark,
            mark.color(tokens.text, tokens.accent),
        );
        resp.on_hover_text(tag.label());
        ui.vertical(body);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_line_is_a_line_tall_so_it_can_be_clicked() {
        use notecraft_formats::markdown::Block;
        let t = crate::theme::Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut heights = Vec::new();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            for block in [Block::Paragraph(Vec::new()), Block::Heading { level: 1, spans: Vec::new() }, Block::Quote(Vec::new())] {
                let out = show_block(
                    ui,
                    &block,
                    0,
                    std::path::Path::new("."),
                    &t,
                    true,
                    Some(notecraft_model::Dir::Ltr),
                    None,
                    None,
                    ui.id().with("block"),
                );
                heights.push(out.response.rect.height());
            }
        });
        frame.textures_delta.clear();
        assert!(heights.iter().all(|h| *h >= 20.0), "an empty line has the height of a line, not a sliver: {heights:?}");
        assert!(is_blank(&Block::Paragraph(Vec::new())) && !is_blank(&Block::Rule));
    }

    #[test]
    fn both_formats_give_the_same_blocks_for_the_same_content() {
        let md = blocks_for(PageFormat::Cmgfm, "# Title\n\n- a\n- b\n");
        let html = blocks_for(PageFormat::Html, "<h1>Title</h1><ul><li>a</li><li>b</li></ul>");
        assert_eq!(md.len(), html.len());
        assert!(matches!(&html[0], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn image_uris_only_for_attachments() {
        let dir = Path::new("/notes/nb/page");
        assert_eq!(image_uri(dir, "assets/a.png").as_deref(), Some("file:///notes/nb/page/assets/a.png"));
        assert_eq!(image_uri(Path::new("C:\\n\\p"), "assets/a b.png").as_deref(), Some("file:///C:/n/p/assets/a%20b.png"));
        assert_eq!(image_uri(Path::new(r"\\?\C:\n\p"), "assets/a.png").as_deref(), Some("file:///C:/n/p/assets/a.png"), "no verbatim prefix");
        assert_eq!(image_uri(dir, "https://x.test/a.png"), None);
        assert_eq!(image_uri(dir, "assets/../../secret.png"), None);
        assert_eq!(image_uri(dir, "assets/"), None);
        assert_eq!(image_uri(dir, "assets/a\\b.png"), None);
        assert_eq!(image_uri(dir, "../assets/a.png"), None);
    }

    #[test]
    fn html_attachment_cards_reach_the_preview_as_linked_images() {
        let blocks = blocks_for(
            PageFormat::Html,
            r#"<p><a href="assets/brief.docx"><img src="assets/brief.docx.png" alt="" width="48"><br>brief.docx</a></p>"#,
        );
        let Block::Paragraph(spans) = &blocks[0] else { panic!("attachment card was not a paragraph") };
        let image = spans.iter().find(|span| span.image.as_deref() == Some("assets/brief.docx.png")).expect("attachment icon");
        assert_eq!(image.link.as_deref(), Some("assets/brief.docx"));
    }

    #[test]
    fn rich_blocks_share_object_selection_kinds() {
        assert_eq!(object_kind(&Block::Image { alt: "a".into(), src: "assets/a.png".into() }), Some("image"));
        assert_eq!(object_kind(&Block::Code { lang: "mermaid".into(), text: "graph TD;".into() }), Some("diagram"));
        assert_eq!(object_kind(&Block::Code { lang: "rust".into(), text: "fn main() {}".into() }), Some("code"));
        assert_eq!(object_kind(&Block::MathBlock("x^2".into())), Some("math"));
        assert_eq!(object_kind(&Block::Table { head: vec![], rows: vec![] }), Some("table"));
        assert_eq!(object_kind(&Block::Paragraph(vec![Span { image: Some("assets/a.png".into()), ..Span::default() }])), Some("image"));
        assert_eq!(object_kind(&Block::Paragraph(vec![Span { text: "plain".into(), ..Span::default() }])), None);
    }

    /// An attachment link must resolve to something egui can actually load.
    #[test]
    fn attachment_images_load_through_the_file_loader() {
        use notecraft_store::testing::TempDir;
        let t = TempDir::new("preview-img");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("logo.png"), crate::LOGO_PNG).expect("png");
        let uri = image_uri(&page, "assets/logo.png").expect("uri");
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let mut last = String::new();
        for _ in 0..100 {
            match ctx.try_load_image(&uri, egui::SizeHint::default()) {
                Ok(egui::load::ImagePoll::Ready { image }) => {
                    assert_eq!(image.size, [256, 256]);
                    return;
                }
                Ok(egui::load::ImagePoll::Pending { .. }) => last = "pending".into(),
                Err(e) => panic!("{uri}: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("never ready: {last}");
    }

    /// A link in an Arabic line is found under the pointer through parley, and the words
    /// around it are not links.
    #[test]
    fn a_link_in_a_right_to_left_line_is_found_under_the_pointer() {
        let blocks = blocks_for(PageFormat::Cmgfm, "مرحبا بالعالم [هذه صفحة](../page-b/) والباقي كلام عادي\n");
        let block = blocks.first().expect("a paragraph");
        let hits: Vec<(i32, Option<String>)> =
            (0..400).step_by(8).map(|x| (x, link_at_point(block, egui::vec2(x as f32, 8.0), 400.0, Some(Dir::Ltr)))).collect();
        let on: Vec<i32> = hits.iter().filter(|(_, h)| h.is_some()).map(|(x, _)| *x).collect();
        assert!(!on.is_empty(), "the link is somewhere on the line: {hits:?}");
        assert!(hits.iter().all(|(_, h)| h.is_none() || h.as_deref() == Some("../page-b/")));
        // The line hangs from the right edge: the first words sit at the far right and are
        // not the link, and the link is left of them.
        assert!(hits.last().is_some_and(|(_, h)| h.is_none()), "the right edge is the first word, not the link");
        let (first_on, last_on) = (*on.first().unwrap_or(&0), *on.last().unwrap_or(&0));
        assert!(last_on < 392 && last_on - first_on > 30, "a contiguous stretch: {first_on}..{last_on}");
        // A left-to-right paragraph is not this painter's; the caller uses egui's layout.
        let ltr = blocks_for(PageFormat::Cmgfm, "Hello [there](../x/) world\n");
        assert_eq!(link_at_point(&ltr[0], egui::vec2(50.0, 8.0), 400.0, Some(Dir::Ltr)), None);
    }

    /// A picture is drawn at its full size even when the layout has no height left.
    ///
    /// This is the defect the Arabic showcase page found: the same picture drew at the top
    /// of one page and came out nothing at all low down another, because egui fits an image
    /// into the space the layout has left and a scrolling page has none once the blocks
    /// above have filled the view.
    #[test]
    fn a_picture_low_on_a_page_is_still_drawn() {
        use notecraft_store::testing::TempDir;
        let t = TempDir::new("preview-low");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("logo.png"), crate::LOGO_PNG).expect("png");
        let blocks = blocks_for(
            PageFormat::Cmgfm,
            "![logo](assets/logo.png)
",
        );
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Dark, accent: crate::theme::Accent::Default }.tokens();
        let mut tall = 0.0;
        for _ in 0..100 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                // The blocks above have taken the whole view: nothing is left below.
                let left = ui.available_height();
                ui.allocate_space(egui::vec2(0.0, left));
                let before = ui.cursor().min.y;
                show(ui, &blocks, &page, &tokens, Some(Dir::Ltr), &Justified::default());
                tall = ui.cursor().min.y - before;
            });
            out.textures_delta.clear();
            if tall > 100.0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the picture came out {tall} points tall");
    }

    #[test]
    fn drawing_every_block_kind_does_not_panic() {
        let md = "# H\n\npara **b** *i* ~~s~~ `c` [l](https://x) $m$ ![a](assets/a.png)\n\n- [ ] t\n- [x] d\n1. n\n\n> q\n\n```mermaid\ngraph TD;\n```\n\n$$\nE\n$$\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n![alt](assets/x.png)\n";
        let blocks = blocks_for(PageFormat::Cmgfm, md);
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Dark, accent: crate::theme::Accent::Default }.tokens();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &blocks, Path::new("."), &tokens, Some(Dir::Ltr), &Justified::default());
        });
        out.textures_delta.clear();
    }
}

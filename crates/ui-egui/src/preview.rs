//! The page preview: blocks from either format, drawn with egui.
//!
//! Bold is shown in a stronger colour because the default egui fonts have no bold
//! face (adding one means adding a font file and its licence). A click on a link is
//! reported by [`show`] for the window to follow. Diagram blocks show their
//! source: diagrams are not rendered yet. Equations are drawn from a TeX subset.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, RichText, Stroke, Ui};
use notecraft_formats::html;
use notecraft_formats::markdown::{self, Block, Span};
use notecraft_model::Dir;
use notecraft_model::PageFormat;
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

pub fn job(ui: &Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens, page: Dir) -> LayoutJob {
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

/// Which way a block's words read, given the direction of the page they are on.
pub fn dir_of(spans: &[Span], page: Dir) -> Dir {
    notecraft_model::direction_or(&text_of(spans), page)
}

/// Draws spans as wrapped text. A paragraph with inline math flows text and formulas
/// together; one without math is a single label. Right-to-left text hangs from the right
/// edge, which is where a reader of Arabic or Hebrew starts.
fn draw_spans(ui: &mut Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens, page: Dir) {
    if !spans.iter().any(|s| s.math) {
        // A line that mixes the two directions is drawn by egui in the wrong order, and a
        // paragraph that reads right to left is not wrapped by it, so both go through the
        // bidi painter: parley says where each run belongs, egui still draws them, and the
        // marks inside the words are kept.
        let text = text_of(spans);
        if crate::bidi::paints(&text, page) {
            let pieces = pieces(spans, size, color, tokens);
            let width = ui.available_width();
            let rect = crate::bidi::show(ui, &text, &pieces, size, width, page);
            let _ = rect;
            return;
        }
        let job = job(ui, spans, size, color, tokens, page);
        if dir_of(spans, page).is_rtl() {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| ui.label(job));
        } else {
            ui.label(job);
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
pub fn link_at_point(block: &Block, at: egui::Vec2, width: f32, page: Dir) -> Option<String> {
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
fn picture_box(ui: &Ui) -> egui::Vec2 {
    egui::vec2(ui.available_width(), 480.0)
}

fn draw_inline_images(ui: &mut Ui, spans: &[Span], page_dir: &Path) {
    for s in spans {
        if let Some(src) = &s.image {
            match image_uri(page_dir, src) {
                Some(uri) => {
                    ui.add(egui::Image::new(uri).fit_to_exact_size(picture_box(ui)));
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

/// Draws only the blocks [`is_rich`] picks out.
pub fn show_rich(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Dir) {
    let rich: Vec<Block> = blocks.iter().filter(|b| is_rich(b)).cloned().collect();
    let _link = show(ui, &rich, page_dir, tokens, page);
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
pub fn show(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Dir) -> Option<String> {
    let mut link = None;
    for (i, block) in blocks.iter().enumerate() {
        let out = show_block(ui, block, i, page_dir, tokens, false, page);
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
/// ticked, which is what the page editor passes.
pub fn show_block(ui: &mut Ui, block: &Block, i: usize, page_dir: &Path, tokens: &Tokens, live: bool, page: Dir) -> BlockResponse {
    let start = ui.cursor().min;
    let body = ui.style().visuals.text_color();
    let mut task_set = None;
    {
        match block {
            Block::Heading { level, spans } => {
                let size = match level {
                    1 => 28.0,
                    2 => 22.0,
                    3 => 18.0,
                    _ => 16.0,
                };
                ui.add_space(6.0);
                match tagged(spans) {
                    Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| draw_spans(ui, &spans, size, tokens.strong, tokens, page)),
                    None => draw_spans(ui, spans, size, tokens.strong, tokens, page),
                }
                ui.add_space(2.0);
            }
            Block::Paragraph(spans) => {
                // A line that starts with a tag marker shows the tag's mark, not the word.
                match tagged(spans) {
                    Some((tag, spans)) => {
                        with_tag(ui, tag, tokens, |ui| {
                            draw_spans(ui, &spans, 14.5, body, tokens, page);
                            draw_inline_images(ui, &spans, page_dir);
                        });
                        ui.add_space(4.0);
                    }
                    None => {
                        if !text_of(spans).is_empty() || spans.iter().any(|s| s.math) {
                            draw_spans(ui, spans, 14.5, body, tokens, page);
                        }
                        draw_inline_images(ui, spans, page_dir);
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
                            draw_spans(ui, &spans, 14.5, body, tokens, page);
                            draw_inline_images(ui, &spans, page_dir);
                        }),
                        None => {
                            draw_spans(ui, spans, 14.5, body, tokens, page);
                            draw_inline_images(ui, spans, page_dir);
                        }
                    });
                });
            }
            Block::Quote(spans) => {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, 18.0)), 1.0, tokens.quote_bar);
                    draw_spans(ui, spans, 14.5, tokens.muted, tokens, page);
                });
                ui.add_space(4.0);
            }
            Block::Code { lang, text } => {
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if lang == "mermaid" {
                        ui.label(RichText::new("Diagram (mermaid). Not rendered yet: the source is shown.").small().color(tokens.muted));
                    } else if !lang.is_empty() {
                        ui.label(RichText::new(lang).small().color(tokens.muted));
                    }
                    ui.label(RichText::new(text).monospace());
                });
                ui.add_space(4.0);
            }
            Block::MathBlock(tex) => {
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::mathview::display(ui, tex, 22.0, tokens.strong);
                });
                ui.add_space(4.0);
            }
            Block::Table { head, rows } => {
                egui::Grid::new(("table", i)).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for cell in head {
                        ui.label(job(ui, cell, 14.5, tokens.strong, tokens, page));
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(job(ui, cell, 14.5, body, tokens, page));
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
                        ui.add(egui::Image::new(uri).fit_to_exact_size(picture_box(ui)));
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
    let response = ui.interact(rect, ui.id().with(("block", i)), egui::Sense::click());
    BlockResponse { response, task_set }
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
                let out = show_block(ui, &block, 0, std::path::Path::new("."), &t, true, notecraft_model::Dir::Ltr);
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
            (0..400).step_by(8).map(|x| (x, link_at_point(block, egui::vec2(x as f32, 8.0), 400.0, Dir::Ltr))).collect();
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
        assert_eq!(link_at_point(&ltr[0], egui::vec2(50.0, 8.0), 400.0, Dir::Ltr), None);
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
                show(ui, &blocks, &page, &tokens, Dir::Ltr);
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
            show(ui, &blocks, Path::new("."), &tokens, Dir::Ltr);
        });
        out.textures_delta.clear();
    }
}

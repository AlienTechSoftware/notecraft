//! The page preview: blocks from either format, drawn with egui.
//!
//! Bold is shown in a stronger colour because the default egui fonts have no bold
//! face (adding one means adding a font file and its licence). Links are underlined
//! but not clickable yet. Diagram blocks show their
//! source: diagrams are not rendered yet. Equations are drawn from a TeX subset.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, RichText, Stroke, Ui};
use notecraft_formats::html;
use notecraft_formats::markdown::{self, Block, Span};
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

fn job(ui: &Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens) -> LayoutJob {
    let mut job = LayoutJob::default();
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

/// Draws spans as wrapped text. A paragraph with inline math flows text and formulas
/// together; one without math is a single label.
fn draw_spans(ui: &mut Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens) {
    if !spans.iter().any(|s| s.math) {
        ui.label(job(ui, spans, size, color, tokens));
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut run: Vec<Span> = Vec::new();
        for s in spans {
            if s.math {
                if !run.is_empty() {
                    ui.label(job(ui, &run, size, color, tokens));
                    run.clear();
                }
                crate::mathview::inline(ui, &s.text, size * 1.05, color);
            } else {
                run.push(s.clone());
            }
        }
        if !run.is_empty() {
            ui.label(job(ui, &run, size, color, tokens));
        }
    });
}

fn text_of(spans: &[Span]) -> String {
    spans.iter().filter(|s| s.image.is_none()).map(|s| s.text.as_str()).collect()
}

fn draw_inline_images(ui: &mut Ui, spans: &[Span], page_dir: &Path) {
    for s in spans {
        if let Some(src) = &s.image {
            match image_uri(page_dir, src) {
                Some(uri) => {
                    ui.add(egui::Image::new(uri).max_width(ui.available_width()).max_height(480.0));
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
pub fn show_rich(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens) {
    let rich: Vec<Block> = blocks.iter().filter(|b| is_rich(b)).cloned().collect();
    show(ui, &rich, page_dir, tokens);
}

/// What happened in one block this frame.
pub struct BlockResponse {
    /// The area the block took, sensing clicks: this is how the caret lands in it.
    pub response: egui::Response,
    /// A to-do box the user ticked or cleared, and its new state.
    pub task_set: Option<bool>,
}

/// Draws blocks into `ui`. `page_dir` is the page's folder, used to find attachments.
/// Read only: to-do boxes are shown but not ticked (see [`show_block`]).
pub fn show(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens) {
    for (i, block) in blocks.iter().enumerate() {
        show_block(ui, block, i, page_dir, tokens, false);
    }
}

/// Draws one block and reports what happened in it. With `live` the to-do box can be
/// ticked, which is what the page editor passes.
pub fn show_block(ui: &mut Ui, block: &Block, i: usize, page_dir: &Path, tokens: &Tokens, live: bool) -> BlockResponse {
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
                    Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| draw_spans(ui, &spans, size, tokens.strong, tokens)),
                    None => draw_spans(ui, spans, size, tokens.strong, tokens),
                }
                ui.add_space(2.0);
            }
            Block::Paragraph(spans) => {
                // A line that starts with a tag marker shows the tag's mark, not the word.
                match tagged(spans) {
                    Some((tag, spans)) => {
                        with_tag(ui, tag, tokens, |ui| {
                            draw_spans(ui, &spans, 14.5, body, tokens);
                            draw_inline_images(ui, &spans, page_dir);
                        });
                        ui.add_space(4.0);
                    }
                    None => {
                        if !text_of(spans).is_empty() || spans.iter().any(|s| s.math) {
                            draw_spans(ui, spans, 14.5, body, tokens);
                        }
                        draw_inline_images(ui, spans, page_dir);
                        ui.add_space(4.0);
                    }
                }
            }
            Block::Item { ordered, number, task, depth, spans } => {
                ui.horizontal_top(|ui| {
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
                            draw_spans(ui, &spans, 14.5, body, tokens);
                            draw_inline_images(ui, &spans, page_dir);
                        }),
                        None => {
                            draw_spans(ui, spans, 14.5, body, tokens);
                            draw_inline_images(ui, spans, page_dir);
                        }
                    });
                });
            }
            Block::Quote(spans) => {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, 18.0)), 1.0, tokens.quote_bar);
                    draw_spans(ui, spans, 14.5, tokens.muted, tokens);
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
                        ui.label(job(ui, cell, 14.5, tokens.strong, tokens));
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(job(ui, cell, 14.5, body, tokens));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
            }
            Block::Rule => {
                ui.separator();
            }
            Block::Image { alt, src } => {
                match image_uri(page_dir, src) {
                    Some(uri) => {
                        ui.add(egui::Image::new(uri).max_width(ui.available_width()).max_height(480.0));
                    }
                    None => {
                        ui.weak(format!("[image: {alt} ({src})]"));
                    }
                }
                ui.add_space(4.0);
            }
        }
    }
    let end = ui.cursor().min;
    let rect = egui::Rect::from_min_max(start, egui::pos2(start.x + ui.available_width().max(1.0), end.y.max(start.y + 1.0)));
    let response = ui.interact(rect, ui.id().with(("block", i)), egui::Sense::click());
    BlockResponse { response, task_set }
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

    #[test]
    fn drawing_every_block_kind_does_not_panic() {
        let md = "# H\n\npara **b** *i* ~~s~~ `c` [l](https://x) $m$ ![a](assets/a.png)\n\n- [ ] t\n- [x] d\n1. n\n\n> q\n\n```mermaid\ngraph TD;\n```\n\n$$\nE\n$$\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n![alt](assets/x.png)\n";
        let blocks = blocks_for(PageFormat::Cmgfm, md);
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Dark, accent: crate::theme::Accent::Default }.tokens();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &blocks, Path::new("."), &tokens);
        });
        out.textures_delta.clear();
    }
}

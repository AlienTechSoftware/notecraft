//! The (>) menu on a picture: detected text as an overlay. This is view state only: it changes how the
//! picture is shown, not what the page contains, so it stays off the undo history. The boxes come from
//! the picture's `.ocr.json` file, which the page's OCR writes beside the picture.

use crate::floating;
use crate::ribbon::Act;
use crate::theme::Tokens;
use egui::{Color32, Id, Pos2, Rect, Stroke, pos2, vec2};
use std::path::{Path, PathBuf};

/// The boxes of a picture's saved OCR file (`<picture>.ocr.json`), in picture pixels: four corners
/// each. Empty when the picture has no saved file.
pub fn read_boxes(picture: &Path) -> Vec<[[f32; 2]; 4]> {
    let Ok(text) = std::fs::read_to_string(ocr_json_path(picture)) else { return Vec::new() };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')) else { return Vec::new() };
    let Some(lines) = value.get("lines").and_then(|l| l.as_array()) else { return Vec::new() };
    lines
        .iter()
        .filter_map(|item| {
            let corners: Option<Vec<[f32; 2]>> =
                item.get("polygon")?.as_array()?.iter().map(|c| Some([c.get(0)?.as_f64()? as f32, c.get(1)?.as_f64()? as f32])).collect();
            let corners = corners.filter(|c| c.len() == 4 && c.iter().all(|p| p[0].is_finite() && p[1].is_finite()))?;
            Some([corners[0], corners[1], corners[2], corners[3]])
        })
        .collect()
}

/// The OCR file beside a picture: `<picture>.ocr.json`.
fn ocr_json_path(picture: &Path) -> PathBuf {
    let mut name = picture.file_name().unwrap_or_default().to_os_string();
    name.push(".ocr.json");
    picture.with_file_name(name)
}
/// The text lines of a picture's saved OCR file, when it has any. Used by Convert permanently.
pub fn saved_text_lines(picture: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(ocr_json_path(picture)).ok()?;
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    // Only a file of this format counts, the same test the search applies.
    if value.get("schema")?.as_str()? != "notecraft.ocr" || value.get("version")?.as_u64()? != 1 {
        return None;
    }
    // A file with a line that has no text is incomplete: nothing is converted from it, rather than part of it.
    let lines: Vec<String> = value.get("lines")?.as_array()?.iter().map(|l| l.get("text")?.as_str().map(str::to_string)).collect::<Option<_>>()?;
    (!lines.is_empty()).then_some(lines)
}
/// Draws the boxes over a picture shown at `rect`. `size` is the picture's size in pixels.
pub fn paint(painter: &egui::Painter, rect: Rect, size: [u32; 2], boxes: &[[[f32; 2]; 4]], colour: Color32) {
    let sx = rect.width() / size[0].max(1) as f32;
    let sy = rect.height() / size[1].max(1) as f32;
    for b in boxes {
        let pts: Vec<Pos2> = b.iter().map(|p| pos2(rect.min.x + p[0] * sx, rect.min.y + p[1] * sy)).collect();
        for i in 0..4 {
            painter.line_segment([pts[i], pts[(i + 1) % 4]], Stroke::new(1.5, colour));
        }
    }
}

/// Whether a picture's detected text is shown, per page and block. Kept in egui's temporary data.
fn overlay_id(page: &Path, block: usize) -> Id {
    Id::new(("image-text-overlay", page.to_path_buf(), block))
}

pub fn showing(ctx: &egui::Context, page: &Path, block: usize) -> bool {
    ctx.data(|d| d.get_temp::<bool>(overlay_id(page, block))).unwrap_or(false)
}

/// Whether a read was asked for on this picture, so its overlay is shown once the boxes exist.
fn pending_id(page: &Path, block: usize) -> Id {
    Id::new(("image-text-pending", page.to_path_buf(), block))
}
fn set_showing(ctx: &egui::Context, page: &Path, block: usize, on: bool) {
    ctx.data_mut(|d| d.insert_temp(overlay_id(page, block), on));
}

/// The (>) button above a picture's top left corner, and its menu. The menu only changes the overlay
/// state. "Detect text" is offered once the picture has a `.lines` file to show.
pub fn menu(ui: &mut egui::Ui, picture_rect: Rect, page: &Path, block: usize, picture: &Path, src: &str, acts: &mut Vec<Act>, t: &Tokens) {
    // A read asked for by Detect text shows the overlay once the boxes exist.
    let pending = pending_id(page, block);
    if ui.ctx().data(|d| d.get_temp::<bool>(pending)).unwrap_or(false) && !read_boxes(picture).is_empty() {
        set_showing(ui.ctx(), page, block, true);
        ui.ctx().data_mut(|d| d.remove::<bool>(pending));
    }
    // Above the picture's top left corner, in the row the frames use for their emoji.
    let at = picture_rect.left_top() + vec2(0.0, -24.0);
    // Invisible until the pointer comes near the picture or a menu is open, so it never hides the text under it.
    let near = ui.ctx().pointer_latest_pos().is_some_and(|p| picture_rect.expand(30.0).contains(p));
    if !near && !egui::Popup::is_any_open(ui.ctx()) {
        return;
    }
    egui::Area::new(Id::new(("image-text-menu", page.to_path_buf(), block))).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let opener = floating::round_button(ui, "👁️", 20.0, true);
        floating::button_menu(&opener, |ui| {
            let boxes = read_boxes(picture);
            if showing(ui.ctx(), page, block) {
                if floating::menu_row(ui, None, crate::tl_id!("ui.image_text_hide"), t).clicked() {
                    set_showing(ui.ctx(), page, block, false);
                    ui.close();
                }
            } else if floating::menu_row(ui, None, crate::tl_id!("ui.image_text_detect"), t).clicked() {
                if boxes.is_empty() {
                    // No text read yet: the app reads the page's pictures; the overlay comes once they are read.
                    ui.ctx().data_mut(|d| d.insert_temp(pending_id(page, block), true));
                    acts.push(Act::ReadPageText);
                } else {
                    set_showing(ui.ctx(), page, block, true);
                }
                ui.close();
            }
            // The conversions are listed now and switched on as their parts are built; each says why it is off.
            floating::menu_row_if(ui, false, None, crate::tl_id!("ui.image_text_convert_text"), t)
                .on_disabled_hover_text(crate::tl_id!("ui.image_text_why_page_format"));
            floating::menu_row_if(ui, false, None, crate::tl_id!("ui.image_text_convert_equation"), t)
                .on_disabled_hover_text(crate::tl_id!("ui.image_text_why_equation_model"));
            let permanent = floating::menu_row_if(ui, !boxes.is_empty(), None, crate::tl_id!("ui.image_text_convert_permanent"), t);
            if permanent.clicked() {
                acts.push(Act::ConvertPermanently { src: src.to_string(), block });
                ui.close();
            }
            permanent.on_disabled_hover_text(crate::tl_id!("ui.image_text_detect_first"));
            if boxes.is_empty() {
                ui.small(crate::tl_id!("ui.image_text_none_yet"));
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boxes_are_the_polygons_of_the_saved_ocr_file() {
        let dir = std::env::temp_dir().join(format!("nc-image-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let picture = dir.join("p.png");
        std::fs::write(&picture, b"picture").expect("picture");
        let saved = serde_json::json!({
            "schema": "notecraft.ocr", "version": 1,
            "picture": { "name": "p.png", "sha256": "x", "width": 4, "height": 4 },
            "produced": { "at": 0, "by": "test" }, "text": "Rye loaf 6.00\nFlat white",
            "lines": [
                { "text": "Rye loaf 6.00", "polygon": [[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 4.0]], "confidence": null },
                { "text": "bad", "polygon": [[1.0, 2.0]], "confidence": null },
            ],
            "bounds": null, "alt": null,
        });
        std::fs::write(dir.join("p.png.ocr.json"), saved.to_string()).expect("saved file");
        let boxes = read_boxes(&picture);
        assert_eq!(boxes.len(), 1, "the malformed line is skipped");
        assert_eq!(boxes[0], [[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 4.0]]);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
    #[test]
    fn the_lines_file_sits_beside_its_picture() {
        assert_eq!(ocr_json_path(Path::new("/page/assets/x.png")), PathBuf::from("/page/assets/x.png.ocr.json"));
    }

    /// The (👁️) opener appears when the pointer is near a picture, and not otherwise.
    #[test]
    fn the_opener_shows_when_the_pointer_is_near_the_picture() {
        let page = Path::new("/page");
        let picture = Path::new("/page/assets/x.png");
        let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 150.0));
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let area = Id::new(("image-text-menu", page.to_path_buf(), 0usize));
        let run = |pointer: Option<Pos2>| {
            let ctx = egui::Context::default();
            crate::fonts::install_once(&ctx);
            let mut input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), ..Default::default() };
            if let Some(p) = pointer {
                input.events.push(egui::Event::PointerMoved(p));
            }
            let mut acts = Vec::new();
            // Two frames: the pointer position is known from the first.
            for _ in 0..2 {
                let mut output = ctx.run_ui(input.clone(), |ui| {
                    menu(ui, rect, page, 0, picture, "assets/x.png", &mut acts, &tokens);
                });
                output.textures_delta.clear();
            }
            ctx.memory(|m| m.area_rect(area)).is_some()
        };
        assert!(run(Some(pos2(105.0, 95.0))), "the opener shows with the pointer just outside the picture's corner");
        assert!(!run(Some(pos2(600.0, 500.0))), "and stays hidden with the pointer far away");
    }

    #[test]
    fn the_overlay_is_per_block_and_off_by_default() {
        let ctx = egui::Context::default();
        let page = Path::new("/page");
        assert!(!showing(&ctx, page, 3));
        set_showing(&ctx, page, 3, true);
        assert!(showing(&ctx, page, 3));
        assert!(!showing(&ctx, page, 4), "another block is unaffected");
    }
}

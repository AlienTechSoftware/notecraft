//! The (>) menu on a picture: detected text as an overlay. This is view state only: it changes how the
//! picture is shown, not what the page contains, so it stays off the undo history. The boxes come from
//! the picture's `.lines` file, which the page's OCR writes beside the picture.

use crate::floating;
use crate::ribbon::Act;
use crate::theme::Tokens;
use egui::{Color32, Id, Pos2, Rect, Stroke, pos2, vec2};
use std::path::{Path, PathBuf};

/// The `.lines` file beside a picture: its full file name with `.lines` added.
pub fn lines_path(picture: &Path) -> PathBuf {
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    picture.with_file_name(format!("{name}.lines"))
}

/// The boxes in a `.lines` file. Each row is `x0,y0,...,y3<TAB>text`, with the four corners in picture
/// pixels. A row that cannot be read is skipped.
pub fn read_boxes(lines: &Path) -> Vec<[[f32; 2]; 4]> {
    let Ok(text) = std::fs::read_to_string(lines) else { return Vec::new() };
    text.lines()
        .filter_map(|row| {
            let coords = row.split('\t').next()?;
            let v: Vec<f32> = coords.split(',').map(|s| s.trim().parse::<f32>()).collect::<Result<_, _>>().ok()?;
            if v.len() != 8 || v.iter().any(|x| !x.is_finite()) {
                return None;
            }
            Some([[v[0], v[1]], [v[2], v[3]], [v[4], v[5]], [v[6], v[7]]])
        })
        .collect()
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

fn set_showing(ctx: &egui::Context, page: &Path, block: usize, on: bool) {
    ctx.data_mut(|d| d.insert_temp(overlay_id(page, block), on));
}

/// The (>) button above a picture's top left corner, and its menu. The menu only changes the overlay
/// state. "Detect text" is offered once the picture has a `.lines` file to show.
pub fn menu(ui: &mut egui::Ui, picture_rect: Rect, page: &Path, block: usize, picture: &Path, acts: &mut Vec<Act>, t: &Tokens) {
    // Above the picture's top left corner, in the row the frames use for their emoji.
    let at = picture_rect.left_top() + vec2(0.0, -24.0);
    egui::Area::new(Id::new(("image-text-menu", page.to_path_buf(), block))).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        let opener = floating::round_button(ui, ">", 20.0, true);
        floating::button_menu(&opener, |ui| {
            let boxes = read_boxes(&lines_path(picture));
            if showing(ui.ctx(), page, block) {
                if floating::menu_row(ui, None, crate::tl_id!("ui.image_text_hide"), t).clicked() {
                    set_showing(ui.ctx(), page, block, false);
                    ui.close();
                }
            } else if floating::menu_row(ui, None, crate::tl_id!("ui.image_text_detect"), t).clicked() {
                if boxes.is_empty() {
                    // No text read yet: the app reads the page's pictures; the overlay comes once they are read.
                    acts.push(Act::ReadPageText);
                } else {
                    set_showing(ui.ctx(), page, block, true);
                }
                ui.close();
            }
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
    fn a_lines_file_gives_one_box_per_row_and_skips_bad_rows() {
        let dir = std::env::temp_dir().join(format!("nc-image-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("p.png.lines");
        std::fs::write(&file, "1,2,3,2,3,4,1,4\tRye loaf 6.00\nnot,a,box\tx\n20,50,100,50,100,70,20,70\tFlat white\n").expect("write");
        let boxes = read_boxes(&file);
        assert_eq!(boxes.len(), 2, "the bad row is skipped");
        assert_eq!(boxes[0], [[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 4.0]]);
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn the_lines_file_sits_beside_its_picture() {
        assert_eq!(lines_path(Path::new("/page/assets/x.png")), PathBuf::from("/page/assets/x.png.lines"));
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

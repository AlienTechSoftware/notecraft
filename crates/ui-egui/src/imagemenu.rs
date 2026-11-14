//! The context menu for an image: "Save picture as...".
//!
//! Opened when an image in the note is right-clicked.

use crate::floating::{self, Glyph};
use crate::icons::Icon;
use crate::ribbon::Act;
use crate::theme::Tokens;
use egui::Ui;
use std::path::{Path, PathBuf};

/// Where the menu is open and for which image source.
pub fn state(ctx: &egui::Context) -> Option<(egui::Pos2, String)> {
    ctx.data(|d| d.get_temp::<(egui::Pos2, String)>(id()))
}

pub fn id() -> egui::Id {
    egui::Id::new("image-menu")
}

/// Validates that `src` is a safe path within `page_dir/assets/` and reads the original file bytes
/// without re-encoding. Returns the resolved file path on disk, default file name, and raw bytes.
pub fn validate_and_read_image(page_dir: &Path, src: &str) -> Result<(PathBuf, String, Vec<u8>), std::io::Error> {
    let rel = src.strip_prefix("assets/").ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "path must begin with assets/"))?;
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') || rel.starts_with('/') {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid path"));
    }
    let source_path = page_dir.join("assets").join(rel);
    let bytes = std::fs::read(&source_path)?;
    let default_name = Path::new(rel).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "picture.png".to_owned());
    Ok((source_path, default_name, bytes))
}

/// Draws the image context menu when an image has been right-clicked, and puts what was picked
/// into `acts`.
pub fn show(ui: &mut Ui, body: egui::Rect, acts: &mut Vec<Act>, t: &Tokens) {
    if crate::containers::took_right_click(ui.ctx()) {
        ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
        return;
    }
    let pointer = ui.ctx().pointer_latest_pos();
    let opened_now = ui.input(|i| i.pointer.secondary_clicked());
    if opened_now {
        let rendered_images = ui.data(|d| d.get_temp::<Vec<crate::preview::RenderedImage>>(crate::preview::rendered_images_id())).unwrap_or_default();
        let hit_image = pointer.filter(|p| body.contains(*p)).and_then(|p| rendered_images.iter().rev().find(|img| img.rect.contains(p)).cloned());

        if let Some(hit) = hit_image {
            let at = pointer.unwrap_or(hit.rect.min);
            log::debug!("image menu: right-clicked at {:?} on {:?}", at, hit.src);
            ui.data_mut(|d| d.insert_temp(id(), (at, hit.src)));
            crate::containers::claim_right_click(ui.ctx());
        } else {
            ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
        }
    }
    let Some((at, src)) = state(ui.ctx()) else { return };

    let mut picked = false;
    let area = egui::Area::new(id().with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                ui.set_width(floating::MENU_W);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                if floating::menu_row(ui, Some(Glyph::Icon(Icon::Picture)), crate::tl_id!("ui.save_picture_as"), t).clicked() {
                    acts.push(Act::SavePictureAs(src.clone()));
                    picked = true;
                }
            });
        })
        .response;

    let away = |p: egui::Pos2| !area.rect.contains(p);
    let out = ui.input(|i| i.pointer.any_click()) && !opened_now && away(pointer.unwrap_or_default());
    if picked || out || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_validate_and_read_image_success() {
        let t = notecraft_store::testing::TempDir::new("imgmenu-valid");
        let page_dir = t.path();
        let assets_dir = page_dir.join("assets");
        fs::create_dir_all(&assets_dir).expect("create assets dir");

        let original_data = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR_raw_test_data";
        let img_path = assets_dir.join("photo.png");
        fs::write(&img_path, original_data).expect("write test image");

        let (resolved_path, name, bytes) = validate_and_read_image(page_dir, "assets/photo.png").expect("validate image");
        assert_eq!(resolved_path, img_path);
        assert_eq!(name, "photo.png");
        assert_eq!(bytes, original_data);
    }

    #[test]
    fn test_validate_and_read_image_rejects_unsafe_paths() {
        let t = notecraft_store::testing::TempDir::new("imgmenu-unsafe");
        let page_dir = t.path();

        assert!(validate_and_read_image(page_dir, "assets/../outside.png").is_err());
        assert!(validate_and_read_image(page_dir, "assets/sub\\nested.png").is_err());
        assert!(validate_and_read_image(page_dir, "assets//root.png").is_err());
        assert!(validate_and_read_image(page_dir, "assets/").is_err());
        assert!(validate_and_read_image(page_dir, "not_assets/pic.png").is_err());
    }

    #[test]
    fn test_validate_and_read_image_missing_file_fails() {
        let t = notecraft_store::testing::TempDir::new("imgmenu-missing");
        let page_dir = t.path();
        let res = validate_and_read_image(page_dir, "assets/missing.png");
        assert!(res.is_err());
        assert_eq!(res.err().map(|e| e.kind()), Some(std::io::ErrorKind::NotFound));
    }

    #[test]
    fn test_image_menu_opens_on_image_right_click_and_claims() {
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

        let mut acts = Vec::new();
        let pos = egui::pos2(150.0, 150.0);
        let mut raw_input = egui::RawInput::default();
        raw_input.events.push(egui::Event::PointerMoved(pos));
        raw_input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });
        raw_input.events.push(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        });

        let mut claimed = false;
        let mut frame = ctx.run_ui(raw_input, |ui| {
            crate::preview::record_rendered_image(
                ui.ctx(),
                0,
                egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(200.0, 150.0)),
                "assets/sample.png",
            );
            show(ui, body, &mut acts, &tokens);
            claimed = crate::containers::took_right_click(ui.ctx());
        });
        frame.textures_delta.clear();

        assert!(claimed);
        let open_state = state(&ctx);
        assert!(open_state.is_some());
        let (pos, src) = open_state.expect("open state");
        assert_eq!(pos, egui::pos2(150.0, 150.0));
        assert_eq!(src, "assets/sample.png");
    }

    #[test]
    fn test_image_menu_does_not_open_outside_image() {
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

        let mut acts = Vec::new();
        let mut raw_input = egui::RawInput::default();
        // Right click at (500, 500), outside the image at (100, 100) -> (300, 250)
        raw_input.events.push(egui::Event::PointerButton {
            pos: egui::pos2(500.0, 500.0),
            button: egui::PointerButton::Secondary,
            pressed: true,
            modifiers: egui::Modifiers::default(),
        });

        let mut claimed = false;
        let mut frame = ctx.run_ui(raw_input, |ui| {
            crate::preview::record_rendered_image(
                ui.ctx(),
                0,
                egui::Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(200.0, 150.0)),
                "assets/sample.png",
            );
            show(ui, body, &mut acts, &tokens);
            claimed = crate::containers::took_right_click(ui.ctx());
        });
        frame.textures_delta.clear();

        assert!(!claimed);
        assert_eq!(state(&ctx), None);
    }

    #[test]
    fn test_image_menu_click_emits_save_picture_as_action() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let acts = Vec::new();

        let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
            |ui, acts: &mut Vec<Act>| {
                show(ui, body, acts, &tokens);
            },
            acts,
        );

        // Pre-set state as open at (100, 100)
        h.ctx.data_mut(|d| d.insert_temp(id(), (egui::pos2(100.0, 100.0), "assets/my_photo.png".to_string())));
        h.run();

        h.get_by_label("Save picture as...").click();
        h.run();

        assert_eq!(*h.state(), vec![Act::SavePictureAs("assets/my_photo.png".to_string())]);
        assert_eq!(state(&h.ctx), None);
    }

    #[test]
    fn test_image_menu_closes_on_escape() {
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));

        // Pre-set state as open
        ctx.data_mut(|d| d.insert_temp(id(), (egui::pos2(100.0, 100.0), "assets/sample.png".to_string())));
        assert!(state(&ctx).is_some());

        let mut acts = Vec::new();
        let mut raw_input = egui::RawInput::default();
        raw_input.events.push(egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::default(),
        });

        let mut frame = ctx.run_ui(raw_input, |ui| {
            show(ui, body, &mut acts, &tokens);
        });
        frame.textures_delta.clear();

        assert_eq!(state(&ctx), None);
    }
}

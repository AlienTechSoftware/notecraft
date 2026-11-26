//! The context menu for an image: "Save picture as...", Copy Text from Picture (issue #274),
//! and Rotate and Flip (issue #273).
//!
//! Opened when an image in the note is right-clicked.

use crate::floating::{self, Glyph};
use crate::icons::Icon;
use crate::ribbon::Act;
use crate::ribbon_icons::Mark;
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

/// The block of the picture the menu is open for, kept beside its state: two copies of one
/// picture share a source but are two pictures.
fn block_id() -> egui::Id {
    id().with("block")
}

/// Validates that `src` is a safe path within `page_dir/assets/` and reads the original file bytes
/// without re-encoding. Returns the resolved file path on disk, default file name, and raw bytes.
pub fn validate_and_read_image(page_dir: &Path, src: &str) -> Result<(PathBuf, String, Vec<u8>), std::io::Error> {
    let source_path =
        crate::preview::page_asset_path(page_dir, src).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid asset path"))?;
    let bytes = std::fs::read(&source_path)?;
    let default_name = source_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "picture.png".to_owned());
    Ok((source_path, default_name, bytes))
}

/// The picture's menu was opened from the keyboard (Shift+F10 or the Menu key on a picture
/// selected with the keyboard, issue #380), so Escape gives the keyboard back to the page.
fn keyed_id() -> egui::Id {
    id().with("keyed")
}

/// Draws the image context menu when an image has been right-clicked, or when a picture of
/// `page` that is selected asks for its menu from the keyboard, and puts what was picked into
/// `acts`.
pub fn show(ui: &mut Ui, body: egui::Rect, page: &Path, acts: &mut Vec<Act>, t: &Tokens) {
    if crate::containers::took_right_click(ui.ctx()) {
        ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
        return;
    }
    let pointer = ui.ctx().pointer_latest_pos();
    let opened_now = ui.input(|i| i.pointer.secondary_clicked());
    let rendered_images = || ui.data(|d| d.get_temp::<Vec<crate::preview::RenderedImage>>(crate::preview::rendered_images_id())).unwrap_or_default();
    if opened_now {
        let hit_image = pointer.filter(|p| body.contains(*p)).and_then(|p| rendered_images().iter().rev().find(|img| img.rect.contains(p)).cloned());

        if let Some(hit) = hit_image {
            let at = pointer.unwrap_or(hit.rect.min);
            log::debug!("image menu: right-clicked at {:?} on {:?}", at, hit.src);
            ui.data_mut(|d| {
                d.insert_temp(id(), (at, hit.src));
                d.insert_temp(block_id(), hit.block);
                d.remove::<bool>(keyed_id());
            });
            crate::containers::claim_right_click(ui.ctx());
        } else {
            ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
        }
    } else if state(ui.ctx()).is_none()
        && let Some(block) = crate::containers::selected_image(ui.ctx(), page)
        && let Some(hit) = rendered_images().into_iter().find(|img| img.block == block)
        && floating::menu_keys_pressed(ui.ctx())
    {
        // Under the picture's top-left corner, as if the click had fallen just inside it.
        let at = hit.rect.min + egui::vec2(8.0, 8.0);
        log::debug!("image menu: opened from the keyboard on {:?}", hit.src);
        ui.data_mut(|d| {
            d.insert_temp(id(), (at, hit.src));
            d.insert_temp(block_id(), hit.block);
            d.insert_temp(keyed_id(), true);
        });
        floating::focus_first_row(ui.ctx(), id());
    }
    let Some((at, src)) = state(ui.ctx()) else { return };

    let mut picked = false;
    let Some(menu) = floating::point_menu(ui.ctx(), id(), egui::Rect::from_pos(at), |ui| {
        floating::menu_heading(ui, crate::tl_id!("ui.this_picture"), t);
        if floating::menu_row(ui, Some(Glyph::Icon(Icon::Picture)), crate::tl_id!("ui.save_picture_as"), t).clicked() {
            acts.push(Act::SavePictureAs(src.clone()));
            picked = true;
        }
        // The printed text of the picture, or of a printout's page or every page (issue #274).
        if crate::picture_text::printout_source(&src).is_some() {
            if floating::menu_row(ui, None, crate::tl_id!("ui.copy_text_from_this_printout_page"), t).clicked() {
                acts.push(Act::CopyTextFromPicture { src: src.clone(), whole_printout: false });
                picked = true;
            }
            if floating::menu_row(ui, None, crate::tl_id!("ui.copy_text_from_all_printout_pages"), t).clicked() {
                acts.push(Act::CopyTextFromPicture { src: src.clone(), whole_printout: true });
                picked = true;
            }
        } else if floating::menu_row(ui, None, crate::tl_id!("ui.copy_text_from_picture"), t).clicked() {
            acts.push(Act::CopyTextFromPicture { src: src.clone(), whole_printout: false });
            picked = true;
        }
        // A picture or printout that shares a box with other blocks can be taken out of it
        // where it stands, as dragging it out does, without the drag (issue #481).
        if let Some(block) = ui.data(|d| d.get_temp::<usize>(block_id()))
            && crate::containers::can_unbox(ui.ctx(), block)
            && floating::menu_row(ui, None, crate::tl_id!("ui.take_out_of_box"), t).clicked()
        {
            ui.data_mut(|d| d.insert_temp(crate::containers::unbox_request_id(), block));
            ui.ctx().request_repaint();
            picked = true;
        }
        // The clipboard rows of the page's menu, added below the picture's own entries so their
        // order and keyboard focus are unchanged. Cut and Copy act on this picture.
        let block = ui.data(|d| d.get_temp::<usize>(block_id()));
        if let Some(block) = block {
            if floating::menu_row(ui, Some(Glyph::Mark(Mark::Cut)), crate::tl_id!("ui.cut"), t).clicked() {
                acts.push(Act::PictureClipboard { src: src.clone(), block, cut: true });
                picked = true;
            }
            if floating::menu_row(ui, Some(Glyph::Mark(Mark::Copy)), crate::tl_id!("ui.copy"), t).clicked() {
                acts.push(Act::PictureClipboard { src: src.clone(), block, cut: false });
                picked = true;
            }
        }
        if floating::menu_row(ui, Some(Glyph::Mark(Mark::Paste)), crate::tl_id!("ui.paste"), t).clicked() {
            acts.push(Act::Paste);
            picked = true;
        }
        floating::menu_separator(ui, t);
        for (glyph, label, how) in [
            (Some(Glyph::Icon(Icon::Redo)), crate::tl_id!("ui.rotate_right_90"), crate::turn::Turn::Right),
            (Some(Glyph::Icon(Icon::Undo)), crate::tl_id!("ui.rotate_left_90"), crate::turn::Turn::Left),
            (None, crate::tl_id!("ui.flip_horizontal"), crate::turn::Turn::FlipHorizontal),
            (None, crate::tl_id!("ui.flip_vertical"), crate::turn::Turn::FlipVertical),
        ] {
            if floating::menu_row(ui, glyph, label, t).clicked() {
                acts.push(Act::TurnPicture(src.clone(), how));
                picked = true;
            }
        }
    }) else {
        return;
    };
    let area = menu.response;

    let away = |p: egui::Pos2| !area.rect.contains(p);
    let out = ui.input(|i| i.pointer.any_click()) && !opened_now && away(pointer.unwrap_or_default());
    if picked || out || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<(egui::Pos2, String)>(id()));
        // From the keyboard, the keyboard is left with nothing in particular; the picture stays
        // selected, so Shift+F10 opens the menu again.
        ui.data_mut(|d| d.remove::<bool>(keyed_id()));
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
            show(ui, body, std::path::Path::new("page"), &mut acts, &tokens);
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
            show(ui, body, std::path::Path::new("page"), &mut acts, &tokens);
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
                show(ui, body, std::path::Path::new("page"), acts, &tokens);
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

    /// Issue #273: the menu offers the four turns, each for the picture it was opened on.
    #[test]
    fn test_image_menu_offers_rotate_and_flip() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        for (label, how) in [
            ("Rotate Right 90°", crate::turn::Turn::Right),
            ("Rotate Left 90°", crate::turn::Turn::Left),
            ("Flip Horizontal", crate::turn::Turn::FlipHorizontal),
            ("Flip Vertical", crate::turn::Turn::FlipVertical),
        ] {
            let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
                |ui, acts: &mut Vec<Act>| {
                    show(ui, body, std::path::Path::new("page"), acts, &tokens);
                },
                Vec::new(),
            );
            h.ctx.data_mut(|d| d.insert_temp(id(), (egui::pos2(100.0, 100.0), "assets/scan.png".to_string())));
            h.run();
            h.get_by_label(label).click();
            h.run();
            assert_eq!(*h.state(), vec![Act::TurnPicture("assets/scan.png".to_string(), how)], "{label}");
            assert_eq!(state(&h.ctx), None, "{label} closes the menu");
        }
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
            show(ui, body, std::path::Path::new("page"), &mut acts, &tokens);
        });
        frame.textures_delta.clear();

        assert_eq!(state(&ctx), None);
    }

    /// The menu offers Copy Text from Picture for a picture, and for a printout's page both that
    /// page and the whole printout (issue #274).
    #[test]
    fn copy_text_rows_fit_what_was_right_clicked() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        for (src, label, whole) in [
            ("assets/receipt.png", "Copy Text from Picture", false),
            ("assets/report.pdf.printout-002.png", "Copy Text from This Page of the Printout", false),
            ("assets/report.pdf.printout-002.png", "Copy Text from All Pages of the Printout", true),
        ] {
            let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
                |ui, acts: &mut Vec<Act>| {
                    show(ui, body, std::path::Path::new("page"), acts, &tokens);
                },
                Vec::new(),
            );
            h.ctx.data_mut(|d| d.insert_temp(id(), (egui::pos2(100.0, 100.0), src.to_string())));
            h.run();
            let printout = src.contains(".printout-");
            assert_eq!(h.query_by_label("Copy Text from Picture").is_some(), !printout, "{src}");
            assert_eq!(h.query_by_label("Copy Text from All Pages of the Printout").is_some(), printout, "{src}");
            h.get_by_label(label).click();
            h.run();
            assert_eq!(*h.state(), vec![Act::CopyTextFromPicture { src: src.to_string(), whole_printout: whole }]);
        }
    }

    /// The picture's menu always offers Cut, Copy and Paste, as the page's does. Cut and Copy act on
    /// the picture at the block the menu was opened on (issue: Copy, Cut and Paste were lost from it).
    #[test]
    fn the_picture_menu_offers_cut_copy_and_paste() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable;

        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let body = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
        let mut h = Harness::builder().with_size(egui::vec2(800.0, 600.0)).with_accessibility_check(false).build_ui_state(
            |ui, acts: &mut Vec<Act>| {
                show(ui, body, std::path::Path::new("page"), acts, &tokens);
            },
            Vec::new(),
        );
        h.ctx.data_mut(|d| {
            d.insert_temp(id(), (egui::pos2(100.0, 100.0), "assets/photo.png".to_string()));
            d.insert_temp(block_id(), 2usize);
        });
        h.run();
        for label in ["Cut", "Copy", "Paste"] {
            assert!(h.query_by_label(label).is_some(), "{label} is on the picture's menu");
        }
        h.get_by_label("Copy").click();
        h.run();
        assert_eq!(*h.state(), vec![Act::PictureClipboard { src: "assets/photo.png".to_string(), block: 2, cut: false }]);
    }
}

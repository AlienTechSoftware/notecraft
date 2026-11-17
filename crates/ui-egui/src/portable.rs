//! Shared review surface for Export, Share and Home > Send Page.

use crate::{
    backstage::{CopyChoice, CopyScope},
    theme::Tokens,
};
use notecraft_store::export::Prepared;
use std::io::{Cursor, Write};
use std::path::PathBuf;

#[derive(Default)]
pub struct Choice {
    pub selection: CopyChoice,
    pub download: bool,
    pub downloaded: bool,
}

pub enum Act {
    Review(PathBuf),
    Cancel,
    ChooseFolder,
    Write,
    Reveal(PathBuf),
}

pub fn show(ui: &mut egui::Ui, choice: &mut Choice, review: Option<&Prepared>, t: &Tokens) -> Vec<Act> {
    egui::ScrollArea::vertical().id_salt("portable-review").auto_shrink([false, false]).show(ui, |ui| contents(ui, choice, review, t)).inner
}

fn contents(ui: &mut egui::Ui, choice: &mut Choice, mut review: Option<&Prepared>, t: &Tokens) -> Vec<Act> {
    let mut acts = Vec::new();
    ui.heading(crate::tl_id!("ui.portable_export_title"));
    ui.label(crate::tl_id!("ui.portable_export_explanation"));
    ui.label(crate::tl_id!("ui.portable_share_fallback"));
    if choice.selection.here.is_none() {
        ui.label(crate::tl_id!("ui.portable_open_page"));
        return acts;
    }
    ui.add_space(8.0);
    for (scope, label) in [
        (CopyScope::Page, crate::tl_id!("ui.this_page")),
        (CopyScope::Section, crate::tl_id!("ui.this_section")),
        (CopyScope::Notebook, crate::tl_id!("ui.this_notebook")),
    ] {
        if let Some(node) = choice.selection.node(scope) {
            let title = notecraft_store::tree::read_node(&node).map(|n| n.title).unwrap_or_default();
            let text = crate::i18n::format_message_id("ui.scope_title", &[("scope", label), ("title", &title)]);
            if ui.radio(choice.selection.scope == scope, text).clicked() && choice.selection.scope != scope {
                choice.selection.scope = scope;
                choice.selection.result = None;
                choice.downloaded = false;
                review = None;
                acts.push(Act::Cancel);
            }
        }
    }
    ui.label(crate::tl_id!("ui.portable_original_formats"));
    if choice.download {
        ui.label(crate::tl_id!("ui.portable_download_destination"));
    } else {
        let label = ui.label(crate::tl_id!("ui.portable_destination"));
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 80.0).clamp(40.0, 360.0);
            ui.add(egui::TextEdit::singleline(&mut choice.selection.folder).desired_width(width)).labelled_by(label.id);
            if ui.button(crate::tl_id!("ui.choose")).clicked() {
                acts.push(Act::ChooseFolder);
            }
        });
        ui.label(crate::tl_id!("ui.portable_no_overwrite"));
    }
    ui.horizontal(|ui| {
        if ui.button(crate::tl_id!("ui.portable_review")).clicked()
            && let Some(node) = choice.selection.node(choice.selection.scope)
        {
            acts.push(Act::Review(node));
        }
        if ui.button(crate::tl_id!("ui.portable_cancel")).clicked() {
            choice.selection.result = None;
            choice.downloaded = false;
            acts.push(Act::Cancel);
        }
    });
    if let Some(review) = review {
        ui.separator();
        ui.label(crate::i18n::format_message_id(
            "ui.portable_review_summary",
            &[
                ("title", &review.title),
                ("pages", &review.pages.to_string()),
                ("files", &review.files.len().to_string()),
                ("bytes", &review.bytes.to_string()),
            ],
        ));
        if !review.dependencies.is_empty() {
            ui.colored_label(t.warn, crate::tl_id!("ui.portable_dependencies"));
            for path in &review.dependencies {
                ui.label(path);
            }
        }
        if !review.warnings.is_empty() {
            ui.colored_label(t.warn, crate::tl_id!("ui.portable_external_links"));
            for link in &review.warnings {
                ui.label(link);
            }
        }
        ui.collapsing(crate::tl_id!("ui.portable_included_files"), |ui| {
            egui::ScrollArea::vertical().id_salt("portable-files").max_height(160.0).show(ui, |ui| {
                for file in &review.files {
                    ui.label(file);
                }
            });
        });
        ui.collapsing(crate::tl_id!("ui.portable_page_contents"), |ui| {
            egui::ScrollArea::vertical().id_salt("portable-text").max_height(240.0).show(ui, |ui| {
                for (path, bytes) in review.entries() {
                    let format = match path.file_name().and_then(|s| s.to_str()) {
                        Some("index.html") => notecraft_model::PageFormat::Html,
                        Some("index.md") => notecraft_model::PageFormat::Cmgfm,
                        _ => continue,
                    };
                    if let Ok(text) = std::str::from_utf8(bytes) {
                        ui.strong(path.to_string_lossy());
                        let body = match format {
                            notecraft_model::PageFormat::Html => notecraft_formats::html::inner(text),
                            notecraft_model::PageFormat::Cmgfm => notecraft_formats::frontmatter::split_front(text).1,
                        };
                        let (blocks, _) = notecraft_formats::boxes::read(format, &body);
                        ui.label(notecraft_formats::render::blocks_to_text(&blocks));
                    }
                }
            });
        });
        ui.label(crate::tl_id!("ui.portable_review_snapshot"));
        if ui
            .add_enabled(choice.download || !choice.selection.folder.trim().is_empty(), egui::Button::new(crate::tl_id!("ui.portable_write")))
            .clicked()
        {
            acts.push(Act::Write);
        }
    }
    if choice.downloaded {
        ui.label(crate::tl_id!("ui.portable_download_ready"));
    }
    match &choice.selection.result {
        Some(Ok(path)) => {
            ui.label(crate::i18n::format_message_id("ui.portable_saved", &[("path", &path.to_string_lossy())]));
            if ui.button(crate::tl_id!("ui.open_folder")).clicked() {
                acts.push(Act::Reveal(path.clone()));
            }
        }
        Some(Err(error)) => {
            ui.colored_label(t.warn, error);
        }
        None => {}
    }
    acts
}

/// Download hosts receive the same reviewed files, wrapped in an ordinary ZIP archive.
pub fn archive(prepared: &Prepared) -> std::io::Result<Vec<u8>> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (path, bytes) in prepared.entries() {
        let name = format!("{}/{}", prepared.notebook_name(), path.to_string_lossy().replace('\\', "/"));
        zip.start_file(name, options).map_err(std::io::Error::other)?;
        zip.write_all(bytes)?;
    }
    Ok(zip.finish().map_err(std::io::Error::other)?.into_inner())
}

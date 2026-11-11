//! The File screen: what Office calls the backstage. It takes over the window and shows the
//! notebooks with where they live, and the things done to a whole notebook rather than to a
//! page. Anything that needs a service NoteCraft does not have is listed and marked as not
//! built, rather than hidden, so nobody goes looking for it.

use crate::icons::{self, Icon};
use crate::theme::Tokens;
use egui::{Align2, Sense, Ui, vec2};
use notecraft_model::NodeColor;
use notecraft_store::tree::Node;

/// What the File screen asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Leave the File screen.
    Close,
    NewNotebook,
    /// Show this notebook's folder in the file manager.
    OpenFolder(std::path::PathBuf),
    /// Show the notebooks folder in the file manager.
    OpenNotebooksFolder,
    /// Ask the system for a folder, and work from the notebooks in it.
    ChooseFolder,
    /// Choose a OneNote notebook folder and pass it to the OfficeIMO bridge.
    ImportNotebookFolder,
    /// Choose one importable file and pass it to the OfficeIMO bridge.
    ImportFile,
    /// Cloud import is intentionally visible but unavailable.
    ImportCloud,
    /// Open a page of this notebook (the first one).
    Open(std::path::PathBuf),
    /// Open a notebook from the recent list.
    OpenRecent(std::path::PathBuf),
    /// Forget a notebook in the known notebooks list without touching its files.
    RemoveRecent(std::path::PathBuf),
    /// Ask for confirmation before deleting a notebook from disk.
    DeleteRecent(std::path::PathBuf, String),
    SetTelemetryLevel(String),
    SetUiLanguage(String),
    OpenTelemetryFolder(std::path::PathBuf),
    ClearTelemetry(std::path::PathBuf),
    ExportTelemetry(std::path::PathBuf),
    ResetPreferences,
    /// Open the dictation setup screen.
    #[cfg(feature = "voice")]
    DictationSetup,
    /// Another page of the File screen: Info, New or Open.
    Page(&'static str),
    /// A side entry that is listed but not built yet.
    NotBuilt(&'static str),
}

/// The entries down the left side, in the order the Office apps use. The flag says whether
/// it works.
pub const SIDE: [(&str, bool); 10] = [
    ("Info", true),
    ("New", true),
    ("Open", true),
    ("Save a Copy", false),
    ("Print", false),
    ("Share", false),
    ("Export", false),
    ("Account", false),
    ("Options", true),
    ("About", true),
];

/// Where NoteCraft comes from. Named once, here, so there is one place to change it.
pub const REPOSITORY: &str = "https://github.com/AlienTechSoftware/notecraft";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Draws the File screen over the whole window. Returns what the user asked for.
pub fn show(
    ui: &mut Ui,
    notebooks: &[Node],
    recent: &[Node],
    notebooks_folder: &str,
    page: &str,
    show_path: &dyn Fn(&std::path::Path) -> String,
    telemetry_root: &std::path::Path,
    telemetry_level: &str,
    session_id: Option<&str>,
    ui_language: &str,
    languages: &[crate::i18n::LanguageChoice],
    t: &Tokens,
) -> Vec<Act> {
    let mut acts = Vec::new();
    egui::Frame::NONE.fill(t.panel).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            side_bar(ui, page, t, &mut acts);
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add_space(10.0);
                match page {
                    "New" => new_page(ui, notebooks_folder, t, &mut acts),
                    "Open" => open_page(ui, recent, notebooks_folder, show_path, t, &mut acts),
                    "About" => about_page(ui, telemetry_root, session_id, t),
                    "Options" => options_page(ui, telemetry_root, telemetry_level, session_id, ui_language, languages, t, &mut acts),
                    _ => info_page(ui, notebooks, notebooks_folder, show_path, t, &mut acts),
                }
            });
        });
    });
    acts
}

fn side_bar(ui: &mut Ui, page: &str, t: &Tokens, acts: &mut Vec<Act>) {
    ui.vertical(|ui| {
        ui.set_width(170.0);
        ui.add_space(8.0);
        // The arrow back to the notebook, as in Office.
        let (rect, back) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
        let painter = ui.painter_at(rect);
        if back.hovered() {
            painter.circle_filled(rect.center(), 15.0, t.row_hover);
        }
        painter.circle_stroke(rect.center(), 11.0, egui::Stroke::new(1.5, t.accent));
        painter.line_segment(
            [egui::pos2(rect.center().x + 5.0, rect.center().y), egui::pos2(rect.center().x - 4.0, rect.center().y)],
            egui::Stroke::new(1.5, t.accent),
        );
        painter.line_segment(
            [egui::pos2(rect.center().x - 4.0, rect.center().y), egui::pos2(rect.center().x, rect.center().y - 4.0)],
            egui::Stroke::new(1.5, t.accent),
        );
        painter.line_segment(
            [egui::pos2(rect.center().x - 4.0, rect.center().y), egui::pos2(rect.center().x, rect.center().y + 4.0)],
            egui::Stroke::new(1.5, t.accent),
        );
        let enabled = ui.is_enabled();
        let back_label = crate::tl_id!("ui.back");
        back.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, back_label));
        if back.on_hover_text(crate::tl_id!("ui.back_to_the_notebook")).clicked() {
            acts.push(Act::Close);
        }
        ui.add_space(8.0);
        for (name, built) in SIDE {
            let selected = name == page;
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
            let painter = ui.painter_at(rect);
            if selected {
                painter.rect_filled(rect, 4.0, t.row_selected);
            } else if resp.hovered() {
                painter.rect_filled(rect, 4.0, t.row_hover);
            }
            let color = if built { t.text } else { t.muted };
            painter.text(
                egui::pos2(rect.min.x + 12.0, rect.center().y),
                Align2::LEFT_CENTER,
                crate::tl!(name),
                egui::FontId::proportional(13.5),
                color,
            );
            if !built {
                painter.text(
                    egui::pos2(rect.max.x - 8.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    crate::tl_id!("ui.not_built_2"),
                    egui::FontId::proportional(10.0),
                    t.muted,
                );
            }
            let enabled = ui.is_enabled();
            let label = crate::tl!(name).to_string();
            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label.clone()));
            if resp.clicked() {
                acts.push(if built { Act::Page(name) } else { Act::NotBuilt(name) });
            }
        }
    });
}

// i18n-ui-text: text
fn heading(ui: &mut Ui, text: &str, t: &Tokens) {
    ui.label(egui::RichText::new(crate::tl!(text)).size(22.0).color(t.strong));
    ui.add_space(8.0);
}

fn info_page(ui: &mut Ui, notebooks: &[Node], folder: &str, show_path: &dyn Fn(&std::path::Path) -> String, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, crate::tl_id!("ui.notebook_information"), t);
    if notebooks.is_empty() {
        ui.label(egui::RichText::new(crate::tl_id!("ui.there_are_no_notebooks_yet_new_starts_one")).color(t.muted));
    }
    egui::ScrollArea::vertical().id_salt("backstage-info").show(ui, |ui| {
        for n in notebooks {
            ui.horizontal_top(|ui| {
                // The notebook's cover, in its own colour.
                let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::click());
                let painter = ui.painter_at(rect);
                let color = n
                    .info
                    .color
                    .map(|c| {
                        let (r, g, b) = c.rgb(t.panel.r() < 128);
                        egui::Color32::from_rgb(r, g, b)
                    })
                    .unwrap_or(t.notebook);
                icons::paint(&painter, rect.shrink(8.0), Icon::Notebook, color);
                let name = n.info.title.clone();
                let enabled = ui.is_enabled();
                let accessible_name = crate::i18n::format_message_id("ui.open_name", &[("name", &name)]);
                resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, accessible_name.clone()));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(crate::tl_id!("ui.open_this_notebook")).clicked() {
                    acts.push(Act::Open(n.path.clone()));
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&n.info.title).size(17.0).color(t.accent));
                    ui.label(egui::RichText::new(show_path(&n.path)).size(11.5).monospace().color(t.muted));
                    ui.horizontal(|ui| {
                        let open_folder =
                            egui::Button::new(crate::tl_id!("ui.open_folder")).small().stroke(egui::Stroke::new(1.0, t.container_border));
                        if ui.add(open_folder).clicked() {
                            acts.push(Act::OpenFolder(n.path.clone()));
                        }
                        let count = n.count().to_string();
                        let colour = n.info.color.map(NodeColor::label).unwrap_or(crate::tl_id!("ui.no"));
                        let summary = crate::i18n::format_message_id("ui.count_items_colour_colour", &[("count", &count), ("colour", colour)]);
                        ui.label(egui::RichText::new(summary).size(11.0).color(t.muted));
                    });
                });
            });
            ui.add_space(10.0);
        }
    });
    ui.add_space(6.0);
    ui.separator();
    let location = crate::i18n::format_message_id("ui.all_of_them_live_in_folder", &[("folder", folder)]);
    ui.label(egui::RichText::new(location).size(11.5).color(t.muted));
    ui.label(egui::RichText::new(crate::tl_id!("ui.online_notebooks_onedrive_sharepoint_are_not_built_a_860f0281c0")).size(11.5).color(t.muted));
}

fn new_page(ui: &mut Ui, folder: &str, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, crate::tl_id!("ui.new_notebook"), t);
    let explanation = crate::i18n::format_message_id("ui.a_new_notebook_is_a_folder_in_folder_with_one_sectio_7208ff46e4", &[("folder", folder)]);
    ui.label(egui::RichText::new(explanation).color(t.text));
    ui.add_space(10.0);
    if ui.button(crate::tl_id!("ui.new_notebook")).clicked() {
        acts.push(Act::NewNotebook);
    }
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.somewhere_else")).size(14.0).color(t.strong));
    ui.label(egui::RichText::new(crate::tl_id!("ui.onedrive_and_sharepoint_not_built_point_notecraft_at_584930dc1b")).size(11.5).color(t.muted));
}

fn open_page(ui: &mut Ui, recent: &[Node], folder: &str, show_path: &dyn Fn(&std::path::Path) -> String, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, crate::tl_id!("ui.open_a_notebook"), t);
    ui.horizontal(|ui| {
        if ui.add_sized([230.0, 42.0], egui::Button::new(egui::RichText::new(crate::tl_id!("ui.browse_for_a_notebook")).size(15.0))).clicked() {
            acts.push(Act::ChooseFolder);
        }
        if open_action(ui, crate::tl_id!("ui.show_notebooks_folder"), 210.0, 42.0, t, false).clicked() {
            acts.push(Act::OpenNotebooksFolder);
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if open_action(ui, crate::tl_id!("ui.import_notebook_folder"), 230.0, 36.0, t, true).clicked() {
            acts.push(Act::ImportNotebookFolder);
        }
        if open_action(ui, crate::tl_id!("ui.import_file"), 230.0, 36.0, t, true).clicked() {
            acts.push(Act::ImportFile);
        }
        let cloud = ui.add_enabled(
            false,
            egui::Button::new(egui::RichText::new(crate::tl_id!("ui.import_from_the_cloud")).size(13.0)).min_size(vec2(190.0, 36.0)),
        );
        cloud.on_hover_text(crate::tl_id!("ui.cloud_import_is_not_implemented_yet"));
    });
    ui.add_space(22.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.recently_opened")).size(16.0).color(t.strong));
    ui.add_space(8.0);
    if recent.is_empty() {
        ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_you_open_will_appear_here")).size(13.0).color(t.muted));
        return;
    }
    egui::ScrollArea::vertical().id_salt("backstage-recent-notebooks").show(ui, |ui| {
        for notebook in recent {
            let width = ui.available_width().min(680.0);
            let (rect, response) = ui.allocate_exact_size(vec2(width, 70.0), Sense::click());
            let background = if response.hovered() { t.row_hover } else { t.panel };
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 5.0, background);
            painter.rect_stroke(rect, 5.0, egui::Stroke::new(1.0, t.faint), egui::StrokeKind::Inside);
            let cover = egui::Rect::from_center_size(egui::pos2(rect.left() + 32.0, rect.center().y), vec2(40.0, 46.0));
            let color = notebook
                .info
                .color
                .map(|c| {
                    let (r, g, b) = c.rgb(t.panel.r() < 128);
                    egui::Color32::from_rgb(r, g, b)
                })
                .unwrap_or(t.notebook);
            icons::paint(&painter, cover, Icon::Notebook, color);
            painter.text(
                egui::pos2(rect.left() + 64.0, rect.center().y - 9.0),
                Align2::LEFT_CENTER,
                &notebook.info.title,
                egui::FontId::proportional(16.0),
                t.text,
            );
            painter.text(
                egui::pos2(rect.left() + 64.0, rect.center().y + 13.0),
                Align2::LEFT_CENTER,
                show_path(&notebook.path),
                egui::FontId::monospace(11.0),
                t.muted,
            );
            painter.text(egui::pos2(rect.right() - 18.0, rect.center().y), Align2::CENTER_CENTER, "›", egui::FontId::proportional(25.0), t.accent);
            let name = notebook.info.title.clone();
            let accessible_name = crate::i18n::format_message_id("ui.open_name", &[("name", &name)]);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, accessible_name.clone()));
            let path = notebook.path.clone();
            response.context_menu(|ui| {
                if ui.button(crate::tl_id!("ui.remove_from_list")).clicked() {
                    acts.push(Act::RemoveRecent(path.clone()));
                    ui.close();
                }
                if ui.button(crate::tl_id!("ui.delete_notebook_2")).clicked() {
                    acts.push(Act::DeleteRecent(path.clone(), name.clone()));
                    ui.close();
                }
            });
            if response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::OpenRecent(notebook.path.clone()));
            }
            ui.add_space(6.0);
        }
    });
    ui.add_space(8.0);
    let hint = crate::i18n::format_message_id("ui.browse_notebooks_in_folder_or_choose_another_folder", &[("folder", folder)]);
    ui.label(egui::RichText::new(hint).size(11.5).color(t.muted));
}

// i18n-ui-text: label
fn open_action(ui: &mut Ui, label: &str, width: f32, height: f32, t: &Tokens, primary: bool) -> egui::Response {
    let label = crate::tl!(label);
    let (fill, stroke, text) = if primary { (t.accent, t.accent, t.band_text) } else { (t.panel, t.faint, t.text) };
    let response = ui
        .add(
            egui::Button::new(egui::RichText::new(label).size(if primary { 13.5 } else { 14.0 }).color(text))
                .min_size(vec2(width, height))
                .fill(fill)
                .stroke(egui::Stroke::new(if primary { 1.0 } else { 0.8 }, stroke)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);
    if response.hovered() {
        ui.painter().rect_stroke(response.rect, 4.0, egui::Stroke::new(2.0, t.band_text), egui::StrokeKind::Inside);
    }
    if response.is_pointer_button_down_on() {
        ui.painter().rect_stroke(response.rect, 4.0, egui::Stroke::new(3.0, t.strong), egui::StrokeKind::Inside);
    }
    response
}

/// About: what this is, which version, and where the code lives.
fn about_page(ui: &mut Ui, telemetry_root: &std::path::Path, session_id: Option<&str>, t: &Tokens) {
    heading(ui, crate::tl_id!("ui.about_notecraft"), t);
    ui.horizontal_top(|ui| {
        ui.add(egui::Image::from_bytes("bytes://notecraft-logo", crate::LOGO_PNG).fit_to_exact_size(vec2(64.0, 64.0)));
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(crate::tl_id!("ui.notecraft")).size(20.0).color(t.accent));
            let version = crate::i18n::format_message_id("ui.version_version_alpha", &[("version", VERSION)]);
            ui.label(egui::RichText::new(version).size(12.0).color(t.muted));
            ui.label(
                egui::RichText::new(crate::tl_id!("ui.note_taking_and_organization_in_pure_rust_your_notes_49e8095e25")).size(12.5).color(t.text),
            );
        });
    });
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.the_code")).size(14.0).color(t.strong));
    ui.hyperlink_to(REPOSITORY, REPOSITORY);
    ui.add_space(4.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.issues_the_roadmap_and_the_documentation_are_all_the_2beb6e100f")).size(11.5).color(t.muted));
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.licence")).size(14.0).color(t.strong));
    ui.label(egui::RichText::new(crate::tl_id!("ui.mit_or_apache_2_0_at_your_option_the_koala_artwork_i_2ad8f523c0")).size(11.5).color(t.muted));
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.built_with")).size(14.0).color(t.strong));
    ui.hyperlink_to(crate::tl_id!("ui.egui_and_eframe"), "https://github.com/emilk/egui");
    ui.label(egui::RichText::new(crate::tl_id!("ui.the_window_drawn_on_the_gpu_the_icons_are_drawn_in_c_27f197a70f")).size(11.5).color(t.muted));
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.onenote_import")).size(14.0).color(t.strong));
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(crate::tl_id!("ui.powered_by")).size(11.5).color(t.muted));
        ui.hyperlink_to(crate::tl_id!("ui.officeimo"), "https://github.com/EvotecIT/OfficeIMO");
        ui.label(egui::RichText::new(crate::tl_id!("ui.under_the")).size(11.5).color(t.muted));
        ui.hyperlink_to(crate::tl_id!("ui.mit_license"), "https://github.com/EvotecIT/OfficeIMO/blob/master/LICENSE");
        ui.label(egui::RichText::new(crate::tl_id!("ui.many_thanks_to_its_authors_and_contributors")).size(11.5).color(t.muted));
    });
    ui.add_space(14.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.local_diagnostics")).strong().size(14.0).color(t.strong));
    ui.label(egui::RichText::new(crate::tl_id!("ui.logs_and_telemetry_stay_on_this_device_notecraft_doe_87b8387dee")).size(11.5).color(t.muted));
    if let Some(id) = session_id {
        let label = crate::i18n::format_message_id("ui.session_id_id", &[("id", id)]);
        ui.label(egui::RichText::new(label).strong().monospace().size(11.5).color(t.text));
    }
    let folder = telemetry_root.display().to_string();
    let label = crate::i18n::format_message_id("ui.logging_folder_folder", &[("folder", &folder)]);
    ui.label(egui::RichText::new(label).strong().size(11.5).color(t.text));
}

fn options_page(
    ui: &mut Ui,
    root: &std::path::Path,
    level: &str,
    session_id: Option<&str>,
    ui_language: &str,
    languages: &[crate::i18n::LanguageChoice],
    t: &Tokens,
    acts: &mut Vec<Act>,
) {
    heading(ui, crate::tl_id!("ui.options"), t);
    let automatic = crate::tl_id!("ui.automatic_system_language");
    let selected = if ui_language == crate::i18n::AUTO {
        automatic.to_string()
    } else {
        languages
            .iter()
            .find(|language| language.code == ui_language)
            .map_or_else(|| crate::tl_id!("ui.english").to_string(), |language| language.name.clone())
    };
    egui::ComboBox::from_label(crate::tl_id!("ui.interface_language"))
        .selected_text(selected)
        .popup_style(crate::theme::picker_popup_style(*t))
        .show_ui(ui, |ui| {
            if ui.selectable_label(ui_language == crate::i18n::AUTO, automatic).clicked() {
                acts.push(Act::SetUiLanguage(crate::i18n::AUTO.into()));
                ui.close();
            }
            for language in languages {
                let mut clicked = false;
                ui.horizontal(|ui| {
                    crate::language_flag::paint(ui, &language.code);
                    clicked = ui.selectable_label(ui_language == language.code, &language.name).clicked();
                });
                if clicked {
                    acts.push(Act::SetUiLanguage(language.code.clone()));
                    ui.close();
                }
            }
        });
    ui.add_space(12.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.privacy_is_our_top_priority_logs_and_telemetry_stay_17a34813d0")).strong().color(t.strong));
    ui.add_space(12.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.local_diagnostics_and_usage")).strong().size(15.0).color(t.strong));
    ui.label(egui::RichText::new(crate::tl_id!("ui.choose_what_notecraft_records_locally_nothing_is_upl_fc372d68e8")).size(12.0).color(t.muted));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(crate::tl_id!("ui.level")).strong());
        let selected_level = crate::tl!(level);
        egui::ComboBox::from_id_salt("telemetry-level").selected_text(selected_level).show_ui(ui, |ui| {
            for choice in ["none", "usage", "warning", "error", "debug"] {
                if ui.selectable_label(level == choice, crate::tl!(choice)).clicked() {
                    acts.push(Act::SetTelemetryLevel(choice.into()));
                    ui.close();
                }
            }
        });
    });
    if let Some(id) = session_id {
        let label = crate::i18n::format_message_id("ui.current_session_id", &[("id", id)]);
        ui.label(egui::RichText::new(label).strong().monospace().size(11.0).color(t.text));
    }
    let folder = root.display().to_string();
    let label = crate::i18n::format_message_id("ui.folder_folder", &[("folder", &folder)]);
    ui.label(egui::RichText::new(label).strong().size(11.0).color(t.text));
    ui.horizontal(|ui| {
        if open_action(ui, crate::tl_id!("ui.open_telemetry_folder"), 190.0, 34.0, t, true).clicked() {
            acts.push(Act::OpenTelemetryFolder(root.to_path_buf()));
        }
        if open_action(ui, crate::tl_id!("ui.export_current_session_zip"), 220.0, 34.0, t, true).clicked() {
            acts.push(Act::ExportTelemetry(root.to_path_buf()));
        }
        if open_action(ui, crate::tl_id!("ui.clear_telemetry"), 150.0, 34.0, t, true).clicked() {
            acts.push(Act::ClearTelemetry(root.to_path_buf()));
        }
    });
    ui.add_space(12.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.export_creates_a_user_selected_zip_then_removes_the_66f2bf3400")).size(11.5).color(t.muted));
    #[cfg(feature = "voice")]
    {
        ui.add_space(18.0);
        ui.separator();
        ui.label(egui::RichText::new(crate::tl_id!("voice.dictation_2a7fe421")).strong().size(15.0).color(t.strong));
        ui.label(egui::RichText::new(crate::tl_id!("voice.the_speech_recognizer_model_language_microphone_an_3fece2c0")).size(11.5).color(t.muted));
        if open_action(ui, crate::tl_id!("voice.dictation_setup_6be1e51d"), 190.0, 34.0, t, true).clicked() {
            acts.push(Act::DictationSetup);
        }
    }
    ui.add_space(18.0);
    ui.separator();
    ui.label(egui::RichText::new(crate::tl_id!("ui.application_preferences")).strong().size(15.0).color(t.strong));
    ui.label(egui::RichText::new(crate::tl_id!("ui.reset_themes_layout_ribbon_choices_and_other_saved_n_61cc3d1cfb")).size(11.5).color(t.muted));
    ui.colored_label(t.warn, crate::tl_id!("ui.this_does_not_delete_notebooks_or_pages_it_cannot_be_undone"));
    if warning_action(ui, crate::tl_id!("ui.reset_app_preferences_2"), 210.0, 34.0, t).clicked() {
        acts.push(Act::ResetPreferences);
    }
}

fn warning_action(ui: &mut Ui, label: &str, width: f32, height: f32, t: &Tokens) -> egui::Response {
    let response = ui
        .add(
            egui::Button::new(egui::RichText::new(label).size(14.0).color(t.warn))
                .min_size(vec2(width, height))
                .fill(t.panel)
                .stroke(egui::Stroke::new(1.0, t.warn)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);
    if response.hovered() {
        ui.painter().rect_stroke(response.rect, 4.0, egui::Stroke::new(2.0, t.warn), egui::StrokeKind::Inside);
    }
    if response.is_pointer_button_down_on() {
        ui.painter().rect_stroke(response.rect, 4.0, egui::Stroke::new(3.0, t.strong), egui::StrokeKind::Inside);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_entries_say_what_works() {
        let built: Vec<&str> = SIDE.iter().filter(|(_, b)| *b).map(|(n, _)| *n).collect();
        assert_eq!(built, vec!["Info", "New", "Open", "Options", "About"]);
        assert_eq!(SIDE.len(), 10, "the rest are listed and marked, not hidden");
        assert!(SIDE.iter().any(|(n, b)| *n == "Print" && !*b));
    }

    #[test]
    fn about_says_where_the_code_lives_and_asks_for_nothing() {
        let t = crate::theme::Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            acts = show(
                ui,
                &[],
                &[],
                "C:/notebooks",
                "About",
                &|p| p.to_string_lossy().into_owned(),
                std::path::Path::new("C:/telemetry"),
                "none",
                None,
                crate::i18n::AUTO,
                &crate::i18n::available_languages(),
                &t,
            );
        });
        out.textures_delta.clear();
        assert_eq!(acts, Vec::new(), "the About page asked for something nobody clicked");
        assert!(REPOSITORY.starts_with("https://github.com/"), "{REPOSITORY}");
        assert!(!VERSION.is_empty());
    }
}

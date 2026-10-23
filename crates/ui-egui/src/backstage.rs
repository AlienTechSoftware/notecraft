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
    /// Write the Welcome notebook into the notebooks folder, or open it if it is there.
    AddWelcome,
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
    ("Options", false),
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
                    "About" => about_page(ui, t),
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
        back.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Back"));
        if back.on_hover_text("Back to the notebook").clicked() {
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
            painter.text(egui::pos2(rect.min.x + 12.0, rect.center().y), Align2::LEFT_CENTER, name, egui::FontId::proportional(13.5), color);
            if !built {
                painter.text(
                    egui::pos2(rect.max.x - 8.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    "not built",
                    egui::FontId::proportional(10.0),
                    t.muted,
                );
            }
            let enabled = ui.is_enabled();
            let label = name.to_string();
            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label.clone()));
            if resp.clicked() {
                acts.push(if built { Act::Page(name) } else { Act::NotBuilt(name) });
            }
        }
    });
}

fn heading(ui: &mut Ui, text: &str, t: &Tokens) {
    ui.label(egui::RichText::new(text).size(22.0).color(t.strong));
    ui.add_space(8.0);
}

fn info_page(ui: &mut Ui, notebooks: &[Node], folder: &str, show_path: &dyn Fn(&std::path::Path) -> String, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, "Notebook information", t);
    if notebooks.is_empty() {
        ui.label(egui::RichText::new("There are no notebooks yet. \"New\" starts one.").color(t.muted));
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
                resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, format!("Open {name}")));
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Open this notebook").clicked() {
                    acts.push(Act::Open(n.path.clone()));
                }
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(&n.info.title).size(17.0).color(t.accent));
                    ui.label(egui::RichText::new(show_path(&n.path)).size(11.5).monospace().color(t.muted));
                    ui.horizontal(|ui| {
                        if ui.small_button("Open folder").clicked() {
                            acts.push(Act::OpenFolder(n.path.clone()));
                        }
                        ui.label(
                            egui::RichText::new(format!("{} items, {} colour", n.count(), n.info.color.map(NodeColor::label).unwrap_or("no")))
                                .size(11.0)
                                .color(t.muted),
                        );
                    });
                });
            });
            ui.add_space(10.0);
        }
    });
    ui.add_space(6.0);
    ui.separator();
    ui.label(egui::RichText::new(format!("All of them live in {folder}")).size(11.5).color(t.muted));
    ui.label(
        egui::RichText::new("Online notebooks (OneDrive, SharePoint) are not built. A notebook in a folder your sync tool watches is synced by it.")
            .size(11.5)
            .color(t.muted),
    );
}

fn new_page(ui: &mut Ui, folder: &str, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, "New notebook", t);
    ui.label(egui::RichText::new(format!("A new notebook is a folder in {folder}, with one section and one page inside it.")).color(t.text));
    ui.add_space(10.0);
    if ui.button("New notebook").clicked() {
        acts.push(Act::NewNotebook);
    }
    ui.add_space(14.0);
    ui.label(egui::RichText::new("The welcome notebook").size(14.0).color(t.strong));
    ui.label(
        egui::RichText::new("A tour of NoteCraft, shown on its own pages. It is added to an empty notebooks folder on the first start; this adds it again, or opens it if it is still here.")
            .size(11.5)
            .color(t.muted),
    );
    ui.add_space(6.0);
    if ui.button("Add the welcome notebook").clicked() {
        acts.push(Act::AddWelcome);
    }
    ui.add_space(14.0);
    ui.label(egui::RichText::new("Somewhere else").size(14.0).color(t.strong));
    ui.label(
        egui::RichText::new(
            "OneDrive and SharePoint: not built. Point NoteCraft at a folder your sync tool already watches and it is the same thing.",
        )
        .size(11.5)
        .color(t.muted),
    );
}

fn open_page(ui: &mut Ui, recent: &[Node], folder: &str, show_path: &dyn Fn(&std::path::Path) -> String, t: &Tokens, acts: &mut Vec<Act>) {
    heading(ui, "Open a notebook", t);
    ui.horizontal(|ui| {
        if ui.add_sized([230.0, 42.0], egui::Button::new(egui::RichText::new("Browse for a notebook…").size(15.0))).clicked() {
            acts.push(Act::ChooseFolder);
        }
        if open_action(ui, "Show notebooks folder", 210.0, 42.0, t, false).clicked() {
            acts.push(Act::OpenNotebooksFolder);
        }
    });
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        if open_action(ui, "Import notebook folder", 230.0, 36.0, t, true).clicked() {
            acts.push(Act::ImportNotebookFolder);
        }
        if open_action(ui, "Import file", 230.0, 36.0, t, true).clicked() {
            acts.push(Act::ImportFile);
        }
        let cloud = ui.add_enabled(false, egui::Button::new(egui::RichText::new("Import from the cloud").size(13.0)).min_size(vec2(190.0, 36.0)));
        cloud.on_hover_text("Cloud import is not implemented yet.");
    });
    ui.add_space(22.0);
    ui.label(egui::RichText::new("Recently opened").size(16.0).color(t.strong));
    ui.add_space(8.0);
    if recent.is_empty() {
        ui.label(egui::RichText::new("Notebooks you open will appear here.").size(13.0).color(t.muted));
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
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Open {name}")));
            let path = notebook.path.clone();
            response.context_menu(|ui| {
                if ui.button("Remove from list").clicked() {
                    acts.push(Act::RemoveRecent(path.clone()));
                    ui.close();
                }
                if ui.button("Delete notebook...").clicked() {
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
    ui.label(egui::RichText::new(format!("Browse notebooks in {folder} or choose another folder.")).size(11.5).color(t.muted));
}

fn open_action(ui: &mut Ui, label: &str, width: f32, height: f32, t: &Tokens, primary: bool) -> egui::Response {
    let (fill, stroke, text) = if primary { (t.accent, t.accent, t.band_text) } else { (t.panel, t.faint, t.text) };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(if primary { 13.5 } else { 14.0 }).color(text))
            .min_size(vec2(width, height))
            .fill(fill)
            .stroke(egui::Stroke::new(if primary { 1.0 } else { 0.8 }, stroke)),
    )
}

/// About: what this is, which version, and where the code lives.
fn about_page(ui: &mut Ui, t: &Tokens) {
    heading(ui, "About NoteCraft", t);
    ui.horizontal_top(|ui| {
        ui.add(egui::Image::from_bytes("bytes://notecraft-logo", crate::LOGO_PNG).fit_to_exact_size(vec2(64.0, 64.0)));
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("NoteCraft").size(20.0).color(t.accent));
            ui.label(egui::RichText::new(format!("Version {VERSION} (alpha)")).size(12.0).color(t.muted));
            ui.label(
                egui::RichText::new("Note taking and organization in pure Rust. Your notes are plain folders and files.").size(12.5).color(t.text),
            );
        });
    });
    ui.add_space(14.0);
    ui.label(egui::RichText::new("The code").size(14.0).color(t.strong));
    ui.hyperlink_to(REPOSITORY, REPOSITORY);
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Issues, the roadmap and the documentation are all there. Nothing in NoteCraft talks to it, or to any other service, by itself.",
        )
        .size(11.5)
        .color(t.muted),
    );
    ui.add_space(14.0);
    ui.label(egui::RichText::new("Licence").size(14.0).color(t.strong));
    ui.label(
        egui::RichText::new("MIT or Apache-2.0, at your option. The koala artwork is not covered by it; see ATTRIBUTION.md.")
            .size(11.5)
            .color(t.muted),
    );
    ui.add_space(14.0);
    ui.label(egui::RichText::new("Built with").size(14.0).color(t.strong));
    ui.hyperlink_to("egui and eframe", "https://github.com/emilk/egui");
    ui.label(
        egui::RichText::new("The window, drawn on the GPU. The icons are drawn in code. The logo is the only picture.").size(11.5).color(t.muted),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_side_entries_say_what_works() {
        let built: Vec<&str> = SIDE.iter().filter(|(_, b)| *b).map(|(n, _)| *n).collect();
        assert_eq!(built, vec!["Info", "New", "Open", "About"]);
        assert_eq!(SIDE.len(), 10, "the rest are listed and marked, not hidden");
        assert!(SIDE.iter().any(|(n, b)| *n == "Print" && !*b));
    }

    #[test]
    fn about_says_where_the_code_lives_and_asks_for_nothing() {
        let t = crate::theme::Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            acts = show(ui, &[], &[], "C:/notebooks", "About", &|p| p.to_string_lossy().into_owned(), &t);
        });
        out.textures_delta.clear();
        assert_eq!(acts, Vec::new(), "the About page asked for something nobody clicked");
        assert!(REPOSITORY.starts_with("https://github.com/"), "{REPOSITORY}");
        assert!(!VERSION.is_empty());
    }
}

//! The code view: every file of a page on the left, the one that is picked on the right.
//!
//! A page is a folder, and the window should be able to show that folder as it is. The
//! old Source button put the page's text beside the page itself, which showed one file of
//! the several a page owns and left two editable copies of the same page on screen at
//! once. This is the view [`docs/page-model.md`](../../../docs/page-model.md) section 4
//! asks for: a list, a viewer, and one editable copy.
//!
//! What can be edited: the page file, whole, head and all, and the SVG of a drawing.
//! The head was read-only at first, so that a hand edit could not break the page's id;
//! the owner asked for the whole file on 2026-10-19, which is the point of a code view -
//! a person debugging a page needs to change the line that is wrong, wherever it is.
//! Everything else is read-only, because a hand edit to a picture or to a `_node.toml`
//! is not something to do with a text box.

use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};
use std::path::Path;

/// How much of a file is read for the viewer. A larger file shows its first part and says
/// so, which keeps the view quick whatever is in the folder.
pub const MAX_SHOWN: usize = 256 * 1024;

/// What kind of file a row is, which decides how it is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `index.html` or `index.md`: the page's own text.
    PageFile,
    /// A drawing of ours: SVG carrying the ink namespace.
    Ink,
    /// A picture the window can draw.
    Picture,
    /// Anything else we can show as text.
    Text,
    /// A file shown by name, size and type only.
    Other,
}

impl Kind {
    /// Whether what is shown can be typed in.
    pub fn editable(self) -> bool {
        matches!(self, Kind::PageFile | Kind::Ink)
    }
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// What the row is called, and what the page would link to: `index.html`, `ink.svg`,
    /// `assets/koala.png`.
    pub name: String,
    pub kind: Kind,
    pub bytes: u64,
    /// False when nothing in the page names this file: an orphan, kept and never drawn.
    pub used: bool,
}

/// The state the view keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// The row that is picked. The page file when the view opens.
    pub picked: Option<String>,
    /// The text of the picked file while it is being read or edited. The page file is not
    /// held here: its body is the editor's own, so there is only ever one copy of it.
    pub text: String,
    /// Which file `text` was read from, so a change of row reads the new one.
    pub loaded: Option<String>,
    /// The picked file has been typed in and not yet written.
    pub dirty: bool,
    /// The page's own file was typed in, so the page has to be read again when the view
    /// closes.
    pub page_edited: bool,
}

/// What the view did this frame.
#[derive(Clone, Debug, Default)]
pub struct Out {
    /// A file was typed in: its name and its new text, for the window to write.
    pub wrote: Option<(String, String)>,
}

/// The files of a page, the page file first, then what sits beside it, then `assets/`.
///
/// `named` is what the page's text links to, so a file nothing names can be marked as an
/// orphan rather than quietly listed as if it were in use.
pub fn files(dir: &Path, page_file: &str, named: &[String]) -> Vec<Entry> {
    let mut beside: Vec<Entry> = Vec::new();
    let mut assets: Vec<Entry> = Vec::new();
    let mut page = None;
    for (folder, prefix) in [(dir.to_path_buf(), ""), (dir.join("assets"), "assets/")] {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            if !entry.path().is_file() {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = format!("{prefix}{file}");
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let kind = kind_of(&entry.path(), &name, page_file);
            let used = kind == Kind::PageFile || named.iter().any(|n| n.trim() == name);
            let row = Entry { name, kind, bytes, used };
            match (kind, prefix) {
                (Kind::PageFile, _) => page = Some(row),
                (_, "assets/") => assets.push(row),
                _ => beside.push(row),
            }
        }
    }
    beside.sort_by(|a, b| a.name.cmp(&b.name));
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::with_capacity(beside.len() + assets.len() + 1);
    out.extend(page);
    out.append(&mut beside);
    out.append(&mut assets);
    out
}

fn kind_of(path: &Path, name: &str, page_file: &str) -> Kind {
    if name == page_file {
        return Kind::PageFile;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "svg" => {
            if head_of(path, 4096).is_some_and(|head| notecraft_ink::svg::is_ink(&head)) {
                Kind::Ink
            } else {
                Kind::Text
            }
        }
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => Kind::Picture,
        "md" | "html" | "htm" | "txt" | "json" | "toml" | "csv" | "xml" | "yaml" | "yml" => Kind::Text,
        _ => Kind::Other,
    }
}

/// The first `want` bytes of a file as text, for looking at a head without reading a
/// drawing that may be megabytes.
fn head_of(path: &Path, want: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = vec![0u8; want];
    let read = file.read(&mut buffer).ok()?;
    buffer.truncate(read);
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Reads a file for the viewer, cut to [`MAX_SHOWN`]. Returns the text and whether it was
/// cut, so the view can say so rather than pretending that is the whole file.
pub fn read_for_view(path: &Path) -> (String, bool) {
    match head_of(path, MAX_SHOWN + 1) {
        Some(text) if text.len() > MAX_SHOWN => (text.chars().take(MAX_SHOWN).collect(), true),
        Some(text) => (text, false),
        None => (String::new(), false),
    }
}

/// How large a file is, in words a person reads rather than a number of bytes.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

/// Draws the view.
pub fn show(ui: &mut Ui, state: &mut State, entries: &[Entry], dir: &Path, t: &Tokens, height: f32) -> Out {
    let mut out = Out::default();
    let page_file = entries.first().filter(|e| e.kind == Kind::PageFile).map(|e| e.name.clone());
    if state.picked.is_none() || !entries.iter().any(|e| Some(&e.name) == state.picked.as_ref()) {
        state.picked = page_file.clone().or_else(|| entries.first().map(|e| e.name.clone()));
        state.loaded = None;
    }

    ui.horizontal_top(|ui| {
        // ---- the list ----
        let list_width = 240.0_f32.min(ui.available_width() * 0.4);
        ui.allocate_ui_with_layout(vec2(list_width, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_min_width(list_width);
            ui.set_max_width(list_width);
            ui.label(egui::RichText::new("FILES IN THIS PAGE").small().color(t.muted));
            ui.add_space(2.0);
            egui::ScrollArea::vertical().id_salt("code-files").max_height(height - 24.0).show(ui, |ui| {
                for entry in entries {
                    let picked = state.picked.as_deref() == Some(entry.name.as_str());
                    let row = row(ui, entry, picked, t);
                    if row.clicked() {
                        state.picked = Some(entry.name.clone());
                        state.loaded = None;
                    }
                }
            });
        });
        ui.separator();

        // ---- the file that is picked ----
        let Some(name) = state.picked.clone() else { return };
        let Some(entry) = entries.iter().find(|e| e.name == name) else { return };
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&entry.name).monospace().strong());
                ui.label(egui::RichText::new(size_text(entry.bytes)).small().color(t.muted));
                if !entry.used {
                    ui.label(egui::RichText::new("not used by the page").small().color(t.warn));
                }
                if !entry.kind.editable() {
                    ui.label(egui::RichText::new("read-only").small().color(t.muted));
                }
            });
            ui.separator();
            match entry.kind {
                Kind::Picture => {
                    let uri = crate::preview::image_uri(dir, &entry.name);
                    match uri {
                        Some(uri) => {
                            egui::ScrollArea::both().id_salt("code-picture").max_height(height - 40.0).show(ui, |ui| {
                                ui.add(egui::Image::new(uri).max_width(ui.available_width()).fit_to_original_size(1.0));
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("This picture cannot be shown here.").color(t.muted));
                        }
                    }
                }
                Kind::Other => {
                    ui.label(egui::RichText::new("Nothing here can show this kind of file.").color(t.muted));
                    ui.label(egui::RichText::new("It is listed so the page's folder is not a secret.").small().color(t.muted));
                }
                Kind::PageFile | Kind::Ink | Kind::Text => {
                    if state.loaded.as_deref() != Some(entry.name.as_str()) {
                        let (text, cut) = read_for_view(&dir.join(entry.name.replace('/', std::path::MAIN_SEPARATOR_STR)));
                        state.text = text;
                        state.loaded = Some(entry.name.clone());
                        state.dirty = false;
                        if cut {
                            state.text.push_str("\n\n... the rest of this file is not shown here.");
                        }
                    }
                    let warning = match entry.kind {
                        Kind::PageFile => Some("The whole file, head and all. The page is read again when the view closes."),
                        Kind::Ink => Some("The pen's own readings are in this file. An edit that breaks them cannot be undone by the pen."),
                        _ => None,
                    };
                    if let Some(words) = warning {
                        ui.label(egui::RichText::new(words).small().color(t.muted));
                    }
                    egui::ScrollArea::vertical().id_salt("code-text").max_height(height - 70.0).show(ui, |ui| {
                        let edit = ui.add_enabled(
                            entry.kind.editable(),
                            egui::TextEdit::multiline(&mut state.text).code_editor().desired_width(f32::INFINITY).desired_rows(26),
                        );
                        if edit.changed() {
                            state.dirty = true;
                            out.wrote = Some((entry.name.clone(), state.text.clone()));
                        }
                    });
                }
            }
        });
    });
    out
}

/// One row of the list: the name, and what the file is.
fn row(ui: &mut Ui, entry: &Entry, picked: bool, t: &Tokens) -> egui::Response {
    let height = 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if picked || resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, if picked { t.row_selected } else { t.row_hover });
    }
    let colour = if entry.used { t.text } else { t.muted };
    let at = egui::pos2(rect.min.x + 6.0, rect.center().y);
    ui.painter().text(at, egui::Align2::LEFT_CENTER, &entry.name, egui::FontId::monospace(12.0), colour);
    let right = egui::pos2(rect.max.x - 6.0, rect.center().y);
    ui.painter().text(right, egui::Align2::RIGHT_CENTER, size_text(entry.bytes), egui::FontId::proportional(10.0), t.muted);
    let enabled = ui.is_enabled();
    let label = format!("{} ({})", entry.name, size_text(entry.bytes));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, picked, label.as_str()));
    resp.on_hover_text(if entry.used { entry.name.clone() } else { format!("{}: not used by the page", entry.name) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_words_not_bytes() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(999), "999 B");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn the_page_file_comes_first_and_an_unnamed_file_is_an_orphan() {
        let t = notecraft_store::testing::TempDir::new("codeview-files");
        let dir = t.path().join("page");
        std::fs::create_dir_all(dir.join("assets")).expect("folders");
        std::fs::write(dir.join("index.html"), "<html></html>").expect("page file");
        std::fs::write(dir.join("_node.toml"), "title = \"P\"").expect("node");
        std::fs::write(dir.join("ink.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"></svg>")
            .expect("ink");
        std::fs::write(dir.join("assets").join("koala.png"), [0u8; 10]).expect("picture");

        let named = vec!["ink.svg".to_string()];
        let rows = files(&dir, "index.html", &named);
        let names: Vec<&str> = rows.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["index.html", "_node.toml", "ink.svg", "assets/koala.png"], "the page file first, then beside it, then assets");

        let by = |name: &str| rows.iter().find(|e| e.name == name).cloned().expect("row");
        assert_eq!(by("index.html").kind, Kind::PageFile);
        assert_eq!(by("ink.svg").kind, Kind::Ink, "ours by its namespace");
        assert_eq!(by("assets/koala.png").kind, Kind::Picture);
        assert_eq!(by("_node.toml").kind, Kind::Text);
        assert!(by("ink.svg").used, "the page names the drawing");
        assert!(!by("assets/koala.png").used, "nothing names the picture: an orphan");
        assert!(by("index.html").used, "the page file is never an orphan");

        assert!(Kind::PageFile.editable() && Kind::Ink.editable(), "ours can be typed in");
        assert!(!Kind::Picture.editable() && !Kind::Text.editable() && !Kind::Other.editable());
    }
}

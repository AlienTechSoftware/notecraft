//! The lab window: the test folder on the left, the picture with its labels and the engine's
//! boxes in the middle, and the run, the score and the editable labels on the right.

use crate::case::{self, Label, LabelFile, Score};
use crate::settings;
use crate::telemetry::{self, Log, Timing};
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use notecraft_ocr::{Line, OcrsReader};
use notecraft_ui_egui::theme::Theme;
use std::path::{Path, PathBuf};

const HANDLE_PX: f32 = 8.0;
const LABEL_COLOUR: Color32 = Color32::from_rgb(40, 160, 255);
const SELECTED_COLOUR: Color32 = Color32::from_rgb(255, 200, 0);
const ENGINE_COLOUR: Color32 = Color32::from_rgb(255, 90, 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Select,
    Draw,
}

/// A gesture in progress, in picture pixels.
#[derive(Debug, Clone, Copy)]
enum Drag {
    New { start: [f32; 2], now: [f32; 2] },
    Move { index: usize, last: [f32; 2] },
    Corner { index: usize, corner: usize },
}

/// The picture on screen and what is known about it.
struct Loaded {
    index: usize,
    texture: egui::TextureHandle,
    size: [u32; 2],
    bytes: u64,
    file: LabelFile,
    lines: Vec<Line>,
    timings: Vec<Timing>,
    score: Option<Score>,
}

pub struct Lab {
    folder: PathBuf,
    pictures: Vec<PathBuf>,
    loaded: Option<Loaded>,
    reader: Option<OcrsReader>,
    reader_error: Option<String>,
    model_load_ms: Option<f64>,
    models: Option<PathBuf>,
    theme: Theme,
    /// The theme last applied to the window, so the visuals are set only when it changes.
    applied_theme: Option<Theme>,
    mode: Mode,
    selected: Option<usize>,
    drag: Option<Drag>,
    show_engine: bool,
    log: Log,
    status: String,
}

impl Lab {
    pub fn new(folder: PathBuf, models: Option<PathBuf>, saved_theme: Option<String>, mut log: Log) -> Lab {
        let pictures = case::discover(&folder);
        log.event(&format!("lab opened: {} pictures in {}", pictures.len(), folder_name(&folder)));
        let mut lab = Lab {
            folder,
            pictures,
            loaded: None,
            reader: None,
            reader_error: Some(
                "No models loaded. Click Models… and pick the folder that holds text-detection.onnx and text-recognition.onnx.".to_string(),
            ),
            model_load_ms: None,
            models: None,
            theme: Theme { mode: notecraft_ui_egui::theme::Mode::Dark, accent: notecraft_ui_egui::theme::Accent::Default },
            applied_theme: None,
            mode: Mode::Select,
            selected: None,
            drag: None,
            show_engine: true,
            log,
            status: String::new(),
        };
        if let Some(dir) = models {
            lab.load_reader(&dir);
        }
        if let Some(id) = saved_theme {
            lab.theme = Theme::all().into_iter().find(|t| t.id() == id).unwrap_or(Theme::DEFAULT);
        }
        lab.status = if lab.pictures.is_empty() { "No pictures in this folder.".to_string() } else { "Pick a picture to start.".to_string() };
        lab
    }

    /// Loads the two ONNX models from a folder. Failure leaves the lab usable for browsing.
    pub fn load_reader(&mut self, dir: &Path) {
        let (det, rec) = (dir.join("text-detection.onnx"), dir.join("text-recognition.onnx"));
        let mut timings = Vec::new();
        let loaded = telemetry::timed("model load", &mut timings, || OcrsReader::from_model_files(&det, &rec));
        self.model_load_ms = timings.last().map(|t| t.ms);
        match loaded {
            Ok(reader) => {
                self.log.event(&format!("models loaded in {:.0} ms", self.model_load_ms.unwrap_or_default()));
                self.reader = Some(reader);
                self.reader_error = None;
                self.models = Some(dir.to_path_buf());
                self.status = "Models loaded.".to_string();
            }
            Err(e) => {
                self.log.event(&format!("models did not load: {e:?}"));
                self.reader = None;
                self.reader_error = Some(format!("The models did not load: {e:?}"));
                self.status = self.reader_error.clone().unwrap_or_default();
            }
        }
    }

    /// Points the lab at another test folder and forgets the picture that was open.
    pub fn set_folder(&mut self, folder: PathBuf) {
        self.pictures = case::discover(&folder);
        self.folder = folder;
        self.loaded = None;
        self.selected = None;
        self.drag = None;
        self.log.event(&format!("folder set: {} pictures in {}", self.pictures.len(), folder_name(&self.folder)));
        self.status = if self.pictures.is_empty() { "No pictures in this folder.".to_string() } else { format!("{} pictures.", self.pictures.len()) };
    }

    /// Writes the folder and model choice to the local settings, so the next start has them.
    fn remember(&self) {
        settings::save(&settings::Settings { folder: Some(self.folder.clone()), models: self.models.clone(), theme: Some(self.theme.id()) });
    }

    fn select(&mut self, ctx: &egui::Context, index: usize) {
        let path = self.pictures[index].clone();
        let name = self.relative_name(&path);
        let decoded = image::open(&path).map(|img| img.to_rgba8());
        let rgba = match decoded {
            Ok(rgba) => rgba,
            Err(e) => {
                self.status = format!("{name}: {e}");
                self.log.event(&format!("could not open {name}: {e}"));
                return;
            }
        };
        let size = [rgba.width(), rgba.height()];
        let texture = ctx.load_texture(
            name.clone(),
            egui::ColorImage::from_rgba_unmultiplied([size[0] as usize, size[1] as usize], rgba.as_raw()),
            egui::TextureOptions::LINEAR,
        );
        match case::load_labels(&path) {
            Ok(file) => {
                self.log.event(&format!("opened {name} {}x{} with {} labels", size[0], size[1], file.labels.len()));
                let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                self.loaded = Some(Loaded { index, texture, size, bytes, file, lines: Vec::new(), timings: Vec::new(), score: None });
                self.selected = None;
                self.drag = None;
                self.status = format!("{name}: {} labels", self.loaded.as_ref().map_or(0, |l| l.file.labels.len()));
            }
            Err(e) => {
                self.status = e.clone();
                self.log.event(&format!("labels did not load for {name}: {e}"));
            }
        }
    }

    fn relative_name(&self, path: &Path) -> String {
        path.strip_prefix(&self.folder).unwrap_or(path).to_string_lossy().replace('\\', "/")
    }

    fn run_ocr(&mut self) {
        let Some(reader) = self.reader.as_ref() else {
            self.status = self.reader_error.clone().unwrap_or_else(|| "The models are not loaded.".to_string());
            return;
        };
        let Some(loaded) = self.loaded.as_mut() else { return };
        let path = self.pictures[loaded.index].clone();
        let name = path.strip_prefix(&self.folder).unwrap_or(&path).to_string_lossy().replace('\\', "/");
        let mut timings = Vec::new();
        let result = telemetry::timed("read picture", &mut timings, || reader.read_image(&path));
        match result {
            Ok(lines) => {
                let score = telemetry::timed("score", &mut timings, || case::score(&loaded.file.labels, &lines));
                let read_ms = timings.first().map_or(0.0, |t| t.ms);
                self.log.event(&format!(
                    "ran OCR on {name}: {} engine lines, {} of {} labels found, {} text matches, read in {read_ms:.0} ms",
                    lines.len(),
                    score.found,
                    score.labels,
                    score.text_matched,
                ));
                self.status = format!("{} lines; {} of {} labels found", lines.len(), score.found, score.labels);
                loaded.lines = lines;
                loaded.score = Some(score);
                loaded.timings = timings;
            }
            Err(e) => {
                self.status = format!("OCR failed on {name}: {e:?}");
                self.log.event(&format!("OCR failed on {name}: {e:?}"));
            }
        }
    }

    /// Writes the labels beside the picture. Called after every change, so nothing waits for a
    /// save button.
    fn save(&mut self) {
        let Some(loaded) = self.loaded.as_ref() else { return };
        let path = self.pictures[loaded.index].clone();
        if let Err(e) = case::save_labels(&path, &loaded.file) {
            self.status = format!("Could not save labels: {e}");
            self.log.event(&format!("labels not saved: {e}"));
        }
        if let Some(loaded) = self.loaded.as_mut() {
            // Re-score against the lines already read, so the counts follow the labels.
            if loaded.lines.is_empty() {
                loaded.score = None;
            } else {
                loaded.score = Some(case::score(&loaded.file.labels, &loaded.lines));
            }
        }
    }

    /// The copy report: what the app found and did, in one JSON document, with no absolute paths
    /// or personal data. It is also written under `local/reports`.
    fn report(&mut self) -> Option<String> {
        let loaded = self.loaded.as_ref()?;
        let name = self.relative_name(&self.pictures[loaded.index]);
        let lines: Vec<serde_json::Value> = loaded
            .lines
            .iter()
            .map(|l| {
                serde_json::json!({
                    "text": l.text,
                    "bounds": case::bounds(&l.quad),
                    "quad": l.quad.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
                    "confidence": l.confidence,
                })
            })
            .collect();
        let value = serde_json::json!({
            "tool": "notecraft-ocr-lab",
            "tool_version": env!("CARGO_PKG_VERSION"),
            "engine": "ocrs 0.14 through notecraft-ocr",
            "platform": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH },
            "picture": name,
            "size": loaded.size,
            "bytes": loaded.bytes,
            "model_load_ms": self.model_load_ms,
            "timings": loaded.timings,
            "score": loaded.score,
            "labels": loaded.file.labels,
            "engine_lines": lines,
            "recent_events": self.log.recent(),
        });
        let text = serde_json::to_string_pretty(&value).ok()?;
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let stem = name.replace('/', "_");
        let dir = crate::lab_root().join("local").join("reports");
        let path = dir.join(format!("{stem}-{secs}.report.json"));
        let written = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, format!("{text}\n")));
        match written {
            Ok(()) => self.log.event(&format!("report written for {name}")),
            Err(e) => self.log.event(&format!("report not written: {e}")),
        }
        Some(text)
    }

    /// Picture pixels to screen, for the picture drawn at `origin` with `scale` screen pixels per pixel.
    fn to_screen(origin: Pos2, scale: f32, p: [f32; 2]) -> Pos2 {
        origin + Vec2::new(p[0], p[1]) * scale
    }

    fn to_pixels(origin: Pos2, scale: f32, p: Pos2) -> [f32; 2] {
        let v = (p - origin) / scale;
        [v.x, v.y]
    }

    fn corners(rect: [f32; 4]) -> [[f32; 2]; 4] {
        [[rect[0], rect[1]], [rect[2], rect[1]], [rect[2], rect[3]], [rect[0], rect[3]]]
    }

    fn clamp_point(p: [f32; 2], size: [u32; 2]) -> [f32; 2] {
        [p[0].clamp(0.0, size[0] as f32), p[1].clamp(0.0, size[1] as f32)]
    }

    fn normalized(rect: [f32; 4]) -> [f32; 4] {
        [rect[0].min(rect[2]), rect[1].min(rect[3]), rect[0].max(rect[2]), rect[1].max(rect[3])]
    }

    /// The topmost label whose rectangle holds the point.
    fn label_at(labels: &[Label], p: [f32; 2]) -> Option<usize> {
        labels
            .iter()
            .enumerate()
            .rev()
            .find(|(_, l)| {
                let r = Self::normalized(l.rect);
                p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]
            })
            .map(|(i, _)| i)
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.applied_theme != Some(self.theme) {
            ctx.set_visuals(self.theme.visuals());
            self.applied_theme = Some(self.theme);
        }
        egui::Panel::left("cases").resizable(true).default_size(260.0).show(ui, |ui| self.case_list(&ctx, ui));
        egui::Panel::right("details").resizable(true).default_size(340.0).show(ui, |ui| self.details(ui));
        egui::Panel::bottom("status").show(ui, |ui| ui.label(&self.status));
        egui::CentralPanel::default().show(ui, |ui| self.picture_area(ui));
        if self.selected.is_some() && !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.delete_selected();
        }
    }

    fn case_list(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Test cases");
        let before = self.theme;
        egui::ComboBox::from_id_salt("theme").selected_text(self.theme.id()).show_ui(ui, |ui| {
            for theme in Theme::all() {
                ui.selectable_value(&mut self.theme, theme, theme.id());
            }
        });
        if self.theme != before {
            self.remember();
        }
        ui.label(format!("{} pictures", self.pictures.len()));
        ui.monospace(folder_name(&self.folder));
        if ui.button("Open folder…").clicked()
            && let Some(dir) = rfd::FileDialog::new().set_directory(&self.folder).pick_folder()
        {
            self.set_folder(dir);
            self.remember();
        }
        ui.separator();
        let mut pick = None;
        // Kind (handwriting, drawings, ...) first, then language, then the pictures.
        let mut tree: std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<usize>>> = std::collections::BTreeMap::new();
        for (i, path) in self.pictures.iter().enumerate() {
            let (kind, language) = case::group_of(&self.folder, path);
            tree.entry(kind).or_default().entry(language).or_default().push(i);
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (kind, languages) in &tree {
                let count: usize = languages.values().map(Vec::len).sum();
                egui::CollapsingHeader::new(format!("{kind} ({count})")).id_salt(kind).default_open(true).show(ui, |ui| {
                    for (language, indexes) in languages {
                        egui::CollapsingHeader::new(format!("{language} ({})", indexes.len())).id_salt((kind, language)).default_open(true).show(
                            ui,
                            |ui| {
                                for &i in indexes {
                                    let path = &self.pictures[i];
                                    let current = self.loaded.as_ref().is_some_and(|l| l.index == i);
                                    let labelled = case::label_path(path).is_file();
                                    let mark = if labelled { "  ✎" } else { "" };
                                    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                    if ui.selectable_label(current, format!("{name}{mark}")).clicked() {
                                        pick = Some(i);
                                    }
                                }
                            },
                        );
                    }
                });
            }
        });
        if let Some(i) = pick {
            self.select(ctx, i);
        }
    }

    fn delete_selected(&mut self) {
        let Some(index) = self.selected.take() else { return };
        if let Some(loaded) = self.loaded.as_mut()
            && index < loaded.file.labels.len()
        {
            loaded.file.labels.remove(index);
            self.save();
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        ui.heading("Run and labels");
        ui.horizontal(|ui| {
            if ui.button("Models…").clicked()
                && let Some(dir) = rfd::FileDialog::new().pick_folder()
            {
                if settings::models_present(&dir) {
                    self.load_reader(&dir);
                    self.remember();
                } else {
                    self.status = "That folder has no text-detection.onnx and text-recognition.onnx.".to_string();
                }
            }
            match &self.models {
                Some(dir) => ui.label(format!("Models: {}", folder_name(dir))),
                None => ui.label("Models: not set"),
            };
        });
        if let Some(e) = self.reader_error.clone() {
            ui.colored_label(Color32::from_rgb(200, 60, 40), e);
        }
        ui.horizontal(|ui| {
            let can_run = self.reader.is_some() && self.loaded.is_some();
            if ui.add_enabled(can_run, egui::Button::new("Run OCR")).clicked() {
                self.run_ocr();
            }
            let can_copy = self.loaded.is_some();
            if ui.add_enabled(can_copy, egui::Button::new("Copy report")).clicked()
                && let Some(text) = self.report()
            {
                ui.ctx().copy_text(text);
                self.status = "Report copied, and written under local/reports.".to_string();
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Select, "Select / move / resize");
            ui.selectable_value(&mut self.mode, Mode::Draw, "Draw a label");
        });
        ui.checkbox(&mut self.show_engine, "Show the engine's lines (orange)");
        ui.separator();
        let Some(loaded) = self.loaded.as_ref() else {
            ui.label("No picture open.");
            return;
        };
        match &loaded.score {
            Some(s) => ui.label(format!(
                "Labels {} · found {} · text matches {} · engine lines {} · lines with no label {}",
                s.labels, s.found, s.text_matched, s.engine_lines, s.unmatched_engine_lines
            )),
            None => ui.label("Not run yet. Labels: ".to_string() + &loaded.file.labels.len().to_string()),
        };
        if !loaded.timings.is_empty() {
            let parts: Vec<String> = loaded.timings.iter().map(|t| format!("{} {:.0} ms", t.step, t.ms)).collect();
            ui.small(parts.join(" · "));
        }
        ui.separator();
        ui.strong("Labels");
        let mut select = None;
        let mut delete = false;
        let mut text_changed = false;
        egui::ScrollArea::vertical().max_height(260.0).id_salt("labels").show(ui, |ui| {
            if let Some(loaded) = self.loaded.as_mut() {
                for (i, label) in loaded.file.labels.iter_mut().enumerate() {
                    let is_selected = self.selected == Some(i);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(is_selected, format!("#{}", i + 1)).clicked() {
                            select = Some(i);
                        }
                        if is_selected {
                            let edit = ui.add(egui::TextEdit::singleline(&mut label.text).hint_text("the text in this box"));
                            text_changed |= edit.lost_focus();
                        } else {
                            ui.label(if label.text.is_empty() { "(no text)" } else { label.text.as_str() });
                        }
                    });
                    if is_selected {
                        let r = label.rect;
                        ui.small(format!("box {:.0}, {:.0} to {:.0}, {:.0}", r[0], r[1], r[2], r[3]));
                        if ui.button("Delete this label (Delete key)").clicked() {
                            delete = true;
                        }
                    }
                }
            }
        });
        if let Some(i) = select {
            self.selected = if self.selected == Some(i) { None } else { Some(i) };
        }
        if delete {
            self.delete_selected();
        } else if text_changed {
            self.save();
        }
        if let Some(loaded) = self.loaded.as_ref() {
            ui.separator();
            ui.strong(format!("Engine lines ({})", loaded.lines.len()));
            egui::ScrollArea::vertical().id_salt("lines").show(ui, |ui| {
                for line in &loaded.lines {
                    let conf = line.confidence.map_or("no score".to_string(), |c| format!("{:.2}", c));
                    ui.label(format!("{}  [{conf}]", line.text));
                }
            });
        }
    }

    fn picture_area(&mut self, ui: &mut egui::Ui) {
        let Some(loaded) = self.loaded.as_ref() else {
            ui.centered_and_justified(|ui| ui.label("Pick a test case on the left."));
            return;
        };
        let [w, h] = loaded.size;
        let avail = ui.available_size();
        let scale = (avail.x / w as f32).min(avail.y / h as f32).clamp(0.01, 4.0);
        let size = Vec2::new(w as f32 * scale, h as f32 * scale);
        let response = ui.allocate_response(size, Sense::click_and_drag());
        let origin = response.rect.min;
        let painter = ui.painter_at(response.rect);
        painter.image(loaded.texture.id(), response.rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        if self.show_engine {
            for line in &loaded.lines {
                let pts: Vec<Pos2> = line.quad.iter().map(|p| Self::to_screen(origin, scale, [p.x, p.y])).collect();
                for i in 0..4 {
                    painter.line_segment([pts[i], pts[(i + 1) % 4]], Stroke::new(1.5, ENGINE_COLOUR));
                }
            }
        }
        for (i, label) in loaded.file.labels.iter().enumerate() {
            let r = Self::normalized(label.rect);
            let screen = Rect::from_min_max(Self::to_screen(origin, scale, [r[0], r[1]]), Self::to_screen(origin, scale, [r[2], r[3]]));
            let colour = if self.selected == Some(i) { SELECTED_COLOUR } else { LABEL_COLOUR };
            painter.rect_stroke(screen, CornerRadius::ZERO, Stroke::new(2.0, colour), StrokeKind::Outside);
            let tag = if label.text.is_empty() { format!("#{}", i + 1) } else { format!("#{} {}", i + 1, label.text) };
            painter.text(screen.min + Vec2::new(2.0, -2.0), egui::Align2::LEFT_BOTTOM, tag, egui::FontId::proportional(12.0), colour);
        }
        if let Some(Drag::New { start, now }) = self.drag {
            let r = Self::normalized([start[0], start[1], now[0], now[1]]);
            let screen = Rect::from_min_max(Self::to_screen(origin, scale, [r[0], r[1]]), Self::to_screen(origin, scale, [r[2], r[3]]));
            painter.rect_stroke(screen, CornerRadius::ZERO, Stroke::new(2.0, LABEL_COLOUR), StrokeKind::Outside);
        }
        self.handle_pointer(&response, origin, scale);
    }

    fn handle_pointer(&mut self, response: &egui::Response, origin: Pos2, scale: f32) {
        let size = match self.loaded.as_ref() {
            Some(l) => l.size,
            None => return,
        };
        if response.drag_started() {
            let Some(pos) = response.interact_pointer_pos() else { return };
            let p = Self::clamp_point(Self::to_pixels(origin, scale, pos), size);
            match self.mode {
                Mode::Draw => self.drag = Some(Drag::New { start: p, now: p }),
                Mode::Select => {
                    let Some(loaded) = self.loaded.as_ref() else { return };
                    // A corner within reach resizes; inside a label moves it; anywhere else clears the selection.
                    let mut found = None;
                    for (i, label) in loaded.file.labels.iter().enumerate().rev() {
                        for (c, corner) in Self::corners(label.rect).iter().enumerate() {
                            let s = Self::to_screen(origin, scale, *corner);
                            if (s - pos).length() <= HANDLE_PX {
                                found = Some(Drag::Corner { index: i, corner: c });
                            }
                            if found.is_some() {
                                break;
                            }
                        }
                        if found.is_some() {
                            break;
                        }
                    }
                    if found.is_none()
                        && let Some(i) = Self::label_at(&loaded.file.labels, p)
                    {
                        found = Some(Drag::Move { index: i, last: p });
                    }
                    self.selected = match found {
                        Some(Drag::Corner { index, .. }) | Some(Drag::Move { index, .. }) => Some(index),
                        _ => None,
                    };
                    self.drag = found;
                }
            }
        }
        if response.dragged() {
            let Some(pos) = response.interact_pointer_pos() else { return };
            let p = Self::clamp_point(Self::to_pixels(origin, scale, pos), size);
            let Some(drag) = self.drag else { return };
            match drag {
                Drag::New { start, .. } => self.drag = Some(Drag::New { start, now: p }),
                Drag::Move { index, last } => {
                    if let Some(loaded) = self.loaded.as_mut()
                        && let Some(label) = loaded.file.labels.get_mut(index)
                    {
                        {
                            let (dx, dy) = (p[0] - last[0], p[1] - last[1]);
                            let r = Self::normalized(label.rect);
                            // Kept inside the picture; a label drawn past its edge only moves inward.
                            let dx = dx.max(-r[0]).min(size[0] as f32 - r[2]);
                            let dy = dy.max(-r[1]).min(size[1] as f32 - r[3]);
                            label.rect = [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy];
                        }
                    }
                    self.drag = Some(Drag::Move { index, last: p });
                }
                Drag::Corner { index, corner } => {
                    if let Some(loaded) = self.loaded.as_mut()
                        && let Some(label) = loaded.file.labels.get_mut(index)
                    {
                        {
                            match corner {
                                0 => {
                                    label.rect[0] = p[0];
                                    label.rect[1] = p[1];
                                }
                                1 => {
                                    label.rect[2] = p[0];
                                    label.rect[1] = p[1];
                                }
                                2 => {
                                    label.rect[2] = p[0];
                                    label.rect[3] = p[1];
                                }
                                _ => {
                                    label.rect[0] = p[0];
                                    label.rect[3] = p[1];
                                }
                            }
                        }
                    }
                }
            }
        }
        if response.drag_stopped() {
            match self.drag.take() {
                Some(Drag::New { start, now }) => {
                    let r = Self::normalized([start[0], start[1], now[0], now[1]]);
                    if r[2] - r[0] >= 2.0 && r[3] - r[1] >= 2.0 {
                        if let Some(loaded) = self.loaded.as_mut() {
                            loaded.file.labels.push(Label { text: String::new(), rect: r });
                            self.selected = Some(loaded.file.labels.len() - 1);
                        }
                        self.save();
                    }
                }
                Some(_) => {
                    if let Some(loaded) = self.loaded.as_mut() {
                        for label in loaded.file.labels.iter_mut() {
                            label.rect = Self::normalized(label.rect);
                        }
                    }
                    self.save();
                }
                None => {}
            }
        }
        if response.clicked() && self.drag.is_none() && self.mode == Mode::Select {
            let Some(pos) = response.interact_pointer_pos() else { return };
            let p = Self::to_pixels(origin, scale, pos);
            self.selected = self.loaded.as_ref().and_then(|l| Self::label_at(&l.file.labels, p));
        }
    }
}

fn folder_name(folder: &Path) -> String {
    folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

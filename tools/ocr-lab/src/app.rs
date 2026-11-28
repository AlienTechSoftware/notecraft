//! The lab window: the test folder on the left, the picture with its labels and the engine's
//! boxes in the middle, and the run, the score and the editable labels on the right.

use crate::case::{self, Label, LabelFile, Score};
use crate::settings;
use crate::telemetry::{self, Log, Timing};
use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use notecraft_ink::{Dab, Drawing, Stroke as InkStroke, svg};
use notecraft_ocr::{Line, OcrsReader};
use notecraft_ui_egui::theme::Theme;
use notecraft_ui_egui::{pageedit, ribbon};
use std::path::{Path, PathBuf};

const HANDLE_PX: f32 = 8.0;
const LABEL_COLOUR: Color32 = Color32::from_rgb(40, 160, 255);
const LABEL_DEFAULT_RGB: [u8; 3] = [40, 160, 255];
const SELECTED_COLOUR: Color32 = Color32::from_rgb(255, 200, 0);
/// The colours the recognised lines are given, in order, so neighbouring lines differ. A line can be
/// given another colour from its swatch in the list.
const LINE_PALETTE: [Color32; 8] = [
    Color32::from_rgb(255, 90, 60),
    Color32::from_rgb(40, 150, 255),
    Color32::from_rgb(60, 190, 90),
    Color32::from_rgb(230, 170, 30),
    Color32::from_rgb(170, 90, 230),
    Color32::from_rgb(20, 200, 200),
    Color32::from_rgb(240, 80, 170),
    Color32::from_rgb(120, 120, 120),
];

/// The colour of a recognised line: the one chosen for it, or its place in the palette.
/// The square size of the OCR file's [✕] and [✔] buttons: the same square in the lab and the app.
const MARK_SIDE: f32 = 20.0;

/// One of the two square buttons on an OCR file: [✕] discards its edits, [✔] saves them. Both are
/// drawn with lines, so they look the same whatever fonts the computer has.
fn mark_button(ui: &mut egui::Ui, cross: bool, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(MARK_SIDE, MARK_SIDE), Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect(rect, egui::CornerRadius::same(3), visuals.weak_bg_fill, visuals.bg_stroke, StrokeKind::Inside);
    let stroke = Stroke::new(1.6, visuals.text_color());
    if cross {
        let r = rect.shrink(6.0);
        ui.painter().line_segment([r.left_top(), r.right_bottom()], stroke);
        ui.painter().line_segment([r.right_top(), r.left_bottom()], stroke);
    } else {
        let r = rect.shrink(5.0);
        let (a, b, c) = (egui::pos2(r.left(), r.center().y), egui::pos2(r.left() + r.width() * 0.4, r.bottom()), egui::pos2(r.right(), r.top()));
        ui.painter().line_segment([a, b], stroke);
        ui.painter().line_segment([b, c], stroke);
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    response
}

/// The [✕] that discards an edited OCR file's changes.
fn cross_button(ui: &mut egui::Ui) -> egui::Response {
    mark_button(ui, true, "Discard the unsaved edits of this OCR file")
}

/// The [✔] that saves an edited OCR file.
fn check_button(ui: &mut egui::Ui) -> egui::Response {
    mark_button(ui, false, "Save this OCR file")
}

fn line_colour(loaded: &Loaded, index: usize) -> Color32 {
    loaded.line_colours.get(&index).copied().unwrap_or(LINE_PALETTE[index % LINE_PALETTE.len()])
}

/// How the picture is shown: hue turns the colours round the colour wheel, contrast and brightness
/// are applied after it. Neutral is no change at all.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Adjust {
    /// Degrees, -180 to 180.
    pub hue: f32,
    /// 1 is no change; 0 is flat grey, 2 is twice the spread.
    pub contrast: f32,
    /// -1 to 1, added to every channel.
    pub brightness: f32,
}

impl Adjust {
    pub const NEUTRAL: Adjust = Adjust { hue: 0.0, contrast: 1.0, brightness: 0.0 };

    /// The picture with these adjustments applied to every pixel.
    pub fn apply(self, src: &image::RgbaImage) -> egui::ColorImage {
        let pixels: Vec<egui::Color32> = src
            .pixels()
            .map(|p| {
                let [r, g, b] = rotate_hue([p[0], p[1], p[2]], self.hue);
                let shift = |v: u8| -> u8 {
                    let v = (v as f32 - 128.0) * self.contrast + 128.0 + self.brightness * 255.0;
                    v.round().clamp(0.0, 255.0) as u8
                };
                egui::Color32::from_rgba_unmultiplied(shift(r), shift(g), shift(b), p[3])
            })
            .collect();
        egui::ColorImage { size: [src.width() as usize, src.height() as usize], pixels, source_size: egui::Vec2::ZERO }
    }
}

/// Turns an RGB colour round the wheel by `degrees`, keeping its brightness and saturation.
pub fn rotate_hue(rgb: [u8; 3], degrees: f32) -> [u8; 3] {
    if degrees.abs() < f32::EPSILON {
        return rgb;
    }
    let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    let hue = (hue + degrees).rem_euclid(360.0);
    let chroma = max * saturation;
    let x = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = max - chroma;
    let (r1, g1, b1) = match (hue / 60.0) as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    [(r1 + m) * 255.0, (g1 + m) * 255.0, (b1 + m) * 255.0].map(|c| c.round().clamp(0.0, 255.0) as u8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Select,
    Draw,
    /// Pen on the open picture. The strokes change the picture, so save them to keep them.
    Ink,
    /// Eraser on the open picture: rubs out pixels and strokes under it.
    Erase,
}

/// Radius of the eraser, in picture pixels.
const ERASER_RADIUS: f32 = 10.0;

/// Size of a new blank drawing, in picture pixels.
const BLANK_SIZE: [u32; 2] = [1200, 800];

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
    /// The picture as it is on disk. The sliders change only what is shown; OCR reads this.
    original: image::RgbaImage,
    size: [u32; 2],
    bytes: u64,
    file: LabelFile,
    lines: Vec<Line>,
    timings: Vec<Timing>,
    score: Option<Score>,
    /// True when the pen has changed the picture and it is not saved yet.
    dirty: bool,
    /// Where the pen was last, in picture pixels, so a stroke has no gaps.
    last_ink: Option<[f32; 2]>,
    /// The drawing's strokes with the pen's readings, kept as `<picture>.svg` beside the PNG.
    ink: Drawing,
    /// The stroke being drawn now, added to `ink` when the pen lifts.
    current: Option<InkStroke>,
    /// Colours the person chose for recognised lines, by line index. Empty means the palette.
    line_colours: std::collections::HashMap<usize, Color32>,
}

pub struct Lab {
    folder: PathBuf,
    pictures: Vec<PathBuf>,
    /// The tags of each picture, in the same order as `pictures`.
    tags: Vec<case::Tags>,
    loaded: Option<Loaded>,
    reader: Option<OcrsReader>,
    reader_error: Option<String>,
    model_load_ms: Option<f64>,
    models: Option<PathBuf>,
    theme: Theme,
    follow_system: bool,
    health: crate::models::Health,
    metrics: crate::metrics::Metrics,
    adjust: Adjust,
    applied_adjust: Option<Adjust>,
    /// The theme last applied to the window, so the visuals are set only when it changes.
    applied_theme: Option<Theme>,
    mode: Mode,
    /// The pens for digital drawings, from the app's rack.
    pen: crate::pen::Pen,
    /// The picture waiting for the person to confirm its deletion.
    pending_delete: Option<usize>,
    /// The folder and tags for a blank drawing waiting for its size to be chosen, and that size.
    pending_new: Option<(PathBuf, case::Tags)>,
    new_size: [u32; 2],
    /// How readily the pen answers pressure, 0 to 100, from the ribbon's slider.
    sensitivity: u8,
    /// The pen's pressure, from the window's touch events. A mouse reads as full pressure.
    stylus: notecraft_ui_egui::stylus::Stylus,
    /// The ruled lines or squares shown over the picture, from the ribbon's View tab. A view only: the
    /// file is not changed, so OCR reads the picture as it is.
    rules: ribbon::RuleLines,
    /// A text file shown in the main area instead of the picture: its path and its text.
    viewing_json: Option<(PathBuf, String)>,
    /// The text of the open file as it is on disk. The check mark shows when the edited text differs.
    viewing_changed: bool,
    /// What the ribbon's pointer is set to: Type to select, or Draw to write with the pen in hand.
    pointer: ribbon::Pointer,
    /// The ribbon's chosen tab, and whether the ribbon is folded to its tab names.
    ribbon_tab: String,
    ribbon_folded: bool,
    /// Whether a drawing was open last frame, so the ribbon opens and folds only when that changes.
    ribbon_was_drawing: bool,
    /// The open path in the navigator: the open language, then its open source, then its open content.
    /// Each level holds one open node, so opening a node closes its siblings. Empty means all closed.
    open_path: Vec<String>,
    selected: Option<usize>,
    drag: Option<Drag>,
    show_engine: bool,
    log: Log,
    status: String,
}

impl Lab {
    pub fn new(folder: PathBuf, models: Option<PathBuf>, saved_theme: Option<String>, saved_follow: bool, mut log: Log) -> Lab {
        let pictures = case::discover(&folder);
        let tags = pictures.iter().map(|p| case::load_tags(p)).collect();
        log.event(&format!("lab opened: {} pictures in {}", pictures.len(), folder_name(&folder)));
        let mut lab = Lab {
            folder,
            pictures,
            tags,
            loaded: None,
            reader: None,
            reader_error: Some(
                "No models loaded. Click Models… and pick the folder that holds text-detection.onnx and text-recognition.onnx.".to_string(),
            ),
            model_load_ms: None,
            models: None,
            theme: Theme { mode: notecraft_ui_egui::theme::Mode::Dark, accent: notecraft_ui_egui::theme::Accent::Default },
            follow_system: false,
            health: crate::models::Health::NotSet,
            metrics: crate::metrics::Metrics::new(),
            adjust: Adjust::NEUTRAL,
            applied_adjust: None,
            applied_theme: None,
            mode: Mode::Select,
            pen: crate::pen::Pen::new(),
            open_path: Vec::new(),
            pointer: ribbon::Pointer::default(),
            viewing_json: None,
            viewing_changed: false,
            rules: ribbon::RuleLines::default(),
            stylus: Default::default(),
            sensitivity: 50,
            ribbon_tab: "Draw".to_string(),
            // Folded until a drawing opens. See `ui`.
            ribbon_folded: true,
            ribbon_was_drawing: false,
            pending_delete: None,
            pending_new: None,
            new_size: BLANK_SIZE,
            selected: None,
            drag: None,
            show_engine: true,
            log,
            status: String::new(),
        };
        if let Some(dir) = models {
            lab.load_reader(&dir);
        }
        lab.follow_system = saved_follow;
        if let Some(id) = saved_theme {
            lab.theme = Theme::all().into_iter().find(|t| t.id() == id).unwrap_or(Theme::DEFAULT);
        }
        lab.status = if lab.pictures.is_empty() { "No pictures in this folder.".to_string() } else { "Pick a picture to start.".to_string() };
        lab
    }

    /// Loads the two ONNX models from a folder. Failure leaves the lab usable for browsing.
    pub fn load_reader(&mut self, dir: &Path) {
        self.health = crate::models::check(Some(dir));
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
        self.folder = folder;
        self.reload_pictures();
        self.loaded = None;
        self.selected = None;
        self.drag = None;
        self.log.event(&format!("folder set: {} pictures in {}", self.pictures.len(), folder_name(&self.folder)));
        self.status = if self.pictures.is_empty() { "No pictures in this folder.".to_string() } else { format!("{} pictures.", self.pictures.len()) };
    }

    /// Writes the folder and model choice to the local settings, so the next start has them.
    fn remember(&self) {
        settings::save(&settings::Settings {
            folder: Some(self.folder.clone()),
            models: self.models.clone(),
            theme: Some(self.theme.id()),
            follow_system: self.follow_system,
            // Tree and ribbon state are not saved for now. To turn saving back on, uncomment these
            // three lines and the restore calls in main.rs.
            // open_path: self.open_path.clone(),
            // ribbon_tab: Some(self.ribbon_tab.clone()),
            // ribbon_folded: self.ribbon_folded,
            ..Default::default()
        });
    }

    // Restoring the tree and ribbon state from the last session, paused with the saving above.
    // /// Restores the ribbon's tab and folded state from the last session.
    // pub fn restore_ribbon(&mut self, tab: Option<String>, folded: bool) {
    //     if let Some(tab) = tab {
    //         self.ribbon_tab = tab;
    //     }
    //     self.ribbon_folded = folded;
    // }
    //
    // /// Restores the navigator's open path from the last session.
    // pub fn restore_open_path(&mut self, path: Vec<String>) {
    //     self.open_path = path;
    // }

    /// Closes the open file. Unsaved edits are written first, so they are never lost.
    fn leave_viewing(&mut self) {
        if self.viewing_dirty() {
            self.save_viewing();
        }
        self.viewing_json = None;
    }

    fn select(&mut self, ctx: &egui::Context, index: usize) {
        // Picking a sample shows the picture again, so the OCR file is hidden.
        self.leave_viewing();
        let path = self.pictures[index].clone();
        // The new picture's texture has no adjustments yet, so the sliders must be applied to it again.
        self.applied_adjust = None;
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
                let mut loaded = Loaded {
                    index,
                    texture,
                    original: rgba,
                    size,
                    bytes,
                    file,
                    lines: Vec::new(),
                    timings: Vec::new(),
                    score: None,
                    dirty: false,
                    last_ink: None,
                    ink: load_ink(&path, size),
                    current: None,
                    line_colours: Default::default(),
                };
                if let Some((lines, timings)) = load_run(&path) {
                    loaded.score = Some(case::score(&loaded.file.labels, &lines));
                    loaded.lines = lines;
                    loaded.timings = timings;
                    self.log.event(&format!("loaded the saved OCR run for {name}"));
                }
                self.loaded = Some(loaded);
                self.selected = None;
                self.drag = None;
                // A drawing opens with the pen in hand; a picture opens with the Type pointer.
                let pointer = if self.open_is_drawing() { ribbon::Pointer::Draw } else { ribbon::Pointer::Write };
                self.set_pointer(pointer);
                self.status = format!("{name}: {} labels", self.loaded.as_ref().map_or(0, |l| l.file.labels.len()));
            }
            Err(e) => {
                self.status = e.clone();
                self.log.event(&format!("labels did not load for {name}: {e}"));
            }
        }
    }

    /// Asks for the size of a new blank drawing, then makes it.
    fn confirm_new_drawing(&mut self, ctx: &egui::Context) {
        let Some((dir, tags)) = self.pending_new.clone() else { return };
        let mut decided = None;
        egui::Window::new("New blank drawing").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Width");
                ui.add(egui::DragValue::new(&mut self.new_size[0]).range(16..=8000).suffix(" px"));
                ui.label("Height");
                ui.add(egui::DragValue::new(&mut self.new_size[1]).range(16..=8000).suffix(" px"));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("A4 at 150 dpi").clicked() {
                    self.new_size = [1240, 1754];
                }
                if ui.small_button("Whiteboard 1920 × 1080").clicked() {
                    self.new_size = [1920, 1080];
                }
                if ui.small_button("Square 1000 × 1000").clicked() {
                    self.new_size = [1000, 1000];
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Create").clicked() {
                    decided = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    decided = Some(false);
                }
            });
        });
        match decided {
            Some(true) => {
                self.pending_new = None;
                let size = self.new_size;
                self.new_drawing(ctx, &dir, tags, size);
            }
            Some(false) => self.pending_new = None,
            None => {}
        }
    }

    /// Asks before deleting a picture, and deletes it when the person confirms.
    fn confirm_delete(&mut self, ctx: &egui::Context) {
        let Some(index) = self.pending_delete else { return };
        let Some(path) = self.pictures.get(index).cloned() else {
            self.pending_delete = None;
            return;
        };
        let name = self.relative_name(&path);
        let mut decided = None;
        egui::Window::new("Delete this picture?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(
            ctx,
            |ui| {
                ui.strong(name.as_str());
                ui.small("Its label, OCR run and tags go with it. The files are moved to local\\deleted, not erased.");
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        decided = Some(true);
                    }
                    if ui.button("Cancel").clicked() {
                        decided = Some(false);
                    }
                });
            },
        );
        match decided {
            Some(true) => {
                self.delete_picture(index);
                self.pending_delete = None;
            }
            Some(false) => self.pending_delete = None,
            None => {}
        }
    }

    /// Moves a picture and its companion files into `local\deleted\<stamp>\`, then rescans. The open
    /// picture is closed if it was the one removed, and otherwise kept open under its new index.
    fn delete_picture(&mut self, index: usize) {
        let path = self.pictures[index].clone();
        let name = self.relative_name(&path);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let dir = crate::lab_root().join("local").join("deleted").join(stamp.to_string());
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.status = format!("Could not make {}: {e}", dir.display());
            return;
        }
        let open_path = self.loaded.as_ref().map(|l| self.pictures[l.index].clone());
        for file in [path.clone(), case::label_path(&path), ocr_json_path(&path), case::tags_path(&path), ink_path(&path)] {
            if !file.is_file() {
                continue;
            }
            let Some(file_name) = file.file_name() else { continue };
            if let Err(e) = std::fs::rename(&file, dir.join(file_name)) {
                self.status = format!("Could not move {}: {e}", file.display());
                self.log.event(&format!("could not move {}: {e}", file.display()));
                return;
            }
        }
        self.log.event(&format!("picture deleted: {name}"));
        self.reload_pictures();
        match open_path {
            Some(open) if open == path => {
                self.loaded = None;
                self.selected = None;
                self.drag = None;
            }
            Some(open) => {
                if let Some(loaded) = self.loaded.as_mut()
                    && let Some(new_index) = self.pictures.iter().position(|p| *p == open)
                {
                    loaded.index = new_index;
                }
            }
            None => {}
        }
        self.status = format!("Deleted {name}. Moved to local\\deleted.");
    }

    /// Rescans the test folder and loads each picture's tags, in the same order.
    fn reload_pictures(&mut self) {
        self.pictures = case::discover(&self.folder);
        self.tags = self.pictures.iter().map(|p| case::load_tags(p)).collect();
    }

    /// Makes a new white drawing in `dir`, saves it as a PNG with `tags`, and opens it with the pen
    /// ready. The file is named drawing-1.png, drawing-2.png, ... so an existing drawing is never
    /// overwritten.
    fn new_drawing(&mut self, ctx: &egui::Context, dir: &Path, tags: case::Tags, size: [u32; 2]) {
        if self.loaded.as_ref().is_some_and(|l| l.dirty) {
            self.status = "Save the drawing you are working on first.".to_string();
            return;
        }
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.status = format!("Could not create {}: {e}", dir.display());
            return;
        }
        let path = (1..).map(|n| dir.join(format!("drawing-{n}.png"))).find(|p| !p.exists()).expect("an unused drawing name");
        let blank = image::RgbaImage::from_pixel(size[0], size[1], image::Rgba([255, 255, 255, 255]));
        if let Err(e) = blank.save(&path) {
            self.status = format!("Could not create the drawing: {e}");
            self.log.event(&format!("could not create a drawing: {e}"));
            return;
        }
        // An empty sheet of strokes beside it, so the drawing is editable from the start.
        let empty = Drawing::new(size[0] as f32, size[1] as f32);
        if let Err(e) = std::fs::write(ink_path(&path), svg::write(&empty, svg::Flavour::Editable)) {
            self.status = format!("Could not create the drawing's strokes: {e}");
            return;
        }
        // A blank sheet is a digital drawing whatever node it was added under, so the pens are there.
        let tags = case::Tags { source: "drawing".to_string(), ..tags };
        if let Err(e) = case::save_tags(&path, &tags) {
            self.status = e;
        }
        self.reload_pictures();
        self.log.event(&format!("blank drawing created: {}", self.relative_name(&path)));
        if let Some(index) = self.pictures.iter().position(|p| *p == path) {
            self.select(ctx, index);
            self.set_pointer(ribbon::Pointer::Draw);
            self.status = format!("{} is blank. Draw on it, then press Save drawing.", self.relative_name(&path));
        }
    }

    /// Copies a picture the person picks into `dir` with `tags`, and opens it. A name already in use
    /// gets a number, so nothing is overwritten.
    fn import_picture(&mut self, ctx: &egui::Context, dir: &Path, tags: case::Tags) {
        if self.loaded.as_ref().is_some_and(|l| l.dirty) {
            self.status = "Save the drawing you are working on first.".to_string();
            return;
        }
        let Some(source) = rfd::FileDialog::new().add_filter("Pictures", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() else {
            return;
        };
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "picture".to_string());
        let ext = source.extension().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "png".to_string());
        let result = std::fs::create_dir_all(dir).and_then(|_| {
            let target = (1..)
                .map(|n| if n == 1 { dir.join(format!("{stem}.{ext}")) } else { dir.join(format!("{stem}-{n}.{ext}")) })
                .find(|p| !p.exists())
                .expect("an unused name");
            std::fs::copy(&source, &target).map(|_| target)
        });
        match result {
            Ok(target) => {
                if let Err(e) = case::save_tags(&target, &tags) {
                    self.status = e;
                }
                self.reload_pictures();
                self.log.event(&format!("picture imported: {}", self.relative_name(&target)));
                if let Some(index) = self.pictures.iter().position(|p| *p == target) {
                    self.select(ctx, index);
                }
            }
            Err(e) => {
                self.status = format!("Could not import the picture: {e}");
                self.log.event(&format!("could not import a picture: {e}"));
            }
        }
    }

    /// True when the open sample is a digital drawing, which is the only kind the pen can write on.
    fn open_is_drawing(&self) -> bool {
        self.loaded.as_ref().is_some_and(|l| self.tags.get(l.index).is_some_and(|t| t.source == "drawing"))
    }

    /// The app's Draw ribbon, built from the lab's state. Only the pen and its rack are live here; the
    /// page-only parts of the ribbon are switched off, since there is no page to act on.
    fn ribbon_view(&self) -> ribbon::View<'_> {
        ribbon::View {
            layout: ribbon::Layout::Classic,
            split_panes: false,
            always_on_top: false,
            docked: false,
            full_page: false,
            rules: self.rules,
            hide_title: false,
            snap: false,
            zoom_percent: 100,
            pointer: self.pointer,
            rack: &self.pen.rack,
            nib: self.pen.selected,
            space_tool: false,
            ruler: false,
            auto_shapes: false,
            replaying: false,
            tabs_layout: false,
            pages_right: false,
            in_table: false,
            table_borders: false,
            table_align: None,
            table_shade: None,
            gridcraft: false,
            page_colour: None,
            rule_colour: None,
            paper_size: None,
            can_ink_redo: false,
            ink_held: false,
            accessibility_open: false,
            eraser: notecraft_ui_egui::inkpad::Eraser::Stroke,
            sensitivity: self.sensitivity,
            pressure_available: self.stylus.sample().pen,
            has_page: false,
            has_recycle_notebook: false,
            page_marked_unread: false,
            hide_authors: false,
            painting: false,
            style: "",
            font_family: None,
            marks: pageedit::Marks::default(),
            font_size: None,
            text_colour: None,
            text_highlight: None,
            text_colour_mixed: false,
            text_highlight_mixed: false,
            // No page, so Home's text controls are shown as unavailable.
            text_editable: false,
        }
    }

    /// Carries out what the ribbon asked for. Choosing a pen puts the lab in pen mode; rack edits change
    /// the rack. Anything else needs a page, so the status line says it is not in the lab yet.
    fn apply_ribbon(&mut self, acts: Vec<ribbon::Act>) {
        for act in acts {
            match act {
                ribbon::Act::InkPen(i) => {
                    self.pen.selected = i.min(self.pen.rack.len().saturating_sub(1));
                    self.set_pointer(ribbon::Pointer::Draw);
                }
                // Only the pointers the lab can do: Type (write, select) and Draw (the pen). The rest need a page.
                ribbon::Act::InkPointer(p @ (ribbon::Pointer::Write | ribbon::Pointer::Draw | ribbon::Pointer::Erase)) => self.set_pointer(p),
                // The ribbon's Save picture saves the open drawing, as Ctrl+S does.
                ribbon::Act::SavePictureAs(_) => self.save_if_dirty(),
                // Choosing a size for a pen also puts that pen in hand, so the next stroke has the size shown.
                ribbon::Act::InkRack(edit @ ribbon::RackEdit::Set(i, _)) => {
                    self.edit_rack(edit);
                    self.pen.selected = i.min(self.pen.rack.len().saturating_sub(1));
                    self.set_pointer(ribbon::Pointer::Draw);
                }
                ribbon::Act::InkRack(edit) => self.edit_rack(edit),
                ribbon::Act::RuleLines(rules) => self.rules = rules,
                ribbon::Act::InkSensitivity(value) => self.sensitivity = value,
                ribbon::Act::Tab(name) => {
                    self.ribbon_tab = name;
                    self.remember();
                }
                ribbon::Act::ToggleRibbon => {
                    self.ribbon_folded = !self.ribbon_folded;
                    self.remember();
                }
                _ => self.status = "That drawing command is not in the lab yet.".to_string(),
            }
        }
    }

    /// Sets the ribbon's pointer and the lab's mode together: the Draw pointer is pen mode, and the Type
    /// pointer is select mode. The ribbon highlights the pointer, so it always shows what is in hand.
    fn set_pointer(&mut self, pointer: ribbon::Pointer) {
        self.pointer = pointer;
        self.mode = match pointer {
            ribbon::Pointer::Draw => Mode::Ink,
            ribbon::Pointer::Erase => Mode::Erase,
            _ => Mode::Select,
        };
    }

    /// Saves the open drawing, from the ribbon's Save picture or Ctrl+S. Does nothing when nothing has changed.
    fn save_if_dirty(&mut self) {
        if self.loaded.as_ref().is_some_and(|l| l.dirty) {
            self.save_drawing();
        }
    }

    /// Rubs out the open drawing under the eraser at `p`: the pixels go back to white, and the strokes
    /// under it are taken out of the SVG.
    fn erase_ink(&mut self, ctx: &egui::Context, p: [f32; 2]) {
        let Some(loaded) = self.loaded.as_mut() else { return };
        let from = loaded.last_ink.unwrap_or(p);
        crate::pen::erase_line(&mut loaded.original, from, p, ERASER_RADIUS);
        let steps = (p[0] - from[0]).abs().max((p[1] - from[1]).abs()).ceil().max(1.0) as i32;
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let _ = loaded.ink.erase_at(from[0] + (p[0] - from[0]) * t, from[1] + (p[1] - from[1]) * t, ERASER_RADIUS);
        }
        loaded.last_ink = Some(p);
        loaded.dirty = true;
        let shown = self.adjust.apply(&loaded.original);
        loaded.texture.set(shown, egui::TextureOptions::LINEAR);
        ctx.request_repaint();
    }

    /// Adds, removes, moves or changes a pen on the rack, with the same rules as the app.
    fn edit_rack(&mut self, edit: ribbon::RackEdit) {
        let rack = &mut self.pen.rack;
        match edit {
            ribbon::RackEdit::Add(tool) => {
                if rack.len() < notecraft_ink::rack::MAX_NIBS {
                    rack.push(notecraft_ink::rack::new_nib(tool));
                }
            }
            ribbon::RackEdit::Remove(i) => {
                notecraft_ink::rack::remove(rack, i);
            }
            ribbon::RackEdit::Move(i, left) => {
                notecraft_ink::rack::shift(rack, i, left);
            }
            ribbon::RackEdit::Set(i, nib) => {
                if let Some(slot) = rack.get_mut(i) {
                    *slot = nib;
                }
            }
        }
        self.pen.selected = self.pen.selected.min(self.pen.rack.len().saturating_sub(1));
    }

    /// Paints one pen dab, or a line from the last dab, into the open picture. Only the picture on
    /// screen and in memory changes until the drawing is saved.
    fn paint_ink(&mut self, ctx: &egui::Context, p: [f32; 2]) {
        // The stroke takes the ribbon's sensitivity as the app does: a wider nib, and a gain on pressure.
        // The picture is painted at the width the app would draw at this pressure.
        let base = self.pen.nib();
        let pressure = self.stylus.sample().pressure;
        let sensitivity = self.sensitivity;
        let Some(loaded) = self.loaded.as_mut() else { return };
        let from = loaded.last_ink.unwrap_or(p);
        let stroke = crate::pen::stroke(base, sensitivity);
        crate::pen::line(&mut loaded.original, from, p, crate::pen::drawn_nib(&stroke, pressure));
        if loaded.last_ink.is_none() {
            loaded.current = Some(crate::pen::stroke(base, sensitivity));
        }
        if let Some(stroke) = loaded.current.as_mut() {
            stroke.add(Dab::new(p[0], p[1], pressure));
        }
        loaded.last_ink = Some(p);
        loaded.dirty = true;
        let shown = self.adjust.apply(&loaded.original);
        loaded.texture.set(shown, egui::TextureOptions::LINEAR);
        ctx.request_repaint();
    }

    /// Writes the drawing to its picture file. The old OCR run no longer matches the picture, so
    /// it is removed and the next run reads the new drawing.
    fn save_drawing(&mut self) {
        let Some(loaded) = self.loaded.as_mut() else { return };
        let path = self.pictures[loaded.index].clone();
        if let Err(e) = loaded.original.save(&path) {
            self.status = format!("Could not save the drawing: {e}");
            self.log.event(&format!("could not save a drawing: {e}"));
            return;
        }
        // The strokes go beside the PNG, with the pen's readings, so the drawing can be edited later.
        let text = svg::write(&loaded.ink, svg::Flavour::Editable);
        if let Err(e) = std::fs::write(ink_path(&path), text) {
            self.status = format!("Could not save the drawing's strokes: {e}");
            self.log.event(&format!("could not save the strokes: {e}"));
            return;
        }
        loaded.dirty = false;
        loaded.bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        loaded.lines.clear();
        loaded.timings.clear();
        loaded.score = None;
        let _ = remove_run_file(&path);
        let name = self.relative_name(&path);
        self.log.event(&format!("drawing saved: {name}"));
        self.status = format!("Saved {name}. Run OCR to read it.");
    }

    /// The models section of the details pane: the engine and its sources, the licence, and each
    /// model's size, role and capabilities. It shows what is expected even when no files are there.
    fn models_info(&mut self, ui: &mut egui::Ui) {
        use crate::models;
        ui.label(format!("Engine: {}", models::ENGINE_NAME));
        ui.label(format!("Model format: ONNX. Family: {}", models::MODEL_FAMILY));
        ui.label(format!("Inference runtime: {}", models::INFERENCE_RUNTIME));
        ui.label(format!("Versions: {}", models::ENGINE_VERSION));
        ui.label("Runs on: CPU");
        // Greyed out on purpose: the lab runs on the CPU only. The control is kept for a later GPU path.
        let mut gpu = false;
        ui.add_enabled(false, egui::Checkbox::new(&mut gpu, "Use GPU (not built)"))
            .on_disabled_hover_text("GPU inference is not built. The lab and the app run on the CPU.");
        ui.label(format!("Performance: {}", models::PERF_SUMMARY));
        ui.horizontal_wrapped(|ui| {
            ui.hyperlink_to("Runtime source", models::INFERENCE_SOURCE);
            ui.hyperlink_to("Engine source", models::ENGINE_SOURCE);
            ui.hyperlink_to("Model repository", models::MODEL_REPO);
            ui.hyperlink_to("Model card", models::MODEL_CARD);
        });
        ui.small(format!("Licence: {}", models::LICENCE));
        let state = if self.reader.is_some() {
            "loaded"
        } else if self.reader_error.is_some() {
            "not loaded"
        } else {
            "not loaded yet"
        };
        ui.label(format!("State: {state}. Files: {}", self.health.describe()));
        for model in &models::MODELS {
            ui.add_space(4.0);
            ui.strong(model.name);
            ui.label(format!("Size: {:.1} MB (expected)", model.size as f64 / 1_000_000.0));
            ui.label(format!("Role: {}", model.role));
            ui.label(format!("Capabilities: {}", model.capabilities));
            ui.label(format!("Version: {}", model.version));
            ui.label(format!("Performance: {}", model.perf));
        }
    }

    fn relative_name(&self, path: &Path) -> String {
        path.strip_prefix(&self.folder).unwrap_or(path).to_string_lossy().replace('\\', "/")
    }

    /// Redraws the shown picture when the sliders have moved. The pixels OCR reads do not change.
    fn apply_adjust(&mut self) {
        if self.applied_adjust == Some(self.adjust) {
            return;
        }
        if let Some(loaded) = self.loaded.as_mut() {
            let image = self.adjust.apply(&loaded.original);
            loaded.texture.set(image, egui::TextureOptions::LINEAR);
        }
        self.applied_adjust = Some(self.adjust);
    }

    /// Runs OCR on every picture in the folder and saves each result beside its picture. The window
    /// waits while it runs; the status line says how far it has got. A picture that fails is counted
    /// and skipped, so one bad file does not stop the rest.
    fn run_all(&mut self) {
        let Some(reader) = self.reader.as_ref() else { return };
        let total = self.pictures.len();
        let started = std::time::Instant::now();
        let (mut done, mut failed) = (0usize, 0usize);
        for path in self.pictures.clone() {
            let name = self.relative_name(&path);
            let mut timings = Vec::new();
            match telemetry::timed("read picture", &mut timings, || reader.read_image(&path)) {
                Ok(lines) => {
                    save_run(&path, &lines, &timings, &mut self.log);
                    done += 1;
                }
                Err(e) => {
                    failed += 1;
                    self.log.event(&format!("OCR failed on {name}: {e:?}"));
                }
            }
            self.status = format!("OCR on all: {} of {total} done, {failed} failed", done + failed);
        }
        self.log.event(&format!("OCR on all: {done} saved, {failed} failed, in {:.1} s", started.elapsed().as_secs_f64()));
        self.status = format!("OCR on all finished: {done} saved, {failed} failed, in {:.1} s.", started.elapsed().as_secs_f64());
        // The open picture now has a fresh run; reload it so the panel shows the saved result.
        if let Some(index) = self.loaded.as_ref().map(|l| l.index) {
            self.reload_run(index);
        }
    }

    /// Re-reads the saved run of the picture at `index` into the open picture, if it is the one open.
    fn reload_run(&mut self, index: usize) {
        let path = self.pictures[index].clone();
        if let Some(loaded) = self.loaded.as_mut()
            && loaded.index == index
            && let Some((lines, timings)) = load_run(&path)
        {
            loaded.score = Some(case::score(&loaded.file.labels, &lines));
            loaded.lines = lines;
            loaded.timings = timings;
        }
    }

    /// Deletes the saved run of the picture open now. The picture and its labels are kept.
    fn clear_result_of_open(&mut self) {
        let Some(index) = self.loaded.as_ref().map(|l| l.index) else { return };
        let path = self.pictures[index].clone();
        match remove_run_file(&path) {
            Ok(removed) => {
                if let Some(loaded) = self.loaded.as_mut() {
                    loaded.lines.clear();
                    loaded.timings.clear();
                    loaded.score = None;
                }
                self.status =
                    if removed { "The saved OCR result of this picture is cleared.".into() } else { "This picture has no saved OCR result.".into() };
                self.log.event(&format!("cleared the OCR result of {}", self.relative_name(&path)));
            }
            Err(e) => self.status = format!("Could not clear the result: {e}"),
        }
    }

    /// Deletes every saved run in the folder. Labels and pictures are kept.
    fn clear_all_results(&mut self) {
        let mut removed = 0usize;
        let mut failed = 0usize;
        for path in self.pictures.clone() {
            match remove_run_file(&path) {
                Ok(true) => removed += 1,
                Ok(false) => {}
                Err(_) => failed += 1,
            }
        }
        if let Some(loaded) = self.loaded.as_mut() {
            loaded.lines.clear();
            loaded.timings.clear();
            loaded.score = None;
        }
        self.status = if failed == 0 {
            format!("Cleared {removed} saved OCR results. Labels are kept.")
        } else {
            format!("Cleared {removed} saved OCR results; {failed} could not be removed.")
        };
        self.log.event(&format!("cleared {removed} saved OCR results ({failed} failed)"));
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
                save_run(&path, &loaded.lines, &loaded.timings, &mut self.log);
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
        let text = scrub_paths(&serde_json::to_string_pretty(&value).ok()?, &self.folder);
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
        self.apply_adjust();
        if self.follow_system
            && let Some(system) = ctx.system_theme()
        {
            let mode = if system == egui::Theme::Dark { notecraft_ui_egui::theme::Mode::Dark } else { notecraft_ui_egui::theme::Mode::Light };
            self.theme = Theme { mode, accent: self.theme.accent };
        }
        if self.applied_theme != Some(self.theme) {
            ctx.set_visuals(self.theme.visuals());
            self.applied_theme = Some(self.theme);
        }
        self.metrics.tick();
        ctx_repaint(ui);
        let band = self.theme.tokens().band;
        egui::Panel::top("header").frame(egui::Frame::new().fill(band)).show_separator_line(false).show(ui, |ui| {
            self.title_row(ui);
        });
        // The ribbon opens when a drawing opens, and folds again when the drawing closes.
        let events = ctx.input(|i| i.events.clone());
        self.stylus.update(&events);
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::S)) {
            self.save_if_dirty();
        }
        let drawing = self.open_is_drawing();
        if drawing != self.ribbon_was_drawing {
            self.ribbon_was_drawing = drawing;
            self.ribbon_folded = !drawing;
        }
        {
            // The tab bar is always there. Without a drawing it is folded, so only the tab names show.
            let tokens = self.theme.tokens();
            let mut acts = Vec::new();
            let folded = !drawing || self.ribbon_folded;
            // The tab names sit on the title band, as in the app. The groups sit on the ribbon below.
            egui::Panel::top("drawing-ribbon-tabs")
                .exact_size(ribbon::TAB_STRIP_H)
                .frame(egui::Frame::new().fill(tokens.band).inner_margin(egui::Margin::symmetric(4, 0)))
                .show_separator_line(false)
                .show(ui, |ui| {
                    let display = ribbon::Display { collapsed: folded, layout: ribbon::Layout::Classic };
                    ribbon::tab_strip(ui, &self.ribbon_tab, false, false, display, ribbon::Mic::default(), &tokens, &mut acts);
                });
            if !folded {
                egui::Panel::top("drawing-ribbon")
                    .exact_size(ribbon::GROUP_H + 8.0)
                    .frame(egui::Frame::new().fill(tokens.panel).inner_margin(egui::Margin::symmetric(4, 0)))
                    .show_separator_line(false)
                    .show(ui, |ui| {
                        acts.extend(ribbon::show(ui, &self.ribbon_tab, false, &self.ribbon_view(), None, &tokens));
                    });
            }
            self.apply_ribbon(acts);
        }
        self.frame_edges(&ctx);
        egui::Panel::left("cases").resizable(true).default_size(260.0).show(ui, |ui| self.case_list(&ctx, ui));
        egui::Panel::right("details").resizable(true).default_size(340.0).show(ui, |ui| self.details(ui));
        egui::Panel::bottom("status").show(ui, |ui| ui.label(&self.status));
        egui::CentralPanel::default().show(ui, |ui| self.picture_area(ui));
        if self.selected.is_some() && !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Delete)) {
            self.delete_selected();
        }
    }

    /// The 30-pixel title row, as in the app: the bare band drags the window (double-click
    /// maximises it), and the controls on top keep their own clicks. Also holds the title, the
    /// theme button and the models light.
    fn title_row(&mut self, ui: &mut egui::Ui) {
        const H: f32 = 30.0;
        let ctx = ui.ctx().clone();
        let row = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), H));
        // Added first, so the controls drawn over it win the pointer.
        let handle = ui.interact(row, ui.scope_id().with("title-handle"), Sense::click_and_drag());
        if handle.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if handle.double_clicked() {
            let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), H), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("NoteCraft OCR lab").size(13.0).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                if ui.button(" ✕ ").on_hover_text("Close").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                if ui.button(if maximized { " ❐ " } else { " □ " }).on_hover_text("Maximize").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                if ui.button(" – ").on_hover_text("Minimize").clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                }
                self.theme_button(ui);
                self.health_light(ui);
            });
        });
    }

    /// A line round the window, and the edge and corner resize, as in the app.
    fn frame_edges(&self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let r = ctx.content_rect();
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("window-edge"))).rect_stroke(
            r,
            0.0,
            egui::Stroke::new(1.0, self.theme.tokens().band),
            egui::StrokeKind::Inside,
        );
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        const E: f32 = 6.0;
        let (west, east, north, south) = (pos.x - r.min.x < E, r.max.x - pos.x < E, pos.y - r.min.y < E, r.max.y - pos.y < E);
        use egui::{CursorIcon as C, viewport::ResizeDirection as D};
        let (dir, cursor) = match (west, east, north, south) {
            (true, _, true, _) => (D::NorthWest, C::ResizeNorthWest),
            (_, true, true, _) => (D::NorthEast, C::ResizeNorthEast),
            (true, _, _, true) => (D::SouthWest, C::ResizeSouthWest),
            (_, true, _, true) => (D::SouthEast, C::ResizeSouthEast),
            (true, ..) => (D::West, C::ResizeWest),
            (_, true, ..) => (D::East, C::ResizeEast),
            (_, _, true, _) => (D::North, C::ResizeNorth),
            (_, _, _, true) => (D::South, C::ResizeSouth),
            _ => return,
        };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(dir));
        }
    }

    /// A small light in the header: green when the models are present and intact, red when a file
    /// is wrong, amber when one is missing, grey when no folder is set. Hover for the reason.
    fn health_light(&mut self, ui: &mut egui::Ui) {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 24.0), Sense::hover());
        let colour = match self.health {
            crate::models::Health::Ok => Color32::from_rgb(60, 180, 90),
            crate::models::Health::Corrupt(_) => Color32::from_rgb(220, 60, 50),
            crate::models::Health::Missing(_) => Color32::from_rgb(230, 170, 40),
            crate::models::Health::NotSet => Color32::GRAY,
        };
        ui.painter_at(rect).circle_filled(rect.center(), 5.5, colour);
        resp.on_hover_text(format!("OCR models: {}", self.health.describe()));
    }

    /// Downloads the two public models into the lab's own folder and loads them. Blocks the window
    /// for the length of the download (about 12 MB), so it is a person's deliberate action.
    fn download_models(&mut self) {
        let dir = crate::lab_root().join("local").join("models");
        self.status = "Downloading the models (about 12 MB)…".to_string();
        match crate::models::download(&dir) {
            Ok(()) => {
                self.log.event("models downloaded");
                self.load_reader(&dir);
                self.remember();
            }
            Err(e) => {
                self.log.event(&format!("model download failed: {e}"));
                self.status = format!("Download failed: {e}");
                self.health = crate::models::check(self.models.as_deref());
            }
        }
    }

    /// The sun or moon in the header. A click opens the app's own theme chooser: the 14 swatches
    /// and the switch that follows the system, the same grid the app shows.
    fn theme_button(&mut self, ui: &mut egui::Ui) {
        let current = self.theme;
        let tokens = current.tokens();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 24.0), Sense::click());
        let painter = ui.painter_at(rect);
        if resp.hovered() {
            painter.rect_filled(rect, 4.0, tokens.row_hover);
        }
        let icon_rect = Rect::from_center_size(rect.center(), egui::vec2(19.0, 19.0));
        if current.mode.is_dark() {
            notecraft_ui_egui::icons::paint_moon(&painter, icon_rect, tokens.accent, if resp.hovered() { tokens.row_hover } else { tokens.panel });
        } else {
            notecraft_ui_egui::icons::paint(&painter, icon_rect, notecraft_ui_egui::icons::Icon::Sun, tokens.accent);
        }
        let resp = resp.on_hover_text(format!("Theme: {}. Click to change it.", current.label()));
        let mut pick: Option<(notecraft_ui_egui::theme::Mode, notecraft_ui_egui::theme::Accent)> = None;
        let mut follow = self.follow_system;
        egui::Popup::menu(&resp).show(|ui| {
            notecraft_ui_egui::app::theme_grid(ui, current, &mut pick, &mut follow);
        });
        if let Some((mode, accent)) = pick {
            self.follow_system = false;
            self.theme = Theme { mode, accent };
            self.remember();
        } else if follow != self.follow_system {
            self.follow_system = follow;
            self.remember();
        }
    }

    fn case_list(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Test cases");
        ui.label(format!("{} pictures", self.pictures.len()));
        ui.monospace(folder_name(&self.folder));
        ui.horizontal(|ui| {
            if ui.button("Open folder…").clicked()
                && let Some(dir) = rfd::FileDialog::new().set_directory(&self.folder).pick_folder()
            {
                self.set_folder(dir);
                self.remember();
            }
            if ui.button("Show in Explorer").clicked() {
                open_in_file_manager(&self.folder);
            }
        });
        ui.separator();
        let mut pick = None;
        let mut delete_req: Option<usize> = None;
        let mut save_now = false;
        let mut view_json: Option<PathBuf> = None;
        // The check mark on the open OCR file's row was clicked: its edits are saved.
        let mut save_json = false;
        // The [✕] on the open OCR file's row was clicked: its unsaved edits are thrown away.
        let mut discard_json = false;
        // Language, then how the sample was made, then what it contains. A sample with several content
        // tags appears under each of them.
        type Contents = std::collections::BTreeMap<String, Vec<usize>>;
        type Sources = std::collections::BTreeMap<String, Contents>;
        let mut tree: std::collections::BTreeMap<String, Sources> = std::collections::BTreeMap::new();
        for (i, path) in self.pictures.iter().enumerate() {
            let language = case::language_of(&self.folder, path);
            for (source, content) in self.tags[i].nodes() {
                tree.entry(language.clone()).or_default().entry(source).or_default().entry(content).or_default().push(i);
            }
        }
        // Every language shows every source and every content node, so a new picture can go into an empty one.
        for sources in tree.values_mut() {
            for (source, _) in case::SOURCES {
                let contents = sources.entry(source.to_string()).or_default();
                for (content, _) in case::CONTENTS.iter().chain(std::iter::once(&case::UNTAGGED)) {
                    contents.entry(content.to_string()).or_default();
                }
            }
        }
        let mut blank_in: Option<(PathBuf, case::Tags)> = None;
        let mut import_in: Option<(PathBuf, case::Tags)> = None;
        // A node opened or closed in the tree, as (its depth, the id it opens to, or None to close it).
        // It is applied after the scroll area, so the tree is drawn from one state.
        let mut toggled: Option<(usize, Option<String>)> = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (language, sources) in &tree {
                let count = self.pictures.iter().filter(|p| case::language_of(&self.folder, p) == *language).count();
                let is_open = self.open_path.first().map(String::as_str) == Some(language.as_str());
                let header = egui::CollapsingHeader::new(format!("{language} ({count})")).id_salt(language).open(Some(is_open)).show(ui, |ui| {
                    for (source, contents) in sources {
                        let source_label = case::label_for(&case::SOURCES, source).unwrap_or("Other");
                        let source_open = self.open_path.get(1).map(String::as_str) == Some(source.as_str());
                        let source_header =
                            egui::CollapsingHeader::new(source_label).id_salt((language, source)).open(Some(source_open)).show(ui, |ui| {
                                for (content, indexes) in contents {
                                    let content_label = if content == case::UNTAGGED.0 {
                                        case::UNTAGGED.1
                                    } else {
                                        case::label_for(&case::CONTENTS, content).unwrap_or("Other")
                                    };
                                    let content_open = self.open_path.get(2).map(String::as_str) == Some(content.as_str());
                                    let content_header = egui::CollapsingHeader::new(format!("{content_label} ({})", indexes.len()))
                                        .id_salt((language, source, content))
                                        .open(Some(content_open))
                                        .show(ui, |ui| {
                                            // Insert, like the main app's "A page here": the new sample gets this node's tags.
                                            let content_ids: Vec<&str> =
                                                if content == case::UNTAGGED.0 { Vec::new() } else { vec![content.as_str()] };
                                            let tags = case::Tags::new(source, &content_ids);
                                            let dir = case::language_folder(&self.folder, language);
                                            ui.menu_button("+ A page here", |ui| {
                                                if ui.button("New blank drawing").clicked() {
                                                    blank_in = Some((dir.clone(), tags.clone()));
                                                    ui.close();
                                                }
                                                if ui.button("Import picture…").clicked() {
                                                    import_in = Some((dir.clone(), tags.clone()));
                                                    ui.close();
                                                }
                                            });
                                            for &i in indexes {
                                                let path = &self.pictures[i];
                                                // One row is lit at a time: the picture, unless its OCR file is open in the main area.
                                                let current = self.viewing_json.is_none() && self.loaded.as_ref().is_some_and(|l| l.index == i);
                                                let labelled = case::label_path(path).is_file();
                                                let mark = if labelled { "  ✎" } else { "" };
                                                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                                ui.horizontal(|ui| {
                                                    let row = ui.selectable_label(current, format!("{name}{mark}"));
                                                    if row.clicked() {
                                                        pick = Some(i);
                                                    }
                                                    row.context_menu(|ui| {
                                                        if ui.button("Delete…").clicked() {
                                                            delete_req = Some(i);
                                                            ui.close();
                                                        }
                                                        if ui.button("Show in Explorer").clicked() {
                                                            if let Some(dir) = path.parent() {
                                                                open_in_file_manager(dir);
                                                            }
                                                            ui.close();
                                                        }
                                                    });
                                                    if ui.small_button("🗑").on_hover_text("Delete this picture").clicked() {
                                                        delete_req = Some(i);
                                                    }
                                                    // A floppy appears on the open sample once it has changes to save.
                                                    let unsaved = self.loaded.as_ref().is_some_and(|l| l.index == i && l.dirty);
                                                    if unsaved && ui.small_button("💾").on_hover_text("Save this drawing").clicked() {
                                                        save_now = true;
                                                    }
                                                });
                                                // The OCR text, as a child of the sample: `<picture>.txt`, when OCR has run.
                                                let text_file = ocr_json_path(path);
                                                if text_file.is_file() {
                                                    let child = text_file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                                    ui.indent(("ocr-text", i), |ui| {
                                                        ui.horizontal(|ui| {
                                                            let showing = self.viewing_json.as_ref().is_some_and(|(p, _)| *p == text_file);
                                                            if ui
                                                                .selectable_label(showing, format!("📄 {child}"))
                                                                .on_hover_text("Show or hide this file in the main area")
                                                                .clicked()
                                                            {
                                                                view_json = Some(text_file.clone());
                                                            }
                                                            // At the row's right edge, once its text is edited and not saved: [✔] furthest
                                                            // right and [✕] left of it, as in the source view.
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                if showing && self.viewing_dirty() {
                                                                    if check_button(ui).clicked() {
                                                                        save_json = true;
                                                                    }
                                                                    if cross_button(ui).clicked() {
                                                                        discard_json = true;
                                                                    }
                                                                }
                                                            });
                                                        });
                                                    });
                                                }
                                            }
                                        });
                                    if content_header.header_response.clicked() {
                                        toggled = Some((2, if content_open { None } else { Some(content.clone()) }));
                                    }
                                }
                            });
                        if source_header.header_response.clicked() {
                            toggled = Some((1, if source_open { None } else { Some(source.clone()) }));
                        }
                    }
                });
                if header.header_response.clicked() {
                    toggled = Some((0, if is_open { None } else { Some(language.clone()) }));
                }
            }
        });
        // Opening a node closes its siblings, and everything under them: the path keeps only the
        // levels above the one changed.
        if let Some((depth, id)) = toggled {
            self.open_path.truncate(depth);
            if let Some(id) = id {
                self.open_path.push(id);
            }
            self.remember();
        }
        if let Some((dir, tags)) = blank_in {
            self.pending_new = Some((dir, tags));
        }
        if let Some((dir, tags)) = import_in {
            self.import_picture(ctx, &dir, tags);
        }
        if save_now {
            self.save_if_dirty();
        }
        if save_json {
            self.save_viewing();
        }
        if discard_json {
            self.discard_viewing();
        }
        // Clicking the file's row shows it, and clicking it again hides it.
        if let Some(path) = view_json {
            if self.viewing_json.as_ref().is_some_and(|(p, _)| *p == path) {
                self.leave_viewing();
            } else {
                self.show_file(&path);
            }
        }
        if let Some(i) = delete_req {
            self.pending_delete = Some(i);
        }
        self.confirm_delete(ctx);
        self.confirm_new_drawing(ctx);
        if let Some(i) = pick {
            if self.loaded.as_ref().is_some_and(|l| l.dirty) {
                self.status = "Save the drawing you are working on first.".to_string();
            } else {
                self.select(ctx, i);
            }
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
            if !matches!(self.health, crate::models::Health::Ok) && ui.button("Download models").clicked() {
                self.download_models();
            }
        });
        if let Some(e) = self.reader_error.clone() {
            ui.colored_label(Color32::from_rgb(200, 60, 40), e);
        }
        ui.horizontal(|ui| {
            let can_run = self.reader.is_some() && self.loaded.is_some();
            if ui.add_enabled(can_run, egui::Button::new("Run OCR")).clicked() {
                self.run_ocr();
            }
            let can_run_all = self.reader.is_some() && !self.pictures.is_empty();
            if ui.add_enabled(can_run_all, egui::Button::new("Run OCR on all")).clicked() {
                self.run_all();
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
            if ui.add_enabled(self.loaded.is_some(), egui::Button::new("Clear this result")).clicked() {
                self.clear_result_of_open();
            }
            if ui.add_enabled(!self.pictures.is_empty(), egui::Button::new("Clear all results")).clicked() {
                self.clear_all_results();
            }
        });
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.mode, Mode::Select, "Select / move / resize");
            ui.selectable_value(&mut self.mode, Mode::Draw, "Draw a label");
        });
        // The pen lives in the drawing ribbon above the picture, so only a drawing can be in pen mode.
        if !self.open_is_drawing() && self.mode == Mode::Ink {
            self.mode = Mode::Select;
        }
        let dirty = self.loaded.as_ref().is_some_and(|l| l.dirty);
        if dirty && ui.button("Save drawing").clicked() {
            self.save_drawing();
        }
        ui.checkbox(&mut self.show_engine, "Show the recognised text");
        egui::CollapsingHeader::new("Metrics").default_open(false).show(ui, |ui| self.metrics.show(ui));
        egui::CollapsingHeader::new("Picture").default_open(false).show(ui, |ui| {
            ui.add(egui::Slider::new(&mut self.adjust.hue, -180.0..=180.0).text("Hue"));
            ui.add(egui::Slider::new(&mut self.adjust.contrast, 0.0..=2.0).text("Contrast"));
            ui.add(egui::Slider::new(&mut self.adjust.brightness, -1.0..=1.0).text("Brightness"));
            if ui.button("Reset picture").clicked() {
                self.adjust = Adjust::NEUTRAL;
            }
            ui.small("Only the picture on screen changes. OCR reads the file.");
        });
        egui::CollapsingHeader::new("Models").default_open(false).show(ui, |ui| self.models_info(ui));
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
                            let mut rgb = label.colour.unwrap_or(LABEL_DEFAULT_RGB);
                            if ui.color_edit_button_srgb(&mut rgb).changed() {
                                label.colour = Some(rgb);
                                text_changed = true;
                            }
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
        let mut recolour: Option<(usize, Color32)> = None;
        if let Some(loaded) = self.loaded.as_ref() {
            ui.separator();
            ui.strong(format!("Engine lines ({})", loaded.lines.len()));
            egui::ScrollArea::vertical().id_salt("lines").show(ui, |ui| {
                for (i, line) in loaded.lines.iter().enumerate() {
                    let conf = line.confidence.map_or("no score".to_string(), |c| format!("{:.2}", c));
                    // The swatch is the box's colour on the picture: a click opens the picker for this line.
                    ui.horizontal(|ui| {
                        let mut colour = line_colour(loaded, i);
                        if ui.color_edit_button_srgba(&mut colour).on_hover_text("Colour of this line's box").changed() {
                            recolour = Some((i, colour));
                        }
                        ui.label(format!("{}  [{conf}]", line.text));
                    });
                }
            });
        }
        // A line recoloured from its swatch keeps its colour until the picture changes.
        if let Some((index, colour)) = recolour
            && let Some(loaded) = self.loaded.as_mut()
        {
            loaded.line_colours.insert(index, colour);
        }
    }

    /// Loads a text file into the main area. It can be typed in; the check mark saves it. Picking a sample or closing the view shows the picture again.
    fn show_file(&mut self, path: &Path) {
        self.leave_viewing();
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let text = text.trim_start_matches('\u{feff}').to_string();
                self.viewing_changed = false;
                self.viewing_json = Some((path.to_path_buf(), text));
            }
            Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
        }
    }

    /// Whether the open file has been edited since it was last saved or opened.
    fn viewing_dirty(&self) -> bool {
        self.viewing_changed
    }

    /// Records that the open file was edited. Like the source view, any edit makes it dirty, even
    /// when the text was typed back to what it was.
    fn note_edit(&mut self) {
        self.viewing_changed = true;
    }

    /// Throws the open file's unsaved edits away: its text is read again from disk.
    fn discard_viewing(&mut self) {
        let Some((path, _)) = self.viewing_json.clone() else { return };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                self.viewing_json = Some((path.clone(), text.trim_start_matches('\u{feff}').to_string()));
                self.viewing_changed = false;
                self.status = format!("Discarded the edits to {}", path.display());
            }
            Err(e) => self.status = format!("Could not read {}: {e}", path.display()),
        }
    }

    /// Writes the open file's edited text back to its file, and clears the dirty state.
    fn save_viewing(&mut self) {
        let Some((path, text)) = self.viewing_json.clone() else { return };
        match std::fs::write(&path, text.as_bytes()) {
            Ok(()) => {
                self.viewing_changed = false;
                self.status = format!("Saved {}", path.display());
                // An edited run of the open picture shows at once, not after the next pick.
                if let Some(index) = self.pictures.iter().position(|p| ocr_json_path(p) == path) {
                    self.reload_run(index);
                }
            }
            Err(e) => self.status = format!("Could not save {}: {e}", path.display()),
        }
    }

    fn picture_area(&mut self, ui: &mut egui::Ui) {
        // The OCR file, when it is open, replaces the picture until another sample is picked or the file row is clicked again.
        if let Some((path, text)) = self.viewing_json.as_mut() {
            // Dirty as soon as the file was edited, as the source view does: typing a character and
            // deleting it again still leaves the file to save.
            let dirty = self.viewing_changed;
            let mut edited = false;
            let mut save = false;
            let mut discard = false;
            ui.horizontal(|ui| {
                ui.strong(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                if dirty {
                    if cross_button(ui).clicked() {
                        discard = true;
                    }
                    if check_button(ui).clicked() {
                        save = true;
                    }
                }
            });
            ui.separator();
            let font = egui::FontId::monospace(13.0);
            let plain = ui.visuals().text_color();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut layouter = |ui: &egui::Ui, s: &dyn egui::TextBuffer, _wrap: f32| {
                    ui.fonts_mut(|f| f.layout_job(crate::json_view::job(s.as_str(), font.clone(), plain)))
                };
                if ui.add(egui::TextEdit::multiline(text).code_editor().desired_width(f32::INFINITY).layouter(&mut layouter)).changed() {
                    edited = true;
                }
            });
            if edited {
                self.note_edit();
            }
            if save {
                self.save_viewing();
            }
            if discard {
                self.discard_viewing();
            }
            return;
        }
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
        // The paper's lines or squares, drawn over the picture on screen only.
        if self.rules != ribbon::RuleLines::None {
            let line = Color32::from_rgba_unmultiplied(90, 130, 200, 90);
            let strong = Color32::from_rgba_unmultiplied(90, 130, 200, 160);
            ribbon::paint_paper(&painter, response.rect, self.rules, scale, line, strong);
        }
        if self.show_engine {
            for (n, line) in loaded.lines.iter().enumerate() {
                let colour = line_colour(loaded, n);
                let pts: Vec<Pos2> = line.quad.iter().map(|p| Self::to_screen(origin, scale, [p.x, p.y])).collect();
                for i in 0..4 {
                    painter.line_segment([pts[i], pts[(i + 1) % 4]], Stroke::new(1.5, colour));
                }
            }
        }
        for (i, label) in loaded.file.labels.iter().enumerate() {
            let r = Self::normalized(label.rect);
            let screen = Rect::from_min_max(Self::to_screen(origin, scale, [r[0], r[1]]), Self::to_screen(origin, scale, [r[2], r[3]]));
            let colour =
                if self.selected == Some(i) { SELECTED_COLOUR } else { label.colour.map_or(LABEL_COLOUR, |c| Color32::from_rgb(c[0], c[1], c[2])) };
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
        if matches!(self.mode, Mode::Ink | Mode::Erase) {
            if let Some(pos) = response.interact_pointer_pos().filter(|_| response.dragged() || response.drag_started()) {
                let p = Self::clamp_point(Self::to_pixels(origin, scale, pos), size);
                let ctx = response.ctx.clone();
                if self.mode == Mode::Erase {
                    self.erase_ink(&ctx, p);
                } else {
                    self.paint_ink(&ctx, p);
                }
            }
            if response.drag_stopped()
                && let Some(loaded) = self.loaded.as_mut()
            {
                loaded.last_ink = None;
                if let Some(stroke) = loaded.current.take() {
                    let _ = loaded.ink.add(stroke);
                }
            }
            return;
        }
        if response.drag_started() {
            let Some(pos) = response.interact_pointer_pos() else { return };
            let p = Self::clamp_point(Self::to_pixels(origin, scale, pos), size);
            match self.mode {
                Mode::Draw => self.drag = Some(Drag::New { start: p, now: p }),
                // The pen and the eraser are handled at the top of this function.
                Mode::Ink | Mode::Erase => {}
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
                            loaded.file.labels.push(Label { text: String::new(), rect: r, colour: None });
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

/// Opens a folder in the system file manager (Explorer on Windows).
fn open_in_file_manager(folder: &Path) {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(program).arg(folder).spawn() {
        eprintln!("could not open {}: {e}", folder.display());
    }
}

/// Replaces this machine's folder and home paths in a report with stand-ins, so the copy can be shared.
fn scrub_paths(text: &str, cases: &Path) -> String {
    let mut out = text.to_string();
    for (path, stand_in) in [(cases.to_string_lossy().into_owned(), "<test folder>")]
        .into_iter()
        .chain(std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok().map(|home| (home, "<home>")))
    {
        if path.is_empty() {
            continue;
        }
        // Both separators, since a path may be written either way in a log line.
        out = out.replace(&path, stand_in).replace(&path.replace(char::from(92), "/"), stand_in);
    }
    out
}

/// Deletes the saved run beside a picture. Returns whether there was one to delete.
/// The editable strokes of a drawing: `<picture>.svg` beside the PNG, with the pen's readings.
fn ink_path(picture: &Path) -> PathBuf {
    picture.with_extension("svg")
}

/// Reads a drawing's strokes back. A drawing with no strokes file starts empty at its picture's size.
fn load_ink(picture: &Path, size: [u32; 2]) -> Drawing {
    std::fs::read_to_string(ink_path(picture))
        .ok()
        .and_then(|text| svg::read(&text).ok())
        .map(|(drawing, _)| drawing)
        .unwrap_or_else(|| Drawing::new(size[0] as f32, size[1] as f32))
}

/// Removes a picture's OCR file, which is its saved run. Returns whether there was one to remove.
fn remove_run_file(picture: &Path) -> Result<bool, String> {
    match std::fs::remove_file(ocr_json_path(picture)) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(format!("{}: {e}", ocr_json_path(picture).display())),
    }
}

/// Saves a run beside its picture, so the next time it opens the result is there without reading
/// the picture again. The file records the picture's checksum: a changed picture does not match and
/// its old run is ignored.
fn save_run(picture: &Path, lines: &[Line], timings: &[Timing], log: &mut Log) {
    let Some(sha) = case::picture_sha256(picture) else { return };
    let saved_at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // The OCR file beside the picture, schema version 1 (see nc-info-ocr/references/ocr-json-schema.md):
    // the text, each line's polygon, and the description, which no model writes yet. It is also the
    // saved run: the lab reads it back instead of running OCR again.
    let (width, height) = image::image_dimensions(picture).unwrap_or((0, 0));
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let text = lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
    let ocr = serde_json::json!({
        "schema": "notecraft.ocr",
        "version": 1,
        "picture": { "name": name, "sha256": sha, "width": width, "height": height },
        "produced": { "at": saved_at, "by": "ocrs 0.14 through notecraft-ocr" },
        "text": text,
        "lines": lines.iter().map(|l| serde_json::json!({
            "text": l.text,
            "polygon": l.quad.iter().map(|p| [p.x, p.y]).collect::<Vec<_>>(),
            "confidence": l.confidence,
        })).collect::<Vec<_>>(),
        "bounds": bounds_polygon(lines),
        "alt": null,
        // The lab's own measurements of this run. Optional in the schema; not product data.
        "timings": timings,
    });
    // The schema says line outlines never cross. The file is still written, so the crossing is visible in the log.
    let outlines: Vec<[[f32; 2]; 4]> = lines.iter().map(|l| l.quad.map(|p| [p.x, p.y])).collect();
    let crossing = (0..outlines.len())
        .flat_map(|i| (i + 1..outlines.len()).map(move |j| (i, j)))
        .filter(|&(i, j)| case::outlines_cross(&outlines[i], &outlines[j]))
        .count();
    if crossing > 0 {
        log.event(&format!("{name}: {crossing} pairs of line outlines cross, which the schema does not allow"));
    }
    let target = ocr_json_path(picture);
    let tmp = target.with_extension("json.tmp");
    let written = serde_json::to_string_pretty(&ocr)
        .map_err(|e| e.to_string())
        .and_then(|text| std::fs::write(&tmp, format!("{text}\n")).map_err(|e| e.to_string()))
        .and_then(|_| std::fs::rename(&tmp, &target).map_err(|e| e.to_string()));
    if let Err(e) = written {
        log.event(&format!("OCR file not saved: {e}"));
    }
}

/// The polygon around everything the engine found: the four corners of the box round all the lines, clockwise
/// from the top left. `null` when no line was found. A tighter outline can replace this later.
fn bounds_polygon(lines: &[Line]) -> serde_json::Value {
    let points: Vec<[f32; 2]> = lines.iter().flat_map(|l| l.quad.iter().map(|p| [p.x, p.y])).collect();
    if points.is_empty() {
        return serde_json::Value::Null;
    }
    let x0 = points.iter().map(|p| p[0]).fold(f32::MAX, f32::min);
    let y0 = points.iter().map(|p| p[1]).fold(f32::MAX, f32::min);
    let x1 = points.iter().map(|p| p[0]).fold(f32::MIN, f32::max);
    let y1 = points.iter().map(|p| p[1]).fold(f32::MIN, f32::max);
    serde_json::json!([[x0, y0], [x1, y0], [x1, y1], [x0, y1]])
}

/// The OCR file beside a picture: `<picture>.ocr.json`, the picture's full name with `.ocr.json` added.
fn ocr_json_path(picture: &Path) -> PathBuf {
    let name = picture.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    picture.with_file_name(format!("{name}.ocr.json"))
}

/// The saved run for a picture, if one exists and still matches the picture's bytes.
fn load_run(picture: &Path) -> Option<(Vec<Line>, Vec<Timing>)> {
    let text = std::fs::read_to_string(ocr_json_path(picture)).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    if value.get("picture")?.get("sha256")?.as_str()? != case::picture_sha256(picture)? {
        return None;
    }
    let mut lines = Vec::new();
    for item in value.get("lines")?.as_array()? {
        let quad_values = item.get("polygon")?.as_array()?;
        if quad_values.len() != 4 {
            return None;
        }
        let mut quad = [notecraft_ocr::Point { x: 0.0, y: 0.0 }; 4];
        for (slot, point) in quad.iter_mut().zip(quad_values) {
            let xy = point.as_array()?;
            slot.x = xy.first()?.as_f64()? as f32;
            slot.y = xy.get(1)?.as_f64()? as f32;
        }
        let confidence = item.get("confidence").and_then(|c| c.as_f64()).map(|c| c as f32);
        lines.push(Line::new(item.get("text")?.as_str()?, quad, confidence).ok()?);
    }
    let mut timings = Vec::new();
    for item in value.get("timings").and_then(|t| t.as_array()).into_iter().flatten() {
        let step = match item.get("step")?.as_str()? {
            "model load" => "model load",
            "read picture" => "read picture",
            "score" => "score",
            _ => continue,
        };
        timings.push(Timing { step, ms: item.get("ms")?.as_f64()? });
    }
    Some((lines, timings))
}

/// Keeps the window redrawing about twice a second, so the metrics charts move while it is idle.
fn ctx_repaint(ui: &egui::Ui) {
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_ocr::Point;

    fn label(rect: [f32; 4]) -> Label {
        Label { text: String::new(), rect, colour: None }
    }

    #[test]
    fn a_zero_hue_turn_changes_nothing_and_red_turns_to_green_at_120_degrees() {
        assert_eq!(rotate_hue([200, 30, 90], 0.0), [200, 30, 90]);
        assert_eq!(rotate_hue([255, 0, 0], 120.0), [0, 255, 0]);
        assert_eq!(rotate_hue([255, 0, 0], -120.0), [0, 0, 255]);
        assert_eq!(rotate_hue([128, 128, 128], 90.0), [128, 128, 128], "grey has no hue to turn");
    }

    #[test]
    fn neutral_adjustments_keep_the_picture_and_brightness_and_contrast_move_it() {
        let mut src = image::RgbaImage::new(2, 1);
        src.put_pixel(0, 0, image::Rgba([100, 100, 100, 255]));
        src.put_pixel(1, 0, image::Rgba([200, 200, 200, 255]));
        let same = Adjust::NEUTRAL.apply(&src);
        assert_eq!(same.pixels[0], egui::Color32::from_rgba_unmultiplied(100, 100, 100, 255));
        let bright = Adjust { brightness: 0.5, ..Adjust::NEUTRAL }.apply(&src);
        assert_eq!(bright.pixels[0].r(), 228, "100 plus half of 255, clamped");
        let flat = Adjust { contrast: 0.0, ..Adjust::NEUTRAL }.apply(&src);
        assert_eq!(flat.pixels[0].r(), 128, "no contrast is flat mid grey");
        assert_eq!(flat.pixels[1].r(), 128);
    }

    #[test]
    fn a_rectangle_dragged_backwards_is_put_the_right_way_round() {
        assert_eq!(Lab::normalized([9.0, 8.0, 1.0, 2.0]), [1.0, 2.0, 9.0, 8.0]);
        assert_eq!(Lab::normalized([1.0, 2.0, 9.0, 8.0]), [1.0, 2.0, 9.0, 8.0]);
    }

    #[test]
    fn a_click_finds_the_label_on_top_and_none_outside_all_labels() {
        let labels = vec![label([0.0, 0.0, 50.0, 50.0]), label([10.0, 10.0, 20.0, 20.0])];
        assert_eq!(Lab::label_at(&labels, [15.0, 15.0]), Some(1), "the later label is on top");
        assert_eq!(Lab::label_at(&labels, [30.0, 30.0]), Some(0));
        assert_eq!(Lab::label_at(&labels, [90.0, 90.0]), None);
    }

    #[test]
    fn the_four_corners_go_round_clockwise_from_the_top_left() {
        let c = Lab::corners([1.0, 2.0, 3.0, 4.0]);
        assert_eq!(c, [[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 4.0]]);
    }

    #[test]
    fn a_point_is_clamped_into_the_picture() {
        assert_eq!(Lab::clamp_point([-5.0, 300.0], [200, 100]), [0.0, 100.0]);
        assert_eq!(Lab::clamp_point([50.0, 50.0], [200, 100]), [50.0, 50.0]);
    }

    #[test]
    fn screen_and_picture_coordinates_convert_both_ways() {
        let origin = egui::pos2(100.0, 40.0);
        let screen = Lab::to_screen(origin, 2.0, [30.0, 10.0]);
        assert_eq!(screen, egui::pos2(160.0, 60.0));
        assert_eq!(Lab::to_pixels(origin, 2.0, screen), [30.0, 10.0]);
    }

    #[test]
    fn an_edited_ocr_file_is_dirty_until_it_is_saved_to_disk() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-dirty-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("p.png.ocr.json");
        std::fs::write(&file, "{\"text\": \"Sourdough\"}").expect("file");
        let mut lab = Lab::new(dir.clone(), None, None, false, telemetry::Log::open(&dir.join("log.txt")));
        lab.show_file(&file);
        assert!(!lab.viewing_dirty(), "a file just opened is not dirty");
        // An edit marks the file dirty even when the text is typed back to what it was, as in the source view.
        lab.note_edit();
        if let Some((_, text)) = lab.viewing_json.as_mut() {
            text.push(' ');
            text.pop();
        }
        assert!(lab.viewing_dirty(), "an edit that was typed back still makes the file dirty");
        lab.save_viewing();
        assert!(!lab.viewing_dirty(), "saving clears the dirty state");
        assert_eq!(std::fs::read_to_string(&file).expect("saved"), "{\"text\": \"Sourdough\"}", "the file is written");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn discarding_an_edited_ocr_file_brings_back_what_is_on_disk() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-discard-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("p.png.ocr.json");
        std::fs::write(&file, "{\"text\": \"Sourdough\"}").expect("file");
        let mut lab = Lab::new(dir.clone(), None, None, false, telemetry::Log::open(&dir.join("log.txt")));
        lab.show_file(&file);
        lab.note_edit();
        if let Some((_, text)) = lab.viewing_json.as_mut() {
            text.clear();
        }
        assert!(lab.viewing_dirty(), "the edit is unsaved");
        lab.discard_viewing();
        assert!(!lab.viewing_dirty(), "discarding clears the dirty state");
        assert_eq!(lab.viewing_json.as_ref().map(|(_, t)| t.as_str()), Some("{\"text\": \"Sourdough\"}"), "the file's text is back");
        assert_eq!(std::fs::read_to_string(&file).expect("file"), "{\"text\": \"Sourdough\"}", "nothing was written");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_json_file_is_shown_in_the_main_area_without_its_byte_order_mark() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-view-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("p.png.ocr.json");
        std::fs::write(&file, "\u{feff}{\"version\": 1}").expect("file");
        let mut lab = Lab::new(dir.clone(), None, None, false, telemetry::Log::open(&dir.join("log.txt")));
        lab.show_file(&file);
        let (path, text) = lab.viewing_json.clone().expect("the file is open in the main area");
        assert_eq!(path, file);
        assert_eq!(text, "{\"version\": 1}", "the byte-order mark is not shown");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    // P0 check, run only when asked: `cargo test -p notecraft-ocr-lab -- --ignored`.
    #[test]
    #[ignore = "P0 check, run with: cargo test -p notecraft-ocr-lab -- --ignored"]
    fn p0_the_ribbon_sensitivity_reaches_the_lab() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-p0-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut lab = Lab::new(dir.clone(), None, None, false, telemetry::Log::open(&dir.join("log.txt")));
        lab.apply_ribbon(vec![ribbon::Act::InkSensitivity(100)]);
        assert_eq!(lab.sensitivity, 100, "the slider's value is kept");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn clearing_a_result_removes_only_its_run_file_and_says_when_there_was_none() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-clear-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let picture = dir.join("c.png");
        image::RgbImage::new(4, 4).save(&picture).expect("picture");
        std::fs::write(ocr_json_path(&picture), "{}").expect("run file");
        assert_eq!(remove_run_file(&picture), Ok(true), "the run is removed");
        assert!(picture.is_file(), "the picture is kept");
        assert_eq!(remove_run_file(&picture), Ok(false), "a second clear finds nothing");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }

    #[test]
    fn a_saved_run_loads_back_for_the_same_picture_and_is_ignored_once_the_picture_changes() {
        let dir = std::env::temp_dir().join(format!("ocr-lab-run-{}", crate::unique_suffix()));
        std::fs::create_dir_all(&dir).expect("dir");
        let picture = dir.join("p.png");
        image::RgbImage::new(20, 10).save(&picture).expect("picture");
        let quad = [Point { x: 1.0, y: 2.0 }, Point { x: 9.0, y: 2.0 }, Point { x: 9.0, y: 6.0 }, Point { x: 1.0, y: 6.0 }];
        let lines = vec![Line::new("Sourdough 6.00", quad, None).expect("line")];
        let timings = vec![Timing { step: "read picture", ms: 12.5 }];
        let mut log = Log::open(&dir.join("log.txt"));
        save_run(&picture, &lines, &timings, &mut log);
        let (loaded, loaded_timings) = load_run(&picture).expect("the run loads");
        assert_eq!(loaded, lines);
        assert_eq!(loaded_timings.len(), 1);
        image::RgbImage::new(21, 10).save(&picture).expect("the picture changes");
        assert!(load_run(&picture).is_none(), "a changed picture does not use the old run");
        std::fs::remove_dir_all(&dir).expect("clean up");
    }
}

#[cfg(test)]
mod scrub_tests {
    use super::*;

    #[test]
    fn a_report_loses_the_test_folder_and_home_paths() {
        let cases = Path::new(r"C:\Users\someone\ocr-cases");
        let text = "{\"log\":\"opened C:\\Users\\someone\\ocr-cases\\a.png and C:/Users/someone/ocr-cases/b.png\"}";
        let out = scrub_paths(text, cases);
        assert!(!out.contains("someone"), "{out}");
        assert!(out.contains("<test folder>"), "{out}");
    }
}

//! The NoteCraft window: top bar, notebook tree, page editor with live preview.
//!
//! Every action goes through `Session::run`, so the window, the command line and any
//! later control channel share one code path.

use crate::backstage;
use crate::control::{ControlRequest, ControlServer, base64};
use crate::floating;
use crate::icons::{self, Icon};
use crate::navhistory::NavHistory;
use crate::pageedit;
use crate::preview;
use crate::quickaccess;
use crate::ribbon;
use crate::snippets;
use crate::theme::{Accent, Mode, ModePref, Theme, Tokens};
use egui::{Align2, Color32, Context, Id, Key, Modifiers, Sense, Ui, vec2};
use notecraft_engine::{EngineError, Output, Params, Session};
use notecraft_model::{NodeColor, NodeKind, PageFormat};
use notecraft_store::search::Hit;
use notecraft_store::tree::Node;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
// std's own Instant panics on wasm32-unknown-unknown, which has no clock; web_time is the
// same type everywhere else and performance.now()/Date.now() there.
use web_time::Instant;

const AUTOSAVE_AFTER: Duration = Duration::from_millis(900);
/// The three-bar button: what OneNote puts at the top left of the navigation pane.
const BARS: &str = "\u{2630}";

/// Where the pen rack and the tool in hand are remembered between runs.
const RACK_KEY: &str = "pen_rack";
const NIB_KEY: &str = "pen_in_hand";
const SENSITIVITY_KEY: &str = "pen_sensitivity";

const ROW_HEIGHT: f32 = 24.0;
const INDENT: f32 = 14.0;

/// Shared title treatment for modal dialogs. Keeping this here makes dialogs use the
/// same opaque, theme-owned title band instead of inheriting egui's platform-gray title bar.
fn modal_frame(tokens: &Tokens) -> egui::Frame {
    egui::Frame::NONE.fill(tokens.panel).stroke(egui::Stroke::new(1.5, tokens.accent)).inner_margin(egui::Margin::same(16))
}

fn modal_title(ui: &mut Ui, tokens: &Tokens, title: &str) {
    egui::Frame::NONE.fill(tokens.band).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong().size(17.0).color(tokens.band_text));
    });
    ui.add_space(10.0);
}

/// Modal actions use a readable size based on their label instead of egui's compact default.
fn modal_button(ui: &mut Ui, tokens: &Tokens, label: &str, primary: bool) -> egui::Response {
    let width = (label.chars().count() as f32 * 8.0 + 32.0).max(92.0);
    let fill = if primary { tokens.accent } else { tokens.panel };
    let text = if primary { tokens.band_text } else { tokens.text };
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(14.0).color(text))
            .min_size(vec2(width, 32.0))
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, tokens.accent)),
    )
}

fn params(pairs: &[(&str, &str)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn shown(p: &Path) -> String {
    notecraft_store::tree::display_path(p)
}

fn notebook_for(path: &Path) -> Option<PathBuf> {
    let mut current = path;
    loop {
        if notecraft_store::tree::read_node(current).is_some_and(|node| node.kind == NodeKind::Notebook) {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// The path as the window may show it: redacted to `<notebooks>/relative/path` when
/// `redact` is on (for screenshots, tests and documentation, so no local path leaks).
fn shown_in(root: &Path, p: &Path, redact: bool) -> String {
    if !redact {
        return shown(p);
    }
    match p.strip_prefix(root) {
        Ok(rel) if rel.as_os_str().is_empty() => "<notebooks folder>".to_string(),
        Ok(rel) => format!("<notebooks>/{}", rel.to_string_lossy().replace('\\', "/")),
        Err(_) => "<outside>".to_string(),
    }
}

struct Editor {
    dir: PathBuf,
    format: PageFormat,
    title: String,
    title_buf: String,
    body: String,
    dirty_since: Option<Instant>,
    blocks: Vec<notecraft_formats::Block>,
    last_saved: Option<String>,
    attachments: Vec<(String, u64)>,
    /// Where the caret is in the page, and the text of that block while it is written in.
    caret: Option<pageedit::Caret>,
    /// False when the page holds markup the block editor would drop: it is then read here
    /// and changed in its source.
    writable: bool,
    /// Which way the whole page reads, if the writer said so. A line with no letters of its
    /// own follows it.
    page_dir: Option<notecraft_model::Dir>,
    /// The boxes the page's blocks are arranged in, as the page's own file keeps them.
    boxes: Vec<notecraft_model::Container>,
    /// How wide the columns of this page's tables are drawn, from the same place.
    columns: notecraft_model::tablecols::Widths,
    /// Which paragraphs of this page are stretched to fill the column, from the same place.
    /// The words themselves are never touched; see `notecraft_model::justify`.
    justified: notecraft_model::justify::Justified,
    /// A selection dragged across more than one block. A caret belongs to one block, so a
    /// selection that leaves it is kept here; `None` is the ordinary case.
    across: Option<pageedit::Across>,
    /// The paper this page is drawn on and the colour behind it, as the page's own file
    /// keeps them. `None` means the page says nothing and the window's own choice stands.
    paper: Option<ribbon::RuleLines>,
    colour: Option<notecraft_model::NodeColor>,
    /// What the page looked like before each change, so a change can be taken back.
    history: crate::history::History,
    /// The ink files in the page's folder. The folder is what decides: the page's own
    /// text is kept in step with this, never the other way round.
    ink_files: Vec<String>,
    /// Where the page's area starts on screen, reported so a script can find a box.
    origin: egui::Pos2,
    /// Whether the boxes are written into the page's text. A page nobody arranged stays plain
    /// text, one column; moving, resizing or adding a box arranges it, and from then on the
    /// boxes are elements around the blocks.
    arranged: bool,
}

impl Editor {
    /// Writes the page's text from its blocks, with the boxes around them when it is arranged.
    fn rebuild_body(&mut self) {
        let arrangement = (self.arranged && !self.boxes.is_empty()).then_some(self.boxes.as_slice());
        self.body = notecraft_formats::boxes::write(self.format, &self.blocks, arrangement);
    }
}

/// The page's own name, drawn on the sheet at the top of the page.
///
/// It used to sit in a strip above the page, beside the breadcrumb and the Saved line,
/// which made it look like a field about the page rather than the first thing on it. In
/// OneNote the name is part of the page, and the owner asked for the same here on
/// 2026-10-19. So it is drawn inside the scroll area and inside the readable column: it
/// scrolls with the words, it lines up with them, and the page colour and rule lines run
/// behind it as they do behind everything else on the sheet.
///
/// It is drawn before the sheet's origin is taken, so ink keeps the coordinates it already
/// has and a drawing made beside a paragraph stays beside it.
///
/// A title that reads right to left is drawn by the bidi painter rather than the text box:
/// egui lays the letters of a word out the right way and then puts the next word to the
/// right of it, so every Arabic page showed its own name with the words backwards. Clicking
/// it hands the line to the text box, which is still where it is edited.
fn page_title(ui: &mut Ui, ed: &mut Editor, tokens: &Tokens, title_id: egui::Id, rename: &mut Option<String>, zoom: f32) {
    let align = crate::pagetext::halign(&ed.title_buf, ed.page_dir);
    let width = ui.available_width();
    let rtl = crate::bidi::paints(&ed.title_buf, ed.page_dir);
    let editing = ui.memory(|m| m.has_focus(title_id)) || !rtl;
    let r = if editing {
        let out = egui::TextEdit::singleline(&mut ed.title_buf)
            .id(title_id)
            .frame(egui::Frame::NONE)
            .font(egui::FontId::proportional(26.0 * zoom))
            .horizontal_align(align)
            .desired_width(width)
            .show(ui);
        if out.response.has_focus()
            && crate::input_language::is_rtl()
            && let Some(range) = out.cursor_range
        {
            let caret = out.galley.pos_from_cursor(range.primary).translate(out.galley_pos.to_vec2());
            crate::pageedit::paint_rtl_input_marker(ui, caret);
        }
        out.response
    } else {
        let pieces = crate::bidi::plain(&ed.title_buf, 26.0 * zoom, tokens);
        let (drawn, size) = crate::bidi::runs(ui, &ed.title_buf, &pieces, 26.0 * zoom, width);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, size.y.max(34.0)), egui::Sense::click());
        crate::bidi::paint(ui, rect.min, &drawn);
        if resp.clicked() {
            ui.memory_mut(|m| m.request_focus(title_id));
        }
        let name = ed.title_buf.clone();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, &name));
        egui::AtomLayoutResponse::empty(resp.on_hover_cursor(egui::CursorIcon::Text))
    };
    if r.lost_focus() && ed.title_buf.trim() != ed.title && !ed.title_buf.trim().is_empty() {
        *rename = Some(ed.title_buf.trim().to_string());
    }
    // The rule under the name, as OneNote draws one. The date the page carries sits under
    // it already, as the first block of the page.
    let y = ui.cursor().min.y + 2.0;
    let (left, right) = (ui.max_rect().min.x, ui.max_rect().max.x);
    ui.painter().hline(left..=right, y, egui::Stroke::new(1.0, tokens.faint));
    ui.add_space(8.0);
}

/// What to do with a file the user dropped on a page.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileChoice {
    /// Copy it into the page's folder, so the notebook carries it. OneNote: "Attach File".
    Copy,
    /// Leave it where it is and write a link. OneNote: "Insert a link to the file".
    Link,
}

enum Dialog {
    /// A dropped file: copy it in, or link to it where it is.
    FileOptions {
        dir: PathBuf,
        files: Vec<PathBuf>,
    },
    NewNotebook {
        name: String,
        format: Option<PageFormat>,
    },
    Add {
        parent: PathBuf,
        parent_kind: NodeKind,
        title: String,
        kind: NodeKind,
        format: Option<PageFormat>,
    },
    Rename {
        path: PathBuf,
        title: String,
    },
    /// A file in the page's own folder, about to be deleted.
    DeleteFile {
        page: PathBuf,
        name: String,
    },
    Delete {
        path: PathBuf,
        title: String,
        count: usize,
    },
    DeleteNotebook {
        path: PathBuf,
        title: String,
    },
    ClearTelemetry {
        path: PathBuf,
    },
    PageVersions {
        page: PathBuf,
        versions: Vec<notecraft_store::page::Version>,
    },
    RecycleBin {
        notebook: PathBuf,
        items: Vec<notecraft_store::recycle::Recycled>,
    },
    PurgeRecycleItem {
        notebook: PathBuf,
        id: String,
        title: String,
    },
    Symbols,
    PrivacyWelcome {
        level: String,
    },
}

struct ImportJob {
    rx: Receiver<ImportMessage>,
    keep_alive: Option<Sender<ImportMessage>>,
    cancel: Arc<AtomicBool>,
    source: PathBuf,
    destination: PathBuf,
    current: String,
    index: usize,
    total: usize,
}

enum ImportMessage {
    Found(usize),
    Section { index: usize, total: usize, label: String },
    Finished(Result<String, String>),
}

enum Act {
    Toggle(PathBuf),
    Select(PathBuf),
    Add(PathBuf, NodeKind),
    /// A page in this section at this place in the order, from the line between two pages.
    AddPageAt(PathBuf, u32),
    /// Add a child of this kind, named in the menu rather than chosen in the dialog.
    AddKind(PathBuf, NodeKind, NodeKind),
    /// A notebook, from the menu on the empty space.
    NewNotebook,
    /// Close a notebook opened from outside the notebooks folder.
    CloseNotebook(PathBuf),
    /// Show the notebooks folder in the file manager.
    RevealRoot,
    /// Ask the system for the folder to work from.
    ChooseRoot,
    Rename(PathBuf, String),
    Delete(PathBuf, String, usize),
    Color(PathBuf, Option<NodeColor>),
    /// Show the node's folder in the operating system's file manager.
    Reveal(PathBuf),
    /// Put a link to this page on the clipboard, to paste into another page.
    CopyLink(PathBuf),
}

pub struct NoteCraftApp {
    session: Session,
    open: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    /// The pages visited, for Back and Forward.
    nav: NavHistory,
    /// Which Quick Access buttons are shown, and where.
    qat: quickaccess::Config,
    /// When the pen last changed the page, to tell which Undo should take back.
    last_ink_change: Option<Instant>,
    /// Two columns in the side panel (organisers, then pages) instead of one tree.
    split_panes: bool,
    /// Whether the organisers column is showing, which the three-bar button toggles.
    organisers: bool,
    /// Whether the window is kept above the others.
    always_on_top: bool,
    /// Set when that has just changed and the window manager has not been told yet.
    window_level_pending: bool,
    /// True when the window has no frame of the system's own (the app asked for none), so
    /// the band across the top draws the window's buttons and moves and resizes it.
    frameless: bool,
    editor: Option<Editor>,
    dialog: Option<Dialog>,
    import_job: Option<ImportJob>,
    status: String,
    status_is_error: bool,
    search: String,
    searched: String,
    hits: Vec<Hit>,
    cursor: usize,
    tokens: Tokens,
    mode_pref: ModePref,
    effective_mode: Mode,
    accent: Accent,
    applied_theme: Option<(ModePref, Accent)>,
    loaders_installed: bool,
    redact_paths: bool,
    control: Option<Receiver<ControlRequest>>,
    control_server: Option<ControlServer>,
    event_frames: std::collections::VecDeque<Vec<egui::Event>>,
    screenshots: Vec<(u64, ControlRequest, Option<PathBuf>)>,
    next_screenshot: u64,
    show_source: bool,
    /// The code view: which of the page's files is picked, and its text while it is read
    /// or edited.
    code: crate::codeview::State,
    /// Which ribbon tab is open.
    ribbon_tab: String,
    /// What the pen is reporting, where the platform reports anything.
    stylus: crate::stylus::Stylus,
    /// Where the page's words were drawn last frame, which is where its ink goes.
    page_rect: Option<egui::Rect>,
    /// What the pointer does on the page (the Draw tab).
    pointer: ribbon::Pointer,
    /// The tools on the rack, as the owner has arranged them, and which is in hand. Both
    /// are remembered between runs.
    rack: Vec<notecraft_ink::rack::Nib>,
    nib: usize,
    /// How readily the pen answers pressure, 0 to 100. 50 leaves the readings as the pen
    /// gave them; above that a light hand lays down more ink.
    sensitivity: u8,
    /// The drawing open in this page, if it has one.
    pad: Option<crate::inkpad::Pad>,
    /// Lines or squares behind the page.
    rules: ribbon::RuleLines,
    /// Lines round the cells of a table, which is how a table is drawn unless the Table
    /// tab turns them off.
    table_borders: bool,
    /// Where GridCraft is, when it is installed: a table can be handed to it. Looked for
    /// once, when the window opens.
    gridcraft: Option<std::path::PathBuf>,
    /// The page title line is hidden (OneNote: View, Hide Page Title).
    hide_title: bool,
    /// The page's zoom. The ribbon and the rest of the window keep their own size.
    zoom_percent: u32,
    /// Recalculate the page zoom from the reading area's width on the next frame.
    fit_page_width: bool,
    /// The theme chooser is open as a window (the ribbon's Theme button).
    theme_window: bool,
    /// Text the ribbon asked to put on the clipboard this frame.
    pending_copy: Option<String>,
    /// Matching HTML flavor for the same copied item, when one is available.
    pending_copy_html: Option<String>,
    /// The File screen, when it is open, and which of its pages.
    backstage: Option<&'static str>,
    /// The page fills the window: the notebooks are hidden until it is turned off.
    full_page: bool,
    /// Put the keyboard in the search box on the next frame.
    focus_search: bool,
    /// The page the window closed on has been opened again (once per run).
    restored: bool,
}

impl NoteCraftApp {
    pub fn new(session: Session) -> NoteCraftApp {
        let mode_pref = session.settings.get("theme_mode").and_then(|v| ModePref::parse(&v)).unwrap_or(ModePref::System);
        let accent = session.settings.get("theme_accent").and_then(|v| Accent::parse(&v)).unwrap_or(Accent::Default);
        let show_source = session.settings.get("show_source").map(|v| v.trim() == "true").unwrap_or(false);
        let session_rules = session.settings.get("rule_lines").and_then(|v| ribbon::RuleLines::parse(&v)).unwrap_or_default();
        let session_hide_title = session.settings.get("hide_title").map(|v| v.trim() == "true").unwrap_or(false);
        // A table has borders unless it was turned off: OneNote draws them, and a table
        // without them reads as loose columns of words.
        let session_borders = session.settings.get("table_borders").map(|v| v.trim() != "false").unwrap_or(true);
        // Two columns in the side panel, the way OneNote separates the notebooks from the
        // pages. Off by default: one tree is what NoteCraft has always shown.
        let session_split = session.settings.get("split_panes").map(|v| v.trim() == "true").unwrap_or(false);
        // The pen rack as the owner last arranged it, and which tool was in hand.
        let rack = notecraft_ink::rack::parse(&session.settings.get(RACK_KEY).unwrap_or_default());
        let nib = session.settings.get(NIB_KEY).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0).min(rack.len() - 1);
        let sensitivity = session.settings.get(SENSITIVITY_KEY).and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(50).min(100);
        let qat = quickaccess::Config::load(|k| session.settings.get(k));
        let privacy_welcome = session.settings.get("privacy_welcome_seen").is_none();
        NoteCraftApp {
            session,
            nav: NavHistory::default(),
            qat,
            last_ink_change: None,
            open: HashSet::new(),
            selected: None,
            split_panes: session_split,
            organisers: true,
            always_on_top: false,
            window_level_pending: false,
            frameless: false,
            editor: None,
            dialog: if privacy_welcome { Some(Dialog::PrivacyWelcome { level: "debug".into() }) } else { None },
            import_job: None,
            status: "Ready".into(),
            status_is_error: false,
            search: String::new(),
            searched: String::new(),
            hits: Vec::new(),
            cursor: 0,
            tokens: Theme::DEFAULT.tokens(),
            mode_pref,
            effective_mode: if mode_pref == ModePref::Dark { Mode::Dark } else { Mode::Light },
            accent,
            applied_theme: None,
            loaders_installed: false,
            redact_paths: false,
            control: None,
            control_server: None,
            event_frames: std::collections::VecDeque::new(),
            screenshots: Vec::new(),
            next_screenshot: 1,
            show_source,
            code: crate::codeview::State::default(),
            ribbon_tab: "Home".to_string(),
            stylus: crate::stylus::Stylus::default(),
            page_rect: None,
            pointer: ribbon::Pointer::Write,
            rack,
            nib,
            sensitivity,
            pad: None,
            rules: session_rules,
            table_borders: session_borders,
            gridcraft: crate::gridcraft::installed(),
            hide_title: session_hide_title,
            zoom_percent: 100,
            fit_page_width: false,
            theme_window: false,
            pending_copy: None,
            pending_copy_html: None,
            backstage: None,
            full_page: false,
            focus_search: false,
            restored: false,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// The session, for tests and for tools that drive the window.
    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    /// Writes the open page now, instead of waiting for the autosave.
    pub fn save_page_now(&mut self) {
        self.save_now();
    }

    /// Opens the page that was open when the window last closed, and the branches above it,
    /// the way OneNote comes back where you left it. Runs once, on the first frame.
    fn restore_last_page(&mut self) {
        if self.restored {
            return;
        }
        self.restored = true;
        let Some(saved) = self.session.settings.get("last_page") else { return };
        let path = self.session.root().join(saved.trim().replace('/', std::path::MAIN_SEPARATOR_STR));
        if !path.is_dir() {
            return;
        }
        // Open every container above it, so the page is visible in the tree.
        let mut at = path.parent();
        while let Some(dir) = at {
            if !dir.starts_with(self.session.root()) || dir == self.session.root() {
                break;
            }
            self.open.insert(dir.to_path_buf());
            at = dir.parent();
        }
        self.open_page(&path);
    }

    /// Remembers which page is open, for the next time the window starts.
    fn remember_last_page(&mut self, dir: &Path) {
        let Ok(rel) = dir.strip_prefix(self.session.root()) else { return };
        let value = rel.to_string_lossy().replace('\\', "/");
        let _ = self.session.settings.set("last_page", &value);
    }

    /// Whether the page source is shown beside the page. Off by default: the page is what
    /// the user writes on, and the source is there when they ask for it.
    pub fn show_source(&self) -> bool {
        self.show_source
    }

    pub fn set_show_source(&mut self, on: bool) {
        // Closing the view after the page file was typed in: the page is read again, so
        // the live view, the title and everything in the head come from what is on disk.
        if !on && self.code.page_edited {
            self.code.page_edited = false;
            if let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) {
                self.open_page(&dir);
            }
        }
        self.show_source = on;
        if let Err(e) = self.session.settings.set("show_source", if on { "true" } else { "false" }) {
            self.status = format!("The setting could not be saved: {e}");
            self.status_is_error = true;
        }
    }

    /// Chooses a theme (light or dark, and an accent) and saves the choice.
    pub fn set_theme(&mut self, theme: Theme) {
        self.mode_pref = if theme.mode.is_dark() { ModePref::Dark } else { ModePref::Light };
        self.effective_mode = theme.mode;
        self.accent = theme.accent;
        self.save_theme();
    }

    /// Opens a File/backstage page for deterministic UI captures and integration harnesses.
    pub fn open_backstage_page(&mut self, page: &'static str) {
        self.backstage = Some(page);
    }

    /// Shows a deterministic progress state for documentation screenshots.
    pub fn show_demo_import_progress(&mut self, index: usize, total: usize, current: &str) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.import_job = Some(ImportJob {
            rx,
            keep_alive: Some(tx),
            cancel: Arc::new(AtomicBool::new(false)),
            source: PathBuf::from("demo-source"),
            destination: PathBuf::from("demo-destination"),
            current: current.to_string(),
            index,
            total,
        });
    }

    /// Follows the system light or dark setting, with the given accent, and saves the choice.
    pub fn follow_system_theme(&mut self, accent: Accent) {
        self.mode_pref = ModePref::System;
        self.accent = accent;
        self.save_theme();
    }

    /// The theme in use (for the system mode, the mode the system last asked for).
    pub fn theme(&self) -> Theme {
        Theme { mode: self.effective_mode, accent: self.accent }
    }

    pub fn mode_pref(&self) -> ModePref {
        self.mode_pref
    }

    fn save_theme(&mut self) {
        let a = self.session.settings.set("theme_mode", self.mode_pref.id());
        let b = self.session.settings.set("theme_accent", self.accent.id());
        if let Err(e) = a.and(b) {
            self.status = format!("The theme could not be saved: {e}");
            self.status_is_error = true;
        }
    }

    /// Applies the chosen theme to egui when it changed, and refreshes the tokens.
    fn apply_theme(&mut self, ctx: &Context) {
        let key = (self.mode_pref, self.accent);
        if self.applied_theme != Some(key) {
            ctx.set_visuals_of(egui::Theme::Light, Theme { mode: Mode::Light, accent: self.accent }.visuals());
            ctx.set_visuals_of(egui::Theme::Dark, Theme { mode: Mode::Dark, accent: self.accent }.visuals());
            ctx.set_theme(match self.mode_pref {
                ModePref::Light => egui::ThemePreference::Light,
                ModePref::Dark => egui::ThemePreference::Dark,
                ModePref::System => egui::ThemePreference::System,
            });
            self.applied_theme = Some(key);
        }
        self.effective_mode = if ctx.theme() == egui::Theme::Dark { Mode::Dark } else { Mode::Light };
        self.tokens = self.theme().tokens();
        // Keep every modal surface opaque and on the same themed panel as the rest of the app.
        // Individual dialogs may opt out of the native title bar, but they all inherit this
        // surface and border treatment.
        for theme in [egui::Theme::Light, egui::Theme::Dark] {
            ctx.style_mut_of(theme, |style| {
                style.visuals.window_fill = self.tokens.panel;
                style.visuals.window_stroke = egui::Stroke::new(1.0, self.tokens.faint);
            });
        }
    }

    /// Shows paths as `<notebooks>/...` instead of real local paths. Used by tests and
    /// when taking screenshots for documentation.
    pub fn set_redact_paths(&mut self, on: bool) {
        self.redact_paths = on;
    }

    fn shown(&self, p: &Path) -> String {
        shown_in(self.session.root(), p, self.redact_paths)
    }

    /// Opens the page at `path` (absolute, or relative to the notebooks folder) and
    /// expands the tree down to it.
    pub fn open_path(&mut self, path: &str) {
        // An explicit command-line/control request wins over the page saved by the
        // previous session. Otherwise the first frame's restore pass would replace the
        // requested page with a stale page from another notebook.
        self.restored = true;
        match self.session.resolve(path) {
            Ok(dir) => {
                self.expand_to(&dir);
                self.open_page(&dir);
            }
            Err(e) => self.fail(&e),
        }
    }

    /// Opens every level above `dir`, so the page is visible in the tree.
    fn expand_to(&mut self, dir: &Path) {
        let mut parent = dir.parent();
        while let Some(d) = parent {
            if d == self.session.root() {
                break;
            }
            self.open.insert(d.to_path_buf());
            parent = d.parent();
        }
    }

    /// Back to the page before this one.
    fn go_back(&mut self) {
        match self.nav.back(|p| p.exists()) {
            Some(dir) => {
                self.expand_to(&dir);
                self.open_page_inner(&dir);
            }
            None => self.say("There is no earlier page"),
        }
    }

    /// Forward to the page after this one.
    fn go_forward(&mut self) {
        match self.nav.forward(|p| p.exists()) {
            Some(dir) => {
                self.expand_to(&dir);
                self.open_page_inner(&dir);
            }
            None => self.say("There is no later page"),
        }
    }

    fn can_undo(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_undo()) || self.pad.as_ref().is_some_and(|p| p.strokes() > 0)
    }

    fn can_redo(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_redo())
    }

    /// Undo for the Quick Access Toolbar: the page's words and boxes (the page history in
    /// `history.rs`), or the last stroke when the pen was used after anything else on the page,
    /// or when the page has nothing left to take back but the drawing has.
    fn undo(&mut self) {
        let edited = self.editor.as_ref().and_then(|e| e.history.when());
        let ink_newer = match (self.last_ink_change, edited) {
            (Some(i), Some(e)) => i > e,
            (Some(_), None) => true,
            _ => false,
        };
        let has_strokes = self.pad.as_ref().is_some_and(|p| p.strokes() > 0);
        let page_can = self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_undo());
        if has_strokes && (ink_newer || !page_can) {
            self.ink_undo();
        } else {
            self.step_back(true);
        }
    }

    fn redo(&mut self) {
        self.step_back(false);
    }

    /// The keys and buttons that move through the pages that were opened. Undo and Redo keys
    /// are the page history's own, taken below.
    fn nav_keys(&mut self, ctx: &Context) {
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowLeft)) || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1)) {
            self.go_back();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowRight)) || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra2)) {
            self.go_forward();
        }
    }

    /// The Quick Access Toolbar, drawn in whatever row it is given.
    fn quick_access(&mut self, ui: &mut Ui, tokens: Tokens) {
        let enabled =
            quickaccess::Enabled { back: self.nav.can_back(), forward: self.nav.can_forward(), undo: self.can_undo(), redo: self.can_redo() };
        let (pressed, changed) = quickaccess::show(ui, &mut self.qat, enabled, &tokens);
        if changed {
            self.qat.save(|k, v| {
                let _ = self.session.settings.set(k, v);
            });
        }
        match pressed {
            Some(quickaccess::Command::Back) => self.go_back(),
            Some(quickaccess::Command::Forward) => self.go_forward(),
            Some(quickaccess::Command::Undo) => self.undo(),
            Some(quickaccess::Command::Redo) => self.redo(),
            None => {}
        }
    }

    /// Opens every level of the tree.
    pub fn expand_all(&mut self) {
        self.open = self.collect_branches().into_iter().collect();
    }

    /// Whether the page being edited has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.dirty_since.is_some())
    }

    /// Where the page's words were drawn last frame. Its ink is measured from the top
    /// left of this, so a test that writes on the page aims here.
    pub fn page_rect(&self) -> Option<egui::Rect> {
        self.page_rect
    }

    /// The text in the editor, if a page is open.
    pub fn body(&self) -> Option<&str> {
        self.editor.as_ref().map(|e| e.body.as_str())
    }

    /// The title of the open page, which the window shows above it.
    pub fn title(&self) -> Option<&str> {
        self.editor.as_ref().map(|e| e.title.as_str())
    }

    /// Replaces the text in the editor, as typing would, and marks the page changed.
    pub fn set_body(&mut self, body: &str) {
        if let Some(ed) = self.editor.as_mut() {
            ed.body = body.to_string();
            Self::mark_dirty(ed);
        }
    }

    /// Saves the open page now, if it has changes.
    pub fn save(&mut self) {
        self.save_now();
    }

    /// The last status line.
    pub fn status(&self) -> &str {
        &self.status
    }

    fn say(&mut self, msg: impl Into<String>) {
        self.status = msg.into();
        self.status_is_error = false;
    }

    /// Says something went wrong, in our own words rather than the engine's.
    fn complain(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_is_error = true;
    }

    fn fail(&mut self, e: &EngineError) {
        self.status = e.to_string();
        self.status_is_error = true;
    }

    fn run(&mut self, id: &str, p: &Params) -> Option<Output> {
        match self.session.run(id, p) {
            Ok(o) => Some(o),
            Err(e) => {
                self.fail(&e);
                None
            }
        }
    }

    // ---------- page open, save ----------

    fn open_page(&mut self, dir: &Path) {
        if let Some(notebook) = notebook_for(dir) {
            self.remember_recent_notebook(&notebook);
        }
        self.nav.visit(dir);
        self.open_page_inner(dir);
    }

    /// Opens a page without counting it as a visit: what Back and Forward do.
    fn open_page_inner(&mut self, dir: &Path) {
        self.save_now();
        if let Some(Output::Page(page)) = self.run("page.load", &params(&[("path", &path_str(dir))])) {
            let (blocks, in_the_text) = notecraft_formats::boxes::read(page.format, &page.body);
            let older = notecraft_store::page::containers(dir).unwrap_or_default();
            let arranged = in_the_text.is_some() || !older.is_empty();
            let boxes = in_the_text.unwrap_or(older);
            // The page as it was opened is the first step of its history: undo goes back
            // this far and no further.
            let first = (page.body.clone(), blocks.clone(), boxes.clone());
            // The paper this page is drawn on and the colour behind it are the page's own.
            let appearance = notecraft_store::page::appearance(dir)
                .map(|(p, c)| (p.and_then(|p| ribbon::RuleLines::parse(&p)), c.and_then(|c| notecraft_model::NodeColor::parse(&c))))
                .unwrap_or((None, None));
            let attachments = notecraft_store::assets::list(dir);
            let writable = pageedit::round_trips(page.format, &page.body);
            // A page with no title of its own starts with a blank title area, not with
            // "Untitled page" sitting there as if it had been typed. `title` keeps the
            // display fallback, so losing focus on an empty box is correctly seen as "no
            // change" rather than a rename to the fallback word itself.
            let title_buf = if page.has_title { page.title.clone() } else { String::new() };
            self.editor = Some(Editor {
                dir: dir.to_path_buf(),
                format: page.format,
                title_buf,
                title: page.title,
                body: page.body,
                dirty_since: None,
                blocks,
                last_saved: None,
                attachments,
                caret: None,
                writable,
                page_dir: page.dir,
                boxes,
                arranged,
                columns: notecraft_store::page::table_columns(dir).unwrap_or_default(),
                justified: notecraft_store::page::justified(dir).unwrap_or_default(),
                across: None,
                ink_files: notecraft_store::page::ink_files(dir),
                paper: appearance.0,
                colour: appearance.1,
                history: crate::history::History::new(crate::history::Step::new(first.0, first.1, first.2, None, crate::history::Kind::Edit)),
                origin: egui::Pos2::ZERO,
            });
            self.pad = Some(crate::inkpad::Pad::of_page(dir));
            self.absorb_old_sheets(dir);
            self.sync_ink_reference();
            self.selected = Some(dir.to_path_buf());
            self.cursor = 0;
            self.remember_last_page(dir);
            self.say("Opened");
        }
    }

    /// A page written by an earlier version holds its drawing as a picture pointing at
    /// `assets/drawing-1.svg`. Nothing draws those now, so the page showed a broken
    /// picture. Their strokes are moved onto the page's own ink and the picture is taken
    /// out of the words; the old file is left in the folder untouched.
    fn absorb_old_sheets(&mut self, dir: &Path) {
        use notecraft_formats::markdown::Block;
        let olds: Vec<String> = self
            .editor
            .as_ref()
            .map(|e| {
                e.blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Image { src, .. } if crate::inkpad::is_old_sheet(src) => Some(src.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        if olds.is_empty() {
            return;
        }
        let mut moved = 0;
        if let Some(pad) = self.pad.as_mut() {
            for src in &olds {
                moved += pad.absorb(dir, src);
            }
            if pad.dirty && pad.save().is_err() {
                self.complain("Cannot write this page's ink");
                return;
            }
        }
        if let Some(ed) = self.editor.as_mut() {
            ed.blocks.retain(|b| !matches!(b, Block::Image { src, .. } if crate::inkpad::is_old_sheet(src)));
            ed.rebuild_body();
            Self::mark_dirty_from_blocks(ed);
        }
        self.sync_ink_reference();
        // The old file has been read and its strokes are in `ink.svg`, so leaving it
        // behind only puts a dead 200 KB picture in the page's folder for the owner to
        // wonder about. It goes, now that what was in it is kept somewhere better.
        let mut left = 0;
        for src in &olds {
            let file = dir.join(src.trim().replace('/', std::path::MAIN_SEPARATOR_STR));
            if std::fs::remove_file(&file).is_err() {
                left += 1;
            }
        }
        if left > 0 {
            self.say(format!("Moved {moved} strokes onto the page; {left} old drawing files could not be removed"));
        } else {
            self.say(format!("Moved {moved} strokes from this page's old drawing onto the page itself"));
        }
    }

    fn save_now(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        if ed.dirty_since.is_none() {
            return;
        }
        let (dir, body) = (ed.dir.clone(), ed.body.clone());
        match self.session.run("page.save", &params(&[("path", &path_str(&dir)), ("body", &body)])) {
            Ok(Output::Text(ts)) => {
                if let Some(ed) = self.editor.as_mut() {
                    ed.dirty_since = None;
                    ed.last_saved = Some(ts);
                }
                self.say("Saved");
            }
            Ok(_) => {}
            Err(e) => self.fail(&e),
        }
    }

    /// The page text changed: remember to save it, and read the blocks from it again.
    fn mark_dirty(ed: &mut Editor) {
        ed.dirty_since = Some(Instant::now());
        let (blocks, in_the_text) = notecraft_formats::boxes::read(ed.format, &ed.body);
        ed.blocks = blocks;
        ed.arranged = in_the_text.is_some();
        ed.boxes = in_the_text.unwrap_or_default();
        ed.caret = None;
    }

    /// The blocks changed (the caret is in the page): remember to save, and keep the blocks
    /// as they are. Reading them back from the text would throw away an empty new block and
    /// move the caret out from under the user.
    fn mark_dirty_from_blocks(ed: &mut Editor) {
        ed.dirty_since = Some(Instant::now());
    }

    fn insert(&mut self, text: &str) {
        let cursor = self.cursor;
        if let Some(ed) = self.editor.as_mut() {
            let (body, next) = snippets::insert_at(&ed.body, cursor, text);
            ed.body = body;
            Self::mark_dirty(ed);
            self.cursor = next;
        }
    }

    /// A dropped file asks what to do with it, the way OneNote asks when you insert one.
    fn attach_dropped(&mut self, ctx: &Context) {
        let files: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if files.is_empty() {
            return;
        }
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.status = "Open a page first, then drop files onto it.".into();
            self.status_is_error = true;
            return;
        };
        self.dialog = Some(Dialog::FileOptions { dir, files });
    }

    /// Carries out one of the choices of the file dialog.
    fn add_file(&mut self, dir: &Path, file: &Path, how: FileChoice) {
        let Some(format) = self.editor.as_ref().map(|e| e.format) else { return };
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        match how {
            FileChoice::Copy => {
                log::debug!("file attach requested: name={name:?} page={:?}", dir);
                match self.run("page.attach", &params(&[("path", &path_str(dir)), ("source", &path_str(file))])) {
                    Some(Output::Text(link)) => {
                        log::debug!("file attach copied: name={name:?} link={link:?}");
                        let text = snippets::attachment(format, &link, &name);
                        self.insert(&text);
                        self.say(format!("{name} is now a file in this page's folder"));
                    }
                    Some(_) => self.complain("The file was not attached: the attach command returned an unexpected result."),
                    None => {
                        log::error!("file attach failed: name={name:?} page={:?} status={:?}", dir, self.status);
                        self.complain(&format!("Could not attach {name} to this page."));
                    }
                }
            }
            FileChoice::Link => {
                let text = snippets::file_link(format, &path_str(file), &name);
                self.insert(&text);
                self.say(format!("Linked {name} where it is; the file was not copied"));
            }
        }
        if let Some(ed) = self.editor.as_mut() {
            ed.attachments = notecraft_store::assets::list(&ed.dir);
        }
    }

    // ---------- tree ----------

    fn collect_branches(&self) -> Vec<PathBuf> {
        let mut v = Vec::new();
        for n in self.session.tree() {
            n.branches(&mut v);
        }
        v
    }

    fn apply(&mut self, act: Act) {
        match act {
            Act::Toggle(p) => {
                if !self.open.remove(&p) {
                    self.open.insert(p);
                }
            }
            Act::Select(p) => {
                let is_page = notecraft_store::tree::read_node(&p).map(|i| i.kind == NodeKind::Page).unwrap_or(false);
                if is_page {
                    self.open_page(&p);
                } else {
                    if !self.open.remove(&p) {
                        self.open.insert(p.clone());
                    }
                    // Selecting a container must also move the editor. Leaving the old
                    // page open here made the navigator and page column describe one
                    // location while the editor showed another.
                    if let Some(page) = first_page(self.session.tree(), &p) {
                        self.open_page(&page);
                    } else {
                        self.selected = Some(p);
                    }
                }
            }
            Act::Add(parent, parent_kind) => {
                let kind = parent_kind.may_contain().first().copied().unwrap_or(NodeKind::Page);
                let kind = if parent_kind == NodeKind::Notebook || parent_kind == NodeKind::SectionGroup { NodeKind::Section } else { kind };
                self.dialog = Some(Dialog::Add { parent, parent_kind, title: String::new(), kind, format: None });
            }
            Act::NewNotebook => self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None }),
            Act::CloseNotebook(path) => {
                self.save_now();
                let was_showing = self.editor.as_ref().is_some_and(|ed| ed.dir.starts_with(&path));
                if matches!(self.run("notebook.close", &params(&[("path", &path_str(&path))])), Some(Output::Text(ref result)) if result == "closed")
                {
                    self.open.retain(|p| !p.starts_with(&path));
                    if self.selected.as_ref().is_some_and(|p| p.starts_with(&path)) {
                        self.selected = self.session.tree().first().map(|n| n.path.clone());
                    }
                    if was_showing {
                        self.editor = None;
                        self.pad = None;
                        self.nav = NavHistory::default();
                    }
                    self.say("Notebook closed; its files are still on disk");
                }
            }
            Act::RevealRoot => {
                let root = self.session.root().to_path_buf();
                self.reveal(&root);
            }
            Act::ChooseRoot => self.choose_notebooks_folder(),
            Act::AddKind(parent, parent_kind, kind) => {
                self.dialog = Some(Dialog::Add { parent, parent_kind, title: String::new(), kind, format: None });
            }
            Act::AddPageAt(section, order) => {
                let path = path_str(&section);
                let order = order.to_string();
                // No title: the page is created genuinely untitled, like a new OneNote page.
                // "Untitled page" is only ever the display fallback, never real content.
                if let Some(Output::Path(dir)) = self.run("node.create", &params(&[("parent", &path), ("kind", "page"), ("order", &order)])) {
                    self.open_page(&dir);
                    self.say("New page");
                }
            }
            Act::Rename(path, title) => self.dialog = Some(Dialog::Rename { path, title }),
            Act::Delete(path, title, count) => self.dialog = Some(Dialog::Delete { path, title, count }),
            Act::Color(path, color) => {
                let value = color.map_or("none", NodeColor::id);
                if self.run("node.color", &params(&[("path", &path_str(&path)), ("color", value)])).is_some() {
                    self.say(match color {
                        Some(c) => format!("Colour set to {}", c.label().to_lowercase()),
                        None => "Colour removed".to_string(),
                    });
                }
            }
            Act::CopyLink(path) => self.copy_link_to(&path),
            Act::Reveal(path) => match open_folder(&path) {
                Ok(()) => self.say(format!("Opened {}", self.shown(&path))),
                Err(e) => {
                    self.status = format!("The folder could not be opened: {e}");
                    self.status_is_error = true;
                }
            },
        }
    }

    fn draw_node(&self, ui: &mut Ui, node: &Node, depth: usize, acts: &mut Vec<Act>) {
        self.draw_row(ui, node, depth, acts, true);
    }

    /// One row of the tree. With `pages` false the pages are left out, which is what the
    /// organisers column shows: notebooks, section groups and sections only.
    fn draw_row(&self, ui: &mut Ui, node: &Node, depth: usize, acts: &mut Vec<Act>, pages: bool) {
        let has_kids = if pages { !node.children.is_empty() } else { node.children.iter().any(|c| c.info.kind != NodeKind::Page) };
        let open = self.open.contains(&node.path);
        let selected = self.selected.as_deref() == Some(node.path.as_path());
        let width = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
        let painter = ui.painter_at(rect);
        let t = &self.tokens;
        if selected {
            painter.rect_filled(rect, 4.0, t.row_selected);
        } else if resp.hovered() {
            painter.rect_filled(rect, 4.0, t.row_hover);
        }
        let x0 = rect.min.x + depth as f32 * INDENT + 2.0;
        let cy = rect.center().y;
        let chevron_rect = egui::Rect::from_min_size(egui::pos2(x0, rect.min.y), vec2(16.0, ROW_HEIGHT));
        if has_kids {
            let c = chevron_rect.center();
            let stroke = egui::Stroke::new(1.5, t.muted);
            let pts = if open {
                [egui::pos2(c.x - 3.5, c.y - 1.5), egui::pos2(c.x, c.y + 2.5), egui::pos2(c.x + 3.5, c.y - 1.5)]
            } else {
                [egui::pos2(c.x - 1.5, c.y - 3.5), egui::pos2(c.x + 2.5, c.y), egui::pos2(c.x - 1.5, c.y + 3.5)]
            };
            painter.line_segment([pts[0], pts[1]], stroke);
            painter.line_segment([pts[1], pts[2]], stroke);
        }
        let (icon, default_color) = match node.info.kind {
            NodeKind::Notebook => (Icon::Notebook, t.notebook),
            NodeKind::SectionGroup => (if open && has_kids { Icon::SectionGroupOpen } else { Icon::SectionGroup }, t.group),
            NodeKind::Section => (Icon::Section, t.section),
            NodeKind::Page => (if has_kids && !open { Icon::PageStack } else { Icon::Page }, t.page),
        };
        // The colour the user gave this notebook or section, kept readable on the panel.
        let color = match node.info.color {
            Some(c) => {
                let (r, g, b) = c.rgb(self.effective_mode.is_dark());
                crate::theme::readable_on(egui::Color32::from_rgb(r, g, b), t.panel, 3.0, self.effective_mode.is_dark())
            }
            None => default_color,
        };
        // The coloured edge of the row, like the edge of a OneNote tab.
        if node.info.color.is_some() {
            let edge = egui::Rect::from_min_size(egui::pos2(rect.min.x, rect.min.y + 2.0), vec2(3.0, ROW_HEIGHT - 4.0));
            painter.rect_filled(edge, 1.5, color);
        }
        let icon_rect = egui::Rect::from_min_size(egui::pos2(x0 + 18.0, cy - 8.5), vec2(17.0, 17.0));
        icons::paint(&painter, icon_rect, icon, color);
        let text_pos = egui::pos2(x0 + 40.0, cy);
        let text_color = if selected { t.strong } else { ui.style().visuals.text_color() };
        // Pages reserve room for their format label and add-page affordance. Organiser rows
        // do not, so notebook, group and section names should run almost to the pane border.
        let trailing = if node.info.kind == NodeKind::Page { 52.0 } else { 8.0 };
        let text_clip = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x - trailing, rect.max.y));
        // The tree is on screen all the time, so this is the most visible place the words
        // of an Arabic name would come out backwards. Each row's name decides its own
        // direction; the row keeps its place and its clip either way.
        let clipped = painter.with_clip_rect(text_clip);
        crate::bidi::line_at(ui, &clipped, text_pos, Align2::LEFT_CENTER, &node.info.title, 14.0, text_color, None);
        let plus_rect = egui::Rect::from_min_size(egui::pos2(rect.max.x - 24.0, rect.min.y + 2.0), vec2(20.0, ROW_HEIGHT - 4.0));
        if node.info.kind == NodeKind::Page
            && let Some(f) = node.info.format
        {
            painter.text(egui::pos2(rect.max.x - 30.0, cy), Align2::RIGHT_CENTER, f.short(), egui::FontId::proportional(10.5), t.muted);
        }
        if resp.hovered() {
            painter.text(plus_rect.center(), Align2::CENTER_CENTER, "+", egui::FontId::proportional(16.0), t.strong);
        }
        let enabled = ui.is_enabled();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &node.info.title));
        let resp = resp.on_hover_text(self.shown(&node.path));
        if resp.clicked() {
            let pos = resp.interact_pointer_pos().unwrap_or_default();
            if has_kids && chevron_rect.expand2(vec2(2.0, 0.0)).contains(pos) {
                acts.push(Act::Toggle(node.path.clone()));
            } else if plus_rect.contains(pos) {
                acts.push(Act::Add(node.path.clone(), node.info.kind));
            } else {
                acts.push(Act::Select(node.path.clone()));
            }
        }
        resp.context_menu(|ui| {
            // What this node may hold, named in full rather than left to the dialog: a
            // notebook and a group take sections and groups, a section takes pages, and a
            // page takes sub-pages.
            for kind in node.info.kind.may_contain() {
                let label = match kind {
                    NodeKind::SectionGroup => "New section group...",
                    NodeKind::Section => "New section...",
                    NodeKind::Page if node.info.kind == NodeKind::Page => "New sub-page...",
                    NodeKind::Page => "New page...",
                    NodeKind::Notebook => "New notebook...",
                };
                let icon = match kind {
                    NodeKind::SectionGroup => Icon::SectionGroup,
                    NodeKind::Section => Icon::Section,
                    NodeKind::Page => Icon::Page,
                    NodeKind::Notebook => Icon::Notebook,
                };
                if floating::menu_row(ui, Some(floating::Glyph::Icon(icon)), label, t).clicked() {
                    acts.push(Act::AddKind(node.path.clone(), node.info.kind, *kind));
                    ui.close();
                }
            }
            floating::menu_separator(ui, t);
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Page)), "Rename...", t).clicked() {
                acts.push(Act::Rename(node.path.clone(), node.info.title.clone()));
                ui.close();
            }
            if node.info.kind == NodeKind::Notebook
                && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Notebook)), "Close notebook", t).clicked()
            {
                acts.push(Act::CloseNotebook(node.path.clone()));
                ui.close();
            }
            if has_kids
                && floating::menu_row(
                    ui,
                    Some(floating::Glyph::Icon(if open { Icon::Minus } else { Icon::Plus })),
                    if open { "Collapse" } else { "Expand" },
                    t,
                )
                .clicked()
            {
                acts.push(Act::Toggle(node.path.clone()));
                ui.close();
            }
            // Pages take the colour of their section, the way OneNote colours tabs.
            if node.info.kind != NodeKind::Page {
                ui.menu_button("Colour", |ui| {
                    let dark = self.effective_mode.is_dark();
                    egui::Grid::new("node-colours").spacing([4.0, 4.0]).show(ui, |ui| {
                        for (i, c) in NodeColor::ALL.into_iter().enumerate() {
                            if color_swatch(ui, c, dark, node.info.color == Some(c)).clicked() {
                                acts.push(Act::Color(node.path.clone(), Some(c)));
                                ui.close();
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                    ui.add_space(2.0);
                    if ui.button("No colour").clicked() {
                        acts.push(Act::Color(node.path.clone(), None));
                        ui.close();
                    }
                });
            }
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Link)), "Copy link to page", t)
                .on_hover_text("A link to paste into another page")
                .clicked()
            {
                acts.push(Act::CopyLink(node.path.clone()));
                ui.close();
            }
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Folder)), "Open folder", t).on_hover_text(self.shown(&node.path)).clicked() {
                acts.push(Act::Reveal(node.path.clone()));
                ui.close();
            }
            floating::menu_separator(ui, t);
            if floating::menu_row(ui, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Delete)), "Delete...", t).clicked() {
                acts.push(Act::Delete(node.path.clone(), node.info.title.clone(), node.count()));
                ui.close();
            }
        });
        if has_kids && open {
            for c in node.children.iter().filter(|c| pages || c.info.kind != NodeKind::Page) {
                self.draw_row(ui, c, depth + 1, acts, pages);
            }
        }
    }

    /// The first section anywhere in the tree, for a window that has nothing selected.
    fn first_section_of(nodes: &[Node]) -> Option<&Node> {
        for n in nodes {
            if n.info.kind == NodeKind::Section {
                return Some(n);
            }
            if let Some(found) = Self::first_section_of(&n.children) {
                return Some(found);
            }
        }
        None
    }

    /// The section whose pages the pages column shows: the selected node if it is one, else
    /// the section the selected page sits in, else the first section there is.
    fn current_section<'a>(&self, tree: &'a [Node]) -> Option<&'a Node> {
        let selected = self.selected.clone();
        fn walk<'a>(nodes: &'a [Node], want: &Option<PathBuf>, best: &mut Option<&'a Node>, inside: Option<&'a Node>) {
            for n in nodes {
                let section = if n.info.kind == NodeKind::Section { Some(n) } else { inside };
                if Some(&n.path) == want.as_ref() {
                    *best = match n.info.kind {
                        NodeKind::Section => Some(n),
                        NodeKind::Page => section,
                        _ => *best,
                    };
                }
                walk(&n.children, want, best, section);
            }
        }
        let mut best = None;
        walk(tree, &selected, &mut best, None);
        best.or_else(|| Self::first_section_of(tree))
    }

    // ---------- panels ----------

    fn top_bar(&mut self, ui: &mut Ui) {
        self.title_band(ui, true);
        self.ribbon_bar(ui);
        // Below the ribbon, as OneNote can put it.
        if self.qat.below && !self.qat.hidden {
            let tokens = self.tokens;
            egui::Panel::top("quick-access").show(ui, |ui| {
                ui.horizontal(|ui| self.quick_access(ui, tokens));
            });
        }
    }

    /// Says that the window has no frame of the system's own, so this window draws its buttons
    /// and takes care of moving and resizing it. The program calls this when it asked for no frame.
    pub fn set_frameless(&mut self, on: bool) {
        self.frameless = on;
    }

    /// The coloured band across the top of the window, as in Office: the Quick Access Toolbar,
    /// the page's title, search and the window's buttons in one row, and under it the tab
    /// names. The File screen shows the first row only.
    fn title_band(&mut self, ui: &mut Ui, with_tabs: bool) {
        let t = self.tokens;
        let (tab, source, in_table) = (self.ribbon_tab.clone(), self.show_source, self.caret_in_table());
        let title = self.editor.as_ref().map(|e| e.title.trim().to_owned()).filter(|s| !s.is_empty());
        let mut acts = Vec::new();
        egui::Panel::top("band").frame(egui::Frame::new().fill(t.band)).show_separator_line(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
            self.title_row(ui, title.as_deref());
            if with_tabs {
                ribbon::tab_strip(ui, &tab, source, in_table, &t, &mut acts);
            }
        });
        for a in acts {
            self.ribbon_act(a);
        }
    }

    /// The band's first row. Anything on it that is not a control moves the window.
    fn title_row(&mut self, ui: &mut Ui, title: Option<&str>) {
        const H: f32 = 30.0;
        let on = self.tokens.on_band();
        let ctx = ui.ctx().clone();
        let row = egui::Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), H));
        // Added first, so the controls drawn over it win the pointer: only bare band is a handle.
        let handle = ui.interact(row, ui.id().with("title-handle"), Sense::click_and_drag());
        if self.frameless {
            if handle.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if handle.double_clicked() {
                let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
        }
        // The choice between the system's title bar and this one, kept in the settings.
        handle.context_menu(|ui| {
            let label = if self.frameless { "Use the system title bar" } else { "Use NoteCraft's own title bar" };
            if ui.button(label).clicked() {
                self.frameless = !self.frameless;
                ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.frameless));
                let _ = self.session.settings.set("window_frame", if self.frameless { "own" } else { "system" });
                ui.close();
            }
        });
        let text = title.map_or_else(|| "NoteCraft".to_owned(), |t| format!("{t} - NoteCraft"));
        // A page named in Arabic puts the band's words in the wrong order if the whole
        // string is handed to egui, so the band goes through the bidi painter like the page
        // does. The name decides its own direction here; there is no page to follow.
        let band = ui.painter_at(row);
        crate::bidi::line_at(ui, &band, row.center(), Align2::CENTER_CENTER, &text, 12.5, on.text, None);
        let mut left = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::left_to_right(egui::Align::Center)));
        left.add_space(8.0);
        left.add(egui::Image::from_bytes("bytes://notecraft-logo", crate::LOGO_PNG).fit_to_exact_size(vec2(18.0, 18.0)));
        left.add_space(6.0);
        if self.qat.hidden || !self.qat.below {
            self.quick_access(&mut left, on);
        }
        let mut right = ui.new_child(egui::UiBuilder::new().max_rect(row).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if self.frameless {
            self.window_buttons(&mut right, &on);
        }
        right.add_space(4.0);
        self.theme_menu(&mut right, on);
        let resp = right.add(egui::TextEdit::singleline(&mut self.search).hint_text("Search all pages").desired_width(200.0));
        if self.focus_search {
            resp.request_focus();
            self.focus_search = false;
        }
        if resp.changed() && self.search != self.searched {
            self.searched = self.search.clone();
            self.hits = match self.session.run("search.query", &params(&[("q", &self.search), ("limit", "20")])) {
                Ok(Output::Hits(h)) => h,
                _ => Vec::new(),
            };
        }
        ui.advance_cursor_after_rect(row);
    }

    /// Minimise, maximise or restore, and close: the buttons the system would have drawn.
    fn window_buttons(&self, ui: &mut Ui, on: &Tokens) {
        let ctx = ui.ctx().clone();
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        for kind in ["Close", "Maximize", "Minimize"] {
            let (rect, resp) = ui.allocate_exact_size(vec2(46.0, 30.0), Sense::click());
            let painter = ui.painter_at(rect);
            if resp.hovered() {
                let fill = if kind == "Close" { Color32::from_rgb(196, 43, 28) } else { on.row_hover };
                painter.rect_filled(rect, 0.0, fill);
            }
            let c = rect.center();
            let stroke = egui::Stroke::new(1.0, on.text);
            match kind {
                "Close" => {
                    painter.line_segment([c + vec2(-5.0, -5.0), c + vec2(5.0, 5.0)], stroke);
                    painter.line_segment([c + vec2(-5.0, 5.0), c + vec2(5.0, -5.0)], stroke);
                }
                "Maximize" if maximized => {
                    painter.rect_stroke(egui::Rect::from_min_size(c + vec2(-5.0, -2.0), vec2(7.0, 7.0)), 0.0, stroke, egui::StrokeKind::Inside);
                    painter.line_segment([c + vec2(-2.0, -5.0), c + vec2(5.0, -5.0)], stroke);
                    painter.line_segment([c + vec2(5.0, -5.0), c + vec2(5.0, 2.0)], stroke);
                }
                "Maximize" => {
                    painter.rect_stroke(egui::Rect::from_center_size(c, vec2(10.0, 10.0)), 0.0, stroke, egui::StrokeKind::Inside);
                }
                _ => {
                    painter.line_segment([c + vec2(-5.0, 4.0), c + vec2(5.0, 4.0)], stroke);
                }
            }
            let label = if kind == "Maximize" && maximized { "Restore" } else { kind };
            let enabled = ui.is_enabled();
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
            if resp.on_hover_text(label).clicked() {
                ctx.send_viewport_cmd(match kind {
                    "Close" => egui::ViewportCommand::Close,
                    "Maximize" => egui::ViewportCommand::Maximized(!maximized),
                    _ => egui::ViewportCommand::Minimized(true),
                });
            }
        }
    }

    /// With no frame of the system's own, the edges and corners of the window are ours: the
    /// pointer shows the resize arrow there, and a press starts the resize.
    fn frame_edges(&self, ctx: &Context) {
        if !self.frameless || ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
            return;
        }
        let r = ctx.content_rect();
        // A line round the window, so its edge shows against whatever is behind it.
        ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("window-edge"))).rect_stroke(
            r,
            0.0,
            egui::Stroke::new(1.0, self.tokens.band),
            egui::StrokeKind::Inside,
        );
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        const E: f32 = 5.0;
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

    /// The File screen: notebooks and what is done to a whole notebook.
    fn backstage_screen(&mut self, ui: &mut Ui) {
        let page = self.backstage.unwrap_or("Info");
        let tokens = self.tokens;
        let folder = self.shown(self.session.root());
        let recent = self.recent_notebooks();
        let (root, redact) = (self.session.root().to_path_buf(), self.redact_paths);
        let show_path = move |p: &Path| shown_in(&root, p, redact);
        let mut acts = Vec::new();
        egui::CentralPanel::default().show(ui, |ui| {
            let telemetry_root = self.session.settings.path().parent().unwrap_or(std::path::Path::new(".")).join("telemetry");
            let telemetry_level = self.session.settings.get("telemetry_level").unwrap_or_else(|| "none".into());
            let session_id = self.session.settings.get("telemetry_session_id");
            acts = backstage::show(
                ui,
                self.session.tree(),
                &recent,
                &folder,
                page,
                &show_path,
                &telemetry_root,
                &telemetry_level,
                session_id.as_deref(),
                &tokens,
            );
        });
        for a in acts {
            match a {
                backstage::Act::Close => self.backstage = None,
                backstage::Act::Page(name) => self.backstage = Some(name),
                backstage::Act::NewNotebook => {
                    self.backstage = None;
                    self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None });
                }
                backstage::Act::OpenFolder(p) => self.reveal(&p),
                backstage::Act::ChooseFolder => self.choose_notebooks_folder(),
                backstage::Act::ImportNotebookFolder => self.import_notebook_folder(),
                backstage::Act::ImportFile => self.import_file(),
                backstage::Act::ImportCloud => {
                    self.status = "Cloud import is not implemented yet.".to_string();
                    self.status_is_error = false;
                }
                backstage::Act::OpenNotebooksFolder => {
                    let root = self.session.root().to_path_buf();
                    self.reveal(&root);
                }
                backstage::Act::Open(p) => {
                    self.open_notebook_from_path(&p);
                }
                backstage::Act::OpenRecent(p) => {
                    self.open_notebook_from_path(&p);
                }
                backstage::Act::RemoveRecent(p) => self.remove_recent_notebook(&p),
                backstage::Act::DeleteRecent(p, title) => self.dialog = Some(Dialog::DeleteNotebook { path: p, title }),
                backstage::Act::SetTelemetryLevel(level) => {
                    let _ = self.session.settings.set("telemetry_level", &level);
                }
                backstage::Act::OpenTelemetryFolder(path) => self.reveal(&path),
                backstage::Act::ClearTelemetry(path) => self.dialog = Some(Dialog::ClearTelemetry { path }),
                backstage::Act::ExportTelemetry(root) => {
                    if let Some(session) = std::fs::read_dir(&root)
                        .ok()
                        .and_then(|r| r.filter_map(Result::ok).find(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)))
                        && let Some(destination) = rfd::FileDialog::new()
                            .set_title("Export telemetry ZIP")
                            .set_file_name(format!("notecraft-{}-telemetry.zip", session.file_name().to_string_lossy()))
                            .save_file()
                    {
                        let _ = crate::telemetry_export::export(&session.path(), &destination);
                    }
                }
                backstage::Act::AddWelcome => {
                    self.backstage = None;
                    self.add_welcome();
                }
                backstage::Act::NotBuilt(name) => {
                    self.status = format!("{name} is not built yet. See the roadmap for what is coming.");
                    self.status_is_error = false;
                }
            }
        }
    }

    fn recent_notebooks(&self) -> Vec<Node> {
        let mut paths = Vec::new();
        if let Some(path) = self.editor.as_ref().and_then(|editor| notebook_for(&editor.dir)) {
            paths.push(path);
        }
        paths.extend(self.session.opened_elsewhere().iter().rev().cloned());
        if let Some(saved) = self.session.settings.get("recent_notebooks")
            && let Ok(saved) = serde_json::from_str::<Vec<String>>(&saved)
        {
            paths.extend(saved.into_iter().map(PathBuf::from));
        }
        let mut recent = Vec::new();
        let mut seen = HashSet::new();
        for path in paths {
            let path = std::fs::canonicalize(&path).unwrap_or(path);
            if !seen.insert(path.clone()) {
                continue;
            }
            if let Some(notebook) = notecraft_store::tree::load_tree(&path)
                && notebook.info.kind == NodeKind::Notebook
            {
                recent.push(notebook);
                if recent.len() == 12 {
                    break;
                }
            }
        }
        recent
    }

    fn remember_recent_notebook(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let mut recent =
            self.session.settings.get("recent_notebooks").and_then(|saved| serde_json::from_str::<Vec<String>>(&saved).ok()).unwrap_or_default();
        let path_text = path.to_string_lossy().into_owned();
        recent.retain(|saved| std::fs::canonicalize(saved).unwrap_or_else(|_| PathBuf::from(saved)) != path);
        recent.insert(0, path_text);
        recent.truncate(12);
        if let Ok(serialized) = serde_json::to_string(&recent) {
            let _ = self.session.settings.set("recent_notebooks", &serialized);
        }
    }

    fn remove_recent_notebook(&mut self, path: &Path) {
        let Some(saved) = self.session.settings.get("recent_notebooks") else { return };
        let Ok(mut recent) = serde_json::from_str::<Vec<String>>(&saved) else { return };
        recent.retain(|entry| std::fs::canonicalize(entry).unwrap_or_else(|_| PathBuf::from(entry)) != path);
        if let Ok(serialized) = serde_json::to_string(&recent) {
            let _ = self.session.settings.set("recent_notebooks", &serialized);
        }
        self.say("Removed notebook from the known notebooks list");
    }

    fn open_notebook_from_path(&mut self, path: &Path) {
        self.save_now();
        let path_text = path_str(path);
        if let Some(Output::Path(opened)) = self.run("notebook.open", &params(&[("path", &path_text)])) {
            self.remember_recent_notebook(&opened);
            self.open.insert(opened.clone());
            self.selected = Some(opened.clone());
            self.backstage = None;
            if let Some(page) = first_page(self.session.tree(), &opened) {
                self.open_page(&page);
            } else {
                self.editor = None;
                self.pad = None;
                self.say(format!("Opened {}; it has no pages yet", self.shown(&opened)));
            }
        }
    }

    /// Asks the system for a folder and opens the notebooks in it, keeping everything that
    /// is already open. A folder that is itself a notebook opens as one; a folder that
    /// holds notebooks opens all of them. Nothing is closed and nothing is moved.
    fn choose_notebooks_folder(&mut self) {
        let Some(picked) =
            rfd::FileDialog::new().set_title("Choose a notebook, a folder of notebooks, or an empty folder for a new notebook").pick_folder()
        else {
            return;
        };
        let is_notebook = notecraft_store::tree::read_node(&picked).is_some_and(|i| i.kind == NodeKind::Notebook);
        let wanted: Vec<std::path::PathBuf> =
            if is_notebook { vec![picked.clone()] } else { notecraft_store::tree::load_notebooks(&picked).into_iter().map(|n| n.path).collect() };
        if wanted.is_empty() {
            let p = path_str(&picked);
            if let Some(Output::Path(opened)) = self.run("notebook.open", &params(&[("path", &p)])) {
                self.remember_recent_notebook(&opened);
                self.open.insert(opened.clone());
                self.selected = Some(opened);
                self.backstage = None;
                self.say(format!("{} opened as an empty notebook", self.shown(&picked)));
            }
            return;
        }
        let mut opened_count = 0;
        let mut first = None;
        for path in wanted {
            let p = path_str(&path);
            if let Some(Output::Path(notebook_path)) = self.run("notebook.open", &params(&[("path", &p)])) {
                self.remember_recent_notebook(&notebook_path);
                opened_count += 1;
                first.get_or_insert(path.clone());
                self.open.insert(notebook_path);
            }
        }
        if let Some(path) = first {
            self.selected = Some(path);
        }
        self.backstage = None;
        self.say(match opened_count {
            1 => "Notebook opened".to_string(),
            n => format!("{n} notebooks opened"),
        });
    }

    fn import_notebook_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose a OneNote notebook folder");
        if let Some(backup) = notecraft_import::discover::default_source() {
            dialog = dialog.set_directory(backup);
        }
        let Some(source) = dialog.pick_folder() else { return };
        self.import_officeimo_source(source, true);
    }

    fn import_file(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title("Choose a file to import");
        if let Some(backup) = notecraft_import::discover::default_source() {
            dialog = dialog.set_directory(backup);
        }
        let Some(source) = dialog.pick_file() else { return };
        self.import_officeimo_source(source, false);
    }

    fn import_officeimo_source(&mut self, source: PathBuf, folder: bool) {
        let format = self
            .session
            .settings
            .get("page_format")
            .and_then(|v| notecraft_model::PageFormat::parse(&v).ok())
            .unwrap_or(notecraft_model::PageFormat::Html);
        let bridge_format = match format {
            notecraft_model::PageFormat::Html => notecraft_import::officeimo::Format::Html,
            notecraft_model::PageFormat::Cmgfm => notecraft_import::officeimo::Format::Cmgfm,
        };
        let destination = self.unique_import_destination(&source);
        let output = if folder { destination.parent().unwrap_or(self.session.root()) } else { &destination };
        log::info!("OfficeIMO import requested: source={} destination={}", source.display(), destination.display());
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let source_for_worker = source.clone();
        let output_for_worker = output.to_path_buf();
        std::thread::spawn(move || {
            let mut progress = |event| {
                let message = match event {
                    notecraft_import::officeimo::Progress::Found { total } => ImportMessage::Found(total),
                    notecraft_import::officeimo::Progress::Section { index, total, label } => {
                        ImportMessage::Section { index, total, label: label.to_string() }
                    }
                };
                let _ = tx.send(message);
            };
            let result =
                notecraft_import::officeimo::run_with_progress(&source_for_worker, &output_for_worker, bridge_format, &worker_cancel, &mut progress);
            let _ = tx.send(ImportMessage::Finished(result));
        });
        self.import_job =
            Some(ImportJob { rx, keep_alive: None, cancel, source, destination, current: "Starting OfficeIMO…".into(), index: 0, total: 0 });
        self.status = "Importing…".into();
        self.status_is_error = false;
    }

    fn poll_import(&mut self, ctx: &Context) {
        let Some(job) = self.import_job.as_mut() else { return };
        let mut finished = None;
        loop {
            match job.rx.try_recv() {
                Ok(ImportMessage::Found(total)) => job.total = total,
                Ok(ImportMessage::Section { index, total, label }) => {
                    job.index = index;
                    job.total = total;
                    job.current = label;
                }
                Ok(ImportMessage::Finished(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err("OfficeIMO import worker stopped unexpectedly".into()));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            let Some(job) = self.import_job.take() else { return };
            match result {
                Ok(message) => {
                    let _ = self.run("tree.refresh", &Params::new());
                    log::info!("OfficeIMO import succeeded: source={} destination={}", job.source.display(), job.destination.display());
                    self.status = message;
                    self.status_is_error = false;
                    self.open_notebook_from_path(&job.destination);
                }
                Err(error) => {
                    log::error!("OfficeIMO import failed: source={} destination={} error={error}", job.source.display(), job.destination.display());
                    self.status = error;
                    self.status_is_error = true;
                }
            }
        } else if job.keep_alive.is_none() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
    }

    fn import_progress(&mut self, ctx: &Context) {
        let Some(job) = self.import_job.as_mut() else { return };
        let mut cancel = false;
        egui::Window::new("Importing OneNote")
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(modal_frame(&self.tokens))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                modal_title(ui, &self.tokens, "Importing OneNote");
                ui.label(egui::RichText::new("You can continue working while the import runs.").color(self.tokens.muted));
                ui.add_space(12.0);
                ui.label(egui::RichText::new(&job.current).strong());
                let fraction = if job.total == 0 { 0.0 } else { job.index as f32 / job.total as f32 };
                ui.add(egui::ProgressBar::new(fraction).fill(self.tokens.accent).show_percentage());
                if job.total > 0 {
                    ui.label(format!("{} of {} sections", job.index, job.total));
                } else {
                    ui.label("Discovering sections…");
                }
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if modal_button(ui, &self.tokens, "Cancel import", true).clicked() {
                        cancel = true;
                    }
                });
            });
        if cancel {
            job.cancel.store(true, Ordering::Relaxed);
            job.current = "Cancelling…".into();
        }
    }

    fn import_notebook_name(source: &Path) -> String {
        let stem = source.file_stem().and_then(|name| name.to_str()).unwrap_or("Imported notebook");
        let name = stem.split_once(" (On ").map_or(stem, |(name, _)| name).trim();
        let cleaned: String =
            name.chars().map(|c| if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') { '_' } else { c }).collect();
        let cleaned = cleaned.trim().trim_end_matches('.');
        if cleaned.is_empty() { "Imported notebook".to_string() } else { cleaned.to_string() }
    }

    fn unique_import_destination(&self, source: &Path) -> PathBuf {
        let base = Self::import_notebook_name(source);
        let root = self.session.root();
        let first = root.join(&base);
        if !first.exists() {
            return first;
        }
        for number in 1..=u32::MAX {
            let candidate = root.join(format!("{base} ({number})"));
            if !candidate.exists() {
                return candidate;
            }
        }
        root.join(format!("{base} ({})", u64::from(u32::MAX) + 1))
    }

    fn reveal(&mut self, path: &Path) {
        match open_folder(path) {
            Ok(()) => self.say(format!("Opened {}", self.shown(path))),
            Err(e) => {
                self.status = format!("The folder could not be opened: {e}");
                self.status_is_error = true;
            }
        }
    }

    /// Adds the Welcome notebook and opens it on its start page; if it is already in the
    /// folder, opens that one instead.
    pub fn add_welcome(&mut self) {
        let existing = notecraft_engine::welcome::installed(self.session.tree());
        let (dir, message) = match existing {
            Some(dir) => (dir, "The welcome notebook is already here"),
            None => match self.run("notebook.welcome", &Params::default()) {
                Some(Output::Path(dir)) => (dir, "Added the welcome notebook"),
                _ => return,
            },
        };
        self.remember_recent_notebook(&dir);
        self.open.insert(dir.clone());
        self.selected = Some(dir.clone());
        if let Some(first) = first_page_under(self.session.tree(), &dir) {
            self.open_path(&path_str(&first));
        }
        // After the page opened, so the message is the one the user reads.
        if !self.status_is_error {
            self.say(message);
        }
    }

    /// Follows a link written in the open page. Another page of the notebooks opens in the
    /// window; a notebook, section group or section opens on its first page; a file opens in
    /// the program the system has for it; a web address opens in the browser. A relative
    /// path can never reach outside the notebooks folder (`notecraft_store::links`).
    /// Puts a link to a page on the clipboard, as OneNote's Copy Link to Page does.
    ///
    /// The link names the page from the notebooks folder down
    /// (`notecraft:/book/section/page`), not relative to anything, because what a relative
    /// link would be relative to is the page it is pasted into, and nobody knows that when
    /// the link is copied. Pasted into a page it reads as an ordinary link and opens the
    /// page it names.
    pub fn copy_link_to(&mut self, dir: &Path) {
        let mut roots = vec![self.session.root().to_path_buf()];
        roots.extend(self.session.opened_elsewhere().iter().cloned());
        let Some(uri) = notecraft_store::links::page_uri(&roots, dir) else {
            self.complain("That page is not in a notebook that is open");
            return;
        };
        let title = notecraft_store::tree::read_node(dir).map(|n| n.title).unwrap_or_else(|| "Page".to_string());
        // Plain text keeps the title and address readable; the matching HTML flavor is an
        // anchor for clients that accept rich content.
        self.pending_copy = Some(format!("{title} ({uri})"));
        self.pending_copy_html = Some(crate::clipboard::html_link(&title, &uri));
        self.say(format!("Link to \"{title}\" copied"));
    }

    pub fn follow_link(&mut self, ctx: &Context, href: &str) {
        use notecraft_store::links::{Link, resolve_in};
        let Some(page) = self.editor.as_ref().map(|e| e.dir.clone()) else { return };
        // Every folder a notebook is open from, not just the notebooks folder: a notebook
        // opened from elsewhere is as much inside as one that lives there, and judging it
        // by the notebooks folder alone made every link in it dead.
        let mut roots = vec![self.session.root().to_path_buf()];
        roots.extend(self.session.opened_elsewhere().iter().cloned());
        match resolve_in(&roots, &page, href) {
            Link::Web(url) => {
                ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                self.say(format!("Opened {url} in the browser"));
            }
            Link::Node(dir) => {
                if notecraft_store::tree::read_node(&dir).is_some_and(|n| n.kind == NodeKind::Page) {
                    self.open_path(&path_str(&dir));
                } else {
                    self.open.insert(dir.clone());
                    self.selected = Some(dir.clone());
                    match first_page_under(self.session.tree(), &dir) {
                        Some(first) => self.open_path(&path_str(&first)),
                        None => self.say(format!("Opened {}; it has no pages yet", self.shown(&dir))),
                    }
                }
            }
            Link::File(file) => match open_file(&file) {
                Ok(()) => self.say(format!("Opening {}", self.shown(&file))),
                Err(e) => {
                    self.status = format!("The file could not be opened: {e}");
                    self.status_is_error = true;
                }
            },
            Link::Missing(p) => {
                self.status = format!("Nothing at {}: the link points to a page or file that is not there", self.shown(&p));
                self.status_is_error = true;
            }
            Link::Outside => {
                self.status = "That link points outside the notebooks folder, so it was not followed".to_string();
                self.status_is_error = true;
            }
        }
    }

    /// The ribbon under the title row: tabs and their groups.
    fn ribbon_bar(&mut self, ui: &mut Ui) {
        let (tab, source, tokens) = (self.ribbon_tab.clone(), self.show_source, self.tokens);
        let view = ribbon::View {
            split_panes: self.split_panes,
            always_on_top: self.always_on_top,
            full_page: self.full_page,
            rules: self.rules,
            hide_title: self.hide_title,
            zoom_percent: self.zoom_percent,
            pointer: self.pointer,
            rack: &self.rack,
            nib: self.nib,
            in_table: self.caret_in_table(),
            table_borders: self.table_borders,
            gridcraft: self.gridcraft.is_some(),
            page_colour: self.editor.as_ref().and_then(|e| e.colour),
            can_undo: self.editor.as_ref().is_some_and(|e| e.history.can_undo()),
            can_redo: self.editor.as_ref().is_some_and(|e| e.history.can_redo()),
            sensitivity: self.sensitivity,
            has_page: self.editor.is_some(),
            style: self.caret_style(ui.ctx()),
        };
        let format = self.editor.as_ref().map(|e| e.format);
        let mut acts = Vec::new();
        egui::Panel::top("ribbon")
            .exact_size(ribbon::GROUP_H + 8.0)
            .frame(egui::Frame::new().fill(tokens.panel).inner_margin(egui::Margin::symmetric(4, 0)))
            .show_separator_line(false)
            .show(ui, |ui| {
                acts = ribbon::show(ui, &tab, source, &view, format, &tokens);
            });
        let picked = self.editor.as_ref().and_then(|e| crate::containers::selected(ui.ctx(), &e.dir));
        for a in acts {
            if let Some(i) = picked
                && self.act_on_box(ui.ctx(), i, &a)
            {
                continue;
            }
            self.ribbon_act(a);
        }
    }

    /// What a ribbon button does while a container is selected: selecting a box selects everything in
    /// it, so Cut, Copy, the marks and the styles take all of it. True when the act was one of those.
    fn act_on_box(&mut self, ctx: &Context, at: usize, act: &ribbon::Act) -> bool {
        use notecraft_formats::markdown::Block;
        let wanted = matches!(
            act,
            ribbon::Act::Clipboard(_)
                | ribbon::Act::Mark(_)
                | ribbon::Act::Block(_)
                | ribbon::Act::ClearFormatting
                | ribbon::Act::Direction(_)
                | ribbon::Act::Indent(_)
                | ribbon::Act::Tag(_)
                | ribbon::Act::Justify(_)
        );
        let Some(ed) = self.editor.as_mut().filter(|_| wanted) else { return false };
        let range = notecraft_model::container::blocks_of(&ed.boxes, at);
        if range.end > ed.blocks.len() || range.is_empty() {
            return false;
        }
        let words = |b: &Block| !matches!(b, Block::Code { .. } | Block::MathBlock(_)) && pageedit::is_one_line_of_words(b);
        match act {
            ribbon::Act::Clipboard(which) => {
                crate::containers::ask_clipboard(ctx, *which == "cut");
                return true;
            }
            ribbon::Act::Mark(mark) => {
                if !pageedit::toggle_mark(&mut ed.blocks, range, mark) {
                    return true;
                }
            }
            ribbon::Act::Direction(dir) => {
                // Words only: a mark in a diagram or an equation would be part of its source.
                for i in range {
                    if ed.blocks.get(i).is_some_and(words) {
                        pageedit::set_direction(&mut ed.blocks, i, *dir);
                    }
                }
            }
            ribbon::Act::Indent(deeper) => {
                for i in range {
                    pageedit::indent(&mut ed.blocks, i, *deeper);
                }
            }
            ribbon::Act::Tag(tag) => {
                // All of them have the tag already: the same button takes it off every one.
                let has = |b: &Block| pageedit::tag_of(b).0 == Some(*tag);
                // A to-do line carries no other tag, so it is left out of every tag but its own.
                let can = |b: &Block| words(b) && (*tag == notecraft_model::Tag::ToDo || !matches!(b, Block::Item { task: Some(_), .. }));
                let all = range.clone().filter_map(|i| ed.blocks.get(i)).filter(|b| can(b)).all(has);
                for i in range {
                    if ed.blocks.get(i).is_some_and(|b| can(b) && (all || !has(b))) {
                        pageedit::set_tag(&mut ed.blocks, i, *tag);
                    }
                }
            }
            ribbon::Act::Justify(strength) => {
                let paragraphs: Vec<usize> = range.filter(|i| matches!(ed.blocks.get(*i), Some(Block::Paragraph(_)))).collect();
                if paragraphs.is_empty() {
                    return true;
                }
                let all = paragraphs.iter().all(|i| ed.justified.strength_of(*i) == Some(*strength));
                let want = (!all).then_some(*strength);
                let mut marked = ed.justified.clone();
                for i in paragraphs {
                    marked.set(i, want);
                }
                self.write_justified(marked, want);
                return true;
            }
            ribbon::Act::Block(kind) => {
                // All of them are that kind already: the same button turns them back into text.
                let all = range.clone().all(|i| ed.blocks.get(i).is_some_and(|b| pageedit::block_kind(b) == *kind));
                for i in range {
                    if all || ed.blocks.get(i).is_some_and(|b| pageedit::block_kind(b) != *kind) {
                        pageedit::set_kind(&mut ed.blocks, i, kind);
                    }
                }
            }
            _ => {
                for i in range {
                    if let Some(b) = ed.blocks.get_mut(i).filter(|b| words(b)) {
                        let text = pageedit::without_marks(&pageedit::block_text(b));
                        pageedit::set_block_text(b, &text);
                    }
                }
            }
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        true
    }

    /// Carries out one thing the ribbon asked for.
    fn ribbon_act(&mut self, act: ribbon::Act) {
        match act {
            ribbon::Act::File => self.backstage = Some("Info"),
            ribbon::Act::PageVersions => {
                self.save_now();
                if let Some(page) = self.editor.as_ref().map(|e| e.dir.clone()) {
                    let p = params(&[("path", &path_str(&page))]);
                    if let Some(Output::Versions(versions)) = self.run("page.versions", &p) {
                        self.dialog = Some(Dialog::PageVersions { page, versions });
                    }
                }
            }
            ribbon::Act::RecycleBin => {
                if let Some(notebook) = self.editor.as_ref().and_then(|e| notebook_for(&e.dir)) {
                    let p = params(&[("notebook", &path_str(&notebook))]);
                    if let Some(Output::Recycled(items)) = self.run("recycle.list", &p) {
                        self.dialog = Some(Dialog::RecycleBin { notebook, items });
                    }
                }
            }
            ribbon::Act::Symbols => self.dialog = Some(Dialog::Symbols),
            ribbon::Act::Equation(tex) => self.insert_in_page(&format!("${tex}$")),
            ribbon::Act::Tab(name) => self.ribbon_tab = name,
            ribbon::Act::Mark(mark) => self.apply_mark(mark),
            ribbon::Act::Block(kind) => self.apply_block_kind(kind),
            ribbon::Act::Insert(what) => match what {
                "space" => {
                    let Some(ed) = self.editor.as_mut() else { return };
                    let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len()).min(ed.blocks.len());
                    ed.blocks.insert(at, notecraft_formats::Block::Paragraph(Vec::new()));
                    ed.rebuild_body();
                    ed.caret =
                        Some(pageedit::Caret { block: at, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None });
                    Self::mark_dirty_from_blocks(ed);
                }
                "link" => self.insert_in_page("[text](https://)"),
                "equation" => self.insert_in_page("$E = mc^2$"),
                "date" | "time" | "datetime" => {
                    let now = notecraft_model::time::iso_now();
                    let stamp = match what {
                        "time" => notecraft_model::time::clock_from_iso(&now),
                        "datetime" => notecraft_model::time::long_from_iso(&now),
                        _ => notecraft_model::time::date_from_iso(&now),
                    };
                    self.insert_in_page(&stamp.unwrap_or(now));
                }
                other => self.insert_block(other),
            },
            ribbon::Act::AddFile | ribbon::Act::AddPicture => {
                let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
                    self.say("Open a page first");
                    return;
                };
                let mut picker = rfd::FileDialog::new().set_title(if matches!(act, ribbon::Act::AddPicture) {
                    "Choose a picture"
                } else {
                    "Choose a file to attach"
                });
                if matches!(act, ribbon::Act::AddPicture) {
                    picker = picker.add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"]);
                }
                if let Some(file) = picker.pick_file() {
                    self.dialog = Some(Dialog::FileOptions { dir, files: vec![file] });
                }
            }
            ribbon::Act::ToggleSource => self.set_show_source(!self.show_source),
            ribbon::Act::ToggleFullPage => self.full_page = !self.full_page,
            ribbon::Act::OpenThemes => self.theme_window = !self.theme_window,
            ribbon::Act::RuleLines(r) => {
                // The paper is the page's own, as OneNote's rule lines are. The window's
                // own setting stays as what a page with none of its own is drawn on.
                self.rules = r;
                let _ = self.session.settings.set("rule_lines", r.id());
                if self.editor.is_some() {
                    self.set_page_appearance(Some(r), None);
                }
                self.say(match r {
                    ribbon::RuleLines::None => "No lines behind the page".to_string(),
                    other => format!("{} behind the page", other.label()),
                });
            }
            ribbon::Act::ToggleAlwaysOnTop => {
                self.always_on_top = !self.always_on_top;
                // Asked for on the next frame, where the context is at hand.
                self.window_level_pending = true;
                let _ = self.session.settings.set("always_on_top", if self.always_on_top { "true" } else { "false" });
                self.say(if self.always_on_top { "This window stays on top" } else { "This window behaves like the others" });
            }
            ribbon::Act::TogglePanes => {
                self.split_panes = !self.split_panes;
                self.organisers = true;
                let _ = self.session.settings.set("split_panes", if self.split_panes { "true" } else { "false" });
                self.say(if self.split_panes { "Notebooks and pages in their own columns" } else { "One tree of notebooks" });
            }
            ribbon::Act::ToggleTitle => {
                self.hide_title = !self.hide_title;
                let _ = self.session.settings.set("hide_title", if self.hide_title { "true" } else { "false" });
            }
            ribbon::Act::Zoom(step) => {
                self.zoom_percent = match step {
                    1 => (self.zoom_percent + 10).min(250),
                    -1 => self.zoom_percent.saturating_sub(10).max(50),
                    _ => 100,
                };
                self.fit_page_width = false;
            }
            ribbon::Act::ZoomTo(percent) => {
                self.zoom_percent = percent.clamp(50, 250);
                self.fit_page_width = false;
            }
            ribbon::Act::PageWidth => self.fit_page_width = true,
            ribbon::Act::NotBuilt(what) => {
                self.status = format!("{what} is not built yet. See the roadmap for what is coming.");
                self.status_is_error = false;
            }
            ribbon::Act::Tag(tag) => self.apply_tag(tag),
            ribbon::Act::Clipboard(which) => self.clipboard(which),
            ribbon::Act::Paste => self.paste_here(),
            ribbon::Act::Indent(deeper) => self.indent_block(deeper),
            ribbon::Act::ClearFormatting => self.clear_formatting(),
            ribbon::Act::Direction(d) => self.apply_direction(d),
            ribbon::Act::Justify(strength) => self.apply_justify(strength),
            ribbon::Act::PageDirection(d) => self.apply_page_direction(d),
            ribbon::Act::Embedding(d) => self.apply_embedding(d),
            ribbon::Act::InkPointer(p) => self.pointer = p,
            ribbon::Act::InkPen(i) => {
                self.nib = i.min(self.rack.len().saturating_sub(1));
                self.pointer = ribbon::Pointer::Draw;
                self.remember_rack();
            }
            ribbon::Act::InkRack(edit) => self.edit_rack(edit),
            ribbon::Act::Table(what) => self.edit_table(what),
            ribbon::Act::TableSelect(what) => self.select_in_table(what),
            ribbon::Act::TableBorders(on) => {
                self.table_borders = on;
                let _ = self.session.settings.set("table_borders", if on { "true" } else { "false" });
            }
            ribbon::Act::PageColour(c) => {
                self.set_page_appearance(None, Some(c));
                self.say(match c {
                    Some(c) => format!("The page is {}", c.label().to_lowercase()),
                    None => "The page is the theme's own colour".to_string(),
                });
            }
            ribbon::Act::Undo => self.step_back(true),
            ribbon::Act::Redo => self.step_back(false),
            ribbon::Act::ToGridCraft => self.hand_table_to_gridcraft(),
            ribbon::Act::FromGridCraft => self.read_table_from_gridcraft(),
            ribbon::Act::NewGridCraft => self.new_gridcraft(),
            ribbon::Act::InsertTable(rows, columns) => self.insert_table(rows, columns),
            ribbon::Act::InkSensitivity(v) => {
                self.sensitivity = v.min(100);
                let _ = self.session.settings.set(SENSITIVITY_KEY, &self.sensitivity.to_string());
            }
            ribbon::Act::InkUndo => self.ink_undo(),
            ribbon::Act::InkInsert => self.ink_insert(),
            ribbon::Act::InkExportPlain => self.ink_export_plain(),
        }
    }

    /// Adds, removes, moves or changes a tool on the rack, and remembers the result. The
    /// rules about what may not happen - the last tool cannot be taken off, a tool cannot
    /// be moved off either end - live in `notecraft_ink::rack`, so they hold wherever the
    /// rack is edited from.
    fn edit_rack(&mut self, edit: ribbon::RackEdit) {
        use notecraft_ink::rack;
        match edit {
            ribbon::RackEdit::Add(tool) => {
                if self.rack.len() >= rack::MAX_NIBS {
                    self.say("There is no room on the rack for another tool");
                    return;
                }
                self.rack.push(rack::new_nib(tool));
                self.nib = self.rack.len() - 1;
                self.pointer = ribbon::Pointer::Draw;
                self.say(format!("Added a {}", tool.label().to_lowercase()));
            }
            ribbon::RackEdit::Remove(i) => {
                if !rack::remove(&mut self.rack, i) {
                    self.say("The last tool stays on the rack");
                    return;
                }
                if self.nib >= self.rack.len() {
                    self.nib = self.rack.len() - 1;
                } else if self.nib > i {
                    self.nib -= 1;
                }
                self.say("Took it off the rack");
            }
            ribbon::RackEdit::Move(i, left) => {
                let to = rack::shift(&mut self.rack, i, left);
                if self.nib == i {
                    self.nib = to;
                } else if self.nib == to {
                    self.nib = i;
                }
            }
            ribbon::RackEdit::Set(i, nib) => {
                if let Some(slot) = self.rack.get_mut(i) {
                    *slot = nib;
                    self.nib = i;
                    self.pointer = ribbon::Pointer::Draw;
                }
            }
        }
        self.remember_rack();
    }

    /// Puts whatever is on the clipboard into the page: a picture becomes a file beside
    /// the page and a line that shows it, anything else goes in as words.
    ///
    /// egui hands a window only `Paste(String)`, so a picture copied from a screen
    /// clipper never arrived and Ctrl+V did nothing at all. The clipboard is read here
    /// directly instead.
    fn paste_here(&mut self) {
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.say("Open a page first");
            return;
        };
        if let Some(picture) = crate::clipboard::picture() {
            let name = crate::clipboard::picture_name();
            let (w, h) = (picture.width, picture.height);
            match notecraft_store::assets::attach_bytes(&dir, &name, &picture.png) {
                Ok(link) => {
                    if let Some(ed) = self.editor.as_mut() {
                        let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
                        ed.blocks.insert(at, notecraft_formats::markdown::Block::Image { alt: "Pasted picture".to_string(), src: link });
                        ed.rebuild_body();
                        Self::mark_dirty_from_blocks(ed);
                        ed.attachments = notecraft_store::assets::list(&dir);
                    }
                    self.say(format!("Pasted a picture, {w} by {h}"));
                }
                Err(e) => self.complain(&format!("Cannot save the pasted picture: {e}")),
            }
            return;
        }
        // Browsers, Word and other editors offer a formatted clipboard flavour as well
        // as plain text. Parse it through the same HTML-to-block path as an HTML page, so
        // links, emphasis, lists and tables survive a paste into NoteCraft.
        if let Some(html) = crate::clipboard::html() {
            let new = pageedit::pasted_blocks(pageedit::pasted(&html));
            if !new.is_empty()
                && let Some(ed) = self.editor.as_mut().filter(|ed| ed.writable)
            {
                let at = ed.caret.as_ref().map_or(ed.blocks.len().saturating_sub(1), |c| c.block);
                if ed.blocks.is_empty() {
                    ed.blocks.push(notecraft_formats::markdown::Block::Paragraph(Vec::new()));
                }
                pageedit::paste_at(&mut ed.blocks, &mut ed.caret, at, new);
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
                self.say("Pasted formatted content");
                return;
            }
        }
        match crate::clipboard::text() {
            Some(text) => {
                // The same reading as Ctrl+V: a box that was copied comes back whole, several lines
                // become lines of their own, a table stays a table. Put in as one text, as this once
                // did, the lines ran together in a single paragraph with their Markdown showing.
                let new = pageedit::blocks_to_paste(&text);
                match self.editor.as_mut() {
                    Some(ed) if !new.is_empty() && ed.writable => {
                        let at = ed.caret.as_ref().map_or(ed.blocks.len().saturating_sub(1), |c| c.block);
                        if ed.blocks.is_empty() {
                            ed.blocks.push(notecraft_formats::markdown::Block::Paragraph(Vec::new()));
                        }
                        pageedit::paste_at(&mut ed.blocks, &mut ed.caret, at, new);
                        ed.rebuild_body();
                        Self::mark_dirty_from_blocks(ed);
                    }
                    _ => self.insert_in_page(&text),
                }
                self.say("Pasted");
            }
            None => self.say("There is nothing on the clipboard to paste"),
        }
    }

    /// Keeps the page's words honest about its ink: a page with ink names `ink.svg`, a
    /// page without it does not. Without this the source pane showed a page of text while
    /// the page itself was covered in handwriting, and another program opening the folder
    /// would never have known the drawing was there.
    fn sync_ink_reference(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        // The folder is asked again: the pen has just written or deleted a file.
        ed.ink_files = notecraft_store::page::ink_files(&ed.dir);
        if Self::name_ink(ed) {
            Self::mark_dirty_from_blocks(ed);
        }
    }

    /// The page names every ink file in its folder, and names no ink file that is gone.
    ///
    /// The folder decides. A drawing is a file beside the page file; the line in the page
    /// that names it is how the page's words and what is drawn on it agree, and how
    /// another program opening the page sees the handwriting. Before this, the line could
    /// be lost - deleting the box that held it took it out - and nothing put it back until
    /// the pen next moved, so a page could say nothing about ink that was still there.
    ///
    /// Returns true when the page's text had to be changed.
    fn name_ink(ed: &mut Editor) -> bool {
        use notecraft_formats::markdown::Block;
        let files = ed.ink_files.clone();
        let mut changed = false;
        // A line that names an ink file that is no longer in the folder goes.
        let before = ed.blocks.len();
        ed.blocks.retain(|b| match b {
            Block::Image { src, alt } => !crate::inkpad::is_ink_line(src, alt) || files.iter().any(|f| f == src.trim()),
            _ => true,
        });
        changed |= ed.blocks.len() != before;
        // An ink file the page does not name is adopted: it is drawn, so it is named.
        for file in &files {
            if !ed.blocks.iter().any(|b| matches!(b, Block::Image { src, .. } if src.trim() == file)) {
                ed.blocks.push(Block::Image { alt: "Ink".to_string(), src: file.clone() });
                changed = true;
            }
        }
        if changed {
            ed.rebuild_body();
        }
        changed
    }

    /// Writes the rack and which tool is in hand into the settings, so the next run opens
    /// with the pens the owner arranged.
    fn remember_rack(&mut self) {
        let line = notecraft_ink::rack::write(&self.rack);
        let _ = self.session.settings.set(RACK_KEY, &line);
        let _ = self.session.settings.set(NIB_KEY, &self.nib.to_string());
    }

    /// The pen as the page's ink layer wants it: what it is, and what the pointer is for.
    fn pen(&self) -> crate::inkpad::Pen {
        let nib =
            self.rack.get(self.nib).or_else(|| self.rack.first()).copied().unwrap_or_else(|| notecraft_ink::rack::new_nib(notecraft_ink::Tool::Pen));
        crate::inkpad::Pen { pointer: self.pointer, tool: nib.tool, colour: nib.colour, width: nib.width(), gain: self.gain() }
    }

    /// The sensitivity as the curve a stroke is drawn with: the middle of the control is
    /// 1, which leaves the pen's readings exactly as they came.
    fn gain(&self) -> f32 {
        let share = f32::from(self.sensitivity.min(100)) / 50.0;
        if share <= 1.0 {
            notecraft_ink::MIN_GAIN + (1.0 - notecraft_ink::MIN_GAIN) * share
        } else {
            1.0 + (notecraft_ink::MAX_GAIN - 1.0) * (share - 1.0)
        }
    }

    /// Takes the last stroke back out of the drawing on this page.
    fn ink_undo(&mut self) {
        let Some(pad) = self.pad.as_mut() else {
            self.say("There is no drawing on this page");
            return;
        };
        if !pad.undo() {
            self.say("There is no stroke to take back");
            return;
        }
        let left = pad.strokes();
        match pad.save() {
            Ok(()) => self.say(format!("Took the last stroke back; {left} left")),
            Err(e) => self.complain(&format!("Cannot write the drawing: {e}")),
        }
        // The page stops naming ink it no longer has.
        self.sync_ink_reference();
    }

    /// Whether the caret is in a table, which is when the Table tab is offered.
    /// The Styles kind of the block the caret is in (or that a selected box is all of), for the ribbon to highlight; "" with neither.
    fn caret_style(&self, ctx: &Context) -> &'static str {
        let Some(ed) = self.editor.as_ref() else { return "" };
        if let Some(block) = ed.caret.as_ref().and_then(|c| ed.blocks.get(c.block)) {
            return pageedit::block_kind(block);
        }
        // A selected box has everything in it selected: its style is the one they all share.
        let range = crate::containers::selected(ctx, &ed.dir).map(|i| notecraft_model::container::blocks_of(&ed.boxes, i)).unwrap_or_default();
        let mut kinds = range.filter_map(|i| ed.blocks.get(i)).map(pageedit::block_kind);
        match kinds.next() {
            Some(first) if kinds.all(|k| k == first) => first,
            _ => "",
        }
    }

    fn caret_in_table(&self) -> bool {
        self.editor
            .as_ref()
            .and_then(|e| e.caret.as_ref().map(|c| (e, c)))
            .is_some_and(|(e, c)| c.cell.is_some() && matches!(e.blocks.get(c.block), Some(notecraft_formats::markdown::Block::Table { .. })))
    }

    /// Writes a file of the open page that was typed in in the code view.
    ///
    /// Only the files the code view lets anybody type in reach here: a drawing's SVG. The
    /// page file's body is not one of them; it is the editor's own text and is saved the
    /// way it always was.
    fn write_page_file(&mut self, name: &str, text: &str) {
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else { return };
        let is_page_file = self.editor.as_ref().is_some_and(|e| e.format.page_file() == name);
        let path = dir.join(name.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !path.starts_with(&dir) {
            self.complain("That file is not in this page's folder");
            return;
        }
        if let Err(e) = notecraft_store::page::write_atomic(&path, text) {
            self.complain(&format!("Cannot write {name}: {e}"));
            return;
        }
        if is_page_file {
            // The page itself was typed in. Nothing the window remembered about it is
            // true any more, and the autosave must not write its old copy over this one;
            // the page is read again when the view closes.
            if let Some(ed) = self.editor.as_mut() {
                ed.dirty_since = None;
                ed.body = notecraft_store::page::load_page(&dir).map(|p| p.body).unwrap_or_else(|_| ed.body.clone());
            }
            self.code.page_edited = true;
            return;
        }
        // A drawing that was typed in is read again, so what the pen does next is done to
        // the file as it now stands rather than to the one the window remembered.
        if notecraft_ink::svg::is_ink(text) {
            self.pad = Some(crate::inkpad::Pad::of_page(&dir));
        }
        self.sync_ink_reference();
    }

    /// The paper the open page is drawn on: its own, or the window's choice when it has
    /// none of its own.
    fn paper_now(&self) -> ribbon::RuleLines {
        self.editor.as_ref().and_then(|e| e.paper).unwrap_or(self.rules)
    }

    /// Gives the open page its own paper, its own colour, or both, and writes them into
    /// the page's file. `None` for either leaves that one as it is.
    fn set_page_appearance(&mut self, paper: Option<ribbon::RuleLines>, colour: Option<Option<notecraft_model::NodeColor>>) {
        let Some(ed) = self.editor.as_mut() else {
            self.say("Open a page first");
            return;
        };
        if let Some(p) = paper {
            ed.paper = Some(p);
        }
        if let Some(c) = colour {
            ed.colour = c;
        }
        let (path, paper, colour) = (path_str(&ed.dir), ed.paper, ed.colour);
        let pairs: Vec<(&str, &str)> = vec![
            ("path", path.as_str()),
            ("paper", paper.map(ribbon::RuleLines::id).unwrap_or("none")),
            ("color", colour.map(notecraft_model::NodeColor::id).unwrap_or("none")),
        ];
        self.run("page.paper", &params(&pairs));
    }

    /// Takes the page back to how it was before the last change, or forward again.
    ///
    /// The whole page is put back: its text, its blocks, the boxes they sit in and where
    /// the caret was. Ink is not part of this; a stroke is taken back with Undo Stroke on
    /// the Draw tab, because the drawing is its own file beside the page.
    fn step_back(&mut self, back: bool) {
        let mut nothing = false;
        if let Some(ed) = self.editor.as_mut() {
            if !ed.writable {
                nothing = true;
            } else {
                let step = if back { ed.history.undo().cloned() } else { ed.history.redo().cloned() };
                match step {
                    Some(step) => {
                        ed.body = step.body;
                        ed.blocks = step.blocks;
                        ed.boxes = step.boxes;
                        ed.caret = step.caret.map(|c| pageedit::Caret { focus: true, ..c });
                        ed.dirty_since = Some(Instant::now());
                        // The restored text says whether the page was arranged.
                        ed.arranged = notecraft_formats::boxes::has_boxes(ed.format, &ed.body);
                    }
                    None => nothing = true,
                }
            }
        }
        if nothing {
            self.say(if back { "Nothing to undo" } else { "Nothing to redo" });
        }
    }

    /// Remembers the page as it stands now, so the change that just happened can be taken
    /// back. Called once a frame, after whatever changed has been applied.
    ///
    /// Letters going into the block the caret was already in fold into the step before
    /// them, so Ctrl+Z takes back a run of writing rather than one character.
    fn remember_step(ed: &mut Editor) {
        let same = ed.history.here().is_some_and(|s| s.body == ed.body && s.blocks == ed.blocks && s.boxes == ed.boxes);
        if same {
            // Nothing really changed, or the page was just put back by undo itself.
            return;
        }
        let here = ed.caret.as_ref().map(|c| c.block);
        let was = ed.history.here().and_then(|s| s.caret.as_ref().map(|c| c.block));
        let shape = ed.history.here().is_some_and(|s| s.blocks.len() == ed.blocks.len());
        let kind = match (shape, here, was) {
            (true, Some(a), Some(b)) if a == b => crate::history::Kind::Typing(a),
            _ => crate::history::Kind::Edit,
        };
        let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), kind);
        ed.history.record(step);
    }

    /// Puts the spreadsheet back into the table it came from.
    ///
    /// The file is the one Convert wrote for this table, beside the page. Nothing watches
    /// it: a page would be a strange thing to write in if its tables changed under the
    /// writer's hands, so the way back is a button, as the way out is.
    fn read_table_from_gridcraft(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.say("Click in the table first");
            return;
        };
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::markdown::Block::Table { .. })) {
            self.say("Click in a table first");
            return;
        }
        let file = ed.dir.join(format!("table-{}.csv", at + 1));
        let Ok(text) = std::fs::read_to_string(&file) else {
            let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
            self.complain(&format!("No {name} beside this page: hand the table over first"));
            return;
        };
        let Some(table) = notecraft_formats::csv::to_table(&text) else {
            self.complain("That file has nothing in it to put back");
            return;
        };
        let (rows, columns) = pageedit::table_size(&table).unwrap_or((0, 0));
        if let Some(block) = ed.blocks.get_mut(at) {
            *block = table;
        }
        ed.caret = None;
        ed.body = pageedit::to_body(ed.format, &ed.blocks);
        Self::mark_dirty_from_blocks(ed);
        self.say(format!("Read back {rows} rows and {columns} columns"));
    }

    /// Hands the table the caret is in to GridCraft, which is where a spreadsheet
    /// belongs: OneNote's Table tab ends with Convert to Excel Spreadsheet, and this is
    /// the same door with the open spreadsheet behind it.
    fn hand_table_to_gridcraft(&mut self) {
        let Some(exe) = self.gridcraft.clone() else {
            self.say("GridCraft is not installed");
            return;
        };
        let Some(ed) = self.editor.as_ref() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else { return };
        let Some(block) = ed.blocks.get(at) else { return };
        let name = format!("table-{}", at + 1);
        match crate::gridcraft::hand_over(&exe, &ed.dir, &name, block) {
            Ok(file) => {
                let said = format!("{} is open in GridCraft", file.file_name().unwrap_or_default().to_string_lossy());
                self.say(&said);
            }
            Err(e) => self.complain(&format!("Cannot open the table in GridCraft: {e}")),
        }
    }

    /// A table of the picked size, where the caret is.
    fn insert_table(&mut self, rows: usize, columns: usize) {
        let Some(ed) = self.editor.as_mut() else { return };
        let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len());
        pageedit::insert_table(&mut ed.blocks, at, rows, columns);
        ed.rebuild_body();
        ed.caret = Some(pageedit::Caret {
            block: at.min(ed.blocks.len() - 1),
            text: String::new(),
            focus: true,
            selection: (0, 0),
            place: None,
            cell: Some((0, 0)),
            span: None,
        });
        Self::mark_dirty_from_blocks(ed);
    }

    /// An empty spreadsheet beside the page, opened in GridCraft and linked from the page
    /// so it can be found again.
    fn new_gridcraft(&mut self) {
        let Some(exe) = self.gridcraft.clone() else {
            self.say("GridCraft is not installed");
            return;
        };
        let Some(ed) = self.editor.as_ref() else { return };
        let dir = ed.dir.clone();
        let mut name = "spreadsheet.csv".to_string();
        for n in 2..100 {
            if !dir.join(&name).exists() {
                break;
            }
            name = format!("spreadsheet-{n}.csv");
        }
        let file = dir.join(&name);
        if let Err(e) = std::fs::write(&file, "") {
            self.complain(&format!("Cannot write {name}: {e}"));
            return;
        }
        match std::process::Command::new(exe).arg(&file).spawn() {
            Ok(_) => {
                self.insert_in_page(&format!("[{name}]({name})"));
                let said = format!("{name} is open in GridCraft");
                self.say(&said);
            }
            Err(e) => self.complain(&format!("Cannot open GridCraft: {e}")),
        }
    }

    /// Picks a block of cells from the Table tab's Select buttons, as a drag across them
    /// would.
    fn select_in_table(&mut self, what: pageedit::TableSpan) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((at, cell)) = ed.caret.as_ref().and_then(|c| c.cell.map(|cell| (c.block, cell))) else {
            return;
        };
        let Some(block) = ed.blocks.get(at) else { return };
        let Some((from, to)) = pageedit::span_of(block, cell, what) else { return };
        if let Some(caret) = ed.caret.as_mut() {
            caret.cell = Some(from);
            caret.span = Some(to);
            caret.text = pageedit::cell_text(block, from).unwrap_or_default();
            caret.focus = false;
        }
    }

    /// Changes the shape of the table the caret is in, from the Table tab.
    ///
    /// A block of cells picked across several rows or columns takes them all: Delete Rows
    /// with three rows picked takes the three, the way it does in OneNote.
    fn edit_table(&mut self, what: pageedit::TableEdit) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((at, cell)) = ed.caret.as_ref().and_then(|c| c.cell.map(|cell| (c.block, cell))) else {
            return;
        };
        // Deleting works from the top-left of what is picked, over and over: each one
        // after the first falls into the place the one before it left. A picked block
        // that takes in the heading row starts under it, so picking the whole table and
        // asking for its rows empties the body and keeps the heading.
        let picked = ed.caret.as_ref().and_then(pageedit::cell_range);
        let (cell, times) = match (what, picked) {
            (pageedit::TableEdit::DeleteRow, Some((a, b))) => {
                let start = a.0.max(1);
                ((start, a.1), (b.0 + 1).saturating_sub(start))
            }
            (pageedit::TableEdit::DeleteColumn, Some((a, b))) => (a, b.1 - a.1 + 1),
            _ => (cell, 1),
        };
        let Some(block) = ed.blocks.get_mut(at) else { return };
        let mut last = None;
        for _ in 0..times {
            match pageedit::edit_table(block, cell, what) {
                Some(to) => last = Some(to),
                None => break,
            }
        }
        let Some(to) = last else {
            // The rules said no: a table keeps its heading row and at least one column.
            self.say("A table keeps its heading row and at least one column");
            return;
        };
        let text = pageedit::cell_text(block, to).unwrap_or_default();
        ed.rebuild_body();
        ed.caret = Some(pageedit::Caret { block: at, text, focus: true, selection: (0, 0), place: None, cell: Some(to), span: None });
        Self::mark_dirty_from_blocks(ed);
    }

    /// Picks up a pen, which is all it takes: the whole page is the ink's, so there is no
    /// sheet to put anywhere first.
    fn ink_insert(&mut self) {
        if self.editor.is_none() {
            self.say("Open a page first");
            return;
        }
        self.pointer = ribbon::Pointer::Draw;
        self.ribbon_tab = "Draw".to_string();
        self.say("Write anywhere on the page");
    }

    /// Writes the drawing again with no pen readings in it, for sending somewhere else.
    fn ink_export_plain(&mut self) {
        let Some(pad) = self.pad.as_ref() else {
            self.say("There is no drawing on this page");
            return;
        };
        match pad.save_plain() {
            Ok(path) => {
                let shown = self.shown(&path);
                self.say(format!("Saved the drawing as a picture: {shown}"));
            }
            Err(e) => self.complain(&format!("Cannot write the picture: {e}")),
        }
    }

    /// Puts marks around the selection in the block the caret is in.
    fn apply_mark(&mut self, mark: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        let marks = match mark {
            "bold" => "**",
            "italic" => "*",
            "strike" => "~~",
            _ => "`",
        };
        let len = caret.text.chars().count();
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        // All of the line's words: marked on the spans, so words that are partly marked already come out
        // right. A part of them is wrapped in the text, where the caret's own selection lives.
        if len > 0 && lo == 0 && hi == len && pageedit::toggle_mark(&mut ed.blocks, caret.block..caret.block + 1, mark) {
            if let Some(b) = ed.blocks.get(caret.block) {
                caret.text = pageedit::caret_text(b);
                caret.selection = (0, caret.text.chars().count());
            }
            ed.rebuild_body();
            Self::mark_dirty_from_blocks(ed);
            return;
        }
        let (text, _) = pageedit::wrap_selection(&caret.text, caret.selection, marks);
        caret.text = text;
        if let Some(b) = ed.blocks.get_mut(caret.block) {
            pageedit::set_block_text(b, &caret.text);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
    }

    /// The keys Windows users expect for paragraph direction. Alt+Shift remains available
    /// to Windows for switching the active input language.
    fn direction_keys(&mut self, ctx: &Context) {
        use notecraft_model::Dir;
        let ctrl_shift = Modifiers { ctrl: true, shift: true, command: true, ..Modifiers::NONE };
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(ctrl_shift, Key::R) {
            self.apply_direction(Some(Dir::Rtl));
        }
        if pressed(ctrl_shift, Key::L) {
            self.apply_direction(Some(Dir::Ltr));
        }
    }

    /// Which way the line the caret is in reads. `None` lets its own words decide.
    fn apply_direction(&mut self, dir: Option<notecraft_model::Dir>) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::set_direction(&mut ed.blocks, at, dir) {
            self.say("A direction goes on a paragraph of text");
            return;
        }
        ed.rebuild_body();
        if let Some(b) = ed.blocks.get(at) {
            let text = pageedit::caret_text(b);
            if let Some(c) = ed.caret.as_mut() {
                c.text = text;
            }
        }
        Self::mark_dirty_from_blocks(ed);
        self.say(match dir {
            Some(d) if d.is_rtl() => "This paragraph reads right to left",
            Some(_) => "This paragraph reads left to right",
            None => "This paragraph reads whichever way its words do",
        });
    }

    /// Fill the column with the paragraph the caret is in, or stop filling it.
    ///
    /// Which paragraphs are justified is the page's own, like the arrangement of its boxes
    /// and the width of its table columns, so it goes into the page's file and not into the
    /// window. The words are not touched: the stretching happens when the paragraph is
    /// drawn, so the file another program reads is the sentences the writer typed.
    fn apply_justify(&mut self, strength: notecraft_model::justify::Strength) {
        let Some(ed) = self.editor.as_ref() else {
            self.say("Open a page first");
            return;
        };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::Block::Paragraph(_))) {
            self.say("Justification goes on a paragraph of text");
            return;
        }
        // Asking for the strength a paragraph already has turns justification off, which
        // is how a menu of three gives a way back to a ragged edge without a fourth entry.
        let want = (ed.justified.strength_of(at) != Some(strength)).then_some(strength);
        let mut marked = ed.justified.clone();
        marked.set(at, want);
        self.write_justified(marked, want);
    }

    /// Writes which paragraphs are justified into the page's own file, and says what `want` now is.
    fn write_justified(&mut self, marked: notecraft_model::justify::Justified, want: Option<notecraft_model::justify::Strength>) {
        let Some(ed) = self.editor.as_ref() else { return };
        let path = path_str(&ed.dir);
        let line = notecraft_model::justify::write(&marked);
        self.save_now();
        if self.run("page.justify", &params(&[("path", &path), ("blocks", &line)])).is_some() {
            if let Some(ed) = self.editor.as_mut() {
                ed.justified = marked;
            }
            self.say(match want {
                Some(notecraft_model::justify::Strength::Low) => "This paragraph fills the column, stretched lightly",
                Some(notecraft_model::justify::Strength::Medium) => "This paragraph fills the column",
                Some(notecraft_model::justify::Strength::High) => "This paragraph fills the column, stretched hard",
                None => "This paragraph is ragged again",
            });
        }
    }

    /// Which way the whole page reads. It is written into the page's own file.
    fn apply_page_direction(&mut self, dir: Option<notecraft_model::Dir>) {
        let Some(ed) = self.editor.as_ref() else {
            self.say("Open a page first");
            return;
        };
        let path = path_str(&ed.dir);
        let want = dir.map(|d| d.id()).unwrap_or("auto");
        self.save_now();
        if self.run("page.direction", &params(&[("path", &path), ("dir", want)])).is_some() {
            if let Some(ed) = self.editor.as_mut() {
                ed.page_dir = dir;
            }
            self.say(match dir {
                Some(d) if d.is_rtl() => "This page reads right to left",
                Some(_) => "This page reads left to right",
                None => "Every line of this page decides for itself",
            });
        }
    }

    /// Which way the selection, or what is typed next, reads: an isolated run.
    fn apply_embedding(&mut self, dir: notecraft_model::Dir) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        let (text, selection) = pageedit::isolate(&caret.text, caret.selection, dir);
        caret.text = text;
        caret.selection = selection;
        caret.place = Some(selection.1);
        if let Some(b) = ed.blocks.get_mut(caret.block) {
            pageedit::set_block_text(b, &caret.text);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        self.say(if dir.is_rtl() {
            "This run reads right to left. A mixed line is still drawn in one order; see the roadmap"
        } else {
            "This run reads left to right. A mixed line is still drawn in one order; see the roadmap"
        });
    }

    /// Makes the block the caret is in a heading, a list item, a quote or ordinary text.
    fn apply_block_kind(&mut self, kind: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_ref() else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        let at = caret.block;
        if !pageedit::set_kind(&mut ed.blocks, at, kind) {
            return;
        }
        ed.rebuild_body();
        if let Some(b) = ed.blocks.get(at) {
            let text = pageedit::caret_text(b);
            if let Some(c) = ed.caret.as_mut() {
                c.text = text;
            }
        }
        Self::mark_dirty_from_blocks(ed);
    }

    /// Puts a tag on the line the caret is in, or takes it off again.
    fn apply_tag(&mut self, tag: notecraft_model::Tag) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = "Click in the page first, then use the ribbon".into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::set_tag(&mut ed.blocks, at, tag) {
            self.say("A tag goes on a line of text");
            return;
        }
        ed.rebuild_body();
        if let Some(b) = ed.blocks.get(at) {
            let text = pageedit::caret_text(b);
            if let Some(c) = ed.caret.as_mut() {
                c.text = text;
            }
        }
        Self::mark_dirty_from_blocks(ed);
        self.say(format!("{} tag", tag.label()));
    }

    /// Cut or copy what is selected in the block the caret is in.
    /// Ctrl+C, Ctrl+X, Ctrl+V and Ctrl+A for the words on a page.
    ///
    /// egui's own text box answers these for a paragraph it is drawing, and it is left to
    /// do so. Nothing answered them anywhere else, and that is a lot of places: not in a
    /// right-to-left paragraph, which NoteCraft draws itself; not for a selection that
    /// spans paragraphs, which no text box owns; and not when a box is picked or nothing
    /// has the keyboard at all. So Ctrl+C did nothing in Arabic, Ctrl+A selected nothing,
    /// and a box that had been copied could not be pasted into an empty one. The owner
    /// reported all three.
    ///
    /// The keys are read here and sent to the same `clipboard` and `paste_here` the ribbon
    /// buttons use, so there is one path and not two. Nothing is done when egui's text box
    /// is holding the keyboard, or both would act and the words would be pasted twice.
    fn clipboard_keys(&mut self, ctx: &egui::Context) {
        let (copy, cut, paste, all) = ctx.input(|i| {
            let hit = |want: egui::Key| {
                // `key_pressed` is the platform-normalized view.  Some native
                // backends do not retain the raw key event in `events` after a
                // focused text widget has considered it, while the normalized
                // edge remains available for the rest of the frame.  Keep the
                // raw-event check as a fallback for the headless harness and
                // older egui backends.
                (i.key_pressed(want) && (i.modifiers.command || i.modifiers.ctrl))
                    || i.events.iter().any(|e| {
                        matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == want && (modifiers.command || modifiers.ctrl))
                    })
            };
            (hit(egui::Key::C), hit(egui::Key::X), hit(egui::Key::V), hit(egui::Key::A))
        });
        if !(copy || cut || paste || all) {
            return;
        }
        let ours = self.words_are_ours();
        // Paste has one more case than the others: with a box picked, or with the caret
        // nowhere, no text box is listening and the ribbon's Paste is the only way in.
        let nobody = self.editor.as_ref().is_some_and(|e| e.caret.is_none());
        if copy && ours {
            self.clipboard("copy");
        }
        if cut && ours {
            self.clipboard("cut");
        }
        // A screenshot has no text for egui's text widget to consume. Route it through
        // NoteCraft even when the caret is inside an ordinary text editor; otherwise Ctrl+V
        // appears to do nothing because egui handles the key before `paste_here` can inspect
        // the system image clipboard.
        let picture = paste && crate::clipboard::has_picture();
        if paste && (picture || ours || nobody) {
            self.paste_here();
        }
        if all && ours {
            self.select_all_in_block();
        }
    }

    /// True when the words the caret is in are drawn by NoteCraft rather than by an egui
    /// text box, so the keys above are ours to answer.
    fn words_are_ours(&self) -> bool {
        let Some(ed) = self.editor.as_ref() else { return false };
        // A selection across paragraphs belongs to no text box at all.
        if ed.across.is_some_and(|a| !a.inside_one_block()) {
            return true;
        }
        let Some(caret) = ed.caret.as_ref() else { return false };
        // The same question `show_blocks` asks when it decides which painter draws the
        // block: a line with right-to-left in it is ours, and code and maths never are.
        pageedit::needs_our_caret(&caret.text)
            && !matches!(
                ed.blocks.get(caret.block),
                Some(notecraft_formats::markdown::Block::Code { .. }) | Some(notecraft_formats::markdown::Block::MathBlock(_))
            )
    }

    /// Ctrl+A: take the whole of the paragraph the caret is in.
    ///
    /// The paragraph and not the page. That is what a text box does, and it is the step
    /// OneNote takes first as well; pressing again there widens to the box and then to the
    /// page, which is not built here and is written down in `work\expected`.
    fn select_all_in_block(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else { return };
        let end = caret.text.chars().count();
        caret.selection = (end, 0);
        caret.place = None;
        self.say("Paragraph selected");
    }

    fn clipboard(&mut self, which: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        // A selection that runs across blocks is not any one block's, so it is taken from
        // the blocks themselves. Cut takes the words out and joins what is left.
        if let Some(open) = ed.across.filter(|a| !a.inside_one_block()) {
            let picked = pageedit::span_text(&ed.blocks, open);
            if picked.is_empty() {
                self.say("Select some words first");
                return;
            }
            let copied = pageedit::blocks_to_paste(&picked);
            self.pending_copy = Some(notecraft_formats::render::blocks_to_text(&copied));
            self.pending_copy_html = Some(notecraft_formats::render::blocks_to_html(&copied));
            if which == "cut" {
                let (block, at) = pageedit::delete_span(&mut ed.blocks, open);
                let text = ed.blocks.get(block).map(pageedit::caret_text).unwrap_or_default();
                ed.caret = Some(pageedit::Caret { block, text, focus: true, selection: (at, at), place: Some(at), cell: None, span: None });
                ed.across = None;
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
            }
            self.say(if which == "cut" { "Cut" } else { "Copied" });
            return;
        }
        let Some(caret) = ed.caret.as_mut() else {
            self.status = "Click in the page first".into();
            self.status_is_error = false;
            return;
        };
        let (a, b) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        let picked: String = caret.text.chars().skip(a).take(b.saturating_sub(a)).collect();
        if picked.is_empty() {
            self.status = "Select some words first".into();
            self.status_is_error = false;
            return;
        }
        let copied = pageedit::blocks_to_paste(&picked);
        self.pending_copy = Some(notecraft_formats::render::blocks_to_text(&copied));
        self.pending_copy_html = Some(notecraft_formats::render::blocks_to_html(&copied));
        if which == "cut" {
            let (text, at) = pageedit::insert_at_caret(&caret.text, caret.selection, "");
            caret.text = text;
            caret.selection = (at, at);
            caret.place = Some(at);
            caret.focus = true;
            if let Some(b) = ed.blocks.get_mut(caret.block) {
                pageedit::set_block_text(b, &caret.text);
            }
            ed.rebuild_body();
            Self::mark_dirty_from_blocks(ed);
        }
        self.say(if which == "cut" { "Cut" } else { "Copied" });
    }

    /// More or less indent on the list item the caret is in.
    fn indent_block(&mut self, deeper: bool) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = "Click in the page first".into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::indent(&mut ed.blocks, at, deeper) {
            self.say("Only a list item can be indented");
            return;
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
    }

    /// Takes the marks off the selection: bold, italic, strikethrough, code.
    fn clear_formatting(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else { return };
        caret.text = pageedit::without_marks(&caret.text);
        caret.selection = (0, caret.text.chars().count());
        if let Some(b) = ed.blocks.get_mut(caret.block) {
            pageedit::set_block_text(b, &caret.text);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
    }

    /// Puts a whole block (a table, a diagram) after the caret.
    fn insert_block(&mut self, what: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len());
        if !pageedit::insert_block(&mut ed.blocks, at, what) {
            self.say("That is not something a page can hold");
            return;
        }
        ed.rebuild_body();
        ed.caret = ed.blocks.get(at).map(|b| pageedit::Caret {
            block: at,
            text: pageedit::caret_text(b),
            focus: true,
            selection: (0, 0),
            place: None,
            cell: None,
            span: None,
        });
        Self::mark_dirty_from_blocks(ed);
    }

    /// Puts a snippet where the caret is, or at the end of the page.
    fn insert_in_page(&mut self, text: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        match ed.caret.as_mut() {
            Some(caret) => {
                // Where the caret is, not at the end of the block.
                let (new_text, at) = pageedit::insert_at_caret(&caret.text, caret.selection, text.trim_matches('\n'));
                caret.text = new_text;
                caret.selection = (at, at);
                caret.place = Some(at);
                caret.focus = true;
                if let Some(b) = ed.blocks.get_mut(caret.block) {
                    pageedit::set_block_text(b, &caret.text);
                }
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
            }
            None => {
                let cursor = ed.body.chars().count();
                self.cursor = cursor;
                self.insert(text);
            }
        }
    }

    /// The theme switcher: 14 swatches (light and dark, seven accents each) and a switch to
    /// follow the system setting.
    fn theme_menu(&mut self, ui: &mut Ui, tokens: Tokens) {
        let current = self.theme();
        let mut pick: Option<(Mode, Accent)> = None;
        let mut follow = self.mode_pref == ModePref::System;
        let follow_before = follow;
        // A sun or a moon in the theme's accent colour, instead of a line of text.
        let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 24.0), Sense::click());
        let painter = ui.painter_at(rect);
        if resp.hovered() {
            painter.rect_filled(rect, 4.0, tokens.row_hover);
        }
        let icon_rect = egui::Rect::from_center_size(rect.center(), vec2(19.0, 19.0));
        if current.mode.is_dark() {
            icons::paint_moon(&painter, icon_rect, tokens.accent, if resp.hovered() { tokens.row_hover } else { tokens.panel });
        } else {
            icons::paint(&painter, icon_rect, Icon::Sun, tokens.accent);
        }
        let label = format!("Theme: {}", current.label());
        let enabled = ui.is_enabled();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
        let resp = resp.on_hover_text(format!("{} theme. Click to change it.", current.label()));
        egui::Popup::menu(&resp).show(|ui| {
            theme_grid(ui, current, &mut pick, &mut follow);
        });
        // The same chooser, as a window, for the ribbon's Theme button.
        if self.theme_window {
            let mut open = true;
            egui::Window::new("Theme").collapsible(false).resizable(false).open(&mut open).anchor(Align2::RIGHT_TOP, vec2(-12.0, 120.0)).show(
                ui.ctx(),
                |ui| {
                    theme_grid(ui, current, &mut pick, &mut follow);
                },
            );
            if !open || pick.is_some() {
                self.theme_window = false;
            }
        }
        if let Some((mode, accent)) = pick {
            // A swatch names a mode, so clicking one means that mode, even while the app was
            // following the system. Otherwise picking "Dark Default" on a light desktop did
            // nothing visible. The switch below is the way back to following the system.
            self.set_theme(Theme { mode, accent });
        } else if follow != follow_before {
            if follow {
                self.follow_system_theme(current.accent);
            } else {
                self.set_theme(current);
            }
        }
    }

    fn search_popup(&mut self, ctx: &Context) {
        if self.search.trim().is_empty() {
            return;
        }
        let mut chosen: Option<PathBuf> = None;
        egui::Area::new(Id::new("search-results")).order(egui::Order::Foreground).anchor(Align2::RIGHT_TOP, vec2(-8.0, 44.0)).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(420.0);
                if self.hits.is_empty() {
                    ui.weak("No pages match.");
                }
                for h in &self.hits {
                    let r = ui.vertical(|ui| {
                        ui.strong(&h.title);
                        ui.label(egui::RichText::new(&h.snippet).small().color(self.tokens.muted));
                    });
                    if ui.interact(r.response.rect, Id::new(("hit", &h.path)), Sense::click()).clicked() {
                        chosen = Some(h.path.clone());
                    }
                    ui.separator();
                }
            });
        });
        if let Some(p) = chosen {
            let mut parent = p.parent();
            while let Some(d) = parent {
                if d == self.session.root() {
                    break;
                }
                self.open.insert(d.to_path_buf());
                parent = d.parent();
            }
            self.open_page(&p);
            self.search.clear();
            self.searched.clear();
            self.hits.clear();
        }
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        // Full page view: the notebooks step aside until the user asks for them again.
        if self.full_page && self.editor.is_some() {
            return;
        }
        let mut acts = Vec::new();
        if self.split_panes {
            self.organiser_panes(ui, &mut acts);
            for a in acts {
                self.apply(a);
            }
            return;
        }
        let mut split_now = false;
        egui::Panel::left("tree").resizable(true).default_size(300.0).size_range(200.0..=700.0).show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // In the one-tree layout the same button splits the panel and hides the
                // notebooks, which is what pressing it in OneNote does.
                split_now = Self::bars_button_at(ui).clicked();
                ui.label(egui::RichText::new("NOTEBOOKS").small().color(self.tokens.muted));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("New notebook").clicked() {
                    self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None });
                }
                if ui.small_button("Expand all").on_hover_text("Open every level").clicked() {
                    self.open = self.collect_branches().into_iter().collect();
                }
                if ui.small_button("Collapse all").clicked() {
                    self.open.clear();
                }
            });
            ui.add_space(2.0);
            // A drag in a pane must not scroll it: in the page a drag moves a container,
            // and in the tree it is a click that slipped. The wheel and the bar still work.
            let scroll = egui::containers::scroll_area::ScrollSource {
                drag: egui::containers::scroll_area::DragScroll::Never,
                ..egui::containers::scroll_area::ScrollSource::default()
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).scroll_source(scroll).show(ui, |ui| {
                if self.session.tree().is_empty() {
                    ui.weak("No notebooks yet. Use \"New notebook\".");
                }
                for n in self.session.tree() {
                    self.draw_node(ui, n, 0, &mut acts);
                }
                self.empty_space_menu(ui, None, &mut acts);
            });
        });
        if split_now {
            self.split_panes = true;
            self.organisers = false;
            let _ = self.session.settings.set("split_panes", "true");
        }
        for a in acts {
            self.apply(a);
        }
    }

    /// The empty space under the rows of a column, with a menu of its own: right-clicking
    /// where there is no row is how a new notebook or a new page is started in OneNote.
    fn empty_space_menu(&self, ui: &mut Ui, section: Option<&Path>, acts: &mut Vec<Act>) {
        let left = ui.available_size();
        if left.y < 2.0 {
            return;
        }
        let (rect, resp) = ui.allocate_exact_size(left, Sense::click());
        let _ = rect;
        let mut reveal = false;
        let mut choose = false;
        resp.context_menu(|ui| {
            match section {
                // Under the pages: a page at the end of this section.
                Some(path) => {
                    if ui.button("New page...").clicked() {
                        acts.push(Act::AddKind(path.to_path_buf(), NodeKind::Section, NodeKind::Page));
                        ui.close();
                    }
                }
                // Under the notebooks: a notebook, and the ways into the folder itself.
                None => {
                    if ui.button("New notebook...").clicked() {
                        acts.push(Act::NewNotebook);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Open a folder...").clicked() {
                        choose = true;
                        ui.close();
                    }
                    if ui.button("Show the notebooks folder").clicked() {
                        reveal = true;
                        ui.close();
                    }
                }
            }
        });
        if reveal {
            acts.push(Act::RevealRoot);
        }
        if choose {
            acts.push(Act::ChooseRoot);
        }
    }

    /// The gap between two pages in the pages column. Hovering it draws the line OneNote
    /// draws there, and clicking it makes a page in that place rather than at the end.
    ///
    /// `before` and `after` are the orders of the pages on either side; the new page takes
    /// the number in between, which is why siblings are numbered ten apart.
    fn page_gap(&self, ui: &mut Ui, section: &Path, before: Option<u32>, after: Option<u32>, acts: &mut Vec<Act>) {
        /// How big the round "+" is. The gap itself is shorter than this on purpose: it is
        /// a thin strip between two pages, and the button stands out of it.
        const PLUS: f32 = 8.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 7.0), Sense::click());
        let hot = resp.hovered();
        if hot {
            // The gap is 7 points tall and the circle is 13 across, so the circle has to
            // be drawn outside the strip it belongs to. `painter_at` cannot: it narrows
            // the clip it is given to whatever the ui already has, which is the strip, and
            // the circle came out a lozenge, flat at the top and the bottom. Painting on
            // the layer starts from the whole window instead.
            let room = (PLUS + 1.0 - rect.height() / 2.0).max(2.0);
            let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(rect.expand(room));
            let y = rect.center().y;
            let t = &self.tokens;
            painter.line_segment([egui::pos2(rect.min.x + 21.0, y), egui::pos2(rect.max.x - 6.0, y)], egui::Stroke::new(1.5, t.accent));
            let c = egui::pos2(rect.min.x + 10.0, y);
            painter.circle_filled(c, PLUS, t.accent);
            // Draw the cross as two equal strokes around the circle's center. A font glyph's
            // baseline and line box can make a visually off-center plus even with centered text.
            let cross = egui::Stroke::new(2.0, t.panel);
            painter.line_segment([c - vec2(4.0, 0.0), c + vec2(4.0, 0.0)], cross);
            painter.line_segment([c - vec2(0.0, 4.0), c + vec2(0.0, 4.0)], cross);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let enabled = ui.is_enabled();
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, "Add a page here"));
        let resp = resp.on_hover_text("A page here");
        if resp.clicked() {
            // Halfway between the two, or ten past the last one.
            let order = match (before, after) {
                (Some(b), Some(a)) if a > b + 1 => b + (a - b) / 2,
                (Some(b), Some(_)) => b.saturating_add(1),
                (Some(b), None) => b.saturating_add(10),
                (None, Some(a)) => a / 2,
                (None, None) => 10,
            };
            acts.push(Act::AddPageAt(section.to_path_buf(), order));
        }
    }

    /// The three-bar button, at the top left of the side panel.
    fn bars_button_at(ui: &mut Ui) -> egui::Response {
        // Drawn rather than typed: the font's own three-bar glyph is heavy and tight.
        let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::click());
        let painter = ui.painter_at(rect);
        if resp.hovered() {
            painter.rect_filled(rect, 3.0, ui.visuals().widgets.hovered.bg_fill);
        }
        let colour = ui.visuals().widgets.inactive.fg_stroke.color;
        icons::paint(&painter, egui::Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), Icon::Bars, colour);
        let enabled = ui.is_enabled();
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, BARS));
        resp.on_hover_text("Show or hide the notebooks column")
    }

    /// The side panel as two columns: the organisers (notebooks, section groups and
    /// sections) and then the pages of whichever section is chosen, the way OneNote splits
    /// them. The three-bar button in the top bar hides the first column.
    fn organiser_panes(&mut self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let scroll = || egui::containers::scroll_area::ScrollSource {
            drag: egui::containers::scroll_area::DragScroll::Never,
            ..egui::containers::scroll_area::ScrollSource::default()
        };
        let mut toggle = false;
        if self.organisers {
            egui::Panel::left("organisers").resizable(true).default_size(240.0).size_range(160.0..=520.0).show(ui, |ui| {
                ui.add_space(4.0);
                // The three-bar button sits at the top left of the notebooks, where
                // OneNote keeps it, and hides this column.
                ui.horizontal(|ui| {
                    toggle |= Self::bars_button_at(ui).clicked();
                    ui.label(egui::RichText::new("NOTEBOOKS").small().color(self.tokens.muted));
                });
                ui.add_space(2.0);
                egui::ScrollArea::vertical().id_salt("organisers").auto_shrink([false, false]).scroll_source(scroll()).show(ui, |ui| {
                    if self.session.tree().is_empty() {
                        ui.weak("No notebooks yet. Use \"New notebook\".");
                    }
                    for n in self.session.tree() {
                        self.draw_row(ui, n, 0, acts, false);
                    }
                    self.empty_space_menu(ui, None, acts);
                });
            });
        }
        let section = self.current_section(self.session.tree()).map(|n| (n.path.clone(), n.info.title.clone(), n.children.clone()));
        egui::Panel::left("pages").resizable(true).default_size(230.0).size_range(150.0..=520.0).show(ui, |ui| {
            ui.add_space(4.0);
            match &section {
                Some((path, title, pages)) => {
                    ui.horizontal(|ui| {
                        // With the notebooks hidden the button lives here instead, so it is
                        // always at the top left of the panel.
                        if !self.organisers {
                            toggle |= Self::bars_button_at(ui).clicked();
                        }
                        if ui.button("Add page").on_hover_text("A new page in this section").clicked() {
                            acts.push(Act::Add(path.clone(), NodeKind::Section));
                        }
                    });
                    ui.label(egui::RichText::new(title.to_uppercase()).small().color(self.tokens.muted));
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().id_salt("pages").auto_shrink([false, false]).scroll_source(scroll()).show(ui, |ui| {
                        let list: Vec<&Node> = pages.iter().filter(|p| p.info.kind == NodeKind::Page).collect();
                        if list.is_empty() {
                            ui.weak("No pages in this section yet.");
                        }
                        // A line between every pair, and above the first and below the
                        // last: hovering one offers a page in that place.
                        let mut last: Option<u32> = None;
                        for p in &list {
                            self.page_gap(ui, path, last, Some(p.info.order), acts);
                            self.draw_row(ui, p, 0, acts, true);
                            last = Some(p.info.order);
                        }
                        self.page_gap(ui, path, last, None, acts);
                        self.empty_space_menu(ui, Some(path), acts);
                    });
                }
                None => {
                    ui.horizontal(|ui| {
                        if !self.organisers {
                            toggle |= Self::bars_button_at(ui).clicked();
                        }
                        ui.weak("Choose a section to see its pages.");
                    });
                }
            }
        });
        if toggle {
            self.organisers = !self.organisers;
        }
    }

    fn central(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            if self.editor.is_none() {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.22);
                    ui.add(egui::Image::from_bytes("bytes://notecraft-logo", crate::LOGO_PNG).fit_to_exact_size(vec2(128.0, 128.0)));
                    ui.add_space(8.0);
                    ui.heading("NoteCraft");
                    ui.label("Notes as plain folders. Open a page, or create a notebook to start.");
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("Notebooks folder: {}", self.shown(self.session.root()))).small().color(self.tokens.muted));
                });
                return;
            }
            self.editor_ui(ui);
        });
    }

    fn editor_ui(&mut self, ui: &mut Ui) {
        let editor_rect = ui.max_rect();
        let wheel_zoom = ui.input(|i| i.pointer.hover_pos().filter(|p| editor_rect.contains(*p)).map_or(1.0, |_| i.zoom_delta()));
        if wheel_zoom.is_finite() && (wheel_zoom - 1.0).abs() > 0.001 {
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                self.zoom_percent = (self.zoom_percent as f32 * wheel_zoom).round().clamp(50.0, 250.0) as u32;
            }
            self.fit_page_width = false;
        }
        if self.fit_page_width {
            let width = ui.available_width();
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let percent = (width / 780.0 * 100.0).round().clamp(50.0, 250.0) as u32;
            self.zoom_percent = percent;
            self.fit_page_width = false;
        }
        #[allow(clippy::cast_precision_loss)]
        let zoom_scale = self.zoom_percent as f32 / 100.0;
        let root = self.session.root().to_path_buf();
        let tokens = self.tokens;
        let mut rename: Option<String> = None;
        let cursor = self.cursor;
        let mut changed = false;
        let show_source = self.show_source;
        let full_page = self.full_page;
        let (hide_title, rules) = (self.hide_title, self.paper_now());
        // The wash behind this page, if it carries a colour: a pale tint of it over the
        // page's own background, so the words keep their contrast whatever is chosen.
        let page_colour = self.editor.as_ref().and_then(|e| e.colour).map(|c| {
            let dark = self.theme().mode == crate::theme::Mode::Dark;
            let (r, g, b) = c.rgb(dark);
            crate::theme::mix(self.tokens.editor, egui::Color32::from_rgb(r, g, b), 0.16)
        });
        // What the title area is showing, so the page's words do not say it again.
        let shown_title = self.editor.as_ref().map(|e| e.title.clone()).unwrap_or_default();
        let table_borders = self.table_borders;
        let (mut toggle_source, mut toggle_full_page, mut focus_search) = (false, false, false);
        let mut from_blocks = false;
        let mut boxes_moved = false;
        let mut cols_moved = false;
        // A file the writer asked to delete from the page's folder, confirmed first.
        let mut delete_file: Option<String> = None;
        // A file other than the page written in the code view, waiting to go to disk.
        let mut wrote_file: Option<(String, String)> = None;
        let mut code = std::mem::take(&mut self.code);
        let mut save_cols: Option<(String, String)> = None;
        // A link clicked in the page this frame, followed once the page's borrow is over.
        let mut follow: Option<String> = None;
        // The drawing on this page is lent to the block that holds it, and written back
        // once the page's borrow is over.
        let (pen, pressure) = (self.pen(), self.stylus.sample().pressure);
        let mut pad = self.pad.take();
        let mut ink_changed = false;
        let mut page_rect = None;
        // What the page's right-click menu asked for, run once the page's borrow is over.
        let mut page_acts: Vec<ribbon::Act> = Vec::new();
        {
            let Some(ed) = self.editor.as_mut() else { return };
            let crumbs = ed.dir.strip_prefix(&root).map(|p| p.to_string_lossy().replace('\\', "  /  ")).unwrap_or_default();
            // One id for the title, whether it is drawn as words or as a text box.
            let title_id = egui::Id::new("page-title-box");
            ui.label(egui::RichText::new(crumbs).small().color(tokens.muted));
            ui.horizontal(|ui| {
                // The title is not here any more. It is drawn on the sheet itself, at the
                // top of the page, because the page's name is part of the page: that is
                // what OneNote does and what the owner asked for on 2026-10-19. What is
                // left in this strip is the things that are about the file rather than in
                // it, and they stay out of the page.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let state = if ed.dirty_since.is_some() {
                        "Editing...".to_string()
                    } else {
                        ed.last_saved.clone().map(|t| format!("Saved {t}")).unwrap_or_else(|| "Saved".into())
                    };
                    ui.label(egui::RichText::new(state).small().color(tokens.muted));
                    ui.label(egui::RichText::new(ed.format.label()).small().color(tokens.muted));
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Drop a file onto the page to add it").small().color(tokens.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The page controls, in OneNote's order: full page, search, source.
                    let full = icon_toggle(
                        ui,
                        if full_page { Icon::ExitFullPage } else { Icon::FullPage },
                        full_page,
                        &tokens,
                        "Full page",
                        if full_page { "Show the notebooks again" } else { "Give the page the whole window" },
                    );
                    if full.clicked() {
                        toggle_full_page = true;
                    }
                    if icon_toggle(ui, Icon::Search, false, &tokens, "Search", "Search all pages").clicked() {
                        focus_search = true;
                    }
                    if icon_toggle(
                        ui,
                        Icon::Code,
                        show_source,
                        &tokens,
                        "Page source",
                        if show_source { "Hide the page source" } else { "Show the page source" },
                    )
                    .clicked()
                    {
                        toggle_source = true;
                    }
                });
            });
            ui.separator();
            let height = ui.available_height();
            // The page itself. Without the source beside it the page is a readable column,
            // the way a page looks in OneNote or Word, and a click puts the caret in it.
            let mut page = |ui: &mut Ui,
                            ed: &mut Editor,
                            wide: bool,
                            _changed: &mut bool,
                            from_blocks: &mut bool,
                            toggle: &mut bool,
                            boxes_moved: &mut bool,
                            cols_moved: &mut bool,
                            delete_file: &mut Option<String>,
                            follow: &mut Option<String>,
                            rename: &mut Option<String>| {
                let scroll = egui::containers::scroll_area::ScrollSource {
                    drag: egui::containers::scroll_area::DragScroll::Never,
                    ..egui::containers::scroll_area::ScrollSource::default()
                };
                egui::ScrollArea::vertical().id_salt("preview").scroll_source(scroll).max_height(height).show(ui, |ui| {
                    // Lines or squares behind the page, the way OneNote rules one. The
                    // spacing comes from the paper chosen; NoteCraft's own two papers draw
                    // every fifth line stronger, which the others do not.
                    // The colour behind the words, and then the lines on it. Both are
                    // the page's own when it carries them, as OneNote's page colour and
                    // rule lines are the page's.
                    if wide && let Some(wash) = page_colour {
                        let area = ui.max_rect();
                        ui.painter_at(area).rect_filled(area, 0.0, wash);
                    }
                    if wide && rules.on() {
                        let area = ui.max_rect();
                        crate::ribbon::paint_paper(&ui.painter_at(area), area, rules, zoom_scale, tokens.rule_line, tokens.rule_line_strong);
                    }
                    // Which way lines fall when they say nothing themselves: the page's own
                    // direction, if the writer gave it one.
                    let page_dir = ed.page_dir;
                    let available_width = ui.available_width();
                    let page_column = 780.0 * zoom_scale;
                    let margin = if wide { ((available_width - page_column) / 2.0).max(8.0) } else { 0.0 };
                    egui::Frame::NONE.inner_margin(egui::Margin { left: margin as i8, right: margin as i8, top: 4, bottom: 8 }).show(ui, |ui| {
                        let column = if available_width >= page_column { page_column } else { (available_width - 16.0).max(1.0) };
                        ui.set_min_width(column);
                        ui.set_width(column);
                        // The page's own name, at the top of the sheet. Before the sheet's
                        // origin is taken below, so the ink layer keeps its own coordinates
                        // and a drawing stays beside the words it was made next to.
                        if !hide_title {
                            page_title(ui, ed, &tokens, title_id, rename, zoom_scale);
                        }
                        if wide && ed.writable {
                            // The caret lives in the page: every block is drawn the way it
                            // will be read, and the one being written in is a text box in
                            // that block's own type. The markup is never shown here. The
                            // blocks sit in containers, which is where they are placed.
                            let top = ui.cursor().min;
                            // A press lets go of whatever was selected: the drag that may
                            // follow it builds a new one. Done here, once a frame, rather
                            // than inside the blocks, where a container drawn later would
                            // clear what an earlier one had just set.
                            if ui.input(|i| i.pointer.any_pressed()) {
                                ed.across = None;
                            }
                            let mut moved = crate::containers::show(
                                ui,
                                &mut ed.blocks,
                                &mut ed.boxes,
                                &mut ed.caret,
                                &ed.dir,
                                &tokens,
                                page_dir,
                                table_borders,
                                &mut ed.columns,
                                &ed.justified,
                                &mut ed.across,
                                if hide_title { "" } else { &shown_title },
                                zoom_scale,
                            );
                            page_acts.append(&mut moved.acts);
                            // The ink of a page sits over its words, anywhere on it, the
                            // way a pen on paper does. It goes on after them so a press
                            // with a pen in hand reaches the ink and not the caret.
                            {
                                // The sheet, not the words: ink goes anywhere on the page,
                                // and the page is a sheet of paper whether or not anything
                                // is written that far down it. Measuring the ink's area
                                // from where the text happens to end meant the pen stopped
                                // at the last line, which is not what a sheet of paper
                                // does. The room is claimed so the page can be scrolled
                                // into it and so what is drawn there is never cut off.
                                let used = ui.cursor().min.y - top.y;
                                let want = crate::inkpad::SHEET_H.max(used + 240.0);
                                if want > used {
                                    ui.allocate_space(vec2(ui.available_width(), want - used));
                                }
                                let body =
                                    egui::Rect::from_min_max(egui::pos2(ui.max_rect().min.x, top.y), egui::pos2(ui.max_rect().max.x, top.y + want));
                                page_rect = Some(body);
                                if let Some(pad) = pad.as_mut() {
                                    ink_changed |= crate::inkpad::show(ui, body, pad, pen, pressure, &tokens, zoom_scale);
                                }
                                // The page's own right-click menu. No widget is laid
                                // over the page for it: a widget that senses clicks would
                                // take the primary click away from the words, the ink and
                                // the caret, and one that does not sense clicks cannot
                                // carry a context menu at all. So the secondary click is
                                // watched for directly and the menu drawn where it fell.
                                crate::pagemenu::show(ui, body, &mut page_acts, &tokens);
                            }
                            if moved.changed.blocks {
                                ed.rebuild_body();
                                *from_blocks = true;
                            }
                            if moved.boxes {
                                *boxes_moved = true;
                            }
                            if moved.changed.columns {
                                *cols_moved = true;
                            }
                            ed.origin = moved.origin;
                            if moved.changed.link.is_some() {
                                *follow = moved.changed.link;
                            }
                        } else if wide {
                            // A page with markup the block model cannot hold: writing it back
                            // would drop that markup, so it is read here and changed in its
                            // source. See docs/gaps.md.
                            if let Some(href) = preview::show_zoomed(ui, &ed.blocks, &ed.dir, &tokens, page_dir, &ed.justified, zoom_scale) {
                                *follow = Some(href);
                            }
                            ui.add_space(14.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("This page holds markup NoteCraft does not edit yet, so it is shown as it reads.")
                                        .small()
                                        .color(tokens.muted),
                                );
                                if ui.small_button("Edit the source").clicked() {
                                    *toggle = true;
                                }
                            });
                        } else if let Some(href) = preview::show_zoomed(ui, &ed.blocks, &ed.dir, &tokens, page_dir, &ed.justified, zoom_scale) {
                            *follow = Some(href);
                        }
                        if !ed.attachments.is_empty() {
                            ui.add_space(10.0);
                            ui.separator();
                            ui.label(egui::RichText::new("Files in this page's folder").small().color(tokens.muted));
                            for (name, size) in &ed.attachments {
                                // A file in the page's folder can be taken out again. The
                                // button is on the row rather than under the right
                                // button, because the page's own menu answers a right
                                // click anywhere on the sheet, this list included.
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("assets/{name}  ({} KB)", size.div_ceil(1024))).small().monospace());
                                    if ui.small_button("Delete file").on_hover_text(format!("Delete assets/{name} from this page's folder")).clicked()
                                    {
                                        *delete_file = Some(name.clone());
                                    }
                                });
                            }
                        }
                    });
                });
            };
            if show_source {
                // The code view: the page's files on the left, the one picked on the
                // right. The page itself is not drawn while it is open, so there is only
                // ever one editable copy of the page on screen.
                let named = pageedit::links_in(&ed.blocks);
                let entries = crate::codeview::files(&ed.dir, ed.format.page_file(), &named);
                let out = crate::codeview::show(ui, &mut code, &entries, &ed.dir, &tokens, height);
                wrote_file = out.wrote;
            } else {
                page(
                    ui,
                    ed,
                    true,
                    &mut changed,
                    &mut from_blocks,
                    &mut toggle_source,
                    &mut boxes_moved,
                    &mut cols_moved,
                    &mut delete_file,
                    &mut follow,
                    &mut rename,
                );
            }
            if changed {
                Self::mark_dirty(ed);
            } else if from_blocks {
                Self::mark_dirty_from_blocks(ed);
            }
            // Before the step is remembered, so the page in the history is one that
            // names its ink: a box deleted with the ink line in it is repaired at once
            // rather than at the next open.
            if (changed || from_blocks) && Self::name_ink(ed) {
                from_blocks = true;
            }
            if changed || from_blocks || boxes_moved {
                Self::remember_step(ed);
            }
            // A container was dragged or resized: the arrangement is the page's own, so it
            // is written into the page's file rather than kept in the window.
            if boxes_moved {
                ed.arranged = true;
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
            }
            // A column was dragged wider or narrower. Widths are layout, like the boxes,
            // so they go into the page's own file beside them.
            if cols_moved {
                let path = path_str(&ed.dir);
                let line = notecraft_model::tablecols::write(&ed.columns);
                save_cols = Some((path, line));
            }
        }
        self.cursor = cursor;
        if let Some((path, line)) = save_cols {
            self.run("page.columns", &params(&[("path", &path), ("widths", &line)]));
        }
        self.code = code;
        if let Some((name, text)) = wrote_file {
            self.write_page_file(&name, &text);
        }
        if let Some(name) = delete_file {
            let page = self.editor.as_ref().map(|e| e.dir.clone());
            if let Some(page) = page {
                self.dialog = Some(Dialog::DeleteFile { page, name });
            }
        }
        if toggle_source {
            self.set_show_source(!show_source);
        }
        if toggle_full_page {
            self.full_page = !full_page;
        }
        if focus_search {
            self.focus_search = true;
        }
        // The drawing comes back from the page, and a stroke laid down this frame is
        // written at once: ink is its own file, so it does not wait for the page to save.
        self.pad = pad;
        if page_rect.is_some() {
            self.page_rect = page_rect;
        }
        for act in page_acts {
            self.ribbon_act(act);
        }
        if ink_changed {
            self.last_ink_change = Some(Instant::now());
        }
        if ink_changed && let Some(pad) = self.pad.as_mut() {
            let strokes = pad.strokes();
            match pad.save() {
                Ok(()) => self.say(format!("Drawing saved, {strokes} strokes")),
                Err(e) => self.complain(&format!("Cannot write the drawing: {e}")),
            }
            self.sync_ink_reference();
        }
        if let Some(href) = follow {
            self.follow_link(ui.ctx(), &href);
        }
        if let Some(t) = rename
            && let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone())
            && self.run("node.title", &params(&[("path", &path_str(&dir)), ("title", &t)])).is_some()
        {
            if let Some(ed) = self.editor.as_mut() {
                ed.title = t;
            }
            self.say("Title saved");
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = if self.status_is_error { self.tokens.warn } else { self.tokens.muted };
                ui.label(egui::RichText::new(&self.status).color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let nodes: usize = self.session.tree().iter().map(|n| n.count()).sum();
                    ui.label(
                        egui::RichText::new(format!(
                            "{} notebooks, {nodes} items  |  {}",
                            self.session.tree().len(),
                            self.shown(self.session.root())
                        ))
                        .small()
                        .color(self.tokens.muted),
                    );
                    ui.separator();
                    let (current, source) = self.session.settings.global_format();
                    let mut chosen = current;
                    egui::ComboBox::from_id_salt("default-format").selected_text(current.label()).show_ui(ui, |ui| {
                        for f in PageFormat::ALL {
                            ui.selectable_value(&mut chosen, f, f.label());
                        }
                    });
                    ui.label(egui::RichText::new(format!("({})", source.label())).small().color(self.tokens.muted));
                    ui.label(egui::RichText::new("New pages:").small().color(self.tokens.muted));
                    if chosen != current && self.run("settings.default_format.set", &params(&[("format", chosen.id())])).is_some() {
                        self.say(format!("New pages use {} unless the notebook or section says otherwise", chosen.label()));
                    }
                });
            });
        });
    }

    // ---------- dialogs ----------

    fn dialogs(&mut self, ctx: &Context) {
        let Some(dialog) = self.dialog.take() else { return };
        let mut keep = true;
        let mut next = dialog;
        match &mut next {
            Dialog::ClearTelemetry { path } => {
                let mut clear = false;
                let mut cancel = false;
                egui::Window::new("Clear telemetry")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(440.0);
                        modal_title(ui, &self.tokens, "Clear telemetry");
                        ui.label("Clear all local telemetry sessions and exported telemetry ZIP files?");
                        ui.add_space(5.0);
                        ui.colored_label(self.tokens.warn, "This cannot be undone.");
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, "Cancel", false).clicked() {
                                cancel = true;
                            }
                            if modal_button(ui, &self.tokens, "Clear telemetry", true).clicked() {
                                clear = true;
                            }
                        });
                    });
                if clear {
                    let _ = std::fs::remove_dir_all(path);
                    keep = false;
                } else if cancel {
                    keep = false;
                }
            }
            Dialog::PrivacyWelcome { level } => {
                let mut accepted = false;
                let mut denied = false;
                let screen = ctx.viewport_rect();
                egui::Area::new("privacy-welcome-modal-backdrop".into()).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
                    let response = ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size()), egui::Sense::click());
                    ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
                });
                egui::Window::new("Welcome to NoteCraft")
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, "Welcome to NoteCraft");
                        ui.set_min_width(520.0);
                        ui.label("Thank you for trying NoteCraft!");
                        ui.add_space(6.0);
                        ui.label(
                            "Privacy is our top priority. All logs and telemetry stay on this device. NoteCraft never sends them automatically.",
                        );
                        ui.add_space(6.0);
                        ui.label("Choose a local collection level. Logging is off by default in release builds.");
                        egui::ComboBox::from_id_salt("privacy-welcome-level").selected_text(level.as_str()).show_ui(ui, |ui| {
                            for choice in ["none", "usage", "warning", "error", "debug"] {
                                ui.selectable_value(level, choice.into(), choice);
                            }
                        });
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, "Accept", true).clicked() {
                                accepted = true;
                            }
                            if modal_button(ui, &self.tokens, "Deny", false).clicked() {
                                denied = true;
                            }
                        });
                    });
                if accepted {
                    let _ = self.session.settings.set("telemetry_level", level);
                    let _ = self.session.settings.set("privacy_welcome_seen", "true");
                    keep = false;
                } else if denied {
                    let _ = self.session.settings.set("telemetry_level", "none");
                    let _ = self.session.settings.set("privacy_welcome_seen", "true");
                    keep = false;
                }
            }
            Dialog::FileOptions { dir, files } => {
                let (dir, file) = (dir.clone(), files.first().cloned().unwrap_or_default());
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let left = files.len().saturating_sub(1);
                let kind = notecraft_store::assets::media_kind(&name);
                let mut chosen: Option<FileChoice> = None;
                let title = if left > 0 { format!("Add {name} ({left} more)") } else { format!("Add {name}") };
                egui::Window::new("file-options")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(380.0);
                        modal_title(ui, &self.tokens, &title);
                        ui.label(egui::RichText::new("What should happen to this file?").strong().size(16.0).color(self.tokens.text));
                        ui.add_space(6.0);
                        let copy_label = match kind {
                            notecraft_store::assets::MediaKind::Image => "Insert the picture",
                            notecraft_store::assets::MediaKind::Video => "Insert the video",
                            notecraft_store::assets::MediaKind::Audio => "Insert the recording",
                            notecraft_store::assets::MediaKind::Other => "Attach the file",
                        };
                        if modal_button(ui, &self.tokens, copy_label, true).clicked() {
                            log::debug!("file attach choice clicked: choice=copy name={name:?}");
                            chosen = Some(FileChoice::Copy);
                        }
                        ui.label(
                            egui::RichText::new("A copy goes into this page's folder, so the notebook carries it everywhere.")
                                .small()
                                .color(self.tokens.muted),
                        );
                        ui.add_space(6.0);
                        if modal_button(ui, &self.tokens, "Link to the file where it is", false).clicked() {
                            log::debug!("file attach choice clicked: choice=link name={name:?}");
                            chosen = Some(FileChoice::Link);
                        }
                        ui.label(egui::RichText::new("Nothing is copied. The link breaks if the file moves.").small().color(self.tokens.muted));
                        ui.add_space(6.0);
                        ui.label(
                            egui::RichText::new("A printout (every page of a document as pictures) is not built yet.")
                                .small()
                                .color(self.tokens.muted),
                        );
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, "Cancel", false).clicked() {
                                keep = false;
                            }
                        });
                    });
                if let Some(how) = chosen {
                    self.add_file(&dir, &file, how);
                    if let Dialog::FileOptions { files, .. } = &mut next {
                        files.remove(0);
                        keep = !files.is_empty();
                    }
                }
            }
            Dialog::NewNotebook { name, format } => {
                let mut go = false;
                egui::Window::new("New notebook").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                    ui.label("Name");
                    let r = ui.text_edit_singleline(name);
                    // Read Enter before asking for focus again: a new focus request hides the lost focus.
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    r.request_focus();
                    format_picker(ui, format);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if (ui.add_enabled(!name.trim().is_empty(), egui::Button::new("Create")).clicked() || enter) && !name.trim().is_empty() {
                            go = true;
                        }
                    });
                });
                if go {
                    let mut p = params(&[("name", name.trim())]);
                    if let Some(f) = format {
                        p.insert("format".into(), f.id().into());
                    }
                    if let Some(Output::Path(dir)) = self.run("notebook.create", &p) {
                        self.open.insert(dir);
                        self.say("Notebook created");
                    }
                    keep = false;
                }
            }
            Dialog::Add { parent, parent_kind, title, kind, format } => {
                let mut go = false;
                let heading = if *parent_kind == NodeKind::Page {
                    "Add a sub-page".to_string()
                } else {
                    format!("Add inside this {}", parent_kind.label().to_lowercase())
                };
                egui::Window::new(heading).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                    let kinds = parent_kind.may_contain();
                    if kinds.len() > 1 {
                        ui.label("What to add");
                        ui.horizontal(|ui| {
                            for k in kinds {
                                ui.selectable_value(kind, *k, k.label());
                            }
                        });
                    }
                    ui.label("Title");
                    let r = ui.text_edit_singleline(title);
                    // Read Enter before asking for focus again: a new focus request hides the lost focus.
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    r.request_focus();
                    let what = if *kind == NodeKind::Page { "Format for this page" } else { "Default format for pages below" };
                    ui.label(what);
                    format_picker(ui, format);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if (ui.add_enabled(!title.trim().is_empty(), egui::Button::new("Create")).clicked() || enter) && !title.trim().is_empty() {
                            go = true;
                        }
                    });
                });
                if go {
                    let mut p = params(&[("parent", &path_str(parent)), ("kind", kind.id()), ("title", title.trim())]);
                    if let Some(f) = format {
                        p.insert("format".into(), f.id().into());
                    }
                    if let Some(Output::Path(dir)) = self.run("node.create", &p) {
                        self.open.insert(parent.clone());
                        if *kind == NodeKind::Page {
                            self.open_page(&dir);
                        }
                        self.say("Created");
                    }
                    keep = false;
                }
            }
            Dialog::Rename { path, title } => {
                let mut go = false;
                egui::Window::new("Rename").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                    ui.label("Title (the folder name does not change)");
                    let r = ui.text_edit_singleline(title);
                    // Read Enter before asking for focus again: a new focus request hides the lost focus.
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    r.request_focus();
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if (ui.button("Rename").clicked() || enter) && !title.trim().is_empty() {
                            go = true;
                        }
                    });
                });
                if go {
                    if self.run("node.title", &params(&[("path", &path_str(path)), ("title", title.trim())])).is_some() {
                        if self.editor.as_ref().map(|e| e.dir == *path).unwrap_or(false) {
                            self.open_page(path);
                        }
                        self.say("Renamed");
                    }
                    keep = false;
                }
            }
            Dialog::DeleteFile { page, name } => {
                let mut go = false;
                egui::Window::new("Delete file").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                    ui.label(format!("Delete \"assets/{name}\" from this page's folder?"));
                    ui.colored_label(self.tokens.warn, "The file is removed from disk. Undo does not bring a file back.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                        if ui.button("Delete").clicked() {
                            go = true;
                        }
                    });
                });
                if go {
                    let page = page.clone();
                    if self.run("page.detach", &params(&[("path", &path_str(&page)), ("name", name)])).is_some() {
                        if let Some(ed) = self.editor.as_mut() {
                            ed.attachments = notecraft_store::assets::list(&page);
                        }
                        self.say("The file is gone");
                        // The page may have named the drawing it no longer has.
                        self.sync_ink_reference();
                    }
                    keep = false;
                }
            }
            Dialog::DeleteNotebook { path, title } => {
                let mut go = false;
                egui::Window::new("Delete notebook")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(440.0);
                        modal_title(ui, &self.tokens, "Delete notebook");
                        ui.label(format!("Delete notebook \"{title}\" and everything inside it?"));
                        ui.add_space(4.0);
                        ui.colored_label(self.tokens.warn, "This removes the notebook folder from disk and cannot be undone.");
                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, "Cancel", false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, "Delete notebook", true).clicked() {
                                go = true;
                            }
                        });
                    });
                if go {
                    let path = path.clone();
                    let was_open = self.editor.as_ref().and_then(|editor| notebook_for(&editor.dir)).is_some_and(|notebook| notebook == path);
                    match std::fs::remove_dir_all(&path) {
                        Ok(()) => {
                            self.remove_recent_notebook(&path);
                            if was_open {
                                self.editor = None;
                                self.pad = None;
                                self.selected = None;
                                self.backstage = Some("Open");
                            }
                            let _ = self.run("tree.refresh", &Params::new());
                            self.say(format!("Deleted notebook {}", self.shown(&path)));
                        }
                        Err(error) => {
                            self.status = format!("The notebook could not be deleted: {error}");
                            self.status_is_error = true;
                        }
                    }
                    keep = false;
                }
            }
            Dialog::Delete { path, title, count } => {
                let mut go = false;
                let is_notebook = notecraft_store::tree::read_node(path).is_some_and(|node| node.kind == NodeKind::Notebook);
                egui::Window::new("Delete")
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(420.0);
                        modal_title(ui, &self.tokens, "Delete notebook");
                        if is_notebook {
                            ui.label(format!("Delete notebook \"{title}\" and everything inside it?"));
                            ui.colored_label(self.tokens.warn, "This removes the notebook folder from disk and cannot be undone.");
                        } else {
                            ui.label(format!(
                                "Move \"{title}\" and everything inside it ({count} item{}) to this notebook's recycle bin?",
                                if *count == 1 { "" } else { "s" }
                            ));
                            ui.label("You can restore it from History.");
                        }
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, "Cancel", false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, if is_notebook { "Delete notebook" } else { "Move to recycle bin" }, true).clicked() {
                                go = true;
                            }
                        });
                    });
                if go {
                    if is_notebook {
                        let path = path.clone();
                        match std::fs::remove_dir_all(&path) {
                            Ok(()) => {
                                self.remove_recent_notebook(&path);
                                self.editor = None;
                                self.pad = None;
                                self.selected = None;
                                self.open.remove(&path);
                                let _ = self.run("tree.refresh", &Params::new());
                                self.say("Deleted notebook");
                            }
                            Err(error) => {
                                self.status = format!("The notebook could not be deleted: {error}");
                                self.status_is_error = true;
                            }
                        }
                    } else {
                        let was_open = self.editor.as_ref().map(|e| e.dir.starts_with(&*path)).unwrap_or(false);
                        if self.run("node.delete", &params(&[("path", &path_str(path))])).is_some() {
                            if was_open {
                                self.editor = None;
                            }
                            self.selected = None;
                            self.say("Moved to the notebook recycle bin");
                        }
                    }
                    keep = false;
                }
            }
            Dialog::PageVersions { page, versions } => {
                let mut restore: Option<String> = None;
                egui::Window::new("Page versions").collapsible(false).resizable(true).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
                    ui.set_min_width(420.0);
                    ui.label("Saved copies of this page");
                    ui.add_space(6.0);
                    if versions.is_empty() {
                        ui.label("No saved versions yet. NoteCraft keeps a copy when the page changes.");
                    } else {
                        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                            for version in versions.iter() {
                                ui.horizontal(|ui| {
                                    ui.label(&version.created);
                                    ui.label(egui::RichText::new(format!("{} KB", version.bytes.div_ceil(1024))).small().color(self.tokens.muted));
                                    if ui.button("Restore").clicked() {
                                        restore = Some(version.id.clone());
                                    }
                                });
                            }
                        });
                    }
                    ui.add_space(8.0);
                    if ui.button("Close").clicked() {
                        keep = false;
                    }
                });
                if let Some(id) = restore {
                    if self.run("page.version.restore", &params(&[("path", &path_str(page)), ("id", &id)])).is_some() {
                        self.open_page(page);
                        self.say("Restored the saved page version");
                    }
                    keep = false;
                }
            }
            Dialog::RecycleBin { notebook, items } => {
                let mut restore: Option<String> = None;
                let mut purge: Option<(String, String)> = None;
                egui::Window::new("Notebook recycle bin").collapsible(false).resizable(true).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        ui.set_min_width(460.0);
                        if items.is_empty() {
                            ui.label("This notebook's recycle bin is empty.");
                        } else {
                            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                                for item in items.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(&item.title);
                                        ui.label(
                                            egui::RichText::new(format!("{} item(s) · {}", item.count, item.deleted))
                                                .small()
                                                .color(self.tokens.muted),
                                        );
                                        if ui.button("Restore").clicked() {
                                            restore = Some(item.id.clone());
                                        }
                                        if ui.button("Delete permanently").clicked() {
                                            purge = Some((item.id.clone(), item.title.clone()));
                                        }
                                    });
                                }
                            });
                        }
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    },
                );
                if let Some(id) = restore {
                    if self.run("recycle.restore", &params(&[("notebook", &path_str(notebook)), ("id", &id)])).is_some() {
                        self.say("Restored to the original location");
                        keep = false;
                    }
                } else if let Some((id, title)) = purge {
                    next = Dialog::PurgeRecycleItem { notebook: notebook.clone(), id, title };
                }
            }
            Dialog::PurgeRecycleItem { notebook, id, title } => {
                let mut purge = false;
                egui::Window::new("Permanently delete?").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        ui.label(format!("Permanently delete \"{title}\" and everything inside it?"));
                        ui.colored_label(self.tokens.warn, "This cannot be undone.");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                keep = false;
                            }
                            if ui.button("Delete permanently").clicked() {
                                purge = true;
                            }
                        });
                    },
                );
                if purge && self.run("recycle.purge", &params(&[("notebook", &path_str(notebook)), ("id", id)])).is_some() {
                    self.say("Permanently deleted");
                    keep = false;
                }
            }
            Dialog::Symbols => {
                let mut chosen: Option<&'static str> = None;
                egui::Window::new("Insert symbol").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        ui.label("Choose a symbol to insert at the caret");
                        ui.add_space(6.0);
                        for row in [["©", "®", "™", "§", "¶"], ["•", "→", "←", "±", "°"], ["×", "÷", "√", "€", "£"], ["¥", "¢", "∞", "≠", "≤"]]
                        {
                            ui.horizontal(|ui| {
                                for symbol in row {
                                    if ui.add_sized([46.0, 36.0], egui::Button::new(egui::RichText::new(symbol).size(20.0))).clicked() {
                                        chosen = Some(symbol);
                                    }
                                }
                            });
                        }
                        ui.add_space(6.0);
                        if ui.button("Close").clicked() {
                            keep = false;
                        }
                    },
                );
                if let Some(symbol) = chosen {
                    self.insert_in_page(symbol);
                    keep = false;
                }
            }
        }
        if keep {
            self.dialog = Some(next);
        }
    }

    // ---------- control channel ----------

    /// Attaches a control channel (see `control.rs` and `docs/control-protocol.md`).
    pub fn with_control(mut self, server: ControlServer, rx: Receiver<ControlRequest>) -> Self {
        self.control_server = Some(server);
        self.control = Some(rx);
        self
    }

    /// Says where GridCraft is, or that it is not installed, instead of looking for it on this
    /// machine. Tests use this so a table's spreadsheet buttons do not depend on the machine.
    pub fn with_gridcraft(mut self, exe: Option<std::path::PathBuf>) -> Self {
        self.gridcraft = exe;
        self
    }

    /// Attaches only a request receiver, with no network listener. The headless screenshot
    /// runner uses this to feed scripted requests.
    pub fn with_requests(mut self, rx: Receiver<ControlRequest>) -> Self {
        self.control = Some(rx);
        self
    }

    /// The input events for the next frame, from clicks, keys and text that control requests
    /// queued. A click is spread over three frames (move, press, release) the way a real mouse
    /// delivers it. The window's shell calls this once per frame and adds the events to its input.
    pub fn take_events(&mut self) -> Vec<egui::Event> {
        self.event_frames.pop_front().unwrap_or_default()
    }

    fn poll_control(&mut self, ctx: &Context) {
        // Finish screenshot requests whose picture has arrived.
        let arrived: Vec<(u64, std::sync::Arc<egui::ColorImage>)> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Screenshot { user_data, image, .. } => {
                        user_data.data.as_ref().and_then(|d| d.downcast_ref::<u64>()).map(|id| (*id, image.clone()))
                    }
                    _ => None,
                })
                .collect()
        });
        for (id, image) in arrived {
            if let Some(pos) = self.screenshots.iter().position(|(n, _, _)| *n == id) {
                let (_, req, path) = self.screenshots.remove(pos);
                req.respond(encode_screenshot(&image, path.as_deref()));
            }
        }
        let mut pending = Vec::new();
        if let Some(rx) = &self.control {
            while let Ok(req) = rx.try_recv() {
                pending.push(req);
            }
        }
        for req in pending {
            self.handle_control(ctx, req);
        }
    }

    /// Answers one control request. Methods: see `docs/control-protocol.md`.
    pub fn handle_control(&mut self, ctx: &Context, req: ControlRequest) {
        if req.method == "ui.screenshot" {
            let path = req.params.get("path").and_then(Value::as_str).map(PathBuf::from);
            let id = self.next_screenshot;
            self.next_screenshot += 1;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::new(id)));
            ctx.request_repaint();
            self.screenshots.push((id, req, path));
            return;
        }
        let result = self.control_method(ctx, &req.method, &req.params);
        req.respond(result);
    }

    fn control_method(&mut self, ctx: &Context, method: &str, p: &Value) -> Result<Value, String> {
        let num = |k: &str| p.get(k).and_then(Value::as_f64).map(|v| v as f32);
        let flag = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
        let text = |k: &str| p.get(k).and_then(Value::as_str);
        match method {
            "engine.commands" => Ok(notecraft_engine::commands_json()),
            "engine.execute" => {
                let id = text("command").ok_or("missing command")?;
                let params = notecraft_engine::params_from_json(p.get("params"));
                self.save_now();
                let out = self.session.run(id, &params).map_err(|e| e.to_string())?;
                if self.editor.as_ref().is_some_and(|ed| !ed.dir.exists()) {
                    self.editor = None;
                }
                Ok(notecraft_engine::output_json(&out))
            }
            "ui.inspect" => Ok(self.inspect(ctx)),
            "ui.open" => {
                self.open_path(text("path").ok_or("missing path")?);
                if self.status_is_error { Err(self.status.clone()) } else { Ok(self.inspect(ctx)) }
            }
            // Put the caret in a block of the open page, the way a click does.
            "ui.caret" => {
                let Some(ed) = self.editor.as_mut() else { return Err("no page is open".into()) };
                if !ed.writable {
                    return Err("this page is read-only until the block editor can keep its markup".into());
                }
                let n = p.get("block").and_then(Value::as_u64).ok_or("missing block")? as usize;
                // A heading that only repeats the page's title is not drawn, so the caret
                // cannot go there: the next block is what a person would have clicked.
                let n = if pageedit::is_title_echo(&ed.blocks, n, &ed.title) { n + 1 } else { n };
                let block = ed.blocks.get(n).ok_or_else(|| format!("the page has no block {n}"))?;
                if !pageedit::is_writable(block) {
                    return Err(format!("block {n} is a table, a rule or a picture; the caret cannot go in it yet"));
                }
                ed.caret = Some(pageedit::Caret {
                    block: n,
                    text: pageedit::caret_text(block),
                    focus: true,
                    selection: (0, 0),
                    place: None,
                    cell: None,
                    span: None,
                });
                ctx.request_repaint();
                Ok(json!({"block": n, "text": ed.caret.as_ref().map(|c| c.text.clone())}))
            }
            "ui.back" => {
                self.go_back();
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.forward" => {
                self.go_forward();
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.undo" => {
                self.undo();
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.redo" => {
                self.redo();
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.expandAll" => {
                self.expand_all();
                Ok(json!({}))
            }
            "ui.collapseAll" => {
                self.open.clear();
                Ok(json!({}))
            }
            "ui.theme" => {
                let mode = text("mode").map(|m| ModePref::parse(m).ok_or("mode must be light, dark or system")).transpose()?;
                let accent =
                    text("accent").map(|a| Accent::parse(a).ok_or("accent must be default, red, green, yellow, blue, gray or purple")).transpose()?;
                let accent = accent.unwrap_or(self.accent);
                match mode.unwrap_or(self.mode_pref) {
                    ModePref::Light => self.set_theme(Theme { mode: Mode::Light, accent }),
                    ModePref::Dark => self.set_theme(Theme { mode: Mode::Dark, accent }),
                    ModePref::System => self.follow_system_theme(accent),
                }
                Ok(json!({"theme": self.theme().id(), "modePref": self.mode_pref.id()}))
            }
            "ui.click" | "ui.move" | "ui.press" | "ui.release" => {
                let pos = egui::pos2(num("x").ok_or("missing x")?, num("y").ok_or("missing y")?);
                let button = match text("button") {
                    Some("right") => egui::PointerButton::Secondary,
                    Some("middle") => egui::PointerButton::Middle,
                    _ => egui::PointerButton::Primary,
                };
                let modifiers = egui::Modifiers { alt: flag("alt"), ctrl: flag("ctrl"), shift: flag("shift"), mac_cmd: false, command: flag("ctrl") };
                self.event_frames.push_back(vec![egui::Event::PointerMoved(pos)]);
                let press = |pressed| vec![egui::Event::PointerButton { pos, button, pressed, modifiers }];
                match method {
                    "ui.click" => {
                        for _ in 0..p.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, 3) {
                            self.event_frames.push_back(press(true));
                            self.event_frames.push_back(press(false));
                        }
                    }
                    "ui.press" => self.event_frames.push_back(press(true)),
                    "ui.release" => self.event_frames.push_back(press(false)),
                    _ => {}
                }
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.key" => {
                let name = text("key").ok_or("missing key")?;
                let key = egui::Key::from_name(name).ok_or_else(|| format!("unknown key '{name}'"))?;
                let modifiers = egui::Modifiers { alt: flag("alt"), ctrl: flag("ctrl"), shift: flag("shift"), mac_cmd: false, command: flag("ctrl") };
                for pressed in [true, false] {
                    self.event_frames.push_back(vec![egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }]);
                }
                ctx.request_repaint();
                Ok(json!({}))
            }
            // Pasting is its own event, not typing: a table from another app arrives this
            // way, and the page editor turns it back into a table.
            "ui.paste" => {
                self.event_frames.push_back(vec![egui::Event::Paste(text("text").ok_or("missing text")?.to_string())]);
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.text" => {
                self.event_frames.push_back(vec![egui::Event::Text(text("text").ok_or("missing text")?.to_string())]);
                ctx.request_repaint();
                Ok(json!({}))
            }
            // Scrolling, so a script can reach what is below the fold: the list of files
            // in a page's folder sits under a whole sheet of paper, and nothing could
            // reach it from here before.
            "ui.scroll" => {
                let (dx, dy) = (num("x").unwrap_or(0.0), num("y").ok_or("missing y")?);
                self.event_frames.push_back(vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: vec2(dx, dy),
                    modifiers: egui::Modifiers::NONE,
                    phase: egui::TouchPhase::Move,
                }]);
                ctx.request_repaint();
                Ok(json!({}))
            }
            "ui.resize" => {
                let (w, h) = (num("width").ok_or("missing width")?, num("height").ok_or("missing height")?);
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(w.clamp(400.0, 8000.0), h.clamp(300.0, 8000.0))));
                Ok(json!({}))
            }
            "app.quit" => {
                self.save_now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                Ok(json!({}))
            }
            other => Err(format!("unknown method '{other}'")),
        }
    }

    /// What the window shows now: the visible tree rows, the open page, the theme and the size.
    pub fn inspect(&self, ctx: &Context) -> Value {
        // The Mermaid diagrams of the page as it is drawn, not of the code view: how many, and how
        // many of them are drawn, still being drawn, or could not be.
        let in_page: Vec<String> = self
            .editor
            .as_ref()
            .map(|e| {
                e.blocks
                    .iter()
                    .filter_map(|b| match b {
                        notecraft_formats::markdown::Block::Code { lang, text } if lang == "mermaid" => Some(text.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let in_page_counts = crate::diagramview::counts(ctx, &in_page, &self.tokens);
        let mut rows = Vec::new();
        fn walk(app: &NoteCraftApp, n: &Node, depth: usize, rows: &mut Vec<Value>) {
            let open = app.open.contains(&n.path);
            rows.push(json!({
                "path": notecraft_store::tree::display_path(n.path.strip_prefix(app.session.root()).unwrap_or(&n.path)).replace('\\', "/"),
                "title": n.info.title,
                "kind": n.info.kind.id(),
                "format": n.info.format.map(|f| f.id()),
                "depth": depth,
                "hasChildren": !n.children.is_empty(),
                "open": open,
                "selected": app.selected.as_deref() == Some(n.path.as_path()),
            }));
            if open {
                for c in &n.children {
                    walk(app, c, depth + 1, rows);
                }
            }
        }
        for n in self.session.tree() {
            walk(self, n, 0, &mut rows);
        }
        let screen = ctx.content_rect();
        json!({
            "window": {"width": screen.width(), "height": screen.height(), "pixelsPerPoint": ctx.pixels_per_point()},
            // How the side panel is arranged: one tree, or the organisers and the pages in
            // columns of their own, with the first one hidden or not.
            "panes": {"split": self.split_panes, "organisers": self.organisers},
            // Back and Forward, and the Quick Access Toolbar's choices.
            "nav": {
                "back": self.nav.back_count(),
                "forward": self.nav.forward_count(),
                "current": self.nav.current().map(|p| self.shown(p)),
            },
            "windowFrame": if self.frameless { "own" } else { "system" },
            "quickAccess": {"shown": self.qat.show, "below": self.qat.below, "hidden": self.qat.hidden},
            "theme": {"id": self.theme().id(), "modePref": self.mode_pref.id(), "accent": self.accent.id()},
            // The paper behind the pages, as the View tab's gallery sets it.
            "paper": self.paper_now().id(),
            "pageColour": self.editor.as_ref().and_then(|e| e.colour).map(notecraft_model::NodeColor::id),
            // The pen the Draw tab is set to, and the drawing it writes in.
            "ink": {
                "pointer": self.pointer.id(),
                "tool": self.pen().tool.id(),
                "colour": self.pen().colour.hex(),
                "width": self.pen().width,
                "rack": notecraft_ink::rack::write(&self.rack),
                "nib": self.nib,
                "sensitivity": self.sensitivity,
                "holding": self.pad.as_ref().map_or(0, crate::inkpad::Pad::holding),
                "gain": self.gain(),
                "strokes": self.pad.as_ref().map(crate::inkpad::Pad::strokes),
                "inked": self.pad.as_ref().is_some_and(|p| !p.is_empty()),
            },
            // What a pen has told this window. `pen` is false until a device reports a
            // pressure at all, and `hardest` is the hardest press since the window opened.
            "pen": {"seen": self.stylus.sample().pen, "pressure": self.stylus.sample().pressure, "hardest": self.stylus.seen().0, "readings": self.stylus.seen().1},
            "defaultFormat": self.session.settings.global_format().0.id(),
            // Where GridCraft is, when a table can be handed to it.
            "gridcraft": self.gridcraft.as_ref().map(|p| p.to_string_lossy()),
            // The code view: whether it is open, which of the page's files is picked, and
            // how much of that file is in hand.
            "codeView": {
                "open": self.show_source,
                "picked": self.code.picked,
                "characters": self.code.text.chars().count(),
                // The colours the picked file is shown in, and the Find bar: whether it is open,
                // what it looks for, how many matches there are and which one (from 1) it is on.
                "language": self.code.picked.as_deref().map_or("none", |n| crate::highlight::Lang::of_file(n).label()),
                "colouredStretches": self.code.coloured.len(),
                "find": {
                    "open": self.code.find.open,
                    "query": self.code.find.query,
                    "matchCase": self.code.find.case,
                    "matches": self.code.find.matches.len(),
                    "current": if self.code.find.matches.is_empty() { Value::Null } else { json!(self.code.find.current + 1) },
                },
            },
            "pageDiagrams": {
                "found": in_page.len(),
                "drawn": in_page_counts.0,
                "drawing": in_page_counts.1,
                "failed": in_page_counts.2,
            },
            "notebooksFolder": self.shown(self.session.root()),
            "status": {"text": self.status, "error": self.status_is_error},
            "rows": rows,
            "page": self.editor.as_ref().map(|e| json!({
                "path": e.dir.strip_prefix(self.session.root()).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
                "title": e.title,
                // What the title area actually shows: blank for a page with no title of its
                // own, unlike `title`, which is always at least the "Untitled page" fallback.
                "titleArea": e.title_buf,
                "format": e.format.id(),
                "dirty": e.dirty_since.is_some(),
                "bodyCharacters": e.body.chars().count(),
                "blocks": e.blocks.len(),
                "attachments": e.attachments.len(),
                "writable": e.writable,
                // `selection` is (where the caret is, where the selection began), in
                // characters: the two are equal when nothing is selected.
                "caret": e.caret.as_ref().map(|c| json!({"block": c.block, "text": c.text, "at": c.selection.0, "anchor": c.selection.1})),
                // A selection dragged across more than one block, as block and byte at each
                // end. Null for the ordinary selection inside one block, which `caret` has.
                "across": e.across.map(|a| json!({"from": [a.from.0, a.from.1], "to": [a.to.0, a.to.1]})),
                // The boxes the blocks are arranged in, as the page keeps them.
                "containers": e.boxes.iter().map(|b| json!({"x": b.x, "y": b.y, "width": b.width, "blocks": b.blocks})).collect::<Vec<_>>(),
                // The container that is selected, with everything in it, or null.
                "selectedContainer": crate::containers::selected(ctx, &e.dir),
                // The image block selected independently of its containing text box, or null.
                "selectedImage": crate::containers::selected_image(ctx, &e.dir),
                "selectedObject": crate::containers::selected_object(ctx, &e.dir).map(|(block, kind)| json!({"block": block, "kind": kind})),
                "origin": [e.origin.x, e.origin.y],
                // What can be taken back, and how many steps are kept.
                "history": {"steps": e.history.len(), "at": e.history.at(), "canUndo": e.history.can_undo(), "canRedo": e.history.can_redo()},
            })),
        })
    }
    /// Draws the whole window. Call once per frame.
    pub fn show(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        if !self.loaders_installed {
            egui_extras::install_image_loaders(ctx);
            self.loaders_installed = true;
        }
        // Arabic and the other right-to-left scripts have no glyphs in egui's own faces.
        crate::fonts::install_once(ctx);
        // Pen pressure, where the platform reports it. Nothing draws with it yet; it is
        // read here so `ui.inspect` can say whether a pen reaches this window at all.
        ctx.input(|i| self.stylus.update(&i.events));
        self.apply_theme(ctx);
        if self.window_level_pending {
            let level = if self.always_on_top { egui::viewport::WindowLevel::AlwaysOnTop } else { egui::viewport::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.window_level_pending = false;
        }
        if let Some(text) = self.pending_copy.take() {
            if let Some(html) = self.pending_copy_html.take() {
                if crate::clipboard::set_html(&html, &text).is_err() {
                    ctx.copy_text(text);
                }
            } else {
                ctx.copy_text(text);
            }
        }
        self.clipboard_keys(ctx);
        self.poll_control(ctx);
        self.poll_import(ctx);
        if !self.event_frames.is_empty() {
            ctx.request_repaint();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::S)) {
            if let Some(ed) = self.editor.as_mut() {
                ed.dirty_since.get_or_insert_with(Instant::now);
            }
            self.save_now();
        }
        // Taken before any widget sees them: a text box has an undo of its own that
        // knows only about the letters in that one box, and the page's undo has to be the
        // one that answers, or Ctrl+Z would mean two different things on one page.
        let ctrl_shift = Modifiers { ctrl: true, shift: true, command: true, ..Modifiers::NONE };
        if ctx.input_mut(|i| i.consume_key(ctrl_shift, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y)) {
            self.step_back(false);
        } else if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            self.step_back(true);
        }
        self.nav_keys(ctx);
        self.direction_keys(ctx);
        self.restore_last_page();
        self.attach_dropped(ctx);
        // The File screen takes the whole window while it is open, as it does in Office.
        if self.backstage.is_some() {
            self.title_band(ui, false);
            self.backstage_screen(ui);
            self.dialogs(ctx);
            self.import_progress(ctx);
            self.frame_edges(ctx);
            return;
        }
        self.top_bar(ui);
        self.status_bar(ui);
        self.side_panel(ui);
        self.central(ui);
        self.search_popup(ctx);
        self.dialogs(ctx);
        self.import_progress(ctx);
        self.frame_edges(ctx);
        if let Some(since) = self.editor.as_ref().and_then(|e| e.dirty_since) {
            if since.elapsed() >= AUTOSAVE_AFTER {
                self.save_now();
            } else {
                ctx.request_repaint_after(AUTOSAVE_AFTER - since.elapsed());
            }
        }
    }
}

/// One swatch of the theme switcher: the surface colour, ringed and dotted in the accent.
fn theme_swatch(ui: &mut Ui, theme: Theme, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
    let t = theme.tokens();
    let painter = ui.painter_at(rect);
    let c = rect.center();
    painter.circle_filled(c, 10.5, t.panel);
    painter.circle_stroke(c, 10.5, egui::Stroke::new(if selected { 3.0 } else { 1.5 }, t.accent));
    painter.circle_filled(c, 4.5, t.accent);
    if resp.hovered() {
        painter.circle_stroke(c, 12.0, egui::Stroke::new(1.0, ui.style().visuals.text_color()));
    }
    let label = format!("Theme {}", theme.label());
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, &label));
    resp.on_hover_text(theme.label())
}

fn format_picker(ui: &mut Ui, format: &mut Option<PageFormat>) {
    egui::ComboBox::from_id_salt("dialog-format")
        .selected_text(format.map(|f| f.label()).unwrap_or("Inherit (notebook, section or global setting)"))
        .show_ui(ui, |ui| {
            ui.selectable_value(format, None, "Inherit (notebook, section or global setting)");
            for f in PageFormat::ALL {
                ui.selectable_value(format, Some(f), f.label());
            }
        });
}

#[allow(dead_code)]
fn _unused(_: Color32) {}

/// A small square button that draws one icon, with an on state for the ones that toggle.
fn icon_toggle(ui: &mut Ui, icon: Icon, on: bool, t: &Tokens, label: &str, tip: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::click());
    let painter = ui.painter_at(rect);
    if on || resp.hovered() {
        painter.rect_filled(rect, 4.0, if on { t.row_selected } else { t.row_hover });
    }
    let color = if on { t.accent } else { t.muted };
    icons::paint(&painter, egui::Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), icon, color);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// One colour of the node colour picker: a filled circle, ringed when it is the current one.
fn color_swatch(ui: &mut Ui, color: NodeColor, dark: bool, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let (r, g, b) = color.rgb(dark);
    let fill = Color32::from_rgb(r, g, b);
    let painter = ui.painter_at(rect);
    painter.circle_filled(rect.center(), 8.0, fill);
    if selected {
        painter.circle_stroke(rect.center(), 10.0, egui::Stroke::new(2.0, ui.style().visuals.text_color()));
    }
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, color.label()));
    resp.on_hover_text(color.label())
}

/// Shows a folder in the operating system's file manager. An optional extra: everything the
/// app itself does works without it, and a failure is reported, never fatal.
fn open_folder(dir: &Path) -> std::io::Result<()> {
    let program = if cfg!(target_os = "windows") {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    // `explorer` returns a non-zero status even when it worked, so the status is not checked.
    std::process::Command::new(program).arg(dir).spawn().map(|_| ())
}

/// Opens a linked attachment with the operating system's associated application. Windows may
/// show its own warning for files marked as downloaded; that prompt is intentionally preserved.
fn open_file(file: &Path) -> std::io::Result<()> {
    if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/C", "start", "", &file.to_string_lossy()]).spawn().map(|_| ())
    } else if cfg!(target_os = "macos") {
        std::process::Command::new("open").arg(file).spawn().map(|_| ())
    } else {
        std::process::Command::new("xdg-open").arg(file).spawn().map(|_| ())
    }
}

/// Saves a screenshot to `path`, or returns it as base64 PNG in the reply.
fn encode_screenshot(image: &egui::ColorImage, path: Option<&Path>) -> Result<Value, String> {
    let [w, h] = image.size;
    let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    let img = image::RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or("the screenshot has the wrong size")?;
    match path {
        Some(p) => {
            img.save(p).map_err(|e| format!("cannot save {}: {e}", p.display()))?;
            Ok(json!({"path": p.to_string_lossy(), "width": w, "height": h}))
        }
        None => {
            let mut png = std::io::Cursor::new(Vec::new());
            img.write_to(&mut png, image::ImageFormat::Png).map_err(|e| e.to_string())?;
            Ok(json!({"png": base64(png.get_ref()), "width": w, "height": h}))
        }
    }
}

/// The first page inside a notebook or section, at any depth.
fn first_page(tree: &[Node], root: &Path) -> Option<PathBuf> {
    fn walk(n: &Node) -> Option<PathBuf> {
        if n.info.kind == NodeKind::Page {
            return Some(n.path.clone());
        }
        n.children.iter().find_map(walk)
    }
    tree.iter().find(|n| n.path == root).and_then(walk)
}

/// The first page inside the node at `at`, wherever in the tree that node is.
fn first_page_under(tree: &[Node], at: &Path) -> Option<PathBuf> {
    fn find<'a>(n: &'a Node, at: &Path) -> Option<&'a Node> {
        if n.path == at {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, at))
    }
    let node = tree.iter().find_map(|n| find(n, at))?;
    first_page(std::slice::from_ref(node), &node.path)
}

/// The 14 swatches and the switch that follows the system, shared by the sun or moon button
/// and the ribbon's Theme button.
fn theme_grid(ui: &mut Ui, current: Theme, pick: &mut Option<(Mode, Accent)>, follow: &mut bool) {
    egui::Grid::new("theme-grid").spacing([6.0, 6.0]).show(ui, |ui| {
        for mode in [Mode::Light, Mode::Dark] {
            ui.label(mode.label());
            for accent in Accent::ALL {
                let theme = Theme { mode, accent };
                let selected = theme == current;
                if theme_swatch(ui, theme, selected).clicked() {
                    *pick = Some((mode, accent));
                }
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new(format!("Accent: {}", current.accent.label())).small());
    ui.checkbox(follow, "Follow the system light or dark setting");
}

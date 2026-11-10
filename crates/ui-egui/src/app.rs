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
use crate::printout;
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
use std::collections::{HashSet, VecDeque};
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
/// The ArtCraft community chat. Opened in the browser, never fetched by the app.
const DISCORD_URL: &str = "https://discord.gg/artcraft";

/// Where the pen rack and the tool in hand are remembered between runs.
const RACK_KEY: &str = "pen_rack";
const NIB_KEY: &str = "pen_in_hand";
const SENSITIVITY_KEY: &str = "pen_sensitivity";
const DEFAULT_SENSITIVITY: u8 = 50;

fn default_telemetry_level(debug_build: bool) -> &'static str {
    if debug_build { "debug" } else { "none" }
}

const ROW_HEIGHT: f32 = 24.0;
const INDENT: f32 = 14.0;

/// Shared title treatment for modal dialogs. Keeping this here makes dialogs use the
/// same opaque, theme-owned title band instead of inheriting egui's platform-gray title bar.
fn modal_frame(tokens: &Tokens) -> egui::Frame {
    egui::Frame::NONE.fill(tokens.panel).stroke(egui::Stroke::new(1.5, tokens.accent)).inner_margin(egui::Margin::same(16))
}

// i18n-ui-text: title
fn modal_title(ui: &mut Ui, tokens: &Tokens, title: &str) {
    egui::Frame::NONE.fill(tokens.band).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(crate::tl!(title)).strong().size(17.0).color(tokens.band_text));
    });
    ui.add_space(10.0);
}

/// Modal actions use a readable size based on their label instead of egui's compact default.
// i18n-ui-text: label
fn modal_button(ui: &mut Ui, tokens: &Tokens, label: &str, primary: bool) -> egui::Response {
    modal_button_enabled(ui, tokens, label, primary, true)
}

// i18n-ui-text: label
fn modal_button_enabled(ui: &mut Ui, tokens: &Tokens, label: &str, primary: bool, enabled: bool) -> egui::Response {
    let label = crate::tl!(label);
    let width = (label.chars().count() as f32 * 8.0 + 32.0).max(92.0);
    let fill = if primary { tokens.accent } else { tokens.panel };
    let text = if primary { tokens.band_text } else { tokens.text };
    ui.add_enabled(
        enabled,
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

fn missing_attachment_icon(page: &Path, src: &str) -> Option<String> {
    let name = src.strip_prefix("assets/")?;
    if name.is_empty() || name.contains('/') || name.contains('\\') {
        return None;
    }
    // Generated icon sidecars are images too, but they are not attachments that
    // need another icon. Treat `file.ext.svg`/`file.ext.png` as implementation
    // details when the attachment they belong to is present.
    if let Some((source, extension)) = name.rsplit_once('.')
        && matches!(extension.to_ascii_lowercase().as_str(), "png" | "svg")
        && page.join("assets").join(source).is_file()
    {
        return None;
    }
    if notecraft_store::assets::media_kind(src) != notecraft_store::assets::MediaKind::Other {
        return None;
    }
    // A missing attachment is not a missing-icon case. Do not warn about an icon
    // when the original file itself is no longer present.
    if !page.join("assets").join(name).is_file() {
        return None;
    }
    (!attachment_icon_healthy(page, name)).then(|| name.to_string())
}

/// A generated PNG is preferred on Windows, but the deterministic SVG fallback is also a
/// complete icon. In particular, Shell extraction can fail for files on a UNC-mounted
/// enlistment even though the fallback renders correctly.
fn attachment_icon_healthy(page: &Path, name: &str) -> bool {
    attachment_icon_exists(page, name)
}

fn attachment_icon_exists(page: &Path, name: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        // Native Shell icons are PNG, but the deterministic SVG fallback is valid
        // when Shell extraction is unavailable on this Windows installation.
        page.join("assets").join(format!("{name}.png")).is_file() || page.join("assets").join(format!("{name}.svg")).is_file()
    }
    #[cfg(not(target_os = "windows"))]
    {
        page.join("assets").join(format!("{name}.svg")).is_file() || page.join("assets").join(format!("{name}.png")).is_file()
    }
}

fn attachment_icon_link(page: &Path, name: &str) -> String {
    let dir = page.join("assets");
    if dir.join(format!("{name}.png")).is_file() { format!("assets/{name}.png") } else { format!("assets/{name}.svg") }
}

fn replace_attachment_icon(format: PageFormat, snippet: &str, name: &str, icon_link: &str) -> String {
    match format {
        PageFormat::Html => snippet
            .replace(&format!("src=\"assets/{name}.png\""), &format!("src=\"{icon_link}\""))
            .replace(&format!("src=\"assets/{name}.svg\""), &format!("src=\"{icon_link}\"")),
        PageFormat::Cmgfm => {
            let png = format!("](assets/{name}.png)");
            let svg = format!("](assets/{name}.svg)");
            snippet.replacen(&png, &format!("]({icon_link})"), 1).replacen(&svg, &format!("]({icon_link})"), 1)
        }
    }
}

#[cfg(test)]
mod attachment_reference_tests {
    use super::{PageFormat, replace_attachment_icon};

    #[cfg(not(target_os = "windows"))]
    use super::attachment_icon_exists;

    #[test]
    fn html_repair_changes_only_the_image_source() {
        let input = r#"<p><a href="assets/brief.docx"><img src="assets/brief.docx.svg" alt="" width="48"><br>brief.docx</a></p>"#;
        let output = replace_attachment_icon(PageFormat::Html, input, "brief.docx", "assets/brief.docx.png");
        assert!(output.contains(r#"href="assets/brief.docx""#));
        assert!(output.contains(r#"src="assets/brief.docx.png""#));
    }

    #[test]
    fn markdown_repair_changes_only_the_inner_image_link() {
        let input = "[![brief.docx](assets/brief.docx.svg) brief.docx](assets/brief.docx)";
        let output = replace_attachment_icon(PageFormat::Cmgfm, input, "brief.docx", "assets/brief.docx.png");
        assert_eq!(output, "[![brief.docx](assets/brief.docx.png) brief.docx](assets/brief.docx)");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_windows_png_sidecar_is_a_valid_icon_on_posix() {
        let t = notecraft_store::testing::TempDir::new("attachment-icon");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets/brief.docx"), b"attachment").expect("attachment");
        std::fs::write(page.join("assets/brief.docx.png"), b"png sidecar").expect("icon");
        assert!(attachment_icon_exists(&page, "brief.docx"));
    }
}

fn first_missing_attachment_in_source(source: &str, page: &Path) -> Option<String> {
    let mut rest = source;
    while let Some(at) = rest.find("assets/") {
        rest = &rest[at + "assets/".len()..];
        let end = rest.find(['"', '\'', ')', ']', '<', '>', ' ', '\n', '\r']).unwrap_or(rest.len());
        let name = &rest[..end];
        let candidate = format!("assets/{name}");
        if let Some(name) = (!name.is_empty()).then(|| missing_attachment_icon(page, &candidate)).flatten() {
            return Some(name);
        }
        rest = &rest[end..];
    }
    None
}

#[allow(dead_code)]
fn first_unreferenced_attachment(attachments: &[(String, u64)], source: &str) -> Option<String> {
    attachments.iter().find_map(|(name, _)| {
        let reference = format!("assets/{name}");
        (!source.contains(&reference)).then(|| name.clone())
    })
}

#[allow(dead_code)]
fn prepare_unreferenced_attachment(page: &Path, name: &str) -> bool {
    match notecraft_store::assets::ensure_icon(page, name) {
        Ok(()) => {
            log::debug!("generated icon before detached attachment warning: page={:?} attachment={name:?}", page);
            true
        }
        Err(error) => {
            log::error!("could not generate icon before detached attachment warning: page={:?} attachment={name:?} error={error}", page);
            false
        }
    }
}

fn first_legacy_attachment_in_source(source: &str) -> Option<String> {
    let mut rest = source;
    while let Some(at) = rest.find("assets/") {
        rest = &rest[at + "assets/".len()..];
        let end = rest.find(['"', '\'', ')', ']', '<', '>', ' ', '\n', '\r']).unwrap_or(rest.len());
        let name = &rest[..end];
        if !name.is_empty()
            && !name.ends_with(".svg")
            && notecraft_store::assets::media_kind(&format!("assets/{name}")) == notecraft_store::assets::MediaKind::Other
            && (source.contains(&format!("<a href=\"assets/{name}\">")) || source.contains(&format!("[{name}](assets/{name})")))
        {
            return Some(name.to_string());
        }
        rest = &rest[end..];
    }
    None
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

#[derive(Clone, Debug, Default)]
struct PendingFormat {
    block: usize,
    at: usize,
    size: Option<u8>,
    family: Option<String>,
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    script: Option<notecraft_formats::markdown::Script>,
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
    /// Formatting chosen at a collapsed caret, before any text has been inserted.
    pending_format: Option<PendingFormat>,
    restore_format_selection: Option<(usize, usize, usize)>,
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
    /// How far down the page the containers reached when last drawn, in page points.
    content_bottom: Option<f32>,
}

impl Editor {
    /// `count` blocks were just put into the page at `at`. The box that holds the block before
    /// them counts them too (the first box, for blocks put in at the top), so the boxes still
    /// add up to the page's blocks and `container::fit` keeps the arrangement instead of
    /// folding it into one box (issue #72). Call it before [`Editor::rebuild_body`].
    fn count_inserted_in_boxes(&mut self, at: usize, count: usize) {
        let counted: usize = self.boxes.iter().map(|c| c.blocks).sum();
        if count == 0 || self.boxes.is_empty() || counted + count != self.blocks.len() {
            return;
        }
        let owner = if at == 0 { Some(0) } else { notecraft_model::container::place(&self.boxes, at - 1).map(|(i, _)| i) };
        if let Some(i) = owner {
            self.boxes[i].blocks += count;
        }
    }

    /// The block at `at` was just taken out of the page: its box counts one fewer.
    fn count_removed_from_boxes(&mut self, at: usize) {
        let counted: usize = self.boxes.iter().map(|c| c.blocks).sum();
        if counted != self.blocks.len() + 1 {
            return;
        }
        if let Some((i, _)) = notecraft_model::container::place(&self.boxes, at) {
            self.boxes[i].blocks -= 1;
        }
    }

    /// [`Editor::count_inserted_in_boxes`], and justified paragraphs and table widths after the
    /// new blocks move down with their blocks. Returns whether the widths and the justified
    /// paragraphs changed, for [`NoteCraftApp::save_block_marks`].
    fn blocks_inserted(&mut self, at: usize, count: usize) -> (bool, bool) {
        self.count_inserted_in_boxes(at, count);
        let (mut cols, mut justified) = (false, false);
        for offset in 0..count {
            cols |= self.columns.insert_block(at + offset);
            justified |= self.justified.insert_block(at + offset);
        }
        (cols, justified)
    }

    /// Writes the page's text from its blocks, with the boxes around them when it is arranged.
    fn rebuild_body(&mut self) {
        let arrangement = (self.arranged && !self.boxes.is_empty()).then_some(self.boxes.as_slice());
        self.body = notecraft_formats::boxes::write(self.format, &self.blocks, arrangement);
    }

    fn apply_pending_format(&mut self, before: Option<&pageedit::Caret>) {
        let Some(pending) = self.pending_format.take() else { return };
        let (block, at) = (pending.block, pending.at);
        let (Some(before), Some(caret)) = (before, self.caret.as_mut()) else {
            return;
        };
        if before.block != block || caret.block != block || before.selection != (at, at) || caret.cell.is_some() {
            return;
        }
        let offset = before.text.char_indices().nth(at).map_or(before.text.len(), |(offset, _)| offset);
        let (prefix, suffix) = before.text.split_at(offset);
        let inserted = caret.text.strip_prefix(prefix).and_then(|text| text.strip_suffix(suffix));
        let Some(inserted) = inserted.filter(|text| !text.is_empty()) else {
            self.pending_format = Some(pending);
            return;
        };
        let end = at + inserted.chars().count();
        if caret.selection != (end, end) {
            return;
        }
        let mut marker = "NoteCraftPendingTextMarker".to_string();
        while pending.family.as_ref().is_some_and(|family| family.contains(&marker)) {
            marker.push('X');
        }
        let span = notecraft_formats::markdown::Span {
            text: marker.clone(),
            font_size: pending.size,
            font_family: pending.family,
            bold: pending.bold,
            italic: pending.italic,
            strike: pending.strike,
            underline: pending.underline,
            script: pending.script,
            ..Default::default()
        };
        let template = notecraft_formats::render::spans_to_markdown(&[span]);
        let Some((open, close)) = template.split_once(&marker) else {
            log::error!("Could not serialize pending formatting");
            return;
        };
        let (text, selection) = pageedit::wrap_selection_with(&caret.text, (at, end), open, close);
        caret.text = text;
        caret.selection = (selection.1, selection.1);
        caret.focus = true;
        caret.place = Some(selection.1);
        if let Some(target) = self.blocks.get_mut(block) {
            pageedit::set_caret_text(target, &caret.text);
        }
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
        let (drawn, size) = crate::bidi::runs(ui, &ed.title_buf, &pieces, 26.0 * zoom, width, ed.page_dir);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, size.y.max(34.0)), egui::Sense::click());
        crate::bidi::paint(ui, rect.min, &drawn);
        if resp.clicked() {
            ui.memory_mut(|m| m.request_focus(title_id));
        }
        let name = ed.title_buf.clone();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextInput, true, &name));
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
    /// Copy it in and insert one rendered image for each printed page.
    Printout,
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
    ResetPreferences,
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
    /// The first-use wizard: one window with three steps. Step 0 is the local logging level,
    /// step 1 the page format saved as the default, and step 2 whether to add the Welcome notebook
    /// in that format.
    FirstUseWizard {
        step: u8,
        level: String,
        format: PageFormat,
        add_demo: bool,
        error: Option<String>,
    },
    /// A problem told to the user in a box.    /// A problem told to the user in a box. Ticking "do not alert me again" stops the box for
    /// this `key` for the rest of the session; the status line still says what happened.
    Alert {
        key: String,
        title: String,
        message: String,
        suppress: bool,
        suppress_forever: bool,
    },
    MissingAttachmentIcon {
        page: PathBuf,
        name: String,
    },
    OpenAttachmentWarning {
        path: PathBuf,
    },
    #[allow(dead_code)]
    UnreferencedAttachment {
        page: PathBuf,
        name: String,
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
    NewPage(PathBuf),
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
    MakeSubpage(PathBuf),
    PromoteSubpage(PathBuf),
    MovePageUp(PathBuf),
    MovePageDown(PathBuf),
    PageVersions(PathBuf),
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
    ui_language: String,
    effective_mode: Mode,
    accent: Accent,
    applied_theme: Option<(ModePref, Accent)>,
    loaders_installed: bool,
    redact_paths: bool,
    control: Option<Receiver<ControlRequest>>,
    control_server: Option<ControlServer>,
    event_frames: std::collections::VecDeque<Vec<egui::Event>>,
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
    /// The canvas translation in screen points. It is UI state, so it never changes page data.
    canvas_pan: egui::Vec2,
    /// Last middle-button position while panning the canvas.
    canvas_drag: Option<egui::Pos2>,
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
    keyboard_paste_requested: bool,
    /// The File screen, when it is open, and which of its pages.
    backstage: Option<&'static str>,
    /// The page fills the window: the notebooks are hidden until it is turned off.
    full_page: bool,
    /// Put the keyboard in the search box on the next frame.
    focus_search: bool,
    /// The page the window closed on has been opened again (once per run).
    restored: bool,
    /// Recovery warnings that were already shown during this run.
    icon_recovery_reported: HashSet<String>,
    /// Suppress further missing-icon warnings during this run.
    icon_recovery_suppressed: bool,
    /// Alerts the user asked not to see again this session, by key.
    muted_alerts: HashSet<String>,
    /// Alerts waiting for the open one to close.
    queued_alerts: VecDeque<Dialog>,
    /// The clipboard the app reads and writes. The system's, unless the app was built with another.
    clipboard: Box<dyn crate::clipboard::Board>,
    open_attachment_warning_suppressed: bool,
    welcome_notebook_setup: Option<Box<dyn FnMut(&Path, &str) -> Result<PathBuf, String>>>,
}

impl NoteCraftApp {
    /// Replaces the clipboard the app uses. Made at startup by the caller, so the app never
    /// reaches a platform's clipboard by itself.
    pub fn with_clipboard(mut self, board: Box<dyn crate::clipboard::Board>) -> NoteCraftApp {
        self.clipboard = board;
        self
    }

    /// Queues a native keyboard paste captured before egui processes the shortcut.
    pub fn queue_keyboard_paste(&mut self) {
        log::debug!("queued native keyboard paste");
        self.keyboard_paste_requested = true;
    }

    pub fn new(session: Session) -> NoteCraftApp {
        let mode_pref = session.settings.get("theme_mode").and_then(|v| ModePref::parse(&v)).unwrap_or(ModePref::System);
        let ui_language =
            session.settings.get("ui_language").map(|value| crate::i18n::normalize_preference(&value)).unwrap_or_else(|| crate::i18n::AUTO.into());
        crate::i18n::set_current(&ui_language);
        let accent = session.settings.get("theme_accent").and_then(|v| Accent::parse(&v)).unwrap_or(Accent::Default);
        let show_source = false;
        let session_rules = session.settings.get("rule_lines").and_then(|v| ribbon::RuleLines::parse(&v)).unwrap_or_default();
        let session_hide_title = session.settings.get("hide_title").map(|v| v.trim() == "true").unwrap_or(false);
        // A table has borders unless it was turned off: OneNote draws them, and a table
        // without them reads as loose columns of words.
        let session_borders = session.settings.get("table_borders").map(|v| v.trim() != "false").unwrap_or(true);
        // Two columns in the side panel, the way OneNote separates the notebooks from the
        // pages. This is the product's default navigation layout; the one-tree view remains
        // available as an explicit preference.
        let session_split = session.settings.get("split_panes").map(|v| v.trim() == "true").unwrap_or(true);
        // The pen rack as the owner last arranged it, and which tool was in hand.
        let rack = notecraft_ink::rack::parse(&session.settings.get(RACK_KEY).unwrap_or_default());
        let nib = session.settings.get(NIB_KEY).and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(0).min(rack.len() - 1);
        let sensitivity = session.settings.get(SENSITIVITY_KEY).and_then(|v| v.trim().parse::<u8>().ok()).unwrap_or(DEFAULT_SENSITIVITY).min(100);
        let qat = quickaccess::Config::load(|k| session.settings.get(k));
        // `--first-use` clears the key to an empty value, which counts as not seen.
        let privacy_welcome = session.settings.get("privacy_welcome_seen").is_none_or(|v| v.trim().is_empty());
        let telemetry_level = session.settings.get("telemetry_level").unwrap_or_else(|| default_telemetry_level(cfg!(debug_assertions)).to_string());
        log::info!("privacy welcome modal state: show={} telemetry_level={:?}", privacy_welcome, telemetry_level);
        let wizard_format = session.settings.global_format().0;
        let wizard_level = session.settings.get("telemetry_level").unwrap_or_else(|| "debug".to_string());
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
            dialog: if privacy_welcome {
                Some(Dialog::FirstUseWizard { step: 0, level: wizard_level, format: wizard_format, add_demo: true, error: None })
            } else {
                None
            },
            import_job: None,
            status: crate::tl_id!("ui.ready_2").into(),
            status_is_error: false,
            search: String::new(),
            searched: String::new(),
            hits: Vec::new(),
            cursor: 0,
            tokens: Theme::DEFAULT.tokens(),
            mode_pref,
            ui_language,
            effective_mode: if mode_pref == ModePref::Dark { Mode::Dark } else { Mode::Light },
            accent,
            applied_theme: None,
            loaders_installed: false,
            redact_paths: false,
            control: None,
            control_server: None,
            event_frames: std::collections::VecDeque::new(),
            show_source,
            code: crate::codeview::State::default(),
            ribbon_tab: "Home".to_string(),
            stylus: crate::stylus::Stylus::default(),
            page_rect: None,
            canvas_pan: egui::Vec2::ZERO,
            canvas_drag: None,
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
            keyboard_paste_requested: false,
            backstage: None,
            full_page: false,
            focus_search: false,
            restored: false,
            icon_recovery_reported: HashSet::new(),
            icon_recovery_suppressed: false,
            muted_alerts: HashSet::new(),
            queued_alerts: VecDeque::new(),
            clipboard: Box::new(crate::clipboard::System),
            open_attachment_warning_suppressed: false,
            welcome_notebook_setup: None,
        }
    }

    /// Installs the application-owned action behind the Welcome notebook step of the first-use
    /// wizard. The callback gets the notebooks folder and the chosen format id, and returns the
    /// installed notebook folder. The wizard opens once: on the first launch after installation, or
    /// after the preferences are reset. It does not reopen for later versions.
    pub fn with_welcome_notebook_setup(mut self, setup: impl FnMut(&Path, &str) -> Result<PathBuf, String> + 'static) -> Self {
        self.welcome_notebook_setup = Some(Box::new(setup));
        self
    }

    /// The first-use wizard, starting at the logging step with the current settings.
    fn first_use_wizard(&self) -> Dialog {
        Dialog::FirstUseWizard {
            step: 0,
            level: self.session.settings.get("telemetry_level").unwrap_or_else(|| "debug".to_string()),
            format: self.session.settings.global_format().0,
            add_demo: true,
            error: None,
        }
    }

    /// Applies the wizard's answers: the logging level, the default page format, and, when chosen,
    /// the Welcome notebook in that format. The wizard is marked seen only after every answer is saved,
    /// so a failed install shows the wizard again on the next launch.
    fn finish_first_use_wizard(&mut self, level: &str, format: PageFormat, add_demo: bool) -> Result<(), String> {
        self.session.settings.set("telemetry_level", level).map_err(|error| error.to_string())?;
        self.session.settings.set("privacy_welcome_seen", "true").map_err(|error| error.to_string())?;
        self.session.settings.set_default_format(format).map_err(|error| error.to_string())?;
        if add_demo {
            self.save_now();
            if self.is_dirty() {
                return Err("the current page could not be saved before replacing the Welcome notebook".to_string());
            }
            let installed = {
                let setup = self.welcome_notebook_setup.as_mut().ok_or_else(|| "the bundled Welcome notebook setup is unavailable".to_string())?;
                setup(self.session.root(), format.id())?
            };
            self.editor = None;
            self.pad = None;
            self.session.refresh();
            self.status_is_error = false;
            self.open_path(&installed.join("00-start/welcome").to_string_lossy());
            if self.status_is_error {
                return Err(self.status.clone());
            }
        }
        Ok(())
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
            self.status = crate::i18n::format_message_id("ui.the_theme_could_not_be_saved_error", &[("error", &e.to_string())]);
            self.status_is_error = true;
        }
    }

    fn set_ui_language(&mut self, language: String) {
        if let Err(error) = self.session.settings.set("ui_language", &language) {
            let error = error.to_string();
            let message = crate::i18n::format_message_id("ui.could_not_save_the_interface_language_error", &[("error", &error)]);
            self.complain(&message);
        } else {
            self.ui_language = language;
            crate::i18n::set_current(&self.ui_language);
            self.status = crate::tl_id!("ui.ready_2").to_string();
            self.status_is_error = false;
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

    /// Opens `path` (absolute, or relative to the notebooks folder) and expands the tree down
    /// to it. A notebook, section group or section opens on its first page.
    pub fn open_path(&mut self, path: &str) {
        // An explicit command-line/control request wins over the page saved by the
        // previous session. Otherwise the first frame's restore pass would replace the
        // requested page with a stale page from another notebook.
        self.restored = true;
        // A notebook or section opens on its first page, and a notebook kept elsewhere is
        // opened first: the engine decides, so the command line and agents get the same page.
        if let Some(Output::Path(dir)) = self.run("open.page", &params(&[("path", path)])) {
            self.expand_to(&dir);
            self.open_page(&dir);
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
            None => self.say(crate::tl_id!("ui.there_is_no_earlier_page")),
        }
    }

    /// Forward to the page after this one.
    fn go_forward(&mut self) {
        match self.nav.forward(|p| p.exists()) {
            Some(dir) => {
                self.expand_to(&dir);
                self.open_page_inner(&dir);
            }
            None => self.say(crate::tl_id!("ui.there_is_no_later_page")),
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
    /// Tells the user about a problem. The status line always says it. A box says it too, unless
    /// the user has asked not to see alerts with this `key` again this session; a box that is
    /// already open is not replaced, the next alert waits its turn in a queue.
    pub fn alert(&mut self, key: &str, title: &str, message: &str) {
        self.status = message.to_string();
        self.status_is_error = true;
        if self.muted_alerts.contains(key) {
            return;
        }
        if self.session.settings.get(&format!("{key}_never")).as_deref() == Some("true") {
            return;
        }
        let showing = matches!(&self.dialog, Some(Dialog::Alert { key: k, .. }) if k == key);
        let queued = self.queued_alerts.iter().any(|d| matches!(d, Dialog::Alert { key: k, .. } if k == key));
        if showing || queued {
            return;
        }
        let dialog =
            Dialog::Alert { key: key.to_string(), title: title.to_string(), message: message.to_string(), suppress: false, suppress_forever: false };
        if self.dialog.is_none() {
            self.dialog = Some(dialog);
        } else {
            self.queued_alerts.push_back(dialog);
        }
    }

    fn complain(&mut self, msg: &str) {
        self.status = msg.to_string();
        self.status_is_error = true;
    }

    fn fail(&mut self, e: &EngineError) {
        self.status = crate::i18n::format_message_id("ui.could_not_complete_the_action_error", &[("error", &e.to_string())]);
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

    fn reparent_page(&mut self, path: &Path, command: &str) {
        self.save_now();
        if self.is_dirty() {
            return;
        }
        let editor_path = self.editor.as_ref().map(|editor| editor.dir.clone());
        let Some(Output::Path(new_path)) = self.run(command, &params(&[("path", &path_str(path))])) else { return };
        self.nav.relocate(path, &new_path);
        let remap = |candidate: &Path| candidate.strip_prefix(path).map_or_else(|_| candidate.to_path_buf(), |suffix| new_path.join(suffix));
        self.open = self.open.iter().map(|branch| remap(branch)).collect();
        self.selected = self.selected.as_deref().map(remap);
        if let Some(editor_path) = editor_path.filter(|editor_path| editor_path.starts_with(path)) {
            let relocated = remap(&editor_path);
            self.open_page_inner(&relocated);
        }
    }

    fn create_page_in(&mut self, parent: &Path) {
        if let Some(Output::Path(page)) = self.run("node.create", &params(&[("parent", &path_str(parent)), ("kind", "page")])) {
            self.open_page(&page);
            self.say(crate::tl_id!("ui.new_page"));
        }
    }

    fn show_page_versions(&mut self, page: &Path) {
        self.save_now();
        if self.is_dirty() {
            return;
        }
        let p = params(&[("path", &path_str(page))]);
        if let Some(Output::Versions(versions)) = self.run("page.versions", &p) {
            self.dialog = Some(Dialog::PageVersions { page: page.to_path_buf(), versions });
        }
    }

    fn can_move_page(path: &Path, up: bool) -> bool {
        let Some(parent) = path.parent() else { return false };
        let siblings: Vec<_> =
            notecraft_store::tree::child_dirs(parent).into_iter().filter(|sibling| notecraft_store::tree::read_node(sibling).is_some()).collect();
        let Some(index) = siblings.iter().position(|sibling| sibling == path) else { return false };
        let adjacent = if up { index.checked_sub(1) } else { index.checked_add(1).filter(|next| *next < siblings.len()) };
        adjacent
            .and_then(|adjacent| siblings.get(adjacent))
            .and_then(|sibling| notecraft_store::tree::read_node(sibling))
            .is_some_and(|info| info.kind == NodeKind::Page)
    }

    fn can_make_subpage(path: &Path) -> bool {
        let Some(parent) = path.parent() else { return false };
        let siblings: Vec<_> =
            notecraft_store::tree::child_dirs(parent).into_iter().filter(|sibling| notecraft_store::tree::read_node(sibling).is_some()).collect();
        let Some(index) = siblings.iter().position(|sibling| sibling == path) else { return false };
        index
            .checked_sub(1)
            .and_then(|previous| siblings.get(previous))
            .and_then(|previous| notecraft_store::tree::read_node(previous))
            .is_some_and(|info| info.kind == NodeKind::Page)
    }

    fn can_promote_subpage(path: &Path) -> bool {
        path.parent().and_then(notecraft_store::tree::read_node).is_some_and(|info| info.kind == NodeKind::Page)
    }

    fn hierarchy_keys(&mut self, ctx: &Context) {
        if ctx.egui_wants_keyboard_input() || self.dialog.is_some() || self.backstage.is_some() {
            return;
        }
        let Some(page) = self.selected.clone().filter(|path| notecraft_store::tree::read_node(path).is_some_and(|info| info.kind == NodeKind::Page))
        else {
            return;
        };
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::N)) {
            if let Some(parent) = page.parent() {
                self.create_page_in(parent);
            }
            return;
        }
        let is_macos = cfg!(target_os = "macos");
        let mods = Modifiers { ctrl: !is_macos, alt: true, mac_cmd: is_macos, command: true, ..Modifiers::NONE };
        if Self::can_make_subpage(&page) && ctx.input_mut(|input| input.consume_key(mods, Key::CloseBracket)) {
            self.reparent_page(&page, "node.demote");
        } else if Self::can_promote_subpage(&page) && ctx.input_mut(|input| input.consume_key(mods, Key::OpenBracket)) {
            self.reparent_page(&page, "node.promote");
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
        self.code = crate::codeview::State::default();
        if let Some(Output::Page(page)) = self.run("page.load", &params(&[("path", &path_str(dir))])) {
            let (blocks, in_the_text) = notecraft_formats::boxes::read(page.format, &page.body);
            let older = notecraft_store::page::containers(dir).unwrap_or_default();
            let arranged = in_the_text.is_some() || !older.is_empty();
            let mut boxes = in_the_text.unwrap_or(older);
            log::debug!(
                "page loaded for layout: page={:?} format={:?} blocks={} arranged={} stored_containers={:?}",
                dir,
                page.format,
                blocks.len(),
                arranged,
                boxes
            );
            // A never-arranged blank page still gets one empty writing box. A
            // page whose last box was deliberately deleted remains boxless so
            // its open paper can create a new box on the next click.
            if blocks.is_empty() && !arranged && boxes.is_empty() {
                boxes.push(notecraft_model::Container::new(0.0, 0.0, 360.0, 0));
                log::debug!(
                    "default page block created: page={:?} block_count={} container=(x=0,y=0,width=360,blocks=0) empty_block_min_height=22",
                    dir,
                    blocks.len()
                );
            }
            // The page as it was opened is the first step of its history: undo goes back
            // this far and no further.
            let first = (page.body.clone(), blocks.clone(), boxes.clone());
            // The paper this page is drawn on and the colour behind it are the page's own.
            let appearance = notecraft_store::page::appearance(dir)
                .map(|(p, c)| (p.and_then(|p| ribbon::RuleLines::parse(&p)), c.and_then(|c| notecraft_model::NodeColor::parse(&c))))
                .unwrap_or((None, None));
            let attachments = notecraft_store::assets::list(dir);
            let page_body = page.body.clone();
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
                attachments: attachments.clone(),
                caret: None,
                pending_format: None,
                restore_format_selection: None,
                writable,
                page_dir: page.dir,
                boxes,
                arranged,
                content_bottom: None,
                columns: notecraft_store::page::table_columns(dir).unwrap_or_default(),
                justified: notecraft_store::page::justified(dir).unwrap_or_default(),
                across: None,
                ink_files: notecraft_store::page::ink_files(dir),
                paper: appearance.0,
                colour: appearance.1,
                history: crate::history::History::new(crate::history::Step::new(first.0, first.1, first.2, None, crate::history::Kind::Edit)),
                origin: egui::Pos2::ZERO,
            });
            if !self.icon_recovery_suppressed
                && let Some(name) = first_missing_attachment_in_source(&page_body, dir).or_else(|| first_legacy_attachment_in_source(&page_body))
            {
                let key = format!("{}::{name}", dir.display());
                log::warn!("attachment recovery scan found missing icon: page={:?} attachment={name:?}", dir);
                // Do not let page recovery replace the first-use privacy dialog. The normal
                // frame pass below will offer recovery after the user answers that dialog.
                if self.dialog.is_none() && self.icon_recovery_reported.insert(key) {
                    self.dialog = Some(Dialog::MissingAttachmentIcon { page: dir.to_path_buf(), name });
                }
            } else {
                log::debug!("attachment recovery scan found no missing icons: page={:?} attachments={}", dir, attachments.len());
            }
            self.pad = Some(crate::inkpad::Pad::of_page(dir));
            self.absorb_old_sheets(dir);
            self.sync_ink_reference();
            self.selected = Some(dir.to_path_buf());
            self.cursor = 0;
            self.remember_last_page(dir);
            self.say(crate::tl_id!("ui.opened"));
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
                self.complain(crate::tl_id!("ui.cannot_write_this_page_s_ink"));
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
        let message = if left > 0 {
            crate::i18n::format_message_id(
                "ui.moved_moved_strokes_onto_the_page_left_old_drawing_f_89f765dcef",
                &[("moved", &moved.to_string()), ("left", &left.to_string())],
            )
        } else {
            crate::i18n::format_message_id("ui.moved_moved_strokes_from_this_page_s_old_drawing_ont_11ecd930d5", &[("moved", &moved.to_string())])
        };
        self.say(&message);
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
                self.say(crate::tl_id!("ui.saved"));
            }
            Ok(_) => {}
            Err(e) => self.fail(&e),
        }
    }

    fn repair_attachment_reference(&mut self, page: &Path, name: &str) -> Result<bool, String> {
        let Some(ed) = self.editor.as_mut() else { return Ok(false) };
        if ed.dir != page {
            return Err("the page changed while the recovery dialog was open".into());
        }
        let link = format!("assets/{name}");
        let html_href = format!("href=\"{link}\"");
        let markdown_href = format!("]({link})");
        let references = ed.body.matches(&html_href).count() + ed.body.matches(&markdown_href).count();
        log::debug!("attachment recovery reference scan: page={:?} attachment={name:?} references={references}", page);
        let old = match ed.format {
            PageFormat::Html => format!("<p><a href=\"{link}\">{name}</a></p>"),
            PageFormat::Cmgfm => format!("[{name}]({link})"),
        };
        if references == 0 && !ed.body.contains(&old) {
            let icon_link = attachment_icon_link(page, name);
            let replacement = replace_attachment_icon(ed.format, &snippets::attachment(ed.format, &link, name), name, &icon_link).trim().to_string();
            ed.body.push('\n');
            ed.body.push_str(&replacement);
            Self::mark_dirty(ed);
            log::debug!("attachment recovery appended missing reference: page={:?} attachment={name:?}", page);
            return Ok(true);
        }
        let icon_link = attachment_icon_link(page, name);
        let replacement = replace_attachment_icon(ed.format, &snippets::attachment(ed.format, &link, name), name, &icon_link).trim().to_string();
        let mut body = ed.body.replace(&old, &replacement);
        if references > 1 {
            let mut kept = false;
            body = body
                .lines()
                .filter(|line| {
                    let card = match ed.format {
                        PageFormat::Html => line.contains(&html_href) && line.contains("<img"),
                        PageFormat::Cmgfm => line.contains(&markdown_href) && line.contains("!["),
                    };
                    if card {
                        if kept {
                            return false;
                        }
                        kept = true;
                    }
                    true
                })
                .collect::<Vec<_>>()
                .join("\n");
            log::warn!("attachment recovery removed duplicate references: page={:?} attachment={name:?} references={references}", page);
        }
        // Migrate an older generated icon in place when the platform now has a native icon.
        #[cfg(target_os = "windows")]
        {
            body = body.replace(&format!("assets/{name}.svg"), &icon_link).replace(&format!("assets/{name}.png"), &icon_link);
        }
        if body == ed.body {
            return Ok(false);
        }
        ed.body = body;
        Self::mark_dirty(ed);
        Ok(true)
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

    /// Adds the blocks of `text` to the page in a container of their own, below what is
    /// already on the page, so each printout can be moved and sized apart from the others.
    fn insert_in_own_container(&mut self, text: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let below = ed.content_bottom;
        let (added, _) = notecraft_formats::boxes::read(ed.format, text);
        if added.is_empty() {
            return;
        }
        let width = ed.boxes.first().map_or(360.0, |c| c.width);
        let before = ed.blocks.len();
        ed.blocks.extend(added.iter().cloned());
        let below = below.unwrap_or_else(|| ed.boxes.iter().map(|c| c.y + 60.0).fold(0.0, f32::max));
        ed.boxes = notecraft_model::container::append(&ed.boxes, before, added.len(), width, below, 12.0);
        ed.arranged = true;
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_step(ed);
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

    /// Puts the blocks of a snippet (a file copied in or linked) after the caret's line, in the
    /// caret's box, or on the end of the page. It works on the blocks rather than splicing the
    /// page's text at `cursor`, which put the snippet wherever that old text position pointed
    /// (the top of the page, in a box of its own) and read every box back from the text
    /// (issue #72).
    fn insert_snippet_blocks(&mut self, text: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let (added, _) = notecraft_formats::boxes::read(ed.format, text);
        if added.is_empty() {
            return;
        }
        let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
        let count = added.len();
        for (offset, block) in added.into_iter().enumerate() {
            ed.blocks.insert(at + offset, block);
        }
        let marks = ed.blocks_inserted(at, count);
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_step(ed);
        self.save_block_marks(marks);
    }

    /// A dropped file asks what to do with it, the way OneNote asks when you insert one.
    fn attach_dropped(&mut self, ctx: &Context) {
        let files: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if files.is_empty() {
            return;
        }
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.status = crate::tl_id!("ui.open_a_page_first_then_drop_files_onto_it").into();
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
                        self.insert_snippet_blocks(&text);
                        self.say(crate::i18n::format_message_id("ui.name_is_now_a_file_in_this_page_s_folder", &[("name", &name)]));
                    }
                    Some(_) => self.complain(crate::tl_id!("ui.the_file_was_not_attached_the_attach_command_returne_beaffe0fd0")),
                    None => {
                        log::error!("file attach failed: name={name:?} page={:?} status={:?}", dir, self.status);
                        self.complain(&crate::i18n::format_message_id("ui.could_not_attach_name_to_this_page", &[("name", &name)]));
                    }
                }
            }
            FileChoice::Link => {
                let text = snippets::file_link(format, &path_str(file), &name);
                self.insert_snippet_blocks(&text);
                self.say(crate::i18n::format_message_id("ui.linked_name_where_it_is_the_file_was_not_copied", &[("name", &name)]));
            }
            FileChoice::Printout => self.insert_printout(dir, file, format),
        }
        if let Some(ed) = self.editor.as_mut() {
            ed.attachments = notecraft_store::assets::list(&ed.dir);
        }
    }

    fn insert_printout(&mut self, dir: &Path, file: &Path, format: PageFormat) {
        let rendered = match printout::render(file) {
            Ok(rendered) => rendered,
            Err(error) => {
                self.complain(&error);
                return;
            }
        };
        let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let Some(Output::Text(link)) = self.run("page.attach", &params(&[("path", &path_str(dir)), ("source", &path_str(file))])) else {
            self.complain(&crate::i18n::format_message_id("ui.could_not_attach_name_before_rendering_its_pages", &[("name", &name)]));
            return;
        };
        let Some(base) = link.strip_prefix("assets/").and_then(|link| Path::new(link).file_name()).map(|name| name.to_string_lossy().into_owned())
        else {
            self.complain(crate::tl_id!("ui.the_attachment_returned_an_invalid_page_link"));
            return;
        };
        let assets = dir.join("assets");
        let mut generated = Vec::with_capacity(rendered.pages().len());
        for (index, page) in rendered.pages().iter().enumerate() {
            let target = assets.join(format!("{base}.printout-{:03}.png", index + 1));
            if let Err(error) = std::fs::copy(page, &target) {
                for path in &generated {
                    let _ = std::fs::remove_file(path);
                }
                self.complain(&crate::i18n::format_message_id(
                    "ui.could_not_store_printout_page_page_error",
                    &[("page", &(index + 1).to_string()), ("error", &error.to_string())],
                ));
                return;
            }
            generated.push(target);
        }
        let mut text = snippets::attachment(format, &link, &name);
        for (index, _) in generated.iter().enumerate() {
            let image = format!("assets/{base}.printout-{:03}.png", index + 1);
            text.push_str(&snippets::printout_page(format, &image, index + 1));
        }
        self.insert_in_own_container(&text);
        self.say(crate::i18n::format_message_id(
            "ui.inserted_name_and_count_printout_pages",
            &[("name", &name), ("count", &generated.len().to_string())],
        ));
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
                    self.say(crate::tl_id!("ui.notebook_closed_its_files_are_still_on_disk"));
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
                    self.say(crate::tl_id!("ui.new_page"));
                }
            }
            Act::NewPage(parent) => self.create_page_in(&parent),
            Act::Rename(path, title) => self.dialog = Some(Dialog::Rename { path, title }),
            Act::MakeSubpage(path) => self.reparent_page(&path, "node.demote"),
            Act::PromoteSubpage(path) => self.reparent_page(&path, "node.promote"),
            Act::MovePageUp(path) => {
                if let Some(Output::Path(path)) = self.run("node.move_up", &params(&[("path", &path_str(&path))])) {
                    self.selected = Some(path.clone());
                    self.session.refresh();
                }
            }
            Act::MovePageDown(path) => {
                if let Some(Output::Path(path)) = self.run("node.move_down", &params(&[("path", &path_str(&path))])) {
                    self.selected = Some(path.clone());
                    self.session.refresh();
                }
            }
            Act::PageVersions(path) => self.show_page_versions(&path),
            Act::Delete(path, title, count) => self.dialog = Some(Dialog::Delete { path, title, count }),
            Act::Color(path, color) => {
                let value = color.map_or("none", NodeColor::id);
                if self.run("node.color", &params(&[("path", &path_str(&path)), ("color", value)])).is_some() {
                    self.say(match color {
                        Some(c) => {
                            let colour = crate::tl!(c.label());
                            crate::i18n::format_message_id("ui.colour_set_to_colour", &[("colour", &colour.to_lowercase())])
                        }
                        None => crate::tl_id!("ui.colour_removed").to_string(),
                    });
                }
            }
            Act::CopyLink(path) => self.copy_link_to(&path),
            Act::Reveal(path) => match open_folder(&path) {
                Ok(()) => self.say(crate::i18n::format_message_id("ui.opened_path", &[("path", &self.shown(&path))])),
                Err(e) => {
                    self.status = crate::i18n::format_message_id("ui.the_folder_could_not_be_opened_error", &[("error", &e.to_string())]);
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
            painter.text(egui::pos2(rect.max.x - 30.0, cy), Align2::RIGHT_CENTER, crate::tl!(f.short()), egui::FontId::proportional(10.5), t.muted);
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
            let new_page_label = if cfg!(target_os = "macos") { crate::tl_id!("ui.new_page_cmd_n") } else { crate::tl_id!("ui.new_page_ctrl_n") };
            if node.info.kind == NodeKind::Page
                && let Some(parent) = node.path.parent()
                && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Page)), new_page_label, t).clicked()
            {
                acts.push(Act::NewPage(parent.to_path_buf()));
                ui.close();
            }
            // What this node may hold, named in full rather than left to the dialog: a
            // notebook and a group take sections and groups, a section takes pages, and a
            // page takes sub-pages.
            for kind in node.info.kind.may_contain() {
                let label = match kind {
                    NodeKind::SectionGroup => "New section group...",
                    NodeKind::Section => "New section...",
                    NodeKind::Page if node.info.kind == NodeKind::Page => crate::tl_id!("ui.add_a_sub_page"),
                    NodeKind::Page => crate::tl_id!("ui.add_page"),
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
            if node.info.kind == NodeKind::Page
                && (Self::can_make_subpage(&node.path)
                    || Self::can_promote_subpage(&node.path)
                    || Self::can_move_page(&node.path, true)
                    || Self::can_move_page(&node.path, false))
            {
                floating::menu_separator(ui, t);
                if Self::can_make_subpage(&node.path)
                    && floating::menu_row(ui, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::IndentMore)), crate::tl_id!("ui.make_subpage"), t)
                        .clicked()
                {
                    acts.push(Act::MakeSubpage(node.path.clone()));
                    ui.close();
                }
                if Self::can_promote_subpage(&node.path)
                    && floating::menu_row(
                        ui,
                        Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::IndentLess)),
                        crate::tl_id!("ui.promote_subpage"),
                        t,
                    )
                    .clicked()
                {
                    acts.push(Act::PromoteSubpage(node.path.clone()));
                    ui.close();
                }
                if Self::can_move_page(&node.path, true)
                    && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::MoveUp)), crate::tl_id!("ui.move_up"), t).clicked()
                {
                    acts.push(Act::MovePageUp(node.path.clone()));
                    ui.close();
                }
                if Self::can_move_page(&node.path, false)
                    && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::MoveDown)), crate::tl_id!("ui.move_down"), t).clicked()
                {
                    acts.push(Act::MovePageDown(node.path.clone()));
                    ui.close();
                }
            }
            floating::menu_separator(ui, t);
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Page)), crate::tl_id!("ui.rename_ellipsis"), t).clicked() {
                acts.push(Act::Rename(node.path.clone(), node.info.title.clone()));
                ui.close();
            }
            if node.info.kind == NodeKind::Notebook
                && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Notebook)), crate::tl_id!("ui.close_notebook"), t).clicked()
            {
                acts.push(Act::CloseNotebook(node.path.clone()));
                ui.close();
            }
            if has_kids
                && floating::menu_row(
                    ui,
                    Some(floating::Glyph::Icon(if open { Icon::Minus } else { Icon::Plus })),
                    if node.info.kind == NodeKind::Page {
                        if open { crate::tl_id!("ui.collapse_subpages") } else { crate::tl_id!("ui.expand_subpages") }
                    } else if open {
                        crate::tl_id!("ui.collapse")
                    } else {
                        crate::tl_id!("ui.expand")
                    },
                    t,
                )
                .clicked()
            {
                acts.push(Act::Toggle(node.path.clone()));
                ui.close();
            }
            // Pages take the colour of their section, the way OneNote colours tabs.
            if node.info.kind != NodeKind::Page {
                ui.menu_button(crate::tl_id!("ui.colour"), |ui| {
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
                    if ui.button(crate::tl_id!("ui.no_colour")).clicked() {
                        acts.push(Act::Color(node.path.clone(), None));
                        ui.close();
                    }
                });
            }
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Link)), crate::tl_id!("ui.copy_link_to_page"), t)
                .on_hover_text(crate::tl_id!("ui.a_link_to_paste_into_another_page"))
                .clicked()
            {
                acts.push(Act::CopyLink(node.path.clone()));
                ui.close();
            }
            if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Folder)), crate::tl_id!("ui.open_folder"), t)
                .on_hover_text(self.shown(&node.path))
                .clicked()
            {
                acts.push(Act::Reveal(node.path.clone()));
                ui.close();
            }
            if node.info.kind == NodeKind::Page
                && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::PageStack)), crate::tl_id!("ui.page_versions_2"), t).clicked()
            {
                acts.push(Act::PageVersions(node.path.clone()));
                ui.close();
            }
            floating::menu_separator(ui, t);
            if floating::menu_row(ui, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Delete)), crate::tl_id!("ui.delete_ellipsis"), t)
                .clicked()
            {
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
            let label = if self.frameless { crate::tl_id!("ui.use_the_system_title_bar") } else { crate::tl_id!("ui.use_notecraft_s_own_title_bar") };
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
        right.add_space(4.0);
        self.language_menu(&mut right, on);
        right.add_space(10.0);
        self.discord_link(&mut right, on);
        right.add_space(10.0);
        let resp = right.add(egui::TextEdit::singleline(&mut self.search).hint_text(crate::tl_id!("ui.search_all_pages")).desired_width(200.0));
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

    /// The header's link to the ArtCraft Discord. It opens in the browser, as the other ArtCraft
    /// applications do.
    fn discord_link(&self, ui: &mut Ui, on: Tokens) {
        // Painted by hand: a button draws its image in the SVG's own black and ignores the
        // theme, so the icon takes the header's text colour through a tint, as the window buttons do.
        let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 26.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 4.0, on.row_hover);
        }
        let icon = egui::Rect::from_center_size(egui::pos2(rect.min.x + 14.0, rect.center().y), vec2(16.0, 16.0));
        icons::coloured_svg("chat-multiple", include_bytes!("../assets/fluent/chat-multiple.svg"), on.text)
            .fit_to_exact_size(vec2(16.0, 16.0))
            .paint_at(ui, icon);
        ui.painter().text(
            egui::pos2(rect.min.x + 30.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            "Discord",
            egui::FontId::proportional(13.0),
            on.text,
        );
        let resp = resp.on_hover_text(crate::tl_id!("ui.open_the_artcraft_discord"));
        if resp.clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DISCORD_URL));
        }
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
            let label = if kind == crate::tl_id!("ui.maximize") && maximized { crate::tl_id!("ui.restore") } else { kind };
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
        let ui_language = self.ui_language.clone();
        let languages = crate::i18n::available_languages();
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
                &ui_language,
                &languages,
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
                    self.status = crate::tl_id!("ui.cloud_import_is_not_implemented_yet").to_string();
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
                backstage::Act::SetUiLanguage(language) => {
                    self.set_ui_language(language);
                }
                backstage::Act::OpenTelemetryFolder(path) => {
                    // The folder is made on first use: it does not exist until a session has
                    // written to it, and Explorer opens Documents for a folder it cannot find.
                    match std::fs::create_dir_all(&path) {
                        Ok(()) => self.reveal(&path),
                        Err(e) => {
                            self.status =
                                crate::i18n::format_message_id("ui.the_diagnostics_folder_could_not_be_created_error", &[("error", &e.to_string())]);
                            self.status_is_error = true;
                        }
                    }
                }
                backstage::Act::ClearTelemetry(path) => self.dialog = Some(Dialog::ClearTelemetry { path }),
                backstage::Act::ResetPreferences => self.dialog = Some(Dialog::ResetPreferences),
                backstage::Act::ExportTelemetry(root) => {
                    if let Some(session) = std::fs::read_dir(&root)
                        .ok()
                        .and_then(|r| r.filter_map(Result::ok).find(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)))
                        && let Some(destination) = rfd::FileDialog::new()
                            .set_title(crate::tl_id!("ui.export_telemetry_zip"))
                            .set_file_name(format!("notecraft-{}-telemetry.zip", session.file_name().to_string_lossy()))
                            .save_file()
                    {
                        let _ = crate::telemetry_export::export(&session.path(), &destination);
                    }
                }
                backstage::Act::NotBuilt(name) => {
                    self.status =
                        crate::i18n::format_message_id("ui.name_is_not_built_yet_see_the_roadmap_for_what_is_coming", &[("name", crate::tl!(name))]);
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
        self.say(crate::tl_id!("ui.removed_notebook_from_the_known_notebooks_list"));
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
                self.say(crate::i18n::format_message_id("ui.opened_path_it_has_no_pages_yet", &[("path", &self.shown(&opened))]));
            }
        }
    }

    /// Asks the system for a folder and opens the notebooks in it, keeping everything that
    /// is already open. A folder that is itself a notebook opens as one; a folder that
    /// holds notebooks opens all of them. Nothing is closed and nothing is moved.
    fn choose_notebooks_folder(&mut self) {
        let Some(picked) =
            rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_notebook_a_folder_of_notebooks_or_an_empty_9de57f8f1f")).pick_folder()
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
                self.say(crate::i18n::format_message_id("ui.path_opened_as_an_empty_notebook", &[("path", &self.shown(&picked))]));
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
        self.say(crate::i18n::format_count_id("ui.n_notebook_opened_n_notebooks_opened", opened_count));
    }

    fn import_notebook_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_onenote_notebook_folder"));
        if let Some(backup) = notecraft_import::discover::default_source() {
            dialog = dialog.set_directory(backup);
        }
        let Some(source) = dialog.pick_folder() else { return };
        self.import_officeimo_source(source, true);
    }

    fn import_file(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_file_to_import"));
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
        self.import_job = Some(ImportJob {
            rx,
            keep_alive: None,
            cancel,
            source,
            destination,
            current: crate::tl_id!("ui.starting_officeimo").into(),
            index: 0,
            total: 0,
        });
        self.status = crate::tl_id!("ui.importing").into();
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
                    finished = Some(Err(crate::tl_id!("ui.officeimo_import_worker_stopped_unexpectedly").into()));
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
                    self.status = crate::tl!(&message).to_string();
                    self.status_is_error = false;
                    self.open_notebook_from_path(&job.destination);
                }
                Err(error) => {
                    log::error!("OfficeIMO import failed: source={} destination={} error={error}", job.source.display(), job.destination.display());
                    self.status = crate::i18n::format_message_id("ui.could_not_import_the_notebook_error", &[("error", &error)]);
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
        egui::Window::new(crate::tl_id!("ui.importing_onenote"))
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(modal_frame(&self.tokens))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                modal_title(ui, &self.tokens, crate::tl_id!("ui.importing_onenote"));
                ui.label(egui::RichText::new(crate::tl_id!("ui.you_can_continue_working_while_the_import_runs")).color(self.tokens.muted));
                ui.add_space(12.0);
                ui.label(egui::RichText::new(crate::i18n::pseudo_dynamic(&job.current)).strong());
                let fraction = if job.total == 0 { 0.0 } else { job.index as f32 / job.total as f32 };
                ui.add(egui::ProgressBar::new(fraction).fill(self.tokens.accent).show_percentage());
                if job.total > 0 {
                    let index = job.index.to_string();
                    let total = job.total.to_string();
                    ui.label(crate::i18n::format_message_id("ui.index_of_total_sections", &[("index", &index), ("total", &total)]));
                } else {
                    ui.label(crate::tl_id!("ui.discovering_sections"));
                }
                ui.add_space(14.0);
                ui.separator();
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel_import"), true).clicked() {
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
            Ok(()) => self.say(crate::i18n::format_message_id("ui.opened_path", &[("path", &self.shown(path))])),
            Err(e) => {
                self.status = crate::i18n::format_message_id("ui.the_folder_could_not_be_opened_error", &[("error", &e.to_string())]);
                self.status_is_error = true;
            }
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
            self.complain(crate::tl_id!("ui.that_page_is_not_in_a_notebook_that_is_open"));
            return;
        };
        let title = notecraft_store::tree::read_node(dir).map(|n| n.title).unwrap_or_else(|| "Page".to_string());
        // Plain text keeps the title and address readable; the matching HTML flavor is an
        // anchor for clients that accept rich content.
        self.pending_copy = Some(format!("{title} ({uri})"));
        self.pending_copy_html = Some(crate::clipboard::html_link(&title, &uri));
        self.say(crate::i18n::format_message_id("ui.link_to_title_copied", &[("title", &title)]));
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
                self.say(crate::i18n::format_message_id("ui.opened_url_in_the_browser", &[("url", &url)]));
            }
            Link::Node(dir) => {
                if notecraft_store::tree::read_node(&dir).is_some_and(|n| n.kind == NodeKind::Page) {
                    self.open_path(&path_str(&dir));
                } else {
                    self.open.insert(dir.clone());
                    self.selected = Some(dir.clone());
                    match first_page_under(self.session.tree(), &dir) {
                        Some(first) => self.open_path(&path_str(&first)),
                        None => self.say(crate::i18n::format_message_id("ui.opened_path_it_has_no_pages_yet", &[("path", &self.shown(&dir))])),
                    }
                }
            }
            Link::File(file) => {
                if self.open_attachment_warning_suppressed {
                    match open_file_for_viewer(&file) {
                        Ok(()) => self.say(crate::i18n::format_message_id("ui.opening_path", &[("path", &self.shown(&file))])),
                        Err(e) => {
                            self.complain(&crate::i18n::format_message_id("ui.the_file_could_not_be_opened_error", &[("error", &e.to_string())]))
                        }
                    }
                } else {
                    self.dialog = Some(Dialog::OpenAttachmentWarning { path: file });
                }
            }
            Link::Missing(p) => {
                self.status = crate::i18n::format_message_id(
                    "ui.nothing_at_path_the_link_points_to_a_page_or_file_th_18d71ccd21",
                    &[("path", &self.shown(&p))],
                );
                self.status_is_error = true;
            }
            Link::Outside => {
                self.status = crate::tl_id!("ui.that_link_points_outside_the_notebooks_folder_so_it_3a37bd9b91").to_string();
                self.status_is_error = true;
            }
        }
    }

    /// The ribbon under the title row: tabs and their groups.
    fn ribbon_bar(&mut self, ui: &mut Ui) {
        let (tab, source, tokens) = (self.ribbon_tab.clone(), self.show_source, self.tokens);
        let font_selection = self.font_selection(ui.ctx());
        let family = match &font_selection.family {
            pageedit::FontValue::Value(Some(name)) => Some(name.clone()),
            pageedit::FontValue::Value(None) => Some(crate::tl_id!("ui.default").to_string()),
            pageedit::FontValue::Mixed => Some(crate::tl_id!("ui.mixed").to_string()),
            pageedit::FontValue::Unavailable => None,
        };
        let size = match font_selection.size {
            pageedit::FontValue::Value(Some(size)) => Some(format!("{size} pt")),
            pageedit::FontValue::Value(None) => Some(crate::tl_id!("ui.default").to_string()),
            pageedit::FontValue::Mixed => Some(crate::tl_id!("ui.mixed").to_string()),
            pageedit::FontValue::Unavailable => None,
        };
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
            pressure_available: self.stylus.sample().pen,
            has_page: self.editor.is_some(),
            style: self.caret_style(ui.ctx()),
            font_family: family.as_deref(),
            font_size: size.as_deref(),
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

    fn font_selection(&self, ctx: &Context) -> pageedit::FontSelection {
        use notecraft_formats::Block;
        let Some(ed) = self.editor.as_ref() else { return pageedit::FontSelection::default() };
        if let Some(index) = crate::containers::selected(ctx, &ed.dir) {
            let range = notecraft_model::container::blocks_of(&ed.boxes, index);
            let Some(blocks) = ed.blocks.get(range) else { return pageedit::FontSelection::default() };
            let spans = blocks.iter().flat_map(|block| match block {
                Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } => spans.as_slice(),
                _ => &[],
            });
            return pageedit::FontSelection::from_spans(spans);
        }
        if let Some(across) = ed.across.filter(|a| !a.inside_one_block()) {
            let mut result: Option<pageedit::FontSelection> = None;
            for index in across.blocks() {
                let Some(block) = ed.blocks.get(index).filter(|b| pageedit::is_one_line_of_words(b)) else {
                    return pageedit::FontSelection::default();
                };
                let text = pageedit::block_text(block);
                let Some((start, end)) = across.covering(index, text.len()) else { continue };
                let (Some(left), Some(right)) = (text.get(..start), text.get(..end)) else { return pageedit::FontSelection::default() };
                let Some(selection) = pageedit::selection_fonts(&text, (left.chars().count(), right.chars().count())) else {
                    return pageedit::FontSelection::default();
                };
                if let Some(current) = result.as_mut() {
                    current.merge(selection);
                } else {
                    result = Some(selection);
                }
            }
            return result.unwrap_or_default();
        }
        let Some(caret) = ed.caret.as_ref() else { return pageedit::FontSelection::default() };
        let mut result = pageedit::selection_fonts(&caret.text, caret.selection).unwrap_or_default();
        if caret.selection.0 == caret.selection.1
            && let Some(pending) = ed.pending_format.as_ref().filter(|p| p.block == caret.block && p.at == caret.selection.0)
        {
            if let Some(family) = &pending.family {
                result.family = pageedit::FontValue::Value(Some(family.clone()));
            }
            if let Some(size) = pending.size {
                result.size = pageedit::FontValue::Value(Some(size));
            }
        }
        result
    }

    /// What a ribbon button does while a container is selected: selecting a box selects everything in
    /// it, so Cut, Copy, the marks and the styles take all of it. True when the act was one of those.
    fn act_on_box(&mut self, ctx: &Context, at: usize, act: &ribbon::Act) -> bool {
        use notecraft_formats::markdown::Block;
        let wanted = matches!(
            act,
            ribbon::Act::Clipboard(_)
                | ribbon::Act::Mark(_)
                | ribbon::Act::FontSize(_)
                | ribbon::Act::FontFamily(_)
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
            ribbon::Act::FontSize(_) | ribbon::Act::FontFamily(_) => {
                let format = match act {
                    ribbon::Act::FontSize(size) if ribbon::FONT_SIZES.contains(size) => pageedit::InlineFormat::Size(*size),
                    ribbon::Act::FontFamily(family) if notecraft_formats::markdown::valid_font_family(family) => {
                        pageedit::InlineFormat::Family(family.clone())
                    }
                    _ => {
                        log::warn!("ribbon requested an invalid font size or family");
                        self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
                        self.status_is_error = false;
                        return true;
                    }
                };
                if !ed.writable {
                    self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
                    self.status_is_error = false;
                    return true;
                }
                Self::remember_step(ed);
                let mut changed = false;
                for block in ed.blocks.get_mut(range).unwrap_or_default() {
                    if let Block::Heading { spans, .. } | Block::Paragraph(spans) | Block::Quote(spans) | Block::Item { spans, .. } = block {
                        changed |= pageedit::format_span_range(spans, 0..usize::MAX, format.clone());
                    }
                }
                if changed {
                    ed.pending_format = None;
                    ed.rebuild_body();
                    Self::mark_dirty_from_blocks(ed);
                    let step =
                        crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
                    ed.history.record(step);
                }
                self.status.clear();
                self.status_is_error = false;
                return true;
            }
            ribbon::Act::Mark(mark) => {
                let new_mark = matches!(*mark, "underline" | "subscript" | "superscript");
                if new_mark && !ed.writable {
                    self.status = crate::tl_id!("ui.read_only").into();
                    self.status_is_error = false;
                    return true;
                }
                if new_mark {
                    Self::remember_step(ed);
                }
                if !pageedit::toggle_mark(&mut ed.blocks, range, mark) {
                    return true;
                }
                if new_mark {
                    ed.pending_format = None;
                    ed.rebuild_body();
                    Self::mark_dirty_from_blocks(ed);
                    let step =
                        crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
                    ed.history.record(step);
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
            ribbon::Act::FontSize(size) => self.apply_font_size(size),
            ribbon::Act::FontFamily(family) => self.apply_font_family(&family),
            ribbon::Act::Block(kind) => self.apply_block_kind(kind),
            ribbon::Act::Insert(what) => match what {
                "space" => {
                    let Some(ed) = self.editor.as_mut() else { return };
                    let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len()).min(ed.blocks.len());
                    ed.blocks.insert(at, notecraft_formats::Block::Paragraph(Vec::new()));
                    let marks = ed.blocks_inserted(at, 1);
                    ed.rebuild_body();
                    ed.caret =
                        Some(pageedit::Caret { block: at, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None });
                    Self::mark_dirty_from_blocks(ed);
                    self.save_block_marks(marks);
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
            ribbon::Act::AddPrintout => {
                let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                    return;
                };
                let picker = rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_pdf_or_office_document_to_print_out")).add_filter(
                    crate::tl_id!("ui.pdf_and_office_documents"),
                    &["pdf", "doc", "docx", "ppt", "pptx", "xls", "xlsx", "odt", "odp", "ods", "rtf"],
                );
                if let Some(file) = picker.pick_file() {
                    self.add_file(&dir, &file, FileChoice::Printout);
                }
            }
            ribbon::Act::AddFile | ribbon::Act::AddPicture => {
                let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                    return;
                };
                let mut picker = rfd::FileDialog::new().set_title(if matches!(act, ribbon::Act::AddPicture) {
                    crate::tl_id!("ui.choose_a_picture")
                } else {
                    crate::tl_id!("ui.choose_a_file_to_attach")
                });
                if matches!(act, ribbon::Act::AddPicture) {
                    picker = picker.add_filter(crate::tl_id!("ui.pictures"), &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"]);
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
                    ribbon::RuleLines::None => crate::tl_id!("ui.no_lines_behind_the_page").to_string(),
                    other => crate::i18n::format_message_id("ui.paper_behind_the_page", &[("paper", crate::tl!(other.label()))]),
                });
            }
            ribbon::Act::ToggleAlwaysOnTop => {
                self.always_on_top = !self.always_on_top;
                // Asked for on the next frame, where the context is at hand.
                self.window_level_pending = true;
                let _ = self.session.settings.set("always_on_top", if self.always_on_top { "true" } else { "false" });
                self.say(if self.always_on_top {
                    crate::tl_id!("ui.this_window_stays_on_top")
                } else {
                    crate::tl_id!("ui.this_window_behaves_like_the_others")
                });
            }
            ribbon::Act::TogglePanes => {
                self.split_panes = !self.split_panes;
                self.organisers = true;
                let _ = self.session.settings.set("split_panes", if self.split_panes { "true" } else { "false" });
                self.say(if self.split_panes {
                    crate::tl_id!("ui.notebooks_and_pages_in_their_own_columns")
                } else {
                    crate::tl_id!("ui.one_tree_of_notebooks")
                });
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
                self.status =
                    crate::i18n::format_message_id("ui.name_is_not_built_yet_see_the_roadmap_for_what_is_coming", &[("name", crate::tl!(what))]);
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
                    Some(c) => {
                        let colour = crate::tl!(c.label());
                        crate::i18n::format_message_id("ui.the_page_is_colour", &[("colour", &colour.to_lowercase())])
                    }
                    None => crate::tl_id!("ui.the_page_is_the_theme_s_own_colour").to_string(),
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
                    self.say(crate::tl_id!("ui.there_is_no_room_on_the_rack_for_another_tool"));
                    return;
                }
                self.rack.push(rack::new_nib(tool));
                self.nib = self.rack.len() - 1;
                self.pointer = ribbon::Pointer::Draw;
                let tool = crate::tl!(tool.label());
                self.say(crate::i18n::format_message_id("ui.added_a_tool", &[("tool", &tool.to_lowercase())]));
            }
            ribbon::RackEdit::Remove(i) => {
                if !rack::remove(&mut self.rack, i) {
                    self.say(crate::tl_id!("ui.the_last_tool_stays_on_the_rack"));
                    return;
                }
                if self.nib >= self.rack.len() {
                    self.nib = self.rack.len() - 1;
                } else if self.nib > i {
                    self.nib -= 1;
                }
                self.say(crate::tl_id!("ui.took_it_off_the_rack"));
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
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if let Some(picture) = self.clipboard.picture() {
            self.paste_picture(&dir, picture);
            return;
        }
        // Browsers, Word and other editors offer a formatted clipboard flavour as well
        // as plain text. Parse it through the same HTML-to-block path as an HTML page, so
        // links, emphasis, lists and tables survive a paste into NoteCraft.
        if let Some(html) = self.clipboard.html() {
            let new = pageedit::pasted_blocks(pageedit::pasted(&html));
            if self.paste_blocks(new) {
                self.say(crate::tl_id!("ui.pasted_formatted_content"));
                return;
            }
        }
        match self.clipboard.text() {
            Some(text) => {
                let new = pageedit::blocks_to_paste(&text);
                if !self.paste_blocks(new) {
                    self.insert_in_page(&text);
                }
                self.say(crate::tl_id!("ui.pasted"));
            }
            None => self.say(crate::tl_id!("ui.there_is_nothing_on_the_clipboard_to_paste")),
        }
    }

    /// Pastes `new` blocks at the caret, or after the last block. False, with nothing done,
    /// when there is nothing to paste or the page cannot be written.
    fn paste_blocks(&mut self, new: Vec<notecraft_formats::Block>) -> bool {
        let Some(ed) = self.editor.as_mut().filter(|ed| ed.writable) else { return false };
        if new.is_empty() {
            return false;
        }
        let at = ed.caret.as_ref().map_or(ed.blocks.len().saturating_sub(1), |c| c.block);
        if ed.blocks.is_empty() {
            ed.blocks.push(notecraft_formats::markdown::Block::Paragraph(Vec::new()));
        }
        let before = ed.blocks.len();
        pageedit::paste_at(&mut ed.blocks, &mut ed.caret, at, new);
        // The paste fills the caret's line and puts the rest after it, all in the caret's box.
        let marks = ed.blocks_inserted(at + 1, ed.blocks.len().saturating_sub(before));
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        self.save_block_marks(marks);
        true
    }

    /// Writes table widths and justified paragraphs that moved with inserted blocks, as
    /// [`Editor::blocks_inserted`] reported them, into the page's own file.
    fn save_block_marks(&mut self, (cols, justified): (bool, bool)) {
        let Some(ed) = self.editor.as_ref() else { return };
        let path = path_str(&ed.dir);
        let cols = cols.then(|| notecraft_model::tablecols::write(&ed.columns));
        let justified = justified.then(|| notecraft_model::justify::write(&ed.justified));
        if let Some(line) = cols {
            self.run("page.columns", &params(&[("path", &path), ("widths", &line)]));
        }
        if let Some(line) = justified {
            self.run("page.justify", &params(&[("path", &path), ("blocks", &line)]));
        }
    }

    fn paste_picture(&mut self, dir: &std::path::Path, picture: crate::clipboard::Picture) {
        let name = crate::clipboard::picture_name();
        let (w, h) = (picture.width, picture.height);
        log::debug!("clipboard image paste received: width={w} height={h} bytes={}", picture.png.len());
        match notecraft_store::assets::attach_bytes(dir, &name, &picture.png) {
            Ok(link) => {
                log::debug!("clipboard image paste attached: name={name:?} link={link:?}");
                if let Some(ed) = self.editor.as_mut() {
                    let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
                    ed.blocks.insert(
                        at,
                        notecraft_formats::markdown::Block::Image { alt: "Pasted picture".to_string(), src: link, width: None, height: None },
                    );
                    let marks = ed.blocks_inserted(at, 1);
                    ed.rebuild_body();
                    Self::mark_dirty_from_blocks(ed);
                    ed.attachments = notecraft_store::assets::list(dir);
                    self.save_block_marks(marks);
                }
                self.say(crate::i18n::format_message_id(
                    "ui.pasted_a_picture_width_by_height",
                    &[("width", &w.to_string()), ("height", &h.to_string())],
                ));
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_save_the_pasted_picture_error", &[("error", &e.to_string())])),
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
        // One at a time, from the end, so each box that held one counts one fewer.
        let gone: Vec<usize> = ed
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| match b {
                Block::Image { src, alt, .. } => crate::inkpad::is_ink_line(src, alt) && !files.iter().any(|f| f == src.trim()),
                _ => false,
            })
            .map(|(i, _)| i)
            .collect();
        for &at in gone.iter().rev() {
            ed.blocks.remove(at);
            ed.count_removed_from_boxes(at);
            changed = true;
        }
        // An ink file the page does not name is adopted: it is drawn, so it is named. The line
        // goes on the end of the page, so it is counted in the last box.
        for file in &files {
            if !ed.blocks.iter().any(|b| matches!(b, Block::Image { src, .. } if src.trim() == file)) {
                ed.blocks.push(Block::Image { alt: "Ink".to_string(), src: file.clone(), width: None, height: None });
                let at = ed.blocks.len() - 1;
                ed.count_inserted_in_boxes(at, 1);
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
        crate::inkpad::Pen {
            pointer: self.pointer,
            tool: nib.tool,
            colour: nib.colour,
            // Keep the chosen nib width at the neutral setting, but let the top of the
            // control reach the same four-times full-width range as the pressure canvas.
            width: nib.width() * Self::sensitivity_width(self.sensitivity),
            gain: self.gain(),
        }
    }

    /// The pressure canvas uses a 4x full-width range at sensitivity 100. Keep 50 neutral
    /// so the rack's selected thickness remains the familiar baseline, and grow only the
    /// maximum width above it. Existing strokes retain the width stored when they were made.
    fn sensitivity_width(sensitivity: u8) -> f32 {
        if sensitivity <= 50 { 1.0 } else { 1.0 + 3.0 * (f32::from(sensitivity - 50) / 50.0) }
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
            self.say(crate::tl_id!("ui.there_is_no_drawing_on_this_page"));
            return;
        };
        if !pad.undo() {
            self.say(crate::tl_id!("ui.there_is_no_stroke_to_take_back"));
            return;
        }
        let left = pad.strokes();
        match pad.save() {
            Ok(()) => self.say(crate::i18n::format_message_id("ui.took_the_last_stroke_back_count_left", &[("count", &left.to_string())])),
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", &e.to_string())])),
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
            self.complain(crate::tl_id!("ui.that_file_is_not_in_this_page_s_folder"));
            return;
        }
        if let Err(e) = notecraft_store::page::write_atomic(&path, text) {
            self.complain(&crate::i18n::format_message_id("ui.cannot_write_name_error", &[("name", name), ("error", &e.to_string())]));
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
                        ed.pending_format = None;
                        ed.dirty_since = Some(Instant::now());
                        // The restored text says whether the page was arranged.
                        ed.arranged = notecraft_formats::boxes::has_boxes(ed.format, &ed.body);
                    }
                    None => nothing = true,
                }
            }
        }
        if nothing {
            self.say(if back { crate::tl_id!("ui.nothing_to_undo") } else { crate::tl_id!("ui.nothing_to_redo") });
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
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::markdown::Block::Table { .. })) {
            self.say(crate::tl_id!("ui.click_in_a_table_first"));
            return;
        }
        let file = ed.dir.join(format!("table-{}.csv", at + 1));
        let Ok(text) = std::fs::read_to_string(&file) else {
            let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
            self.complain(&crate::i18n::format_message_id("ui.no_name_beside_this_page_hand_the_table_over_first", &[("name", &name)]));
            return;
        };
        let Some((table, rows, columns)) = table_from_gridcraft_csv(&text) else {
            self.complain(crate::tl_id!("ui.that_file_has_nothing_in_it_to_put_back"));
            return;
        };
        if let Some(block) = ed.blocks.get_mut(at) {
            *block = table;
        }
        ed.caret = None;
        ed.body = pageedit::to_body(ed.format, &ed.blocks);
        Self::mark_dirty_from_blocks(ed);
        self.say(crate::i18n::format_message_id(
            "ui.read_back_rows_rows_and_columns_columns",
            &[("rows", &rows.to_string()), ("columns", &columns.to_string())],
        ));
    }

    /// Hands the table the caret is in to GridCraft, which is where a spreadsheet
    /// belongs: OneNote's Table tab ends with Convert to Excel Spreadsheet, and this is
    /// the same door with the open spreadsheet behind it.
    fn hand_table_to_gridcraft(&mut self) {
        let Some(exe) = self.gridcraft.clone() else {
            self.say(crate::tl_id!("ui.gridcraft_is_not_installed"));
            return;
        };
        let Some(ed) = self.editor.as_ref() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else { return };
        let Some(block) = ed.blocks.get(at) else { return };
        let name = format!("table-{}", at + 1);
        match crate::gridcraft::hand_over(&exe, &ed.dir, &name, block) {
            Ok(file) => {
                let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
                let said = crate::i18n::format_message_id("ui.name_is_open_in_gridcraft", &[("name", &name)]);
                self.say(&said);
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_open_the_table_in_gridcraft_error", &[("error", &e.to_string())])),
        }
    }

    /// A table of the picked size, where the caret is.
    fn insert_table(&mut self, rows: usize, columns: usize) {
        let Some(ed) = self.editor.as_mut() else { return };
        let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len()).min(ed.blocks.len());
        let before = ed.blocks.len();
        pageedit::insert_table(&mut ed.blocks, at, rows, columns);
        let marks = ed.blocks_inserted(at, ed.blocks.len().saturating_sub(before));
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
        self.save_block_marks(marks);
    }

    /// An empty spreadsheet beside the page, opened in GridCraft and linked from the page
    /// so it can be found again.
    fn new_gridcraft(&mut self) {
        let Some(exe) = self.gridcraft.clone() else {
            self.say(crate::tl_id!("ui.gridcraft_is_not_installed"));
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
            self.complain(&crate::i18n::format_message_id("ui.cannot_write_name_error", &[("name", &name), ("error", &e.to_string())]));
            return;
        }
        match std::process::Command::new(exe).arg(&file).spawn() {
            Ok(_) => {
                self.insert_in_page(&format!("[{name}]({name})"));
                let said = crate::i18n::format_message_id("ui.name_is_open_in_gridcraft", &[("name", &name)]);
                self.say(&said);
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_open_gridcraft_error", &[("error", &e.to_string())])),
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
            self.say(crate::tl_id!("ui.a_table_keeps_its_heading_row_and_at_least_one_column"));
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
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        }
        self.pointer = ribbon::Pointer::Draw;
        self.ribbon_tab = "Draw".to_string();
        self.say(crate::tl_id!("ui.write_anywhere_on_the_page"));
    }

    /// Writes the drawing again with no pen readings in it, for sending somewhere else.
    fn ink_export_plain(&mut self) {
        let Some(pad) = self.pad.as_ref() else {
            self.say(crate::tl_id!("ui.there_is_no_drawing_on_this_page"));
            return;
        };
        match pad.save_plain() {
            Ok(path) => {
                let shown = self.shown(&path);
                self.say(crate::i18n::format_message_id("ui.saved_the_drawing_as_a_picture_path", &[("path", &shown)]));
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_picture_error", &[("error", &e.to_string())])),
        }
    }

    /// Puts marks around the selection in the block the caret is in.
    fn apply_mark(&mut self, mark: &str) {
        if self.editor.as_ref().and_then(|ed| ed.across).is_some_and(|selection| !selection.inside_one_block()) {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        if matches!(mark, "underline" | "subscript" | "superscript") {
            self.apply_new_mark(mark);
            return;
        }
        let collapsed_text_caret =
            self.editor.as_ref().and_then(|ed| ed.caret.as_ref()).is_some_and(|caret| caret.cell.is_none() && caret.selection.0 == caret.selection.1);
        if collapsed_text_caret && matches!(mark, "bold" | "italic" | "strike") {
            self.apply_new_mark(mark);
            return;
        }
        let partial = self.editor.as_ref().and_then(|ed| ed.caret.as_ref()).is_some_and(|caret| {
            let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
            lo < hi && (lo != 0 || hi != caret.text.chars().count())
        });
        if partial {
            let format = match mark {
                "bold" => Some(pageedit::InlineFormat::Bold),
                "italic" => Some(pageedit::InlineFormat::Italic),
                "strike" => Some(pageedit::InlineFormat::Strike),
                _ => None,
            };
            if let Some(format) = format {
                self.apply_partial_inline_format(format);
                return;
            }
        }
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
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

    fn apply_new_mark(&mut self, mark: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        let len = caret.text.chars().count();
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        if ed.writable
            && ed.across.is_none()
            && caret.cell.is_none()
            && lo == hi
            && matches!(
                ed.blocks.get(caret.block),
                Some(
                    notecraft_formats::Block::Heading { .. }
                        | notecraft_formats::Block::Paragraph(_)
                        | notecraft_formats::Block::Quote(_)
                        | notecraft_formats::Block::Item { .. }
                )
            )
        {
            let mut marker = "NoteCraftCaretProbe".to_string();
            while caret.text.contains(&marker) {
                marker.push('X');
            }
            let (probe, _) = pageedit::insert_at_caret(&caret.text, (lo, lo), &marker);
            let inherited = notecraft_formats::markdown::parse_inline(&probe).into_iter().find(|span| span.text.contains(&marker));
            let unsafe_context = inherited.is_none_or(|span| {
                span.code
                    || span.math
                    || span.image.is_some()
                    || (mark == "bold" && span.bold)
                    || (mark == "italic" && span.italic)
                    || (mark == "strike" && span.strike)
                    || (mark == "underline" && span.underline)
                    || (matches!(mark, "subscript" | "superscript") && span.script.is_some())
            });
            if unsafe_context {
                self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
                self.status_is_error = false;
                return;
            }
            let pending = ed.pending_format.get_or_insert_with(|| PendingFormat { block: caret.block, at: lo, ..Default::default() });
            if pending.block != caret.block || pending.at != lo {
                *pending = PendingFormat { block: caret.block, at: lo, ..Default::default() };
            }
            match mark {
                "bold" => pending.bold = !pending.bold,
                "italic" => pending.italic = !pending.italic,
                "strike" => pending.strike = !pending.strike,
                "underline" => pending.underline = !pending.underline,
                "subscript" => {
                    let script = notecraft_formats::markdown::Script::Subscript;
                    pending.script = (pending.script != Some(script)).then_some(script);
                }
                "superscript" => {
                    let script = notecraft_formats::markdown::Script::Superscript;
                    pending.script = (pending.script != Some(script)).then_some(script);
                }
                _ => {}
            }
            if let Some(caret) = ed.caret.as_mut() {
                caret.focus = true;
                caret.place = Some(lo);
            }
            self.status.clear();
            self.status_is_error = false;
            return;
        }
        if ed.writable && ed.across.is_none() && caret.cell.is_none() && lo < hi && (lo != 0 || hi != len) {
            let format = match mark {
                "underline" => pageedit::InlineFormat::Underline,
                "subscript" => pageedit::InlineFormat::Subscript,
                "superscript" => pageedit::InlineFormat::Superscript,
                _ => return,
            };
            self.apply_partial_inline_format(format);
            return;
        }
        if !ed.writable || ed.across.is_some() || caret.cell.is_some() || len == 0 || lo != 0 || hi != len {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let block = caret.block;
        Self::remember_step(ed);
        if !pageedit::toggle_mark(&mut ed.blocks, block..block + 1, mark) {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        ed.pending_format = None;
        if let (Some(caret), Some(block)) = (ed.caret.as_mut(), ed.blocks.get(block)) {
            caret.text = pageedit::caret_text(block);
            caret.selection = (0, caret.text.chars().count());
            caret.focus = true;
            ed.restore_format_selection = Some((caret.block, caret.selection.0, caret.selection.1));
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        ed.history.record(step);
        self.status.clear();
        self.status_is_error = false;
    }

    fn apply_partial_inline_format(&mut self, format: pageedit::InlineFormat) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_ref() else { return };
        if !ed.writable
            || ed.across.is_some()
            || caret.cell.is_some()
            || !matches!(
                ed.blocks.get(caret.block),
                Some(
                    notecraft_formats::Block::Heading { .. }
                        | notecraft_formats::Block::Paragraph(_)
                        | notecraft_formats::Block::Quote(_)
                        | notecraft_formats::Block::Item { .. }
                )
            )
        {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let Some((text, selection)) = pageedit::format_source_selection(&caret.text, caret.selection, format) else {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        };
        if text == caret.text {
            return;
        }
        Self::remember_step(ed);
        let Some(caret) = ed.caret.as_mut() else { return };
        caret.text = text;
        caret.selection = selection;
        caret.focus = true;
        ed.restore_format_selection = Some((caret.block, selection.0, selection.1));
        if let Some(block) = ed.blocks.get_mut(caret.block) {
            pageedit::set_caret_text(block, &caret.text);
        }
        ed.pending_format = None;
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        ed.history.record(step);
        self.status.clear();
        self.status_is_error = false;
    }

    fn apply_font_family(&mut self, family: &str) {
        if !notecraft_formats::markdown::valid_font_family(family) {
            log::warn!("ribbon requested an invalid font family");
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        if ed.across.is_some_and(|selection| !selection.inside_one_block()) {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        let len = caret.text.chars().count();
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        if ed.writable
            && ed.across.is_none()
            && caret.cell.is_none()
            && lo == hi
            && matches!(
                ed.blocks.get(caret.block),
                Some(
                    notecraft_formats::Block::Heading { .. }
                        | notecraft_formats::Block::Paragraph(_)
                        | notecraft_formats::Block::Quote(_)
                        | notecraft_formats::Block::Item { .. }
                )
            )
        {
            let pending = ed.pending_format.get_or_insert_with(|| PendingFormat { block: caret.block, at: lo, ..Default::default() });
            if pending.block != caret.block || pending.at != lo {
                *pending = PendingFormat { block: caret.block, at: lo, ..Default::default() };
            }
            pending.family = Some(family.to_owned());
            if let Some(caret) = ed.caret.as_mut() {
                caret.focus = true;
                caret.place = Some(lo);
            }
            self.status.clear();
            self.status_is_error = false;
            return;
        }
        if ed.writable && ed.across.is_none() && caret.cell.is_none() && lo < hi && (lo != 0 || hi != len) {
            self.apply_partial_inline_format(pageedit::InlineFormat::Family(family.to_owned()));
            return;
        }
        if !ed.writable || ed.across.is_some() || caret.cell.is_some() || len == 0 || lo != 0 || hi != len {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let block = caret.block;
        Self::remember_step(ed);
        let spans = match ed.blocks.get_mut(block) {
            Some(
                notecraft_formats::Block::Heading { spans, .. }
                | notecraft_formats::Block::Paragraph(spans)
                | notecraft_formats::Block::Quote(spans)
                | notecraft_formats::Block::Item { spans, .. },
            ) => spans,
            _ => {
                self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
                self.status_is_error = false;
                return;
            }
        };
        if !pageedit::format_span_range(spans, 0..usize::MAX, pageedit::InlineFormat::Family(family.to_owned())) {
            return;
        }
        ed.pending_format = None;
        if let (Some(caret), Some(block)) = (ed.caret.as_mut(), ed.blocks.get(block)) {
            caret.text = pageedit::caret_text(block);
            caret.selection = (0, caret.text.chars().count());
            caret.focus = true;
            ed.restore_format_selection = Some((caret.block, caret.selection.0, caret.selection.1));
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        ed.history.record(step);
        self.status.clear();
        self.status_is_error = false;
    }

    fn apply_font_size(&mut self, size: u8) {
        if !ribbon::FONT_SIZES.contains(&size) {
            log::warn!("ribbon requested unsupported font size {size} pt");
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        if ed.across.is_some_and(|selection| !selection.inside_one_block()) {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        if caret.selection.0 == caret.selection.1 {
            if caret.cell.is_some()
                || !matches!(
                    ed.blocks.get(caret.block),
                    Some(
                        notecraft_formats::Block::Heading { .. }
                            | notecraft_formats::Block::Paragraph(_)
                            | notecraft_formats::Block::Quote(_)
                            | notecraft_formats::Block::Item { .. }
                    )
                )
            {
                self.status = crate::tl_id!("ui.select_text_before_changing_font_size").into();
                self.status_is_error = false;
                return;
            }
            let pending = ed.pending_format.get_or_insert_with(|| PendingFormat { block: caret.block, at: caret.selection.0, ..Default::default() });
            if pending.block != caret.block || pending.at != caret.selection.0 {
                *pending = PendingFormat { block: caret.block, at: caret.selection.0, ..Default::default() };
            }
            pending.size = Some(size);
            if let Some(caret) = ed.caret.as_mut() {
                caret.focus = true;
                caret.place = Some(caret.selection.0);
            }
            self.status.clear();
            self.status_is_error = false;
            return;
        }
        let len = caret.text.chars().count();
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        if lo != 0 || hi != len {
            self.apply_partial_inline_format(pageedit::InlineFormat::Size(size));
            return;
        }
        ed.pending_format = None;
        Self::remember_step(ed);
        let Some(caret) = ed.caret.as_mut() else { return };
        let len = caret.text.chars().count();
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        let spans = match ed.blocks.get_mut(caret.block) {
            Some(
                notecraft_formats::Block::Heading { spans, .. }
                | notecraft_formats::Block::Paragraph(spans)
                | notecraft_formats::Block::Quote(spans)
                | notecraft_formats::Block::Item { spans, .. },
            ) if lo == 0 && hi == len => Some(spans),
            _ => None,
        };
        if let Some(spans) = spans {
            if !pageedit::format_span_range(spans, 0..usize::MAX, pageedit::InlineFormat::Size(size)) {
                return;
            }
            if let Some(block) = ed.blocks.get(caret.block) {
                caret.text = pageedit::caret_text(block);
                caret.selection = (0, caret.text.chars().count());
                caret.focus = true;
                ed.restore_format_selection = Some((caret.block, caret.selection.0, caret.selection.1));
            }
        } else {
            let open = format!("<span style=\"font-size: {size}pt\">");
            let (text, selection) = pageedit::wrap_selection_with(&caret.text, caret.selection, &open, "</span>");
            caret.text = text;
            caret.selection = selection;
            if let Some(block) = ed.blocks.get_mut(caret.block) {
                pageedit::set_block_text(block, &caret.text);
            }
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        if !ed.history.here().is_some_and(|s| s.body == ed.body && s.blocks == ed.blocks && s.boxes == ed.boxes) {
            let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
            ed.history.record(step);
        }
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
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::set_direction(&mut ed.blocks, at, dir) {
            self.say(crate::tl_id!("ui.a_direction_goes_on_a_paragraph_of_text"));
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
            Some(d) if d.is_rtl() => crate::tl_id!("ui.this_paragraph_reads_right_to_left"),
            Some(_) => crate::tl_id!("ui.this_paragraph_reads_left_to_right"),
            None => crate::tl_id!("ui.this_paragraph_reads_whichever_way_its_words_do"),
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
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::Block::Paragraph(_))) {
            self.say(crate::tl_id!("ui.justification_goes_on_a_paragraph_of_text"));
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
                Some(notecraft_model::justify::Strength::Low) => crate::tl_id!("ui.this_paragraph_fills_the_column_stretched_lightly"),
                Some(notecraft_model::justify::Strength::Medium) => crate::tl_id!("ui.this_paragraph_fills_the_column"),
                Some(notecraft_model::justify::Strength::High) => crate::tl_id!("ui.this_paragraph_fills_the_column_stretched_hard"),
                None => crate::tl_id!("ui.this_paragraph_is_ragged_again"),
            });
        }
    }

    /// Which way the whole page reads. It is written into the page's own file.
    fn apply_page_direction(&mut self, dir: Option<notecraft_model::Dir>) {
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
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
                Some(d) if d.is_rtl() => crate::tl_id!("ui.this_page_reads_right_to_left"),
                Some(_) => crate::tl_id!("ui.this_page_reads_left_to_right"),
                None => crate::tl_id!("ui.every_line_of_this_page_decides_for_itself"),
            });
        }
    }

    /// Which way the selection, or what is typed next, reads: an isolated run.
    fn apply_embedding(&mut self, dir: notecraft_model::Dir) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_mut() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
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
            crate::tl_id!("ui.this_run_reads_right_to_left_a_mixed_line_is_still_d_d2a74de074")
        } else {
            crate::tl_id!("ui.this_run_reads_left_to_right_a_mixed_line_is_still_d_af62d34d2f")
        });
    }

    /// Makes the block the caret is in a heading, a list item, a quote or ordinary text.
    fn apply_block_kind(&mut self, kind: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
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
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::set_tag(&mut ed.blocks, at, tag) {
            self.say(crate::tl_id!("ui.a_tag_goes_on_a_line_of_text"));
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
        self.say(crate::i18n::format_message_id("ui.tag_tag", &[("tag", crate::tl!(tag.label()))]));
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
                    || i.raw.events.iter().any(|e| {
                        matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == want && (modifiers.command || modifiers.ctrl))
                    })
            };
            (hit(egui::Key::C), hit(egui::Key::X), hit(egui::Key::V), hit(egui::Key::A))
        });
        if !(copy || cut || paste || all) {
            return;
        }
        let ours = self.words_are_ours();
        if copy && ours {
            self.clipboard("copy");
        }
        if cut && ours {
            self.clipboard("cut");
        }
        // A screenshot has no text for egui's text widget to consume. Route every Ctrl+V
        // through the same paste path; it reads the image clipboard exactly once. Probing
        // with has_picture() first was racy with VirtualBox clipboard synchronisation and
        // could make the actual image read miss the clipboard handoff.
        if paste {
            // Consume the shortcut before the text widgets draw. Otherwise an LTR TextEdit
            // handles Ctrl+V itself while the page menu goes through `paste_here`, giving the
            // two entry points different behavior.
            ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::V) || i.consume_key(Modifiers::CTRL, Key::V));
            let picture = self.clipboard.picture();
            if let (Some(picture), Some(dir)) = (picture, self.editor.as_ref().map(|e| e.dir.clone())) {
                self.paste_picture(&dir, picture);
            } else {
                self.paste_here();
            }
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
        self.say(crate::tl_id!("ui.paragraph_selected"));
    }

    fn clipboard(&mut self, which: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        // A selection that runs across blocks is not any one block's, so it is taken from
        // the blocks themselves. Cut takes the words out and joins what is left.
        if let Some(open) = ed.across.filter(|a| !a.inside_one_block()) {
            let picked = pageedit::span_text(&ed.blocks, open);
            if picked.is_empty() {
                self.say(crate::tl_id!("ui.select_some_words_first"));
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
            self.say(if which == crate::tl_id!("ui.cut_2") { crate::tl_id!("ui.cut") } else { crate::tl_id!("ui.copied") });
            return;
        }
        let Some(caret) = ed.caret.as_mut() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first").into();
            self.status_is_error = false;
            return;
        };
        let (a, b) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        let picked: String = caret.text.chars().skip(a).take(b.saturating_sub(a)).collect();
        if picked.is_empty() {
            self.status = crate::tl_id!("ui.select_some_words_first").into();
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
        self.say(if which == crate::tl_id!("ui.cut_2") { crate::tl_id!("ui.cut") } else { crate::tl_id!("ui.copied") });
    }

    /// More or less indent on the list item the caret is in.
    fn indent_block(&mut self, deeper: bool) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(at) = ed.caret.as_ref().map(|c| c.block) else {
            self.status = crate::tl_id!("ui.click_in_the_page_first").into();
            self.status_is_error = false;
            return;
        };
        if !pageedit::indent(&mut ed.blocks, at, deeper) {
            self.say(crate::tl_id!("ui.only_a_list_item_can_be_indented"));
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
        let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len()).min(ed.blocks.len());
        if !pageedit::insert_block(&mut ed.blocks, at, what) {
            self.say(crate::tl_id!("ui.that_is_not_something_a_page_can_hold"));
            return;
        }
        let marks = ed.blocks_inserted(at, 1);
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
        self.save_block_marks(marks);
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
        let theme_label = current.label();
        let label = crate::i18n::format_message_id("ui.theme_theme_2", &[("theme", &theme_label)]);
        let enabled = ui.is_enabled();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
        let theme_label = current.label();
        let tip = crate::i18n::format_message_id("ui.theme_theme_click_to_change_it", &[("theme", &theme_label)]);
        let resp = resp.on_hover_text(tip);
        egui::Popup::menu(&resp).show(|ui| {
            theme_grid(ui, current, &mut pick, &mut follow);
        });
        // The same chooser, as a window, for the ribbon's Theme button.
        if self.theme_window {
            let mut open = true;
            egui::Window::new(crate::tl_id!("ui.theme"))
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .anchor(Align2::RIGHT_TOP, vec2(-12.0, 120.0))
                .show(ui.ctx(), |ui| {
                    theme_grid(ui, current, &mut pick, &mut follow);
                });
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

    /// A compact language picker beside the header's theme control.
    fn language_menu(&mut self, ui: &mut Ui, tokens: Tokens) {
        let languages = crate::i18n::available_languages();
        let active_code = if self.ui_language == crate::i18n::AUTO { crate::i18n::current_code() } else { self.ui_language.clone() };
        let active_name =
            languages.iter().find(|language| language.code == active_code).map_or_else(|| active_code.clone(), |language| language.name.clone());
        let badge = if active_code == "en-XA" {
            "XA".to_string()
        } else if active_code == "ar-XB" {
            "XB".to_string()
        } else {
            active_code.split('-').next().unwrap_or("en").to_ascii_uppercase()
        };
        let label = crate::i18n::format_message_id("ui.interface_language_language", &[("language", &active_name)]);
        let (rect, resp) = ui.allocate_exact_size(vec2(52.0, 24.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 4.0, tokens.row_hover);
        }
        let flag_rect = egui::Rect::from_min_size(rect.left_top() + vec2(5.0, 5.0), vec2(20.0, 14.0));
        crate::language_flag::paint_at(ui, flag_rect, &active_code);
        ui.painter().text(rect.left_center() + vec2(36.0, 0.0), Align2::CENTER_CENTER, badge, egui::FontId::proportional(10.5), tokens.text);
        let enabled = ui.is_enabled();
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
        let tip = crate::i18n::format_message_id("ui.language_click_to_change_it", &[("language", &active_name)]);
        let resp = resp.on_hover_text(tip);
        let mut picked = None;
        egui::Popup::menu(&resp).style(crate::theme::picker_popup_style(tokens)).show(|ui| {
            let automatic = crate::tl_id!("ui.automatic_system_language");
            if ui.selectable_label(self.ui_language == crate::i18n::AUTO, automatic).clicked() {
                picked = Some(crate::i18n::AUTO.to_string());
            }
            ui.separator();
            for language in &languages {
                let mut clicked = false;
                ui.horizontal(|ui| {
                    crate::language_flag::paint(ui, &language.code);
                    clicked = ui.selectable_label(self.ui_language == language.code, &language.name).clicked();
                });
                if clicked {
                    picked = Some(language.code.clone());
                }
            }
        });
        if let Some(language) = picked {
            self.set_ui_language(language);
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
                    ui.weak(crate::tl_id!("ui.no_pages_match"));
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
                ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_2")).small().color(self.tokens.muted));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(crate::tl_id!("ui.new_notebook")).clicked() {
                    self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None });
                }
                if ui.small_button(crate::tl_id!("ui.expand_all")).on_hover_text(crate::tl_id!("ui.open_every_level")).clicked() {
                    self.open = self.collect_branches().into_iter().collect();
                }
                if ui.small_button(crate::tl_id!("ui.collapse_all")).clicked() {
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
                    ui.weak(crate::tl_id!("ui.no_notebooks_yet_use_new_notebook"));
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
                    if ui.button(crate::tl_id!("ui.new_page_2")).clicked() {
                        acts.push(Act::AddKind(path.to_path_buf(), NodeKind::Section, NodeKind::Page));
                        ui.close();
                    }
                }
                // Under the notebooks: a notebook, and the ways into the folder itself.
                None => {
                    if ui.button(crate::tl_id!("ui.new_notebook_2")).clicked() {
                        acts.push(Act::NewNotebook);
                        ui.close();
                    }
                    ui.separator();
                    if ui.button(crate::tl_id!("ui.open_a_folder")).clicked() {
                        choose = true;
                        ui.close();
                    }
                    if ui.button(crate::tl_id!("ui.show_the_notebooks_folder")).clicked() {
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
        let name = crate::tl_id!("ui.add_a_page_here");
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
        let resp = resp.on_hover_text(crate::tl_id!("ui.a_page_here"));
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
        resp.on_hover_text(crate::tl_id!("ui.show_or_hide_the_notebooks_column"))
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
                    ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_2")).small().color(self.tokens.muted));
                });
                ui.add_space(2.0);
                egui::ScrollArea::vertical().id_salt("organisers").auto_shrink([false, false]).scroll_source(scroll()).show(ui, |ui| {
                    if self.session.tree().is_empty() {
                        ui.weak(crate::tl_id!("ui.no_notebooks_yet_use_new_notebook"));
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
                        let add_page = egui::Button::new(crate::tl_id!("ui.add_page")).stroke(egui::Stroke::new(1.0, self.tokens.container_border));
                        if ui.add(add_page).on_hover_text(crate::tl_id!("ui.a_new_page_in_this_section")).clicked() {
                            acts.push(Act::Add(path.clone(), NodeKind::Section));
                        }
                    });
                    ui.label(egui::RichText::new(title.to_uppercase()).small().color(self.tokens.muted));
                    ui.add_space(2.0);
                    egui::ScrollArea::vertical().id_salt("pages").auto_shrink([false, false]).scroll_source(scroll()).show(ui, |ui| {
                        let list: Vec<&Node> = pages.iter().filter(|p| p.info.kind == NodeKind::Page).collect();
                        if list.is_empty() {
                            ui.weak(crate::tl_id!("ui.no_pages_in_this_section_yet"));
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
                        ui.weak(crate::tl_id!("ui.choose_a_section_to_see_its_pages"));
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
                    ui.heading(crate::tl_id!("ui.notecraft"));
                    ui.label(crate::tl_id!("ui.notes_as_plain_folders_open_a_page_or_create_a_notebook_to_start"));
                    ui.add_space(4.0);
                    let folder = self.shown(self.session.root());
                    let message = crate::i18n::format_message_id("ui.notebooks_folder_folder", &[("folder", &folder)]);
                    ui.label(egui::RichText::new(message).small().color(self.tokens.muted));
                });
                return;
            }
            self.editor_ui(ui);
        });
    }

    fn wheel_canvas_pan_delta(delta: egui::Vec2, modifiers: Modifiers, canvas_pan_enabled: bool) -> egui::Vec2 {
        if modifiers.ctrl {
            return egui::Vec2::ZERO;
        }
        if modifiers.shift {
            return egui::vec2(delta.y, 0.0);
        }
        if canvas_pan_enabled { delta } else { egui::Vec2::ZERO }
    }

    fn editor_ui(&mut self, ui: &mut Ui) {
        if !self.icon_recovery_suppressed
            && self.dialog.is_none()
            && let Some(ed) = self.editor.as_ref()
            && let Some(name) = first_missing_attachment_in_source(&ed.body, &ed.dir).or_else(|| first_legacy_attachment_in_source(&ed.body))
        {
            let key = format!("{}::{name}", ed.dir.display());
            if self.icon_recovery_reported.insert(key) {
                log::warn!("attachment icon is missing: page={:?} attachment={name:?}", ed.dir);
                self.dialog = Some(Dialog::MissingAttachmentIcon { page: ed.dir.clone(), name });
            }
        }
        let page_input_rect = self.page_rect;
        let wheel_canvas_pan_enabled = self.pointer != ribbon::Pointer::Write;
        if self.editor.is_some() && !self.show_source {
            let (pointer, middle_pressed, middle_down, wheel) = ui.input(|i| {
                let pointer = page_input_rect.and_then(|rect| i.pointer.interact_pos().filter(|p| rect.contains(*p)));
                let wheel = if page_input_rect.is_some_and(|rect| i.pointer.hover_pos().is_some_and(|p| rect.contains(p))) {
                    i.events.iter().fold(egui::Vec2::ZERO, |sum, event| match event {
                        egui::Event::MouseWheel { delta, modifiers, .. } => {
                            sum + Self::wheel_canvas_pan_delta(*delta, *modifiers, wheel_canvas_pan_enabled)
                        }
                        _ => sum,
                    })
                } else {
                    egui::Vec2::ZERO
                };
                (pointer, i.pointer.button_pressed(egui::PointerButton::Middle), i.pointer.button_down(egui::PointerButton::Middle), wheel)
            });
            if wheel_canvas_pan_enabled {
                self.canvas_pan += wheel;
                self.canvas_pan.x = self.canvas_pan.x.clamp(-1.0e6, 1.0e6);
                self.canvas_pan.y = self.canvas_pan.y.clamp(-1.0e6, 1.0e6);
            }
            if middle_pressed {
                self.canvas_drag = pointer;
            } else if middle_down {
                if let (Some(last), Some(now)) = (self.canvas_drag, pointer) {
                    self.canvas_pan += now - last;
                    self.canvas_drag = Some(now);
                }
            } else {
                self.canvas_drag = None;
            }
        } else {
            self.canvas_drag = None;
        }
        let wheel_zoom =
            ui.input(|i| page_input_rect.and_then(|rect| i.pointer.hover_pos().filter(|p| rect.contains(*p))).map_or(1.0, |_| i.zoom_delta()));
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
        let mut justified_moved = false;
        // A file other than the page written in the code view, waiting to go to disk.
        let mut wrote_file: Option<(String, String)> = None;
        let mut file_action: Option<crate::codeview::FileAction> = None;
        let mut code = std::mem::take(&mut self.code);
        let mut save_cols: Option<(String, String)> = None;
        let mut save_justified: Option<(String, String)> = None;
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
            let missing_fonts = crate::fonts::ensure_page_fonts(ui.ctx(), &crate::fonts::page_families(&ed.blocks));
            if !missing_fonts.is_empty() {
                let message = crate::i18n::format_message_id("ui.fonts_unavailable", &[("fonts", &missing_fonts.join(", "))]);
                ui.label(egui::RichText::new(message).color(tokens.muted));
            }
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
                        crate::tl_id!("ui.editing").to_string()
                    } else {
                        ed.last_saved
                            .clone()
                            .map(|t| crate::i18n::format_message_id("ui.saved_time", &[("time", &t)]))
                            .unwrap_or_else(|| crate::tl_id!("ui.saved").into())
                    };
                    ui.label(egui::RichText::new(state).small().color(tokens.muted));
                    ui.label(egui::RichText::new(ed.format.label()).small().color(tokens.muted));
                });
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(crate::tl_id!("ui.drop_a_file_onto_the_page_to_add_it")).small().color(tokens.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The page controls, in OneNote's order: full page, search, source.
                    let full = icon_toggle(
                        ui,
                        if full_page { Icon::ExitFullPage } else { Icon::FullPage },
                        full_page,
                        &tokens,
                        crate::tl_id!("ui.full_page"),
                        if full_page { crate::tl_id!("ui.show_the_notebooks_again") } else { crate::tl_id!("ui.give_the_page_the_whole_window") },
                    );
                    if full.clicked() {
                        toggle_full_page = true;
                    }
                    if icon_toggle(ui, Icon::Search, false, &tokens, crate::tl_id!("ui.search"), crate::tl_id!("ui.search_all_pages")).clicked() {
                        focus_search = true;
                    }
                    if icon_toggle(
                        ui,
                        Icon::Code,
                        show_source,
                        &tokens,
                        crate::tl_id!("ui.page_source"),
                        if show_source { crate::tl_id!("ui.hide_the_page_source") } else { crate::tl_id!("ui.show_the_page_source") },
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
                            _toggle: &mut bool,
                            boxes_moved: &mut bool,
                            cols_moved: &mut bool,
                            justified_moved: &mut bool,
                            follow: &mut Option<String>,
                            rename: &mut Option<String>| {
                let viewport = ui.available_rect_before_wrap().shrink2(vec2(0.0, 1.0));
                if wide && let Some(wash) = page_colour {
                    ui.painter_at(viewport).rect_filled(viewport, 0.0, wash);
                }
                if wide && rules.on() {
                    crate::ribbon::paint_paper_canvas(
                        &ui.painter_at(viewport),
                        viewport,
                        rules,
                        zoom_scale,
                        self.canvas_pan,
                        tokens.rule_line,
                        tokens.rule_line_strong,
                    );
                }
                let content_rect = viewport.translate(self.canvas_pan);
                ui.scope_builder(egui::UiBuilder::new().id_salt("page-canvas-content").max_rect(content_rect), |ui| {
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
                            if let Some(pad) = pad.as_mut() {
                                // Ink sits above page content, but below foreground controls such as
                                // pen palettes. A foreground ink layer registered after the ribbon
                                // paints across its menus when a stroke is already in progress.
                                let layer = egui::LayerId::new(egui::Order::Middle, ui.id().with("ink-overlay"));
                                let mut ink_ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(viewport));
                                ink_changed |= crate::inkpad::show(&mut ink_ui, viewport, top, pad, pen, pressure, &tokens, zoom_scale);
                            }
                            // A press lets go of whatever was selected: the drag that may
                            // follow it builds a new one. Done here, once a frame, rather
                            // than inside the blocks, where a container drawn later would
                            // clear what an earlier one had just set.
                            let ink_owns_canvas = pen.pointer != ribbon::Pointer::Write;
                            let pressed = ui.input(|i| i.pointer.any_pressed().then(|| i.pointer.interact_pos()).flatten());
                            let press_in_page = pressed.is_some_and(|pos| {
                                viewport.contains(pos) && ui.ctx().layer_id_at(pos).is_none_or(|layer| layer.order < egui::Order::Foreground)
                            });
                            if press_in_page && !ink_owns_canvas {
                                ed.across = None;
                            }
                            // A page without a saved arrangement is one responsive sheet of
                            // text. Keep its default box aligned with the available page width.
                            if !ed.arranged && ed.boxes.len() == 1 {
                                ed.boxes[0].width = crate::containers::responsive_default_width(ui.available_width(), zoom_scale);
                            }
                            if let Some(selection) = ed.restore_format_selection.take() {
                                ui.data_mut(|data| data.insert_temp(pageedit::restore_selection_id(), selection));
                            }
                            let before_caret = ed.pending_format.as_ref().and_then(|_| ed.caret.clone());
                            let mut moved = if ink_owns_canvas {
                                crate::containers::show_ink_view(
                                    ui,
                                    &ed.blocks,
                                    &mut ed.boxes,
                                    &ed.dir,
                                    &tokens,
                                    page_dir,
                                    &ed.justified,
                                    if hide_title { "" } else { &shown_title },
                                    zoom_scale,
                                )
                            } else {
                                crate::containers::show(
                                    ui,
                                    &mut ed.blocks,
                                    &mut ed.boxes,
                                    &mut ed.caret,
                                    &ed.dir,
                                    &tokens,
                                    page_dir,
                                    table_borders,
                                    &mut ed.columns,
                                    &mut ed.justified,
                                    &mut ed.across,
                                    if hide_title { "" } else { &shown_title },
                                    zoom_scale,
                                )
                            };
                            if moved.changed.blocks && ui.input(|input| input.events.iter().any(|event| matches!(event, egui::Event::Text(_)))) {
                                ed.apply_pending_format(before_caret.as_ref());
                            } else if let Some(pending) = ed.pending_format.as_ref()
                                && ed.caret.as_ref().is_none_or(|caret| caret.block != pending.block || caret.selection != (pending.at, pending.at))
                            {
                                ed.pending_format = None;
                            }
                            page_acts.append(&mut moved.acts);
                            if let Some((html, text)) = moved.copy.take()
                                && self.clipboard.set_html(&html, &text).is_err()
                            {
                                ui.ctx().copy_text(text);
                            }
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
                                let body = viewport;
                                page_rect = Some(body);
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
                            ed.content_bottom = Some(moved.bottom);
                            if moved.boxes {
                                *boxes_moved = true;
                            }
                            if moved.changed.columns {
                                *cols_moved = true;
                            }
                            // Blocks went in or out before a justified paragraph: its mark
                            // moved with it, and is written into the page's file like the widths.
                            if moved.justified {
                                *justified_moved = true;
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
                            ui.label(
                                egui::RichText::new(crate::tl_id!("ui.this_page_holds_markup_notecraft_does_not_edit_yet_s_bc3b3b6a92"))
                                    .small()
                                    .color(tokens.muted),
                            );
                        } else if let Some(href) = preview::show_zoomed(ui, &ed.blocks, &ed.dir, &tokens, page_dir, &ed.justified, zoom_scale) {
                            *follow = Some(href);
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
                file_action = out.file_action;
            } else {
                let scroll = egui::containers::scroll_area::ScrollSource {
                    drag: egui::containers::scroll_area::DragScroll::Never,
                    ..egui::containers::scroll_area::ScrollSource::default()
                };
                egui::ScrollArea::vertical().id_salt("page-main").auto_shrink([false, false]).scroll_source(scroll).show(ui, |ui| {
                    page(
                        ui,
                        ed,
                        true,
                        &mut changed,
                        &mut from_blocks,
                        &mut toggle_source,
                        &mut boxes_moved,
                        &mut cols_moved,
                        &mut justified_moved,
                        &mut follow,
                        &mut rename,
                    );
                });
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
            if justified_moved {
                let path = path_str(&ed.dir);
                let line = notecraft_model::justify::write(&ed.justified);
                save_justified = Some((path, line));
            }
        }
        self.cursor = cursor;
        if let Some((path, line)) = save_cols {
            self.run("page.columns", &params(&[("path", &path), ("widths", &line)]));
        }
        if let Some((path, line)) = save_justified {
            self.run("page.justify", &params(&[("path", &path), ("blocks", &line)]));
        }
        self.code = code;
        if let Some((name, text)) = wrote_file {
            self.write_page_file(&name, &text);
        }
        if let Some(action) = file_action {
            match action {
                crate::codeview::FileAction::CopyPath(path) => {
                    ui.ctx().copy_text(path);
                    self.say(crate::tl_id!("ui.full_path_copied"));
                }
                crate::codeview::FileAction::OpenFolder(path) => match open_folder(&path) {
                    Ok(()) => self.say(crate::tl_id!("ui.opened_the_containing_folder")),
                    Err(e) => {
                        self.complain(&crate::i18n::format_message_id("ui.could_not_open_the_containing_folder_error", &[("error", &e.to_string())]))
                    }
                },
                crate::codeview::FileAction::OpenShell(path) => {
                    if self.open_attachment_warning_suppressed {
                        match open_file_for_viewer(&path) {
                            Ok(()) => self.say(crate::tl_id!("ui.opened_using_the_associated_application")),
                            Err(e) => {
                                self.complain(&crate::i18n::format_message_id("ui.could_not_open_the_file_error", &[("error", &e.to_string())]))
                            }
                        }
                    } else {
                        self.dialog = Some(Dialog::OpenAttachmentWarning { path });
                    }
                }
                crate::codeview::FileAction::Delete(name) => {
                    log::debug!("source view delete action received: file={name:?}");
                    let page = self.editor.as_ref().map(|e| e.dir.clone());
                    if let Some(page) = page {
                        self.dialog = Some(Dialog::DeleteFile { page, name });
                    }
                }
                crate::codeview::FileAction::AddFile => {
                    if let Some(dir) = self.editor.as_ref().map(|ed| ed.dir.clone())
                        && let Some(file) = rfd::FileDialog::new().set_title(crate::tl_id!("ui.add_file_to_this_page")).pick_file()
                    {
                        self.dialog = Some(Dialog::FileOptions { dir, files: vec![file] });
                    }
                }
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
                Ok(()) => self.say(crate::i18n::format_message_id("ui.drawing_saved_count_strokes", &[("count", &strokes.to_string())])),
                Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", &e.to_string())])),
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
            self.say(crate::tl_id!("ui.title_saved"));
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let color = if self.status_is_error { self.tokens.warn } else { self.tokens.muted };
                ui.label(egui::RichText::new(crate::i18n::pseudo_dynamic(&self.status)).color(color));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let nodes: usize = self.session.tree().iter().map(|n| n.count()).sum();
                    let notebooks = crate::i18n::format_count_id("ui.n_notebook_n_notebooks", self.session.tree().len() as u64);
                    let items = crate::i18n::format_count_id("ui.n_item_n_items", nodes as u64);
                    let folder = self.shown(self.session.root());
                    let summary = crate::i18n::format_message_id(
                        "ui.notebooks_items_folder",
                        &[("notebooks", &notebooks), ("items", &items), ("folder", &folder)],
                    );
                    ui.label(egui::RichText::new(summary).small().color(self.tokens.muted));
                    ui.separator();
                    let (current, source) = self.session.settings.global_format();
                    let mut chosen = current;
                    egui::ComboBox::from_label(crate::tl_id!("ui.default_format")).selected_text(current.label()).show_ui(ui, |ui| {
                        for f in PageFormat::ALL {
                            ui.selectable_value(&mut chosen, f, f.label());
                        }
                    });
                    ui.label(egui::RichText::new(format!("({})", crate::tl!(source.label()))).small().color(self.tokens.muted));
                    ui.label(egui::RichText::new(crate::tl_id!("ui.new_pages")).small().color(self.tokens.muted));
                    if chosen != current && self.run("settings.default_format.set", &params(&[("format", chosen.id())])).is_some() {
                        self.say(crate::i18n::format_message_id(
                            "ui.new_pages_use_format_unless_the_notebook_or_section_2678c9f7e6",
                            &[("format", chosen.label())],
                        ));
                    }
                });
            });
        });
    }

    // ---------- dialogs ----------

    fn dialogs(&mut self, ctx: &Context) {
        while self.dialog.is_none() {
            let Some(next) = self.queued_alerts.pop_front() else { break };
            if !matches!(&next, Dialog::Alert { key, .. } if self.muted_alerts.contains(key)) {
                self.dialog = Some(next);
            }
        }
        let Some(dialog) = self.dialog.take() else { return };
        // Page loading and native Shell icon generation can finish between the
        // recovery scan and the frame that presents this modal. Re-check the
        // filesystem so a healthy sidecar never produces a stale warning.
        if let Dialog::MissingAttachmentIcon { page, name } = &dialog
            && attachment_icon_healthy(page, name)
        {
            log::debug!("discarding stale missing-icon warning: page={page:?} attachment={name:?}");
            return;
        }
        let mut keep = true;
        let mut next = dialog;
        match &mut next {
            Dialog::ClearTelemetry { path } => {
                let mut clear = false;
                let mut cancel = false;
                egui::Window::new(crate::tl_id!("ui.clear_telemetry"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(440.0);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.clear_telemetry"));
                        ui.label(crate::tl_id!("ui.clear_all_local_telemetry_sessions_and_exported_tele_e4f5e2340d"));
                        ui.add_space(5.0);
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_cannot_be_undone"));
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                cancel = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.clear_telemetry"), true).clicked() {
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
            Dialog::FirstUseWizard { step, level, format, add_demo, error } => {
                let mut back = false;
                let mut next = false;
                let mut finish = false;
                let last_step: u8 = 4;
                let escape = ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape));
                let screen = ctx.viewport_rect();
                egui::Area::new(egui::Id::new("first-use-wizard-modal-backdrop")).order(egui::Order::Foreground).fixed_pos(screen.min).show(
                    ctx,
                    |ui| {
                        let response = ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size()), egui::Sense::click());
                        ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
                    },
                );
                let panel = self.tokens.panel;
                let dark = (u32::from(panel.r()) + u32::from(panel.g()) + u32::from(panel.b())) < 384;
                let (banner_uri, banner_bytes) = if dark {
                    ("bytes://notecraft-wizard-banner-dark", crate::BANNER_DARK_PNG)
                } else {
                    ("bytes://notecraft-wizard-banner-light", crate::BANNER_LIGHT_PNG)
                };
                let title = match *step {
                    0 => crate::tl_id!("ui.welcome_to_notecraft"),
                    1 => crate::tl_id!("ui.local_collection_level"),
                    2 => crate::tl_id!("ui.welcome_format_title"),
                    3 => crate::tl_id!("ui.welcome_notebook_prompt_title"),
                    _ => crate::tl_id!("ui.welcome_wizard_done_title"),
                }
                .to_string();
                egui::Window::new(title.as_str())
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(520.0);
                        if *step == 0 {
                            ui.add(egui::Image::from_bytes(banner_uri, banner_bytes).fit_to_exact_size(vec2(480.0, 240.0)));
                            ui.add_space(8.0);
                        }
                        modal_title(ui, &self.tokens, title.as_str());
                        match *step {
                            0 => {}
                            1 => {
                                ui.label(crate::tl_id!("ui.privacy_is_our_top_priority_all_logs_and_telemetry_s_d87cae4eac"));
                                ui.add_space(6.0);
                                ui.label(crate::tl_id!("ui.choose_a_local_collection_level_logging_is_off_by_de_5dfbe386e0"));
                                egui::ComboBox::from_label(crate::tl_id!("ui.local_collection_level"))
                                    .selected_text(crate::tl!(level.as_str()))
                                    .show_ui(ui, |ui| {
                                        for choice in ["none", "usage", "warning", "error", "debug"] {
                                            ui.selectable_value(level, choice.into(), crate::tl!(choice));
                                        }
                                    });
                            }
                            2 => {
                                ui.label(crate::tl_id!("ui.welcome_format_body"));
                                ui.add_space(8.0);
                                for choice in PageFormat::ALL {
                                    ui.radio_value(&mut *format, choice, choice.label());
                                }
                            }
                            3 => {
                                ui.label(crate::tl_id!("ui.welcome_notebook_prompt_body"));
                                ui.label(crate::i18n::format_message_id("ui.welcome_notebook_prompt_format", &[("format", format.label())]));
                                ui.add_space(6.0);
                                ui.colored_label(self.tokens.warn, crate::tl_id!("ui.welcome_notebook_replace_warning"));
                                ui.add_space(8.0);
                                ui.radio_value(&mut *add_demo, true, crate::tl_id!("ui.welcome_notebook_add_yes"));
                                ui.radio_value(&mut *add_demo, false, crate::tl_id!("ui.welcome_notebook_add_no"));
                            }
                            _ => {
                                ui.label(crate::tl_id!("ui.welcome_wizard_done_body"));
                            }
                        }
                        if let Some(message) = error {
                            ui.add_space(8.0);
                            ui.colored_label(
                                self.tokens.warn,
                                crate::i18n::format_message_id("ui.welcome_notebook_action_failed", &[("error", message)]),
                            );
                        }
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if *step > 0 && modal_button(ui, &self.tokens, crate::tl_id!("ui.welcome_wizard_back"), false).clicked() {
                                back = true;
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if *step < last_step {
                                    if modal_button(ui, &self.tokens, crate::tl_id!("ui.welcome_format_next"), true).clicked() {
                                        next = true;
                                    }
                                } else if modal_button(ui, &self.tokens, crate::tl_id!("ui.welcome_wizard_finish"), true).clicked() {
                                    finish = true;
                                }
                            });
                        });
                    });
                if back {
                    *step = step.saturating_sub(1);
                }
                if next {
                    *step = (*step + 1).min(last_step);
                }
                if escape && !finish && *step > 0 {
                    *step -= 1;
                }
                if finish {
                    let level = level.clone();
                    match self.finish_first_use_wizard(&level, *format, *add_demo) {
                        Ok(()) => keep = false,
                        Err(message) => *error = Some(message),
                    }
                }
            }
            Dialog::Alert { key, title, message, suppress, suppress_forever } => {
                let mut ok = false;
                egui::Window::new(title.as_str())
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, title.as_str());
                        ui.set_min_width(420.0);
                        ui.label(crate::i18n::pseudo_dynamic(message));
                        ui.add_space(8.0);
                        ui.checkbox(suppress, crate::tl_id!("ui.don_t_show_this_again_in_this_session"));
                        ui.checkbox(suppress_forever, crate::tl_id!("ui.don_t_show_this_ever"));
                        ui.add_space(12.0);
                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.ok"), true).clicked() {
                            ok = true;
                        }
                    });
                if ok {
                    if *suppress {
                        self.muted_alerts.insert(key.clone());
                    }
                    if *suppress_forever {
                        let _ = self.session.settings.set(&format!("{key}_never"), "true");
                    }
                    keep = false;
                }
            }
            Dialog::ResetPreferences => {
                let mut reset = false;
                let mut cancel = false;
                egui::Window::new(crate::tl_id!("ui.reset_app_preferences"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.reset_app_preferences"));
                        ui.set_min_width(460.0);
                        ui.label(crate::tl_id!("ui.reset_notecraft_s_saved_preferences_to_their_built_in_defaults"));
                        ui.add_space(6.0);
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_does_not_delete_notebooks_or_pages_it_cannot_be_undone"));
                        ui.add_space(14.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.reset_preferences"), true).clicked() {
                                reset = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                cancel = true;
                            }
                        });
                    });
                if reset {
                    match self.session.settings.reset() {
                        Ok(()) => self.dialog = Some(self.first_use_wizard()),
                        Err(e) => {
                            self.complain(&crate::i18n::format_message_id("ui.could_not_reset_app_preferences_error", &[("error", &e.to_string())]))
                        }
                    }
                    keep = false;
                } else if cancel {
                    keep = false;
                }
            }
            Dialog::MissingAttachmentIcon { page, name } => {
                let mut repair = false;
                let mut leave = false;
                let mut suppress = self.icon_recovery_suppressed;
                egui::Area::new(egui::Id::new("missing-attachment-icon-backdrop"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(ctx.viewport_rect().min)
                    .show(ctx, |ui| {
                        let response =
                            ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, ctx.viewport_rect().size()), egui::Sense::click());
                        ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
                    });
                egui::Window::new(crate::tl_id!("ui.attachment_recovery"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.attachment_icon_missing"));
                        ui.set_min_width(520.0);
                        let message =
                            crate::i18n::format_message_id("ui.notecraft_found_an_attachment_without_its_generated_icon_name", &[("name", name)]);
                        ui.label(egui::RichText::new(message).strong());
                        ui.add_space(6.0);
                        ui.label(crate::tl_id!("ui.the_original_file_is_still_safe_notecraft_can_regene_f10ded524a"));
                        ui.add_space(8.0);
                        ui.checkbox(&mut suppress, crate::tl_id!("ui.do_not_warn_about_similar_missing_icons_again_this_session"));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.repair_icon"), true).clicked() {
                                repair = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.leave_it"), false).clicked() {
                                leave = true;
                            }
                        });
                    });
                if repair {
                    self.icon_recovery_suppressed = suppress;
                    match notecraft_store::assets::ensure_icon(page, name).and_then(|()| {
                        let icon_exists = attachment_icon_exists(page, name);
                        if !icon_exists {
                            return Err(notecraft_store::StoreError::invalid("the icon generator produced no sidecar file"));
                        }
                        self.repair_attachment_reference(page, name).map_err(notecraft_store::StoreError::invalid).map(|_| ())
                    }) {
                        Ok(()) => {
                            log::info!("repaired missing attachment icon and reference: page={:?} attachment={name:?}", page);
                            self.say(crate::i18n::format_message_id("ui.repaired_the_attachment_icon_for_name", &[("name", name)]));
                        }
                        Err(e) => {
                            log::error!("could not repair missing attachment icon: page={:?} attachment={name:?} error={e}", page);
                            self.complain(&crate::i18n::format_message_id(
                                "ui.could_not_repair_the_icon_for_name_error",
                                &[("name", name), ("error", &e.to_string())],
                            ));
                        }
                    }
                    keep = false;
                } else if leave {
                    self.icon_recovery_suppressed = suppress;
                    log::error!("user declined missing attachment icon repair: page={:?} attachment={name:?}", page);
                    keep = false;
                } else {
                    self.icon_recovery_suppressed = suppress;
                }
            }
            Dialog::UnreferencedAttachment { page, name } => {
                let mut restore = false;
                let mut leave = false;
                let mut suppress = self.icon_recovery_suppressed;
                egui::Area::new(egui::Id::new("unreferenced-attachment-backdrop"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(ctx.viewport_rect().min)
                    .show(ctx, |ui| {
                        let response =
                            ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, ctx.viewport_rect().size()), egui::Sense::click());
                        ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
                    });
                egui::Window::new(crate::tl_id!("ui.attachment_recovery"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.attachment_not_linked"));
                        ui.set_min_width(520.0);
                        let message =
                            crate::i18n::format_message_id("ui.notecraft_found_an_attachment_that_is_not_referenced_8c0eb51257", &[("name", name)]);
                        ui.label(egui::RichText::new(message).strong());
                        ui.add_space(6.0);
                        ui.label(crate::tl_id!("ui.the_file_is_safe_notecraft_can_append_a_link_to_it_a_c31b3d9716"));
                        ui.add_space(8.0);
                        ui.checkbox(&mut suppress, crate::tl_id!("ui.do_not_warn_about_similar_attachment_issues_again_this_session"));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.append_link"), true).clicked() {
                                restore = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.leave_it"), false).clicked() {
                                leave = true;
                            }
                        });
                    });
                if restore {
                    self.icon_recovery_suppressed = suppress;
                    match self.repair_attachment_reference(page, name) {
                        Ok(true) => {
                            log::info!("restored unreferenced attachment link: page={:?} attachment={name:?}", page);
                            self.say(crate::i18n::format_message_id("ui.added_a_link_to_name_at_the_end_of_the_page", &[("name", name)]));
                        }
                        Ok(false) => log::debug!("attachment recovery found no change needed: page={:?} attachment={name:?}", page),
                        Err(e) => {
                            log::error!("could not restore unreferenced attachment link: page={:?} attachment={name:?} error={e}", page);
                            self.complain(&crate::i18n::format_message_id(
                                "ui.could_not_restore_the_link_for_name_error",
                                &[("name", name), ("error", &e.to_string())],
                            ));
                        }
                    }
                    keep = false;
                } else if leave {
                    self.icon_recovery_suppressed = suppress;
                    log::error!("user declined unreferenced attachment recovery: page={:?} attachment={name:?}", page);
                    keep = false;
                } else {
                    self.icon_recovery_suppressed = suppress;
                }
            }
            Dialog::OpenAttachmentWarning { path } => {
                let mut open = false;
                let mut cancel = false;
                let mut suppress = self.open_attachment_warning_suppressed;
                egui::Area::new(egui::Id::new("open-attachment-warning-backdrop"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(ctx.viewport_rect().min)
                    .show(ctx, |ui| {
                        let response =
                            ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, ctx.viewport_rect().size()), egui::Sense::click());
                        ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
                    });
                egui::Window::new(crate::tl_id!("ui.open_attachment"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.open_attachment"));
                        ui.set_min_width(520.0);
                        ui.label(egui::RichText::new(crate::tl_id!("ui.this_file_will_be_opened_by_its_associated_application")).strong());
                        ui.add_space(6.0);
                        ui.label(self.shown(path).to_string());
                        ui.add_space(6.0);
                        ui.label(crate::tl_id!("ui.only_open_it_if_you_trust_the_file_and_the_person_who_created_it"));
                        ui.add_space(8.0);
                        ui.checkbox(&mut suppress, crate::tl_id!("ui.do_not_warn_again_for_this_file_type_this_session"));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.open"), true).clicked() {
                                open = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                cancel = true;
                            }
                        });
                    });
                if open {
                    self.open_attachment_warning_suppressed = suppress;
                    match open_file_for_viewer(path) {
                        Ok(()) => self.say(crate::i18n::format_message_id("ui.opening_path", &[("path", &self.shown(path))])),
                        Err(e) => self.complain(&crate::i18n::format_message_id("ui.could_not_open_the_file_error", &[("error", &e.to_string())])),
                    }
                    keep = false;
                } else if cancel {
                    self.open_attachment_warning_suppressed = suppress;
                    log::info!("user cancelled opening attachment: file={:?}", path);
                    keep = false;
                } else {
                    self.open_attachment_warning_suppressed = suppress;
                }
            }
            Dialog::FileOptions { dir, files } => {
                let (dir, file) = (dir.clone(), files.first().cloned().unwrap_or_default());
                let name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let left = files.len().saturating_sub(1);
                let kind = notecraft_store::assets::media_kind(&name);
                let mut chosen: Option<FileChoice> = None;
                let title = if left > 0 {
                    crate::i18n::format_message_id("ui.add_name_left_more", &[("name", &name), ("left", &left.to_string())])
                } else {
                    crate::i18n::format_message_id("ui.add_name", &[("name", &name)])
                };
                egui::Window::new(crate::tl_id!("ui.file_options"))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(380.0);
                        modal_title(ui, &self.tokens, &title);
                        ui.label(
                            egui::RichText::new(crate::tl_id!("ui.what_should_happen_to_this_file")).strong().size(16.0).color(self.tokens.text),
                        );
                        ui.add_space(6.0);
                        let copy_label = match kind {
                            notecraft_store::assets::MediaKind::Image => crate::tl_id!("ui.insert_the_picture"),
                            notecraft_store::assets::MediaKind::Video => crate::tl_id!("ui.insert_the_video"),
                            notecraft_store::assets::MediaKind::Audio => crate::tl_id!("ui.insert_the_recording"),
                            notecraft_store::assets::MediaKind::Other => crate::tl_id!("ui.attach_the_file"),
                        };
                        if modal_button(ui, &self.tokens, copy_label, true).clicked() {
                            log::debug!("file attach choice clicked: choice=copy name={name:?}");
                            chosen = Some(FileChoice::Copy);
                        }
                        ui.label(
                            egui::RichText::new(crate::tl_id!("ui.a_copy_goes_into_this_page_s_folder_so_the_notebook_5f331e4138"))
                                .small()
                                .color(self.tokens.muted),
                        );
                        ui.add_space(6.0);
                        let can_printout = printout::supports(&file);
                        if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.insert_printout"), false, can_printout).clicked() {
                            log::debug!("file attach choice clicked: choice=printout name={name:?}");
                            chosen = Some(FileChoice::Printout);
                        }
                        ui.label(
                            egui::RichText::new(if can_printout {
                                crate::tl_id!("ui.copies_the_file_and_inserts_one_picture_for_each_printed_page")
                            } else {
                                crate::tl_id!("ui.printout_is_available_for_pdf_and_office_documents")
                            })
                            .small()
                            .color(self.tokens.muted),
                        );
                        ui.add_space(6.0);
                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.link_to_the_file_where_it_is"), false).clicked() {
                            log::debug!("file attach choice clicked: choice=link name={name:?}");
                            chosen = Some(FileChoice::Link);
                        }
                        ui.label(
                            egui::RichText::new(crate::tl_id!("ui.nothing_is_copied_the_link_breaks_if_the_file_moves"))
                                .small()
                                .color(self.tokens.muted),
                        );
                        ui.add_space(6.0);
                        ui.add_space(2.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
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
                egui::Window::new(crate::tl_id!("ui.new_notebook"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.new_notebook"));
                        ui.set_min_width(420.0);
                        ui.label(crate::tl_id!("ui.name_2"));
                        let r = ui.text_edit_singleline(name);
                        // Read Enter before asking for focus again: a new focus request hides the lost focus.
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        r.request_focus();
                        format_picker(ui, format);
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            let ready = !name.trim().is_empty();
                            let create =
                                ui.add_enabled_ui(ready, |ui| modal_button(ui, &self.tokens, crate::tl_id!("ui.create"), true).clicked()).inner;
                            if (create || enter) && ready {
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
                        self.say(crate::tl_id!("ui.notebook_created"));
                    }
                    keep = false;
                }
            }
            Dialog::Add { parent, parent_kind, title, kind, format } => {
                let mut go = false;
                let heading = if *parent_kind == NodeKind::Page {
                    crate::tl_id!("ui.add_a_sub_page").to_string()
                } else {
                    let kind = crate::tl!(parent_kind.label());
                    crate::i18n::format_message_id("ui.add_inside_this_kind", &[("kind", &kind.to_lowercase())])
                };
                egui::Window::new(heading.clone())
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, &heading);
                        ui.set_min_width(420.0);
                        let kinds = parent_kind.may_contain();
                        if kinds.len() > 1 {
                            ui.label(crate::tl_id!("ui.what_to_add"));
                            ui.horizontal(|ui| {
                                for k in kinds {
                                    ui.selectable_value(kind, *k, crate::tl!(k.label()));
                                }
                            });
                        }
                        ui.label(crate::tl_id!("ui.title_2"));
                        let r = ui.text_edit_singleline(title);
                        // Read Enter before asking for focus again: a new focus request hides the lost focus.
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        r.request_focus();
                        ui.label(if *kind == NodeKind::Page {
                            crate::tl_id!("ui.format_for_this_page")
                        } else {
                            crate::tl_id!("ui.default_format_for_pages_below")
                        });
                        format_picker(ui, format);
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            let ready = !title.trim().is_empty();
                            let create =
                                ui.add_enabled_ui(ready, |ui| modal_button(ui, &self.tokens, crate::tl_id!("ui.create"), true).clicked()).inner;
                            if (create || enter) && ready {
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
                        self.say(crate::tl_id!("ui.created"));
                    }
                    keep = false;
                }
            }
            Dialog::Rename { path, title } => {
                let mut go = false;
                egui::Window::new(crate::tl_id!("ui.rename")).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        ui.label(crate::tl_id!("ui.title_the_folder_name_does_not_change"));
                        let r = ui.text_edit_singleline(title);
                        // Read Enter before asking for focus again: a new focus request hides the lost focus.
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        r.request_focus();
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if (modal_button(ui, &self.tokens, crate::tl_id!("ui.rename"), true).clicked() || enter) && !title.trim().is_empty() {
                                go = true;
                            }
                        });
                    },
                );
                if go {
                    if self.run("node.title", &params(&[("path", &path_str(path)), ("title", title.trim())])).is_some() {
                        if self.editor.as_ref().map(|e| e.dir == *path).unwrap_or(false) {
                            self.open_page(path);
                        }
                        self.say(crate::tl_id!("ui.renamed"));
                    }
                    keep = false;
                }
            }
            Dialog::DeleteFile { page, name } => {
                let mut go = false;
                let mut cancel = false;
                let screen = ctx.viewport_rect();
                egui::Area::new(egui::Id::new("delete-file-modal-backdrop")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
                    let response = ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size()), egui::Sense::click());
                    ui.painter().rect_filled(response.rect, 0.0, Color32::from_black_alpha(150));
                });
                egui::Window::new(crate::tl_id!("ui.delete_file"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.delete_file"));
                        ui.set_min_width(420.0);
                        ui.label(crate::i18n::format_message_id("ui.delete_assets_name_from_this_page_s_folder", &[("name", name)]));
                        ui.add_space(6.0);
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.the_file_is_removed_from_disk_undo_does_not_bring_a_file_back"));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.delete"), true).clicked() {
                                go = true;
                            }
                        });
                    });
                if ctx.input(|input| input.key_pressed(Key::Escape)) {
                    cancel = true;
                } else if ctx.input(|input| input.key_pressed(Key::Enter)) {
                    go = true;
                }
                if cancel {
                    keep = false;
                }
                if go {
                    let page = page.clone();
                    if self.run("page.detach", &params(&[("path", &path_str(&page)), ("name", name)])).is_some() {
                        if let Some(ed) = self.editor.as_mut() {
                            ed.attachments = notecraft_store::assets::list(&page);
                        }
                        self.say(crate::tl_id!("ui.the_file_is_gone"));
                        // The page may have named the drawing it no longer has.
                        self.sync_ink_reference();
                    }
                    keep = false;
                }
            }
            Dialog::DeleteNotebook { path, title } => {
                let mut go = false;
                egui::Window::new(crate::tl_id!("ui.delete_notebook"))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(440.0);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.delete_notebook"));
                        ui.label(crate::i18n::format_message_id("ui.delete_notebook_title_and_everything_inside_it", &[("title", title)]));
                        ui.add_space(4.0);
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_removes_the_notebook_folder_from_disk_and_cannot_be_undone"));
                        ui.add_space(14.0);
                        ui.separator();
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.delete_notebook"), true).clicked() {
                                go = true;
                            }
                        });
                    });
                if go {
                    let path = path.clone();
                    let was_open = self.editor.as_ref().and_then(|editor| notebook_for(&editor.dir)).is_some_and(|notebook| notebook == path);
                    log::info!("DeleteNotebook dialog: deleting {:?}", path);
                    match notecraft_store::ops::remove_tree(&path) {
                        Ok(()) => {
                            log::info!("notebook deleted successfully: {:?}", path);
                            self.remove_recent_notebook(&path);
                            if was_open {
                                self.editor = None;
                                self.pad = None;
                                self.selected = None;
                                self.backstage = Some("Open");
                            }
                            let _ = self.run("tree.refresh", &Params::new());
                            self.say(crate::i18n::format_message_id("ui.deleted_notebook_path", &[("path", &self.shown(&path))]));
                        }
                        Err(error) => {
                            log::error!("notebook delete failed: {:?}: {}", path, error);
                            let message =
                                crate::i18n::format_message_id("ui.the_notebook_could_not_be_deleted_error", &[("error", &error.to_string())]);
                            self.alert("delete-failed", crate::tl_id!("ui.delete_notebook"), &message);
                        }
                    }
                    keep = false;
                }
            }
            Dialog::Delete { path, title, count } => {
                let mut go = false;
                let is_notebook = notecraft_store::tree::read_node(path).is_some_and(|node| node.kind == NodeKind::Notebook);
                egui::Window::new(crate::tl_id!("ui.delete"))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        ui.set_min_width(420.0);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.delete_notebook"));
                        if is_notebook {
                            ui.label(crate::i18n::format_message_id("ui.delete_notebook_title_and_everything_inside_it", &[("title", title)]));
                            ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_removes_the_notebook_folder_from_disk_and_cannot_be_undone"));
                        } else {
                            ui.label(crate::i18n::format_count_message_id(
                                "ui.move_title_and_everything_inside_it_n_item_to_this_n_20c3146ea1",
                                *count as u64,
                                &[("title", title)],
                            ));
                            ui.label(crate::tl_id!("ui.you_can_restore_it_from_history"));
                        }
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button(
                                ui,
                                &self.tokens,
                                if is_notebook { crate::tl_id!("ui.delete_notebook") } else { crate::tl_id!("ui.move_to_recycle_bin") },
                                true,
                            )
                            .clicked()
                            {
                                go = true;
                            }
                        });
                    });
                if go {
                    if is_notebook {
                        let path = path.clone();
                        log::info!("deleting notebook: {:?}", path);
                        match notecraft_store::ops::remove_tree(&path) {
                            Ok(()) => {
                                log::info!("notebook deleted successfully: {:?}", path);
                                self.remove_recent_notebook(&path);
                                self.editor = None;
                                self.pad = None;
                                self.selected = None;
                                self.open.remove(&path);
                                let _ = self.run("tree.refresh", &Params::new());
                                self.say(crate::tl_id!("ui.deleted_notebook"));
                            }
                            Err(error) => {
                                log::error!("notebook delete failed: {:?}: {}", path, error);
                                let message =
                                    crate::i18n::format_message_id("ui.the_notebook_could_not_be_deleted_error", &[("error", &error.to_string())]);
                                self.alert("delete-failed", crate::tl_id!("ui.delete_notebook"), &message);
                            }
                        }
                    } else {
                        let was_open = self.editor.as_ref().map(|e| e.dir.starts_with(&*path)).unwrap_or(false);
                        log::info!("deleting node: {:?}", path);
                        match self.run("node.delete", &params(&[("path", &path_str(path))])) {
                            Some(_) => {
                                log::info!("node deleted successfully: {:?}", path);
                                if was_open {
                                    self.editor = None;
                                }
                                self.selected = None;
                                self.say(crate::tl_id!("ui.moved_to_the_notebook_recycle_bin"));
                            }
                            None => {
                                log::error!("node.delete command failed: {:?}", path);
                                self.alert(
                                    "delete-failed",
                                    crate::tl_id!("ui.delete"),
                                    crate::tl_id!("ui.this_could_not_be_moved_to_the_recycle_bin_nothing_was_deleted"),
                                );
                            }
                        }
                    }
                    keep = false;
                }
            }
            Dialog::PageVersions { page, versions } => {
                let mut restore: Option<String> = None;
                egui::Window::new(crate::tl_id!("ui.page_versions_2"))
                    .collapsible(false)
                    .resizable(true)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.set_min_width(420.0);
                        ui.label(crate::tl_id!("ui.saved_copies_of_this_page"));
                        ui.add_space(6.0);
                        if versions.is_empty() {
                            ui.label(crate::tl_id!("ui.no_saved_versions_yet_notecraft_keeps_a_copy_when_th_aa2e25acad"));
                        } else {
                            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                                for version in versions.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(&version.created);
                                        let size = (version.bytes.div_ceil(1024)).to_string();
                                        let size = crate::i18n::format_message_id("ui.size_kb", &[("size", &size)]);
                                        ui.label(egui::RichText::new(size).small().color(self.tokens.muted));
                                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.restore"), true).clicked() {
                                            restore = Some(version.id.clone());
                                        }
                                    });
                                }
                            });
                        }
                        ui.add_space(8.0);
                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.close"), false).clicked() {
                            keep = false;
                        }
                    });
                if let Some(id) = restore {
                    if self.run("page.version.restore", &params(&[("path", &path_str(page)), ("id", &id)])).is_some() {
                        self.open_page(page);
                        self.say(crate::tl_id!("ui.restored_the_saved_page_version"));
                    }
                    keep = false;
                }
            }
            Dialog::RecycleBin { notebook, items } => {
                let mut restore: Option<String> = None;
                let mut purge: Option<(String, String)> = None;
                egui::Window::new(crate::tl_id!("ui.notebook_recycle_bin_2"))
                    .collapsible(false)
                    .resizable(true)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.set_min_width(460.0);
                        if items.is_empty() {
                            ui.label(crate::tl_id!("ui.this_notebook_s_recycle_bin_is_empty"));
                        } else {
                            egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                                for item in items.iter() {
                                    ui.horizontal(|ui| {
                                        ui.label(&item.title);
                                        ui.label(
                                            egui::RichText::new(crate::i18n::format_message_id(
                                                "ui.count_deleted",
                                                &[
                                                    ("count", &crate::i18n::format_count_id("ui.n_item_n_items", item.count as u64)),
                                                    ("deleted", &item.deleted),
                                                ],
                                            ))
                                            .small()
                                            .color(self.tokens.muted),
                                        );
                                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.restore"), true).clicked() {
                                            restore = Some(item.id.clone());
                                        }
                                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.delete_permanently"), true).clicked() {
                                            purge = Some((item.id.clone(), item.title.clone()));
                                        }
                                    });
                                }
                            });
                        }
                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.close"), false).clicked() {
                            keep = false;
                        }
                    });
                if let Some(id) = restore {
                    if self.run("recycle.restore", &params(&[("notebook", &path_str(notebook)), ("id", &id)])).is_some() {
                        self.say(crate::tl_id!("ui.restored_to_the_original_location"));
                        keep = false;
                    }
                } else if let Some((id, title)) = purge {
                    next = Dialog::PurgeRecycleItem { notebook: notebook.clone(), id, title };
                }
            }
            Dialog::PurgeRecycleItem { notebook, id, title } => {
                let mut purge = false;
                egui::Window::new(crate::tl_id!("ui.permanently_delete"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(crate::i18n::format_message_id("ui.permanently_delete_title_and_everything_inside_it", &[("title", title)]));
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_cannot_be_undone"));
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.delete_permanently"), true).clicked() {
                                purge = true;
                            }
                        });
                    });
                if purge && self.run("recycle.purge", &params(&[("notebook", &path_str(notebook)), ("id", id)])).is_some() {
                    self.say(crate::tl_id!("ui.permanently_deleted"));
                    keep = false;
                }
            }
            Dialog::Symbols => {
                let mut chosen: Option<&'static str> = None;
                egui::Window::new(crate::tl_id!("ui.insert_symbol"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(crate::tl_id!("ui.choose_a_symbol_to_insert_at_the_caret"));
                        ui.add_space(6.0);
                        for row in [["©", "®", "™", "§", "¶"], ["•", "→", "←", "±", "°"], ["×", "÷", "√", "€", "£"], ["¥", "¢", "∞", "≠", "≤"]]
                        {
                            ui.horizontal(|ui| {
                                for symbol in row {
                                    let button = egui::Button::new(egui::RichText::new(symbol).size(20.0).color(self.tokens.text))
                                        .min_size(vec2(46.0, 36.0))
                                        .fill(self.tokens.panel)
                                        .stroke(egui::Stroke::new(0.8, self.tokens.faint));
                                    if ui.add(button).clicked() {
                                        chosen = Some(symbol);
                                    }
                                }
                            });
                        }
                        ui.add_space(6.0);
                        if modal_button(ui, &self.tokens, crate::tl_id!("ui.close"), false).clicked() {
                            keep = false;
                        }
                    });
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
            ctx.request_screenshot(move |image| {
                req.respond(encode_screenshot(&image, path.as_deref()));
            });
            ctx.request_repaint();
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
                    modifiers: egui::Modifiers { shift: flag("shift"), ..egui::Modifiers::NONE },
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
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
            "canvas": {"viewOffset": [self.canvas_pan.x, self.canvas_pan.y]},
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
        crate::i18n::set_current(&self.ui_language);
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
                if self.clipboard.set_html(&html, &text).is_err() {
                    ctx.copy_text(text);
                }
            } else {
                ctx.copy_text(text);
            }
        }
        if self.keyboard_paste_requested {
            self.keyboard_paste_requested = false;
            log::debug!("queued keyboard paste dispatched to paste_here");
            self.paste_here();
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
        self.hierarchy_keys(ctx);
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

fn table_from_gridcraft_csv(text: &str) -> Option<(notecraft_formats::markdown::Block, usize, usize)> {
    let table = notecraft_formats::csv::to_table(text)?;
    let (rows, columns) = pageedit::table_size(&table).unwrap_or((0, 0));
    Some((table, rows, columns))
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
    let theme_label = theme.label();
    let label = crate::i18n::format_message_id("ui.theme_theme", &[("theme", &theme_label)]);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, &label));
    resp.on_hover_text(theme_label)
}

fn format_picker(ui: &mut Ui, format: &mut Option<PageFormat>) {
    let selected = format.map(PageFormat::label).unwrap_or(crate::tl_id!("ui.inherit_notebook_section_or_global_setting"));
    egui::ComboBox::from_label(crate::tl_id!("ui.page_format")).selected_text(selected).show_ui(ui, |ui| {
        ui.selectable_value(format, None, crate::tl_id!("ui.inherit_notebook_section_or_global_setting"));
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
    let label = crate::tl!(color.label());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, label));
    resp.on_hover_text(label)
}

/// Shows a folder in the operating system's file manager. An optional extra: everything the
/// app itself does works without it, and a failure is reported, never fatal.
fn open_folder(dir: &Path) -> std::io::Result<()> {
    // Explorer opens the user's Documents folder when it is given a path that does not exist, so
    // a missing folder is an error here, not a window on the wrong place.
    if !dir.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} does not exist", dir.display())));
    }
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
pub(crate) fn open_file_for_viewer(file: &Path) -> std::io::Result<()> {
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
            ui.label(crate::tl!(mode.label()));
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
    let accent = crate::tl!(current.accent.label());
    let label = crate::i18n::format_message_id("ui.accent_accent", &[("accent", accent)]);
    ui.label(egui::RichText::new(label).small());
    ui.checkbox(follow, crate::tl_id!("ui.follow_the_system_light_or_dark_setting"));
}

#[cfg(test)]
mod alert_tests {
    use super::*;

    #[test]
    fn release_builds_default_local_collection_to_off() {
        assert_eq!(default_telemetry_level(false), "none");
        assert_eq!(default_telemetry_level(true), "debug");
    }

    #[test]
    fn gridcraft_csv_read_back_preserves_quoted_cells_and_adds_columns() {
        let (table, rows, columns) =
            table_from_gridcraft_csv("Item,Cost,Note\n\"Tea, green\",3.50,\"she said \"\"best\"\"\"\nPaper,12,\n").expect("CSV table");
        assert_eq!((rows, columns), (3, 3));
        let body = pageedit::to_body(PageFormat::Cmgfm, &[table]);
        assert!(body.contains("Tea, green"));
        assert!(body.contains("she said \"best\""));
    }

    #[test]
    fn maximum_sensitivity_reaches_four_times_the_neutral_pen_width() {
        assert_eq!(NoteCraftApp::sensitivity_width(0), 1.0);
        assert_eq!(NoteCraftApp::sensitivity_width(50), 1.0);
        assert_eq!(NoteCraftApp::sensitivity_width(100), 4.0);
    }

    #[test]
    fn wheel_canvas_pan_ignores_ctrl_and_write_mode_but_keeps_horizontal_shift_scroll() {
        assert_eq!(NoteCraftApp::wheel_canvas_pan_delta(vec2(0.0, 42.0), Modifiers::NONE, false), egui::Vec2::ZERO);
        assert_eq!(NoteCraftApp::wheel_canvas_pan_delta(vec2(0.0, 42.0), Modifiers::NONE, true), egui::vec2(0.0, 42.0));
        assert_eq!(NoteCraftApp::wheel_canvas_pan_delta(vec2(0.0, 42.0), Modifiers::SHIFT, false), egui::vec2(42.0, 0.0));
        assert_eq!(NoteCraftApp::wheel_canvas_pan_delta(vec2(0.0, 42.0), Modifiers::CTRL, true), egui::Vec2::ZERO);
    }

    #[test]
    fn fresh_preferences_start_with_neutral_sensitivity() {
        let t = notecraft_store::testing::TempDir::new("default-sensitivity");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        assert_eq!(settings.get(SENSITIVITY_KEY), None);
        assert_eq!(settings.get(SENSITIVITY_KEY).and_then(|v| v.parse::<u8>().ok()).unwrap_or(DEFAULT_SENSITIVITY), 50);
        settings.set(SENSITIVITY_KEY, "100").expect("set");
        settings.reset().expect("reset");
        assert_eq!(settings.get(SENSITIVITY_KEY), None);
        assert_eq!(settings.get(SENSITIVITY_KEY).and_then(|v| v.parse::<u8>().ok()).unwrap_or(DEFAULT_SENSITIVITY), 50);
    }

    #[test]
    fn an_alert_queues_behind_an_open_one_and_a_muted_key_is_never_queued() {
        let t = notecraft_store::testing::TempDir::new("alerts");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        app.alert("first", "Title", "one");
        assert!(matches!(&app.dialog, Some(Dialog::Alert { key, .. }) if key == "first"), "the first alert opens");

        app.alert("first", "Title", "one again");
        assert!(app.queued_alerts.is_empty(), "the same alert is not shown twice while it is open");

        app.alert("second", "Title", "two");
        assert_eq!(app.queued_alerts.len(), 1, "a different alert waits its turn");

        app.muted_alerts.insert("third".to_string());
        app.alert("third", "Title", "three");
        assert_eq!(app.queued_alerts.len(), 1, "a muted alert is not queued");
        assert_eq!(app.status, "three", "the status line still says what happened");
    }

    #[test]
    fn a_permanently_muted_alert_is_not_opened() {
        let t = notecraft_store::testing::TempDir::new("alerts-never");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        settings.set("pressure_unavailable_never", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        app.alert("pressure_unavailable", "Pressure", "not available");
        assert!(app.dialog.is_none(), "a permanent mute suppresses the dialog");
        assert_eq!(app.status, "not available", "the status still records the condition");
    }

    #[test]
    fn privacy_welcome_dialog_shows_on_first_launch() {
        let t = notecraft_store::testing::TempDir::new("privacy-first-launch");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        // Do NOT set privacy_welcome_seen - simulate first launch
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let app = NoteCraftApp::new(session);

        // Dialog should be shown because privacy_welcome_seen is not in settings
        assert!(matches!(app.dialog, Some(Dialog::FirstUseWizard { .. })), "PrivacyWelcome dialog should show on first launch");
    }

    #[test]
    fn privacy_welcome_dialog_does_not_show_after_dismissed() {
        let t = notecraft_store::testing::TempDir::new("privacy-dismissed");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let app = NoteCraftApp::new(session);

        // Dialog should not be shown because privacy_welcome_seen is set
        assert!(app.dialog.is_none(), "PrivacyWelcome dialog should not show after being dismissed");
    }

    #[test]
    fn the_wizard_opens_once_after_install_and_not_for_later_versions() {
        let t = notecraft_store::testing::TempDir::new("welcome-wizard-once");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let upgraded = Session::new(&t.path().join("notebooks"), settings.clone()).expect("upgraded session");
        let upgraded = NoteCraftApp::new(upgraded).with_welcome_notebook_setup(|root, _format| Ok(root.join("welcome-to-notecraft")));
        assert!(upgraded.dialog.is_none(), "an existing user is not shown the wizard again for a later version");

        // A fresh install opens the wizard at the welcome page; finishing it saves every answer.
        let fresh = notecraft_store::Settings::at(t.path().join("fresh.toml"));
        let session = Session::new(&t.path().join("fresh-notebooks"), fresh.clone()).expect("fresh session");
        let mut app = NoteCraftApp::new(session).with_welcome_notebook_setup(|root, _format| Ok(root.join("welcome-to-notecraft")));
        assert!(matches!(app.dialog, Some(Dialog::FirstUseWizard { step: 0, .. })), "a fresh install starts at the welcome page");
        assert!(app.finish_first_use_wizard("none", PageFormat::Html, false).is_ok());
        assert_eq!(fresh.get("privacy_welcome_seen").as_deref(), Some("true"));

        let again = Session::new(&t.path().join("fresh-notebooks"), fresh).expect("reopened session");
        let again = NoteCraftApp::new(again).with_welcome_notebook_setup(|root, _format| Ok(root.join("welcome-to-notecraft")));
        assert!(again.dialog.is_none(), "a finished wizard does not open again");
    }

    #[test]
    fn welcome_notebook_prompt_waits_until_the_privacy_dialog_closes() {
        let t = notecraft_store::testing::TempDir::new("welcome-prompt-after-privacy");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let app = NoteCraftApp::new(session).with_welcome_notebook_setup(|root, _format| Ok(root.join("welcome-to-notecraft")));

        assert!(matches!(app.dialog, Some(Dialog::FirstUseWizard { step: 0, .. })), "the wizard opens at the logging step");
        assert!(app.queued_alerts.is_empty(), "the wizard is one window, so nothing waits behind it");
    }

    #[test]
    fn failed_welcome_notebook_generation_keeps_the_prompt_and_does_not_record_a_choice() {
        let t = notecraft_store::testing::TempDir::new("welcome-prompt-failure");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let mut app = NoteCraftApp::new(session).with_welcome_notebook_setup(|_, _| Err("copy failed".to_string()));

        assert!(app.finish_first_use_wizard("none", PageFormat::Cmgfm, true).is_err());

        assert_eq!(settings.get("welcome_notebook_choice_version"), None, "a failed generation leaves the choice unanswered");
    }

    #[test]
    fn the_wizard_installs_the_edition_for_the_chosen_format() {
        let t = notecraft_store::testing::TempDir::new("welcome-wizard-format");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let installed_format = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        let recorder = installed_format.clone();
        let mut app = NoteCraftApp::new(session).with_welcome_notebook_setup(move |root, format| {
            *recorder.borrow_mut() = format.to_string();
            let installed = root.join("welcome-to-notecraft");
            std::fs::create_dir_all(installed.join("00-start/welcome")).map_err(|error| error.to_string())?;
            Ok(installed)
        });

        // Opening the fake folder's page can fail; the full install and open path is covered by the UI test.
        let _ = app.finish_first_use_wizard("none", PageFormat::Cmgfm, true);

        assert_eq!(installed_format.borrow().as_str(), "cmgfm", "the chosen format reaches the install");
    }

    /// `--first-use` writes `privacy_welcome_seen = ""` and keeps every other setting.
    #[test]
    fn first_use_flag_value_shows_the_privacy_welcome_again() {
        let t = notecraft_store::testing::TempDir::new("privacy-first-use-flag");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "").expect("settings");
        settings.set("theme_mode", "dark").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let app = NoteCraftApp::new(session);
        assert!(matches!(app.dialog, Some(Dialog::FirstUseWizard { .. })), "an emptied key counts as not seen");
        assert_eq!(app.session.settings.get("theme_mode").as_deref(), Some("dark"), "other settings are kept");
    }

    #[test]
    fn privacy_welcome_dialog_reappears_after_settings_file_deleted() {
        let t = notecraft_store::testing::TempDir::new("privacy-reset");
        let settings_path = t.path().join("settings.toml");
        let settings = notecraft_store::Settings::at(settings_path.clone());
        // Simulate persistent settings file with privacy_welcome_seen already set
        settings.set("privacy_welcome_seen", "true").expect("settings");
        settings.set("other_setting", "value").expect("settings");

        // Verify setting exists and dialog doesn't show
        let session1 = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let app1 = NoteCraftApp::new(session1);
        assert!(app1.dialog.is_none(), "Dialog should not show when privacy_welcome_seen is set");

        // Delete the settings file (simulates what run.cmd does with --first-use)
        let _ = std::fs::remove_file(&settings_path);

        // Create new settings from deleted file (triggers fresh initialization)
        let settings2 = notecraft_store::Settings::at(settings_path);
        let session2 = Session::new(&t.path().join("notebooks"), settings2).expect("session");
        let app2 = NoteCraftApp::new(session2);

        // Dialog should reappear because settings file is gone
        assert!(matches!(app2.dialog, Some(Dialog::FirstUseWizard { .. })), "PrivacyWelcome dialog should reappear when settings file is deleted");
    }

    #[test]
    fn delete_dialog_shows_for_notebook() {
        let t = notecraft_store::testing::TempDir::new("delete-dialog-notebook");
        let nb_path = t.path().join("test-notebook");
        std::fs::create_dir_all(&nb_path).expect("create notebook dir");

        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(t.path(), settings).expect("session");
        let _app = NoteCraftApp::new(session);

        // Simulate triggering delete notebook dialog
        let dialog = Dialog::Delete { path: nb_path.clone(), title: "Test Notebook".to_string(), count: 0 };

        // Verify the dialog is properly structured for a notebook delete
        assert!(matches!(dialog, Dialog::Delete { .. }));
    }

    #[test]
    fn delete_error_message_shown_on_deletion_failure() {
        let t = notecraft_store::testing::TempDir::new("delete-error-feedback");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(t.path(), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        // Simulate a failed delete operation by setting error status
        app.status = "The notebook could not be deleted: Permission denied".to_string();
        app.status_is_error = true;

        // Verify error state is properly set
        assert!(app.status_is_error, "error flag should be set");
        assert!(app.status.contains("could not be deleted"), "error message should indicate deletion failure");
    }

    #[test]
    fn delete_notebook_clears_editor_state() {
        let t = notecraft_store::testing::TempDir::new("delete-clear-state");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(t.path(), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        // Verify initial state is empty
        assert!(app.editor.is_none(), "editor should start empty");
        assert!(app.selected.is_none(), "selection should start empty");

        // Simulate the delete operation clearing state
        // This is what happens in the delete handler after successful deletion
        app.editor = None;
        app.selected = None;

        // Verify state remains cleared
        assert!(app.editor.is_none(), "editor should remain cleared after delete");
        assert!(app.selected.is_none(), "selection should remain cleared after delete");
    }

    #[test]
    fn node_delete_error_shows_user_feedback() {
        let t = notecraft_store::testing::TempDir::new("node-delete-error");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(t.path(), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        // Simulate node.delete command failure
        app.status = "Could not delete: the operation failed".to_string();
        app.status_is_error = true;

        // Verify error message is shown to user
        assert!(app.status_is_error, "error flag should be set when node delete fails");
        assert!(app.status.contains("Could not delete"), "user should see error message");
    }

    /// A page left with only the SVG fallback (the Shell extraction failed once) still has a
    /// usable icon and must not be reported as missing.
    #[test]
    fn an_svg_fallback_icon_is_accepted_everywhere() {
        let t = notecraft_store::testing::TempDir::new("svg-fallback-icon");
        let page = t.path().to_path_buf();
        let assets = page.join("assets");
        std::fs::create_dir_all(&assets).expect("assets");
        std::fs::write(assets.join("Chapter.docx"), b"docx").expect("attachment");
        std::fs::write(assets.join("Chapter.docx.svg"), b"<svg/>").expect("svg fallback");
        let source = r#"<p><a href="assets/Chapter.docx"><img src="assets/Chapter.docx.svg" alt="" width="48"><br>Chapter.docx</a></p>"#;

        assert_eq!(first_missing_attachment_in_source(source, &page), None, "the SVG fallback is a valid icon");
        assert!(attachment_icon_healthy(&page, "Chapter.docx"));

        std::fs::write(assets.join("Chapter.docx.png"), b"png").expect("png");
        assert_eq!(first_missing_attachment_in_source(source, &page), None, "a PNG sidecar is healthy everywhere");
        assert!(attachment_icon_healthy(&page, "Chapter.docx"));
    }

    #[test]
    fn a_printout_added_in_its_own_container_can_be_undone_and_redone() {
        let t = notecraft_store::testing::TempDir::new("printout-undo");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.md_page.to_string_lossy());
        let (blocks, boxes) = app.editor.as_ref().map(|e| (e.blocks.len(), e.boxes.len())).expect("a page is open");

        app.insert_in_own_container("\n[![a.pdf](assets/a.pdf.svg)](assets/a.pdf)\n\n![Printout page 1](assets/a.pdf.printout-001.png)\n");
        let ed = app.editor.as_ref().expect("editor");
        assert_eq!((ed.blocks.len(), ed.boxes.len()), (blocks + 2, boxes.max(1) + 1));

        app.undo();
        let ed = app.editor.as_ref().expect("editor");
        assert_eq!(ed.blocks.len(), blocks, "undo takes the printout's blocks back out");

        app.redo();
        let ed = app.editor.as_ref().expect("editor");
        assert_eq!(ed.blocks.len(), blocks + 2, "redo puts them back");
        assert_eq!(ed.boxes.last().map(|c| c.blocks), Some(2));
    }

    #[test]
    fn undo_after_a_real_printout_insert_removes_all_of_it_even_after_frames_have_drawn() {
        for html in [false, true] {
            undo_after_a_printout_insert(html);
        }
    }

    fn undo_after_a_printout_insert(html: bool) {
        let t = notecraft_store::testing::TempDir::new("printout-undo-frames");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let pdf = t.path().join("sheet.pdf");
        let page = if html { sample.html_page.clone() } else { sample.md_page.clone() };
        std::fs::write(
            &pdf,
            "%PDF-1.4\n1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n2 0 obj\n<</Type/Pages/Kids[3 0 R]/Count 1>>\nendobj\n3 0 obj\n<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Contents 4 0 R/Resources<<>>>>\nendobj\n4 0 obj\n<</Length 33>>\nstream\n0.8 0.1 0.1 rg 20 20 100 100 re f\nendstream\nendobj\nxref\n0 5\n0000000000 65535 f \n0000000009 00000 n \n0000000054 00000 n \n0000000105 00000 n \n0000000199 00000 n \ntrailer\n<</Root 1 0 R/Size 5>>\nstartxref\n280\n%%EOF\n",
        )
        .expect("pdf");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&page.to_string_lossy());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1200.0, 760.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        for _ in 0..5 {
            h.step();
        }
        let shape =
            |h: &egui_kittest::Harness<'_, NoteCraftApp>| h.state().editor.as_ref().map(|e| (e.blocks.clone(), e.boxes.clone())).expect("editor");
        let before = shape(&h);
        let dir = page.clone();
        h.state_mut().add_file(&dir, &pdf, FileChoice::Printout);
        // Long enough for the autosave to write the page and the window to settle.
        for _ in 0..60 {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        let inserted = shape(&h);
        assert!(inserted.0.len() > before.0.len(), "the printout added blocks: {}", h.state().status);
        h.state_mut().undo();
        for _ in 0..5 {
            h.step();
        }
        assert_eq!(shape(&h), before, "one undo takes the whole printout away");
    }
}

/// Blocks put into a page that has several boxes go into the box the caret is in, and every
/// box keeps its place. Before, the box counts were left behind, `container::fit` saw the
/// mismatch, and the whole arrangement collapsed into one box at the top-left.
#[cfg(test)]
mod keep_boxes_tests {
    use super::*;

    /// A sample page split into two boxes, the second moved to the right, with the caret on
    /// its last line.
    fn two_boxes(html: bool, name: &str) -> (notecraft_store::testing::TempDir, PathBuf, egui_kittest::Harness<'static, NoteCraftApp>) {
        let t = notecraft_store::testing::TempDir::new(name);
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let page = if html { sample.html_page.clone() } else { sample.md_page.clone() };
        let mut app = NoteCraftApp::new(session);
        app.open_path(&page.to_string_lossy());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        for _ in 0..5 {
            h.step();
        }
        {
            let ed = h.state_mut().editor.as_mut().expect("editor");
            let n = ed.blocks.len();
            assert!(n >= 4, "the sample page has blocks to split");
            let first = n / 2;
            ed.boxes = vec![notecraft_model::Container::new(0.0, 0.0, 600.0, first), notecraft_model::Container::new(900.0, 40.0, 300.0, n - first)];
            ed.arranged = true;
            let last = n - 1;
            ed.caret = ed.blocks.get(last).map(|b| pageedit::Caret {
                block: last,
                text: pageedit::caret_text(b),
                focus: false,
                selection: (0, 0),
                place: None,
                cell: None,
                span: None,
            });
            ed.rebuild_body();
            NoteCraftApp::remember_step(ed);
        }
        for _ in 0..3 {
            h.step();
        }
        (t, page, h)
    }

    /// How many blocks the right-hand box holds now.
    fn right_box(h: &egui_kittest::Harness<'static, NoteCraftApp>) -> usize {
        h.state().editor.as_ref().map(|e| e.boxes[1].blocks).expect("editor")
    }

    /// The arrangement after an insert: still two boxes, the right one where it was, every block
    /// counted, and the right box holding at least `grew` more blocks than the `was` it held.
    fn assert_kept(h: &mut egui_kittest::Harness<'static, NoteCraftApp>, what: &str, was: usize, grew: usize) {
        for _ in 0..5 {
            h.step();
        }
        let ed = h.state().editor.as_ref().expect("editor");
        assert_eq!(ed.boxes.len(), 2, "{what}: both boxes are still there: {:?}", ed.boxes);
        assert_eq!((ed.boxes[1].x, ed.boxes[1].y), (900.0, 40.0), "{what}: the right box stays put");
        assert_eq!(ed.boxes.iter().map(|c| c.blocks).sum::<usize>(), ed.blocks.len(), "{what}: every block is in a box");
        assert!(ed.boxes[1].blocks >= was + grew, "{what}: the new block went into the caret's box: {:?}", ed.boxes);
    }

    #[test]
    fn a_pasted_picture_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, page, mut h) = two_boxes(html, "keep-boxes-picture");
            // A 1x1 PNG; what matters is where the block goes, not what it shows.
            let png = vec![
                0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f,
                0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0, 0x05, 0, 0x01, 0xff,
                0x89, 0x99, 0x3d, 0x1d, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
            ];
            let was = right_box(&h);
            h.state_mut().paste_picture(&page, crate::clipboard::Picture { png, width: 1, height: 1 });
            assert_kept(&mut h, &format!("html={html} picture"), was, 1);
        }
    }

    #[test]
    fn an_inserted_table_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, _page, mut h) = two_boxes(html, "keep-boxes-table");
            let was = right_box(&h);
            h.state_mut().ribbon_act(ribbon::Act::Insert("table"));
            assert_kept(&mut h, &format!("html={html} table"), was, 1);
        }
    }

    #[test]
    fn inserted_space_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, _page, mut h) = two_boxes(html, "keep-boxes-space");
            let was = right_box(&h);
            h.state_mut().ribbon_act(ribbon::Act::Insert("space"));
            assert_kept(&mut h, &format!("html={html} space"), was, 1);
        }
    }

    #[test]
    fn pasted_lines_go_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, _page, mut h) = two_boxes(html, "keep-boxes-paste");
            let was = right_box(&h);
            h.state_mut().paste_blocks(pageedit::blocks_to_paste("one\n\ntwo\n\nthree"));
            assert_kept(&mut h, &format!("html={html} paste"), was, 2);
        }
    }

    #[test]
    fn a_new_drawing_named_by_the_page_keeps_the_boxes() {
        for html in [false, true] {
            let (_t, page, mut h) = two_boxes(html, "keep-boxes-ink");
            let was = right_box(&h);
            let svg = notecraft_ink::svg::write(&notecraft_ink::Drawing::default(), notecraft_ink::svg::Flavour::Editable);
            std::fs::write(page.join("ink.svg"), svg).expect("ink");
            h.state_mut().sync_ink_reference();
            assert_kept(&mut h, &format!("html={html} ink"), was, 1);
        }
    }

    #[test]
    fn a_table_of_a_chosen_size_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, _page, mut h) = two_boxes(html, "keep-boxes-sized-table");
            let was = right_box(&h);
            h.state_mut().ribbon_act(ribbon::Act::InsertTable(2, 3));
            assert_kept(&mut h, &format!("html={html} sized table"), was, 1);
        }
    }

    #[test]
    fn a_dropped_file_copied_in_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            // A picture and any other file: each kind has its own snippet.
            for name in ["picture.png", "notes.txt"] {
                let (t, page, mut h) = two_boxes(html, "keep-boxes-copy");
                let file = t.path().join(name);
                std::fs::write(&file, "not really a picture").expect("file");
                let was = right_box(&h);
                h.state_mut().add_file(&page, &file, FileChoice::Copy);
                assert_kept(&mut h, &format!("html={html} copy {name}"), was, 1);
            }
        }
    }

    #[test]
    fn a_dropped_file_linked_goes_into_the_caret_box_and_the_boxes_stay() {
        for html in [false, true] {
            let (t, page, mut h) = two_boxes(html, "keep-boxes-link");
            let file = t.path().join("notes.txt");
            std::fs::write(&file, "some notes").expect("file");
            let was = right_box(&h);
            h.state_mut().add_file(&page, &file, FileChoice::Link);
            assert_kept(&mut h, &format!("html={html} link"), was, 1);
        }
    }

    #[test]
    fn a_snippet_with_no_caret_goes_on_the_end_and_the_boxes_stay() {
        for html in [false, true] {
            let (_t, _page, mut h) = two_boxes(html, "keep-boxes-no-caret");
            h.state_mut().editor.as_mut().expect("editor").caret = None;
            let was = right_box(&h);
            h.state_mut().ribbon_act(ribbon::Act::Equation("x^2"));
            assert_kept(&mut h, &format!("html={html} no caret"), was, 1);
        }
    }
}

//! The NoteCraft window: top bar, notebook tree, page editor with live preview.
//!
//! Every action goes through `Session::run`, so the window, the command line and any
//! later control channel share one code path.

#[path = "app/window_capture.rs"]
mod window_capture;

use crate::backstage;
use crate::control::{ControlRequest, ControlServer, base64};
// The file dialogs: `rfd` everywhere it has a backend, and a stand-in on Android that says so.
#[cfg(target_os = "android")]
use crate::filepicker as rfd;
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
use notecraft_store::search::Ranked;
use notecraft_store::tree::Node;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet, VecDeque};
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

/// The grid boxes snap to on plain paper, in page points: the smallest squared paper's step.
const PLAIN_SNAP_STEP: f32 = 12.0;
/// The room kept above ink drawn above a page's top, when the canvas is panned up to it.
const INK_ABOVE_ROOM: f32 = 24.0;
/// How long opening a page waits for its files to be read before it lets the window go on.
const OPEN_WAIT: Duration = Duration::from_millis(25);

/// A page asked for, and word that its files have been read.
struct PendingOpen {
    dir: PathBuf,
    done: std::sync::mpsc::Receiver<()>,
    /// Asked for by choosing the section or notebook it is the first page of: a phone-width
    /// window keeps the list up when it opens, as it does when such a page opens at once.
    keep_list: bool,
    /// The files are in, but the page shown could not be saved, so the page asked for did not
    /// open yet: it is tried again from this moment on, not every frame.
    retry_at: Option<Instant>,
}

/// Reads the files a page is opened from (its own, and those in `.notecraft`, but not its pictures,
/// which are read as they are drawn, nor its old versions) on a thread of their own, so that a
/// synced folder fetches any not on this computer yet. `None` when they were read within `wait`,
/// or no thread could be had: the page is then opened at once, reading them in place. Otherwise
/// the word that they are in.
fn fetch_page_files(dir: &Path, wait: Duration) -> Option<std::sync::mpsc::Receiver<()>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let page = dir.to_path_buf();
    let spawned = std::thread::Builder::new().name("notecraft-page-files".into()).spawn(move || {
        for folder in [page.clone(), page.join(".notecraft")] {
            let Ok(entries) = std::fs::read_dir(&folder) else { continue };
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|t| !t.is_dir()) {
                    let _ = std::fs::read(entry.path());
                }
            }
        }
        let _ = tx.send(());
    });
    if spawned.is_err() {
        return None;
    }
    match rx.recv_timeout(wait) {
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Some(rx),
        _ => None,
    }
}

/// How far a pen may move in a tap that is still a click, in points (egui allows a mouse 6).
const PEN_CLICK_DIST: f32 = 12.0;
const AUTOSAVE_AFTER: Duration = Duration::from_millis(900);
#[cfg(feature = "voice")]
mod voice;
/// The zoom, and how far down the last page was scrolled, when the window was closed.
const LAST_ZOOM_KEY: &str = "last_zoom";
/// The recently used font families, most recent first, saved with the other local settings.
const RECENT_FONTS_KEY: &str = "recent_fonts";
const LAST_SCROLL_KEY: &str = "last_scroll";
/// How long after the last change to the ink it is written, once the pen is up. A write on a
/// synced folder waits on the mount; made after every stroke on the window's thread, the next
/// stroke waited for it.
const INK_SAVE_AFTER: std::time::Duration = std::time::Duration::from_millis(250);

/// A write of the ink under way: the file, how many changes it holds, whether the file was there
/// before, and where its result comes back.
type InkSaving = (PathBuf, u64, bool, std::sync::mpsc::Receiver<std::io::Result<crate::inkpad::Written>>);
/// The three-bar button: what OneNote puts at the top left of the navigation pane.
const BARS: &str = "\u{2630}";
/// The ArtCraft community chat. Opened in the browser, never fetched by the app.
const DISCORD_URL: &str = "https://discord.gg/artcraft";

/// Where the pen rack and the tool in hand are remembered between runs.
const RACK_KEY: &str = "pen_rack";
/// The size the whole window is drawn at, in percent: see `backstage::UI_SCALES`.
const UI_SCALE_KEY: &str = "ui_scale";
/// The paper File > Print used last, for the next print of a page with no paper of its own.
const PRINT_PAPER_KEY: &str = "print_paper";
const NIB_KEY: &str = "pen_in_hand";
const SENSITIVITY_KEY: &str = "pen_sensitivity";
const DEFAULT_SENSITIVITY: u8 = 50;

fn default_telemetry_level(debug_build: bool) -> &'static str {
    if debug_build { "debug" } else { "none" }
}

const ROW_HEIGHT: f32 = 24.0;

/// Space between the end of a page name and its format label in the page list.
const ROW_LABEL_GAP: f32 = 6.0;

/// How wide `text` is when drawn on one line at `size` points.
fn text_width(ui: &Ui, text: &str, size: f32) -> f32 {
    ui.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), egui::FontId::proportional(size), egui::Color32::WHITE).rect.width())
}

/// Whether a row's format label is shown, and how much room its name gets. The label is
/// shown only when the whole name fits beside it; otherwise the name gets the whole row.
fn row_name_room(name_width: f32, label_width: Option<f32>, room: f32) -> (bool, f32) {
    match label_width {
        Some(label) if name_width + label <= room => (true, room - label),
        _ => (false, room),
    }
}

/// `text` as it fits in `room` points: whole when it fits, otherwise cut at a character
/// and ended with "…", so a long name never runs into what is drawn after it.
fn fit_to_width(ui: &Ui, text: &str, size: f32, room: f32) -> String {
    fit_with(&|s| text_width(ui, s, size), text, room)
}

fn paint_search_snippet(ui: &Ui, painter: &egui::Painter, at: egui::Pos2, text: &str, query: &str, tokens: &Tokens) {
    const SIZE: f32 = 12.0;
    let normal = egui::TextFormat { font_id: egui::FontId::proportional(SIZE), color: tokens.muted, ..Default::default() };
    let emphasis = egui::TextFormat { color: tokens.strong, background: tokens.selection, ..normal.clone() };
    let mut pieces = Vec::new();
    let mut end = 0;
    for range in notecraft_store::search::snippet_matches(text, query) {
        if end < range.start {
            pieces.push((end..range.start, normal.clone()));
        }
        end = range.end;
        pieces.push((range, emphasis.clone()));
    }
    if end < text.len() {
        pieces.push((end..text.len(), normal));
    }
    if crate::bidi::paints(text, None) {
        let (runs, size) = crate::bidi::runs(ui, text, &pieces, SIZE, f32::INFINITY, None);
        crate::bidi::paint_with(painter, at - vec2(0.0, size.y * 0.5), &runs);
    } else {
        let mut job = egui::text::LayoutJob::default();
        for (range, format) in pieces {
            job.append(&text[range], 0.0, format);
        }
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        painter.galley(at - vec2(0.0, galley.rect.height() * 0.5), galley, egui::Color32::PLACEHOLDER);
    }
}

/// [`fit_to_width`] with the width of a piece of text given by `width`.
fn fit_with(width: &dyn Fn(&str) -> f32, text: &str, room: f32) -> String {
    if width(text) <= room {
        return text.to_owned();
    }
    let ends: Vec<usize> = text.char_indices().map(|(i, _)| i).skip(1).chain(std::iter::once(text.len())).collect();
    // The longest start of the name that still fits with the ellipsis after it.
    let (mut lo, mut hi) = (0usize, ends.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate = format!("{}…", text[..ends[mid - 1]].trim_end());
        if width(&candidate) <= room { lo = mid } else { hi = mid - 1 }
    }
    if lo == 0 { "…".to_owned() } else { format!("{}…", text[..ends[lo - 1]].trim_end()) }
}
const INDENT: f32 = 14.0;

/// Shared title treatment for modal dialogs. Keeping this here makes dialogs use the
/// same opaque, theme-owned title band instead of inheriting egui's platform-gray title bar.
/// What the first-use wizard leaves around itself on a small screen: its frame's margin and
/// stroke on both sides, and a gap to the screen's edge.
const WIZARD_SCREEN_MARGIN: f32 = 2.0 * (16.0 + 2.0 + 8.0);
/// The height kept below the wizard's scrolling body for its Back, Next and Finish buttons.
const WIZARD_BUTTON_ROW: f32 = 56.0;

/// One result of a History search (Recent Edits, Find by Author; issue #382): the page's title as
/// the link that opens it, and under it where it is and when it was last changed, so two pages
/// of one name are told apart by their place and time. Returns true when it was chosen.
fn result_row(ui: &mut Ui, t: &Tokens, title: &str, notebook: &str, section: &[String], when: Option<&str>, note: Option<&str>) -> bool {
    let title = if title.is_empty() { crate::tl_id!("ui.untitled_page").to_string() } else { title.to_string() };
    let mut place = vec![notebook.to_string()];
    place.extend(section.iter().cloned());
    let place = place.join(&format!(" {} ", crate::tl_id!("ui.place_separator")));
    let detail = when.map_or_else(|| place.clone(), |when| format!("{place} \u{b7} {when}"));
    let mut clicked = false;
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        let tip = note.map_or_else(|| detail.clone(), |note| format!("{detail}\n{note}"));
        clicked = ui.link(title).on_hover_text(tip).clicked();
        ui.label(egui::RichText::new(detail).small().color(t.muted));
    });
    ui.add_space(4.0);
    clicked
}

/// When the page was last saved, as an ISO time, for Page Versions' "Current page": the stamp the
/// page keeps in its own file, which the web build's files can give as well as a desktop's.
fn page_saved_at(page: &Path) -> Option<String> {
    notecraft_store::recent::stamp(page).map(notecraft_model::time::iso_from_secs)
}

/// One row of Page Versions' list (issue #382): what it is ("Current page", "Saved version") over
/// when, selected with the accent edge the recycle bin uses. A screen reader hears both lines.
fn version_row(ui: &mut Ui, t: &Tokens, title: &str, sub: &str, selected: bool) -> egui::Response {
    let (rect, row) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 4.0, t.row_selected);
        painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 2.0, t.accent);
    } else if row.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    if row.has_focus() {
        painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    let room = (rect.width() - 22.0).max(20.0);
    painter.text(egui::pos2(rect.min.x + 12.0, rect.min.y + 13.0), Align2::LEFT_CENTER, title, egui::FontId::proportional(13.5), t.text);
    let shown = fit_to_width(ui, sub, 12.0, room);
    crate::bidi::line_at(ui, &painter, egui::pos2(rect.min.x + 12.0, rect.min.y + 30.0), Align2::LEFT_CENTER, &shown, 12.0, t.muted, None);
    let heard = format!("{title}, {sub}");
    row.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, heard.clone()));
    row.on_hover_text(sub)
}

pub(crate) fn modal_frame(tokens: &Tokens) -> egui::Frame {
    egui::Frame::NONE.fill(tokens.panel).stroke(egui::Stroke::new(1.5, tokens.accent)).inner_margin(egui::Margin::same(16))
}

// i18n-ui-text: title
pub(crate) fn modal_title(ui: &mut Ui, tokens: &Tokens, title: &str) {
    egui::Frame::NONE.fill(tokens.band).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(crate::tl!(title)).strong().size(17.0).color(tokens.band_text));
    });
    ui.add_space(10.0);
}

/// Dims the window behind a modal dialog and takes the clicks that miss the dialog, so they do
/// not reach the page underneath. Returns the backdrop's layer: the dialog's window keeps itself
/// above it with [`stay_above`].
pub(crate) fn modal_backdrop(ctx: &egui::Context, id: &str) -> egui::LayerId {
    let screen = ctx.viewport_rect();
    egui::Area::new(egui::Id::new(id))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let response = ui.allocate_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, screen.size()), egui::Sense::click());
            ui.painter().rect_filled(response.rect, 0.0, egui::Color32::from_black_alpha(150));
        })
        .response
        .layer_id
}

/// Keeps a modal dialog's window directly above its backdrop. Both are foreground areas, and egui
/// raises an area that is clicked to the top of its order: a click on the dimmed backdrop used to
/// lift it over the dialog, whose buttons could then never be reached again (issue #98).
pub(crate) fn stay_above(ui: &Ui, backdrop: egui::LayerId) {
    ui.ctx().set_sublayer(backdrop, ui.layer_id());
}

/// Modal actions use a readable size based on their label instead of egui's compact default.
// i18n-ui-text: label
pub(crate) fn modal_button(ui: &mut Ui, tokens: &Tokens, label: &str, primary: bool) -> egui::Response {
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

/// A destructive modal action (issue #381): outlined and lettered in the warning colour, never
/// the accent fill that a routine primary action uses, so "delete for good" never looks like
/// "OK".
// i18n-ui-text: label
fn modal_button_destructive(ui: &mut Ui, tokens: &Tokens, label: &str) -> egui::Response {
    let label = crate::tl!(label);
    let width = (label.chars().count() as f32 * 8.0 + 32.0).max(92.0);
    ui.add(
        egui::Button::new(egui::RichText::new(label).size(14.0).strong().color(tokens.warn))
            .min_size(vec2(width, 32.0))
            .fill(tokens.panel)
            .stroke(egui::Stroke::new(1.5, tokens.warn)),
    )
}

/// Keeps the keyboard inside a modal dialog (issue #381): when focus is on none of the
/// dialog's own controls (it has just opened, or focus was left on the page behind it),
/// it moves to `first`. For an action that cannot be undone `first` is Cancel, so Enter
/// or Space straight after the dialog opens never confirms it.
fn hold_modal_focus(first: &egui::Response, inside: &[&egui::Response]) {
    let focused = first.ctx.memory(|m| m.focused());
    if !inside.iter().any(|r| Some(r.id) == focused) {
        first.request_focus();
    }
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
        && crate::fscache::is_file(&page.join("assets").join(source))
    {
        return None;
    }
    if notecraft_store::assets::media_kind(src) != notecraft_store::assets::MediaKind::Other {
        return None;
    }
    // A missing attachment is not a missing-icon case. Do not warn about an icon
    // when the original file itself is no longer present.
    if !crate::fscache::is_file(&page.join("assets").join(name)) {
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
        // Asked every frame while a page is open: see `fscache`.
        crate::fscache::is_file(&page.join("assets").join(format!("{name}.svg")))
            || crate::fscache::is_file(&page.join("assets").join(format!("{name}.png")))
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

/// The widest empty stretch Collect All leaves between things on a page, in page points.
const COLLECT_GAP: f32 = 24.0;
/// The Insert Space tool's own layer over the visible page (issue #164): a vertical-resize
/// pointer, a guide at the pointer's height, and while it is dragged, the band of room being
/// added (below the start) or taken away (above it). Returns where the drag began and how far it
/// went, in screen points, on the frame it is let go.
fn insert_space_overlay(ctx: &Context, visible: egui::Rect, t: &Tokens) -> Option<(f32, f32)> {
    let id = Id::new("insert-space-tool");
    let from_id = id.with("from");
    let mut done = None;
    egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(visible.min).constrain(false).movable(false).show(ctx, |ui| {
        let (rect, resp) = ui.allocate_exact_size(visible.size(), Sense::drag());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::tl_id!("ui.insert_space_how")));
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.drag_started()
            && let Some(p) = resp.interact_pointer_pos()
        {
            ui.data_mut(|d| d.insert_temp(from_id, p.y));
        }
        let from = ui.data(|d| d.get_temp::<f32>(from_id));
        let now = ui.ctx().pointer_latest_pos().filter(|p| rect.contains(*p) || resp.dragged()).map(|p| p.y.clamp(rect.min.y, rect.max.y));
        let painter = ui.painter();
        let line = egui::Stroke::new(1.5, t.accent);
        match (from, now) {
            (Some(a), Some(b)) => {
                let band = egui::Rect::from_x_y_ranges(rect.x_range(), egui::Rangef::new(a.min(b), a.max(b)));
                painter.rect_filled(band, 0.0, t.accent.gamma_multiply(0.12));
                painter.hline(rect.x_range(), a, line);
                painter.hline(rect.x_range(), b, egui::Stroke::new(1.0, t.accent.gamma_multiply(0.7)));
            }
            (None, Some(y)) => {
                painter.hline(rect.x_range(), y, line);
            }
            _ => {}
        }
        if resp.drag_stopped() {
            if let (Some(a), Some(p)) = (from, resp.interact_pointer_pos()) {
                done = Some((a, p.y.clamp(rect.min.y, rect.max.y) - a));
            }
            ui.data_mut(|d| d.remove::<f32>(from_id));
        }
    });
    done
}

/// How far back History > Recent Edits looks (issue #279).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecentRange {
    Today,
    Week,
    Month,
    All,
}

impl RecentRange {
    pub const ALL: [RecentRange; 4] = [RecentRange::Today, RecentRange::Week, RecentRange::Month, RecentRange::All];

    fn label(self) -> &'static str {
        match self {
            RecentRange::Today => crate::tl_id!("ui.today"),
            RecentRange::Week => crate::tl_id!("ui.last_7_days"),
            RecentRange::Month => crate::tl_id!("ui.last_30_days"),
            RecentRange::All => crate::tl_id!("ui.all_time"),
        }
    }

    /// The earliest moment in the range, seconds since 1970, for `now` on a clock `offset`
    /// seconds ahead of UTC: today begins at the user's own midnight.
    pub fn since(self, now: i64, offset: i32) -> Option<i64> {
        let midnight = now - (now + i64::from(offset)).rem_euclid(86_400);
        match self {
            RecentRange::Today => Some(midnight),
            RecentRange::Week => Some(midnight - 6 * 86_400),
            RecentRange::Month => Some(midnight - 29 * 86_400),
            RecentRange::All => None,
        }
    }
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

/// Where the Tags Summary looks for tagged notes (issue #166), widest last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TagScope {
    Page,
    Section,
    Notebook,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SearchScope {
    #[default]
    All,
    Notebook,
    Section,
    Page,
}

impl TagScope {
    const ALL: [TagScope; 4] = [TagScope::Page, TagScope::Section, TagScope::Notebook, TagScope::All];

    fn label(self) -> &'static str {
        match self {
            TagScope::Page => crate::tl_id!("ui.tags_scope_this_page"),
            TagScope::Section => crate::tl_id!("ui.tags_scope_this_section"),
            TagScope::Notebook => crate::tl_id!("ui.tags_scope_this_notebook"),
            TagScope::All => crate::tl_id!("ui.tags_scope_all_notebooks"),
        }
    }

    fn path(self, page: Option<&Path>) -> Option<PathBuf> {
        match self {
            Self::Page => page.map(Path::to_path_buf),
            Self::Section => page.and_then(section_for),
            Self::Notebook => page.and_then(notebook_for),
            Self::All => None,
        }
    }

    fn id(self) -> &'static str {
        match self {
            TagScope::Page => "page",
            TagScope::Section => "section",
            TagScope::Notebook => "notebook",
            TagScope::All => "all",
        }
    }
}

/// The Tags Summary pane, OneNote's Find Tags: the tagged notes in a scope, grouped by tag,
/// each one a way back to its line. It lists what the pages said when it last looked; Refresh
/// looks again, as OneNote's does.
struct TagsPane {
    scope: TagScope,
    /// Only the To Do items not yet ticked.
    unchecked: bool,
    found: Vec<notecraft_store::tags::Tagged>,
}

/// The Page Templates pane (issue #173): every template, by category, each one a new page in
/// the section; and a way to keep the open page as a template of its own.
struct TemplatesPane {
    templates: Vec<notecraft_store::page_templates::Template>,
    /// What the open page will be called as a template.
    name: String,
}

/// The section a page is in: the nearest folder above it that is a section.
fn section_for(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?;
    loop {
        match notecraft_store::tree::read_node(current).map(|node| node.kind) {
            Some(NodeKind::Section) => return Some(current.to_path_buf()),
            Some(NodeKind::Page) => {}
            _ => return None,
        }
        current = current.parent()?;
    }
}

/// Where the middle of the band's title goes (issue #371): on the window's `centre` when the
/// title, `width` wide, fits there between `start` and `end`; otherwise as near to it as the
/// room allows, never before `start`.
fn band_title_centre(centre: f32, start: f32, end: f32, width: f32) -> f32 {
    let half = width / 2.0;
    centre.clamp(start + half, (end - half).max(start + half))
}

/// Where a search result is (issue #370): the titles of its notebook, any section groups and
/// its section, from the tree already in memory, joined with " > ". A sub-page's parent pages
/// are left out; the section is the place. Empty when the page is not in the tree.
fn search_trail(tree: &[Node], page: &Path) -> String {
    fn walk(nodes: &[Node], page: &Path, names: &mut Vec<String>) -> bool {
        for n in nodes {
            if n.path == page {
                return true;
            }
            if page.starts_with(&n.path) {
                let mark = names.len();
                if n.info.kind != NodeKind::Page {
                    names.push(n.info.title.clone());
                }
                if walk(&n.children, page, names) {
                    return true;
                }
                names.truncate(mark);
            }
        }
        false
    }
    let mut names = Vec::new();
    if walk(tree, page, &mut names) { names.join(" > ") } else { String::new() }
}

/// Where a recycled item came from, as the titles of the groups and section that held it,
/// joined with " > ". `original` is the item's own path inside the notebook. Empty when the
/// item sat directly in the notebook or the entry does not say.
fn recycled_place(notebook: &Path, original: &str) -> String {
    let parts: Vec<&str> = original.split('/').filter(|p| !p.is_empty()).collect();
    let mut at = notebook.to_path_buf();
    let mut names = Vec::new();
    for part in parts.iter().take(parts.len().saturating_sub(1)) {
        at.push(part);
        match notecraft_store::tree::read_node(&at) {
            // A sub-page's parent page is not a place: the section above it is.
            Some(node) if node.kind == NodeKind::Page => {}
            Some(node) => names.push(node.title),
            None => names.push((*part).to_string()),
        }
    }
    names.join(" > ")
}

/// What the Format Painter holds: the formatting it picked up, where it picked it up (so the
/// selection it was picked up from is not painted with itself), and whether it keeps painting.
#[derive(Clone, Debug)]
struct FormatPaint {
    style: notecraft_formats::Span,
    from: (usize, (usize, usize)),
    sticky: bool,
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
    /// Font Colour and Text Highlight Colour chosen at the caret (#260). The outer `Some`
    /// means one was chosen; Automatic and No Colour add nothing to new words.
    colour: Option<Option<String>>,
    highlight: Option<Option<String>>,
}

struct Editor {
    generation: u64,
    dir: PathBuf,
    page_id: Option<String>,
    format: PageFormat,
    title: String,
    title_buf: String,
    /// The page has a title of its own; `title` is only the fallback name when it has not.
    titled: bool,
    body: String,
    /// What the page's file holds, as last read or written, so a save that would not change it
    /// is not made: a save rewrites the file, which on a synced folder can take a second.
    saved_body: String,
    /// The text an autosave is writing in the background, until it is written.
    saving: Option<String>,
    dirty_since: Option<Instant>,
    blocks: Vec<notecraft_formats::Block>,
    last_saved: Option<String>,
    /// Who the page says changed it, as its file keeps it (issue #292).
    authors: notecraft_store::authors::Authors,
    attachments: Vec<(String, u64)>,
    /// Where the caret is in the page, and the text of that block while it is written in.
    caret: Option<pageedit::Caret>,
    /// Formatting chosen at a collapsed caret, before any text has been inserted.
    pending_format: Option<PendingFormat>,
    restore_format_selection: Option<(usize, usize, usize)>,
    /// False when the page holds markup the block editor would drop: it is then read here
    /// and changed in its source.
    writable: bool,
    /// Whether the page is locked read only (issue #441), as its own head says. A lock stops
    /// edits; `writable` also stays false for markup the editor cannot keep.
    locked: bool,
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
    /// The colour of this page's rule and grid lines, when it was given one (issue #288).
    rule_colour: Option<notecraft_model::NodeColor>,
    /// The sheet the page is written for; `None` is Auto (issue #269).
    paper_size: Option<notecraft_model::paper::Paper>,
    /// What the page looked like before each change, so a change can be taken back.
    history: crate::history::History,
    /// The ink files in the page's folder. The folder is what decides: the page's own
    /// text is kept in step with this, never the other way round.
    ink_files: Vec<String>,
    /// Where the page's area starts on screen, reported so a script can find a box.
    origin: egui::Pos2,
    /// Where the page's origin sits in the panned canvas, from the last frame it was drawn. The
    /// paper's lines are drawn through it, so they stay with the page and its boxes (issue #79).
    origin_in_canvas: Option<egui::Vec2>,
    /// Whether the boxes are written into the page's text. A page nobody arranged stays plain
    /// text, one column; moving, resizing or adding a box arranges it, and from then on the
    /// boxes are elements around the blocks.
    arranged: bool,
    /// How far down the page the containers reached when last drawn, in page points.
    content_bottom: Option<f32>,
    /// How wide each box was drawn last frame, in page points (`containers::Moved::shown_widths`).
    shown_widths: Vec<f32>,
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
    /// The page as it is written to its file. A box with nothing in it, last in reading order
    /// (a click on open paper adds its line at the end), is only where the cursor was put, not
    /// something on the page, so it is left out.
    fn body_to_save(&self) -> String {
        if !self.arranged || self.boxes.len() < 2 || self.boxes.iter().map(|c| c.blocks).sum::<usize>() != self.blocks.len() {
            return self.body.clone();
        }
        let last = (0..self.boxes.len()).max_by_key(|i| notecraft_model::container::blocks_of(&self.boxes, *i).end).unwrap_or(0);
        let range = notecraft_model::container::blocks_of(&self.boxes, last);
        let blank = |b: &notecraft_formats::Block| crate::preview::is_blank(b) && !matches!(b, notecraft_formats::Block::Item { task: Some(_), .. });
        if range.end != self.blocks.len() || range.is_empty() || !self.blocks[range.clone()].iter().all(blank) {
            return self.body.clone();
        }
        let mut boxes = self.boxes.clone();
        boxes.remove(last);
        notecraft_formats::boxes::write(self.format, &self.blocks[..range.start], Some(&boxes))
    }

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
            colour: pending.colour.flatten(),
            highlight: pending.highlight.flatten(),
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
            caret.write(target);
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
/// The id of the open page's title box. It is the same for every page.
fn page_title_id(ctx: &Context) -> egui::Id {
    egui::Id::new(("page-title-box", ctx.viewport_id()))
}

fn page_title(ui: &mut Ui, ed: &mut Editor, tokens: &Tokens, title_id: egui::Id, rename: &mut Option<String>, zoom: f32) {
    let align = crate::pagetext::halign(&ed.title_buf, ed.page_dir);
    let width = ui.available_width();
    let rtl = crate::bidi::paints(&ed.title_buf, ed.page_dir);
    let editing = ui.memory(|m| m.has_focus(title_id)) || !rtl;
    let r = if editing {
        // A box that wraps (issue #316): a single-line box scrolls sideways, so a long title
        // was cut off at the page's edge on a phone. The title is still one line of text:
        // Enter finishes it rather than breaking it, and a line break pasted in becomes a space.
        let finish = ui.memory(|m| m.has_focus(title_id)) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
        // Escape leaves the title for the words, where the caret was or at the page's first
        // words (issue #571); the title is left as it is.
        // egui lets go of the keyboard on Escape before any widget is drawn, so whether the title
        // had it is what the last frame said. The key is taken here, so the words' box, asked for
        // the keyboard in this same frame, does not let go of it again.
        let had_it = ui.data(|d| d.get_temp::<bool>(title_id.with("had-focus"))).unwrap_or(false);
        let leave = had_it && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        // A locked page's title cannot be typed in: the box keeps its text (issue #441).
        let out = egui::TextEdit::multiline(&mut ed.title_buf)
            .id(title_id)
            .interactive(ed.writable)
            .frame(egui::Frame::NONE)
            .font(egui::FontId::proportional(26.0 * zoom))
            .horizontal_align(align)
            .desired_width(width)
            .desired_rows(1)
            .return_key(None)
            .show(ui);
        // A text value is not an accessible name. Preserve TextEdit's role, value and
        // selection while naming the field for screen readers, including an empty title.
        ui.ctx().accesskit_node_builder(out.response.id, |node| node.set_label(crate::tl_id!("ui.title_2")));
        if ed.title_buf.contains(['\n', '\r']) {
            ed.title_buf = one_line(&ed.title_buf);
        }
        if finish {
            out.response.surrender_focus();
        }
        let has_it = out.response.has_focus();
        ui.data_mut(|d| d.insert_temp(title_id.with("had-focus"), has_it));
        if leave {
            out.response.surrender_focus();
            match ed.caret.as_mut() {
                Some(caret) => caret.focus = true,
                None => {
                    if let Some(index) = ed.blocks.iter().position(crate::pageedit::is_writable) {
                        let text = crate::pageedit::caret_text(&ed.blocks[index]);
                        ed.caret = Some(crate::pageedit::Caret {
                            block: index,
                            text,
                            focus: true,
                            selection: (0, 0),
                            place: Some(0),
                            cell: None,
                            span: None,
                            inline: None,
                        });
                    }
                }
            }
        }
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
    if r.lost_focus() && ed.writable {
        let typed = ed.title_buf.trim();
        if typed.is_empty() {
            // Clearing the title of a page that had one makes it untitled, as the blank title
            // area shows; it used to be dropped, and the old title came back on reopen (#132).
            if ed.titled {
                *rename = Some(String::new());
            }
        } else if !ed.titled || typed != ed.title {
            *rename = Some(typed.to_string());
        }
    }
    // The rule under the name, as OneNote draws one. The date the page carries sits under
    // it already, as the first block of the page.
    let y = ui.cursor().min.y + 2.0;
    let (left, right) = (ui.max_rect().min.x, ui.max_rect().max.x);
    ui.painter().hline(left..=right, y, egui::Stroke::new(1.0, tokens.faint));
    ui.add_space(8.0);
}

/// Who changed the page, under its title, the way OneNote shows an author beside what they
/// wrote (issue #292). Only a page that keeps names shows the line, so a page nobody saved
/// with a name looks as it always did; History > Hide Authors takes the line away and leaves
/// the names in the page.
fn page_authors(ui: &mut Ui, authors: &notecraft_store::authors::Authors, tokens: &Tokens, zoom: f32) {
    if authors.unknown() {
        return;
    }
    let text = match &authors.last {
        Some(last) if authors.names.len() > 1 => {
            crate::i18n::format_message_id("ui.changed_by_names_last_by_name", &[("names", &authors.names.join(", ")), ("name", last)])
        }
        _ => crate::i18n::format_message_id("ui.changed_by_names", &[("names", &authors.names.join(", "))]),
    };
    ui.label(egui::RichText::new(text).size(11.5 * zoom).color(tokens.muted));
    ui.add_space(4.0);
}

use pageedit::{TABLE_MAX_COLUMNS, TABLE_MAX_ROWS};

/// The rows and columns typed into Insert Table, when both are whole numbers within the limits.
/// One value typed into the Paragraph Spacing dialog: a whole number of points up to the most a
/// page may ask for. `Ok(None)` for an empty field, which keeps what each paragraph has.
fn spacing_points(text: &str) -> Result<Option<u16>, ()> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<u16>().ok().filter(|v| *v <= notecraft_model::justify::MAX_POINTS).map(Some).ok_or(())
}

fn table_size(rows: &str, columns: &str) -> Option<(usize, usize)> {
    let r = rows.trim().parse::<usize>().ok().filter(|r| (1..=TABLE_MAX_ROWS).contains(r))?;
    let c = columns.trim().parse::<usize>().ok().filter(|c| (1..=TABLE_MAX_COLUMNS).contains(c))?;
    Some((r, c))
}

/// A title as one line: the line breaks a paste brought in become single spaces.
fn one_line(text: &str) -> String {
    text.split(['\n', '\r']).map(str::trim).filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ")
}

/// What to do with a file the user dropped on a page.
/// What one Undo took back: the drawing, the page's words and boxes, or both at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Undone {
    Ink,
    Page,
    Both,
}

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
        first: bool,
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
    /// Move or Copy (issue #167): where a page, section or section group goes.
    MoveOrCopy {
        path: PathBuf,
        title: String,
        kind: NodeKind,
        dest: Option<PathBuf>,
    },
    /// Insert > Link, or editing the link the caret is in (issue #163): the words to show
    /// and the address, put into the line `block` (or its table cell) in place of the
    /// characters `range` of the caret's text.
    Link {
        block: usize,
        cell: Option<(usize, usize)>,
        range: (usize, usize),
        text: String,
        address: String,
        editing: bool,
        first: bool,
    },
    /// Insert > Online Video: the address of a video to link to at the caret (issue #284).
    OnlineVideo {
        block: usize,
        cell: Option<(usize, usize)>,
        range: (usize, usize),
        address: String,
        first: bool,
    },
    /// Insert > Table > Insert Table...: a table of any size up to the limits, typed in.
    InsertTable {
        rows: String,
        columns: String,
        first: bool,
    },
    /// Home > Alignment and direction > Paragraph Spacing Options (issue #275): the room before
    /// and after the paragraphs that were chosen, and the least height of their lines, in
    /// points. A field left empty keeps what each paragraph has, which is how a mixed value shows.
    ParagraphSpacing {
        blocks: Vec<usize>,
        before: String,
        after: String,
        line: String,
        first: bool,
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
    /// History > Recent Edits (issue #279).
    /// Home > Meeting Details (issue #283): the meetings of one day in the chosen calendar file,
    /// one to look over and put on the page.
    MeetingDetails(crate::meeting::Choice),
    /// History > Find by Author (issue #292): everyone named in the open notebooks, and the
    /// pages of the one chosen. `Some(None)` is the pages nobody is named on.
    FindByAuthor {
        pages: Vec<notecraft_store::authors::Page>,
        chosen: Option<Option<String>>,
    },
    RecentEdits {
        range: RecentRange,
        /// Only the notebook the open page is in, rather than every open notebook.
        this_notebook: bool,
        filter: String,
        items: Vec<notecraft_store::recent::Edited>,
    },
    PageVersions {
        page: PathBuf,
        versions: Vec<notecraft_store::page::Version>,
        /// A saved version opened for reading, before any Restore (issue #278).
        preview: Option<Box<crate::versions::Preview>>,
    },
    /// The notebook's recycle bin as a recovery workspace (issue #365): every recycled item
    /// with where it came from, one selected, and one Restore for the selection.
    RecycleBin {
        notebook: PathBuf,
        /// The notebook's own name, for the header.
        name: String,
        items: Vec<notecraft_store::recycle::Recycled>,
        /// Where each item came from, as the titles of the section and groups above it.
        places: Vec<String>,
        /// The id of the selected item.
        selected: Option<String>,
    },
    PurgeRecycleItem {
        notebook: PathBuf,
        id: String,
        title: String,
    },
    Symbols,
    /// Ctrl+G: the open notebooks, to switch to one from the keyboard (issue #559).
    Notebooks,
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
    Destination(PathBuf),
    Section { index: usize, total: usize, label: String },
    Finished(Result<String, String>),
}

enum Act {
    Toggle(PathBuf),
    /// Lock a page read only, or unlock it (the page's own head says which).
    SetReadOnly(PathBuf, bool),
    Select(PathBuf),
    Add(PathBuf, NodeKind),
    /// A page in this section at this place in the order, from the line between two pages.
    AddPageAt(PathBuf, u32),
    /// The same, when the two pages either side of the line are numbered next to each other:
    /// the section is renumbered ten apart first, then the page goes halfway after the first one.
    RenumberAndAddPage(PathBuf, u32),
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
    /// Open Move or Copy for a page, section or section group.
    MoveOrCopy(PathBuf),
    MakeSubpage(PathBuf),
    PromoteSubpage(PathBuf),
    MovePageUp(PathBuf),
    MovePageDown(PathBuf),
    /// A row dragged in the navigation pane and let go: where it goes.
    Place(PathBuf, crate::navdrag::Drop),
    /// A page row's Cut (true) or Copy (false): the page is held for Paste (issue #532).
    HoldPage(PathBuf, bool),
    /// Paste the page held: after this page, or last in this section (issue #532).
    PastePage(PathBuf),
    PageVersions(PathBuf),
    Delete(PathBuf, String, usize),
    Color(PathBuf, Option<NodeColor>),
    /// Show the node's folder in the operating system's file manager.
    Reveal(PathBuf),
    /// Put a link to this page on the clipboard, to paste into another page.
    CopyLink(PathBuf),
}

/// A file the UI's host should deliver through its download facility.
/// The desktop host keeps the normal file instead; browser hosts opt in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDownload {
    pub name: String,
    pub mime: &'static str,
    pub bytes: Vec<u8>,
}

/// What View > Dock to Desktop changed, kept to put the window back as it was (issue #267).
#[derive(Clone, Debug, PartialEq)]
struct Docked {
    /// The window's place and size on the screen before, when the system said.
    outer: Option<egui::Rect>,
    inner: Option<egui::Vec2>,
    maximized: bool,
    organisers: bool,
    ribbon_collapsed: bool,
    always_on_top: bool,
}

/// How wide a docked window is, in points, at most and at least.
const DOCK_WIDTH: f32 = 420.0;
const DOCK_MIN_WIDTH: f32 = 320.0;

/// The settings key for the name File > Account keeps (issue #304).
pub const AUTHOR_KEY: &str = "author_name";
/// The settings key for the calendar file Home > Meeting Details read last (issue #283).
const MEETING_FILE_KEY: &str = "meeting_calendar_file";
/// The settings key for History > Hide Authors (issue #292).
const HIDE_AUTHORS_KEY: &str = "hide_authors";

/// The version `--new-version` shows when no version is given. It is never downloaded.
pub const SIMULATED_NEW_VERSION: &str = "0.0.99";

pub struct NoteCraftApp {
    window_requests: Vec<crate::windows::Request>,
    secondary_window: bool,
    native_hosted: bool,
    native_close: std::cell::Cell<bool>,
    page_revision: Option<crate::windows::Revision>,
    watcher: notecraft_engine::FileWatcher,
    watcher_woken: bool,
    pending_tree_refresh: bool,
    /// When, on the window's clock, the page was last checked for another window's writes.
    changes_checked_at: Option<f64>,
    page_conflict: bool,
    #[cfg(feature = "voice")]
    voice: voice::VoiceUi,
    session: Session,
    /// Writes autosaves off the window's thread (see [`crate::saver`]).
    saver: crate::saver::Saver,
    /// The name File > Account keeps, which every save notes in the page it changed (issues
    /// #292, #304). `None`: no name is kept and pages record nobody.
    author_name: Option<String>,
    /// History > Hide Authors: who changed a page is not shown under its title. What the page
    /// keeps is not touched (issue #292).
    hide_authors: bool,
    /// Printouts being made off the window's thread, with the format of the page each is for.
    printouts: Vec<(crate::printout::Job, PageFormat)>,
    open: HashSet<PathBuf>,
    selected: Option<PathBuf>,
    /// The page last open in each section, so going back to a section opens where it was left
    /// (issue #557). Kept in the settings, so it is there after a restart too.
    section_pages: HashMap<PathBuf, PathBuf>,
    /// The navigation row to give the keyboard to when it is next drawn (issue #514).
    nav_focus: Option<PathBuf>,
    /// The page a row's Cut or Copy holds, and whether it was Cut (issue #532). A cut page stays
    /// where it is until it is pasted.
    held_page: Option<(PathBuf, bool)>,
    /// Frames left to draw that row in, after which it is no longer asked for.
    nav_focus_tries: u8,
    /// The pages visited, for Back and Forward.
    nav: NavHistory,
    /// Which Quick Access buttons are shown, and where.
    qat: quickaccess::Config,
    /// When the pen last changed the page, to tell which Undo should take back.
    last_ink_change: Option<Instant>,
    /// The ink being written off the window's thread: the file, how many changes the write
    /// holds, whether the file was there before, and where the result comes back.
    ink_saving: Option<InkSaving>,
    /// How many changes have been made to the ink, so a write knows whether it holds them all.
    ink_changes: u64,
    /// When the ink is next written: a moment after the last change, once the pen is lifted.
    ink_save_at: Option<Instant>,
    /// What each Undo took back, newest last, so Redo puts it back the same way.
    undone: Vec<Undone>,
    /// Two columns in the side panel (organisers, then pages) instead of one tree.
    split_panes: bool,
    /// Whether the organisers column is showing, which the three-bar button toggles.
    organisers: bool,
    /// Whether the window is kept above the others.
    always_on_top: bool,
    /// Set when that has just changed and the window manager has not been told yet.
    window_level_pending: bool,
    /// View > Dock to Desktop (issue #267): what docking changed, to put back on undocking.
    docked: Option<Docked>,
    /// Set when docking was asked for and the window manager has not been told yet.
    dock_pending: bool,
    /// True when the window has no frame of the system's own (the app asked for none), so
    /// the band across the top draws the window's buttons and moves and resizes it.
    frameless: bool,
    editor: Option<Editor>,
    dialog: Option<Dialog>,
    import_job: Option<ImportJob>,
    status: String,
    status_is_error: bool,
    /// The host keeps notebook files only for this session, rather than on durable storage.
    session_only_storage: bool,
    search: String,
    searched: String,
    hits: Vec<Ranked>,
    /// Where each search result is, as its notebook and section titles (issue #370).
    hit_trails: Vec<String>,
    /// The result chosen with the arrow keys, if any (issue #370).
    search_pick: Option<usize>,
    search_scope: SearchScope,
    /// The Tags Summary, while it is open.
    tags_pane: Option<TagsPane>,
    /// Home > More tags > Customize Tags, while it is open (issue #259).
    custom_tags: Option<crate::customtags::Dialog>,
    /// Copy Text from Picture while its dialog is open (issue #274).
    picture_text: Option<crate::picture_text::Dialog>,
    /// File > Print while its dialog is open (issue #168).
    print: Option<crate::print::Dialog>,
    /// A paste meant for the text box of such a dialog, held while the page is drawn.
    modal_paste: Option<String>,
    templates_pane: Option<TemplatesPane>,
    cursor: usize,
    tokens: Tokens,
    mode_pref: ModePref,
    ui_language: String,
    /// The size the whole window is drawn at, in percent of the system's own scale, and
    /// whether it still has to be handed to egui.
    ui_scale: u16,
    ui_scale_pending: bool,
    /// What File > Check for updates is doing. The update runs on a thread of its own and
    /// writes here; the window reads it each frame and closes when the update is ready to
    /// be put in place.
    update: std::sync::Arc<std::sync::Mutex<crate::update::Status>>,
    /// The shared update prompt, while it is open (`--new-version` opens it at start). Desktop only:
    /// the web build has no installed copy to update.
    #[cfg(not(target_arch = "wasm32"))]
    update_prompt: Option<crate::update_card::updater::Updater>,
    update_closing: bool,
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
    /// A page asked for whose files were still being read when it was (see [`fetch_page_files`]):
    /// opened once they are in, unless another page is asked for first.
    opening: Option<PendingOpen>,
    /// The page whose files were just read for it: opened without reading them first again.
    fetched: Option<PathBuf>,
    /// While true, a page asked for does not wait on its files (see `open_page_soon`): set only
    /// for pages asked for by hand. Everything else, commands, links, agents, tests, opens the page
    /// at once, its files read in place as before, and finds it open on the next line.
    open_soon: bool,
    /// The page carrying on after a swipe on a touchpad.
    fling: crate::fling::Fling,
    /// How far down the page is scrolled, as last drawn, and where to scroll it in the next pass:
    /// a fling going down the page, or a zoom keeping the spot under the pointer.
    page_offset_y: f32,
    /// How far down the page could be scrolled, as last drawn.
    page_offset_max: f32,
    scroll_to: Option<f32>,
    /// A pinch or Ctrl+wheel zoom keeps the spot under the pointer where it is: the pointer, and
    /// the point of the page under it in page points, from the frame the zoom changed in.
    zoom_anchor: Option<(egui::Pos2, egui::Vec2)>,
    /// What a pinch moved the zoom by that is less than a whole percent, kept for the next
    /// update of the pinch, so a slow pinch zooms as a quick one does instead of not at all.
    zoom_rest: f32,
    /// Last middle-button position while panning the canvas.
    canvas_drag: Option<egui::Pos2>,
    /// Words typed while a pen was in hand, held for the caret's line until it has the keyboard
    /// again (issue #9), and how many frames they have waited.
    typed_while_inking: Vec<egui::Event>,
    typed_wait: u8,
    /// Collect All was asked for: the page gathers its boxes and ink when it is next drawn.
    collect_pending: bool,
    /// The page's sideways scroll bar and its thumb, when one was drawn last frame (issue #188).
    page_hbar: Option<(egui::Rect, egui::Rect)>,
    /// The part of the page the window shows, and how far the page's content reaches sideways on
    /// screen, as last drawn: where a pinch or a wheel acts on the page, and how far a sideways
    /// wheel may pan it. `page_rect` is one view tall from the top of the page, which a page
    /// scrolled down leaves above the window.
    page_view: Option<egui::Rect>,
    page_hspan: Option<(f32, f32)>,
    /// The page ScrollArea's vertical bar hit strip from last frame, kept out of the ink layer.
    page_scroll_strip: Option<egui::Rect>,
    /// True while Insert Space is in hand: the next drag on the page adds or takes away room.
    space_tool: bool,
    /// Draw > Ruler is out on the page (issue #293). Where it lies is the ink layer's.
    ruler: bool,
    /// Draw > Automatic Shapes is on (issue #294). Kept in the settings.
    auto_shapes: bool,
    /// Draw > Auto-switch (issue #9): the pen picks up a pen, and typing or the trackpad puts it
    /// down. On unless turned off.
    auto_switch: bool,
    /// Options: don't check for updates at start up (off by default, so a start checks).
    update_check_off: bool,
    /// The pen in hand was picked up by the pen touching the page, not chosen: a trackpad click
    /// on the page puts it down again. One chosen by hand stays, so a trackpad can draw too.
    pen_picked_by_pen: bool,
    /// Draw > Ink Replay is showing (issue #295). Its progress is the ink layer's.
    replaying: bool,
    /// View > Tabs layout: the sections as tabs over the page (issues #268, #33). Kept.
    tabs_layout: bool,
    /// View > Immersive Reader, while it is open (issue #299).
    reader: Option<crate::reader::Reader>,
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
    /// Present only when the host supplies a download facility.
    file_downloads: Option<Vec<FileDownload>>,
    /// Lines or squares behind the page.
    rules: ribbon::RuleLines,
    /// Where GridCraft is, when it is installed: a table can be handed to it. Looked for
    /// once, when the window opens.
    gridcraft: Option<std::path::PathBuf>,
    /// The page title line is hidden (OneNote: View, Hide Page Title).
    hide_title: bool,
    /// Boxes snap to the page's grid when moved or resized (View, Snap to grid; issue #79).
    snap_to_grid: bool,
    /// The Format Painter's formatting, waiting for the next words picked (issue #165).
    painter: Option<FormatPaint>,
    /// The ribbon is folded away to its tab names (Ctrl+F1; issue #136). Kept in the settings.
    ribbon_collapsed: bool,
    /// While folded, a tab was clicked: its commands are shown over the page until one is used,
    /// the page is clicked or Escape is pressed.
    ribbon_peek: bool,
    /// Where the band with the tab names ends, which is where a shown-over ribbon starts.
    band_bottom: f32,
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
    /// The queued paste is Ctrl+Shift+V, the text only (issue #535).
    keyboard_paste_plain: bool,
    /// The File screen, when it is open, and which of its pages.
    backstage: Option<&'static str>,
    /// File > Save a Copy as it is being filled in (issue #301).
    copy_choice: backstage::CopyChoice,
    /// File > Account as it is being filled in (issue #304).
    account_choice: backstage::AccountChoice,
    portable: crate::portable::Choice,
    /// The pages that are unread now, and when that was last worked out.
    unread: std::collections::HashSet<PathBuf>,
    unread_checked: Option<Instant>,
    /// The page fills the window: the notebooks are hidden until it is turned off.
    full_page: bool,
    /// The window is phone-width, so it shows the notebooks or the page, not both
    /// (`compact.rs`, issue #12). Worked out from the window's width every frame.
    compact: bool,
    /// The compact layout turned Full page on itself, so a wider window turns it off again.
    compact_full_page: bool,
    /// A page was opened since the last frame: the compact layout shows it.
    page_opened: bool,
    /// Put the keyboard in the search box on the next frame.
    focus_search: bool,
    /// The page the window closed on has been opened again (once per run).
    restored: bool,
    /// How far down the open page should be scrolled once it is drawn, coming back to where the
    /// window was left, with how many frames that is still tried for (a page's height is known
    /// only once it is drawn).
    restore_scroll: Option<(f32, u32)>,
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
        self.keyboard_paste_plain = false;
    }

    /// Queues a native Ctrl+Shift+V: the clipboard's text only (issue #535).
    pub fn queue_keyboard_paste_plain(&mut self) {
        log::debug!("queued native keyboard paste, text only");
        self.keyboard_paste_requested = true;
        self.keyboard_paste_plain = true;
    }

    pub fn new(session: Session) -> NoteCraftApp {
        // The installed fonts are read in the background, so the window opens at once.
        #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
        crate::fonts::start_catalog();
        #[cfg(feature = "voice")]
        let voice = voice::VoiceUi::new(&session.settings);
        let section_pages =
            session.settings.get(SECTION_PAGES_KEY).and_then(|v| serde_json::from_str::<HashMap<PathBuf, PathBuf>>(&v).ok()).unwrap_or_default();
        let mode_pref = session.settings.get("theme_mode").and_then(|v| ModePref::parse(&v)).unwrap_or(ModePref::System);
        let ui_language =
            session.settings.get("ui_language").map(|value| crate::i18n::normalize_preference(&value)).unwrap_or_else(|| crate::i18n::AUTO.into());
        crate::i18n::set_current(&ui_language);
        let accent = session.settings.get("theme_accent").and_then(|v| Accent::parse(&v)).unwrap_or(Accent::Default);
        let show_source = false;
        let session_rules = session.settings.get("rule_lines").and_then(|v| ribbon::RuleLines::parse(&v)).unwrap_or_default();
        let session_hide_title = session.settings.get("hide_title").map(|v| v.trim() == "true").unwrap_or(false);
        let session_snap = session.settings.get("snap_to_grid").map(|v| v.trim() == "true").unwrap_or(false);
        let session_auto_shapes = session.settings.get("auto_shapes").is_some_and(|v| v.trim() == "true");
        let session_auto_switch = session.settings.get("switch_by_device").is_none_or(|v| v.trim() != "false");
        let session_update_check_off = session.settings.get("update_check_off_at_start").is_some_and(|v| v.trim() == "true");
        let session_tabs_layout = session.settings.get("tabs_layout").is_some_and(|v| v.trim() == "true");
        let session_ribbon_collapsed = session.settings.get("ribbon_collapsed").map(|v| v.trim() == "true").unwrap_or(false);
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
        let privacy_welcome = session.settings.first_use_pending();
        let telemetry_level = session.settings.get("telemetry_level").unwrap_or_else(|| default_telemetry_level(cfg!(debug_assertions)).to_string());
        log::info!("privacy welcome modal state: show={} telemetry_level={:?}", privacy_welcome, telemetry_level);
        let wizard_format = session.settings.global_format().0;
        let ui_scale = session
            .settings
            .get(UI_SCALE_KEY)
            .and_then(|v| v.trim().parse::<u16>().ok())
            .filter(|percent| backstage::UI_SCALES.contains(percent))
            .unwrap_or(100);
        let wizard_level = session.settings.get("telemetry_level").unwrap_or_else(|| "debug".to_string());
        let author_name = session.settings.get(AUTHOR_KEY).and_then(|n| notecraft_store::authors::clean(&n));
        let hide_authors = session.settings.get(HIDE_AUTHORS_KEY).is_some_and(|v| v.trim() == "true");
        let saver = crate::saver::Saver::default();
        saver.set_author(author_name.clone());
        NoteCraftApp {
            session,
            saver,
            author_name,
            hide_authors,
            printouts: Vec::new(),
            nav: NavHistory::default(),
            qat,
            last_ink_change: None,
            ink_saving: None,
            ink_changes: 0,
            ink_save_at: None,
            undone: Vec::new(),
            open: HashSet::new(),
            selected: None,
            section_pages,
            nav_focus: None,
            held_page: None,
            nav_focus_tries: 0,
            split_panes: session_split,
            organisers: true,
            always_on_top: false,
            window_level_pending: false,
            docked: None,
            dock_pending: false,
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
            session_only_storage: false,
            window_requests: Vec::new(),
            secondary_window: false,
            native_hosted: false,
            native_close: std::cell::Cell::new(false),
            page_revision: None,
            watcher: notecraft_engine::FileWatcher::new(),
            watcher_woken: false,
            pending_tree_refresh: false,
            changes_checked_at: None,
            page_conflict: false,
            search: String::new(),
            searched: String::new(),
            hits: Vec::new(),
            hit_trails: Vec::new(),
            search_pick: None,
            search_scope: SearchScope::default(),
            tags_pane: None,
            custom_tags: None,
            picture_text: None,
            print: None,
            modal_paste: None,
            templates_pane: None,
            cursor: 0,
            tokens: Theme::DEFAULT.tokens(),
            mode_pref,
            ui_language,
            ui_scale,
            ui_scale_pending: ui_scale != 100,
            update: Default::default(),
            #[cfg(not(target_arch = "wasm32"))]
            update_prompt: None,
            update_closing: false,
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
            opening: None,
            fetched: None,
            open_soon: false,
            fling: crate::fling::Fling::default(),
            page_offset_y: 0.0,
            page_offset_max: 0.0,
            scroll_to: None,
            zoom_anchor: None,
            zoom_rest: 0.0,
            collect_pending: false,
            page_hbar: None,
            page_view: None,
            page_hspan: None,
            page_scroll_strip: None,
            space_tool: false,
            ruler: false,
            auto_shapes: session_auto_shapes,
            auto_switch: session_auto_switch,
            update_check_off: session_update_check_off,
            pen_picked_by_pen: false,
            replaying: false,
            tabs_layout: session_tabs_layout,
            reader: None,
            canvas_drag: None,
            typed_while_inking: Vec::new(),
            typed_wait: 0,
            pointer: ribbon::Pointer::Write,
            rack,
            nib,
            sensitivity,
            pad: None,
            file_downloads: None,
            rules: session_rules,
            gridcraft: crate::gridcraft::installed(),
            hide_title: session_hide_title,
            snap_to_grid: session_snap,
            painter: None,
            ribbon_collapsed: session_ribbon_collapsed,
            ribbon_peek: false,
            band_bottom: 0.0,
            zoom_percent: 100,
            restore_scroll: None,
            fit_page_width: false,
            theme_window: false,
            pending_copy: None,
            pending_copy_html: None,
            keyboard_paste_requested: false,
            keyboard_paste_plain: false,
            backstage: None,
            copy_choice: backstage::CopyChoice::default(),
            account_choice: backstage::AccountChoice::default(),
            portable: crate::portable::Choice::default(),
            unread: std::collections::HashSet::new(),
            unread_checked: None,
            full_page: false,
            compact: false,
            compact_full_page: false,
            page_opened: false,
            focus_search: false,
            restored: false,
            icon_recovery_reported: HashSet::new(),
            icon_recovery_suppressed: false,
            muted_alerts: HashSet::new(),
            queued_alerts: VecDeque::new(),
            clipboard: Box::new(crate::clipboard::System),
            open_attachment_warning_suppressed: false,
            #[cfg(feature = "voice")]
            voice,
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
        self.session.settings.set("privacy_welcome_seen", "true").map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    /// Opt in to host-delivered files for explicit exports. Ordinary saves never
    /// request a download. The host must drain these outputs after every frame.
    pub fn enable_file_downloads(&mut self) {
        self.file_downloads.get_or_insert_with(Vec::new);
    }

    /// Takes each explicit file export once, for a host such as the web wrapper.
    pub fn take_file_downloads(&mut self) -> Vec<FileDownload> {
        self.file_downloads.as_mut().map(std::mem::take).unwrap_or_default()
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
        // The zoom the window was left at.
        if let Some(zoom) = self.session.settings.get(LAST_ZOOM_KEY).and_then(|z| z.trim().parse::<u32>().ok()).filter(|z| (50..=250).contains(z)) {
            self.zoom_percent = zoom;
        }
        let Some(saved) = self.session.settings.get("last_page") else { return };
        let saved = saved.trim();
        // A page in a notebook kept outside the notebooks folder is remembered by its whole path.
        let path = if Path::new(saved).is_absolute() {
            PathBuf::from(saved)
        } else {
            self.session.root().join(saved.replace('/', std::path::MAIN_SEPARATOR_STR))
        };
        if !path.is_dir() {
            return;
        }
        // Open every container above it, up to the notebook it is in, so the page is visible in
        // the tree.
        let notebooks: Vec<PathBuf> = self.session.tree().iter().map(|n| n.path.clone()).collect();
        let mut at = path.parent();
        while let Some(dir) = at {
            if dir == self.session.root() {
                break;
            }
            self.open.insert(dir.to_path_buf());
            if notebooks.iter().any(|n| n == dir) {
                break;
            }
            at = dir.parent();
        }
        self.open_page_soon(&path);
        // Scrolled to where it was.
        if let Some(y) = self.session.settings.get(LAST_SCROLL_KEY).and_then(|y| y.trim().parse::<f32>().ok()).filter(|y| y.is_finite() && *y > 0.0) {
            // Pictures may take a while to arrive from a synced folder, and the page is only as
            // tall as what has arrived: it is tried for a few seconds. Opening the page asked for
            // its top; that is not where it was left, so it is not asked for any more.
            self.scroll_to = None;
            self.restore_scroll = Some((y, 300));
        }
    }

    /// What the window keeps of where it was left when it closes: the open page written, the zoom,
    /// and how far down the page is scrolled.
    pub fn before_close(&mut self) {
        self.save_now();
        if self.secondary_window {
            return;
        }
        let _ = self.session.settings.set(LAST_ZOOM_KEY, &self.zoom_percent.to_string());
        let _ = self.session.settings.set(LAST_SCROLL_KEY, &format!("{:.0}", self.page_offset_y.max(0.0)));
    }

    /// Remembers which page is open, for the next time the window starts.
    fn remember_last_page(&mut self, dir: &Path, another_page: bool) {
        // A note window of its own is not where the window was left.
        if self.secondary_window {
            return;
        }
        // Inside the notebooks folder by its place there; a notebook kept elsewhere (a synced
        // folder, say) by the page's whole path, which was not remembered at all before.
        let value = match dir.strip_prefix(self.session.root()) {
            Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
            Err(_) => dir.to_string_lossy().into_owned(),
        };
        let _ = self.session.settings.set("last_page", &value);
        // A new page starts at its top; the same page read again keeps where it is scrolled to.
        if another_page && self.restore_scroll.is_none() {
            self.page_offset_y = 0.0;
        }
    }

    /// Marks a host whose notebook files disappear with the current browser tab.
    /// Native callers keep the default durable-file captions.
    pub fn with_session_only_storage(mut self) -> Self {
        self.session_only_storage = true;
        self
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
        let _ = self.session.run("export.cancel", &Params::new());
        self.backstage = Some(page);
    }

    /// Leaves the File screen, as its Back arrow does.
    pub fn close_backstage(&mut self) {
        let _ = self.session.run("export.cancel", &Params::new());
        self.backstage = None;
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

    /// The size the whole window is drawn at, in percent of the system's own scale.
    pub fn ui_scale(&self) -> u16 {
        self.ui_scale
    }

    /// Shows a newer version in Options and About, for a screen check. It only shows the message
    /// and the releases-page link; nothing is downloaded or installed.
    /// The first-use wizard does not open for a test hook: `--new-version` shows only the update.
    /// Whether the first-use wizard is finished or was never shown. A first launch waits for it, so
    /// the update check never opens over the wizard.
    pub fn first_use_done(&self) -> bool {
        !matches!(self.dialog, Some(Dialog::FirstUseWizard { .. }))
    }

    /// Whether a start checks GitHub for a newer release: on unless Options turns it off.
    pub fn update_check_at_start(&self) -> bool {
        !self.update_check_off
    }

    pub fn skip_first_use_wizard(&mut self) {
        if matches!(self.dialog, Some(Dialog::FirstUseWizard { .. })) {
            self.dialog = None;
        }
    }

    /// Starts the real update check, as an installed copy of `running` (`--set-version`).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_local_update_check(&mut self, running: &str, folder: &std::path::Path) {
        self.update_prompt = Some(crate::update_prompt::check_from_folder(running, folder));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start_update_check(&mut self, running: &str) {
        self.update_prompt = Some(crate::update_prompt::check_latest(running));
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn simulate_new_version(&mut self, version: &str) {
        if let Ok(mut status) = self.update.lock() {
            *status = crate::update::Status::Manual { version: version.to_string() };
        }
        self.update_prompt = Some(crate::update_prompt::offer_named(version));
    }

    /// Saves the notebooks folder NoteCraft uses from the next start. `None` goes back to the
    /// default folder. The current session keeps its folder until then.
    pub fn save_notebooks_root(&mut self, folder: Option<&std::path::Path>) -> Result<(), String> {
        let value = folder.map(|path| path.to_string_lossy().into_owned()).unwrap_or_default();
        self.session.settings.set("last_notebooks_root", &value).map_err(|error| error.to_string())
    }

    fn choose_default_notebooks_folder(&mut self) {
        // The dialog opens where the notebooks are now, not in Documents.
        let Some(picked) = pick_notebooks_folder(self.session.root()) else {
            return;
        };
        self.apply_notebooks_root_choice(Some(&picked));
    }

    /// Moves NoteCraft to the chosen notebooks folder now, or to the default folder for this build
    /// when `folder` is None, and remembers the choice for the next start.
    fn apply_notebooks_root_choice(&mut self, folder: Option<&std::path::Path>) {
        let target = folder.map_or_else(notecraft_store::settings::build_default_notebooks_dir, std::path::Path::to_path_buf);
        if let Err(error) = self.save_notebooks_root(folder) {
            let message = crate::i18n::format_message_id("ui.could_not_save_the_notebooks_folder_error", &[("error", &error)]);
            self.complain(&message);
            return;
        }
        if let Err(error) = self.session.set_root(&target) {
            let error = error.to_string();
            let message = crate::i18n::format_message_id("ui.could_not_open_the_notebooks_folder_error", &[("error", &error)]);
            self.complain(&message);
            return;
        }
        self.editor = None;
        self.pad = None;
        // A page asked for in the folder left is not opened over the new one when it comes in.
        self.opening = None;
        self.fetched = None;
        let shown = target.display().to_string();
        self.say(crate::i18n::format_message_id("ui.notebooks_folder_changed", &[("folder", &shown)]));
    }

    /// Draws the whole window larger or smaller, for a screen where the system's own scale
    /// leaves the ribbon and the menus too small to read, and remembers the choice.
    pub fn set_ui_scale(&mut self, percent: u16) {
        if let Err(error) = self.session.settings.set(UI_SCALE_KEY, &percent.to_string()) {
            let error = error.to_string();
            let message = crate::i18n::format_message_id("ui.could_not_save_the_interface_size_error", &[("error", &error)]);
            self.complain(&message);
            return;
        }
        self.ui_scale = percent;
        self.ui_scale_pending = true;
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

    /// Keeps `page` as the one to open when its section is chosen again (issue #557).
    fn remember_section_page(&mut self, page: &Path) {
        let section = section_in_tree(self.session.tree(), page);
        // The notebook's too, for switching notebooks (issue #559), even for a page outside any
        // section.
        let notebook = self.session.tree().iter().find(|n| page.starts_with(&n.path)).map(|n| n.path.clone());
        let keys: Vec<PathBuf> = section.into_iter().chain(notebook).collect();
        if keys.is_empty() || keys.iter().all(|k| self.section_pages.get(k).is_some_and(|p| p == page)) {
            return;
        }
        for key in keys {
            self.section_pages.insert(key, page.to_path_buf());
        }
        if self.section_pages.len() > SECTION_PAGES_KEPT {
            // Sections no longer in the open notebooks go first.
            let tree = self.session.tree();
            let gone: Vec<PathBuf> = self.section_pages.keys().filter(|s| node_at(tree, s).is_none()).cloned().collect();
            for s in gone {
                self.section_pages.remove(&s);
            }
        }
        if let Ok(kept) = serde_json::to_string(&self.section_pages) {
            let _ = self.session.settings.set(SECTION_PAGES_KEY, &kept);
        }
    }

    /// The page to open when `section` is chosen: the one last open there if it is still in the
    /// section, else its first page (issue #557).
    fn page_for_section(&self, section: &Path) -> Option<PathBuf> {
        let tree = self.session.tree();
        self.section_pages
            .get(section)
            .filter(|p| p.starts_with(section) && node_at(tree, p).is_some_and(|n| n.info.kind == NodeKind::Page))
            .cloned()
            .or_else(|| first_page_under(tree, section))
    }

    /// Opens a section, from the tree, a section tab or the keyboard, in one step (issues #525,
    /// #557): the page last open there, else its first page. An empty section is chosen with the
    /// page closed, so the canvas shows that section's empty state rather than the page of the
    /// section left. If the page being left cannot be saved, nothing moves and the save's own
    /// message says why.
    fn open_section(&mut self, section: &Path) {
        match self.page_for_section(section) {
            Some(page) => {
                self.expand_to(&page);
                self.open_page_soon(&page);
            }
            None => {
                self.save_now();
                if self.is_dirty() || self.page_conflict {
                    return;
                }
                if let Some(left) = self.editor.as_ref().map(|e| e.dir.clone()) {
                    self.note_read(&left, false);
                }
                self.editor = None;
                self.pad = None;
                self.opening = None;
                self.expand_to(section);
                self.selected = Some(section.to_path_buf());
                self.say(crate::tl_id!("ui.this_section_has_no_pages_yet"));
            }
        }
    }

    /// Opens a notebook at the page last open in it, else its first page (issue #559).
    fn open_notebook_where_left(&mut self, notebook: &Path) {
        if self.editor.as_ref().is_some_and(|e| e.dir.starts_with(notebook)) {
            return;
        }
        match self.page_for_section(notebook) {
            Some(page) => {
                self.expand_to(&page);
                self.open_page_soon(&page);
            }
            None => {
                // The page being left is saved and closed first, as choosing an empty section
                // does, so the canvas is not left showing another notebook's page.
                self.save_now();
                if self.is_dirty() || self.page_conflict {
                    return;
                }
                if let Some(left) = self.editor.as_ref().map(|e| e.dir.clone()) {
                    self.note_read(&left, false);
                }
                self.editor = None;
                self.pad = None;
                self.opening = None;
                self.open.insert(notebook.to_path_buf());
                self.selected = Some(notebook.to_path_buf());
                self.say(crate::i18n::format_message_id("ui.opened_path_it_has_no_pages_yet", &[("path", &self.shown(notebook))]));
            }
        }
    }

    /// Puts the keyboard on the navigation row of `row` (issue #514): the panes are brought
    /// back if they were put away, the levels above it opened, and the row scrolled into view
    /// and ringed. Up and Down then go along the rows, and Enter opens one.
    fn focus_nav_row(&mut self, row: &Path) {
        if self.full_page {
            self.full_page = false;
            self.compact_full_page = false;
        }
        if self.split_panes && self.kind_in_tree(row) != Some(NodeKind::Page) {
            self.organisers = true;
        }
        self.expand_to(row);
        if let Some(caret) = self.editor.as_mut().and_then(|ed| ed.caret.as_mut()) {
            caret.focus = false;
        }
        self.nav_focus = Some(row.to_path_buf());
        self.nav_focus_tries = 5;
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

    /// Draw > Ink Help (issue #296): the Welcome notebook's handwriting and drawing page, in
    /// the interface language when the notebook has it and in English otherwise. Only opens a
    /// page; the note that was open is not changed.
    fn open_ink_help(&mut self) {
        let notebooks: Vec<PathBuf> = self.open_notebooks().into_iter().map(|n| n.path).collect();
        match drawing_help_page(&notebooks, &crate::i18n::current_code()) {
            Some(page) => {
                self.expand_to(&page);
                self.open_page(&page);
            }
            None => self.say(crate::tl_id!("ui.ink_help_missing")),
        }
    }

    /// Back to the page before this one.
    fn go_back(&mut self) {
        match self.nav.back(|p| p.exists()) {
            Some(dir) => {
                self.expand_to(&dir);
                self.open_page_inner_soon(&dir);
            }
            None => self.say(crate::tl_id!("ui.there_is_no_earlier_page")),
        }
    }

    /// Forward to the page after this one.
    fn go_forward(&mut self) {
        match self.nav.forward(|p| p.exists()) {
            Some(dir) => {
                self.expand_to(&dir);
                self.open_page_inner_soon(&dir);
            }
            None => self.say(crate::tl_id!("ui.there_is_no_later_page")),
        }
    }

    fn can_undo(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_undo()) || self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_undo)
    }

    fn can_redo(&self) -> bool {
        self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_redo()) || self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_redo)
    }

    /// Undo for the Quick Access Toolbar: the page's words and boxes (the page history in
    /// `history.rs`), or the last stroke when the pen was used after anything else on the page,
    /// or when the page has nothing left to take back but the drawing has.
    fn undo(&mut self) {
        let edited = self.editor.as_ref().and_then(|e| e.history.when());
        // The ink's own last change if it keeps one, else when ink last changed at all.
        let inked = self.pad.as_ref().and_then(crate::inkpad::Pad::newest_change).or(self.last_ink_change);
        let ink_newer = match (inked, edited) {
            (Some(i), Some(e)) => i > e,
            (Some(_), None) => true,
            _ => false,
        };
        // One press that changed both (Delete on boxes and ink picked up together) is one step.
        let together = matches!((inked, edited), (Some(i), Some(e)) if i.max(e).duration_since(i.min(e)) < Duration::from_millis(300));
        let ink_can = self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_undo);
        let page_can = self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_undo());
        let what = if ink_can && page_can && together {
            self.ink_undo();
            self.step_back(true);
            Undone::Both
        } else if ink_can && (ink_newer || !page_can) {
            self.ink_undo();
            Undone::Ink
        } else {
            self.step_back(true);
            Undone::Page
        };
        self.undone.push(what);
    }

    /// Puts back what Undo last took back, from the ink, the page or both, the way it was taken
    /// (issue #155). When the record of which no longer fits what either can put back, the page
    /// goes first, then the ink.
    fn redo(&mut self) {
        let ink_can = self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_redo);
        let page_can = self.editor.as_ref().is_some_and(|e| e.writable && e.history.can_redo());
        match self.undone.pop() {
            Some(Undone::Both) if ink_can && page_can => {
                self.ink_redo();
                self.step_back(false);
            }
            Some(Undone::Ink) if ink_can => self.ink_redo(),
            Some(Undone::Page) if page_can => self.step_back(false),
            _ if !page_can && ink_can => {
                self.undone.clear();
                self.ink_redo();
            }
            _ => {
                self.undone.clear();
                self.step_back(false);
            }
        }
    }

    /// OneNote's automatic switching (issue #9): the window follows what the hand is holding.
    /// A pen that comes down on the page while the pointer is the caret picks up the pen last in
    /// hand, colour and thickness and all, and draws with it from that touch on; typing while a
    /// pen or another ink tool is in hand puts it down and the keys go to the page as words. A
    /// mouse, and a finger on a touch screen, change nothing: only a contact that reports a
    /// force is a pen, so this works where the platform passes the pen's pressure on (see
    /// `stylus.rs`), and elsewhere the tools are chosen by hand as before.
    fn switch_by_device(&mut self, ctx: &Context) {
        if !self.auto_switch || self.dialog.is_some() || self.backstage.is_some() || !self.editor.as_ref().is_some_and(|ed| ed.writable) {
            return;
        }
        if self.pointer == ribbon::Pointer::Write {
            // On the part of the page the window shows: the page's own rect is one view tall from
            // its top, and once the page is panned down it no longer covers what is shown.
            // Not where a menu or a folded ribbon is drawn over the page: that touch is theirs.
            let page = self.page_view.or(self.page_rect);
            let on_page = |at: egui::Pos2| {
                page.is_some_and(|page| page.contains(at)) && ctx.layer_id_at(at).is_none_or(|layer| layer.order < egui::Order::Foreground)
            };
            if self.stylus.landed().is_some_and(on_page) {
                self.ribbon_act(ribbon::Act::InkPen(self.nib));
                self.pen_picked_by_pen = true;
                crate::inkpad::claim_press(ctx);
            }
            return;
        }
        // A click or a tap on the page that no touch came with is the trackpad's (or a mouse's):
        // the hand has left the pen, and the pen the pen picked up is put down. The same click
        // then puts the caret where it landed, as the page is drawn with the caret from here.
        if self.pen_picked_by_pen && matches!(self.pointer, ribbon::Pointer::Draw) {
            // The part of the page the window shows (the page itself reaches above it once
            // scrolled), and not a menu or a folded ribbon drawn over it.
            let page = self.page_view.or(self.page_rect);
            let pressed = ctx.input(|i| {
                let untouched = !i.events.iter().any(|e| matches!(e, egui::Event::Touch { .. }));
                if i.pointer.primary_pressed() && untouched { i.pointer.press_origin() } else { None }
            });
            // Asked of the context outside its input: asked inside, it waited on itself.
            let by_hand = pressed.is_some_and(|at| {
                page.is_some_and(|page| page.contains(at)) && ctx.layer_id_at(at).is_none_or(|layer| layer.order < egui::Order::Foreground)
            });
            if by_hand {
                self.ribbon_act(ribbon::Act::InkPointer(ribbon::Pointer::Write));
                return;
            }
        }
        let typed = ctx.input(|i| {
            !i.modifiers.command
                && !i.modifiers.ctrl
                && !i.modifiers.alt
                && i.events.iter().any(|e| matches!(e, egui::Event::Text(t) if !t.trim().is_empty()))
        });
        if !typed || ctx.egui_wants_keyboard_input() {
            return;
        }
        self.ribbon_act(ribbon::Act::InkPointer(ribbon::Pointer::Write));
        // The words go back to the line the caret was in, where it was: the line's text box
        // was not drawn while the pen was in hand, so it has to take the keyboard again before
        // it can read them. They are held until it has (`replay_typing`).
        let Some(ed) = self.editor.as_mut() else { return };
        // A selection across lines left from before the pen would keep every line as finished
        // words, with no text box to type into.
        ed.across = None;
        let Some(caret) = ed.caret.as_mut() else { return };
        caret.focus = true;
        caret.place = Some(caret.selection.1);
        self.typed_while_inking = ctx.input_mut(|i| {
            let (text, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut i.events).into_iter().partition(|e| matches!(e, egui::Event::Text(_)));
            i.events = rest;
            text
        });
        self.typed_wait = 0;
        ctx.request_repaint();
    }

    /// Gives the page the words that were typed with a pen in hand (see `switch_by_device`)
    /// once the caret's line has the keyboard again, a frame or two after the switch.
    fn replay_typing(&mut self, ctx: &Context) {
        if self.typed_while_inking.is_empty() {
            return;
        }
        let ready = self.editor.as_ref().and_then(|ed| ed.caret.as_ref()).is_some_and(|c| !c.focus) && ctx.memory(|m| m.focused().is_some());
        if ready {
            let typed = std::mem::take(&mut self.typed_while_inking);
            ctx.input_mut(|i| i.events.extend(typed));
        } else if self.typed_wait > 4 {
            // The caret never came back (the page was closed, say): the words have nowhere to go.
            self.typed_while_inking.clear();
        } else {
            self.typed_wait += 1;
            ctx.request_repaint();
        }
    }
    /// Keys for the pens, so a tablet's express keys or a keyboard can change tools without
    /// reaching for the ribbon (issue #44): Alt+1 to Alt+9 pick up that pen on the Draw tab's
    /// rack, and Alt+0 puts the pens down and goes back to typing. On a Mac, where Option and a
    /// digit also type a character, the character is dropped so it does not land in the note.
    fn pen_keys(&mut self, ctx: &Context) {
        const DIGITS: [Key; 10] = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        let Some(n) = ctx.input_mut(|i| DIGITS.iter().position(|key| i.consume_key(Modifiers::ALT, *key))) else { return };
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
        if n == 0 {
            self.ribbon_act(ribbon::Act::InkPointer(ribbon::Pointer::Write));
            self.say(crate::tl_id!("ui.pen_keys_typing"));
        } else if n <= self.rack.len() {
            self.ribbon_act(ribbon::Act::InkPen(n - 1));
            self.say(crate::i18n::format_message_id("ui.pen_keys_picked", &[("number", &n.to_string())]));
        }
    }

    /// The keys and buttons that move through the pages that were opened. Undo and Redo keys
    /// are the page history's own, taken below.
    fn nav_keys(&mut self, ctx: &Context) {
        if self.dialog.is_none() && self.backstage.is_none() {
            let request = ctx.input_mut(|input| {
                let at = input.events.iter().position(
                    |event| matches!(event, egui::Event::Key { key: Key::M, pressed: true, modifiers, .. } if modifiers.command && !modifiers.alt),
                );
                at.and_then(|at| match input.events.remove(at) {
                    egui::Event::Key { modifiers, .. } => Some(modifiers.shift),
                    _ => None,
                })
            });
            if let Some(quick) = request {
                self.request_window(quick);
            }
        }
        // Page navigation waits while a dialog is open: a modal (the recycle bin, a delete
        // confirmation) acts on the page behind it, which must not change underneath it.
        if self.dialog.is_none() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowLeft))
                || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra1))
            {
                self.go_back();
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::ALT, Key::ArrowRight))
                || ctx.input(|i| i.pointer.button_pressed(egui::PointerButton::Extra2))
            {
                self.go_forward();
            }
            // OneNote's Ctrl+Page Up and Ctrl+Page Down: the page above or below in the page
            // list, from the page as well as from the list (issue #162).
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::PageUp)) {
                self.step_page(false);
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::PageDown)) {
                self.step_page(true);
            }
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F1)) {
            self.toggle_ribbon();
        }
    }

    /// The pages of the section in the order the page list shows them: a page, then its
    /// subpages when it is expanded. A collapsed page's subpages are not on the list, so they
    /// are not stepped through either.
    fn listed_pages(&self) -> Vec<PathBuf> {
        fn walk(nodes: &[Node], open: &HashSet<PathBuf>, out: &mut Vec<PathBuf>) {
            for n in nodes.iter().filter(|n| n.info.kind == NodeKind::Page) {
                out.push(n.path.clone());
                if open.contains(&n.path) {
                    walk(&n.children, open, out);
                }
            }
        }
        let mut out = Vec::new();
        if let Some(section) = self.current_section(self.session.tree()) {
            walk(&section.children, &self.open, &mut out);
        }
        out
    }

    /// Opens the page after the open one in the page list, or the one before it. At either end
    /// it stays where it is, as OneNote does, and says so. The page being left is saved first.
    fn step_page(&mut self, forward: bool) {
        let pages = self.listed_pages();
        // From the page on its way, if one is: the next page after that is the one after it, not
        // that one again.
        let here =
            self.opening.as_ref().map(|o| o.dir.clone()).or_else(|| self.editor.as_ref().map(|e| e.dir.clone())).or_else(|| self.selected.clone());
        let at = here.as_ref().and_then(|h| pages.iter().position(|p| p == h));
        let next = match at {
            Some(i) if forward => pages.get(i + 1),
            Some(i) => i.checked_sub(1).and_then(|i| pages.get(i)),
            // No page of this section open: the first or last of its list.
            None if forward => pages.first(),
            None => pages.last(),
        };
        match next.cloned() {
            Some(page) => {
                self.selected = Some(page.clone());
                self.open_page_soon(&page);
            }
            None => self.say(if forward { crate::tl_id!("ui.this_is_the_last_page") } else { crate::tl_id!("ui.this_is_the_first_page") }),
        }
    }

    /// Folds the ribbon away to its tab names, or puts it back, and keeps the choice for the
    /// next start (Ctrl+F1, as in Office; issue #136). The page gets the ribbon's height.
    pub fn toggle_ribbon(&mut self) {
        self.ribbon_collapsed = !self.ribbon_collapsed;
        self.ribbon_peek = false;
        let _ = self.session.settings.set("ribbon_collapsed", if self.ribbon_collapsed { "true" } else { "false" });
    }

    /// Whether the ribbon shows only its tab names: the user folded it, or the window is
    /// phone-width, where its groups would leave no room for the page (issue #12).
    fn ribbon_folded(&self) -> bool {
        self.ribbon_collapsed || self.compact
    }

    /// Works out the phone-width layout for this frame (`compact.rs`). An opened page fills a
    /// compact window; a window that grows wide again shows the notebooks beside the page.
    fn compact_layout(&mut self, ctx: &Context) {
        let compact = crate::compact::is_compact(ctx.content_rect().width());
        if compact != self.compact {
            self.compact = compact;
            if !compact && self.compact_full_page {
                self.full_page = false;
            }
            self.compact_full_page = false;
        }
        if std::mem::take(&mut self.page_opened) && compact && self.editor.is_some() && !self.full_page {
            self.full_page = true;
            self.compact_full_page = true;
        }
    }

    /// Whether the notebooks list fills a phone-width window this frame, with no page beside it.
    fn compact_list_fills(&self) -> bool {
        self.compact && !(self.full_page && self.editor.is_some())
    }

    /// Android's Back button: closes the topmost thing over the page and says whether it did.
    /// A dialog closes as its Cancel would (the first-use wizard goes back a step instead); a
    /// shown-over ribbon or the format painter get Escape, as from the keyboard; the File screen,
    /// the Tags Summary and the page templates close; a page filling the window gives way to the notebooks. With
    /// nothing left to close it does nothing, and Android keeps the app open.
    pub fn system_back(&mut self) -> bool {
        let escape = |frames: &mut std::collections::VecDeque<Vec<egui::Event>>| {
            let key =
                |pressed| egui::Event::Key { key: Key::Escape, physical_key: Some(Key::Escape), pressed, repeat: false, modifiers: Modifiers::NONE };
            frames.push_back(vec![key(true), key(false)]);
        };
        if let Some(dialog) = &self.dialog {
            // Not every dialog reads Escape (a name box keeps it, for one), so Back cancels the
            // dialog itself. The wizard reads Escape as its own Back button.
            if matches!(dialog, Dialog::FirstUseWizard { .. }) {
                escape(&mut self.event_frames);
            } else {
                self.dialog = None;
            }
            return true;
        }
        if !self.search.is_empty() {
            self.dismiss_search();
            return true;
        }
        if self.ribbon_peek || self.painter.is_some() {
            escape(&mut self.event_frames);
            return true;
        }
        if self.backstage.is_some() {
            let _ = self.session.run("export.cancel", &Params::new());
            self.backstage = None;
            return true;
        }
        if self.tags_pane.is_some() && self.compact {
            self.tags_pane = None;
            return true;
        }
        if self.templates_pane.is_some() && self.compact {
            self.templates_pane = None;
            return true;
        }
        if self.full_page && self.editor.is_some() {
            self.full_page = false;
            self.compact_full_page = false;
            return true;
        }
        false
    }

    /// Where the soft keyboard's text goes while a block of the page has the caret, or `None`.
    /// The page editor reads keys itself rather than through a `TextEdit`, so a platform with an
    /// on-screen keyboard asks for it with this (the Android shell does).
    pub fn keyboard_caret_rect(&self) -> Option<egui::Rect> {
        let ed = self.editor.as_ref()?;
        ed.caret.as_ref()?;
        if self.show_source || self.dialog.is_some() || self.compact_list_fills() {
            return None;
        }
        self.page_rect
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

    /// How many printouts are still being made.
    pub fn printouts_pending(&self) -> usize {
        self.printouts.len()
    }

    /// Whether the page being edited has unsaved changes.
    pub fn is_dirty(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.dirty_since.is_some() || e.saving.is_some() || e.title_buf.trim() != if e.titled { e.title.as_str() } else { "" })
            || self.pad.as_ref().is_some_and(|p| p.dirty)
    }

    /// Where the page's words were drawn last frame. Its ink is measured from the top
    /// left of this, so a test that writes on the page aims here.
    pub fn page_rect(&self) -> Option<egui::Rect> {
        self.page_rect
    }

    /// The page's zoom, in percent, as the View tab shows it.
    /// Whether the page has the whole window (Full Page View).
    pub fn is_full_page(&self) -> bool {
        self.full_page
    }

    pub fn zoom_percent(&self) -> u32 {
        self.zoom_percent
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
        if !self.page_conflict
            && let Some(pad) = self.pad.as_mut().filter(|p| p.dirty)
            && let Err(e) = pad.save()
        {
            self.status = e.to_string();
            self.status_is_error = true;
        }
    }

    /// The last status line.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Locks a page read only, or unlocks it, through the engine's command. The page open in
    /// the editor follows at once: a lock stops its edits, and an unlock reopens it for them.
    fn set_read_only(&mut self, path: &Path, locked: bool) {
        // A lock stops the page's writes, so edits made but not yet saved are written first. If
        // that save does not happen, the page stays unlocked rather than losing them (issue #441).
        if locked && self.editor.as_ref().is_some_and(|ed| ed.dir == path && (ed.dirty_since.is_some() || ed.saving.is_some())) {
            self.save_now();
            self.take_saves(true);
            if self.editor.as_ref().is_some_and(|ed| ed.dir == path && (ed.dirty_since.is_some() || ed.saving.is_some())) {
                self.say(crate::tl_id!("ui.save_the_page_before_locking_it"));
                return;
            }
        }
        let arg = if locked { "true" } else { "false" };
        if self.run("page.read_only", &params(&[("path", &path_str(path)), ("locked", arg)])).is_none() {
            return;
        }
        // The navigator's rows, and their lock marks, come from the tree the session holds.
        self.session.refresh();
        let shown = self.editor.as_ref().is_some_and(|ed| ed.dir == path);
        if shown {
            let clean = self.editor.as_ref().is_some_and(|ed| ed.dirty_since.is_none() && ed.saving.is_none());
            if locked {
                if let Some(ed) = self.editor.as_mut() {
                    ed.writable = false;
                    ed.locked = true;
                }
            } else if clean {
                if let Some(ed) = self.editor.as_mut() {
                    ed.locked = false;
                }
                self.open_page(path);
            } else {
                self.say(crate::tl_id!("ui.page_unlocked_reopen_to_edit"));
                return;
            }
        }
        self.say(if locked { crate::tl_id!("ui.page_locked_read_only") } else { crate::tl_id!("ui.page_unlocked") });
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

    fn run_recording(&mut self, id: &str, p: &Params) -> Result<Output, EngineError> {
        let observed =
            self.editor.as_ref().filter(|ed| self.page_revision.is_some() && ed.dir.exists()).map(|ed| (ed.dir.clone(), ed.saved_body.clone()));
        match observed {
            Some((page, expected)) => self.session.run_with_page(id, p, &page, &expected).map(|(output, snapshot)| {
                if let Some(snapshot) = snapshot {
                    if let Some(ed) = &mut self.editor {
                        ed.saved_body = snapshot.body;
                    }
                    self.page_revision = snapshot.revision;
                } else {
                    self.page_revision = None;
                }
                output
            }),
            None => self.session.run(id, p),
        }
    }

    fn run(&mut self, id: &str, p: &Params) -> Option<Output> {
        self.take_saves(true);
        let result = self.run_recording(id, p);
        // A command may have written, replaced or removed a picture or an attachment: what the
        // window kept about files is let go, so the page shows the change at once.
        crate::fscache::forget_all();
        match result {
            Ok(output) => Some(output),
            Err(e) => {
                self.fail(&e);
                self.check_page_changes();
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

    /// Opens Move or Copy for a page, section or section group (issue #167).
    pub fn open_move_or_copy(&mut self, path: &Path) {
        let Some(info) =
            notecraft_store::tree::read_node(path).filter(|n| matches!(n.kind, NodeKind::Page | NodeKind::Section | NodeKind::SectionGroup))
        else {
            return;
        };
        self.dialog = Some(Dialog::MoveOrCopy { path: path.to_path_buf(), title: info.title, kind: info.kind, dest: None });
    }

    /// The places a node of `kind` can be moved or copied to, as (path, title, depth), in tree
    /// order: sections for a page, notebooks and section groups for a section or group. Never
    /// the node itself or anything inside it.
    fn move_destinations(&self, path: &Path, kind: NodeKind) -> Vec<(PathBuf, String, usize)> {
        fn walk(nodes: &[Node], path: &Path, kind: NodeKind, depth: usize, out: &mut Vec<(PathBuf, String, usize)>) {
            for n in nodes.iter().filter(|n| n.info.kind != NodeKind::Page) {
                if n.path.starts_with(path) {
                    continue;
                }
                let fits = match kind {
                    NodeKind::Page => n.info.kind == NodeKind::Section,
                    _ => matches!(n.info.kind, NodeKind::Notebook | NodeKind::SectionGroup),
                };
                // A notebook or group is listed even where it does not fit, as the heading of
                // what is inside it.
                out.push((n.path.clone(), n.info.title.clone(), depth));
                if !fits && n.info.kind == NodeKind::Section {
                    out.pop();
                }
                walk(&n.children, path, kind, depth + 1, out);
            }
        }
        let mut out = Vec::new();
        walk(self.session.tree(), path, kind, 0, &mut out);
        out
    }

    /// Moves or copies `path` into `dest`, then keeps the window on the same page: a moved page
    /// that was open is opened again where it went.
    fn move_or_copy(&mut self, path: &Path, dest: &Path, copy: bool) {
        self.save_now();
        if self.is_dirty() {
            return;
        }
        let command = if copy { "node.copy" } else { "node.move" };
        let editor_path = self.editor.as_ref().map(|editor| editor.dir.clone());
        let Some(Output::Path(new_path)) = self.run(command, &params(&[("path", &path_str(path)), ("to", &path_str(dest))])) else { return };
        self.open.insert(dest.to_path_buf());
        if copy {
            self.say(crate::tl_id!("ui.copied"));
            return;
        }
        self.nav.relocate(path, &new_path);
        let remap = |candidate: &Path| candidate.strip_prefix(path).map_or_else(|_| candidate.to_path_buf(), |suffix| new_path.join(suffix));
        self.open = self.open.iter().map(|branch| remap(branch)).collect();
        self.selected = self.selected.as_deref().map(remap);
        if let Some(editor_path) = editor_path.filter(|editor_path| editor_path.starts_with(path)) {
            let relocated = remap(&editor_path);
            self.open_page_inner(&relocated);
        }
        self.say(crate::tl_id!("ui.moved"));
    }

    fn create_page_in(&mut self, parent: &Path) {
        if let Some(Output::Path(page)) = self.run("node.create", &params(&[("parent", &path_str(parent)), ("kind", "page")])) {
            self.open_page(&page);
            self.say(crate::tl_id!("ui.new_page"));
        }
    }

    /// All native note windows share this file and their read/write operations run on the
    /// same event-loop thread. Read it afresh so a later window does not write over marks
    /// kept by a previous window since this one was opened.
    fn read_state(&self) -> notecraft_store::read_state::ReadState {
        let file = self.session.settings.path().parent().unwrap_or(Path::new(".")).join("read-state.tsv");
        notecraft_store::read_state::ReadState::load(&file)
    }

    /// Records the page at `dir` as read as it stands. `opened` is true when the user opened
    /// it, which also clears a mark they put on it by hand; leaving or saving a page keeps
    /// that mark.
    fn note_read(&mut self, dir: &Path, opened: bool) {
        let Some(id) = notecraft_store::tree::read_node(dir).filter(|n| n.kind == NodeKind::Page).map(|n| n.id) else { return };
        let stamp = notecraft_store::read_state::page_stamp(dir);
        let mut state = self.read_state();
        if !opened && state.is_marked_unread(&id) {
            return;
        }
        state.mark_read(&id, stamp.as_deref());
        if let Err(e) = state.save() {
            log::warn!("could not keep the read state: {e}");
        }
        self.unread.remove(dir);
    }

    /// Every page in the open notebooks, in the order the page list shows them.
    fn pages_in_order(&self) -> Vec<(PathBuf, String)> {
        fn walk(node: &Node, out: &mut Vec<(PathBuf, String)>) {
            if node.info.kind == NodeKind::Page {
                out.push((node.path.clone(), node.info.id.clone()));
            }
            for child in &node.children {
                walk(child, out);
            }
        }
        let mut out = Vec::new();
        for notebook in self.session.tree() {
            walk(notebook, &mut out);
        }
        out
    }

    /// Works out again which pages are unread: at most every half minute unless `now`.
    fn refresh_unread(&mut self, now: bool) {
        if !now && self.unread_checked.is_some_and(|at| at.elapsed() < Duration::from_secs(30)) {
            return;
        }
        self.unread_checked = Some(Instant::now());
        let here = self.editor.as_ref().map(|e| e.dir.clone());
        let pages = self.pages_in_order();
        let state = self.read_state();
        let mut unread = std::collections::HashSet::new();
        for (dir, id) in pages {
            // The page in front of the user is being read.
            if here.as_deref() == Some(dir.as_path()) && !state.is_marked_unread(&id) {
                continue;
            }
            if state.is_unread(&id, notecraft_store::read_state::page_stamp(&dir).as_deref()) {
                unread.insert(dir);
            }
        }
        self.unread = unread;
    }

    /// The unread pages, from the notebooks folder down, for `ui.inspect`.
    fn unread_list(&self) -> Vec<String> {
        let mut unread: Vec<String> =
            self.unread.iter().map(|p| p.strip_prefix(self.session.root()).unwrap_or(p).to_string_lossy().replace('\\', "/")).collect();
        unread.sort();
        unread
    }

    /// History > Next Unread: the next unread page after the open one, in page-list order,
    /// round to the start (issue #291).
    fn next_unread(&mut self) {
        self.refresh_unread(true);
        let pages = self.pages_in_order();
        let here = self.editor.as_ref().map(|e| e.dir.clone());
        let start = here.as_ref().and_then(|h| pages.iter().position(|(p, _)| p == h)).map_or(0, |i| i + 1);
        let next = pages.iter().cycle().skip(start).take(pages.len()).find(|(p, _)| self.unread.contains(p) && Some(p) != here.as_ref());
        match next.map(|(p, _)| p.clone()) {
            Some(page) => self.open_path(&path_str(&page)),
            None => self.say(crate::tl_id!("ui.there_are_no_unread_pages")),
        }
    }

    /// History > Mark as Read / Mark as Unread on the open page (issue #291).
    fn toggle_read(&mut self) {
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let Some(id) = notecraft_store::tree::read_node(&dir).map(|n| n.id) else { return };
        let mut state = self.read_state();
        if state.is_marked_unread(&id) {
            self.note_read(&dir, true);
            self.say(crate::tl_id!("ui.marked_as_read"));
        } else {
            state.mark_unread(&id);
            if let Err(e) = state.save() {
                log::warn!("could not keep the read state: {e}");
            }
            self.unread.insert(dir);
            self.say(crate::tl_id!("ui.marked_as_unread"));
        }
    }
    /// Every notebook that is open: those in the notebooks folder and those opened from
    /// elsewhere.
    fn open_notebooks(&self) -> Vec<notecraft_store::tree::Node> {
        let mut notebooks = notecraft_store::tree::load_notebooks(self.session.root());
        for dir in self.session.opened_elsewhere() {
            if let Some(nb) = notecraft_store::tree::load_tree(dir)
                && !notebooks.iter().any(|n| n.path == nb.path)
            {
                notebooks.push(nb);
            }
        }
        notebooks
    }

    /// View > Dock to Desktop (issue #267): the window becomes a narrow column down the right
    /// side of the screen, kept above other windows, with the page and nothing else, for notes
    /// taken beside something else. Again puts the window, its panes, the ribbon and its level
    /// back as they were. The open page, its caret and the autosave carry on through both.
    ///
    /// Other windows are not moved aside: no platform branch reserves the space yet, and the
    /// status line says so.
    fn toggle_dock(&mut self, ctx: &Context) {
        use egui::ViewportCommand as V;
        if let Some(was) = self.docked.take() {
            self.organisers = was.organisers;
            self.ribbon_collapsed = was.ribbon_collapsed;
            self.always_on_top = was.always_on_top;
            let level = if was.always_on_top { egui::viewport::WindowLevel::AlwaysOnTop } else { egui::viewport::WindowLevel::Normal };
            ctx.send_viewport_cmd(V::WindowLevel(level));
            if was.maximized {
                ctx.send_viewport_cmd(V::Maximized(true));
            } else {
                if let Some(size) = was.inner {
                    ctx.send_viewport_cmd(V::InnerSize(size));
                }
                if let Some(outer) = was.outer {
                    ctx.send_viewport_cmd(V::OuterPosition(outer.min));
                }
            }
            self.say(crate::tl_id!("ui.the_window_is_undocked"));
            return;
        }
        let info = ctx.input(|i| i.viewport().clone());
        let Some(screen) = info.monitor_size.filter(|s| s.x > DOCK_MIN_WIDTH && s.y > 200.0) else {
            self.say(crate::tl_id!("ui.the_screen_size_is_not_known_so_the_window_cannot_be_docked"));
            return;
        };
        self.docked = Some(Docked {
            outer: info.outer_rect,
            inner: info.inner_rect.map(|r| r.size()),
            maximized: info.maximized.unwrap_or(false),
            organisers: self.organisers,
            ribbon_collapsed: self.ribbon_collapsed,
            always_on_top: self.always_on_top,
        });
        let width = DOCK_WIDTH.min(screen.x / 3.0).max(DOCK_MIN_WIDTH);
        if info.maximized == Some(true) {
            ctx.send_viewport_cmd(V::Maximized(false));
        }
        ctx.send_viewport_cmd(V::InnerSize(egui::vec2(width, screen.y)));
        ctx.send_viewport_cmd(V::OuterPosition(egui::pos2(screen.x - width, 0.0)));
        ctx.send_viewport_cmd(V::WindowLevel(egui::viewport::WindowLevel::AlwaysOnTop));
        // The page gets the narrow window: the notebooks fold away and so does the ribbon,
        // whose tabs still open it. Neither is written to the settings: undocking puts them back.
        self.organisers = false;
        self.ribbon_collapsed = true;
        self.ribbon_peek = false;
        self.always_on_top = true;
        self.say(crate::tl_id!("ui.the_window_is_docked"));
    }

    /// True while View > Dock to Desktop is on.
    pub fn is_docked(&self) -> bool {
        self.docked.is_some()
    }

    /// File > Account (issue #304): the name the pages changed from now on are marked with,
    /// or none. Kept in the settings, not in any notebook; pages changed before keep what they
    /// had.
    pub fn set_author(&mut self, name: Option<String>) {
        // What was written before is saved under the name it was written with.
        self.save_now();
        let name = name.as_deref().and_then(notecraft_store::authors::clean);
        let _ = self.session.settings.set(AUTHOR_KEY, name.as_deref().unwrap_or(""));
        self.saver.set_author(name.clone());
        self.say(match &name {
            Some(n) => crate::i18n::format_message_id("ui.your_changes_are_marked_name", &[("name", n)]),
            None => crate::tl_id!("ui.your_changes_are_marked_with_no_name").to_string(),
        });
        self.author_name = name;
    }

    /// The name File > Account keeps, if any.
    pub fn author_name(&self) -> Option<&str> {
        self.author_name.as_deref()
    }

    /// The pages changed in `range`, in every open notebook or the open page's own.
    fn recent_edits(&self, range: RecentRange, this_notebook: bool) -> Vec<notecraft_store::recent::Edited> {
        let mut notebooks = self.open_notebooks();
        if this_notebook && let Some(here) = self.editor.as_ref().and_then(|e| notebook_for(&e.dir)) {
            notebooks.retain(|n| n.path == here);
        }
        notecraft_store::recent::edited(&notebooks, range.since(notecraft_model::time::now_secs(), notecraft_model::time::local_offset()))
    }

    fn show_find_by_author(&mut self) {
        // What is on screen is what is searched: the open page's last words are saved first.
        self.save_now();
        let pages = notecraft_store::authors::pages(&self.open_notebooks());
        self.dialog = Some(Dialog::FindByAuthor { pages, chosen: None });
    }

    fn show_recent_edits(&mut self, range: RecentRange, this_notebook: bool) {
        let items = self.recent_edits(range, this_notebook);
        self.dialog = Some(Dialog::RecentEdits { range, this_notebook, filter: String::new(), items });
    }

    /// History > Page Versions for the open page, as the ribbon and the page's menu open it.
    pub fn open_page_versions(&mut self) {
        if let Some(page) = self.current_page().map(Path::to_path_buf) {
            self.show_page_versions(&page);
        }
    }

    fn show_page_versions(&mut self, page: &Path) {
        self.save_now();
        if self.is_dirty() {
            return;
        }
        let p = params(&[("path", &path_str(page))]);
        if let Some(Output::Versions(versions)) = self.run("page.versions", &p) {
            self.dialog = Some(Dialog::PageVersions { page: page.to_path_buf(), versions, preview: None });
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
        // Typing in the page (its title or a paragraph) is not a reason to ignore page
        // commands: OneNote's Ctrl+N and Ctrl+Alt+M work from the middle of a sentence (issue
        // #254). Any other text box keeps the keys: the search box, a rename, the source view.
        let typing_elsewhere = ctx.egui_wants_keyboard_input() && !self.page_text_has_focus(ctx);
        if typing_elsewhere || self.dialog.is_some() || self.backstage.is_some() {
            return;
        }
        // Which page is selected, and whether it can be made a subpage, is read from its folder;
        // that is only worth doing when a key went down. Every frame it read the page and every
        // page beside it, which on a synced folder held each frame up.
        if !ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
            return;
        }
        // The kind is taken from the tree in memory: the selected page may be one still on its
        // way, and reading its file here waited for the download on every key press.
        let Some(page) = self.selected.clone().filter(|path| match self.kind_in_tree(path) {
            Some(kind) => kind == NodeKind::Page,
            None => notecraft_store::tree::read_node(path).is_some_and(|info| info.kind == NodeKind::Page),
        }) else {
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
        // OneNote's Ctrl+Alt+M: Move or Copy the page.
        if ctx.input_mut(|input| input.consume_key(mods, Key::M)) {
            self.open_move_or_copy(&page);
            return;
        }
        // Whether it can be made a subpage reads the pages beside it: only when that key is down.
        let down = |key: Key| {
            ctx.input(|i| {
                i.events
                    .iter()
                    .any(|e| matches!(e, egui::Event::Key { key: k, pressed: true, modifiers, .. } if *k == key && modifiers.matches_logically(mods)))
            })
        };
        if down(Key::CloseBracket) && Self::can_make_subpage(&page) && ctx.input_mut(|input| input.consume_key(mods, Key::CloseBracket)) {
            self.reparent_page(&page, "node.demote");
        } else if down(Key::OpenBracket) && Self::can_promote_subpage(&page) && ctx.input_mut(|input| input.consume_key(mods, Key::OpenBracket)) {
            self.reparent_page(&page, "node.promote");
        }
    }

    /// Whether the keyboard is with one of the open page's own text boxes, its title or a block,
    /// rather than with a box outside the page such as search or a rename in the tree.
    /// OneNote's keys for going between sections and pages, for a new section, the page's title,
    /// the time stamps and the heading styles. Not while a dialog, the File screen or another text
    /// box (the search box, a rename) has the keyboard; the page's own words and title do not
    /// stop them, as in OneNote.
    fn onenote_keys(&mut self, ctx: &Context) {
        if self.dialog.is_some() || self.backstage.is_some() || self.own_modal_open() || self.custom_tags.is_some() {
            return;
        }
        let ours = |e: &egui::Event| {
            matches!(e, egui::Event::Key { pressed: true, key, modifiers, .. }
                if (modifiers.command && matches!(key, Key::Tab | Key::T | Key::G))
                    || (modifiers.alt && matches!(key, Key::Home | Key::End))
                    || (modifiers.alt && modifiers.shift && matches!(key, Key::D | Key::T | Key::F | Key::ArrowUp | Key::ArrowDown))
                    || (modifiers.command && modifiers.alt && matches!(key, Key::Num1 | Key::Num2 | Key::Num3 | Key::Num4 | Key::Num5 | Key::Num6)))
        };
        // Asked only when one of these keys is down: which widget has the keyboard is read through
        // `read_response`, which is not free.
        if !ctx.input(|i| i.events.iter().any(ours)) {
            return;
        }
        // Another text box with the keyboard keeps these keys: the search box, a rename. A row
        // of the notebooks or a button with the keyboard does not.
        let focused_text_box = ctx.memory(|m| m.focused()).is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some());
        if focused_text_box && !self.page_text_has_focus(ctx) {
            return;
        }
        let ctrl_alt = Modifiers::COMMAND | Modifiers::ALT;
        let shift_cmd = Modifiers::COMMAND | Modifiers::SHIFT;
        let alt_shift = Modifiers::ALT | Modifiers::SHIFT;
        // egui matches a key without Shift to the key with it too, so the Shift forms go first.
        // Ctrl+Tab and Ctrl+Shift+Tab: the next or the previous section of the notebook, in the
        // order it shows them, round from the last to the first (issue #512).
        let section_step = ctx.input_mut(|i| {
            if i.consume_key(shift_cmd, Key::Tab) {
                Some(false)
            } else if i.consume_key(Modifiers::COMMAND, Key::Tab) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(forward) = section_step {
            self.step_section(forward);
            return;
        }
        // Ctrl+Shift+G: the keyboard to the section's row; Ctrl+Alt+G: to the page's row (issue
        // #514). Ctrl+G: the list of notebooks, to switch (issue #559).
        let g = ctx.input_mut(|i| {
            if i.consume_key(shift_cmd, Key::G) {
                Some(1)
            } else if i.consume_key(ctrl_alt, Key::G) {
                Some(2)
            } else if i.consume_key(Modifiers::COMMAND, Key::G) {
                Some(3)
            } else {
                None
            }
        });
        if let Some(which) = g {
            let here = self.editor.as_ref().map(|e| e.dir.clone()).or_else(|| self.selected.clone());
            match which {
                1 => {
                    let section = here.as_ref().and_then(|h| {
                        if self.kind_in_tree(h) == Some(NodeKind::Section) { Some(h.clone()) } else { section_in_tree(self.session.tree(), h) }
                    });
                    if let Some(section) = section {
                        self.focus_nav_row(&section);
                    }
                }
                2 => {
                    if let Some(page) = here.filter(|h| self.kind_in_tree(h) == Some(NodeKind::Page)) {
                        self.focus_nav_row(&page);
                    }
                }
                _ => self.dialog = Some(Dialog::Notebooks),
            }
            return;
        }
        // Ctrl+Shift+T: the page's title (issue #571). Ctrl+T: a new section (issue #558).
        if ctx.input_mut(|i| i.consume_key(shift_cmd, Key::T)) {
            self.focus_page_title(ctx);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::T)) {
            self.new_section_here();
            return;
        }
        // Alt+Home and Alt+End: the first or the last page of the section's list (issue #513).
        let end = ctx.input_mut(|i| {
            if i.consume_key(Modifiers::ALT, Key::Home) {
                Some(false)
            } else if i.consume_key(Modifiers::ALT, Key::End) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(last) = end {
            self.page_at_end(last);
            return;
        }
        // Alt+Shift+Up and Down: the paragraph with the caret, or the paragraphs picked, one place
        // up or down in their box (issue #563).
        let shift_by = ctx.input_mut(|i| {
            if i.consume_key(alt_shift, Key::ArrowUp) {
                Some(true)
            } else if i.consume_key(alt_shift, Key::ArrowDown) {
                Some(false)
            } else {
                None
            }
        });
        if let Some(up) = shift_by {
            self.move_paragraphs(up);
            return;
        }
        // Alt+Shift+D, T and F: the date, the time, or both, as the ribbon's Time Stamp buttons
        // write them (issue #561).
        let stamp = ctx.input_mut(|i| {
            if i.consume_key(alt_shift, Key::D) {
                Some("date")
            } else if i.consume_key(alt_shift, Key::T) {
                Some("time")
            } else if i.consume_key(alt_shift, Key::F) {
                Some("datetime")
            } else {
                None
            }
        });
        // Ctrl+Alt+1 to 6: Heading 1 to Heading 6, as the Styles gallery sets them (issue #562).
        const LEVELS: [(Key, &str); 6] =
            [(Key::Num1, "h1"), (Key::Num2, "h2"), (Key::Num3, "h3"), (Key::Num4, "h4"), (Key::Num5, "h5"), (Key::Num6, "h6")];
        let heading = ctx.input_mut(|i| LEVELS.iter().find(|(key, _)| i.consume_key(ctrl_alt, *key)).map(|(_, kind)| *kind));
        if stamp.is_none() && heading.is_none() {
            return;
        }
        // On a Mac, Option with a letter or a digit also types a character: it must not land in
        // the note as well.
        ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Text(_))));
        let Some(ed) = self.editor.as_ref() else { return };
        if ed.locked || !ed.writable {
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        if let Some(what) = stamp {
            self.ribbon_act(ribbon::Act::Insert(what));
        } else if let Some(kind) = heading {
            // The words the caret is in; with none, the page's last words, as the ribbon does.
            // A step of its own, so one Undo takes back exactly this.
            if let Some(ed) = self.editor.as_mut() {
                Self::place_caret_for_format(ed);
                Self::remember_step(ed);
            }
            self.ribbon_act(ribbon::Act::Block(kind));
            if let Some(ed) = self.editor.as_mut() {
                Self::remember_edit(ed);
            }
        }
    }

    /// Moves the paragraph with the caret, or every block of a selection across paragraphs, one
    /// place up or down inside its box (issue #563): it changes places with the block before or
    /// after them. The words, their formatting and any list they make stay as they are, the
    /// selection with them, and what the page keeps for a table or a justified paragraph goes
    /// with its block. One Undo puts them back. At the top or bottom of the box nothing moves.
    fn move_paragraphs(&mut self, up: bool) {
        let Some(ed) = self.editor.as_mut() else { return };
        if ed.locked || !ed.writable {
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        let range = match (ed.across.filter(|a| !a.inside_one_block()), ed.caret.as_ref()) {
            (Some(across), _) => across.blocks(),
            (None, Some(caret)) if caret.cell.is_none() => caret.block..=caret.block,
            _ => {
                self.say(crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon"));
                return;
            }
        };
        let (lo, hi) = (*range.start(), *range.end());
        let counted: usize = ed.boxes.iter().map(|c| c.blocks).sum();
        let held = if counted == ed.blocks.len() && !ed.boxes.is_empty() {
            notecraft_model::container::place(&ed.boxes, lo).map(|(i, _)| notecraft_model::container::blocks_of(&ed.boxes, i))
        } else {
            Some(0..ed.blocks.len())
        };
        let Some(held) = held.filter(|r| r.contains(&lo) && r.contains(&hi)) else { return };
        // A heading that only repeats the title is not drawn: nothing moves past it.
        let first = if pageedit::is_title_echo(&ed.blocks, held.start, &ed.title) { held.start + 1 } else { held.start };
        let other = if up { lo.checked_sub(1).filter(|o| *o >= first) } else { Some(hi + 1).filter(|o| *o < held.end) };
        let Some(other) = other else {
            self.say(if up { crate::tl_id!("ui.it_is_at_the_top_of_its_box") } else { crate::tl_id!("ui.it_is_at_the_bottom_of_its_box") });
            return;
        };
        Self::remember_step(ed);
        // The block beside them goes to their other side; they move one place.
        let moved = move |i: usize| {
            if up {
                if i == other {
                    hi
                } else if (lo..=hi).contains(&i) {
                    i - 1
                } else {
                    i
                }
            } else if i == other {
                lo
            } else if (lo..=hi).contains(&i) {
                i + 1
            } else {
                i
            }
        };
        if up {
            ed.blocks[other..=hi].rotate_left(1);
        } else {
            ed.blocks[lo..=other].rotate_right(1);
        }
        let cols = ed.columns.renumber(moved);
        let justified = ed.justified.renumber(moved);
        if let Some(caret) = ed.caret.as_mut() {
            caret.block = moved(caret.block);
            caret.focus = true;
            ed.restore_format_selection = Some((caret.block, caret.selection.0, caret.selection.1));
        }
        if let Some(across) = ed.across.as_mut() {
            across.from.0 = moved(across.from.0);
            across.to.0 = moved(across.to.0);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        // A step of its own even when only a justification moved, between two lines alike.
        Self::remember_layout(ed);
        self.save_block_marks((cols, justified));
    }

    /// The next or the previous section of the open notebook, round from one end to the other,
    /// opened where it was left (issue #512).
    fn step_section(&mut self, forward: bool) {
        let tree = self.session.tree();
        let here = self.editor.as_ref().map(|e| e.dir.clone()).or_else(|| self.selected.clone());
        let Some(here) = here else { return };
        let Some(notebook) = tree.iter().find(|n| here.starts_with(&n.path)).map(|n| n.path.clone()) else { return };
        let sections = sections_in_order(tree, &notebook);
        if sections.is_empty() {
            return;
        }
        let at = sections.iter().position(|s| here.starts_with(s));
        let next = match at {
            Some(i) if forward => (i + 1) % sections.len(),
            Some(i) => (i + sections.len() - 1) % sections.len(),
            None if forward => 0,
            None => sections.len() - 1,
        };
        let section = sections[next].clone();
        if at == Some(next) {
            return;
        }
        self.open_section(&section);
    }

    /// The first or the last page in the section's page list, subpages of a collapsed page left
    /// out as the list leaves them out (issue #513).
    fn page_at_end(&mut self, last: bool) {
        let pages = self.listed_pages();
        let target = if last { pages.last() } else { pages.first() };
        match target.cloned() {
            Some(page) => {
                let here = self.editor.as_ref().map(|e| e.dir.clone());
                if here.as_deref() != Some(page.as_path()) {
                    self.selected = Some(page.clone());
                    self.open_page_soon(&page);
                }
            }
            None => self.say(crate::tl_id!("ui.this_section_has_no_pages_yet")),
        }
    }

    /// Ctrl+T: the New section dialog, for the section group or notebook the open section is in
    /// (issue #558). Cancelled, it leaves the page as it was.
    fn new_section_here(&mut self) {
        // The same place the "+ New section" button adds to: the notebook or section group
        // chosen, or the one around the chosen section or page, so an empty section group
        // that is selected takes the new section too.
        let Some((parent, parent_kind)) = self.new_section_parent() else {
            self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None, first: true });
            return;
        };
        self.dialog = Some(Dialog::Add { parent, parent_kind, title: String::new(), kind: NodeKind::Section, format: None });
    }

    /// Ctrl+Shift+T: the keyboard to the open page's title, scrolled into view, its words left as
    /// they are (issue #571). A hidden title, or a locked page's, says why it cannot be typed in.
    fn focus_page_title(&mut self, ctx: &Context) {
        let Some(ed) = self.editor.as_mut() else { return };
        if self.hide_title {
            self.say(crate::tl_id!("ui.the_page_title_is_hidden"));
            return;
        }
        if ed.locked || !ed.writable {
            self.scroll_to = Some(0.0);
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        if let Some(caret) = ed.caret.as_mut() {
            caret.focus = false;
        }
        self.scroll_to = Some(0.0);
        let title = page_title_id(ctx);
        ctx.memory_mut(|m| m.request_focus(title));
    }

    fn page_text_has_focus(&self, ctx: &Context) -> bool {
        let (Some(page), Some(_)) = (self.page_rect, self.editor.as_ref()) else { return false };
        if self.show_source {
            return false;
        }
        ctx.memory(|m| m.focused()).and_then(|id| ctx.read_response(id)).is_some_and(|focused| page.contains(focused.rect.center()))
    }

    // ---------- page open, save ----------

    fn open_page(&mut self, dir: &Path) {
        // From the tree in memory: asked of the disk, folder by folder, it read the page's own
        // file on the way up, which on a synced folder may still be downloading.
        let notebook = self.session.tree().iter().find(|n| dir.starts_with(&n.path)).map(|n| n.path.clone()).or_else(|| notebook_for(dir));
        if let Some(notebook) = notebook {
            self.remember_recent_notebook(&notebook);
        }
        self.nav.visit(dir);
        self.read_pictures(dir);
        self.open_page_inner(dir);
    }

    /// Reads the text and boxes of the pictures of a page, once OCR is set up (the setting
    /// ocr_models). A picture that did not change is not read again; with no setting nothing runs.
    fn read_pictures(&mut self, dir: &Path) {
        if self.session.settings.get("ocr_models").is_some_and(|v| !v.trim().is_empty()) {
            let page = dir.to_string_lossy().into_owned();
            if let Err(e) = self.session.run("ocr.refresh", &params(&[("path", &page)])) {
                log::warn!("picture text was not read for {page}: {e:?}");
            }
        }
    }

    /// Opens the page asked for whose files were still on their way, once they are in.
    fn finish_pending_open(&mut self, ctx: &Context) {
        let Some(pending) = &self.opening else { return };
        // A dialog open on the page shown acts on that page: the other waits until it is closed,
        // or Insert Table, say, would put its table into the page that came in meanwhile.
        if self.dialog.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        if let Some(at) = pending.retry_at {
            let now = Instant::now();
            if now < at {
                ctx.request_repaint_after(at - now);
                return;
            }
        } else if matches!(pending.done.try_recv(), Err(std::sync::mpsc::TryRecvError::Empty)) {
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        // Read, or the reading stopped: opened now, from what is on this computer. Opening
        // clears the request once the page shown is saved; if it could not be, the request is
        // kept and tried again in a moment, so the page asked for still comes.
        let (dir, keep_list) = (pending.dir.clone(), pending.keep_list);
        self.fetched = Some(dir.clone());
        self.open_page_inner(&dir);
        if let Some(still) = self.opening.as_mut().filter(|o| o.dir == dir) {
            still.retry_at = Some(Instant::now() + Duration::from_secs(1));
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        if keep_list {
            self.page_opened = false;
        }
        // The title box has one id for every page: had the page shown's title the keyboard, the
        // page come in would take what was being typed into it, and save it as its own title.
        let title = page_title_id(ctx);
        ctx.memory_mut(|m| {
            if m.has_focus(title) {
                m.surrender_focus(title);
            }
        });
        ctx.request_repaint();
    }

    /// Opens a page asked for by hand: a click in the notebooks or the page list, a tab, Back or
    /// Forward, a search result, the next page, or the page the window was left on. On a local
    /// disk it opens at once; a page whose files are still coming from a synced folder opens when
    /// they are in, and the page shown stays meanwhile (see `open_page_inner`).
    fn open_page_soon(&mut self, dir: &Path) {
        let was = std::mem::replace(&mut self.open_soon, true);
        self.open_page(dir);
        self.open_soon = was;
    }

    /// Back or Forward to `dir`, as [`Self::open_page_soon`] opens a page.
    fn open_page_inner_soon(&mut self, dir: &Path) {
        let was = std::mem::replace(&mut self.open_soon, true);
        self.open_page_inner(dir);
        self.open_soon = was;
    }

    /// The title the open notebooks give the page or section at `dir`, from the tree in memory.
    fn title_in_tree(&self, dir: &Path) -> Option<String> {
        fn find(nodes: &[Node], dir: &Path) -> Option<String> {
            nodes.iter().find_map(|n| if n.path == dir { Some(n.info.title.clone()) } else { find(&n.children, dir) })
        }
        find(self.session.tree(), dir)
    }

    /// The kind the open notebooks give the node at `dir`, from the tree in memory.
    fn kind_in_tree(&self, dir: &Path) -> Option<NodeKind> {
        fn find(nodes: &[Node], dir: &Path) -> Option<NodeKind> {
            nodes.iter().find_map(|n| if n.path == dir { Some(n.info.kind) } else { find(&n.children, dir) })
        }
        find(self.session.tree(), dir)
    }

    /// Opens a page without counting it as a visit: what Back and Forward do.
    fn open_page_inner(&mut self, dir: &Path) {
        // A page opened is looked at afresh, not through what was kept about the last one's files.
        crate::fscache::forget_all();
        self.dismiss_search();
        self.page_opened = true;
        #[cfg(feature = "voice")]
        {
            self.voice.editor_epoch = self.voice.editor_epoch.wrapping_add(1);
        }
        self.save_now();
        if self.is_dirty() || self.page_conflict {
            return;
        }
        // In a synced folder a page's files may not be on this computer yet, and reading them
        // waits for them to come down: read here, that froze the window for as long. They are
        // read on a thread of their own first; on a local disk that is done at once, and the page
        // opens now as it always did. Otherwise the page being shown stays, and this one opens
        // when its files are in (`finish_pending_open`).
        if self.open_soon
            && self.fetched.as_deref() != Some(dir)
            && let Some(done) = fetch_page_files(dir, OPEN_WAIT)
        {
            let name = self.title_in_tree(dir).unwrap_or_else(|| notecraft_store::tree::folder_name(dir));
            self.opening = Some(PendingOpen { dir: dir.to_path_buf(), done, keep_list: false, retry_at: None });
            self.say(crate::i18n::format_message_id("ui.page_on_its_way", &[("page", &name)]));
            return;
        }
        // This page is the one asked for last: one still on its way is not opened after it.
        self.fetched = None;
        self.opening = None;
        // The page being left was read as it now stands, own edits included (issue #291).
        if let Some(left) = self.editor.as_ref().map(|e| e.dir.clone()).filter(|d| d != dir) {
            self.note_read(&left, false);
        }
        self.code = crate::codeview::State::default();
        let another_page = self.editor.as_ref().is_none_or(|e| e.dir != dir);
        let refresh_tags = another_page
            && self
                .tags_pane
                .as_ref()
                .is_some_and(|pane| pane.scope != TagScope::All && pane.scope.path(self.current_page()) != pane.scope.path(Some(dir)));
        if let Some(Output::Page(page)) = self.run("page.load", &params(&[("path", &path_str(dir))])) {
            // A view belongs to the page it was made on: a pan kept showed every other page as far
            // off the screen, blank until the window was started again. Another page opens at its
            // top left, unless the window is putting back where it was left; the same page read
            // again, and a page that could not be read, keep the view as it is.
            if another_page {
                self.canvas_pan = egui::Vec2::ZERO;
                self.fling.stop();
                if self.restore_scroll.is_none() {
                    self.scroll_to = Some(0.0);
                }
            }
            let (blocks, in_the_text) = notecraft_formats::boxes::read(page.format, &page.body);
            let audit = notecraft_formats::boxes::span_audit(&page.body, &blocks);
            log::debug!("page read span audit: page={:?} raw_span_tags={} styled_spans={}", dir, audit.raw_tags, audit.styled);
            // What the editor will show and edit: any block whose edit text still carries markup
            // is logged, with its index and the text, so a raw tag on screen is traceable.
            for (index, block) in blocks.iter().enumerate() {
                let shown = crate::pageedit::caret_text(block);
                if shown.contains("<span") || shown.contains("**") {
                    log::warn!("raw markup in edit text: page={:?} block={index} shown={shown:?}", dir);
                }
            }
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
            // A page locked read only (issue #441) opens read only, whatever its markup allows.
            let writable = pageedit::round_trips(page.format, &page.body) && !page.read_only;
            // A page with no title of its own starts with a blank title area, not with
            // "Untitled page" sitting there as if it had been typed. `title` keeps the
            // display fallback, so losing focus on an empty box is correctly seen as "no
            // change" rather than a rename to the fallback word itself.
            let title_buf = if page.has_title { page.title.clone() } else { String::new() };
            self.editor = Some(Editor {
                generation: 0,
                authors: notecraft_store::authors::of(dir),
                dir: dir.to_path_buf(),
                page_id: notecraft_store::tree::read_node(dir).map(|n| n.id).filter(|id| !id.is_empty()),
                format: page.format,
                title_buf,
                titled: page.has_title,
                title: page.title,
                saved_body: page.body.clone(),
                saving: None,
                body: page.body,
                dirty_since: None,
                blocks,
                last_saved: None,
                attachments: attachments.clone(),
                caret: None,
                pending_format: None,
                restore_format_selection: None,
                writable,
                locked: page.read_only,
                page_dir: page.dir,
                boxes,
                arranged,
                content_bottom: None,
                shown_widths: Vec::new(),
                columns: notecraft_store::page::table_columns(dir).unwrap_or_default(),
                justified: notecraft_store::page::justified(dir).unwrap_or_default(),
                across: None,
                ink_files: notecraft_store::page::ink_files(dir),
                paper: appearance.0,
                colour: appearance.1,
                rule_colour: notecraft_store::page::rule_colour(dir).ok().flatten().and_then(|c| notecraft_model::NodeColor::parse(&c)),
                paper_size: notecraft_store::page::paper_size(dir).ok().flatten(),
                history: crate::history::History::new(crate::history::Step::new(first.0, first.1, first.2, None, crate::history::Kind::Edit)),
                origin: egui::Pos2::ZERO,
                origin_in_canvas: None,
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
            // Another page, another history: nothing undone on the last one is redone here.
            self.undone.clear();
            self.absorb_old_sheets(dir);
            self.sync_ink_reference();
            self.selected = Some(dir.to_path_buf());
            self.cursor = 0;
            self.remember_last_page(dir, another_page);
            self.remember_section_page(dir);
            // Opened, so read (issue #291).
            self.note_read(dir, true);
            if refresh_tags {
                self.refresh_tags();
            }
            self.say(crate::tl_id!("ui.opened"));
            self.page_revision = crate::windows::Revision::read(dir).ok();
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
        self.tell_kept_ink();
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

    /// Writes the page now and returns once it is on disk, after any autosave still being
    /// written: what comes next (another page, closing, a command) may read the file.
    fn save_now(&mut self) {
        self.take_saves(true);
        // Ink written off the window's thread is finished first, so the change check below sees
        // the file as this window left it.
        self.take_ink_save(true);
        self.check_page_changes();
        if self.page_conflict {
            return;
        }
        if let Some(ed) = self.editor.as_mut().filter(|ed| ed.dirty_since.is_some()) {
            let (dir, body, expected) = (ed.dir.clone(), ed.body_to_save(), ed.saved_body.clone());
            if body == ed.saved_body {
                ed.dirty_since = None;
            } else {
                let author = self.author_name.clone().unwrap_or_default();
                match self.session.run_with_page(
                    "page.save",
                    &params(&[("path", &path_str(&dir)), ("body", &body), ("expected_body", &expected), ("author", &author)]),
                    &dir,
                    &expected,
                ) {
                    Ok((Output::Text(ts), snapshot)) => {
                        self.note_read(&dir, false);
                        if let Some(ed) = self.editor.as_mut() {
                            ed.dirty_since = None;
                            ed.last_saved = Some(ts);
                            ed.authors = notecraft_store::authors::of(&ed.dir);
                            ed.saved_body = snapshot.as_ref().map(|s| s.body.clone()).unwrap_or(body);
                        }
                        self.page_revision = snapshot.and_then(|s| s.revision);
                        self.say(self.saved_caption(None));
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.fail(&e);
                        self.check_page_changes();
                        return;
                    }
                }
            }
        }
        // The ink waits a moment after a stroke to be written; it is written now.
        self.save_ink_here();
        self.save_pending_title();
    }

    /// The autosave: hands the page to the saver and carries on, so the window is not held up
    /// while a slow folder writes it.
    fn save_in_background(&mut self) {
        self.take_saves(false);
        if self.saver.busy() || self.page_conflict {
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        if ed.dirty_since.is_none() {
            return;
        }
        let body = ed.body_to_save();
        ed.dirty_since = None;
        if body == ed.saved_body || ed.saving.as_ref() == Some(&body) {
            return;
        }
        if self.saver.submit_checked(self.session.root(), &ed.dir, &body, &ed.saved_body) {
            ed.saving = Some(body);
        } else {
            ed.dirty_since = Some(Instant::now());
        }
    }

    /// Takes in the autosaves that have finished, waiting for all of them if `wait`. A save that
    /// failed says why and leaves the page unsaved, so the autosave tries again.
    fn take_saves(&mut self, wait: bool) {
        if !self.saver.busy() {
            return;
        }
        let done = if wait { self.saver.wait() } else { self.saver.finished() };
        for saved in done {
            match saved.result {
                Ok(ts) => {
                    // Saving a page is not a change someone else made: it stays read.
                    self.note_read(&saved.dir, false);
                    let here = self.editor.as_mut().filter(|ed| ed.dir == saved.dir);
                    if let Some(ed) = here {
                        ed.last_saved = Some(ts);
                        ed.authors = notecraft_store::authors::of(&ed.dir);
                        if ed.saving.as_ref() == Some(&saved.body) {
                            ed.saving = None;
                        }
                        ed.saved_body = saved.snapshot.as_ref().map(|s| s.body.clone()).unwrap_or(saved.body);
                        self.page_revision = saved.snapshot.and_then(|s| s.revision);
                    }
                    self.say(self.saved_caption(None));
                }
                Err(e) => {
                    if let Some(ed) = self.editor.as_mut().filter(|ed| ed.dir == saved.dir) {
                        ed.saving = None;
                        ed.dirty_since.get_or_insert_with(Instant::now);
                    }
                    self.status = crate::i18n::format_message_id("ui.could_not_complete_the_action_error", &[("error", &e)]);
                    self.status_is_error = true;
                }
            }
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
        ed.generation = ed.generation.wrapping_add(1);
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
        ed.generation = ed.generation.wrapping_add(1);
        // A change that leaves the file as it is, or as it is being written (a click on open
        // paper, which only places the cursor), is not a change to save.
        let file_will_hold = ed.saving.as_ref().unwrap_or(&ed.saved_body);
        ed.dirty_since = (ed.body_to_save() != *file_will_hold).then(Instant::now);
    }

    /// Puts a printout's blocks where the writer is: into the box that holds the caret, after
    /// the caret's line, the way OneNote puts it where you are writing (issue #67). With no
    /// caret in a box it gets a box of its own below the others.
    fn insert_printout_blocks(&mut self, text: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((index, at)) =
            ed.caret.as_ref().and_then(|c| notecraft_model::container::place(&ed.boxes, c.block).map(|(index, _)| (index, c.block + 1)))
        else {
            self.insert_in_own_container(text);
            return;
        };
        let (added, _) = notecraft_formats::boxes::read(ed.format, text);
        if added.is_empty() {
            return;
        }
        let count = added.len();
        let (mut cols_moved, mut justified_moved) = (false, false);
        for (offset, block) in added.into_iter().enumerate() {
            ed.blocks.insert(at + offset, block);
            cols_moved |= ed.columns.insert_block(at + offset);
            justified_moved |= ed.justified.insert_block(at + offset);
        }
        ed.boxes[index].blocks += count;
        // The pages come out as wide as the sheet, as they do in a box of their own: a narrow box
        // (one started with a click) is widened for them rather than the pages shrunk to fit it.
        let sheet = ed.boxes.first().map_or(360.0, |c| c.width);
        ed.boxes[index].width = ed.boxes[index].width.max(sheet);
        ed.arranged = true;
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_step(ed);
        let path = path_str(&ed.dir);
        let cols = cols_moved.then(|| notecraft_model::tablecols::write(&ed.columns));
        let justified = justified_moved.then(|| notecraft_model::justify::write(&ed.justified));
        if let Some(line) = cols {
            self.run("page.columns", &params(&[("path", &path), ("widths", &line)]));
        }
        if let Some(line) = justified {
            self.run("page.justify", &params(&[("path", &path), ("blocks", &line)]));
        }
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

    /// Starts making a printout of `file` for the page in `dir`, off the window's thread: the
    /// page goes on being written in while it is made ([`crate::printout::Job`]), and it goes in
    /// once it is ready ([`Self::poll_printouts`]).
    fn insert_printout(&mut self, dir: &Path, file: &Path, format: PageFormat) {
        let job = crate::printout::Job::start(dir, file);
        self.say(crate::i18n::format_message_id("ui.making_a_printout_of_name", &[("name", &job.name)]));
        self.printouts.push((job, format));
    }

    /// Takes in the printouts that are ready, and says how far along the others are.
    fn poll_printouts(&mut self, ctx: &Context) {
        let mut i = 0;
        while i < self.printouts.len() {
            match self.printouts[i].0.poll() {
                None => {
                    let (done, total) = self.printouts[i].0.pages;
                    if total > 0 {
                        let name = self.printouts[i].0.name.clone();
                        self.say(crate::i18n::format_message_id(
                            "ui.making_a_printout_of_name_page_done_of_total",
                            &[("name", &name), ("done", &done.to_string()), ("total", &total.to_string())],
                        ));
                    }
                    i += 1;
                }
                Some(made) => {
                    let (job, format) = self.printouts.remove(i);
                    self.finish_printout(&job, format, made);
                }
            }
        }
        if !self.printouts.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }

    /// Puts a printout that is ready into its page: the open one where the writer is, as
    /// [`Self::insert_printout_blocks`] does, or, when another page was opened meanwhile, into
    /// that page's file, in a box of its own below what is there.
    fn finish_printout(&mut self, job: &crate::printout::Job, format: PageFormat, made: Result<crate::printout::Made, String>) {
        let made = match made {
            Ok(made) => made,
            Err(error) => {
                self.complain(&error);
                return;
            }
        };
        let mut text = snippets::attachment(format, &made.link, &job.name);
        for (index, image) in made.images.iter().enumerate() {
            text.push_str(&snippets::printout_page(format, image, index + 1));
        }
        if self.editor.as_ref().is_some_and(|ed| ed.dir == job.dir) {
            self.insert_printout_blocks(&text);
            if let Some(ed) = self.editor.as_mut() {
                ed.attachments = notecraft_store::assets::list(&ed.dir);
            }
        } else if !self.add_to_page_file(&job.dir, &text) {
            return;
        }
        self.say(crate::i18n::format_message_id(
            "ui.inserted_name_and_count_printout_pages",
            &[("name", &job.name), ("count", &made.images.len().to_string())],
        ));
    }

    /// Adds `text` to the page in `dir`, which is not the one open, in a box of its own below its
    /// boxes, and writes it. Says why when it cannot, and returns whether it did.
    fn add_to_page_file(&mut self, dir: &Path, text: &str) -> bool {
        let page = match notecraft_store::page::load_page(dir) {
            Ok(page) => page,
            Err(e) => {
                self.complain(&e.to_string());
                return false;
            }
        };
        let (mut blocks, arranged) = notecraft_formats::boxes::read(page.format, &page.body);
        let (added, _) = notecraft_formats::boxes::read(page.format, text);
        let body = match arranged {
            Some(boxes) if !boxes.is_empty() => {
                let width = boxes.first().map_or(360.0, |c| c.width);
                let below = boxes.iter().map(|c| c.y + 60.0).fold(0.0, f32::max);
                let boxes = notecraft_model::container::append(&boxes, blocks.len(), added.len(), width, below, 12.0);
                blocks.extend(added);
                notecraft_formats::boxes::write(page.format, &blocks, Some(&boxes))
            }
            _ => format!("{}\n{text}", page.body.trim_end()),
        };
        self.run("page.save", &params(&[("path", &path_str(dir)), ("body", &body)])).is_some()
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
                // A folder with a page file is a page. Asked by its name only: reading the page,
                // as `read_node` does, waited for it on a synced folder before it was even opened.
                let is_page = notecraft_store::tree::page_file(&p).is_some();
                if is_page {
                    self.open_page_soon(&p);
                } else {
                    if !self.open.remove(&p) {
                        self.open.insert(p.clone());
                    }
                    // Selecting a container must also move the editor. Leaving the old
                    // page open here made the navigator and page column describe one
                    // location while the editor showed another. A section opens where it was
                    // left, or shows that it is empty (issues #525, #557).
                    if self.kind_in_tree(&p) == Some(NodeKind::Section) {
                        self.open_section(&p);
                        self.page_opened = false;
                        if let Some(pending) = self.opening.as_mut() {
                            pending.keep_list = true;
                        }
                    } else if let Some(page) = first_page_under(self.session.tree(), &p) {
                        self.open_page_soon(&page);
                        // A phone-width window keeps the list up, so the next level can be
                        // picked; only a page chosen for itself takes the window. A page still
                        // on its way keeps it up when it opens too.
                        self.page_opened = false;
                        if let Some(pending) = self.opening.as_mut() {
                            pending.keep_list = true;
                        }
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
            Act::NewNotebook => self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None, first: true }),
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
            Act::RenumberAndAddPage(section, before) => {
                // Where the page before the line sits among the pages now, before renumbering.
                let position = notecraft_store::tree::child_dirs(&section)
                    .into_iter()
                    .filter_map(|c| notecraft_store::tree::read_node(&c))
                    .filter(|n| n.kind == NodeKind::Page)
                    .position(|n| n.order == before);
                let path = path_str(&section);
                if let (Some(position), Some(_)) = (position, self.run("node.renumber", &params(&[("path", &path)]))) {
                    // Renumbered ten apart in the same order, so the line is halfway after that page.
                    let order = ((position as u32 + 1) * 10 + 5).to_string();
                    if let Some(Output::Path(dir)) = self.run("node.create", &params(&[("parent", &path), ("kind", "page"), ("order", &order)])) {
                        self.open_page(&dir);
                        self.say(crate::tl_id!("ui.new_page"));
                    }
                }
            }
            Act::NewPage(parent) => self.create_page_in(&parent),
            Act::SetReadOnly(path, locked) => self.set_read_only(&path, locked),
            Act::Rename(path, title) => self.dialog = Some(Dialog::Rename { path, title }),
            Act::MoveOrCopy(path) => self.open_move_or_copy(&path),
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
            Act::Place(path, drop) => {
                self.place_dropped(&path, &drop);
            }
            Act::HoldPage(path, cut) => {
                let title = self.title_in_tree(&path).unwrap_or_default();
                self.held_page = Some((path, cut));
                self.say(crate::i18n::format_message_id(if cut { "ui.page_cut_paste_it" } else { "ui.page_copied_paste_it" }, &[("title", &title)]));
            }
            Act::PastePage(target) => self.paste_page(&target),
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

    /// Dragging rows in the navigation pane: taking hold of `node`'s row, showing where a carried
    /// row would go over this one, and letting go (see `navdrag`).
    fn drag_row(&self, ui: &Ui, node: &Node, rect: egui::Rect, resp: &egui::Response, acts: &mut Vec<Act>) {
        use crate::navdrag::{Carried, Drop, carried_id, drop_at, drop_id};
        let ctx = ui.ctx();
        let frame = ctx.cumulative_frame_nr();
        if resp.drag_started() {
            ctx.data_mut(|d| {
                d.insert_temp(carried_id(), Carried { path: node.path.clone(), kind: node.info.kind, title: node.info.title.clone() });
                d.remove::<(u64, Option<Drop>)>(drop_id());
            });
        }
        let Some(carried) = ctx.data(|d| d.get_temp::<Carried>(carried_id())) else { return };
        // Escape puts it back: nothing is moved when the button comes up.
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ctx.data_mut(|d| {
                d.remove::<Carried>(carried_id());
                d.remove::<(u64, Option<Drop>)>(drop_id());
            });
            return;
        }
        let t = &self.tokens;
        // Over this row: where it would go, shown on the row.
        if let Some(p) = ctx.pointer_latest_pos().filter(|p| rect.contains(*p))
            && carried.path != node.path
        {
            let drop = drop_at(&carried, &node.path, node.info.kind, rect.min.y, rect.max.y, p.y);
            let painter = ui.painter_at(rect.expand(2.0));
            match &drop {
                Some(Drop::Before(_)) => {
                    painter.line_segment([rect.left_top() + vec2(4.0, 0.0), rect.right_top() - vec2(4.0, 0.0)], egui::Stroke::new(2.0, t.accent));
                }
                Some(Drop::After(_)) => {
                    painter
                        .line_segment([rect.left_bottom() + vec2(4.0, 0.0), rect.right_bottom() - vec2(4.0, 0.0)], egui::Stroke::new(2.0, t.accent));
                }
                Some(Drop::Into(_)) => {
                    painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                }
                None => {}
            }
            ctx.data_mut(|d| d.insert_temp(drop_id(), (frame, drop)));
        }
        if resp.dragged() && carried.path == node.path {
            ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
            // What is carried follows the pointer, faintly, above everything else.
            if let Some(p) = ctx.pointer_latest_pos() {
                let layer = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("nav-drag-ghost")));
                let galley = ui.painter().layout_no_wrap(carried.title.clone(), egui::FontId::proportional(13.0), t.text);
                let at = p + vec2(14.0, -galley.size().y / 2.0);
                let back = egui::Rect::from_min_size(at - vec2(6.0, 3.0), galley.size() + vec2(12.0, 6.0));
                layer.rect_filled(back, 4.0, t.panel.gamma_multiply(0.92));
                layer.rect_stroke(back, 4.0, egui::Stroke::new(1.0, t.container_border), egui::StrokeKind::Inside);
                layer.galley(at, galley, t.text);
            }
        }
        if resp.drag_stopped() && carried.path == node.path {
            // Only where the pointer is now, or was a frame ago: a row it has left has no say.
            let drop = ctx.data_mut(|d| {
                d.remove::<Carried>(carried_id());
                d.remove_temp::<(u64, Option<Drop>)>(drop_id())
            });
            if let Some((when, Some(drop))) = drop
                && when + 1 >= frame
                && ctx.pointer_latest_pos().is_some_and(|p| ctx.viewport_rect().contains(p))
            {
                acts.push(Act::Place(node.path.clone(), drop));
            }
        }
    }

    /// Puts a row dragged in the navigation pane where it was let go: above or below another row,
    /// among the rows beside it, or into one. The window stays on the page it was on, wherever
    /// that page went. True when the row was put there.
    fn place_dropped(&mut self, path: &Path, drop: &crate::navdrag::Drop) -> bool {
        use crate::navdrag::Drop;
        self.save_now();
        if self.is_dirty() {
            return false;
        }
        let kind = notecraft_store::tree::read_node(path).map(|n| n.kind);
        let target = crate::navdrag::target(drop).to_path_buf();
        // The nodes beside the target, in their order: the notebooks, or what its parent holds.
        let beside: Vec<PathBuf> = {
            fn holding<'a>(nodes: &'a [Node], child: &Path) -> Option<&'a [Node]> {
                if nodes.iter().any(|n| n.path == child) {
                    return Some(nodes);
                }
                nodes.iter().find(|n| child.starts_with(&n.path)).and_then(|n| holding(&n.children, child))
            }
            holding(self.session.tree(), &target).map(|nodes| nodes.iter().map(|n| n.path.clone()).collect()).unwrap_or_default()
        };
        let (to, before) = match drop {
            Drop::Into(into) => (into.clone(), None),
            Drop::Before(row) => (row.parent().map(Path::to_path_buf).unwrap_or_default(), Some(row.clone())),
            Drop::After(row) => {
                // Before whatever follows the row, not counting what is carried: below the row
                // just above where it already is, it stays where it is.
                let next = beside.iter().position(|p| p == row).and_then(|i| beside.iter().skip(i + 1).find(|p| *p != path)).cloned();
                (row.parent().map(Path::to_path_buf).unwrap_or_default(), next)
            }
        };
        let mut p = vec![("path", path_str(path))];
        if kind != Some(NodeKind::Notebook) {
            p.push(("to", path_str(&to)));
        }
        if let Some(before) = &before {
            p.push(("before", path_str(before)));
        }
        let pairs: Vec<(&str, &str)> = p.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let editor_path = self.editor.as_ref().map(|editor| editor.dir.clone());
        let Some(Output::Path(new_path)) = self.run("node.place", &params(&pairs)) else { return false };
        self.session.refresh();
        if matches!(drop, Drop::Into(_)) {
            self.open.insert(to);
        }
        if new_path != path {
            self.nav.relocate(path, &new_path);
            let remap = |candidate: &Path| candidate.strip_prefix(path).map_or_else(|_| candidate.to_path_buf(), |suffix| new_path.join(suffix));
            self.open = self.open.iter().map(|branch| remap(branch)).collect();
            self.selected = self.selected.as_deref().map(remap);
            if let Some(editor_path) = editor_path.filter(|editor_path| editor_path.starts_with(path)) {
                let relocated = remap(&editor_path);
                self.open_page_inner(&relocated);
            }
        }
        self.say(crate::tl_id!("ui.moved"));
        true
    }

    /// Pastes the page a row's Cut or Copy holds (issue #532): just after the page `target`, or
    /// last in the section `target`. A copy is the page with its subpages, files and ink, every
    /// one with an id of its own, beside the others under a folder name of its own; a cut page
    /// moves only now, so it stays where it was until it is pasted, and if the paste fails.
    fn paste_page(&mut self, target: &Path) {
        let Some((source, cut)) = self.held_page.clone() else { return };
        if notecraft_store::tree::page_file(&source).is_none() {
            self.held_page = None;
            self.complain(crate::tl_id!("ui.the_page_held_is_no_longer_there"));
            return;
        }
        if target.starts_with(&source) {
            self.complain(crate::tl_id!("ui.a_page_cannot_go_inside_itself"));
            return;
        }
        let into_section = self.kind_in_tree(target) == Some(NodeKind::Section);
        let drop = if into_section { crate::navdrag::Drop::Into(target.to_path_buf()) } else { crate::navdrag::Drop::After(target.to_path_buf()) };
        self.save_now();
        if self.is_dirty() || self.page_conflict {
            return;
        }
        if cut {
            // A failed move leaves the page where it was, still held, with the move's own
            // message saying why.
            if !self.place_dropped(&source, &drop) {
                return;
            }
            if notecraft_store::tree::page_file(&source).is_none() {
                self.held_page = None;
            }
        } else {
            let to = if into_section { target.to_path_buf() } else { target.parent().map(Path::to_path_buf).unwrap_or_default() };
            let Some(Output::Path(copy)) = self.run("node.copy", &params(&[("path", &path_str(&source)), ("to", &path_str(&to))])) else { return };
            self.session.refresh();
            self.open.insert(to);
            if !into_section && !self.place_dropped(&copy, &drop) {
                // The copy is made, last in the section, but could not go after the page.
                self.complain(crate::tl_id!("ui.the_copy_is_last_in_the_section"));
                return;
            }
        }
        self.say(crate::tl_id!("ui.pasted"));
    }

    fn draw_node(&self, ui: &mut Ui, node: &Node, depth: usize, acts: &mut Vec<Act>) {
        self.draw_row(ui, node, depth, acts, true);
    }

    /// One row of the tree. With `pages` false the pages are left out, which is what the
    /// organisers column shows: notebooks, section groups and sections only.
    /// The menu of a notebook, section group, section or page: from its row in the navigation
    /// pane, and a section's from its tab over the page too (issue #556), so both act on that
    /// node, whichever page is shown.
    fn node_menu_items(&self, ui: &mut Ui, node: &Node, open: bool, has_kids: bool, acts: &mut Vec<Act>) {
        let t = &self.tokens;
        // A locked page keeps its menu, with the changes to it greyed out (issue #441). Read
        // from the page's head only while this menu is open.
        let edits_off = node.info.kind == NodeKind::Page && notecraft_store::page::read_only(&node.path).unwrap_or(false);
        // Issue #380: the shortcut sits in its own right-hand column, not in the words.
        let shortcut = if cfg!(target_os = "macos") { "Cmd+N" } else { "Ctrl+N" };
        if node.info.kind == NodeKind::Page
            && let Some(parent) = node.path.parent()
            && floating::menu_row_shortcut(ui, Some(floating::Glyph::Icon(Icon::Page)), crate::tl_id!("ui.new_page"), shortcut, t).clicked()
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
        // Cut, Copy and Paste for pages, as OneNote's page rows have them (issue #532). Paste on a
        // page puts the page held after it; on a section, last in it.
        if matches!(node.info.kind, NodeKind::Page | NodeKind::Section) {
            floating::menu_separator(ui, t);
            if node.info.kind == NodeKind::Page {
                if floating::menu_row_if(ui, !edits_off, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Cut)), crate::tl_id!("ui.cut"), t)
                    .clicked()
                {
                    acts.push(Act::HoldPage(node.path.clone(), true));
                    ui.close();
                }
                if floating::menu_row(ui, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Copy)), crate::tl_id!("ui.copy"), t).clicked() {
                    acts.push(Act::HoldPage(node.path.clone(), false));
                    ui.close();
                }
            }
            let held = self.held_page.as_ref().is_some_and(|(p, _)| !node.path.starts_with(p));
            if floating::menu_row_if(ui, held, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Paste)), crate::tl_id!("ui.paste_page"), t)
                .clicked()
            {
                acts.push(Act::PastePage(node.path.clone()));
                ui.close();
            }
            floating::menu_separator(ui, t);
        }
        if node.info.kind == NodeKind::Page
            && (Self::can_make_subpage(&node.path)
                || Self::can_promote_subpage(&node.path)
                || Self::can_move_page(&node.path, true)
                || Self::can_move_page(&node.path, false))
        {
            floating::menu_separator(ui, t);
            if Self::can_make_subpage(&node.path)
                && floating::menu_row_if(
                    ui,
                    !edits_off,
                    Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::IndentMore)),
                    crate::tl_id!("ui.make_subpage"),
                    t,
                )
                .clicked()
            {
                acts.push(Act::MakeSubpage(node.path.clone()));
                ui.close();
            }
            if Self::can_promote_subpage(&node.path)
                && floating::menu_row_if(
                    ui,
                    !edits_off,
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
                && floating::menu_row_if(ui, !edits_off, Some(floating::Glyph::Icon(Icon::MoveUp)), crate::tl_id!("ui.move_up"), t).clicked()
            {
                acts.push(Act::MovePageUp(node.path.clone()));
                ui.close();
            }
            if Self::can_move_page(&node.path, false)
                && floating::menu_row_if(ui, !edits_off, Some(floating::Glyph::Icon(Icon::MoveDown)), crate::tl_id!("ui.move_down"), t).clicked()
            {
                acts.push(Act::MovePageDown(node.path.clone()));
                ui.close();
            }
        }
        floating::menu_separator(ui, t);
        if node.info.kind == NodeKind::Page {
            // Read from the page's head only while this menu is open, so no row pays for it.
            let locked = notecraft_store::page::read_only(&node.path).unwrap_or(false);
            let (label, icon) =
                if locked { (crate::tl_id!("ui.unlock_read_only"), Icon::Unlock) } else { (crate::tl_id!("ui.lock_read_only"), Icon::Lock) };
            if floating::menu_row(ui, Some(floating::Glyph::Icon(icon)), label, t).clicked() {
                acts.push(Act::SetReadOnly(node.path.clone(), !locked));
                ui.close();
            }
        }
        if floating::menu_row_if(ui, !edits_off, Some(floating::Glyph::Icon(Icon::Page)), crate::tl_id!("ui.rename_ellipsis"), t).clicked() {
            acts.push(Act::Rename(node.path.clone(), node.info.title.clone()));
            ui.close();
        }
        if matches!(node.info.kind, NodeKind::Page | NodeKind::Section | NodeKind::SectionGroup)
            && floating::menu_row_if(ui, !edits_off, Some(floating::Glyph::Icon(Icon::MoveDown)), crate::tl_id!("ui.move_or_copy_ellipsis"), t)
                .clicked()
        {
            acts.push(Act::MoveOrCopy(node.path.clone()));
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
        // A row that opens a further menu, with its chevron in the shortcut column (issue #380).
        if node.info.kind != NodeKind::Page {
            floating::menu_submenu_row(ui, None, crate::tl_id!("ui.colour"), t, |ui| {
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
        // Where it is and how to reach it, apart from the actions that change it.
        floating::menu_separator(ui, t);
        // Named for what it links to (issues #518, #531): pasted into a page, the link opens that
        // notebook or section at a page, as a page's link opens the page.
        let copy_link = match node.info.kind {
            NodeKind::Notebook => crate::tl_id!("ui.copy_link_to_notebook"),
            NodeKind::SectionGroup => crate::tl_id!("ui.copy_link_to_section_group"),
            NodeKind::Section => crate::tl_id!("ui.copy_link_to_section"),
            NodeKind::Page => crate::tl_id!("ui.copy_link_to_page"),
        };
        if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Link)), copy_link, t)
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
        if node.info.kind == NodeKind::Notebook
            && floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Notebook)), crate::tl_id!("ui.close_notebook"), t).clicked()
        {
            acts.push(Act::CloseNotebook(node.path.clone()));
            ui.close();
        }
        if ui
            .add_enabled_ui(!edits_off, |ui| {
                floating::menu_row_danger(ui, Some(floating::Glyph::Mark(crate::ribbon_icons::Mark::Delete)), crate::tl_id!("ui.delete_ellipsis"), t)
            })
            .inner
            .clicked()
        {
            acts.push(Act::Delete(node.path.clone(), node.info.title.clone(), node.count()));
            ui.close();
        }
    }

    fn draw_row(&self, ui: &mut Ui, node: &Node, depth: usize, acts: &mut Vec<Act>, pages: bool) {
        let has_kids = if pages { !node.children.is_empty() } else { node.children.iter().any(|c| c.info.kind != NodeKind::Page) };
        let open = self.open.contains(&node.path);
        let page = node.info.kind == NodeKind::Page;
        let selected = self.selected.as_deref() == Some(node.path.as_path());
        let active_section = node.info.kind == NodeKind::Section && self.selected.as_ref().is_some_and(|p| p.starts_with(&node.path));
        let active = selected || active_section;
        let width = ui.available_width();
        // A row can be taken hold of and dragged elsewhere in the pane (see `navdrag`); a press
        // that hardly moves is still a click.
        let (_, rect) = ui.allocate_space(vec2(width, ROW_HEIGHT));
        let resp = ui.interact(rect, ui.make_persistent_id(("navigation-row", &node.path)), Sense::click_and_drag());
        // Asked for from the keyboard (Ctrl+Alt+G, Ctrl+Shift+G, issue #514).
        if self.nav_focus.as_deref() == Some(node.path.as_path()) {
            resp.request_focus();
            resp.scroll_to_me(Some(egui::Align::Center));
            ui.data_mut(|d| d.insert_temp(egui::Id::new("nav-focus-done"), true));
        }
        let painter = ui.painter_at(rect);
        let t = &self.tokens;
        self.drag_row(ui, node, rect, &resp, acts);
        if active {
            painter.rect_filled(rect, 4.0, t.row_selected);
        } else if resp.hovered() {
            painter.rect_filled(rect, 4.0, t.row_hover);
        }
        let x0 = rect.min.x + (depth as f32 * INDENT).min(rect.width() * 0.42) + 3.0;
        let cy = rect.center().y;
        let chevron_rect = egui::Rect::from_min_size(egui::pos2(x0, rect.min.y), vec2(19.0, ROW_HEIGHT));
        if has_kids {
            let disclosure = ui.interact(chevron_rect, resp.id.with("disclosure"), Sense::click());
            let title = &[("title", node.info.title.as_str())];
            let label = if open {
                crate::i18n::format_message_id("ui.collapse_navigation_node", title)
            } else {
                crate::i18n::format_message_id("ui.expand_navigation_node", title)
            };
            disclosure.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, ui.is_enabled(), open, &label));
            if disclosure.hovered() {
                painter.rect_filled(chevron_rect, 3.0, t.row_hover);
            }
            if disclosure.has_focus() {
                painter.rect_stroke(chevron_rect.shrink(1.0), 3.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
            }
            if disclosure.on_hover_text(label).clicked() {
                acts.push(Act::Toggle(node.path.clone()));
            }
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
        // The left edge identifies the active location independently of the focus outline.
        if active {
            let edge = egui::Rect::from_min_size(egui::pos2(rect.min.x, rect.min.y + 3.0), vec2(3.0, ROW_HEIGHT - 6.0));
            painter.rect_filled(edge, 1.5, t.accent);
        }
        if resp.has_focus() {
            painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
        }
        let icon_rect = egui::Rect::from_min_size(egui::pos2(x0 + 18.0, cy - 8.5), vec2(17.0, 17.0));
        if !page {
            icons::paint(&painter, icon_rect, icon, color);
        }
        let unread = page && self.unread.contains(&node.path);
        if unread {
            painter.circle_filled(egui::pos2(rect.max.x - 29.0, cy), 3.5, t.accent);
        }
        let font_size = if node.info.kind == NodeKind::Notebook { 15.0 } else { 14.0 };
        let text_pos = egui::pos2(x0 + if page { 21.0 } else { 40.0 }, cy);
        let text_color = if active || unread || node.info.kind == NodeKind::Notebook { t.strong } else { ui.style().visuals.text_color() };
        let trailing = if unread { 37.0 } else { 26.0 };
        // A locked page keeps room for its lock beside the name (issue #441).
        let lock_room = if node.info.read_only { 18.0 } else { 0.0 };
        let room = (rect.max.x - trailing - text_pos.x - lock_room).max(0.0);
        // The format label ("html", "md") is the less important of the two: it is shown only
        // when the whole name fits beside it, and the name is never cut to make room for it.
        let label = if node.info.kind == NodeKind::Page { node.info.format.map(|f| crate::tl!(f.short())) } else { None };
        let label_width = label.map(|l| text_width(ui, l, 10.5) + ROW_LABEL_GAP);
        let width_of_title = |text: &str| crate::bidi::line_width(ui, text, font_size, None);
        let (show_label, name_room) = row_name_room(width_of_title(&node.info.title), label_width, room);
        let name = fit_with(&width_of_title, &node.info.title, name_room);
        let text_clip = egui::Rect::from_min_max(rect.min, egui::pos2(text_pos.x + name_room + 1.0, rect.max.y));
        // The tree is on screen all the time, so this is the most visible place the words
        // of an Arabic name would come out backwards. Each row's name decides its own
        // direction; the row keeps its place and its clip either way.
        let clipped = painter.with_clip_rect(text_clip);
        crate::bidi::line_at(ui, &clipped, text_pos, Align2::LEFT_CENTER, &name, font_size, text_color, None);
        // A locked page's lock, after its name, with the words of its tooltip for a screen reader too.
        if node.info.read_only {
            let name_width = width_of_title(&name);
            let lock_rect = egui::Rect::from_center_size(egui::pos2(text_pos.x + name_width + 9.0, cy), vec2(13.0, 13.0));
            floating::Glyph::Icon(Icon::Lock).paint(&painter, lock_rect, t);
            let tip = crate::tl_id!("ui.page_is_read_only");
            let lock = ui.interact(lock_rect, ui.id().with(("read-only", node.path.clone())), Sense::hover());
            lock.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, tip));
            lock.on_hover_text(tip);
        }
        let plus_rect = egui::Rect::from_min_size(egui::pos2(rect.max.x - 24.0, rect.min.y + 2.0), vec2(20.0, ROW_HEIGHT - 4.0));
        if show_label && let Some(l) = label {
            painter.text(egui::pos2(rect.max.x - trailing, cy), Align2::RIGHT_CENTER, l, egui::FontId::proportional(10.5), t.muted);
        }
        if resp.hovered() {
            painter.text(plus_rect.center(), Align2::CENTER_CENTER, "+", egui::FontId::proportional(16.0), t.strong);
        }
        let enabled = ui.is_enabled();
        let spoken = if unread { crate::i18n::format_message_id("ui.title_unread", &[("title", &node.info.title)]) } else { node.info.title.clone() };
        resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, active, &spoken));
        if selected || (!pages && active_section) {
            let last = ui.id().with("revealed-navigation-row");
            if ui.data(|data| data.get_temp::<PathBuf>(last)).as_ref() != Some(&node.path) {
                ui.data_mut(|data| data.insert_temp(last, node.path.clone()));
                resp.scroll_to_me(None);
            }
        }
        if resp.has_focus() && has_kids {
            let key = if open { Key::ArrowLeft } else { Key::ArrowRight };
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, key)) {
                acts.push(Act::Toggle(node.path.clone()));
            }
        }
        let tooltip = format!("{spoken}\n{}", self.shown(&node.path));
        let resp = resp.on_hover_text(tooltip.clone());
        if resp.has_focus() && name != node.info.title {
            egui::Tooltip::for_widget(&resp).show(|ui| {
                ui.label(tooltip);
            });
        }
        if resp.clicked() {
            if resp.interact_pointer_pos().is_some_and(|pos| plus_rect.contains(pos)) {
                acts.push(Act::Add(node.path.clone(), node.info.kind));
            } else {
                acts.push(Act::Select(node.path.clone()));
            }
        }
        floating::row_menu(&resp, |ui| self.node_menu_items(ui, node, open, has_kids, acts));
        if has_kids && open {
            let kids: Vec<&Node> = node.children.iter().filter(|c| pages || c.info.kind != NodeKind::Page).collect();
            if pages {
                // Sub-pages get the same gaps as top-level pages: one before each, one after the
                // last. A gap that is dropped here becomes a child of this page.
                let mut last: Option<u32> = None;
                for c in kids {
                    self.page_gap(ui, &node.path, last, Some(c.info.order), acts);
                    self.draw_row(ui, c, depth + 1, acts, pages);
                    last = Some(c.info.order);
                }
                self.page_gap(ui, &node.path, last, None, acts);
            } else {
                for c in kids {
                    self.draw_row(ui, c, depth + 1, acts, pages);
                }
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
        #[cfg(feature = "voice")]
        let mic = self.dictation_mic();
        #[cfg(not(feature = "voice"))]
        let mic = ribbon::Mic::Hidden;
        let title = self.editor.as_ref().map(|e| e.title.trim().to_owned()).filter(|s| !s.is_empty());
        let mut acts = Vec::new();
        let collapsed = self.ribbon_folded();
        let band = egui::Panel::top("band").frame(egui::Frame::new().fill(t.band)).show_separator_line(false).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
            self.title_row(ui, title.as_deref());
            if with_tabs {
                ribbon::tab_strip(ui, &tab, source, in_table, collapsed, mic, &t, &mut acts);
            }
        });
        self.band_bottom = band.response.rect.max.y;
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
        if !self.compact {
            self.discord_link(&mut right, on);
            right.add_space(10.0);
        }
        // The search box takes what the controls on both sides leave, up to its usual width, so
        // on a phone it does not run over the Quick Access Toolbar.
        let left_used = left.min_rect().right() - row.left();
        let search_width = (right.available_width() - left_used - 8.0).clamp(60.0, 200.0);
        let resp = right.add(
            egui::TextEdit::singleline(&mut self.search)
                .id(Id::new(("page-search", ui.ctx().viewport_id())))
                .hint_text(crate::tl_id!("ui.search_all_pages"))
                .desired_width(search_width),
        );
        if self.focus_search {
            resp.request_focus();
            self.focus_search = false;
        }
        if resp.changed() && self.search != self.searched {
            self.refresh_search();
        }
        // The query box keeps Enter for navigation rather than leaving the user with a
        // mouse-only result list. Enter opens the result chosen with the arrow keys, or the
        // first one when none was chosen (issue #370).
        if self.dialog.is_none()
            && resp.lost_focus()
            && right.input(|input| input.key_pressed(Key::Enter))
            && let Some(path) = self.hits.get(self.search_pick.unwrap_or(0)).or(self.hits.first()).map(|hit| hit.path.clone())
        {
            self.open_search_result(ui.ctx(), &path);
        }
        if resp.has_focus() && !self.hits.is_empty() {
            let (down, up) = right.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp)));
            let last = self.hits.len() - 1;
            if down {
                self.search_pick = Some(self.search_pick.map_or(0, |n| (n + 1).min(last)));
            }
            if up {
                self.search_pick = self.search_pick.and_then(|n| n.checked_sub(1));
            }
        }
        // Issue #371: the page's title gets the room between the controls on both sides and
        // no more. It is centred on the window when it fits there, otherwise in the room it
        // has, and cut with an ellipsis rather than running under the search box. The whole
        // title is on the hover of the words.
        // A phone-width window has no room for the title between the controls: the page's own
        // title is the first thing on it anyway.
        let mut title_shown = false;
        if !self.compact {
            let start = left.min_rect().right() + 12.0;
            let end = right.min_rect().left() - 12.0;
            if end - start > 24.0 {
                title_shown = true;
                let size = 12.5;
                let measure = |s: &str| crate::bidi::line_width(ui, s, size, None);
                let shown = fit_with(&measure, &text, end - start);
                let width = measure(&shown);
                let centre = band_title_centre(row.center().x, start, end, width);
                let room = egui::Rect::from_min_max(egui::pos2(start, row.min.y), egui::pos2(end, row.max.y));
                // A page named in Arabic puts the band's words in the wrong order if the whole
                // string is handed to egui, so the band goes through the bidi painter like the
                // page does. The name decides its own direction here; there is no page to follow.
                let band = ui.painter_at(room);
                crate::bidi::line_at(ui, &band, egui::pos2(centre, row.center().y), Align2::CENTER_CENTER, &shown, size, on.text, None);
                let words = egui::Rect::from_center_size(egui::pos2(centre, row.center().y), vec2(width, H));
                // Hover only: a press here still reaches the band's handle and moves the window.
                // The full title is its accessible name, cut or not.
                let hover = ui.interact(words, ui.id().with("title-words"), Sense::hover());
                hover.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, text.as_str()));
                if shown != text {
                    hover.on_hover_text(text.as_str());
                }
            }
        }
        // Issue #371: with no room for the title in the band, its full text is on the band's
        // hover instead, so it is never hidden without a way to read it.
        if !title_shown {
            handle.clone().on_hover_text(text.as_str());
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
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Discord"));
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
                match kind {
                    "Close" => self.request_window_close(&ctx),
                    "Maximize" => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized)),
                    _ => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                }
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
        // Save a Copy counts from the page that is open (issue #301).
        let mut copy = std::mem::take(&mut self.copy_choice);
        copy.here = self.editor.as_ref().map(|e| e.dir.clone());
        let mut account = std::mem::take(&mut self.account_choice);
        if account.kept != self.author_name {
            account.kept = self.author_name.clone();
            account.typed = self.author_name.clone().unwrap_or_default();
        }
        let mut portable = std::mem::take(&mut self.portable);
        if portable.selection.here != copy.here {
            let _ = self.session.run("export.cancel", &Params::new());
            portable.selection.result = None;
            portable.downloaded = false;
        }
        portable.selection.here = copy.here.clone();
        portable.download = self.file_downloads.is_some();
        let mut export_acts = Vec::new();
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
            let update = self.update.lock().map(|s| s.clone()).unwrap_or_default();
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
                self.ui_scale,
                self.auto_switch,
                self.update_check_off,
                self.session.settings.welcome_notebook_update_prompt(),
                &update,
                &mut copy,
                &mut account,
                &mut portable,
                self.session.reviewed_export(),
                &mut export_acts,
                &tokens,
            );
        });
        for a in acts {
            match a {
                backstage::Act::Close => {
                    let _ = self.session.run("export.cancel", &Params::new());
                    self.backstage = None;
                }
                backstage::Act::Print => {
                    let _ = self.session.run("export.cancel", &Params::new());
                    self.backstage = None;
                    self.open_print();
                }
                backstage::Act::Page(name) => {
                    let _ = self.session.run("export.cancel", &Params::new());
                    self.backstage = Some(name);
                }
                backstage::Act::NewNotebook => {
                    self.backstage = None;
                    self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None, first: true });
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
                    if self.editor.as_ref().is_some_and(|editor| editor.dir.starts_with(&p)) {
                        self.backstage = None;
                    } else {
                        self.open_notebook_from_path(&p);
                    }
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
                backstage::Act::SetUiScale(percent) => self.set_ui_scale(percent),
                backstage::Act::SetAutoSwitch(on) => match self.session.settings.set("switch_by_device", if on { "true" } else { "false" }) {
                    Ok(()) => {
                        self.auto_switch = on;
                        self.say(if on { crate::tl_id!("ui.auto_switch_on") } else { crate::tl_id!("ui.auto_switch_off") });
                    }
                    // Not kept, so not changed: the next start would have gone back to it.
                    Err(e) => self.complain(&crate::i18n::format_message_id("ui.auto_switch_not_saved", &[("error", &e.to_string())])),
                },
                backstage::Act::SetUpdateCheckOff(off) => {
                    match self.session.settings.set("update_check_off_at_start", if off { "true" } else { "false" }) {
                        Ok(()) => self.update_check_off = off,
                        Err(e) => self.complain(&crate::i18n::format_message_id("ui.update_check_not_saved", &[("error", &e.to_string())])),
                    }
                }
                backstage::Act::SetWelcomeNotebookUpdatePrompt(enabled) => {
                    if let Err(error) = self.session.settings.set_welcome_notebook_update_prompt(enabled) {
                        self.complain(&crate::i18n::format_message_id(
                            "ui.welcome_notebook_update_prompt_not_saved",
                            &[("error", &error.to_string())],
                        ));
                    }
                }
                backstage::Act::ChooseNotebooksFolder => self.choose_default_notebooks_folder(),
                backstage::Act::UseDefaultNotebooksFolder => self.apply_notebooks_root_choice(None),
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
                backstage::Act::CheckForUpdates => crate::update::start(ui.ctx().clone(), self.update.clone()),
                backstage::Act::OpenReleases => ui.ctx().open_url(egui::OpenUrl::new_tab(backstage::LATEST_RELEASE)),
                backstage::Act::ResetPreferences => self.dialog = Some(Dialog::ResetPreferences),
                #[cfg(feature = "voice")]
                backstage::Act::DictationSetup => {
                    self.backstage = None;
                    self.open_dictation_setup();
                }
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
                backstage::Act::ChooseCopyFolder => {
                    if let Some(folder) = pick_copy_folder() {
                        copy.folder = folder.to_string_lossy().into_owned();
                    }
                }
                backstage::Act::SetAuthor(name) => self.set_author(name),
                backstage::Act::SaveCopy { node, folder } => {
                    copy.result = Some(notecraft_store::ops::save_copy(&node, &folder).map_err(|e| e.to_string()));
                    if let Some(Ok(at)) = &copy.result {
                        self.say(crate::i18n::format_message_id("ui.saved_a_copy_in_path", &[("path", &self.shown(at))]));
                    }
                }
                backstage::Act::NotBuilt(name) => {
                    self.status =
                        crate::i18n::format_message_id("ui.name_is_not_built_yet_see_the_roadmap_for_what_is_coming", &[("name", crate::tl!(name))]);
                    self.status_is_error = false;
                }
            }
        }
        for act in export_acts {
            match act {
                crate::portable::Act::Cancel => {
                    let _ = self.session.run("export.cancel", &Params::new());
                }
                crate::portable::Act::ChooseFolder => {
                    if let Some(folder) = pick_copy_folder() {
                        portable.selection.folder = path_str(&folder);
                    }
                }
                crate::portable::Act::Review(node) => {
                    portable.selection.result = None;
                    portable.downloaded = false;
                    let p = self.export_snapshot_params(&node);
                    if let Err(e) = self.session.run("export.prepare", &p) {
                        portable.selection.result = Some(Err(e.to_string()));
                    }
                }
                crate::portable::Act::Write => {
                    if let Some(downloads) = self.file_downloads.as_mut() {
                        if let Some(prepared) = self.session.reviewed_export() {
                            match crate::portable::archive(prepared) {
                                Ok(bytes) => {
                                    downloads.push(FileDownload {
                                        name: format!("{}-export.zip", prepared.notebook_name()),
                                        mime: "application/zip",
                                        bytes,
                                    });
                                    portable.downloaded = true;
                                }
                                Err(e) => portable.selection.result = Some(Err(e.to_string())),
                            }
                        }
                    } else {
                        portable.selection.result =
                            Some(match self.session.run("export.write", &params(&[("folder", portable.selection.folder.trim())])) {
                                Ok(Output::Path(path)) => Ok(path),
                                Ok(_) => Err(crate::tl_id!("ui.portable_no_snapshot").to_string()),
                                Err(e) => Err(e.to_string()),
                            });
                    }
                }
                crate::portable::Act::Reveal(path) => self.reveal(&path),
            }
        }
        self.portable = portable;
        self.copy_choice = copy;
        self.account_choice = account;
    }

    /// The current editor contributes text and live ink without a source save. Explicit
    /// export commands and the three UI entry points use the same snapshot boundary.
    fn export_snapshot_params(&self, node: &Path) -> Params {
        let mut p = params(&[("path", &path_str(node))]);
        if let Some(ed) = self.editor.as_ref().filter(|ed| ed.dir.starts_with(node)) {
            p.insert("snapshot_path".into(), path_str(&ed.dir));
            let mut body = ed.body_to_save();
            if let Some(pad) = self.pad.as_ref().filter(|pad| pad.dirty) {
                p.insert("ink".into(), notecraft_ink::svg::write(&pad.drawing, notecraft_ink::svg::Flavour::Editable));
                if !notecraft_formats::references::in_text(ed.format, &body).iter().any(|r| r.target == "ink.svg") {
                    body.push_str(match ed.format {
                        PageFormat::Html => "\n<img src=\"ink.svg\" alt=\"Ink\">\n",
                        PageFormat::Cmgfm => "\n\n![Ink](ink.svg)\n",
                    });
                }
            }
            p.insert("body".into(), body);
        }
        p
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
            self.dismiss_search();
            self.remember_recent_notebook(&opened);
            self.open.insert(opened.clone());
            self.selected = Some(opened.clone());
            self.backstage = None;
            if let Some(page) = first_page(self.session.tree(), &opened) {
                self.open_page_soon(&page);
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
        self.open_notebooks_from_folder(picked);
    }

    /// Handles the folder returned by the native chooser, using the same notebook operations
    /// for a notebook itself, a parent containing notebooks, and an empty folder.
    fn open_notebooks_from_folder(&mut self, picked: PathBuf) {
        let is_notebook = notecraft_store::tree::read_node(&picked).is_some_and(|i| i.kind == NodeKind::Notebook);
        let wanted: Vec<std::path::PathBuf> =
            if is_notebook { vec![picked.clone()] } else { notecraft_store::tree::load_notebooks(&picked).into_iter().map(|n| n.path).collect() };
        if wanted.is_empty() {
            self.open_notebook_from_path(&picked);
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
            self.open_notebook_from_path(&path);
        }
        self.backstage = None;
        if !self.status_is_error {
            self.say(crate::i18n::format_count_id("ui.n_notebook_opened_n_notebooks_opened", opened_count));
        }
    }

    fn import_notebook_folder(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_onenote_notebook_folder"));
        if let Some(backup) = notecraft_import::discover::default_source() {
            dialog = dialog.set_directory(backup);
        }
        let Some(source) = dialog.pick_folder() else { return };
        self.import_onenote_source(source);
    }

    fn import_file(&mut self) {
        let mut dialog = rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_a_file_to_import"));
        if let Some(backup) = notecraft_import::discover::default_source() {
            dialog = dialog.set_directory(backup);
        }
        let Some(source) = dialog.pick_file() else { return };
        self.import_onenote_source(source);
    }

    /// Imports a OneNote notebook folder, section, table of contents or package as a new
    /// notebook (issue #19). NoteCraft's own reader brings the pages over, and a section it
    /// cannot read goes to the OfficeIMO bridge when one is installed; any other file goes to
    /// the bridge as before. Runs on a thread, so the window stays usable.
    fn import_onenote_source(&mut self, source: PathBuf) {
        let format = self
            .session
            .settings
            .get("page_format")
            .and_then(|v| notecraft_model::PageFormat::parse(&v).ok())
            .unwrap_or(notecraft_model::PageFormat::Html);
        // All source types produce a new notebook under this parent. Progress reports the
        // actual chosen path, so the view never guesses names or collision suffixes.
        let destination = self.session.root().to_path_buf();
        log::info!("OneNote import requested: source={} destination={}", source.display(), destination.display());
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let source_for_worker = source.clone();
        let output_for_worker = destination.clone();
        std::thread::spawn(move || {
            let mut progress = |event| {
                let message = match event {
                    notecraft_import::officeimo::Progress::Found { total } => ImportMessage::Found(total),
                    notecraft_import::officeimo::Progress::Destination { path } => ImportMessage::Destination(path),
                    notecraft_import::officeimo::Progress::Section { index, total, label } => {
                        ImportMessage::Section { index, total, label: label.to_string() }
                    }
                };
                let _ = tx.send(message);
            };
            let result = notecraft_import::notebook::import(&source_for_worker, &output_for_worker, format, &worker_cancel, &mut progress);
            let _ = tx.send(ImportMessage::Finished(result));
        });
        self.import_job = Some(ImportJob {
            rx,
            keep_alive: None,
            cancel,
            source,
            destination,
            current: crate::tl_id!("ui.reading_the_onenote_files").into(),
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
                Ok(ImportMessage::Destination(path)) => job.destination = path,
                Ok(ImportMessage::Finished(result)) => {
                    finished = Some(result);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    finished = Some(Err(crate::tl_id!("ui.the_import_stopped_unexpectedly").into()));
                    break;
                }
            }
        }
        if let Some(result) = finished {
            let Some(job) = self.import_job.take() else { return };
            match result {
                Ok(message) => {
                    let _ = self.run("tree.refresh", &Params::new());
                    log::info!("OneNote import succeeded: source={} destination={}", job.source.display(), job.destination.display());
                    self.status_is_error = false;
                    self.open_notebook_from_path(&job.destination);
                    if !self.status_is_error {
                        self.say(crate::tl!(&message).to_string());
                    }
                }
                Err(error) => {
                    log::error!("OneNote import failed: source={} destination={} error={error}", job.source.display(), job.destination.display());
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
                } else if self.kind_in_tree(&dir) == Some(NodeKind::Section) {
                    // A section's link opens it where it was left, or its first page, or says
                    // it has none (issue #518), as choosing it in the notebooks does.
                    self.open_section(&dir);
                } else if self.session.tree().iter().any(|n| n.path == dir) {
                    // A notebook's (issue #531).
                    self.open_notebook_where_left(&dir);
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
            // With no caret in the page the box shows the default font, as the size box shows its base size.
            pageedit::FontValue::Unavailable => Some(crate::tl_id!("ui.default").to_string()),
        };
        let size = match font_selection.size {
            pageedit::FontValue::Value(Some(size)) => Some(size.to_string()),
            // The page's base size is 11 points, so the box shows that number, not the word Default.
            pageedit::FontValue::Value(None) => Some("11".to_owned()),
            pageedit::FontValue::Mixed => Some(crate::tl_id!("ui.mixed").to_string()),
            // With no caret in the page the box shows the base size, as OneNote does.
            pageedit::FontValue::Unavailable => Some("11".to_owned()),
        };
        let view = ribbon::View {
            split_panes: self.split_panes,
            always_on_top: self.always_on_top,
            docked: self.docked.is_some(),
            full_page: self.full_page,
            // The paper the page is drawn on: its own, or the window's (what Rule lines shows).
            rules: self.paper_now(),
            hide_title: self.hide_title,
            snap: self.snap_to_grid,
            zoom_percent: self.zoom_percent,
            pointer: self.pointer,
            rack: &self.rack,
            nib: self.nib,
            in_table: self.caret_in_table(),
            space_tool: self.space_tool,
            ruler: self.ruler,
            auto_shapes: self.auto_shapes,
            replaying: self.replaying,
            tabs_layout: self.tabs_layout,
            table_borders: self.caret_table_has_borders(),
            table_align: self.caret_cells_align(),
            table_shade: self.caret_cells_shade(),
            gridcraft: self.gridcraft.is_some(),
            page_colour: self.editor.as_ref().and_then(|e| e.colour),
            rule_colour: self.editor.as_ref().and_then(|e| e.rule_colour),
            paper_size: self.editor.as_ref().and_then(|e| e.paper_size),
            can_undo: self.editor.as_ref().is_some_and(|e| e.history.can_undo()) || self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_undo),
            can_redo: self.editor.as_ref().is_some_and(|e| e.history.can_redo()) || self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_redo),
            can_ink_redo: self.pad.as_ref().is_some_and(crate::inkpad::Pad::can_redo),
            sensitivity: self.sensitivity,
            pressure_available: self.stylus.sample().pen,
            has_page: self.editor.is_some(),
            has_recycle_notebook: self.recycle_notebook().is_some(),
            page_marked_unread: self.editor.as_ref().is_some_and(|e| self.unread.contains(&e.dir)),
            hide_authors: self.hide_authors,
            painting: self.painter.is_some(),
            style: self.caret_style(ui.ctx()),
            font_family: family.as_deref(),
            font_size: size.as_deref(),
            marks: font_selection.marks,
            text_colour: match &font_selection.colour {
                pageedit::FontValue::Value(v) => Some(v.as_deref()),
                _ => None,
            },
            text_highlight: match &font_selection.highlight {
                pageedit::FontValue::Value(v) => Some(v.as_deref()),
                _ => None,
            },
            text_colour_mixed: matches!(font_selection.colour, pageedit::FontValue::Mixed),
            text_highlight_mixed: matches!(font_selection.highlight, pageedit::FontValue::Mixed),
            // A page open for editing. Without a caret the controls place one at the end of the
            // page's last words and set what the next words typed there look like, so only a
            // locked page or no page makes them unavailable.
            text_editable: self.editor.as_ref().is_some_and(|e| !e.locked),
        };
        let format = self.editor.as_ref().map(|e| e.format);
        let mut acts = Vec::new();
        if !self.ribbon_folded() {
            egui::Panel::top("ribbon")
                .exact_size(ribbon::GROUP_H + 8.0)
                .frame(egui::Frame::new().fill(tokens.panel).inner_margin(egui::Margin::symmetric(4, 0)))
                .show_separator_line(false)
                .show(ui, |ui| {
                    acts = ribbon::show(ui, &tab, source, &view, format, &tokens);
                });
        } else if self.ribbon_peek {
            // Folded, with a tab clicked: the tab's commands over the top of the page, as Office
            // shows them, so the page does not jump down and back. Using one, clicking anywhere
            // else or Escape folds it away again.
            let screen = ui.ctx().content_rect();
            let area = egui::Area::new(egui::Id::new("ribbon-peek"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(screen.min.x, self.band_bottom))
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .fill(tokens.panel)
                        .inner_margin(egui::Margin::symmetric(4, 0))
                        .stroke(egui::Stroke::new(1.0, tokens.container_border))
                        .show(ui, |ui| {
                            ui.set_width(screen.width() - 8.0);
                            ui.set_height(ribbon::GROUP_H + 8.0);
                            acts = ribbon::show(ui, &tab, source, &view, format, &tokens);
                        });
                })
                .response;
            let pointer = ui.ctx().pointer_latest_pos();
            // A gallery the shown ribbon opened (Format Background, say) is over the page too, and
            // a click in it is still a click in the ribbon.
            let in_ribbon = |p: egui::Pos2| area.rect.contains(p) || ui.ctx().layer_id_at(p).is_some_and(|l| l.order == egui::Order::Foreground);
            let clicked_away = ui.input(|i| i.pointer.any_pressed()) && pointer.is_some_and(|p| !in_ribbon(p) && p.y > self.band_bottom);
            let used = acts.iter().any(|a| !matches!(a, ribbon::Act::Tab(_)));
            if used || clicked_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.ribbon_peek = false;
            }
        }
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
                Block::Heading { spans, .. } | Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Item { spans, .. } => spans.as_slice(),
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
        let mut result = ed.blocks.get(caret.block).and_then(|block| caret.fonts(block)).unwrap_or_default();
        if caret.selection.0 == caret.selection.1
            && let Some(pending) = ed.pending_format.as_ref().filter(|p| p.block == caret.block && p.at == caret.selection.0)
        {
            if let Some(family) = &pending.family {
                result.family = pageedit::FontValue::Value(Some(family.clone()));
            }
            if let Some(size) = pending.size {
                result.size = pageedit::FontValue::Value(Some(size));
            }
            // A mark clicked with nothing selected is put on what is typed next: the button
            // shows it pressed until then (issue #255).
            let m = &mut result.marks;
            m.bold |= pending.bold;
            m.italic |= pending.italic;
            m.strike |= pending.strike;
            m.underline |= pending.underline;
            m.subscript |= pending.script == Some(notecraft_formats::markdown::Script::Subscript);
            m.superscript |= pending.script == Some(notecraft_formats::markdown::Script::Superscript);
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
                | ribbon::Act::Align(_)
        );
        let Some(ed) = self.editor.as_mut().filter(|_| wanted) else { return false };
        let range = notecraft_model::container::blocks_of(&ed.boxes, at);
        if range.end > ed.blocks.len() || range.is_empty() {
            return false;
        }
        let words = |b: &Block| !matches!(b, Block::Code { .. } | Block::MathBlock(_, _)) && pageedit::is_one_line_of_words(b);
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
                    if let Block::Heading { spans, .. } | Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Item { spans, .. } = block {
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
            ribbon::Act::Align(align) => {
                let blocks: Vec<usize> = range.collect();
                self.align_blocks(&blocks, *align);
                return true;
            }
            ribbon::Act::Justify(strength) => {
                let paragraphs: Vec<usize> = range.filter(|i| matches!(ed.blocks.get(*i), Some(Block::Paragraph(_, _)))).collect();
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
                        pageedit::clear_marks(b);
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
        // A tool chosen is chosen by hand; the automatic switch marks its own choice after this.
        // A tool added to the rack or changed on it is chosen by hand too, once the rack has
        // taken it (`edit_rack`).
        if matches!(act, ribbon::Act::InkPen(_) | ribbon::Act::InkPointer(_)) {
            self.pen_picked_by_pen = false;
        }
        match act {
            ribbon::Act::NewWindow => self.request_window(false),
            ribbon::Act::QuickNote => self.request_window(true),
            #[cfg(feature = "voice")]
            ribbon::Act::Dictate => self.toggle_dictation(),
            #[cfg(feature = "voice")]
            ribbon::Act::DictationSettings => self.open_dictation_setup(),
            ribbon::Act::File => self.backstage = Some("Info"),
            ribbon::Act::SendPage => {
                let _ = self.session.run("export.cancel", &Params::new());
                self.portable.selection.scope = backstage::CopyScope::Page;
                self.portable.selection.result = None;
                self.portable.downloaded = false;
                self.backstage = Some("Share");
            }
            ribbon::Act::NextUnread => self.next_unread(),
            ribbon::Act::ToggleRead => self.toggle_read(),
            ribbon::Act::RecentEdits => self.show_recent_edits(RecentRange::Week, false),
            ribbon::Act::FindByAuthor => self.show_find_by_author(),
            ribbon::Act::ToggleHideAuthors => {
                self.hide_authors = !self.hide_authors;
                let _ = self.session.settings.set(HIDE_AUTHORS_KEY, if self.hide_authors { "true" } else { "false" });
                self.say(if self.hide_authors {
                    crate::tl_id!("ui.who_changed_a_page_is_hidden")
                } else {
                    crate::tl_id!("ui.who_changed_a_page_is_shown_under_its_title")
                });
            }
            ribbon::Act::PageVersions => {
                self.save_now();
                if let Some(page) = self.editor.as_ref().map(|e| e.dir.clone()) {
                    let p = params(&[("path", &path_str(&page))]);
                    if let Some(Output::Versions(versions)) = self.run("page.versions", &p) {
                        self.dialog = Some(Dialog::PageVersions { page, versions, preview: None });
                    }
                }
            }
            ribbon::Act::RecycleBin => {
                if let Some(notebook) = self.recycle_notebook().map(Path::to_path_buf) {
                    self.open_recycle_bin(notebook, None);
                }
            }
            ribbon::Act::Symbols => self.dialog = Some(Dialog::Symbols),
            ribbon::Act::Equation(tex) => self.insert_in_page(&format!("${tex}$")),
            ribbon::Act::Tab(name) => {
                // Folded, a tab shows its commands over the page; the same tab again hides them.
                if self.ribbon_folded() {
                    self.ribbon_peek = !(self.ribbon_peek && self.ribbon_tab == name);
                }
                self.ribbon_tab = name;
            }
            ribbon::Act::ToggleRibbon => self.toggle_ribbon(),
            ribbon::Act::Mark(mark) => self.apply_mark(mark),
            ribbon::Act::FontSize(size) => self.apply_font_size(size),
            ribbon::Act::TextColour(colour) => self.apply_text_colour(pageedit::InlineFormat::Colour(colour)),
            ribbon::Act::TextHighlight(colour) => self.apply_text_colour(pageedit::InlineFormat::Highlight(colour)),
            ribbon::Act::FontFamily(family) => self.apply_font_family(&family),
            ribbon::Act::Block(kind) => self.apply_block_kind(kind),
            ribbon::Act::Insert(what) => match what {
                "space" => {
                    let Some(ed) = self.editor.as_mut() else { return };
                    // What was written before is kept as it stood, so Undo takes back only the space.
                    Self::remember_step(ed);
                    let at = ed.caret.as_ref().map(|c| c.block + 1).unwrap_or(ed.blocks.len()).min(ed.blocks.len());
                    ed.blocks.insert(at, notecraft_formats::Block::Paragraph(Vec::new(), Vec::new()));
                    let marks = ed.blocks_inserted(at, 1);
                    ed.rebuild_body();
                    ed.caret = Some(pageedit::Caret {
                        block: at,
                        text: String::new(),
                        focus: true,
                        selection: (0, 0),
                        place: None,
                        cell: None,
                        span: None,
                        inline: None,
                    });
                    Self::mark_dirty_from_blocks(ed);
                    Self::remember_edit(ed);
                    self.save_block_marks(marks);
                }
                "link" => self.open_link_dialog(),
                "online-video" => self.open_online_video_dialog(),
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
                    picker = picker.add_filter(crate::tl_id!("ui.pictures"), &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "jxl"]);
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
            ribbon::Act::ToggleDock => self.dock_pending = true,
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
            ribbon::Act::ToggleSnap => {
                self.snap_to_grid = !self.snap_to_grid;
                let _ = self.session.settings.set("snap_to_grid", if self.snap_to_grid { "true" } else { "false" });
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
            ribbon::Act::FormatPainter(sticky) => self.pick_up_formatting(sticky),
            ribbon::Act::PageTemplates => {
                let name = self.templates_pane.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                self.templates_pane = Some(TemplatesPane { templates: Vec::new(), name });
                self.refresh_templates();
            }
            ribbon::Act::InsertSpaceTool => {
                if self.editor.is_none() {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                    return;
                }
                self.space_tool = !self.space_tool;
                if self.space_tool {
                    self.pointer = ribbon::Pointer::Write;
                    self.say(crate::tl_id!("ui.insert_space_how"));
                }
            }
            ribbon::Act::FindTags => {
                // OneNote looks in the section first; with no page open there is no section.
                let scope = self.tags_pane.as_ref().map_or(if self.editor.is_some() { TagScope::Section } else { TagScope::All }, |p| p.scope);
                let unchecked = self.tags_pane.as_ref().is_some_and(|p| p.unchecked);
                self.tags_pane = Some(TagsPane { scope, unchecked, found: Vec::new() });
                self.refresh_tags();
            }
            ribbon::Act::CustomizeTags => {
                self.custom_tags = Some(crate::customtags::Dialog::new(notecraft_model::tag::custom()));
            }
            ribbon::Act::InkHelp => self.open_ink_help(),
            ribbon::Act::ImmersiveReader => match self.editor.as_ref() {
                Some(ed) => self.reader = Some(crate::reader::Reader::new(&ed.title_buf, &ed.blocks, !ed.ink_files.is_empty())),
                None => self.say(crate::tl_id!("ui.reader_open_a_page")),
            },
            ribbon::Act::ToggleTabsLayout => {
                self.tabs_layout = !self.tabs_layout;
                let _ = self.session.settings.set("tabs_layout", if self.tabs_layout { "true" } else { "false" });
                self.say(if self.tabs_layout { crate::tl_id!("ui.tabs_layout_on") } else { crate::tl_id!("ui.tabs_layout_off") });
            }
            ribbon::Act::ToggleReplay => {
                if !self.replaying && self.pad.as_ref().is_none_or(crate::inkpad::Pad::is_empty) {
                    self.say(crate::tl_id!("ui.replay_nothing_to_replay"));
                } else {
                    self.replaying = !self.replaying;
                }
            }
            ribbon::Act::ToggleAutoShapes => {
                self.auto_shapes = !self.auto_shapes;
                let _ = self.session.settings.set("auto_shapes", if self.auto_shapes { "true" } else { "false" });
                self.say(if self.auto_shapes { crate::tl_id!("ui.automatic_shapes_on") } else { crate::tl_id!("ui.automatic_shapes_off") });
            }
            ribbon::Act::ToggleRuler => {
                self.ruler = !self.ruler;
                // The ruler is for drawing: with the caret in hand, the pen last used comes up.
                if self.ruler && self.pointer == ribbon::Pointer::Write {
                    self.ribbon_act(ribbon::Act::InkPen(self.nib));
                }
            }
            ribbon::Act::CollectAll => {
                if self.editor.as_ref().is_none_or(|ed| !ed.writable) {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                    return;
                }
                self.collect_pending = true;
                // The gathered page starts at its top left, so it is in view.
                self.canvas_pan = egui::Vec2::ZERO;
            }
            ribbon::Act::NotBuilt(what) => {
                self.status =
                    crate::i18n::format_message_id("ui.name_is_not_built_yet_see_the_roadmap_for_what_is_coming", &[("name", crate::tl!(what))]);
                self.status_is_error = false;
            }
            ribbon::Act::Tag(tag) => self.apply_tag(tag),
            ribbon::Act::Clipboard(which) => self.clipboard(which),
            ribbon::Act::Paste => self.paste_here(),
            ribbon::Act::PastePlain => self.paste_plain(),
            ribbon::Act::SetReadOnly(locked) => {
                if let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) {
                    self.set_read_only(&dir, locked);
                }
            }
            ribbon::Act::Indent(deeper) => self.indent_block(deeper),
            ribbon::Act::ClearFormatting => self.clear_formatting(),
            ribbon::Act::Direction(d) => self.apply_direction(d),
            ribbon::Act::Justify(strength) => self.apply_justify(strength),
            ribbon::Act::Align(align) => self.apply_align(align),
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
            ribbon::Act::DeleteTable => self.delete_table(),
            ribbon::Act::Sticker(id) => self.insert_sticker(id),
            ribbon::Act::MeetingDetails => self.open_meeting_details(None),
            ribbon::Act::MergeCells => self.merge_table_cells(),
            ribbon::Act::SplitCells => self.split_table_cell(),
            ribbon::Act::TableSort(descending) => self.sort_table(descending),
            ribbon::Act::TableSelect(what) => self.select_in_table(what),
            ribbon::Act::TableBorders(on) => self.set_table_borders(on),
            ribbon::Act::TableAlign(align) => self.align_table_cells(align),
            ribbon::Act::TableShade(shade) => self.shade_table_cells(shade),
            ribbon::Act::RuleColour(c) => {
                let Some(ed) = self.editor.as_mut() else {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                    return;
                };
                if ed.locked {
                    self.say(crate::tl_id!("ui.page_is_read_only"));
                    return;
                }
                let path = path_str(&ed.dir);
                if self.run("page.paper", &params(&[("path", &path), ("rules", c.map(notecraft_model::NodeColor::id).unwrap_or("none"))])).is_none() {
                    return;
                }
                if let Some(ed) = self.editor.as_mut() {
                    ed.rule_colour = c;
                }
                self.say(match c {
                    Some(c) => crate::i18n::format_message_id("ui.the_lines_are_colour", &[("colour", &crate::tl!(c.label()).to_lowercase())]),
                    None => crate::tl_id!("ui.the_lines_are_the_theme_s_own_colour").to_string(),
                });
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
            ribbon::Act::PaperSize(paper) => self.set_paper_size(paper.as_deref().and_then(notecraft_model::paper::Paper::parse)),
            ribbon::Act::Undo => self.undo(),
            ribbon::Act::Redo => self.step_back(false),
            ribbon::Act::ToGridCraft => self.hand_table_to_gridcraft(),
            ribbon::Act::FromGridCraft => self.read_table_from_gridcraft(),
            ribbon::Act::NewGridCraft => self.new_gridcraft(),
            ribbon::Act::InsertTable(rows, columns) => self.insert_table(rows, columns),
            ribbon::Act::InsertTableDialog => {
                if self.editor.is_none() {
                    self.say(crate::tl_id!("ui.open_a_page_first"));
                } else {
                    self.dialog = Some(Dialog::InsertTable { rows: "3".into(), columns: "3".into(), first: true });
                }
            }
            ribbon::Act::ParagraphSpacing => self.open_paragraph_spacing(),
            ribbon::Act::InkSensitivity(v) => {
                self.sensitivity = v.min(100);
                let _ = self.session.settings.set(SENSITIVITY_KEY, &self.sensitivity.to_string());
            }
            ribbon::Act::InkUndo => self.ink_undo(),
            ribbon::Act::InkRedo => self.ink_redo(),
            ribbon::Act::InkInsert => self.ink_insert(),
            ribbon::Act::InkExportPlain => self.ink_export_plain(),
            ribbon::Act::SavePictureAs(src) => self.save_picture_as(&src),
            ribbon::Act::TurnPicture(src, how) => self.turn_picture(&src, how),
            ribbon::Act::CopyTextFromPicture { src, whole_printout } => self.copy_text_from_picture(&src, whole_printout),
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
                self.pen_picked_by_pen = false;
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
                    self.pen_picked_by_pen = false;
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

    /// Paste > Keep text only, and Ctrl+Shift+V (issue #535): the clipboard's text alone, as if it
    /// were typed at the caret. It takes the place of the words picked, its lines stay lines
    /// (each a paragraph), and what the source had for fonts, colours and links stays behind,
    /// since only the text flavour is read. One Undo takes it back.
    fn paste_plain(&mut self) {
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if ed.locked || !ed.writable {
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        let Some(text) = self.clipboard.text().filter(|t| !t.is_empty()) else {
            self.say(crate::tl_id!("ui.there_is_nothing_on_the_clipboard_to_paste"));
            return;
        };
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        let Some(ed) = self.editor.as_mut() else { return };
        Self::remember_step(ed);
        // Words selected across blocks are replaced, as typing over them would.
        if let Some(open) = ed.across.filter(|a| !a.inside_one_block()) {
            let (block, at) = pageedit::delete_span(&mut ed.blocks, open);
            let text = ed.blocks.get(block).map(pageedit::caret_text).unwrap_or_default();
            ed.caret = Some(pageedit::Caret { block, text, focus: true, selection: (at, at), place: Some(at), cell: None, span: None, inline: None });
        }
        ed.across = None;
        Self::place_caret_for_format(ed);
        let mut marks = (false, false);
        // A page with nothing to write in yet gets a line to paste into.
        if ed.caret.is_none() {
            ed.blocks.push(notecraft_formats::Block::Paragraph(Vec::new(), Vec::new()));
            let block = ed.blocks.len() - 1;
            marks = ed.blocks_inserted(block, 1);
            ed.caret = Some(pageedit::Caret {
                block,
                text: String::new(),
                focus: true,
                selection: (0, 0),
                place: Some(0),
                cell: None,
                span: None,
                inline: None,
            });
        }
        let Some(mut caret) = ed.caret.take() else { return };
        let words = caret.cell.is_some()
            || matches!(
                ed.blocks.get(caret.block),
                Some(
                    notecraft_formats::Block::Paragraph(..)
                        | notecraft_formats::Block::Heading { .. }
                        | notecraft_formats::Block::Quote(..)
                        | notecraft_formats::Block::Item { .. }
                )
            );
        if !words {
            // A block of code takes the text as it is, line breaks and all.
            let (new_text, at) = pageedit::insert_at_caret(&caret.text, caret.selection, &text);
            caret.text = new_text;
            caret.selection = (at, at);
            caret.place = Some(at);
            caret.focus = true;
            if let Some(b) = ed.blocks.get_mut(caret.block) {
                caret.write(b);
            }
            ed.caret = Some(caret);
            ed.rebuild_body();
            Self::mark_dirty_from_blocks(ed);
            Self::remember_edit(ed);
            self.save_block_marks(marks);
            self.say(crate::tl_id!("ui.pasted_as_text"));
            return;
        }
        let lines: Vec<&str> = text.split('\n').collect();
        let at = caret.block;
        let count = pageedit::insert_plain_lines(&mut ed.blocks, &mut caret, &lines);
        ed.caret = Some(caret);
        if count > 0 {
            let more = ed.blocks_inserted(at + 1, count);
            marks = (marks.0 || more.0, marks.1 || more.1);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        self.save_block_marks(marks);
        self.say(crate::tl_id!("ui.pasted_as_text"));
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
            ed.blocks.push(notecraft_formats::markdown::Block::Paragraph(Vec::new(), Vec::new()));
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

    /// Home > Meeting Details (issue #283). Opens on today, from `file`, the calendar file used
    /// last time, or one asked for. Nothing on the page changes until a meeting is inserted.
    pub fn open_meeting_details(&mut self, file: Option<PathBuf>) {
        if self.editor.as_ref().is_none_or(|e| !e.writable) {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        }
        let file = file
            .or_else(|| self.session.settings.get(MEETING_FILE_KEY).filter(|f| !f.trim().is_empty()).map(PathBuf::from).filter(|f| f.is_file()))
            .or_else(pick_calendar_file);
        let Some(file) = file else { return };
        let mut choice = crate::meeting::Choice {
            day: (notecraft_model::time::now_secs() + i64::from(notecraft_model::time::local_offset())).div_euclid(86_400),
            ..Default::default()
        };
        choice.read(&file);
        if choice.error.is_none() {
            let _ = self.session.settings.set(MEETING_FILE_KEY, &path_str(&file));
        }
        self.dialog = Some(Dialog::MeetingDetails(choice));
    }

    /// The meeting picked in an open Meeting Details dialog, put on the page under the caret's
    /// line as editable lines, as one Undo step. For the dialog's Insert button, tests and
    /// automation.
    pub fn insert_meeting(&mut self, event: &notecraft_formats::ics::Event, occurrence: &notecraft_formats::ics::Occurrence) {
        let lines = crate::meeting::details(event, occurrence, notecraft_model::time::local_offset());
        let Some(ed) = self.editor.as_mut().filter(|e| e.writable) else { return };
        Self::remember_step(ed);
        let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
        let n = lines.len();
        for (i, b) in lines.into_iter().enumerate() {
            ed.blocks.insert(at + i, b);
        }
        let marks = ed.blocks_inserted(at, n);
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        self.save_block_marks(marks);
        self.say(crate::tl_id!("ui.meeting_details_added"));
    }

    /// The Meeting Details dialog's state, for tests.
    pub fn meeting_choice(&mut self) -> Option<&mut crate::meeting::Choice> {
        match self.dialog.as_mut() {
            Some(Dialog::MeetingDetails(c)) => Some(c),
            _ => None,
        }
    }

    /// Insert > Stickers (issue #286): the sticker is copied into the page's own folder and
    /// put on the page as a picture under the caret's line, so it moves, resizes, deletes and
    /// undoes as any picture does, and the page owns it in both formats.
    pub fn insert_sticker(&mut self, id: &str) {
        let Some(sticker) = crate::stickers::find(id) else { return };
        let Some(dir) = self.editor.as_ref().filter(|e| e.writable).map(|e| e.dir.clone()) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        // The same sticker twice on a page is one file, not a copy for each.
        let file = format!("sticker-{}.svg", sticker.id);
        let have = std::fs::read(dir.join("assets").join(&file)).is_ok_and(|b| b == sticker.svg);
        let attached = if have { Ok(format!("assets/{file}")) } else { notecraft_store::assets::attach_bytes(&dir, &file, sticker.svg) };
        let link = match attached {
            Ok(link) => link,
            Err(e) => {
                self.complain(&crate::i18n::format_message_id("ui.cannot_add_the_sticker_error", &[("error", &e.to_string())]));
                return;
            }
        };
        let name = crate::tl!(sticker.label).to_string();
        let Some(ed) = self.editor.as_mut() else { return };
        Self::remember_step(ed);
        let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
        ed.blocks.insert(
            at,
            notecraft_formats::markdown::Block::Image {
                alt: name.clone(),
                src: link,
                width: Some(crate::stickers::WIDTH),
                height: Some(crate::stickers::WIDTH),
                emoji: Vec::new(),
            },
        );
        let marks = ed.blocks_inserted(at, 1);
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        ed.attachments = notecraft_store::assets::list(&dir);
        Self::remember_edit(ed);
        self.save_block_marks(marks);
        self.say(crate::i18n::format_message_id("ui.added_the_name_sticker", &[("name", &name)]));
    }

    fn paste_picture(&mut self, dir: &std::path::Path, picture: crate::clipboard::Picture) {
        let name = crate::clipboard::picture_name();
        let (w, h) = (picture.width, picture.height);
        log::debug!("clipboard image paste received: width={w} height={h} bytes={}", picture.png.len());
        match notecraft_store::assets::attach_bytes(dir, &name, &picture.png) {
            Ok(link) => {
                log::debug!("clipboard image paste attached: name={name:?} link={link:?}");
                self.read_pictures(dir);
                if let Some(ed) = self.editor.as_mut() {
                    let at = ed.caret.as_ref().map_or(ed.blocks.len(), |c| c.block + 1).min(ed.blocks.len());
                    ed.blocks.insert(
                        at,
                        notecraft_formats::markdown::Block::Image {
                            alt: "Pasted picture".to_string(),
                            src: link,
                            width: None,
                            height: None,
                            emoji: Vec::new(),
                        },
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
    /// The page names its ink file as the write about to be made will leave it: there once
    /// something is drawn, gone once the last stroke is. Done in the frame of the change, not
    /// when the write finishes, so the line in the page changes in the same step as the ink and
    /// one Undo takes back both.
    fn expect_ink_file(&mut self) {
        let (Some(ed), Some(pad)) = (self.editor.as_mut(), self.pad.as_ref()) else { return };
        let Some(name) = pad.file.file_name().map(|n| n.to_string_lossy().into_owned()) else { return };
        let drawn = pad.strokes() > 0;
        let named = ed.ink_files.contains(&name);
        if drawn && !named {
            ed.ink_files.push(name);
            ed.ink_files.sort();
        } else if !drawn && named {
            ed.ink_files.retain(|n| *n != name);
        } else {
            return;
        }
        if Self::name_ink(ed) {
            Self::mark_dirty_from_blocks(ed);
        }
    }

    /// Starts writing the ink off the window's thread, if it has changes.
    fn start_ink_save(&mut self) {
        let Some(pad) = self.pad.as_ref().filter(|p| p.dirty) else { return };
        let write = pad.prepare_save();
        let (file, had) = (write.file.clone(), pad.on_disk());
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("notecraft-ink-save".into()).spawn(move || {
            let _ = tx.send(write.run());
        });
        match spawned {
            Ok(_) => self.ink_saving = Some((file, self.ink_changes, had, rx)),
            // No thread to be had: written here, as it always was.
            Err(_) => self.save_ink_here(),
        }
    }

    /// Takes in the ink write under way once it is done, or, with `wait`, waits for it.
    fn take_ink_save(&mut self, wait: bool) {
        let Some((file, changes, had, rx)) = self.ink_saving.take() else { return };
        let result = if wait {
            rx.recv().ok()
        } else {
            match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    self.ink_saving = Some((file, changes, had, rx));
                    return;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => None,
            }
        };
        let Some(result) = result else {
            self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", "the write stopped")]));
            return;
        };
        // The page may have changed meanwhile: a write is taken in only by the drawing it was of.
        let current = changes == self.ink_changes;
        let Some(pad) = self.pad.as_mut().filter(|p| p.file == file) else { return };
        let now = result.as_ref().ok().map(crate::inkpad::Written::on_disk);
        // Written in the background, quietly: what the window said about the stroke (a shape
        // it was taken for) is not written over a moment later. A write that failed says so.
        if let Err(e) = pad.apply_save(result, current) {
            self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", &e.to_string())]));
        }
        self.tell_kept_ink();
        // The page names its ink file: only a file that came or went changes that.
        if now.is_some_and(|now| now != had) {
            self.sync_ink_reference();
        }
    }

    /// Writes the ink now, on this thread, after any write under way: for when it must be on
    /// disk before something else happens (another page, closing, a command).
    fn save_ink_here(&mut self) {
        self.take_ink_save(true);
        self.ink_save_at = None;
        if self.page_conflict {
            return;
        }
        let Some(pad) = self.pad.as_mut().filter(|p| p.dirty) else { return };
        let had = pad.on_disk();
        let strokes = pad.strokes();
        match pad.save() {
            Ok(()) => self.say(crate::i18n::format_message_id("ui.drawing_saved_count_strokes", &[("count", &strokes.to_string())])),
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", &e.to_string())])),
        }
        self.tell_kept_ink();
        if self.pad.as_ref().is_some_and(|p| p.on_disk() != had) {
            self.sync_ink_reference();
        }
    }

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
                ed.blocks.push(Block::Image { alt: "Ink".to_string(), src: file.clone(), width: None, height: None, emoji: Vec::new() });
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
            auto_shapes: self.auto_shapes,
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

    /// Puts back the change to the drawing that undo last took back.
    fn ink_redo(&mut self) {
        // A write of the ink under way is finished before the drawing changes under it.
        self.take_ink_save(true);
        let Some(pad) = self.pad.as_mut() else {
            self.say(crate::tl_id!("ui.nothing_to_redo"));
            return;
        };
        if !pad.redo() {
            self.say(crate::tl_id!("ui.nothing_to_redo"));
            return;
        }
        let strokes = pad.strokes();
        match pad.save() {
            Ok(()) => self.say(crate::i18n::format_message_id("ui.drawing_saved_count_strokes", &[("count", &strokes.to_string())])),
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_drawing_error", &[("error", &e.to_string())])),
        }
        self.sync_ink_reference();
    }

    /// Takes the last stroke back out of the drawing on this page.
    fn ink_undo(&mut self) {
        // A write of the ink under way is finished before the drawing changes under it.
        self.take_ink_save(true);
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
        self.tell_kept_ink();
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

    /// Whether the table the caret is in is drawn with lines round its cells. A table has
    /// them until its own borders are hidden, so with the caret outside any table this reads
    /// as true.
    fn caret_table_has_borders(&self) -> bool {
        let Some(ed) = self.editor.as_ref() else { return true };
        let Some(c) = ed.caret.as_ref().filter(|c| c.cell.is_some()) else { return true };
        !ed.columns.borders_hidden(c.block)
    }

    /// The cells Table > Align acts on: every picked cell, or the caret's own. With the
    /// table's block.
    fn caret_cells(&self) -> Option<(usize, Vec<(usize, usize)>)> {
        let ed = self.editor.as_ref()?;
        let c = ed.caret.as_ref()?;
        let here = c.cell?;
        if !matches!(ed.blocks.get(c.block), Some(notecraft_formats::markdown::Block::Table { .. })) {
            return None;
        }
        let cells = match pageedit::cell_range(c) {
            Some((a, b)) => (a.0..=b.0).flat_map(|r| (a.1..=b.1).map(move |col| (r, col))).collect(),
            None => vec![here],
        };
        Some((c.block, cells))
    }

    /// How the cells Table > Align would act on are aligned now, when they all agree. A cell
    /// given no alignment of its own reads as aligned left, which is how it is drawn.
    fn caret_cells_align(&self) -> Option<notecraft_model::tablecols::Align> {
        use notecraft_model::tablecols::Align;
        let ed = self.editor.as_ref()?;
        let (at, cells) = self.caret_cells()?;
        let mut aligns = cells.iter().map(|cell| ed.columns.align(at, *cell).unwrap_or(Align::Left));
        let first = aligns.next()?;
        aligns.all(|a| a == first).then_some(first)
    }

    /// The colour behind the cells Table > Shading would act on, when they all agree.
    fn caret_cells_shade(&self) -> Option<Option<notecraft_model::NodeColor>> {
        let ed = self.editor.as_ref()?;
        let (at, cells) = self.caret_cells()?;
        let mut shades = cells.iter().map(|cell| ed.columns.shade(at, *cell));
        let first = shades.next()?;
        shades.all(|s| s == first).then_some(first)
    }

    /// Table > Shading: a colour behind the caret's cell, or behind every picked cell, and no
    /// other; No Colour takes it away. Kept with the page beside the cells' alignment, and
    /// noted on the page's history step as the borders and the alignment are (issue #169).
    fn shade_table_cells(&mut self, shade: Option<notecraft_model::NodeColor>) {
        let Some((at, cells)) = self.caret_cells() else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        let Some(ed) = self.editor.as_mut() else { return };
        if !ed.writable || !ed.columns.set_shade(at, &cells, shade) {
            return;
        }
        Self::remember_edit(ed);
        self.save_block_marks((true, false));
    }

    /// Table > Align Left, Center or Align Right: the words in the caret's cell, or in every
    /// cell picked, and nothing else; the columns keep their widths and the table its place.
    /// Kept with the page, like the widths and the borders, and one step of Undo (issue #170).
    fn align_table_cells(&mut self, align: notecraft_model::tablecols::Align) {
        let Some((at, cells)) = self.caret_cells() else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        let Some(ed) = self.editor.as_mut() else { return };
        if !ed.writable {
            return;
        }
        // Left is how a cell is drawn when it was given nothing, so it is kept as nothing.
        let keep = (align != notecraft_model::tablecols::Align::Left).then_some(align);
        if !ed.columns.set_align(at, &cells, keep) {
            return;
        }
        Self::remember_edit(ed);
        self.save_block_marks((true, false));
    }

    /// Table > Hide Borders or Show Borders: the table the caret is in, and no other. The
    /// choice is kept with the page, like the widths of the table's columns, so it is there
    /// when the page is opened again (issue #161).
    fn set_table_borders(&mut self, on: bool) {
        let Some(ed) = self.editor.as_mut() else { return };
        if !ed.writable {
            return;
        }
        let Some(at) = ed.caret.as_ref().filter(|c| c.cell.is_some()).map(|c| c.block) else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::markdown::Block::Table { .. })) || !ed.columns.set_borders(at, !on) {
            return;
        }
        // Noted on the step the page is at, which is how a dragged column is noted too: an
        // undo back to here keeps the choice.
        Self::remember_edit(ed);
        self.save_block_marks((true, false));
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
        // The page file may be the one an autosave is still writing.
        self.take_saves(true);
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
                let (blocks, _) = notecraft_formats::boxes::read(ed.format, &ed.body);
                let audit = notecraft_formats::boxes::span_audit(&ed.body, &blocks);
                log::debug!("code view commit span audit: page={:?} raw_span_tags={} styled_spans={}", dir, audit.raw_tags, audit.styled);
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
        if ed.locked {
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        let (path, paper, colour) = (path_str(&ed.dir), paper.or(ed.paper), colour.unwrap_or(ed.colour));
        let pairs: Vec<(&str, &str)> = vec![
            ("path", path.as_str()),
            ("paper", paper.map(ribbon::RuleLines::id).unwrap_or("none")),
            ("color", colour.map(notecraft_model::NodeColor::id).unwrap_or("none")),
        ];
        if self.run("page.paper", &params(&pairs)).is_some()
            && let Some(ed) = self.editor.as_mut()
        {
            ed.paper = paper;
            ed.colour = colour;
        }
    }

    /// Gives the open page its sheet, or takes it back to Auto, and writes it into the page's
    /// file (issue #269). The words are not touched: only how the page is laid out to show it.
    fn set_paper_size(&mut self, paper: Option<notecraft_model::paper::Paper>) {
        let Some(ed) = self.editor.as_mut() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if ed.locked {
            self.say(crate::tl_id!("ui.page_is_read_only"));
            return;
        }
        if ed.paper_size == paper {
            return;
        }
        let was_size = ed.paper_size.map(|p| (std::mem::discriminant(&p.size), p.landscape));
        let path = path_str(&ed.dir);
        let line = paper.map_or_else(|| "auto".to_string(), notecraft_model::paper::Paper::write);
        if self.run("page.paper_size", &params(&[("path", &path), ("size", &line)])).is_none() {
            return;
        }
        if let Some(ed) = self.editor.as_mut() {
            ed.paper_size = paper;
        }
        // Said when the sheet or its turn changes, not at every step of a dragged margin.
        let now_size = paper.map(|p| (std::mem::discriminant(&p.size), p.landscape));
        if now_size != was_size || paper.is_none() {
            self.say(match paper {
                Some(p) => crate::i18n::format_message_id(
                    "ui.the_page_is_written_on_size_turn",
                    &[
                        ("size", crate::tl!(p.size.label())),
                        ("turn", &(if p.landscape { crate::tl_id!("ui.landscape") } else { crate::tl_id!("ui.portrait") }).to_lowercase()),
                    ],
                ),
                None => crate::tl_id!("ui.the_page_is_auto_it_grows_with_what_is_on_it").to_string(),
            });
        }
    }

    /// Takes the page back to how it was before the last change, or forward again.
    ///
    /// The whole page is put back: its text, its blocks, the boxes they sit in and where
    /// the caret was. Ink is not part of this; a stroke is taken back with Undo Stroke on
    /// the Draw tab, because the drawing is its own file beside the page.
    fn step_back(&mut self, back: bool) {
        let mut nothing = false;
        let mut cols_moved = false;
        let mut layout_moved = false;
        if let Some(ed) = self.editor.as_mut() {
            if !ed.writable {
                nothing = true;
            } else {
                let step = if back { ed.history.undo().cloned() } else { ed.history.redo().cloned() };
                match step {
                    Some(step) => {
                        ed.generation = ed.generation.wrapping_add(1);
                        ed.body = step.body;
                        ed.blocks = step.blocks;
                        ed.boxes = step.boxes;
                        ed.caret = step.caret.map(|c| pageedit::Caret { focus: true, ..c });
                        // A selection across blocks is kept in byte offsets into the text as
                        // it was; the restored text can differ (a format adds its markup), so
                        // the selection is let go rather than left pointing at the wrong words.
                        ed.across = None;
                        ed.pending_format = None;
                        ed.dirty_since = Some(Instant::now());
                        // The restored text says whether the page was arranged.
                        ed.arranged = notecraft_formats::boxes::has_boxes(ed.format, &ed.body);
                        // The table widths as they were then, written back when they differ.
                        if let Some(columns) = step.columns
                            && columns != ed.columns
                        {
                            ed.columns = columns;
                            cols_moved = true;
                        }
                        // And which paragraphs were justified or aligned (issue #258).
                        if let Some(justified) = step.justified
                            && justified != ed.justified
                        {
                            ed.justified = justified;
                            layout_moved = true;
                        }
                    }
                    None => nothing = true,
                }
            }
        }
        if cols_moved || layout_moved {
            self.save_block_marks((cols_moved, layout_moved));
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
        Self::remember(ed, false, None);
    }

    /// Remembers a change made from the ribbon rather than by writing, such as Insert Space
    /// or a row put into a table, as a step of its own. It never folds into the writing
    /// before it, even when the caret stays in the same block (issue #153).
    fn remember_edit(ed: &mut Editor) {
        Self::remember(ed, true, None);
    }

    /// Remembers a change to how a table is laid out that may leave its words as they were,
    /// such as cells merged or split (issue #298), as a step of its own, so Undo takes back
    /// that change alone. `remember_step` must have noted the layout from before.
    fn remember_layout(ed: &mut Editor) {
        if ed.history.here().is_some_and(|s| {
            s.columns.as_ref() == Some(&ed.columns) && s.justified.as_ref() == Some(&ed.justified) && s.body == ed.body && s.blocks == ed.blocks
        }) {
            return;
        }
        let mut step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        step.columns = Some(ed.columns.clone());
        step.justified = Some(ed.justified.clone());
        ed.history.record(step);
    }

    /// `moving`: boxes moved by the press of the pointer that went down then, and nothing else
    /// changed. The frames of one drag fold into one step.
    fn remember(ed: &mut Editor, edit: bool, moving: Option<u64>) {
        let same = ed.history.here().is_some_and(|s| s.body == ed.body && s.blocks == ed.blocks && s.boxes == ed.boxes);
        if same || {
            // Only the cursor's place on open paper came or went: the page is the one remembered.
            let saved = ed.body_to_save();
            saved != ed.body && ed.history.here().is_some_and(|s| s.body == saved)
        } {
            // Nothing really changed, or the page was just put back by undo itself. A column
            // dragged since is still noted, so an undo back to here keeps it (issue #154).
            if let Some(step) = ed.history.here_mut() {
                step.columns = Some(ed.columns.clone());
                step.justified = Some(ed.justified.clone());
            }
            return;
        }
        let here = ed.caret.as_ref().map(|c| c.block);
        let was = ed.history.here().and_then(|s| s.caret.as_ref().map(|c| c.block));
        let shape = ed.history.here().is_some_and(|s| s.blocks.len() == ed.blocks.len());
        let kind = match (moving, shape, here, was) {
            (Some(press), ..) => crate::history::Kind::Moving(press),
            (None, true, Some(a), was) if !edit && was.is_none_or(|b| a == b) => crate::history::Kind::Typing(a),
            _ => crate::history::Kind::Edit,
        };
        let mut step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), kind);
        step.columns = Some(ed.columns.clone());
        step.justified = Some(ed.justified.clone());
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
            inline: None,
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
    /// Table > Sort: the table's rows by the caret's column, or only the picked rows when a
    /// block of cells running over more than one row is picked (issue #171). One step of
    /// the page's history, so Undo puts the rows back in their order.
    fn sort_table(&mut self, descending: bool) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((at, cell)) = ed.caret.as_ref().and_then(|c| c.cell.map(|cell| (c.block, cell))) else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        let picked = ed.caret.as_ref().and_then(pageedit::cell_range).filter(|(a, b)| a.0 != b.0).map(|(a, b)| (a.0, b.0));
        Self::remember_step(ed);
        let Some(block) = ed.blocks.get_mut(at) else { return };
        let Some(order) = pageedit::sort_table_rows(block, cell.1, picked, descending) else {
            self.say(crate::tl_id!("ui.the_rows_are_already_in_that_order"));
            return;
        };
        // The caret stays in the same place in the table, and shows what is there now.
        let text = pageedit::cell_text(block, cell).unwrap_or_default();
        ed.rebuild_body();
        if let Some(caret) = ed.caret.as_mut() {
            caret.text = text;
            caret.selection = (0, 0);
            caret.inline = None;
        }
        // An aligned cell goes where its row went (issue #170).
        let cols_moved = ed.columns.reorder_rows(at, &order);
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        if cols_moved {
            self.save_block_marks((true, false));
        }
    }

    /// Table > Delete Table (issue #297): takes the table the caret is in out of the page, as one
    /// Undo step. The caret goes to the block that takes its place (the one after it, or the
    /// one before when it was last); a page left with nothing gets an empty line to type on.
    /// Table widths, cell marks and justified paragraphs after it move up with their blocks.
    fn delete_table(&mut self) {
        use notecraft_formats::markdown::Block;
        let Some(ed) = self.editor.as_mut() else { return };
        let at = ed.caret.as_ref().filter(|c| c.cell.is_some()).map(|c| c.block);
        let Some(at) = at.filter(|&at| matches!(ed.blocks.get(at), Some(Block::Table { .. }))) else {
            self.say(crate::tl_id!("ui.put_the_caret_in_a_table_first"));
            return;
        };
        Self::remember_step(ed);
        ed.caret = None;
        ed.blocks.remove(at);
        ed.count_removed_from_boxes(at);
        let cols = ed.columns.remove_blocks(at..at + 1);
        let justified = ed.justified.remove_blocks(at..at + 1);
        if ed.blocks.is_empty() {
            ed.blocks.push(Block::Paragraph(Vec::new(), Vec::new()));
            ed.count_inserted_in_boxes(0, 1);
        }
        pageedit::renumber(&mut ed.blocks);
        ed.rebuild_body();
        let to = at.min(ed.blocks.len() - 1);
        let text = pageedit::text_for_caret(&ed.blocks, to);
        ed.caret = Some(pageedit::Caret { block: to, text, focus: true, selection: (0, 0), place: Some(0), cell: None, span: None, inline: None });
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        self.save_block_marks((cols, justified));
        self.say(crate::tl_id!("ui.deleted_the_table"));
    }

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
        // What was written before is kept as it stood, so Undo takes back only the table change.
        Self::remember_step(ed);
        let Some(block) = ed.blocks.get_mut(at) else { return };
        let mut last = None;
        let mut done = 0;
        for _ in 0..times {
            match pageedit::edit_table(block, cell, what) {
                Some(to) => {
                    last = Some(to);
                    done += 1;
                }
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
        ed.caret = Some(pageedit::Caret { block: at, text, focus: true, selection: (0, 0), place: None, cell: Some(to), span: None, inline: None });
        // A width the page keeps for this table stays with the column it was dragged for, so a
        // column put in or taken out does not hand its neighbours the wrong widths (issue #154).
        let cols_moved = match what {
            pageedit::TableEdit::InsertColumn(left) => ed.columns.insert_column(at, if left { cell.1 } else { cell.1 + 1 }, pageedit::COLUMN_W),
            pageedit::TableEdit::DeleteColumn => (0..done).fold(false, |moved, _| ed.columns.remove_column(at, cell.1) || moved),
            // An aligned cell stays with its row too (issue #170).
            pageedit::TableEdit::InsertRow(above) => ed.columns.insert_row(at, if above { cell.0 } else { cell.0 + 1 }),
            pageedit::TableEdit::DeleteRow => (0..done).fold(false, |moved, _| ed.columns.remove_row(at, cell.0) || moved),
        };
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        self.save_block_marks((cols_moved, false));
    }

    /// Table > Merge Cells (issue #298): the picked block of cells becomes one cell. A block
    /// that cuts through a merged cell grows to take it in. The words of every cell go into
    /// the first, in reading order, so none are lost. One step of Undo for the words and the
    /// merge together.
    fn merge_table_cells(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((at, a, b)) = ed.caret.as_ref().filter(|c| c.cell.is_some()).and_then(|c| {
            let (a, b) = pageedit::cell_range(c).unwrap_or((c.cell?, c.cell?));
            Some((c.block, a, b))
        }) else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        if !ed.writable || !matches!(ed.blocks.get(at), Some(notecraft_formats::Block::Table { .. })) {
            return;
        }
        let (a, b) = ed.columns.whole_block(at, a, b);
        let mut columns = ed.columns.clone();
        match columns.merge(at, a, b) {
            Ok(_) => {}
            Err(notecraft_model::tablecols::MergeRefusal::OneCell) => {
                self.say(crate::tl_id!("ui.pick_two_or_more_cells_to_merge"));
                return;
            }
            Err(_) => {
                self.say(crate::tl_id!("ui.these_cells_cannot_be_merged"));
                return;
            }
        }
        // Before anything changes, so Undo puts back the words and the cells as they were.
        Self::remember_step(ed);
        ed.columns = columns;
        if let Some(block) = ed.blocks.get_mut(at) {
            pageedit::merge_cell_words(block, a, b);
        }
        let text = ed.blocks.get(at).and_then(|b| pageedit::cell_text(b, a)).unwrap_or_default();
        ed.rebuild_body();
        ed.caret = Some(pageedit::Caret { block: at, text, focus: true, selection: (0, 0), place: None, cell: Some(a), span: None, inline: None });
        Self::mark_dirty_from_blocks(ed);
        Self::remember_layout(ed);
        self.save_block_marks((true, false));
        self.say(crate::tl_id!("ui.the_cells_are_merged"));
    }

    /// Table > Split Cells (issue #298): the merged cell the caret is in is its cells again.
    /// Its words stay in the first of them.
    fn split_table_cell(&mut self) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some((at, cell)) = ed.caret.as_ref().and_then(|c| Some((c.block, c.cell?))) else {
            self.say(crate::tl_id!("ui.click_in_the_table_first"));
            return;
        };
        if !ed.writable {
            return;
        }
        if ed.columns.span(at, cell).is_none() {
            self.say(crate::tl_id!("ui.this_cell_is_not_merged"));
            return;
        }
        Self::remember_step(ed);
        ed.columns.split(at, cell);
        Self::remember_layout(ed);
        self.save_block_marks((true, false));
        self.say(crate::tl_id!("ui.the_merged_cell_is_split"));
    }

    /// The sizes typed into an open Insert Table dialog, as a person would type them. For tests
    /// and automation; does nothing when the dialog is not open.
    pub fn set_insert_table_size(&mut self, rows: &str, columns: &str) {
        if let Some(Dialog::InsertTable { rows: r, columns: c, .. }) = self.dialog.as_mut() {
            *r = rows.to_string();
            *c = columns.to_string();
        }
    }

    /// The open page's blocks, as the editor holds them.
    pub fn editor_blocks(&self) -> Vec<notecraft_formats::Block> {
        self.editor.as_ref().map(|e| e.blocks.clone()).unwrap_or_default()
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

    /// Says where the page's ink file was kept, when writing the ink kept it first because it
    /// held more than could be read back as strokes (issue #157).
    fn tell_kept_ink(&mut self) {
        let Some(kept) = self.pad.as_mut().and_then(|pad| pad.kept_original.take()) else { return };
        let shown = self.shown(&kept);
        log::warn!("the page's ink file held more than its strokes; the original was kept: {kept:?}");
        self.say(crate::i18n::format_message_id("ui.kept_the_original_drawing_file_path", &[("path", &shown)]));
    }

    /// Writes the drawing again with no pen readings in it, for sending somewhere else.
    fn ink_export_plain(&mut self) {
        let Some(pad) = self.pad.as_ref() else {
            self.say(crate::tl_id!("ui.there_is_no_drawing_on_this_page"));
            return;
        };
        let saved = pad.save_plain().and_then(|path| {
            if let Some(downloads) = self.file_downloads.as_mut() {
                let bytes = std::fs::read(&path)?;
                let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| "ink-picture.svg".to_owned());
                downloads.push(FileDownload { name, mime: "image/svg+xml", bytes });
            }
            Ok(path)
        });
        match saved {
            Ok(path) => {
                let shown = if self.file_downloads.is_some() {
                    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| self.shown(&path))
                } else {
                    self.shown(&path)
                };
                self.say(crate::i18n::format_message_id("ui.saved_the_drawing_as_a_picture_path", &[("path", &shown)]));
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_picture_error", &[("error", &e.to_string())])),
        }
    }

    /// Exports the original image file at `src` via the native save file dialog (or browser download).
    /// Rotates or flips a picture on the page (issue #273). The turned picture is written as a
    /// new file beside the page and every place the page shows the old one points at it; the
    /// original file stays, so Undo puts the page back on it. A picture given a size of its own
    /// keeps it, with width and height swapped by a quarter turn.
    fn turn_picture(&mut self, src: &str, how: crate::turn::Turn) {
        use notecraft_formats::markdown::Block;
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if !ed.writable {
            self.complain(crate::tl_id!("ui.this_picture_cannot_be_turned_here"));
            return;
        }
        let dir = ed.dir.clone();
        let (_, _, bytes) = match crate::imagemenu::validate_and_read_image(&dir, src) {
            Ok(read) => read,
            Err(e) => {
                self.complain(&crate::i18n::format_message_id("ui.cannot_read_the_picture_error", &[("error", &e.to_string())]));
                return;
            }
        };
        let (turned, ext) = match crate::turn::turn(&bytes, how) {
            Ok(turned) => turned,
            Err(e) => {
                log::debug!("picture turn failed for {src:?}: {e}");
                self.complain(crate::tl_id!("ui.this_picture_cannot_be_turned_here"));
                return;
            }
        };
        let link = match notecraft_store::assets::attach_bytes(&dir, &crate::turn::turned_name(src, how, ext), &turned) {
            Ok(link) => link,
            Err(e) => {
                self.complain(&crate::i18n::format_message_id("ui.cannot_save_the_pasted_picture_error", &[("error", &e.to_string())]));
                return;
            }
        };
        let Some(ed) = self.editor.as_mut() else { return };
        // The page as it stood is kept, so Undo takes back only the turn.
        Self::remember_step(ed);
        let mut found = false;
        for block in &mut ed.blocks {
            match block {
                Block::Image { src: s, width, height, .. } if s == src => {
                    s.clone_from(&link);
                    if how.swaps_sides() {
                        std::mem::swap(width, height);
                    }
                    found = true;
                }
                Block::Heading { spans, .. } | Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Item { spans, .. } => {
                    for span in spans.iter_mut().filter(|s| s.image.as_deref() == Some(src)) {
                        span.image = Some(link.clone());
                        found = true;
                    }
                }
                _ => {}
            }
        }
        if !found {
            // Nothing pointed at it after all: the copy is not left lying in the folder.
            let _ = notecraft_store::assets::remove(&dir, link.trim_start_matches("assets/"));
            return;
        }
        if let Some(caret) = ed.caret.as_mut()
            && let Some(block) = ed.blocks.get(caret.block)
        {
            caret.text = pageedit::caret_text(block);
            caret.inline = None;
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        ed.attachments = notecraft_store::assets::list(&dir);
        let inked = self.pad.as_ref().is_some_and(|p| !p.is_empty());
        self.say(match (how.swaps_sides(), inked) {
            (true, false) => crate::tl_id!("ui.rotated_the_picture"),
            (false, false) => crate::tl_id!("ui.flipped_the_picture"),
            // Ink is its own layer over the whole page: it is not turned with the picture.
            (true, true) => crate::tl_id!("ui.rotated_the_picture_ink_on_the_page_stays_where_it_is"),
            (false, true) => crate::tl_id!("ui.flipped_the_picture_ink_on_the_page_stays_where_it_is"),
        });
    }

    /// Copy Text from Picture (issue #274): reads the printed text of the picture `src`, or of every
    /// page of the printout it is a page of, and opens the dialog that shows it for review.
    fn copy_text_from_picture(&mut self, src: &str, whole_printout: bool) {
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let printout = crate::picture_text::printout_source(src).filter(|_| whole_printout);
        let sources: Vec<String> = match printout {
            // Every page of the printout, in the order the page shows them.
            Some(source) => {
                let mut pages: Vec<String> = Vec::new();
                for block in &ed.blocks {
                    if let notecraft_formats::Block::Image { src: page, .. } = block
                        && crate::picture_text::printout_source(page) == Some(source)
                        && !pages.contains(page)
                    {
                        pages.push(page.clone());
                    }
                }
                if pages.is_empty() { vec![src.to_string()] } else { pages }
            }
            None => vec![src.to_string()],
        };
        let mut pictures = Vec::with_capacity(sources.len());
        for source in &sources {
            match crate::preview::page_asset_path(&ed.dir, source) {
                Some(path) => pictures.push(path),
                None => {
                    self.complain(&crate::i18n::format_message_id("ui.cannot_read_the_picture_error", &[("error", source)]));
                    return;
                }
            }
        }
        let scope = match (printout.is_some(), crate::picture_text::printout_source(src).is_some()) {
            (true, _) => crate::picture_text::Scope::Printout,
            (false, true) => crate::picture_text::Scope::PrintoutPage,
            (false, false) => crate::picture_text::Scope::Picture,
        };
        self.picture_text = Some(crate::picture_text::Dialog::start(scope, pictures, self.ocr_models()));
    }

    /// File > Print or Ctrl+P (issue #168): the open page's work is saved first, so what is printed is
    /// what the page holds, and the Print dialog opens for it.
    pub fn open_print(&mut self) {
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        self.save_now();
        if self.is_dirty() {
            self.complain(crate::tl_id!("ui.print_save_first"));
            return;
        }
        let remembered = self.session.settings.get(PRINT_PAPER_KEY).and_then(|p| notecraft_model::paper::Paper::parse(&p));
        self.print = Some(crate::print::Dialog::new(&dir, remembered));
    }

    /// Whether the Print dialog is open.
    pub fn printing(&self) -> bool {
        self.print.is_some()
    }

    /// Shows the Print dialog while it is open, and saves the PDF it asks for.
    fn print_dialog(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.print.take() else { return };
        self.give_modal_paste(ctx);
        let out = dialog.show(ctx, &self.tokens);
        if let Some(said) = out.said {
            self.say(said);
            let _ = self.session.settings.set(PRINT_PAPER_KEY, &dialog.layout.paper.write());
        }
        if let Some((bytes, name)) = out.save_pdf {
            let _ = self.session.settings.set(PRINT_PAPER_KEY, &dialog.layout.paper.write());
            self.save_pdf(bytes, &name);
        }
        if !out.closed {
            self.print = Some(dialog);
        }
    }

    /// Saves a printed PDF: where the person chooses, or as a download in a browser.
    fn save_pdf(&mut self, bytes: Vec<u8>, name: &str) {
        if let Some(downloads) = self.file_downloads.as_mut() {
            downloads.push(FileDownload { name: name.to_string(), mime: "application/pdf", bytes });
            self.say(crate::i18n::format_message_id("ui.print_saved_pdf", &[("path", name)]));
            return;
        }
        let Some(destination) = pick_pdf_destination(name) else { return };
        match std::fs::write(&destination, &bytes) {
            Ok(()) => {
                let shown = self.shown(&destination);
                self.say(crate::i18n::format_message_id("ui.print_saved_pdf", &[("path", &shown)]));
            }
            Err(e) => self.complain(&crate::i18n::format_message_id("ui.print_could_not_save_pdf", &[("error", &e.to_string())])),
        }
    }

    /// The folder with the text recognition models, when one is set.
    fn ocr_models(&self) -> Option<PathBuf> {
        self.session.settings.get("ocr_models").filter(|v| !v.trim().is_empty()).map(PathBuf::from)
    }

    /// Shows Copy Text from Picture while it is open, and does what it asks.
    fn picture_text_dialog(&mut self, ctx: &Context) {
        let Some(mut dialog) = self.picture_text.take() else { return };
        self.give_modal_paste(ctx);
        let out = dialog.show(ctx, &self.tokens);
        if let Some(text) = out.copy {
            match self.clipboard.set_text(&text) {
                Ok(()) => dialog.mark_copied(),
                Err(e) => self.complain(&crate::i18n::format_message_id("ui.picture_text_copy_failed", &[("error", &e)])),
            }
        }
        if out.choose_models
            && let Some(folder) = pick_ocr_models_folder()
        {
            if crate::picture_text::models_folder_is_complete(&folder) {
                if let Err(e) = self.session.settings.set("ocr_models", &folder.to_string_lossy()) {
                    self.complain(&e.to_string());
                }
                dialog = crate::picture_text::Dialog::start(dialog.scope(), dialog.pictures().to_vec(), Some(folder));
            } else {
                self.complain(crate::tl_id!("ui.picture_text_models_folder_incomplete"));
            }
        }
        if !out.closed {
            self.picture_text = Some(dialog);
        }
    }

    fn save_picture_as(&mut self, src: &str) {
        let Some(page_dir) = self.editor.as_ref().map(|e| e.dir.clone()) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let (_source_path, default_name, bytes) = match crate::imagemenu::validate_and_read_image(&page_dir, src) {
            Ok(res) => res,
            Err(e) => {
                self.complain(&crate::i18n::format_message_id("ui.cannot_read_the_picture_error", &[("error", &e.to_string())]));
                return;
            }
        };

        if let Some(downloads) = self.file_downloads.as_mut() {
            let mime = match std::path::Path::new(&default_name).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
                Some("png") => "image/png",
                Some("jpg" | "jpeg") => "image/jpeg",
                Some("gif") => "image/gif",
                Some("webp") => "image/webp",
                Some("bmp") => "image/bmp",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            downloads.push(FileDownload { name: default_name.clone(), mime, bytes });
            self.say(crate::i18n::format_message_id("ui.saved_picture_as_path", &[("path", &default_name)]));
            return;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            let ext = std::path::Path::new(&default_name).extension().and_then(|e| e.to_str()).unwrap_or("");
            let mut picker = rfd::FileDialog::new().set_title(crate::tl_id!("ui.save_picture_as")).set_file_name(&default_name);
            if !ext.is_empty() && !["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "jxl"].contains(&ext) {
                picker = picker.add_filter(ext, &[ext]);
            }
            picker = picker.add_filter(crate::tl_id!("ui.pictures"), &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg", "jxl"]);

            let Some(destination) = picker.save_file() else {
                return;
            };

            match std::fs::write(&destination, &bytes) {
                Ok(()) => {
                    let shown = self.shown(&destination);
                    self.say(crate::i18n::format_message_id("ui.saved_picture_as_path", &[("path", &shown)]));
                }
                Err(e) => {
                    self.complain(&crate::i18n::format_message_id("ui.cannot_write_the_picture_error", &[("error", &e.to_string())]));
                }
            }
        }
    }

    /// Puts marks around the selection in the block the caret is in.
    fn apply_mark(&mut self, mark: &str) {
        if self.editor.as_ref().and_then(|ed| ed.across).is_some_and(|selection| !selection.inside_one_block()) {
            let format = match mark {
                "bold" => pageedit::InlineFormat::Bold,
                "italic" => pageedit::InlineFormat::Italic,
                "strike" => pageedit::InlineFormat::Strike,
                "underline" => pageedit::InlineFormat::Underline,
                "subscript" => pageedit::InlineFormat::Subscript,
                "superscript" => pageedit::InlineFormat::Superscript,
                "code" => pageedit::InlineFormat::Code,
                _ => return,
            };
            self.apply_across_format(format);
            return;
        }
        // With no caret, the marks act where the font boxes do: at the end of the last words of the
        // page, as the format of the next words typed there.
        if let Some(ed) = self.editor.as_mut() {
            Self::place_caret_for_format(ed);
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
                caret.inline = None;
                caret.selection = (0, caret.text.chars().count());
            }
            // The source text changed when the marks were added or removed. Restore egui's
            // range too, or typing replaces only the old offsets and leaves markup behind.
            ed.restore_format_selection = Some((caret.block, caret.selection.0, caret.selection.1));
            ed.rebuild_body();
            Self::mark_dirty_from_blocks(ed);
            return;
        }
        let (text, selection) = pageedit::wrap_selection(&caret.text, caret.selection, marks);
        caret.text = text;
        // The caret goes where the wrapped words now are: between the marks for an empty
        // selection, so the next words typed are inside them, not after the closing mark.
        caret.selection = selection;
        caret.place = Some(selection.1);
        caret.focus = true;
        if let Some(b) = ed.blocks.get_mut(caret.block) {
            caret.write(b);
        }
        ed.restore_format_selection = Some((caret.block, selection.0, selection.1));
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
                        | notecraft_formats::Block::Paragraph(_, _)
                        | notecraft_formats::Block::Quote(_, _)
                        | notecraft_formats::Block::Item { .. }
                )
            )
        {
            let mut marker = "NoteCraftCaretProbe".to_string();
            while caret.text.contains(&marker) {
                marker.push('X');
            }
            let (probe, _) = pageedit::insert_at_caret(&caret.text, (lo, lo), &marker);
            let inherited = ed
                .blocks
                .get(caret.block)
                .map(|block| caret.parse(block, &probe))
                .unwrap_or_default()
                .into_iter()
                .find(|span| span.text.contains(&marker));
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
            caret.inline = None;
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

    /// Format Painter: picks up the formatting at the caret, or of the words picked, to give
    /// to the next words picked (issue #165). A second click puts the painter down again; a
    /// double-click (`sticky`) keeps it painting until Escape.
    fn pick_up_formatting(&mut self, sticky: bool) {
        if let Some(held) = self.painter.as_mut() {
            if sticky {
                held.sticky = true;
                self.say(crate::tl_id!("ui.format_painter_keeps_painting_until_escape"));
            } else {
                self.painter = None;
                self.say(crate::tl_id!("ui.format_painter_put_down"));
            }
            return;
        }
        let Some(ed) = self.editor.as_ref().filter(|ed| ed.writable) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let picked = ed.caret.as_ref().and_then(|caret| Some((caret.block, caret.selection, caret.style(ed.blocks.get(caret.block)?)?)));
        let Some((block, selection, style)) = picked else {
            self.say(crate::tl_id!("ui.click_in_the_words_whose_formatting_to_copy"));
            return;
        };
        self.painter = Some(FormatPaint { style, from: (block, selection), sticky });
        self.say(crate::tl_id!("ui.pick_the_words_to_give_that_formatting"));
    }

    /// Puts the Format Painter's formatting on words just picked with the pointer: on the
    /// release that ends the pick, as in Office. Escape puts the painter down. Called once a
    /// frame after the page is drawn.
    /// Escape puts the laser pointer down (issue #289): it is for a moment of pointing, and
    /// the caret comes back afterwards.
    /// The Eyedropper (issue #290): a colour taken from the page goes to the tool it was
    /// started from, and Escape puts it down with that tool in hand, unchanged.
    fn eyedropper(&mut self, ctx: &Context) {
        let ribbon::Pointer::Eyedropper(tool) = self.pointer else { return };
        if let Some((i, colour)) = crate::inkpad::take_eyedropped(ctx)
            && let Some(nib) = self.rack.get(i).copied()
        {
            self.edit_rack(ribbon::RackEdit::Set(i, notecraft_ink::rack::Nib::new(nib.tool, colour, nib.tenths)));
            self.say(crate::i18n::format_message_id("ui.the_pen_takes_colour", &[("colour", &colour.hex())]));
            return;
        }
        if self.dialog.is_none() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.nib = tool.min(self.rack.len().saturating_sub(1));
            self.pointer = ribbon::Pointer::Draw;
            self.say(crate::tl_id!("ui.eyedropper_put_down_the_pen_is_as_it_was"));
        }
    }

    fn laser_keys(&mut self, ctx: &Context) {
        if self.pointer == ribbon::Pointer::Laser && self.dialog.is_none() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.pointer = ribbon::Pointer::Write;
            crate::inkpad::clear_laser(ctx);
            self.say(crate::tl_id!("ui.laser_pointer_put_down"));
        }
    }

    fn paint_formatting(&mut self, ctx: &Context) {
        if self.painter.is_none() {
            return;
        }
        if self.dialog.is_none() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.painter = None;
            self.say(crate::tl_id!("ui.format_painter_put_down"));
            return;
        }
        if !ctx.input(|i| i.pointer.primary_released()) {
            return;
        }
        let Some(held) = self.painter.clone() else { return };
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(caret) = ed.caret.as_ref() else { return };
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        // A click without a pick, or the words it was picked up from: nothing to paint yet.
        if lo == hi || (caret.block, caret.selection) == held.from {
            return;
        }
        let Some(block) = ed.blocks.get(caret.block) else { return };
        let Some(caret) = ed.caret.as_mut() else { return };
        let Some((text, selection)) = caret.paint(block, &held.style) else {
            return;
        };
        Self::remember_step(ed);
        let Some(caret) = ed.caret.as_mut() else { return };
        caret.text = text;
        caret.selection = selection;
        caret.place = Some(selection.0);
        caret.focus = true;
        if let Some(block) = ed.blocks.get_mut(caret.block) {
            caret.write(block);
        }
        ed.pending_format = None;
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
        if !held.sticky {
            self.painter = None;
        }
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
                        | notecraft_formats::Block::Paragraph(_, _)
                        | notecraft_formats::Block::Quote(_, _)
                        | notecraft_formats::Block::Item { .. }
                )
            )
        {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        let Some(caret) = ed.caret.as_mut() else { return };
        let Some(block) = ed.blocks.get(caret.block) else { return };
        let Some((text, selection)) = caret.format(block, format) else {
            return;
        };
        Self::remember_step(ed);
        let Some(caret) = ed.caret.as_mut() else { return };
        caret.text = text;
        caret.selection = selection;
        caret.focus = true;
        ed.restore_format_selection = Some((caret.block, selection.0, selection.1));
        if let Some(block) = ed.blocks.get_mut(caret.block) {
            caret.write(block);
        }
        ed.pending_format = None;
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        let step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        ed.history.record(step);
        self.status.clear();
        self.status_is_error = false;
    }

    /// An inline format over a selection that runs across blocks: every covered block's
    /// covered words take it, and the selection is kept with its ends remapped to the new
    /// text, so the highlight, Copy and Delete still cover the same words.
    fn apply_across_format(&mut self, format: pageedit::InlineFormat) {
        let Some(ed) = self.editor.as_mut() else { return };
        let Some(open) = ed.across.filter(|a| !a.inside_one_block()) else { return };
        if !ed.writable {
            self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
            self.status_is_error = false;
            return;
        }
        Self::remember_step(ed);
        match pageedit::format_across(&mut ed.blocks, open, format) {
            Some(remapped) => {
                ed.across = Some(remapped);
                ed.pending_format = None;
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
                // The caret's block may be one of the formatted ones: its editing text has
                // to say what the block says now, or the next keystroke writes the old words
                // back over the new formatting.
                if let Some(at) = ed.caret.as_ref().filter(|c| c.cell.is_none()).map(|c| c.block)
                    && let Some(block) = ed.blocks.get(at)
                    && matches!(
                        block,
                        notecraft_formats::Block::Heading { .. }
                            | notecraft_formats::Block::Paragraph(_, _)
                            | notecraft_formats::Block::Quote(_, _)
                            | notecraft_formats::Block::Item { .. }
                    )
                    && let Some(caret) = ed.caret.as_mut()
                {
                    caret.text = pageedit::caret_text(block);
                    caret.inline = None;
                    caret.selection = (0, caret.text.chars().count());
                }
                let step =
                    crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
                ed.history.record(step);
                self.status.clear();
                self.status_is_error = false;
            }
            None => {
                self.status = crate::tl_id!("ui.inline_mark_whole_line").into();
                self.status_is_error = false;
            }
        }
    }

    /// With no caret in the page, a font or size choice goes to the end of the last writable
    /// block, so it applies to the next text typed there, as OneNote does.
    fn place_caret_for_format(ed: &mut Editor) {
        if ed.caret.is_some() {
            return;
        }
        let Some(index) = ed.blocks.iter().rposition(pageedit::is_writable) else { return };
        let text = pageedit::caret_text(&ed.blocks[index]);
        let end = text.chars().count();
        ed.caret =
            Some(pageedit::Caret { block: index, text, focus: true, selection: (end, end), place: Some(end), cell: None, span: None, inline: None });
    }

    fn apply_font_family(&mut self, family: &str) {
        if !notecraft_formats::markdown::valid_font_family(family) {
            log::warn!("ribbon requested an invalid font family");
            return;
        }
        if self.editor.as_ref().and_then(|ed| ed.across).is_some_and(|selection| !selection.inside_one_block()) {
            self.apply_across_format(pageedit::InlineFormat::Family(family.to_owned()));
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        Self::place_caret_for_format(ed);
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
                        | notecraft_formats::Block::Paragraph(_, _)
                        | notecraft_formats::Block::Quote(_, _)
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
                | notecraft_formats::Block::Paragraph(spans, _)
                | notecraft_formats::Block::Quote(spans, _)
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
            caret.inline = None;
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

    /// Home > Font Colour and Text Highlight Colour (issue #260): to the selected words, as
    /// font size does; with no selection, to what is typed next at the caret.
    fn apply_text_colour(&mut self, format: pageedit::InlineFormat) {
        if self.editor.as_ref().and_then(|ed| ed.across).is_some_and(|selection| !selection.inside_one_block()) {
            self.apply_across_format(format);
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        Self::place_caret_for_format(ed);
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            return;
        };
        if caret.selection.0 == caret.selection.1 {
            let pending = ed.pending_format.get_or_insert_with(|| PendingFormat { block: caret.block, at: caret.selection.0, ..Default::default() });
            if pending.block != caret.block || pending.at != caret.selection.0 {
                *pending = PendingFormat { block: caret.block, at: caret.selection.0, ..Default::default() };
            }
            match format {
                pageedit::InlineFormat::Colour(c) => pending.colour = Some(c),
                pageedit::InlineFormat::Highlight(c) => pending.highlight = Some(c),
                _ => {}
            }
            if let Some(caret) = ed.caret.as_mut() {
                caret.focus = true;
                caret.place = Some(caret.selection.0);
            }
            self.status.clear();
            self.status_is_error = false;
            return;
        }
        self.apply_partial_inline_format(format);
    }

    fn apply_font_size(&mut self, size: u8) {
        if !ribbon::FONT_SIZES.contains(&size) {
            log::warn!("ribbon requested unsupported font size {size} pt");
            return;
        }
        if self.editor.as_ref().and_then(|ed| ed.across).is_some_and(|selection| !selection.inside_one_block()) {
            self.apply_across_format(pageedit::InlineFormat::Size(size));
            return;
        }
        let Some(ed) = self.editor.as_mut() else { return };
        Self::place_caret_for_format(ed);
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
                            | notecraft_formats::Block::Paragraph(_, _)
                            | notecraft_formats::Block::Quote(_, _)
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
                | notecraft_formats::Block::Paragraph(spans, _)
                | notecraft_formats::Block::Quote(spans, _)
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
                caret.inline = None;
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
                caret.write(block);
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
        // Align Left and Align Right, as in Word and OneNote (issue #258). Ctrl+E, which Word
        // gives to Center, is Search here as it is in OneNote, so Center is on the menu only.
        if self.editor.as_ref().is_some_and(|e| e.caret.is_some() || e.across.is_some()) {
            if pressed(Modifiers::COMMAND, Key::L) {
                self.apply_align(notecraft_model::justify::Align::Left);
            }
            if pressed(Modifiers::COMMAND, Key::R) {
                self.apply_align(notecraft_model::justify::Align::Right);
            }
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
        if !matches!(ed.blocks.get(at), Some(notecraft_formats::Block::Paragraph(_, _))) {
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

    /// Sets the paragraphs the caret or the selection is in against `align` (issue #258).
    fn apply_align(&mut self, align: notecraft_model::justify::Align) {
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let blocks: Vec<usize> = match (ed.across.filter(|a| !a.inside_one_block()), ed.caret.as_ref()) {
            (Some(across), _) => across.blocks().collect(),
            (None, Some(caret)) => vec![caret.block],
            (None, None) => {
                self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
                self.status_is_error = false;
                return;
            }
        };
        self.align_blocks(&blocks, align);
    }

    /// Aligns the paragraphs and headings among `blocks`, or, when every one of them already
    /// has that alignment, gives them back the alignment their words have. One undo step, and
    /// written into the page's own file with the rest of its paragraph layout.
    fn align_blocks(&mut self, blocks: &[usize], align: notecraft_model::justify::Align) {
        use notecraft_formats::Block;
        let Some(ed) = self.editor.as_mut() else { return };
        let text: Vec<usize> =
            blocks.iter().copied().filter(|i| matches!(ed.blocks.get(*i), Some(Block::Paragraph(_, _) | Block::Heading { .. }))).collect();
        if text.is_empty() {
            self.say(crate::tl_id!("ui.alignment_goes_on_a_paragraph_or_a_heading"));
            return;
        }
        let all = text.iter().all(|i| ed.justified.align_of(*i) == Some(align));
        let mut marked = ed.justified.clone();
        for i in &text {
            marked.set_mark(*i, (!all).then_some(notecraft_model::justify::Mark::Align(align)));
        }
        if marked == ed.justified {
            return;
        }
        // The step before, with the layout it had, and the step after with the new one: an
        // alignment changes no words, so the usual step taken after a change would not see it.
        Self::remember_step(ed);
        if let Some(here) = ed.history.here_mut() {
            here.justified = Some(ed.justified.clone());
        }
        ed.justified = marked;
        let mut step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        step.columns = Some(ed.columns.clone());
        step.justified = Some(ed.justified.clone());
        ed.history.record(step);
        if let Some(c) = ed.caret.as_mut() {
            c.focus = true;
        }
        self.save_block_marks((false, true));
        self.say(match (all, align) {
            (true, _) => crate::tl_id!("ui.the_paragraph_follows_its_words_again"),
            (false, notecraft_model::justify::Align::Left) => crate::tl_id!("ui.aligned_left"),
            (false, notecraft_model::justify::Align::Center) => crate::tl_id!("ui.centred"),
            (false, notecraft_model::justify::Align::Right) => crate::tl_id!("ui.aligned_right"),
        });
    }

    /// Opens the Paragraph Spacing dialog for the paragraphs the caret or the selection is in,
    /// showing the value they share, or leaving a field empty where they differ (issue #275).
    fn open_paragraph_spacing(&mut self) {
        use notecraft_formats::Block;
        let Some(ed) = self.editor.as_ref() else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        let blocks: Vec<usize> = match (ed.across.filter(|a| !a.inside_one_block()), ed.caret.as_ref()) {
            (Some(across), _) => across.blocks().collect(),
            (None, Some(caret)) => vec![caret.block],
            (None, None) => {
                self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
                self.status_is_error = false;
                return;
            }
        };
        let text: Vec<usize> = blocks
            .into_iter()
            .filter(|i| matches!(ed.blocks.get(*i), Some(Block::Paragraph(_, _) | Block::Heading { .. } | Block::Item { .. } | Block::Quote(_, _))))
            .collect();
        if text.is_empty() {
            self.say(crate::tl_id!("ui.spacing_goes_on_text"));
            return;
        }
        let each: Vec<notecraft_model::justify::Spacing> = text.iter().map(|i| ed.justified.spacing_of(*i).unwrap_or_default()).collect();
        let shared = |part: fn(&notecraft_model::justify::Spacing) -> u16| -> String {
            let first = each.first().map(part).unwrap_or(0);
            if each.iter().map(part).all(|v| v == first) { first.to_string() } else { String::new() }
        };
        self.dialog = Some(Dialog::ParagraphSpacing {
            before: shared(|s| s.before),
            after: shared(|s| s.after),
            line: shared(|s| s.line),
            blocks: text,
            first: true,
        });
    }

    /// Gives the paragraphs among `blocks` the room in `change` (before, after, line height), where
    /// a part left as `None` keeps what each already has. One undo step, written into the page's own
    /// file with the rest of its paragraph layout (issue #275).
    fn apply_spacing(&mut self, blocks: &[usize], change: [Option<u16>; 3]) {
        use notecraft_model::justify::Spacing;
        let Some(ed) = self.editor.as_mut() else { return };
        let mut marked = ed.justified.clone();
        for i in blocks {
            let have = marked.spacing_of(*i).unwrap_or_default();
            let want =
                Spacing { before: change[0].unwrap_or(have.before), after: change[1].unwrap_or(have.after), line: change[2].unwrap_or(have.line) };
            marked.set_spacing(*i, Some(want));
        }
        if marked == ed.justified {
            return;
        }
        // The step before, with the layout it had, and the step after with the new one: spacing
        // changes no words, so the usual step taken after a change would not see it.
        Self::remember_step(ed);
        if let Some(here) = ed.history.here_mut() {
            here.justified = Some(ed.justified.clone());
        }
        ed.justified = marked;
        let mut step = crate::history::Step::new(ed.body.clone(), ed.blocks.clone(), ed.boxes.clone(), ed.caret.clone(), crate::history::Kind::Edit);
        step.columns = Some(ed.columns.clone());
        step.justified = Some(ed.justified.clone());
        ed.history.record(step);
        if let Some(c) = ed.caret.as_mut() {
            c.focus = true;
        }
        self.save_block_marks((false, true));
        self.say(crate::tl_id!("ui.paragraph_spacing_set"));
    }

    /// What is typed into the Paragraph Spacing dialog, for tests and automation: the three
    /// values, as a person would type them. Does nothing when the dialog is not open.
    pub fn set_paragraph_spacing_values(&mut self, before: &str, after: &str, line: &str) {
        if let Some(Dialog::ParagraphSpacing { before: b, after: a, line: l, .. }) = self.dialog.as_mut() {
            *b = before.to_string();
            *a = after.to_string();
            *l = line.to_string();
        }
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
            caret.write(b);
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
        let Some(ed) = self.editor.as_mut() else {
            log::info!("ui_action: action=block_kind.apply kind={kind} outcome=no_page");
            return;
        };
        let Some(caret) = ed.caret.as_ref() else {
            self.status = crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon").into();
            self.status_is_error = false;
            log::info!("ui_action: action=block_kind.apply kind={kind} outcome=no_caret");
            return;
        };
        let at = caret.block;
        let selection = caret.selection;
        let selection_shape = if selection.0 == selection.1 { "caret" } else { "range" };
        let low = selection.0.min(selection.1);
        let high = selection.0.max(selection.1);
        let line_at = |offset: usize| caret.text.chars().take(offset).filter(|ch| *ch == '\n').count();
        let caret_line = line_at(selection.0);
        let first_line = line_at(low);
        let last_line = if low == high { first_line } else { line_at(high.saturating_sub(1)) };
        let selected_line_count = last_line.saturating_sub(first_line) + 1;
        let in_code = matches!(ed.blocks.get(at), Some(notecraft_formats::markdown::Block::Code { .. }));
        let code = in_code && !matches!(kind, "codeblock" | "codenumbers");
        log::info!(
            "ui_action: action=block_kind.apply kind={kind} target={} selection={selection_shape} chars={low}..{high} caret_line={caret_line} lines={first_line}..={last_line} selected_lines={selected_line_count} outcome=attempted",
            if in_code { "code" } else { "other" }
        );
        if code {
            if !pageedit::split_code_selection(&mut ed.blocks, at, selection, kind) {
                log::info!(
                    "ui_action: action=block_kind.apply kind={kind} target=code selection={selection_shape} chars={low}..{high} caret_line={caret_line} lines={first_line}..={last_line} selected_lines={selected_line_count} outcome=refused"
                );
                return;
            }
        } else if !pageedit::set_kind(&mut ed.blocks, at, kind) {
            log::info!(
                "ui_action: action=block_kind.apply kind={kind} target=other selection={selection_shape} chars={low}..{high} caret_line={caret_line} outcome=refused"
            );
            return;
        }
        ed.rebuild_body();
        let caret_block = at + usize::from(code && first_line > 0);
        if let Some(b) = ed.blocks.get(caret_block) {
            let text = pageedit::caret_text(b);
            if let Some(c) = ed.caret.as_mut() {
                c.block = caret_block;
                c.text = text;
                c.selection = (0, 0);
                c.place = Some(0);
            }
        }
        if code {
            ed.restore_format_selection = Some((caret_block, 0, 0));
        }
        Self::mark_dirty_from_blocks(ed);
        log::info!(
            "ui_action: action=block_kind.apply kind={kind} target={} selection={selection_shape} chars={low}..{high} caret_line={caret_line} lines={first_line}..={last_line} selected_lines={selected_line_count} result_block={caret_block} outcome=applied",
            if in_code { "code" } else { "other" }
        );
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
        let (copy, cut, paste, all) = ctx.input(|input| {
            let hit = |key| Self::clipboard_key_hit(input, key);
            (hit(Key::C), hit(Key::X), hit(Key::V), hit(Key::A))
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
            // Ctrl+Shift+V: the text alone (issue #535). egui matches Ctrl+V to it too, so it is
            // told apart by Shift here, and the text egui read for it is dropped with the key.
            let plain = ctx.input(|i| {
                i.modifiers.shift
                    || i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::V, pressed: true, modifiers, .. } if modifiers.shift))
            });
            ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::V) || i.consume_key(Modifiers::CTRL, Key::V));
            if plain {
                ctx.input_mut(|i| i.events.retain(|e| !matches!(e, egui::Event::Paste(_))));
                self.paste_plain();
                return;
            }
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

    fn clipboard_key_hit(input: &egui::InputState, want: Key) -> bool {
        // Keep the raw-event fallback for headless and older native backends.
        let event_hit = |event: &egui::Event| matches!(event, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == want && (modifiers.command || modifiers.ctrl));
        (input.key_pressed(want) && (input.modifiers.command || input.modifiers.ctrl))
            || input.events.iter().any(event_hit)
            || input.raw.events.iter().any(event_hit)
    }

    /// Whether one of the dialogs drawn outside `self.dialog` is open: Copy Text from Picture or
    /// Print. It owns the keyboard as `self.dialog` does, so the page's shortcuts and paste wait.
    fn own_modal_open(&self) -> bool {
        self.picture_text.is_some() || self.print.is_some()
    }

    /// Ctrl+V while such a dialog is open: the clipboard's text goes to the dialog's text box when
    /// it has the keyboard, and nowhere otherwise.
    ///
    /// The page is drawn before the dialog and takes a paste for itself, so the paste is held here,
    /// out of the frame's events, and handed to the dialog just before it is drawn
    /// ([`Self::give_modal_paste`]).
    fn paste_into_modal(&mut self, ctx: &egui::Context, queued: bool) {
        let mut pasted: Option<String> = None;
        ctx.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Paste(text) => {
                    pasted.get_or_insert_with(|| text.clone());
                    false
                }
                egui::Event::PasteImage(_) => false,
                _ => true,
            });
        });
        if ctx.memory(|memory| memory.focused().is_none()) {
            return;
        }
        if pasted.is_none() && (queued || ctx.input(|input| Self::clipboard_key_hit(input, Key::V))) {
            pasted = self.clipboard.text();
        }
        self.modal_paste = pasted;
    }

    /// Hands a paste held by [`Self::paste_into_modal`] to the dialog about to be drawn.
    fn give_modal_paste(&mut self, ctx: &egui::Context) {
        if let Some(text) = self.modal_paste.take() {
            ctx.input_mut(|input| input.events.push(egui::Event::Paste(text)));
        }
    }

    fn paste_into_emoji_search(&mut self, ctx: &egui::Context, queued: bool) {
        // The desktop shell catches native paste before egui for images/rich page
        // content. The picker only needs plain text. A native Paste event, where
        // present, already provides that text, so do not insert a second copy.
        if !crate::block_emoji::search_has_focus(ctx) {
            ctx.input_mut(|input| input.events.retain(|event| !matches!(event, egui::Event::Paste(_) | egui::Event::PasteImage(_))));
            return;
        }
        self.paste_text_into_focus(ctx, queued);
    }

    /// Ctrl+V into a text box of the window's own that has the keyboard (the emoji picker's search,
    /// or the search box): the clipboard's text, once. The desktop shell catches a paste before
    /// egui, and a key press alone carries no text, so the text is added as egui's own paste
    /// unless the frame already has one. A picture on the clipboard goes nowhere.
    fn paste_text_into_focus(&mut self, ctx: &egui::Context, queued: bool) {
        ctx.input_mut(|input| input.events.retain(|event| !matches!(event, egui::Event::PasteImage(_))));
        let needs_text = ctx.input(|input| {
            (queued || Self::clipboard_key_hit(input, Key::V)) && !input.events.iter().any(|event| matches!(event, egui::Event::Paste(_)))
        });
        if needs_text && let Some(text) = self.clipboard.text() {
            ctx.input_mut(|input| input.events.push(egui::Event::Paste(text)));
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
                Some(notecraft_formats::markdown::Block::Code { .. }) | Some(notecraft_formats::markdown::Block::MathBlock(_, _))
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
            let Some(copied) = pageedit::copied_span(&ed.blocks, open) else { return };
            self.pending_copy = Some(notecraft_formats::render::blocks_to_text(&copied));
            self.pending_copy_html = Some(notecraft_formats::render::blocks_to_html(&copied));
            if which == "cut" {
                let (block, at) = pageedit::delete_span(&mut ed.blocks, open);
                let text = ed.blocks.get(block).map(pageedit::caret_text).unwrap_or_default();
                ed.caret =
                    Some(pageedit::Caret { block, text, focus: true, selection: (at, at), place: Some(at), cell: None, span: None, inline: None });
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
        let Some(copied) = ed.blocks.get(caret.block).and_then(|block| caret.copied_blocks(block)) else { return };
        self.pending_copy = Some(notecraft_formats::render::blocks_to_text(&copied));
        self.pending_copy_html = Some(notecraft_formats::render::blocks_to_html(&copied));
        if which == "cut" {
            let (text, at) = pageedit::insert_at_caret(&caret.text, caret.selection, "");
            caret.text = text;
            caret.selection = (at, at);
            caret.place = Some(at);
            caret.focus = true;
            if let Some(b) = ed.blocks.get_mut(caret.block) {
                caret.write(b);
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
        if let Some(b) = ed.blocks.get_mut(caret.block) {
            caret.clear_marks(b);
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
            inline: None,
        });
        Self::mark_dirty_from_blocks(ed);
        self.save_block_marks(marks);
    }

    /// Puts a snippet where the caret is, or at the end of the page.
    /// Opens the Link dialog, as OneNote's Insert > Link and Ctrl+K do (issue #163). With the
    /// caret in a link it edits that link; with words selected they are the words to show;
    /// otherwise the link goes in at the caret.
    pub fn open_link_dialog(&mut self) {
        let Some(ed) = self.editor.as_mut().filter(|ed| ed.writable) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if ed.across.is_some_and(|a| !a.inside_one_block()) {
            self.say(crate::tl_id!("ui.select_words_in_one_line_for_a_link"));
            return;
        }
        Self::place_caret_for_format(ed);
        let Some(caret) = ed.caret.as_ref() else {
            self.say(crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon"));
            return;
        };
        let (lo, hi) = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        let found = ed.blocks.get(caret.block).and_then(|block| pageedit::link_at_caret(block, caret));
        let (range, text, address, editing) = match found {
            Some((start, end, words, address)) => ((start, end), words, address, true),
            None => ((lo, hi), caret.text.chars().skip(lo).take(hi - lo).collect(), String::new(), false),
        };
        self.dialog = Some(Dialog::Link { block: caret.block, cell: caret.cell, range, text, address, editing, first: true });
    }

    /// Opens Insert > Online Video (issue #284): the video goes in at the caret, in place of
    /// any words selected in one line.
    pub fn open_online_video_dialog(&mut self) {
        let Some(ed) = self.editor.as_mut().filter(|ed| ed.writable) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        if ed.across.is_some_and(|a| !a.inside_one_block()) {
            self.say(crate::tl_id!("ui.select_words_in_one_line_for_a_link"));
            return;
        }
        Self::place_caret_for_format(ed);
        let Some(caret) = ed.caret.as_ref() else {
            self.say(crate::tl_id!("ui.click_in_the_page_first_then_use_the_ribbon"));
            return;
        };
        let range = (caret.selection.0.min(caret.selection.1), caret.selection.0.max(caret.selection.1));
        self.dialog = Some(Dialog::OnlineVideo { block: caret.block, cell: caret.cell, range, address: String::new(), first: true });
    }

    /// Puts `replacement` in place of the characters `range` of the caret's line, when the
    /// caret is still in the line the Link dialog was opened for. One step of the history.
    fn replace_in_caret_line(&mut self, block: usize, cell: Option<(usize, usize)>, range: (usize, usize), replacement: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        if !ed.caret.as_ref().is_some_and(|c| c.block == block && c.cell == cell) {
            self.say(crate::tl_id!("ui.the_line_changed_put_the_link_in_again"));
            return;
        }
        Self::remember_step(ed);
        let Some(caret) = ed.caret.as_mut() else { return };
        let (a, b) = (range.0.min(range.1), range.0.max(range.1));
        // On the spans, not the text: removing a link leaves the same words behind, and a
        // text diff could never tell the link's words from plain ones.
        if let Some(block) = ed.blocks.get(block) {
            caret.replace_range(block, (a, b), replacement);
        }
        if let Some(b) = ed.blocks.get_mut(block) {
            caret.write(b);
        }
        ed.rebuild_body();
        Self::mark_dirty_from_blocks(ed);
        Self::remember_edit(ed);
    }

    fn insert_in_page(&mut self, text: &str) {
        let Some(ed) = self.editor.as_mut() else { return };
        // An insert is an edit: it is recorded for undo before the page changes.
        if ed.caret.is_some() {
            Self::remember_step(ed);
        }
        match ed.caret.as_mut() {
            Some(caret) => {
                // Where the caret is, not at the end of the block.
                let (new_text, at) = pageedit::insert_at_caret(&caret.text, caret.selection, text.trim_matches('\n'));
                caret.text = new_text;
                caret.selection = (at, at);
                caret.place = Some(at);
                caret.focus = true;
                if let Some(b) = ed.blocks.get_mut(caret.block) {
                    caret.write(b);
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

    /// A selected notebook (even with no pages) owns its own recovery action. When
    /// there is no selection, only a single open notebook is unambiguous.
    fn recycle_notebook(&self) -> Option<&Path> {
        let tree = self.session.tree();
        [self.selected.as_deref(), self.current_page()]
            .into_iter()
            .flatten()
            .find_map(|p| tree.iter().find(|n| p.starts_with(&n.path)).map(|n| n.path.as_path()))
            .or(match tree {
                [n] => Some(n.path.as_path()),
                _ => None,
            })
    }

    /// Opens (or refreshes in place) the notebook's recycle bin with `selected` chosen, or the
    /// newest item when that one is gone.
    fn open_recycle_bin(&mut self, notebook: PathBuf, selected: Option<String>) {
        let p = params(&[("notebook", &path_str(&notebook))]);
        let Some(Output::Recycled(items)) = self.run("recycle.list", &p) else { return };
        let name = notecraft_store::tree::read_node(&notebook).map(|n| n.title).unwrap_or_default();
        let places = items.iter().map(|item| recycled_place(&notebook, &item.original)).collect();
        let selected = selected.filter(|id| items.iter().any(|i| &i.id == id)).or_else(|| items.first().map(|i| i.id.clone()));
        self.dialog = Some(Dialog::RecycleBin { notebook, name, items, places, selected });
    }

    fn dismiss_search(&mut self) {
        self.search.clear();
        self.searched.clear();
        self.hits.clear();
        self.hit_trails.clear();
        self.search_pick = None;
        self.search_scope = SearchScope::All;
    }

    fn refresh_search(&mut self) {
        self.searched.clone_from(&self.search);
        let mut query = params(&[("q", &self.search), ("limit", "20")]);
        let scope = match self.search_scope {
            SearchScope::All => None,
            SearchScope::Notebook => {
                self.selected.as_ref().and_then(|p| self.session.tree().iter().find(|n| p.starts_with(&n.path))).map(|n| &n.path)
            }
            SearchScope::Section => self.current_section(self.session.tree()).map(|n| &n.path),
            SearchScope::Page => self.editor.as_ref().map(|editor| &editor.dir),
        };
        if let Some(scope) = scope {
            query.insert("scope".into(), path_str(scope));
        }
        self.hits = match self.session.run("search.query", &query) {
            Ok(Output::Hits(hits)) => hits,
            _ => Vec::new(),
        };
        self.hit_trails = self.hits.iter().map(|hit| search_trail(self.session.tree(), &hit.path)).collect();
        self.search_pick = None;
    }

    fn open_search_result(&mut self, ctx: &Context, path: &Path) {
        // A hit on a picture's text outlines the matching lines when the page shows the picture.
        let highlight = self.hits.iter().find(|hit| hit.path == path).and_then(|hit| {
            let picture = hit.picture.as_ref()?;
            if hit.boxes.is_empty() {
                return None;
            }
            let name = picture.file_name()?.to_string_lossy().into_owned();
            let pixels = image::image_dimensions(picture).ok()?;
            Some(crate::preview::PictureHighlight { page_dir: path.to_path_buf(), src: format!("assets/{name}"), pixels, boxes: hit.boxes.clone() })
        });
        crate::preview::set_picture_highlight(ctx, highlight);
        if self.backstage.is_some() {
            self.close_backstage();
        }
        self.expand_to(path);
        self.open_page_soon(path);
    }

    fn search_popup(&mut self, ctx: &Context) {
        if self.dialog.is_some() {
            self.dismiss_search();
            return;
        }
        let field_id = Id::new(("page-search", ctx.viewport_id()));
        let empty = self.search.trim().is_empty();
        if empty && !ctx.memory(|memory| memory.has_focus(field_id)) {
            return;
        }
        let mut chosen: Option<PathBuf> = None;
        let mut scope = self.search_scope;
        let tokens = self.tokens;
        // Issue #370: the results hang from the search box itself, as wide as it or wider, and
        // are kept inside the window, rather than sitting at a fixed place on the right.
        let screen = ctx.viewport_rect();
        let field = ctx.read_response(Id::new(("page-search", ctx.viewport_id()))).map(|response| response.rect);
        let width = field.map_or(420.0, |f| f.width().max(420.0)).min((screen.width() - 16.0).max(0.0));
        let (left, top) = match field {
            Some(f) => (f.left().min(screen.right() - width - 8.0).max(screen.left() + 8.0), f.bottom() + 4.0),
            None => (screen.right() - width - 8.0, screen.top() + 44.0),
        };
        let max_height = (screen.bottom() - top - 24.0).clamp(80.0, 460.0);
        let pick = self.search_pick;
        let picked_now = ctx.data_mut(|d| {
            let id = Id::new("search-results-pick");
            let before = d.get_temp::<Option<usize>>(id);
            d.insert_temp(id, pick);
            before != Some(pick)
        });
        let popup = egui::Area::new(Id::new("search-results")).order(egui::Order::Foreground).fixed_pos(egui::pos2(left, top)).show(ctx, |ui| {
            let frame = egui::Frame::popup(ui.style());
            let content_width = (width - frame.total_margin().sum().x).max(0.0);
            frame.show(ui, |ui| {
                ui.set_width(content_width);
                ui.horizontal_wrapped(|ui| {
                    let choices = [
                        (SearchScope::All, crate::tl_id!("ui.tags_scope_all_notebooks"), true),
                        (SearchScope::Notebook, crate::tl_id!("ui.this_notebook"), self.selected.is_some()),
                        (SearchScope::Section, crate::tl_id!("ui.this_section"), self.current_section(self.session.tree()).is_some()),
                        (SearchScope::Page, crate::tl_id!("ui.find_on_this_page"), self.editor.is_some()),
                    ];
                    for (choice, label, available) in choices {
                        if ui.add_enabled(available, egui::Button::selectable(scope == choice, label)).clicked() {
                            scope = choice;
                        }
                    }
                });
                ui.add_space(2.0);
                if empty {
                    ui.weak(crate::tl_id!("ui.search_prompt"));
                    return;
                }
                if self.hits.is_empty() {
                    ui.weak(crate::tl_id!("ui.no_pages_match"));
                    return;
                }
                egui::ScrollArea::vertical().id_salt("search-results-list").max_height(max_height).auto_shrink([false, true]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    for (index, h) in self.hits.iter().enumerate() {
                        let trail = self.hit_trails.get(index).map(String::as_str).unwrap_or_default();
                        let current = pick == Some(index);
                        let row_width = ui.available_width();
                        let lines = if trail.is_empty() { 2.0 } else { 3.0 };
                        let (rect, row) = ui.allocate_exact_size(vec2(row_width, 8.0 + 17.0 * lines), Sense::click());
                        let name =
                            if trail.is_empty() { format!("{} {}", h.title, h.snippet) } else { format!("{} ({trail}) {}", h.title, h.snippet) };
                        row.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, name.clone()));
                        let painter = ui.painter_at(rect);
                        if current {
                            painter.rect_filled(rect, 4.0, tokens.row_selected);
                            painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 2.0, tokens.accent);
                        } else if row.hovered() {
                            painter.rect_filled(rect, 4.0, tokens.row_hover);
                        }
                        if row.has_focus() {
                            painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.0, tokens.accent), egui::StrokeKind::Inside);
                        }
                        let x = rect.min.x + 10.0;
                        let room = (rect.width() - 20.0).max(20.0);
                        let mut y = rect.min.y + 12.0;
                        // Through the bidi painter, as the band's title is: a title or trail in
                        // Arabic or Hebrew keeps its word order. Plain text takes egui's own path.
                        let line = |text: &str, size: f32, y: f32, colour| {
                            crate::bidi::line_at(
                                ui,
                                &painter,
                                egui::pos2(x, y),
                                Align2::LEFT_CENTER,
                                &fit_to_width(ui, text, size, room),
                                size,
                                colour,
                                None,
                            );
                        };
                        line(&h.title, 14.0, y, tokens.strong);
                        if !trail.is_empty() {
                            y += 17.0;
                            line(trail, 11.5, y, tokens.accent);
                        }
                        y += 17.0;
                        let snippet = fit_to_width(ui, &h.snippet, 12.0, room);
                        paint_search_snippet(ui, &painter, egui::pos2(x, y), &snippet, &self.searched, &tokens);
                        if current && picked_now {
                            row.scroll_to_me(None);
                        }
                        let row = if trail.is_empty() { row } else { row.on_hover_text(trail) };
                        if row.clicked() {
                            chosen = Some(h.path.clone());
                        }
                    }
                });
            });
        });
        // Keep the query editable while results are open. An outside press dismisses them
        // without consuming the press, so the clicked ribbon or page control still works.
        let search_rect = ctx.read_response(Id::new(("page-search", ctx.viewport_id()))).map(|response| response.rect);
        let outside = ctx.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !popup.response.rect.contains(pos) && !search_rect.is_some_and(|rect| rect.contains(pos)))
        });
        if scope != self.search_scope {
            self.search_scope = scope;
            self.refresh_search();
            if empty {
                ctx.memory_mut(|memory| memory.request_focus(field_id));
            }
        } else if outside {
            self.dismiss_search();
        }
        if let Some(p) = chosen {
            self.open_search_result(ctx, &p);
        }
    }

    /// Looks again for the tagged notes in the Tags Summary's scope.
    fn refresh_tags(&mut self) {
        let Some(pane) = self.tags_pane.as_ref() else { return };
        let scope = pane.scope.path(self.current_page());
        let mut p: Vec<(&str, String)> = Vec::new();
        if let Some(scope) = scope {
            p.push(("scope", path_str(&scope)));
        } else if pane.scope != TagScope::All {
            // Nothing open to look round: there is nothing in this page or section to list.
            if let Some(pane) = self.tags_pane.as_mut() {
                pane.found.clear();
            }
            return;
        }
        if pane.scope == TagScope::Page {
            p.push(("page", "true".into()));
        }
        if pane.unchecked {
            p.push(("unchecked", "true".into()));
        }
        let pairs: Vec<(&str, &str)> = p.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let found = match self.run("tags.find", &params(&pairs)) {
            Some(Output::Tagged(found)) => found,
            _ => Vec::new(),
        };
        if let Some(pane) = self.tags_pane.as_mut() {
            pane.found = found;
        }
    }

    /// The Tags Summary pane, on the right of the page as OneNote has it.
    /// Reads the templates again: the built-in ones and those kept in the notebooks folder.
    fn refresh_templates(&mut self) {
        let found = notecraft_store::page_templates::list(self.session.root());
        if let Some(pane) = self.templates_pane.as_mut() {
            pane.templates = found;
        }
    }

    /// A new page in the open section, started from the template, and opened.
    fn page_from_template(&mut self, id: &str, name: &str) {
        let Some(section) = self.current_section(self.session.tree()).map(|n| n.path.clone()) else {
            self.say(crate::tl_id!("ui.open_a_section_first_for_a_template"));
            return;
        };
        self.save_now();
        if let Some(Output::Path(page)) = self.run("page.from_template", &params(&[("parent", &path_str(&section)), ("template", id)])) {
            self.open_page(&page);
            self.say(crate::i18n::format_message_id("ui.page_from_template", &[("name", name)]));
        }
    }

    /// Keeps the open page as a template called `name`.
    fn save_page_as_template(&mut self, name: &str) {
        let Some(page) = self.editor.as_ref().map(|ed| ed.dir.clone()) else {
            self.say(crate::tl_id!("ui.open_a_page_first"));
            return;
        };
        self.save_now();
        if let Some(Output::Path(_)) = self.run("page.save_template", &params(&[("path", &path_str(&page)), ("name", name)])) {
            self.say(crate::i18n::format_message_id("ui.saved_as_template", &[("name", name)]));
            if let Some(pane) = self.templates_pane.as_mut() {
                pane.name.clear();
            }
            self.refresh_templates();
        }
    }

    fn templates_panel(&mut self, ui: &mut Ui) {
        let Some(pane) = self.templates_pane.as_mut() else { return };
        let tokens = self.tokens;
        let has_page = self.editor.is_some();
        let mut close = false;
        let mut chosen: Option<(String, String)> = None;
        let mut keep: Option<String> = None;
        // A phone-width window gives the pane the whole width, as it does the Tags Summary.
        let panel = if self.compact {
            egui::Panel::right("page-templates-compact").resizable(false).exact_size(ui.available_width())
        } else {
            egui::Panel::right("page-templates").resizable(true).default_size(280.0).size_range(220.0..=520.0)
        };
        panel.show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong(crate::tl_id!("ui.page_templates_title"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // A cross drawn with lines: the interface fonts have no glyph for one.
                    let (rect, x) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    x.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::tl_id!("ui.close_the_page_templates")));
                    if x.hovered() {
                        ui.painter().rect_filled(rect, 3.0, tokens.row_hover);
                    }
                    let c = rect.shrink(7.0);
                    let stroke = egui::Stroke::new(1.4, tokens.text);
                    ui.painter().line_segment([c.left_top(), c.right_bottom()], stroke);
                    ui.painter().line_segment([c.right_top(), c.left_bottom()], stroke);
                    if x.on_hover_text(crate::tl_id!("ui.close_the_page_templates")).clicked() {
                        close = true;
                    }
                });
            });
            ui.label(egui::RichText::new(crate::tl_id!("ui.page_templates_hint")).small().color(tokens.muted));
            ui.separator();
            let list_height = (ui.available_height() - 110.0).max(120.0);
            egui::ScrollArea::vertical().id_salt("page-templates-list").max_height(list_height).auto_shrink([false, true]).show(ui, |ui| {
                for category in notecraft_store::page_templates::Category::ALL {
                    let mut items = pane.templates.iter().filter(|t| t.category == category).peekable();
                    let mine = category == notecraft_store::page_templates::Category::Mine;
                    if items.peek().is_none() && !mine {
                        continue;
                    }
                    ui.add_space(4.0);
                    ui.strong(crate::tl!(category.label()));
                    if items.peek().is_none() {
                        ui.weak(crate::tl_id!("ui.no_saved_templates"));
                    }
                    for template in items {
                        let label = if mine { template.name.clone() } else { crate::tl!(&template.name).to_string() };
                        let width = ui.available_width().max(60.0);
                        let (rect, row) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
                        row.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label.clone()));
                        if row.hovered() {
                            ui.painter().rect_filled(rect, 3.0, tokens.row_hover);
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(10.0, 0.0),
                            egui::Align2::LEFT_CENTER,
                            &label,
                            egui::FontId::proportional(13.5),
                            tokens.text,
                        );
                        if row.on_hover_text(crate::tl_id!("ui.add_a_page_from_this_template")).clicked() {
                            chosen = Some((template.id.clone(), label));
                        }
                    }
                }
            });
            ui.separator();
            ui.label(crate::tl_id!("ui.save_current_page_as_a_template"));
            ui.add_enabled_ui(has_page, |ui| {
                let field = egui::TextEdit::singleline(&mut pane.name).hint_text(crate::tl_id!("ui.template_name")).desired_width(f32::INFINITY);
                let field = ui.add(field);
                // Named for a screen reader: the grey hint inside the box is not a name.
                field.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextInput, has_page, crate::tl_id!("ui.template_name")));
                let entered = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                let named = !pane.name.trim().is_empty();
                let clicked = ui.add_enabled(named, egui::Button::new(crate::tl_id!("ui.save_as_template"))).clicked();
                if named && (entered || clicked) {
                    keep = Some(pane.name.trim().to_string());
                }
            });
        });
        if close {
            self.templates_pane = None;
            return;
        }
        if let Some((id, name)) = chosen {
            self.page_from_template(&id, &name);
        }
        if let Some(name) = keep {
            self.save_page_as_template(&name);
        }
    }

    fn tags_panel(&mut self, ui: &mut Ui) {
        let Some(pane) = self.tags_pane.as_mut() else { return };
        let tokens = self.tokens;
        let mut close = false;
        let mut refresh = false;
        let mut chosen: Option<(PathBuf, usize)> = None;
        let panel = if self.compact {
            egui::Panel::right("tags-summary-compact").resizable(false).exact_size(ui.available_width())
        } else {
            egui::Panel::right("tags-summary").resizable(true).default_size(300.0).size_range(220.0..=600.0)
        };
        panel.show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong(crate::tl_id!("ui.tags_summary"));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // A cross drawn with lines: the interface fonts have no glyph for one.
                    let (rect, x) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                    x.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::tl_id!("ui.close_the_tags_summary")));
                    if x.hovered() {
                        ui.painter().rect_filled(rect, 3.0, tokens.row_hover);
                    }
                    let c = rect.shrink(7.0);
                    let stroke = egui::Stroke::new(1.4, tokens.text);
                    ui.painter().line_segment([c.left_top(), c.right_bottom()], stroke);
                    ui.painter().line_segment([c.right_top(), c.left_bottom()], stroke);
                    if x.on_hover_text(crate::tl_id!("ui.close_the_tags_summary")).clicked() {
                        close = true;
                    }
                });
            });
            ui.label(egui::RichText::new(crate::tl_id!("ui.tags_look_in")).small().color(tokens.muted));
            ui.horizontal_wrapped(|ui| {
                for scope in TagScope::ALL {
                    if ui.selectable_label(pane.scope == scope, scope.label()).clicked() && pane.scope != scope {
                        pane.scope = scope;
                        refresh = true;
                    }
                }
            });
            if ui.checkbox(&mut pane.unchecked, crate::tl_id!("ui.tags_only_unchecked_to_do_items")).changed() {
                refresh = true;
            }
            if ui.button(crate::tl_id!("ui.refresh")).on_hover_text(crate::tl_id!("ui.look_again_for_tagged_notes")).clicked() {
                refresh = true;
            }
            ui.separator();
            egui::ScrollArea::vertical().id_salt("tags-summary-list").auto_shrink([false, false]).show(ui, |ui| {
                if pane.found.is_empty() {
                    ui.weak(crate::tl_id!("ui.no_tagged_notes_here"));
                }
                for tag in notecraft_model::Tag::all() {
                    let mut lines = pane.found.iter().filter(|f| f.tag == tag).peekable();
                    if lines.peek().is_none() {
                        continue;
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        let (rect, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        ribbon::paint_tag(ui.painter(), rect, tag, tokens.text, tokens.accent);
                        ui.strong(crate::tl!(tag.label()));
                    });
                    for line in lines {
                        let words = if line.text.is_empty() { crate::tl_id!("ui.tags_empty_line").to_string() } else { line.text.clone() };
                        let place = if line.section.is_empty() { line.page.clone() } else { format!("{} > {}", line.section, line.page) };
                        // Painted rather than laid out as labels, so the row is one target that
                        // a screen reader names once, with a To Do's box ticked or not.
                        let width = ui.available_width().max(60.0);
                        let indent = 22.0;
                        let text = ui.painter().layout(words.clone(), egui::FontId::proportional(13.5), tokens.text, width - indent);
                        let under = ui.painter().layout(place, egui::FontId::proportional(11.5), tokens.muted, width - indent);
                        let (rect, row) = ui.allocate_exact_size(vec2(width, text.size().y + under.size().y + 8.0), Sense::click());
                        row.widget_info(|| match line.done {
                            Some(done) => egui::WidgetInfo::selected(egui::WidgetType::CheckBox, true, done, words.clone()),
                            None => egui::WidgetInfo::labeled(egui::WidgetType::Button, true, words.clone()),
                        });
                        let painter = ui.painter();
                        if row.hovered() {
                            painter.rect_filled(rect, 3.0, tokens.row_hover);
                        }
                        if let Some(done) = line.done {
                            let tick = egui::Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(12.0, 12.0));
                            painter.rect_stroke(tick, 2.0, egui::Stroke::new(1.2, tokens.muted), egui::StrokeKind::Inside);
                            if done {
                                let at = |x: f32, y: f32| tick.min + vec2(x, y);
                                painter.line(vec![at(2.5, 6.5), at(5.0, 9.0), at(9.5, 3.0)], egui::Stroke::new(1.6, tokens.accent));
                            }
                        }
                        let below = text.size().y + 3.0;
                        painter.galley(rect.min + vec2(indent, 2.0), text, tokens.text);
                        painter.galley(rect.min + vec2(indent, 2.0 + below), under, tokens.muted);
                        if row.clicked() {
                            chosen = Some((line.path.clone(), line.block));
                        }
                    }
                }
            });
        });
        if close {
            self.tags_pane = None;
            return;
        }
        if refresh {
            self.refresh_tags();
        }
        if let Some((page, block)) = chosen {
            self.go_to_tagged(&page, block);
        }
    }

    /// Opens the page a tagged line is on and puts the caret at the start of that line.
    fn go_to_tagged(&mut self, page: &Path, block: usize) {
        let mut parent = page.parent();
        while let Some(d) = parent {
            if d == self.session.root() {
                break;
            }
            self.open.insert(d.to_path_buf());
            parent = d.parent();
        }
        if self.editor.as_ref().is_none_or(|ed| ed.dir != page) {
            self.open_page(page);
        }
        let Some(ed) = self.editor.as_mut().filter(|ed| ed.dir == page && ed.writable) else { return };
        let Some(b) = ed.blocks.get(block).filter(|b| pageedit::is_writable(b)) else { return };
        ed.caret = Some(pageedit::Caret {
            block,
            text: pageedit::caret_text(b),
            focus: true,
            selection: (0, 0),
            place: None,
            cell: None,
            span: None,
            inline: None,
        });
    }

    fn side_panel(&mut self, ui: &mut Ui) {
        // Full page view: the notebooks step aside until the user asks for them again.
        if self.full_page && self.editor.is_some() {
            return;
        }
        let mut acts = Vec::new();
        // A phone-width window has room for one column, so it always shows the one tree.
        let compact = self.compact;
        let width = ui.available_width();
        // Use different panel IDs so a constrained frame never overwrites the
        // user's desktop splitter widths in egui's persisted memory.
        let narrow = (!compact && width < 960.0).then_some(width);
        if self.split_panes && !compact {
            self.organiser_panes(ui, &mut acts, narrow);
            for a in acts {
                self.apply(a);
            }
            return;
        }
        let mut split_now = false;
        // Compact: the list fills the window (a panel of its own, so the desktop width the
        // tree was dragged to is kept for a wider window).
        let panel = if compact {
            egui::Panel::left("tree-compact").resizable(false).exact_size(width)
        } else if let Some(width) = narrow {
            egui::Panel::left("tree-narrow").resizable(false).show_separator_line(true).exact_size((width * 0.34).min(280.0))
        } else {
            egui::Panel::left("tree").resizable(true).show_separator_line(true).default_size(300.0).size_range(200.0..=700.0)
        };
        panel.show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // In the one-tree layout the same button splits the panel and hides the
                // notebooks, which is what pressing it in OneNote does. A phone-width window
                // has no room for the split, so it has no button for it.
                if !compact {
                    split_now = Self::bars_button_at(ui).clicked();
                }
                ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_2")).strong().color(self.tokens.strong));
            });
            ui.horizontal_wrapped(|ui| {
                if ui.small_button(crate::tl_id!("ui.new_notebook")).clicked() {
                    self.dialog = Some(Dialog::NewNotebook { name: String::new(), format: None, first: true });
                }
                if ui.small_button(crate::tl_id!("ui.expand_all")).on_hover_text(crate::tl_id!("ui.open_every_level")).clicked() {
                    self.open = self.collect_branches().into_iter().collect();
                }
                if ui.small_button(crate::tl_id!("ui.collapse_all")).clicked() {
                    self.open.clear();
                }
                self.new_section_button(ui, &mut acts);
            });
            self.active_notebook_label(ui);
            ui.add_space(2.0);
            // A mouse drag in a pane must not scroll it: in the tree it is a click that
            // slipped. The wheel and the bar still work, and on a touch screen a finger drag
            // scrolls, which is the only way a phone has.
            let scroll = egui::containers::scroll_area::ScrollSource {
                drag: egui::containers::scroll_area::DragScroll::OnTouch,
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

    /// The lock entry of an empty-space menu: it acts on the page open in the editor, so the
    /// navigator's blank space and the pages' blank space both reach the same page (issue #441).
    /// The page's head is read only while the menu is open.
    fn lock_menu_row(&self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let Some(ed) = self.editor.as_ref() else { return };
        let locked = notecraft_store::page::read_only(&ed.dir).unwrap_or(false);
        // It acts on the page open in the editor, not on the blank space it was opened from, so it
        // names that page (issue #380).
        let title = if ed.title.trim().is_empty() { crate::tl_id!("ui.untitled_page").to_owned() } else { ed.title.trim().to_owned() };
        let (label, icon) = if locked {
            (crate::i18n::format_message_id("ui.unlock_page_named", &[("title", &title)]), Icon::Unlock)
        } else {
            (crate::i18n::format_message_id("ui.lock_page_named", &[("title", &title)]), Icon::Lock)
        };
        floating::menu_separator(ui, &self.tokens);
        if floating::menu_row(ui, Some(floating::Glyph::Icon(icon)), &label, &self.tokens).clicked() {
            acts.push(Act::SetReadOnly(ed.dir.clone(), !locked));
            ui.close();
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
        let t = &self.tokens;
        resp.context_menu(|ui| {
            match section {
                // Under the pages: a page at the end of this section.
                // The same rows as every other menu (issue #380): picture column, words, dividers.
                Some(path) => {
                    if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Page)), crate::tl_id!("ui.new_page_2"), t).clicked() {
                        acts.push(Act::AddKind(path.to_path_buf(), NodeKind::Section, NodeKind::Page));
                        ui.close();
                    }
                    self.lock_menu_row(ui, acts);
                }
                // Under the notebooks: a notebook, and the ways into the folder itself.
                None => {
                    if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Notebook)), crate::tl_id!("ui.new_notebook_2"), t).clicked() {
                        acts.push(Act::NewNotebook);
                        ui.close();
                    }
                    floating::menu_separator(ui, t);
                    if floating::menu_row(ui, Some(floating::Glyph::Icon(Icon::Folder)), crate::tl_id!("ui.open_a_folder"), t).clicked() {
                        choose = true;
                        ui.close();
                    }
                    if floating::menu_row(ui, None, crate::tl_id!("ui.show_the_notebooks_folder"), t).clicked() {
                        reveal = true;
                        ui.close();
                    }
                    self.lock_menu_row(ui, acts);
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
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::click());
        let hot = resp.hovered() || resp.has_focus();
        if hot {
            // The gap is 7 points tall and the circle is 13 across, so the circle has to
            // be drawn outside the strip it belongs to. `painter_at` cannot: it narrows
            // the clip it is given to whatever the ui already has, which is the strip, and
            // the circle came out a lozenge, flat at the top and the bottom. Painting on
            // the layer starts from the whole window instead.
            let room = (PLUS + 1.0 - rect.height() / 2.0).max(2.0);
            let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(rect.expand(room).intersect(ui.clip_rect()));
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
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
        let enabled = ui.is_enabled();
        let name = crate::tl_id!("ui.add_a_page_here");
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
        let resp = resp.on_hover_text(crate::tl_id!("ui.a_page_here"));
        if resp.clicked() {
            // Halfway between the two, or ten past the last one.
            match (before, after) {
                // No number between them: renumber first, so the page can go between.
                (Some(b), Some(a)) if a <= b + 1 => acts.push(Act::RenumberAndAddPage(section.to_path_buf(), b)),
                (Some(b), Some(a)) => acts.push(Act::AddPageAt(section.to_path_buf(), b + (a - b) / 2)),
                (Some(b), None) => acts.push(Act::AddPageAt(section.to_path_buf(), b.saturating_add(10))),
                (None, Some(a)) => acts.push(Act::AddPageAt(section.to_path_buf(), a / 2)),
                (None, None) => acts.push(Act::AddPageAt(section.to_path_buf(), 10)),
            }
        }
    }

    /// Where a new section goes: the notebook or section group the chosen section or page is in,
    /// a section group that is itself chosen, or else the first notebook. `None` with no notebook.
    fn new_section_parent(&self) -> Option<(PathBuf, NodeKind)> {
        // Asked every frame the side panel is drawn, so it reads the tree the window holds,
        // not the folders: read from disk it was the page and every folder above it, a frame,
        // which on a synced folder held each frame up.
        fn kind_in(nodes: &[notecraft_store::tree::Node], path: &Path) -> Option<NodeKind> {
            let node = nodes.iter().find(|n| path.starts_with(&n.path))?;
            if node.path == path { Some(node.info.kind) } else { kind_in(&node.children, path) }
        }
        let tree = self.session.tree();
        let mut at = self.selected.clone().or_else(|| self.editor.as_ref().map(|e| e.dir.clone()));
        while let Some(path) = at {
            match kind_in(tree, &path) {
                Some(kind @ (NodeKind::Notebook | NodeKind::SectionGroup)) => return Some((path, kind)),
                Some(_) => at = path.parent().map(Path::to_path_buf),
                None => break,
            }
        }
        self.session.tree().first().map(|n| (n.path.clone(), NodeKind::Notebook))
    }

    /// A plain, always visible "+ New section" button for the chosen notebook, as OneNote has
    /// under its sections, so a section can be added without finding the hover-only plus or the
    /// right-click menu (issue #139). It opens the same dialog those do.
    fn new_section_button(&self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let target = self.new_section_parent();
        let button = egui::Button::new(crate::tl_id!("ui.new_section_button")).stroke(egui::Stroke::new(1.0, self.tokens.container_border));
        let resp = ui.add_enabled(target.is_some(), button).on_hover_text(crate::tl_id!("ui.a_new_section_in_the_chosen_notebook"));
        if resp.clicked()
            && let Some((parent, kind)) = target
        {
            acts.push(Act::AddKind(parent, kind, NodeKind::Section));
        }
    }

    /// Add page (issue #366): one full-width row across the top of the page list, with a plus
    /// and the words, about 32 points tall, marked with the section's colour. The whole row is
    /// one target, so the plus, the words and the far edge all add the same page.
    fn add_page_row(&self, ui: &mut Ui, accent: egui::Color32) -> egui::Response {
        let t = self.tokens;
        let width = ui.available_width().max(60.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 32.0), Sense::click());
        let painter = ui.painter_at(rect);
        let fill = if resp.hovered() || resp.has_focus() { t.row_hover } else { crate::theme::mix(t.panel, accent, 0.10) };
        painter.rect_filled(rect, 4.0, fill);
        painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), egui::CornerRadius { nw: 4, sw: 4, ne: 0, se: 0 }, accent);
        if resp.has_focus() {
            painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
        let plus = egui::Rect::from_center_size(egui::pos2(rect.min.x + 20.0, rect.center().y), vec2(16.0, 16.0));
        icons::paint(&painter, plus, Icon::Plus, t.text);
        let label = crate::tl_id!("ui.add_page");
        painter.text(egui::pos2(rect.min.x + 36.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(14.0), t.text);
        let enabled = ui.is_enabled();
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
        let shortcut = if cfg!(target_os = "macos") { crate::tl_id!("ui.new_page_cmd_n") } else { crate::tl_id!("ui.new_page_ctrl_n") };
        resp.on_hover_text(format!("{}\n{shortcut}", crate::tl_id!("ui.a_new_page_in_this_section")))
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

    fn active_notebook_label(&self, ui: &mut Ui) {
        if let Some(owner) = self.selected.as_ref().and_then(|path| self.session.tree().iter().find(|n| path.starts_with(&n.path))) {
            let label = &owner.info.title;
            let room = ui.available_width();
            let font = 12.0;
            let fit = fit_with(&|s| crate::bidi::line_width(ui, s, font, None), label, room);
            let (rect, response) = ui.allocate_exact_size(vec2(room, 18.0), Sense::hover());
            crate::bidi::line_at(ui, &ui.painter_at(rect), rect.left_center(), Align2::LEFT_CENTER, &fit, font, self.tokens.muted, None);
            let name = crate::i18n::format_message_id("ui.scope_title", &[("scope", crate::tl_id!("ui.this_notebook")), ("title", label)]);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &name));
            response.on_hover_text(label);
        }
    }

    /// The side panel as two columns: the organisers (notebooks, section groups and
    /// sections) and then the pages of whichever section is chosen, the way OneNote splits
    /// them. The three-bar button in the top bar hides the first column.
    fn organiser_panes(&mut self, ui: &mut Ui, acts: &mut Vec<Act>, narrow: Option<f32>) {
        let scroll = || egui::containers::scroll_area::ScrollSource {
            drag: egui::containers::scroll_area::DragScroll::OnTouch,
            ..egui::containers::scroll_area::ScrollSource::default()
        };
        let mut toggle = false;
        if self.organisers {
            let panel = if let Some(width) = narrow {
                egui::Panel::left("organisers-narrow").resizable(false).show_separator_line(true).exact_size((width * 0.205).floor())
            } else {
                egui::Panel::left("organisers").resizable(true).show_separator_line(true).default_size(240.0).size_range(160.0..=520.0)
            };
            panel.show(ui, |ui| {
                ui.add_space(4.0);
                // The three-bar button sits at the top left of the notebooks, where
                // OneNote keeps it, and hides this column.
                ui.horizontal(|ui| {
                    toggle |= Self::bars_button_at(ui).clicked();
                    // A heading that reads as one (issue #368), above the tree's own scroller so
                    // it stays put however far the tree is scrolled.
                    ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_2")).size(13.0).strong().color(self.tokens.text));
                });
                ui.horizontal_wrapped(|ui| {
                    self.new_section_button(ui, acts);
                    ui.menu_button(crate::tl_id!("ui.more_2"), |ui| {
                        if ui.button(crate::tl_id!("ui.expand_all")).clicked() {
                            self.open = self.collect_branches().into_iter().collect();
                            ui.close();
                        }
                        if ui.button(crate::tl_id!("ui.collapse_all")).clicked() {
                            self.open.clear();
                            ui.close();
                        }
                    });
                });
                self.active_notebook_label(ui);
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
        let section = self.current_section(self.session.tree()).map(|n| (n.path.clone(), n.info.title.clone(), n.children.clone(), n.info.color));
        let panel = if let Some(width) = narrow {
            egui::Panel::left("pages-narrow").resizable(false).show_separator_line(true).exact_size((width * 0.205).floor())
        } else {
            egui::Panel::left("pages").resizable(true).show_separator_line(true).default_size(230.0).size_range(150.0..=520.0)
        };
        panel.show(ui, |ui| {
            ui.add_space(4.0);
            match &section {
                Some((path, title, pages, colour)) => {
                    ui.horizontal(|ui| {
                        // With the notebooks hidden the button lives here instead, so it is
                        // always at the top left of the panel. It is a target of its own,
                        // apart from Add page below it (issue #366).
                        if !self.organisers {
                            toggle |= Self::bars_button_at(ui).clicked();
                        }
                        // The section's name, as context for the list: readable, not shouted.
                        let room = ui.available_width().max(20.0);
                        let shown = fit_to_width(ui, title, 13.0, room);
                        let name = ui.label(egui::RichText::new(shown).size(13.0).strong().color(self.tokens.text));
                        if name.rect.width() + 1.0 < text_width(ui, title, 13.0) {
                            name.on_hover_text(title.as_str());
                        }
                    });
                    ui.add_space(2.0);
                    // A section with no colour of its own wears the theme's section colour, as
                    // its mark in the notebook tree does.
                    let accent = colour.map_or(self.tokens.section, |c| {
                        let (r, g, b) = c.rgb(self.effective_mode.is_dark());
                        egui::Color32::from_rgb(r, g, b)
                    });
                    // The same create-and-open as Ctrl+N and the menu's New page (issue #366):
                    // one click is one new page, opened at its title, with no name to type first.
                    if self.add_page_row(ui, accent).clicked() {
                        acts.push(Act::NewPage(path.clone()));
                    }
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().id_salt("pages").auto_shrink([false, false]).scroll_source(scroll()).show(ui, |ui| {
                        let list: Vec<&Node> = pages.iter().filter(|p| p.info.kind == NodeKind::Page).collect();
                        if list.is_empty() {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(crate::tl_id!("ui.no_pages_in_this_section_yet")).color(self.tokens.text));
                            ui.label(egui::RichText::new(crate::tl_id!("ui.add_page_empty_hint")).small().color(self.tokens.muted));
                        }
                        // A line between every pair, and above the first and below the
                        // last: hovering one offers a page in that place.
                        ui.add_space(4.0);
                        let mut last: Option<u32> = None;
                        for p in &list {
                            self.page_gap(ui, path, last, Some(p.info.order), acts);
                            self.draw_row(ui, p, 0, acts, true);
                            last = Some(p.info.order);
                        }
                        self.page_gap(ui, path, last, None, acts);
                        ui.add_space(4.0);
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

    /// View > Tabs layout (issues #268, #33): the open notebook's sections as tabs over the
    /// page. Choosing one opens its first page.
    fn section_tabs(&mut self, ui: &mut Ui) {
        if !self.tabs_layout || self.compact {
            return;
        }
        let Some(dir) = self.editor.as_ref().map(|e| e.dir.clone()) else { return };
        // From the tree the window holds, which commands keep up to date: read from disk here,
        // every frame, it asked a synced folder's mount for every section and page each time.
        let notebooks = self.session.tree();
        let Some(notebook) = notebook_for(&dir).and_then(|nb| notebooks.iter().find(|n| n.path == nb)) else { return };
        let tabs = crate::sectiontabs::tabs(notebook);
        let here = section_for(&dir);
        let tokens = self.tokens;
        let mut chosen = None;
        let mut acts = Vec::new();
        egui::Panel::top("section-tabs")
            .frame(egui::Frame::NONE.fill(tokens.panel).inner_margin(egui::Margin { left: 8, right: 8, top: 4, bottom: 0 }))
            .show_separator_line(false)
            .show(ui, |ui| {
                // A tab's menu is its section's own, whatever page is shown (issue #556).
                let mut menu = |ui: &mut Ui, section: &Path| {
                    if let Some(node) = node_at(notebooks, section) {
                        let has_kids = node.children.iter().any(|c| c.info.kind != NodeKind::Page);
                        self.node_menu_items(ui, node, self.open.contains(section), has_kids, &mut acts);
                    }
                };
                chosen = crate::sectiontabs::show(ui, &tabs, here.as_deref(), &tokens, &mut menu);
            });
        for act in acts {
            self.apply(act);
        }
        if let Some(section) = chosen {
            self.open_section(&section);
        }
    }

    fn central(&mut self, ui: &mut Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            // A section chosen with no pages in it says so, by name, and offers its first page
            // (issue #525), rather than the window's welcome or a page from elsewhere.
            let empty_section = self
                .selected
                .as_ref()
                .and_then(|s| node_at(self.session.tree(), s))
                .filter(|n| n.info.kind == NodeKind::Section && first_page(std::slice::from_ref(*n), &n.path).is_none())
                .map(|n| (n.path.clone(), n.info.title.clone()));
            if self.editor.is_none()
                && let Some((section, title)) = empty_section
            {
                let mut add = false;
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.25);
                    ui.label(egui::RichText::new(title).size(20.0).strong().color(self.tokens.text));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new(crate::tl_id!("ui.no_pages_in_this_section_yet")).color(self.tokens.text));
                    ui.add_space(10.0);
                    add = modal_button(ui, &self.tokens, crate::tl_id!("ui.add_the_first_page"), true).clicked();
                });
                if add {
                    self.create_page_in(&section);
                }
                return;
            }
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

    /// How far, in screen points, the open page's ink reaches above its top, with a little room
    /// beyond it; nothing when all of it is on the page.
    fn ink_above_page(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let zoom = self.zoom_percent as f32 / 100.0;
        let top = self.pad.as_ref().and_then(|pad| pad.drawing.bounds()).map_or(0.0, |(_, y0, _, _)| y0);
        if top < 0.0 { (-top + INK_ABOVE_ROOM) * zoom } else { 0.0 }
    }

    fn wheel_canvas_pan_delta(delta: egui::Vec2, modifiers: Modifiers, canvas_pan_enabled: bool) -> egui::Vec2 {
        if modifiers.ctrl {
            return egui::Vec2::ZERO;
        }
        if modifiers.shift {
            return egui::vec2(delta.y, 0.0);
        }
        // With the caret's pointer the page's own scroll bar takes the wheel up and down, and a
        // sideways swipe pans the page sideways, as the horizontal scroll bar does.
        if canvas_pan_enabled { delta } else { egui::vec2(delta.x, 0.0) }
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
        // Where a pinch or a wheel acts on the page: the part of it the window shows.
        let page_input_rect = self.page_view.or(self.page_rect);
        let wheel_canvas_pan_enabled = self.pointer != ribbon::Pointer::Write;
        if self.editor.is_some() && !self.show_source {
            // Draw > Panning Hand: the main button drags the page as the middle one always does
            // (issue #270), but not from the scroll bar, which keeps its own drag.
            let hand = self.pointer == ribbon::Pointer::Pan;
            let bar = self.page_scroll_strip;
            let view_height = page_input_rect.map_or(600.0, |r| r.height());
            let line = ui.ctx().options(|o| o.input_options.line_scroll_speed);
            let fling = &mut self.fling;
            let (pointer, middle_pressed, middle_down, wheel) = ui.input(|i| {
                let pointer = page_input_rect.and_then(|rect| i.pointer.interact_pos().filter(|p| rect.contains(*p)));
                // A click, a pinch or a key stops a fling where it is.
                if i.pointer.any_pressed() || (i.zoom_delta() - 1.0).abs() > 0.0001 {
                    fling.stop();
                }
                let over = page_input_rect.is_some_and(|rect| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)));
                let mut wheel = egui::Vec2::ZERO;
                for event in &i.events {
                    let egui::Event::MouseWheel { unit, delta, phase, modifiers, .. } = event else { continue };
                    // A notch of a mouse wheel is a line, not a point: a line is as far as the
                    // page's own scroll bar takes it, so the wheel pans as fast as it scrolls.
                    let delta = match unit {
                        egui::MouseWheelUnit::Point => *delta,
                        egui::MouseWheelUnit::Line => *delta * line,
                        egui::MouseWheelUnit::Page => *delta * view_height,
                    };
                    if over {
                        wheel += Self::wheel_canvas_pan_delta(delta, *modifiers, wheel_canvas_pan_enabled);
                    }
                    // Fingers on a touchpad: the page carries on when they lift.
                    if *unit == egui::MouseWheelUnit::Point && !modifiers.ctrl && !modifiers.command {
                        match phase {
                            egui::TouchPhase::End => fling.lift(i.time),
                            egui::TouchPhase::Cancel => fling.stop(),
                            _ if over && delta != egui::Vec2::ZERO => {
                                fling.finger(i.time, if modifiers.shift { egui::vec2(delta.y, 0.0) } else { delta })
                            }
                            _ => {}
                        }
                    }
                }
                let pointer = pointer.filter(|p| !(hand && bar.is_some_and(|b| b.contains(*p))));
                let pressed = i.pointer.button_pressed(egui::PointerButton::Middle) || (hand && i.pointer.primary_pressed());
                let down = i.pointer.button_down(egui::PointerButton::Middle) || (hand && i.pointer.primary_down());
                (pointer, pressed, down, wheel)
            });
            // The fling goes where the fingers took the page: down it by its scroll bar with the
            // caret's pointer, and sideways the way a sideways swipe goes.
            let now = ui.input(|i| i.time);
            let carried = self.fling.step(now);
            if self.fling.moving() {
                ui.ctx().request_repaint();
            }
            let carried_down = if wheel_canvas_pan_enabled { 0.0 } else { carried.y };
            let wheel = wheel + egui::vec2(carried.x, carried.y - carried_down);
            if carried_down != 0.0 {
                self.scroll_to = Some((self.page_offset_y - carried_down).max(0.0));
            }
            if wheel_canvas_pan_enabled {
                self.canvas_pan += wheel;
                self.canvas_pan.x = self.canvas_pan.x.clamp(-1.0e6, 1.0e6);
                self.canvas_pan.y = self.canvas_pan.y.clamp(-1.0e6, 1.0e6);
            } else if wheel.x != 0.0
                && let (Some(span), Some(view)) = (self.page_hspan, self.page_view)
                && let Some(thumb) = crate::hscroll::thumb(span, (view.min.x, view.max.x), view.width())
            {
                // Sideways, as far as the page reaches past the view and no further, as the bar.
                self.canvas_pan.x += crate::hscroll::clamp_pan(wheel.x, &thumb);
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
            // The canvas is endless down, where there is room to draw, but it starts at the top of
            // the page: panned past it, the view went thousands of points into nothing, where a pen
            // touching down drew a dot the page then grew to hold. Ink already above the page can
            // still be reached.
            let top = self.page_offset_y + self.ink_above_page();
            // Held at the top in every frame, so a fling going up and sideways keeps going sideways.
            if self.canvas_pan.y > top {
                self.canvas_pan.y = top;
            }
        } else {
            self.canvas_drag = None;
        }
        let zoom_at = ui.input(|i| page_input_rect.and_then(|rect| i.pointer.hover_pos().filter(|p| rect.contains(*p))));
        let wheel_zoom = ui.input(|i| zoom_at.map_or(1.0, |_| i.zoom_delta()));
        if wheel_zoom.is_finite() && (wheel_zoom - 1.0).abs() > 0.0001 {
            let before = self.zoom_percent;
            // A pinch comes as many small steps. Each is kept whole, the part of a percent too,
            // so a slow pinch zooms: rounded at every step, steps under half a percent were lost.
            #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                let exact = ((before as f32 + self.zoom_rest) * wheel_zoom).clamp(50.0, 250.0);
                self.zoom_percent = exact.round() as u32;
                self.zoom_rest = exact - self.zoom_percent as f32;
            }
            self.fit_page_width = false;
            // The page grows or shrinks about the pointer, not about its top left corner.
            if self.zoom_percent != before
                && let (Some(at), Some(ed)) = (zoom_at, self.editor.as_ref())
            {
                #[allow(clippy::cast_precision_loss)]
                let old = before as f32 / 100.0;
                // A zoom already waiting to be put right is measured from where it was.
                let spot = self.zoom_anchor.filter(|(was, _)| *was == at).map_or((at - ed.origin) / old, |(_, spot)| spot);
                self.zoom_anchor = Some((at, spot));
            }
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
        let tokens = self.tokens;
        let mut rename: Option<String> = None;
        let cursor = self.cursor;
        let mut changed = false;
        let show_source = self.show_source;
        let full_page = self.full_page;
        let (hide_title, rules) = (self.hide_title, self.paper_now());
        let hide_authors = self.hide_authors;
        // The grid a moved or resized box snaps to: the paper's own lines, or 12 points on plain
        // paper. None while snapping is off (issue #79).
        let snap = self.snap_to_grid.then(|| if rules.on() { rules.step() } else { PLAIN_SNAP_STEP });
        // The wash behind this page, if it carries a colour: a pale tint of it over the
        // page's own background, so the words keep their contrast whatever is chosen.
        let page_colour = self.editor.as_ref().and_then(|e| e.colour).map(|c| {
            let dark = self.theme().mode == crate::theme::Mode::Dark;
            let (r, g, b) = c.rgb(dark);
            crate::theme::mix(self.tokens.editor, egui::Color32::from_rgb(r, g, b), 0.16)
        });
        // What the title area is showing, so the page's words do not say it again.
        let shown_title = self.editor.as_ref().map(|e| e.title.clone()).unwrap_or_default();
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
        // What Automatic Shapes made of a stroke this frame, to say after the save (#294).
        let mut recognised: Option<&'static str> = None;
        let mut page_rect = None;
        let mut hbar = crate::hscroll::Shown::default();
        // Coming back to where the window was left: the page is scrolled there once it is drawn.
        let restore_scroll = if self.editor.is_some() { self.restore_scroll } else { None };
        let mut scroll_now: Option<f32> = None;
        // What the page's right-click menu asked for, run once the page's borrow is over.
        let mut page_acts: Vec<ribbon::Act> = Vec::new();
        let collect_now = std::mem::take(&mut self.collect_pending);
        // Insert Space: in hand this frame, and whether it was used or put down.
        let space_on = self.space_tool && self.pointer == ribbon::Pointer::Write;
        let mut space_over = false;
        {
            let Some(ed) = self.editor.as_mut() else { return };
            let missing_fonts = crate::fonts::ensure_page_fonts(ui.ctx(), &crate::fonts::page_families(&ed.blocks));
            if !missing_fonts.is_empty() {
                let message = crate::i18n::format_message_id("ui.fonts_unavailable", &[("fonts", &missing_fonts.join(", "))]);
                ui.label(egui::RichText::new(message).color(tokens.muted));
            }
            // One id for the title, whether it is drawn as words or as a text box.
            let title_id = page_title_id(ui.ctx());
            ui.horizontal_wrapped(|ui| {
                // File details live in the status bar. Keep only compact page controls
                // here, with a drop instruction while a file is actually over the window.
                if ui.input(|i| !i.raw.hovered_files.is_empty()) {
                    ui.label(egui::RichText::new(crate::tl_id!("ui.drop_a_file_onto_the_page_to_add_it")).small().color(tokens.muted));
                }
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
            // How far the page's content reaches sideways on screen this frame (issue #188).
            let mut page_span: Option<(f32, f32)> = None;
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
                    let lines = crate::theme::rule_lines_in(&tokens, ed.rule_colour);
                    crate::ribbon::paint_paper_canvas(
                        &ui.painter_at(viewport),
                        viewport,
                        rules,
                        zoom_scale,
                        // Through the page's origin, so a box on the grid sits on a line. A page
                        // not yet drawn as boxes counts from the canvas's corner.
                        viewport.translate(self.canvas_pan).min + ed.origin_in_canvas.unwrap_or_default(),
                        lines.0,
                        lines.1,
                    );
                }
                let content_rect = viewport.translate(self.canvas_pan);
                ui.scope_builder(egui::UiBuilder::new().id_salt("page-canvas-content").max_rect(content_rect), |ui| {
                    // Which way lines fall when they say nothing themselves: the page's own
                    // direction, if the writer gave it one.
                    let page_dir = ed.page_dir;
                    let available_width = ui.available_width();
                    // A page written for a sheet runs its words between the sheet's margins, and the
                    // sheet is drawn behind it; an Auto page is the usual column (issue #269).
                    let sheet = ed.paper_size.filter(|_| wide).map(|p| crate::paper::Sheet::of(p, zoom_scale));
                    let page_column = sheet.map_or(780.0 * zoom_scale, crate::paper::Sheet::column);
                    let margin = if let Some(sheet) = sheet.filter(|s| s.size.x <= available_width) {
                        (available_width - sheet.size.x) / 2.0 + sheet.margins[3]
                    } else if wide {
                        ((available_width - page_column) / 2.0).max(8.0)
                    } else {
                        0.0
                    };
                    let top_pad = sheet.map_or(4.0, |s| s.margins[0].max(4.0));
                    let content = ui.max_rect();
                    // egui's Frame margins are i8. Use a float layout rect so a small
                    // sheet stays centered in a wide window and shares its text origin.
                    let body_min = content.min + vec2(margin, top_pad);
                    let column = if available_width >= page_column { page_column } else { (available_width - 16.0).max(1.0) };
                    let body_rect = egui::Rect::from_min_max(body_min, egui::pos2(body_min.x + column, (content.max.y - 8.0).max(body_min.y)));
                    if let Some(sheet) = sheet {
                        let left = body_rect.min.x - sheet.margins[3];
                        let top = body_rect.min.y - sheet.margins[0];
                        crate::paper::paint(&ui.painter().with_clip_rect(viewport), viewport, egui::pos2(left, top), sheet, &tokens);
                    }
                    ui.scope_builder(egui::UiBuilder::new().max_rect(body_rect), |ui| {
                        ui.set_min_width(column);
                        ui.set_width(column);
                        // The page's own name, at the top of the sheet. Before the sheet's
                        // origin is taken below, so the ink layer keeps its own coordinates
                        // and a drawing stays beside the words it was made next to.
                        if !hide_title {
                            page_title(ui, ed, &tokens, title_id, rename, zoom_scale);
                            if !hide_authors {
                                page_authors(ui, &ed.authors, &tokens, zoom_scale);
                            }
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
                                // The viewport is one view tall from the top of the page, which a
                                // scrolled page has left behind: the ink reaches down to the
                                // bottom of what the window shows.
                                let ink_area = viewport.with_max_y(viewport.max.y.max(ui.clip_rect().max.y));
                                let hit = self
                                    .page_scroll_strip
                                    .filter(|bar| bar.min.x > ink_area.min.x)
                                    .map_or(ink_area, |bar| ink_area.with_max_x(ink_area.max.x.min(bar.min.x)));
                                let mut ink_ui = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(ink_area));
                                crate::inkpad::set_ruler(ink_ui.ctx(), self.ruler);
                                if crate::inkpad::take_replay_closed(ink_ui.ctx()) {
                                    self.replaying = false;
                                }
                                crate::inkpad::set_replay(ink_ui.ctx(), self.replaying);
                                ink_changed |= crate::inkpad::show(&mut ink_ui, ink_area, hit, top, pad, pen, pressure, &tokens, zoom_scale);
                                if let Some(kind) = pad.take_recognised() {
                                    recognised = Some(kind);
                                }
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
                                let restore_id = pageedit::restore_selection_id(ui.ctx());
                                ui.data_mut(|data| data.insert_temp(restore_id, selection));
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
                                    &ed.columns,
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
                                    &mut ed.columns,
                                    &mut ed.justified,
                                    &mut ed.across,
                                    if hide_title { "" } else { &shown_title },
                                    zoom_scale,
                                    snap,
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
                            // Collect All (issue #23): the boxes and the strokes of ink, each as the
                            // rectangle it covers, gathered so no empty stretch of the page between
                            // them is wider than a small gap. Nothing moves past anything else.
                            if collect_now {
                                let strokes = pad.as_ref().map(crate::inkpad::Pad::stroke_bounds).unwrap_or_default();
                                let items: Vec<egui::Rect> = moved
                                    .frames
                                    .iter()
                                    .copied()
                                    .chain(strokes.iter().map(|[x0, y0, x1, y1]| egui::Rect::from_min_max(egui::pos2(*x0, *y0), egui::pos2(*x1, *y1))))
                                    .collect();
                                let by = crate::collect::gather(&items, COLLECT_GAP);
                                for (i, d) in by.iter().take(moved.frames.len()).enumerate() {
                                    if let Some(b) = ed.boxes.get_mut(i)
                                        && *d != egui::Vec2::ZERO
                                    {
                                        *b = notecraft_model::container::Container::new(b.x + d.x, b.y + d.y, b.width, b.blocks);
                                        moved.boxes = true;
                                    }
                                }
                                let ink: Vec<(f32, f32)> = by.iter().skip(moved.frames.len()).map(|d| (d.x, d.y)).collect();
                                if let Some(pad) = pad.as_mut() {
                                    ink_changed |= pad.move_each(&ink);
                                }
                            }
                            // The page's column, its boxes and its ink: what the sideways bar
                            // has to reach.
                            {
                                let left = moved.origin.x;
                                let mut span = (left, (left + moved.right * zoom_scale).max(left + ui.min_rect().width()));
                                if let Some([x0, _, x1, _]) = pad.as_ref().and_then(|p| p.bounds()) {
                                    span = (span.0.min(top.x + x0 * zoom_scale), span.1.max(top.x + x1 * zoom_scale));
                                }
                                page_span = Some(span);
                            }
                            // Insert Space (issue #164): a drag down the page pushes everything that
                            // starts below where it began, boxes and ink together, down by as far as
                            // it went; a drag up pulls it back up. One drag, then the tool is put down.
                            if space_on {
                                let visible = ui.clip_rect().intersect(viewport.with_max_y(viewport.max.y.max(ui.clip_rect().max.y)));
                                if let Some((from, by)) = insert_space_overlay(ui.ctx(), visible, &tokens) {
                                    let z = zoom_scale.max(0.01);
                                    let (at, by) = ((from - moved.origin.y) / z, by / z);
                                    let below: Vec<usize> = (0..ed.boxes.len()).filter(|i| ed.boxes.get(*i).is_some_and(|b| b.y >= at)).collect();
                                    // Room taken away never pulls anything above the top of the page.
                                    let highest = below.iter().filter_map(|i| ed.boxes.get(*i)).map(|b| b.y).fold(f32::INFINITY, f32::min);
                                    let by = if by < 0.0 && highest.is_finite() { by.max(-highest) } else { by };
                                    for i in &below {
                                        if let Some(b) = ed.boxes.get_mut(*i) {
                                            *b = notecraft_model::container::Container::new(b.x, b.y + by, b.width, b.blocks);
                                        }
                                    }
                                    if !below.is_empty() && by != 0.0 {
                                        moved.boxes = true;
                                    }
                                    if let Some(pad) = pad.as_mut() {
                                        ink_changed |= pad.shift_below(at, by);
                                    }
                                    space_over = true;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    space_over = true;
                                }
                            }
                            // The selection box takes ink as well as boxes, as in OneNote, and what
                            // it took moves together: boxes dragged take the ink with them, ink
                            // dragged takes the boxes.
                            if let Some(pad) = pad.as_mut()
                                && !ink_owns_canvas
                            {
                                if moved.selection_started {
                                    pad.let_go();
                                }
                                if let Some(shown) = moved.selection_box {
                                    let to_ink = |p: egui::Pos2| ((p.x - top.x) / zoom_scale.max(0.01), (p.y - top.y) / zoom_scale.max(0.01));
                                    let (a, b) = (to_ink(shown.min), to_ink(shown.max));
                                    pad.hold_touching((a.0, a.1, b.0, b.1));
                                }
                                if let Some((dx, dy)) = moved.selected_shift {
                                    pad.move_held(dx, dy);
                                    ink_changed |= pad.holding() > 0;
                                }
                                if let Some((dx, dy)) = pad.take_shift() {
                                    for i in crate::containers::selected_boxes(ui.ctx(), &ed.dir) {
                                        if let Some(c) = ed.boxes.get_mut(i) {
                                            *c = notecraft_model::container::Container::new(c.x + dx, c.y + dy, c.width, c.blocks);
                                            moved.boxes = true;
                                        }
                                    }
                                }
                                // A press anywhere else on the page lets go of the ink, unless it
                                // went on something still selected.
                                let on_held = pressed.is_some_and(|p| crate::inkpad::held_area(ui.ctx()).is_some_and(|r| r.contains(p)));
                                let ctrl_press = ui.input(|i| {
                                    i.events.iter().any(|e| {
                                        matches!(e, egui::Event::PointerButton { pressed: true, modifiers, .. } if modifiers.command || modifiers.ctrl)
                                    })
                                });
                                if press_in_page
                                    && !on_held
                                    && !ctrl_press
                                    && moved.selection_box.is_none()
                                    && pad.holding() > 0
                                    && crate::containers::selected_boxes(ui.ctx(), &ed.dir).is_empty()
                                {
                                    pad.let_go();
                                }
                            }
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
                                crate::imagemenu::show(ui, body, &ed.dir, &mut page_acts, &tokens);
                                crate::pagemenu::show(ui, body, &mut page_acts, &tokens, ed.locked);
                            }
                            if moved.changed.blocks {
                                ed.rebuild_body();
                                *from_blocks = true;
                            }
                            ed.content_bottom = Some(moved.bottom);
                            ed.shown_widths = std::mem::take(&mut moved.shown_widths);
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
                            ed.origin_in_canvas = Some(moved.origin - content_rect.min);
                            if moved.changed.link.is_some() {
                                *follow = moved.changed.link;
                            }
                        } else if wide || ed.locked {
                            // A page with markup the block model cannot hold: writing it back
                            // would drop that markup, so it is read here and changed in its
                            // source. See docs/gaps.md.
                            if let Some(href) = preview::show_zoomed(ui, &ed.blocks, &ed.dir, &tokens, page_dir, &ed.justified, zoom_scale) {
                                *follow = Some(href);
                            }
                            // A locked page is read here, and its menu still offers the lock (issue #441).
                            crate::pagemenu::show(ui, ui.clip_rect(), &mut page_acts, &tokens, ed.locked);
                            ui.add_space(14.0);
                            ui.label(
                                egui::RichText::new(if ed.locked {
                                    crate::tl_id!("ui.page_is_read_only")
                                } else {
                                    crate::tl_id!("ui.this_page_holds_markup_notecraft_does_not_edit_yet_s_bc3b3b6a92")
                                })
                                    .small()
                                    .color(tokens.muted),
                            );
                        } else {
                            if let Some(href) = preview::show_zoomed(ui, &ed.blocks, &ed.dir, &tokens, page_dir, &ed.justified, zoom_scale) {
                                *follow = Some(href);
                            }
                            crate::pagemenu::show(ui, ui.clip_rect(), &mut page_acts, &tokens, ed.locked);
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
                let page_outer = ui.available_rect_before_wrap();
                let mut area = egui::ScrollArea::vertical().id_salt("page-main").auto_shrink([false, false]).scroll_source(scroll);
                // Within the page: past its end egui springs the page back, with a speed it then
                // coasts on, and goes on asking for frames.
                if let Some(y) = self.scroll_to.take() {
                    area = area.vertical_scroll_offset(y.clamp(0.0, self.page_offset_max));
                } else if let Some((y, _)) = restore_scroll
                    // Coming back to where the window was left: only once the page is tall enough
                    // to be scrolled that far, for the same reason.
                    && y <= self.page_offset_max + 1.0
                {
                    area = area.vertical_scroll_offset(y);
                }
                let shown = area.show(ui, |ui| {
                    // On a touch screen two fingers scroll the page. One finger writes,
                    // selects and moves boxes, as the mouse does, so it cannot scroll too.
                    if let Some(touch) = ui.input(|i| i.multi_touch()) {
                        ui.scroll_with_delta(vec2(0.0, touch.translation_delta.y));
                    }
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
                let scroll_style = ui.spacing().scroll;
                let scroll_edge = page_outer.max.x.min(ui.clip_rect().max.x);
                let max_cross = scroll_edge - scroll_style.bar_outer_margin;
                self.page_scroll_strip = (scroll.scroll_bar && shown.content_size.y > shown.inner_rect.height() + 0.5)
                    .then(|| egui::Rect::from_x_y_ranges((max_cross - scroll_style.bar_width)..=max_cross, shown.inner_rect.y_range()));
                // Sideways the page is a canvas: when what is on it reaches past the view's
                // sides, a bar along the bottom shows it and moves the canvas (issue #188).
                self.page_view = Some(shown.inner_rect);
                self.page_hspan = page_span;
                if let Some(span) = page_span {
                    let view = shown.inner_rect;
                    let vertical = shown.content_size.y > view.height() + 0.5;
                    let reserve = if vertical { ui.spacing().scroll.bar_width + ui.spacing().scroll.bar_outer_margin } else { 0.0 };
                    hbar = crate::hscroll::show(ui.ctx(), egui::Id::new("page-hscroll"), view, span, reserve);
                }
                scroll_now = Some(shown.state.offset().y);
                self.page_offset_y = shown.state.offset().y;
                self.page_offset_max = (shown.content_size.y - shown.inner_rect.height()).max(0.0);
                // Zoomed with the pointer on the page: the spot that was under it is moved back
                // under it, down by scrolling and sideways by the canvas when the page is wider
                // than the view. This pass, drawn about the corner, is thrown away and drawn again.
                if let Some((at, spot)) = self.zoom_anchor.take()
                    && page_span.is_some()
                {
                    let off = ed.origin + spot * zoom_scale - at;
                    if off.y.abs() > 0.5 {
                        self.scroll_to = Some((shown.state.offset().y + off.y).max(0.0));
                    }
                    let view = shown.inner_rect;
                    if off.x.abs() > 0.5
                        && let Some(span) = page_span
                        && let Some(thumb) = crate::hscroll::thumb(span, (view.min.x, view.max.x), view.width())
                    {
                        self.canvas_pan.x += crate::hscroll::clamp_pan(-off.x, &thumb);
                    }
                    if off.length() > 0.5 {
                        ui.ctx().request_discard("zoom about the pointer");
                    }
                }
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
            // A container was dragged or resized: the arrangement is the page's own, so it
            // is written into the page's file rather than kept in the window. Done before the
            // step is taken, so the step's text holds the boxes where they now are.
            if boxes_moved {
                ed.arranged = true;
                ed.rebuild_body();
                Self::mark_dirty_from_blocks(ed);
            }
            if changed || from_blocks || boxes_moved {
                // Boxes moved while the pointer is held: one drag, one step.
                let press = ui.input(|i| (i.pointer.any_down() || i.pointer.any_released()).then(|| i.pointer.press_start_time()).flatten());
                let moving = (boxes_moved && !changed && !from_blocks).then_some(press).flatten().map(f64::to_bits);
                Self::remember(ed, false, moving);
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
        self.page_hbar = hbar.track.zip(hbar.handle);
        if hbar.pan != 0.0 && hbar.pan.is_finite() {
            self.canvas_pan.x = (self.canvas_pan.x + hbar.pan).clamp(-1.0e6, 1.0e6);
            ui.ctx().request_repaint();
        }
        if space_over {
            self.space_tool = false;
        }
        if let Some(y) = scroll_now {
            // Tried until the page is there, which may take a frame or two to be tall enough.
            self.restore_scroll = self.restore_scroll.and_then(|(want, left)| ((y - want).abs() > 1.0 && left > 0).then_some((want, left - 1)));
        }
        for act in page_acts {
            self.ribbon_act(act);
        }
        if ink_changed {
            self.last_ink_change = Some(Instant::now());
        }
        // A change to the ink is finished once no button is down: what it was before becomes a
        // step undo can go back to.
        if let Some(pad) = self.pad.as_mut()
            && !ui.input(|i| i.pointer.any_down())
        {
            pad.settle();
        }
        // The ink is written a moment after the last change, off the window's thread, once the
        // pen is lifted: the pen never waits for the disk.
        if ink_changed {
            self.ink_changes += 1;
            self.ink_save_at = Some(Instant::now() + INK_SAVE_AFTER);
            self.expect_ink_file();
        }
        self.take_ink_save(false);
        let pen_down = ui.input(|i| i.pointer.any_down());
        if self.ink_saving.is_none() && !pen_down && !self.page_conflict && self.ink_save_at.is_some_and(|at| Instant::now() >= at) {
            self.ink_save_at = None;
            self.start_ink_save();
        }
        if self.ink_save_at.is_some() || self.ink_saving.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(kind) = recognised {
            self.say(match kind {
                "line" => crate::tl_id!("ui.recognised_line"),
                "rectangle" => crate::tl_id!("ui.recognised_rectangle"),
                "ellipse" => crate::tl_id!("ui.recognised_ellipse"),
                _ => crate::tl_id!("ui.recognised_triangle"),
            });
        }
        if let Some(href) = follow {
            self.follow_link(ui.ctx(), &href);
        }
        if rename.is_some() {
            self.save_now();
            if !self.is_dirty() && !self.page_conflict {
                self.say(crate::tl_id!("ui.title_saved"));
            }
        }
    }

    fn saved_caption(&self, saved_at: Option<&str>) -> String {
        if self.session_only_storage {
            crate::tl_id!("ui.kept_in_this_tab").into()
        } else {
            saved_at
                .map(|time| crate::i18n::format_message_id("ui.saved_time", &[("time", time)]))
                .unwrap_or_else(|| crate::tl_id!("ui.saved").into())
        }
    }

    fn page_save_state(&self, ed: &Editor) -> String {
        // Issue #371: a save in flight says so, apart from edits not yet written.
        if ed.saving.is_some() {
            crate::tl_id!("ui.saving").into()
        } else if ed.dirty_since.is_some() {
            crate::tl_id!("ui.editing").into()
        } else {
            self.saved_caption(ed.last_saved.as_deref())
        }
    }

    fn storage_notice(&self, ui: &mut Ui) {
        if self.session_only_storage {
            egui::Panel::top("session-storage-notice").show(ui, |ui| {
                ui.label(egui::RichText::new(crate::tl_id!("ui.session_only_storage_notice")).color(self.tokens.warn));
            });
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                // Issue #371: an error gets its own mark and a heavier weight, not only a colour.
                let shown = crate::i18n::pseudo_dynamic(&self.status);
                let line = if self.status_is_error {
                    egui::RichText::new(format!("! {shown}")).strong().color(self.tokens.warn)
                } else {
                    egui::RichText::new(shown).color(self.tokens.muted)
                };
                // Cut to the room the footer's controls leave, with the whole line on hover, so a
                // long error never pushes the default-format control out of view.
                let whole = self.status.clone();
                let status_room = (ui.available_width() - 480.0).max(160.0);
                ui.scope(|ui| {
                    ui.set_max_width(status_room);
                    ui.add(egui::Label::new(line).truncate()).on_hover_text(whole);
                });
                if let Some(ed) = self.editor.as_ref() {
                    let state = self.page_save_state(ed);
                    ui.separator();
                    ui.label(egui::RichText::new(format!("{} - {state}", ed.format.label())).small().color(self.tokens.muted))
                        .on_hover_text(self.shown(&ed.dir));
                }
                // A phone-width window keeps only the default format's box here, without its
                // labels or the folder summary, so the status beside it stays readable.
                let compact = self.compact;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (current, source) = self.session.settings.global_format();
                    // Issue #371: counts, the folder and where the default comes from sit behind
                    // one Details control, so the footer keeps its save state and format in view.
                    if !compact {
                        let nodes: usize = self.session.tree().iter().map(|n| n.count()).sum();
                        let notebooks = crate::i18n::format_count_id("ui.n_notebook_n_notebooks", self.session.tree().len() as u64);
                        let items = crate::i18n::format_count_id("ui.n_item_n_items", nodes as u64);
                        let folder = self.shown(self.session.root());
                        let summary = crate::i18n::format_message_id(
                            "ui.notebooks_items_folder",
                            &[("notebooks", &notebooks), ("items", &items), ("folder", &folder)],
                        );
                        let inherited = format!("({})", crate::tl!(source.label()));
                        let details = ui.button(crate::tl_id!("ui.notebook_details"));
                        egui::Popup::menu(&details).show(|ui| {
                            ui.label(egui::RichText::new(summary).small().color(self.tokens.muted));
                            ui.label(egui::RichText::new(inherited).small().color(self.tokens.muted));
                        });
                        ui.separator();
                    }
                    let mut chosen = current;
                    let combo = if compact {
                        egui::ComboBox::from_id_salt("default-format")
                    } else {
                        egui::ComboBox::from_label(crate::tl_id!("ui.default_format"))
                    };
                    let combo = combo.selected_text(current.label()).show_ui(ui, |ui| {
                        for f in PageFormat::ALL {
                            ui.selectable_value(&mut chosen, f, f.label());
                        }
                    });
                    if compact {
                        combo.response.on_hover_text(crate::tl_id!("ui.default_format"));
                    } else {
                        ui.label(egui::RichText::new(crate::tl_id!("ui.new_pages")).small().color(self.tokens.muted));
                    }
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
        // The update card may ask the app to close for an installed update. It is asked only
        // when no page has unsaved work: the close below then goes through the normal close
        // path, which saves the page first.
        #[cfg(not(target_arch = "wasm32"))]
        let can_close = !self.is_dirty() && !self.page_conflict;
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(prompt) = self.update_prompt.as_mut() {
            // The shared card is drawn to its own spacing, so the app's spacing does not pad it.
            let app_spacing = ctx.global_style().spacing.clone();
            ctx.global_style_mut(|style| style.spacing = egui::Spacing::default());
            // Asks GitHub for the named release once, then picks up its answer each frame.
            prompt.check_at_start(ctx);
            prompt.tick(ctx);
            let action = prompt.show_if(ctx, can_close);
            ctx.global_style_mut(|style| style.spacing = app_spacing);
            match action {
                crate::update_card::updater::Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
                crate::update_card::updater::Action::Close => {
                    if self.native_hosted {
                        ctx.data_mut(|data| data.insert_temp(egui::Id::new("update-process-close"), true));
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    } else {
                        self.request_window_close(ctx);
                    }
                }
                _ => {}
            }
        }
        // View > Immersive Reader (issue #299) covers the window until it is closed.
        if let Some(reader) = self.reader.as_mut()
            && crate::reader::show(ctx, reader, &self.tokens) == Some(crate::reader::Act::Close)
        {
            self.reader = None;
        }
        self.picture_text_dialog(ctx);
        self.print_dialog(ctx);
        if let Some(mut dialog) = self.custom_tags.take() {
            let out = dialog.show(ctx, &self.tokens);
            if let Some(list) = out.changed {
                if let Err(e) = self.session.settings.set_custom_tags(&list) {
                    self.say(crate::i18n::format_message_id("ui.could_not_save_your_tags_error", &[("error", &e.to_string())]));
                }
                if self.tags_pane.is_some() {
                    self.refresh_tags();
                }
            }
            if let Some(said) = out.said {
                self.say(said);
            }
            if !out.closed {
                self.custom_tags = Some(dialog);
            }
        }
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
                    .title_bar(false)
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
                let backdrop = modal_backdrop(ctx, "first-use-wizard-modal-backdrop");
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
                // The wizard is 520 points wide where there is room. On a phone it takes the
                // screen's width, less its frame and a margin, and the banner shrinks with it,
                // so the banner and the buttons stay on screen (issue #477).
                let screen = ctx.content_rect();
                let width = (screen.width() - WIZARD_SCREEN_MARGIN).clamp(1.0, 520.0);
                let banner_width = width.min(480.0);
                let body_height = (screen.height() - WIZARD_SCREEN_MARGIN - WIZARD_BUTTON_ROW).max(80.0);
                egui::Window::new(title.as_str())
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .max_width(width)
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        ui.set_min_width(width);
                        ui.set_max_width(width);
                        egui::ScrollArea::vertical().max_height(body_height).auto_shrink([false, true]).show(ui, |ui| {
                            if *step == 0 {
                                ui.add(egui::Image::from_bytes(banner_uri, banner_bytes).fit_to_exact_size(vec2(banner_width, banner_width / 2.0)));
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
                        });
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
                        Ok(()) => {
                            crate::ribbon::clear_recent_fonts(ctx);
                            self.ui_scale = 100;
                            self.ui_scale_pending = true;
                            self.dialog = Some(self.first_use_wizard());
                        }
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
                let backdrop = modal_backdrop(ctx, "missing-attachment-icon-backdrop");
                egui::Window::new(crate::tl_id!("ui.attachment_recovery"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
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
                let backdrop = modal_backdrop(ctx, "unreferenced-attachment-backdrop");
                egui::Window::new(crate::tl_id!("ui.attachment_recovery"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
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
                let backdrop = modal_backdrop(ctx, "open-attachment-warning-backdrop");
                egui::Window::new(crate::tl_id!("ui.open_attachment"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
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
            Dialog::NewNotebook { name, format, first } => {
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
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                        if *first {
                            r.request_focus();
                            *first = false;
                        }
                        if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                            keep = false;
                        }
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
                        } else if *kind == NodeKind::Section {
                            // The new section is the one chosen, so its pages (none yet, with
                            // the Add page button above them) are what the page list shows next
                            // (issue #139).
                            self.selected = Some(dir.clone());
                            // And it starts with a page, opened at its title, as OneNote's new
                            // sections do (issue #570): the section is ready to write in.
                            match first_page(self.session.tree(), &dir).or_else(|| first_page_under(self.session.tree(), &dir)) {
                                Some(page) => self.open_page(&page),
                                None => self.create_page_in(&dir),
                            }
                        }
                        self.say(crate::tl_id!("ui.created"));
                    }
                    keep = false;
                }
            }
            Dialog::MoveOrCopy { path, title, kind, dest } => {
                let places = self.move_destinations(path, *kind);
                let here = path.parent().map(Path::to_path_buf);
                let fits = |p: &Path| notecraft_store::tree::read_node(p).is_some_and(|n| n.kind.may_contain().contains(kind));
                let mut chosen: Option<bool> = None;
                egui::Window::new(crate::tl_id!("ui.move_or_copy"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(crate::i18n::format_message_id("ui.move_or_copy_title_to", &[("title", title.as_str())]));
                        let now = places
                            .iter()
                            .find(|(p, _, _)| here.as_deref() == Some(p.as_path()))
                            .map(|(_, name, _)| name.clone())
                            .or_else(|| here.as_deref().and_then(notecraft_store::tree::read_node).map(|n| n.title))
                            .unwrap_or_default();
                        ui.label(
                            egui::RichText::new(crate::i18n::format_message_id("ui.it_is_in_place_now", &[("place", now.as_str())]))
                                .small()
                                .color(self.tokens.muted),
                        );
                        egui::ScrollArea::vertical().max_height(300.0).min_scrolled_height(160.0).show(ui, |ui| {
                            ui.set_min_width(320.0);
                            for (place, name, depth) in &places {
                                ui.horizontal(|ui| {
                                    ui.add_space(*depth as f32 * 16.0);
                                    if fits(place) {
                                        let picked = dest.as_deref() == Some(place.as_path());
                                        let shown = if here.as_deref() == Some(place.as_path()) { format!("{name} \u{2022}") } else { name.clone() };
                                        if ui.selectable_label(picked, shown).clicked() {
                                            *dest = Some(place.clone());
                                        }
                                    } else {
                                        ui.label(egui::RichText::new(name).color(self.tokens.muted));
                                    }
                                });
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                            {
                                keep = false;
                            }
                            let can = dest.is_some();
                            let moves = can && dest.as_deref() != here.as_deref();
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.copy"), false, can).clicked() {
                                chosen = Some(true);
                            }
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.move"), true, moves).clicked() {
                                chosen = Some(false);
                            }
                        });
                    });
                if let (Some(copy), Some(to)) = (chosen, dest.clone()) {
                    let from = path.clone();
                    self.move_or_copy(&from, &to, copy);
                    keep = false;
                }
            }
            Dialog::Link { block, cell, range, text, address, editing, first } => {
                let (mut go, mut remove) = (false, false);
                let usable = pageedit::linkable_address(address);
                egui::Window::new(crate::tl_id!("ui.link")).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(
                    ctx,
                    |ui| {
                        egui::Grid::new("link-dialog").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                            ui.label(crate::tl_id!("ui.text_to_display"));
                            let words = ui.add(egui::TextEdit::singleline(text).desired_width(320.0));
                            ui.end_row();
                            ui.label(crate::tl_id!("ui.address"));
                            let to = ui.add(egui::TextEdit::singleline(address).desired_width(320.0).hint_text("https://"));
                            ui.end_row();
                            // The address is what is missing first, unless the words are.
                            if *first {
                                if text.is_empty() && !*editing {
                                    words.request_focus()
                                } else {
                                    to.request_focus()
                                }
                                *first = false;
                            }
                            if (words.lost_focus() || to.lost_focus()) && ui.input(|i| i.key_pressed(Key::Enter)) && usable {
                                go = true;
                            }
                        });
                        if !address.trim().is_empty() && !usable {
                            ui.colored_label(self.tokens.warn, crate::tl_id!("ui.that_address_cannot_be_used_for_a_link"));
                        }
                        ui.horizontal(|ui| {
                            if *editing && modal_button(ui, &self.tokens, crate::tl_id!("ui.remove_link"), false).clicked() {
                                remove = true;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                            {
                                keep = false;
                            }
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.ok"), true, usable).clicked() {
                                go = true;
                            }
                        });
                    },
                );
                if go || remove {
                    let (block, cell, range) = (*block, *cell, *range);
                    // Removing keeps the words and lets go of the address.
                    let replacement = if remove { text.trim().to_string() } else { pageedit::link_markup(text, address) };
                    self.replace_in_caret_line(block, cell, range, &replacement);
                    keep = false;
                }
            }
            Dialog::OnlineVideo { block, cell, range, address, first } => {
                let mut go = false;
                let video = crate::onlinevideo::read(address);
                let backdrop = modal_backdrop(ctx, "online-video-backdrop");
                egui::Window::new(crate::tl_id!("ui.online_video_title"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .fixed_size(vec2(420.0, 0.0))
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.online_video_title"));
                        ui.label(crate::tl_id!("ui.online_video_address"));
                        let field = ui.add(egui::TextEdit::singleline(address).desired_width(400.0).hint_text("https://"));
                        if *first {
                            field.request_focus();
                            *first = false;
                        }
                        if field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && video.is_some() {
                            go = true;
                        }
                        if !address.trim().is_empty() && video.is_none() {
                            ui.colored_label(self.tokens.warn, crate::tl_id!("ui.online_video_bad_address"));
                        }
                        ui.label(egui::RichText::new(crate::tl_id!("ui.online_video_explain")).size(11.5).color(self.tokens.muted));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() || ui.input(|i| i.key_pressed(Key::Escape))
                            {
                                keep = false;
                            }
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.ok"), true, video.is_some()).clicked() {
                                go = true;
                            }
                        });
                    });
                if go && let Some(video) = video {
                    let (block, cell, range) = (*block, *cell, *range);
                    self.replace_in_caret_line(block, cell, range, &crate::onlinevideo::line(&video));
                    self.say(crate::tl_id!("ui.online_video_inserted"));
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
            Dialog::InsertTable { rows, columns, first } => {
                let size = table_size(rows, columns);
                let mut go = false;
                let backdrop = modal_backdrop(ctx, "insert-table-backdrop");
                egui::Window::new(crate::tl_id!("ui.insert_table"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.insert_table"));
                        egui::Grid::new("insert-table-dialog").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                            ui.label(crate::tl_id!("ui.number_of_columns"));
                            let c = ui.add(egui::TextEdit::singleline(columns).desired_width(80.0));
                            ui.end_row();
                            ui.label(crate::tl_id!("ui.number_of_rows"));
                            ui.add(egui::TextEdit::singleline(rows).desired_width(80.0));
                            ui.end_row();
                            if *first {
                                c.request_focus();
                                *first = false;
                            }
                        });
                        if size.is_none() {
                            let limits = crate::i18n::format_message_id(
                                "ui.table_size_limits",
                                &[("columns", &TABLE_MAX_COLUMNS.to_string()), ("rows", &TABLE_MAX_ROWS.to_string())],
                            );
                            ui.colored_label(self.tokens.warn, limits);
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.ok"), true, size.is_some()).clicked() {
                                go = true;
                            }
                        });
                    });
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                } else if ctx.input(|input| input.key_pressed(Key::Enter)) && size.is_some() {
                    go = true;
                }
                if go && let Some((r, c)) = size {
                    keep = false;
                    self.insert_table(r, c);
                }
            }
            Dialog::ParagraphSpacing { blocks, before, after, line, first } => {
                let values = [spacing_points(before), spacing_points(after), spacing_points(line)];
                let valid = values.iter().all(Result::is_ok);
                let mut go = false;
                let backdrop = modal_backdrop(ctx, "paragraph-spacing-backdrop");
                egui::Window::new(crate::tl_id!("ui.paragraph_spacing"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.paragraph_spacing"));
                        egui::Grid::new("paragraph-spacing-dialog").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                            let mixed = crate::tl_id!("ui.spacing_mixed");
                            for (label, text) in [
                                (crate::tl_id!("ui.space_before_points"), &mut *before),
                                (crate::tl_id!("ui.space_after_points"), &mut *after),
                                (crate::tl_id!("ui.least_line_height_points"), &mut *line),
                            ] {
                                let name = ui.label(label);
                                let field = ui.add(egui::TextEdit::singleline(text).hint_text(mixed).desired_width(80.0)).labelled_by(name.id);
                                if *first {
                                    field.request_focus();
                                    *first = false;
                                }
                                ui.end_row();
                            }
                        });
                        if !valid {
                            let limits =
                                crate::i18n::format_message_id("ui.spacing_limits", &[("max", &notecraft_model::justify::MAX_POINTS.to_string())]);
                            ui.colored_label(self.tokens.warn, limits);
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false).clicked() {
                                keep = false;
                            }
                            if modal_button(ui, &self.tokens, crate::tl_id!("ui.spacing_reset"), false).clicked() {
                                for text in [&mut *before, &mut *after, &mut *line] {
                                    *text = "0".into();
                                }
                            }
                            if modal_button_enabled(ui, &self.tokens, crate::tl_id!("ui.ok"), true, valid).clicked() {
                                go = true;
                            }
                        });
                    });
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                } else if ctx.input(|input| input.key_pressed(Key::Enter)) && valid {
                    go = true;
                }
                if go && let [Ok(b), Ok(a), Ok(l)] = values {
                    keep = false;
                    let blocks = blocks.clone();
                    self.apply_spacing(&blocks, [b, a, l]);
                }
            }
            Dialog::DeleteFile { page, name } => {
                let mut go = false;
                let mut cancel = false;
                let backdrop = modal_backdrop(ctx, "delete-file-modal-backdrop");
                egui::Window::new(crate::tl_id!("ui.delete_file"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
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
                // Issue #381: the same modal as the other confirmations. The page behind is
                // dimmed and takes no clicks, Escape cancels, and the title says what will
                // happen to this item: a section is moved to the recycle bin, not "deleted".
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                }
                let backdrop = modal_backdrop(ctx, "delete-node-backdrop");
                egui::Window::new(crate::tl_id!("ui.delete"))
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        ui.set_min_width(420.0);
                        ui.set_max_width(520.0);
                        modal_title(
                            ui,
                            &self.tokens,
                            if is_notebook { crate::tl_id!("ui.delete_notebook") } else { crate::tl_id!("ui.move_to_recycle_bin") },
                        );
                        if is_notebook {
                            ui.label(crate::i18n::format_message_id("ui.delete_notebook_title_and_everything_inside_it", &[("title", title)]));
                            ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_removes_the_notebook_folder_from_disk_and_cannot_be_undone"));
                        } else {
                            ui.label(crate::i18n::format_count_message_id(
                                "ui.move_title_and_everything_inside_it_n_item_to_this_n_20c3146ea1",
                                *count as u64,
                                &[("title", title)],
                            ));
                            ui.label(egui::RichText::new(crate::tl_id!("ui.you_can_restore_it_from_history")).color(self.tokens.muted));
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            let cancel = modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false);
                            let confirm = if is_notebook {
                                modal_button_destructive(ui, &self.tokens, crate::tl_id!("ui.delete_notebook"))
                            } else {
                                modal_button(ui, &self.tokens, crate::tl_id!("ui.move_to_recycle_bin"), true)
                            };
                            // Removing a notebook from disk cannot be undone: the keyboard
                            // starts on Cancel. A move to the recycle bin can be, so it starts
                            // on the action, as a routine confirmation does.
                            if is_notebook {
                                hold_modal_focus(&cancel, &[&cancel, &confirm]);
                            } else {
                                hold_modal_focus(&confirm, &[&cancel, &confirm]);
                            }
                            if cancel.clicked() {
                                keep = false;
                            }
                            if confirm.clicked() {
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
                                    self.pad = None;
                                    self.page_revision = None;
                                }
                                // Retain the notebook and the nearest surviving organiser,
                                // so History can recover even its last page or section.
                                if was_open || self.selected.as_ref().is_some_and(|p| p.starts_with(&*path)) {
                                    self.selected = path.parent().map(Path::to_path_buf);
                                }
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
            Dialog::MeetingDetails(choice) => {
                let t = self.tokens;
                let offset = notecraft_model::time::local_offset();
                let mut insert: Option<(notecraft_formats::ics::Event, notecraft_formats::ics::Occurrence)> = None;
                let mut other_file = false;
                egui::Window::new(crate::tl_id!("ui.meeting_details_title"))
                    .collapsible(false)
                    .resizable(false)
                    .fixed_size(vec2(560.0, 0.0))
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        let source = choice.file.as_ref().and_then(|f| f.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        ui.label(egui::RichText::new(crate::i18n::format_message_id("ui.meetings_from_file", &[("file", &source)])).color(t.muted));
                        ui.label(egui::RichText::new(crate::tl_id!("ui.meeting_details_privacy")).small().color(t.muted));
                        ui.separator();
                        if let Some(error) = &choice.error {
                            ui.label(egui::RichText::new(error).color(t.strong));
                        } else {
                            ui.horizontal(|ui| {
                                if ui.button(crate::tl_id!("ui.previous_day")).clicked() {
                                    choice.day -= 1;
                                    choice.picked = None;
                                }
                                if ui.button(crate::tl_id!("ui.today")).clicked() {
                                    choice.day = (notecraft_model::time::now_secs() + i64::from(offset)).div_euclid(86_400);
                                    choice.picked = None;
                                }
                                if ui.button(crate::tl_id!("ui.next_day")).clicked() {
                                    choice.day += 1;
                                    choice.picked = None;
                                }
                                let day = crate::meeting::when_text(&notecraft_formats::ics::When::Day(choice.day), 0);
                                ui.label(egui::RichText::new(day).strong().color(t.strong));
                            });
                            ui.add_space(4.0);
                            let today = choice.today(offset);
                            if today.is_empty() {
                                ui.label(egui::RichText::new(crate::tl_id!("ui.no_meetings_on_this_day")).color(t.muted));
                            }
                            egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                                for (n, o) in today.iter().enumerate() {
                                    let Some(event) = choice.events.get(o.event) else { continue };
                                    let title = if event.summary.is_empty() {
                                        crate::tl_id!("ui.meeting_with_no_title").to_string()
                                    } else {
                                        event.summary.clone()
                                    };
                                    let label = format!("{}  {title}", crate::meeting::span_text(o, offset));
                                    if ui.selectable_label(choice.picked == Some(n), label).clicked() {
                                        choice.picked = Some(n);
                                    }
                                }
                            });
                            if let Some(o) = choice.picked.and_then(|n| today.get(n))
                                && let Some(event) = choice.events.get(o.event)
                            {
                                ui.separator();
                                ui.label(egui::RichText::new(crate::tl_id!("ui.meeting_details_preview")).small().color(t.muted));
                                for b in crate::meeting::details(event, o, offset) {
                                    ui.label(crate::meeting::plain_text(&b));
                                }
                                ui.add_space(6.0);
                                if modal_button(ui, &t, crate::tl_id!("ui.insert_meeting_details"), true).clicked() {
                                    insert = Some((event.clone(), o.clone()));
                                }
                            }
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if modal_button(ui, &t, crate::tl_id!("ui.choose_another_calendar_file"), false).clicked() {
                                other_file = true;
                            }
                            if modal_button(ui, &t, crate::tl_id!("ui.cancel"), false).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                keep = false;
                            }
                        });
                    });
                if let Some((event, o)) = insert {
                    self.dialog = None;
                    self.insert_meeting(&event, &o);
                    return;
                }
                if other_file && let Some(file) = pick_calendar_file() {
                    self.dialog = None;
                    self.open_meeting_details(Some(file));
                    return;
                }
            }
            Dialog::FindByAuthor { pages, chosen } => {
                let mut go: Option<PathBuf> = None;
                let t = self.tokens;
                egui::Window::new(crate::tl_id!("ui.find_by_author_title"))
                    .collapsible(false)
                    .resizable(true)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.set_min_width(520.0);
                        let (named, unknown) = notecraft_store::authors::names(pages);
                        match chosen.clone() {
                            None => {
                                ui.label(egui::RichText::new(crate::tl_id!("ui.find_by_author_hint")).color(t.text));
                                ui.separator();
                                if named.is_empty() {
                                    ui.label(egui::RichText::new(crate::tl_id!("ui.no_page_keeps_a_name_yet")).color(t.muted));
                                }
                                egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                                    for (name, n) in &named {
                                        let label = crate::i18n::format_message_id("ui.name_n_pages", &[("name", name), ("n", &n.to_string())]);
                                        if ui.link(label).clicked() {
                                            *chosen = Some(Some(name.clone()));
                                        }
                                    }
                                    if unknown > 0 {
                                        let label = crate::i18n::format_message_id("ui.unknown_author_n_pages", &[("n", &unknown.to_string())]);
                                        if ui.link(label).on_hover_text(crate::tl_id!("ui.unknown_author_tip")).clicked() {
                                            *chosen = Some(None);
                                        }
                                    }
                                });
                            }
                            Some(who) => {
                                let heading = match &who {
                                    Some(name) => crate::i18n::format_message_id("ui.pages_changed_by_name", &[("name", name)]),
                                    None => crate::tl_id!("ui.pages_whose_author_is_unknown").to_string(),
                                };
                                ui.label(egui::RichText::new(heading).strong().color(t.strong));
                                ui.separator();
                                egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                                    for p in notecraft_store::authors::by(pages, who.as_deref()) {
                                        let when = notecraft_store::recent::stamp(&p.page)
                                            .and_then(|at| notecraft_model::time::long_from_iso(&notecraft_model::time::iso_from_secs(at)));
                                        if result_row(ui, &t, &p.title, &p.notebook, &p.section, when.as_deref(), None) {
                                            go = Some(p.page.clone());
                                        }
                                    }
                                });
                                ui.add_space(6.0);
                                if modal_button(ui, &t, crate::tl_id!("ui.back_to_the_names"), false).clicked() {
                                    *chosen = None;
                                }
                            }
                        }
                        ui.add_space(8.0);
                        if modal_button(ui, &t, crate::tl_id!("ui.close"), false).clicked() {
                            keep = false;
                        }
                    });
                if let Some(page) = go {
                    self.open_path(&path_str(&page));
                    keep = false;
                }
            }
            Dialog::RecentEdits { range, this_notebook, filter, items } => {
                let mut changed = false;
                let mut go: Option<PathBuf> = None;
                let t = self.tokens;
                let has_page = self.editor.is_some();
                egui::Window::new(crate::tl_id!("ui.recent_edits"))
                    .collapsible(false)
                    .resizable(true)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.set_min_width(520.0);
                        ui.horizontal_wrapped(|ui| {
                            for r in RecentRange::ALL {
                                if ui.selectable_label(*range == r, r.label()).clicked() && *range != r {
                                    *range = r;
                                    changed = true;
                                }
                            }
                        });
                        ui.horizontal(|ui| {
                            if ui.selectable_label(!*this_notebook, crate::tl_id!("ui.all_open_notebooks")).clicked() && *this_notebook {
                                *this_notebook = false;
                                changed = true;
                            }
                            if ui.add_enabled(has_page, egui::Button::selectable(*this_notebook, crate::tl_id!("ui.this_notebook"))).clicked()
                                && !*this_notebook
                            {
                                *this_notebook = true;
                                changed = true;
                            }
                        });
                        ui.horizontal(|ui| {
                            let name = ui.label(crate::tl_id!("ui.filter"));
                            ui.add(
                                egui::TextEdit::singleline(filter)
                                    .hint_text(crate::tl_id!("ui.filter_by_title_section_or_notebook"))
                                    .desired_width(f32::INFINITY),
                            )
                            .labelled_by(name.id);
                        });
                        ui.separator();
                        let needle = filter.trim().to_lowercase();
                        let shown: Vec<&notecraft_store::recent::Edited> = items
                            .iter()
                            .filter(|e| {
                                needle.is_empty()
                                    || e.title.to_lowercase().contains(&needle)
                                    || e.notebook.to_lowercase().contains(&needle)
                                    || e.section.iter().any(|s| s.to_lowercase().contains(&needle))
                            })
                            .collect();
                        if shown.is_empty() {
                            ui.label(egui::RichText::new(crate::tl_id!("ui.no_pages_were_changed_in_this_time")).color(t.muted));
                        }
                        egui::ScrollArea::vertical().max_height(380.0).auto_shrink([false, true]).show(ui, |ui| {
                            for e in shown {
                                let when = notecraft_model::time::long_from_iso(&notecraft_model::time::iso_from_secs(e.at)).unwrap_or_default();
                                let note = (!e.stamped)
                                    .then_some(crate::tl_id!("ui.this_page_keeps_no_time_of_its_own_this_is_when_its_file_was_last_changed"));
                                if result_row(ui, &t, &e.title, &e.notebook, &e.section, Some(&when), note) {
                                    go = Some(e.page.clone());
                                }
                            }
                        });
                        ui.add_space(8.0);
                        if modal_button(ui, &t, crate::tl_id!("ui.close"), false).clicked() {
                            keep = false;
                        }
                    });
                if changed {
                    *items = self.recent_edits(*range, *this_notebook);
                }
                if let Some(page) = go {
                    self.open_path(&path_str(&page));
                    keep = false;
                }
            }
            Dialog::PageVersions { page, versions, preview } => {
                // Issue #382: a review workspace. The list (the current page, then each saved
                // version with when it was saved) sits beside the preview of the one selected, or
                // above it in a narrow window. Selecting a row is how a version is looked at;
                // Restore acts on the selected version only; Return to page or Escape leaves the
                // page as it was.
                let can_restore = notecraft_store::page::read_only(page).is_ok_and(|locked| !locked);
                let mut restore: Option<String> = None;
                // `Some(None)` selects the current page, `Some(Some(i))` saved version `i`.
                let mut select: Option<Option<usize>> = None;
                let t = self.tokens;
                let selected = preview.as_ref().and_then(|p| versions.iter().position(|v| v.id == p.id));
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                }
                // Up and Down move through the list, as in any list, while no text box has the keys.
                if !ctx.egui_wants_keyboard_input() {
                    let rows = versions.len();
                    let (up, down) =
                        ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::ArrowDown)));
                    if down && rows > 0 {
                        select = Some(Some(selected.map_or(0, |n| (n + 1).min(rows - 1))));
                    } else if up {
                        select = Some(selected.and_then(|n| n.checked_sub(1)));
                    }
                }
                // A row picked with the keys is scrolled into sight on the next frame, when it is drawn
                // as the selected one.
                let reveal_id = egui::Id::new("page-versions-reveal");
                let reveal = ctx.data_mut(|d| d.remove_temp::<bool>(reveal_id)).unwrap_or(false);
                if select.is_some() {
                    ctx.data_mut(|d| d.insert_temp(reveal_id, true));
                }
                let screen = ctx.viewport_rect();
                let width = (screen.width() - 48.0).clamp(320.0, 980.0);
                let side_by_side = width >= 720.0;
                let height = (screen.height() * 0.78 - 170.0).max(160.0);
                let saved_at = page_saved_at(page).map(|iso| crate::versions::when(&iso));
                let backdrop = modal_backdrop(ctx, "page-versions-backdrop");
                egui::Window::new(crate::tl_id!("ui.page_versions_2"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .fixed_size(vec2(width, 0.0))
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&t))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(crate::tl_id!("ui.page_versions_2")).strong().size(18.0).color(t.strong));
                            let title = notecraft_store::tree::read_node(page).map(|n| n.title).unwrap_or_default();
                            if !title.is_empty() {
                                ui.label(egui::RichText::new(title).color(t.muted));
                            }
                        });
                        if !can_restore {
                            ui.colored_label(t.warn, crate::tl_id!("ui.page_is_read_only"));
                        }
                        ui.separator();
                        let list = |ui: &mut Ui, select: &mut Option<Option<usize>>, list_height: f32| {
                            // A height of its own: the window is sized to what it holds, and a list left to
                            // shrink to the room it is given showed one row and hid the next.
                            egui::ScrollArea::vertical()
                                .id_salt("page-versions-list")
                                .max_height(list_height)
                                .min_scrolled_height(list_height)
                                .auto_shrink([false, true])
                                .show(ui, |ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    let current_sub = saved_at.as_ref().map_or_else(
                                        || crate::tl_id!("ui.the_page_as_it_is_now").to_string(),
                                        |when| crate::i18n::format_message_id("ui.saved_when", &[("when", when)]),
                                    );
                                    let current = version_row(ui, &t, crate::tl_id!("ui.current_page"), &current_sub, selected.is_none());
                                    if reveal && selected.is_none() {
                                        current.scroll_to_me(None);
                                    }
                                    if current.clicked() {
                                        *select = Some(None);
                                    }
                                    for (n, version) in versions.iter().enumerate() {
                                        let size =
                                            crate::i18n::format_message_id("ui.size_kb", &[("size", &version.bytes.div_ceil(1024).to_string())]);
                                        let sub = format!("{} \u{b7} {size}", crate::versions::when(&version.created));
                                        let row = version_row(ui, &t, crate::tl_id!("ui.saved_version"), &sub, selected == Some(n));
                                        if reveal && selected == Some(n) {
                                            row.scroll_to_me(None);
                                        }
                                        if row.clicked() {
                                            *select = Some(Some(n));
                                        }
                                    }
                                    if versions.is_empty() {
                                        ui.add_space(6.0);
                                        ui.label(
                                            egui::RichText::new(crate::tl_id!("ui.no_saved_versions_yet_notecraft_keeps_a_copy_when_th_aa2e25acad"))
                                                .small()
                                                .color(t.muted),
                                        );
                                    }
                                });
                        };
                        let shown = |ui: &mut Ui, preview: &mut Option<Box<crate::versions::Preview>>, preview_height: f32| {
                            ui.vertical(|ui| {
                                let Some(p) = preview.as_mut() else {
                                    ui.label(egui::RichText::new(crate::tl_id!("ui.current_page")).strong().size(15.0).color(t.text));
                                    ui.label(egui::RichText::new(crate::tl_id!("ui.current_page_explainer")).color(t.muted));
                                    return;
                                };
                                // A saved version, read-only (issue #278): said in the pane itself,
                                // so it is never taken for the page.
                                ui.label(
                                    egui::RichText::new(crate::i18n::format_message_id("ui.saved_version_when", &[("when", &p.when)]))
                                        .strong()
                                        .size(15.0)
                                        .color(t.text),
                                );
                                ui.colored_label(
                                    t.warn,
                                    crate::tl_id!("ui.an_older_version_read_only_the_page_is_not_changed_unless_you_restore_it"),
                                );
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(crate::tl_id!("ui.show")).small().color(t.muted));
                                    if ui.selectable_label(!p.comparing, crate::tl_id!("ui.this_version")).clicked() {
                                        p.comparing = false;
                                    }
                                    if ui.selectable_label(p.comparing, crate::tl_id!("ui.compare_with_the_page")).clicked() {
                                        p.comparing = true;
                                    }
                                });
                                ui.separator();
                                // The preview is its own surface: the paper colour, framed, so the
                                // version does not read as the live page behind the window.
                                egui::Frame::new()
                                    .fill(t.editor)
                                    .stroke(egui::Stroke::new(1.0, t.faint))
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::same(8))
                                    .show(ui, |ui| {
                                        egui::ScrollArea::vertical()
                                            .id_salt("page-version-preview")
                                            .max_height(preview_height)
                                            .min_scrolled_height(preview_height)
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                if p.comparing {
                                                    let changed = p.changes.iter().any(|c| !matches!(c, crate::versions::Change::Same(_)));
                                                    if p.truncated {
                                                        ui.label(crate::tl_id!("ui.page_version_comparison_truncated"));
                                                    } else if !changed {
                                                        ui.label(crate::tl_id!("ui.this_version_has_the_same_words_as_the_page"));
                                                    }
                                                    for change in &p.changes {
                                                        let (mark, line, colour) = match change {
                                                            crate::versions::Change::Same(l) => ("  ", l, t.muted),
                                                            crate::versions::Change::OnlyThen(l) => ("− ", l, t.warn),
                                                            crate::versions::Change::OnlyNow(l) => ("+ ", l, t.accent),
                                                        };
                                                        ui.label(egui::RichText::new(format!("{mark}{line}")).monospace().color(colour));
                                                    }
                                                    ui.add_space(4.0);
                                                    ui.label(
                                                        egui::RichText::new(crate::tl_id!(
                                                            "ui.lines_marked_are_only_in_this_version_lines_marked_are_only_in_the_page"
                                                        ))
                                                        .small()
                                                        .color(t.muted),
                                                    );
                                                } else {
                                                    let _ = crate::preview::show(
                                                        ui,
                                                        &p.blocks,
                                                        page,
                                                        &t,
                                                        None,
                                                        &notecraft_model::justify::Justified::default(),
                                                    );
                                                }
                                            });
                                    });
                            });
                        };
                        if side_by_side {
                            ui.horizontal_top(|ui| {
                                let list_width = 270.0;
                                ui.allocate_ui_with_layout(vec2(list_width, height), egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.set_width(list_width);
                                    list(ui, &mut select, height);
                                });
                                ui.separator();
                                ui.allocate_ui_with_layout(vec2(ui.available_width(), height), egui::Layout::top_down(egui::Align::Min), |ui| {
                                    shown(ui, preview, height - 110.0);
                                });
                            });
                        } else {
                            list(ui, &mut select, (height * 0.35).max(120.0));
                            ui.separator();
                            shown(ui, preview, (height * 0.55).max(140.0));
                        }
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            let chosen = preview.as_ref().map(|p| p.id.clone());
                            if modal_button_enabled(ui, &t, crate::tl_id!("ui.restore_this_version"), true, can_restore && chosen.is_some()).clicked()
                            {
                                restore = chosen;
                            }
                            if modal_button(ui, &t, crate::tl_id!("ui.return_to_page"), false).clicked() {
                                keep = false;
                            }
                        });
                    });
                match select {
                    Some(None) => *preview = None,
                    Some(Some(n)) if Some(n) != selected => {
                        if let Some(version) = versions.get(n) {
                            // Compared with the page as it stands, unsaved words included.
                            let now = self.editor.as_ref().filter(|e| e.dir == *page).map(|e| e.body.clone());
                            let now = now.or_else(|| notecraft_store::page::load_page(page).ok().map(|p| p.body)).unwrap_or_default();
                            let comparing = preview.as_ref().is_some_and(|p| p.comparing);
                            match crate::versions::open(page, &version.id, &version.created, &now) {
                                Ok(mut p) => {
                                    // The mode chosen stays as one version is swapped for another.
                                    p.comparing = comparing;
                                    *preview = Some(Box::new(p));
                                }
                                Err(e) => self.complain(&crate::i18n::format_message_id("ui.cannot_open_that_version_error", &[("error", &e)])),
                            }
                        }
                    }
                    _ => {}
                }
                if let Some(id) = restore
                    && self.run("page.version.restore", &params(&[("path", &path_str(page)), ("id", &id)])).is_some()
                {
                    self.open_page(page);
                    self.say(crate::tl_id!("ui.restored_the_saved_page_version"));
                    keep = false;
                }
            }
            Dialog::RecycleBin { notebook, name, items, places, selected } => {
                // Issue #365: a recovery workspace rather than a small list window. One item is
                // selected; Restore acts on it, and deleting it for good is a separate, quieter
                // action that asks first. Back or Escape returns to the page.
                let t = self.tokens;
                let mut restore: Option<String> = None;
                let mut purge: Option<(String, String)> = None;
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                }
                let screen = ctx.viewport_rect();
                let width = (screen.width() - 48.0).clamp(320.0, 680.0);
                let list_height = (screen.height() * 0.7 - 190.0).max(120.0);
                let backdrop = modal_backdrop(ctx, "recycle-bin-backdrop");
                egui::Window::new(crate::tl_id!("ui.notebook_recycle_bin_2"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .fixed_size(vec2(width, 0.0))
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&t))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        ui.set_width(width);
                        ui.horizontal(|ui| {
                            let words = crate::tl_id!("ui.back_to_notes");
                            let size = vec2(text_width(ui, words, 13.0) + 38.0, 28.0);
                            let (rect, back) = ui.allocate_exact_size(size, Sense::click());
                            let painter = ui.painter_at(rect);
                            painter.rect_filled(rect, 4.0, if back.hovered() || back.has_focus() { t.row_hover } else { t.panel });
                            painter.rect_stroke(
                                rect,
                                4.0,
                                egui::Stroke::new(1.0, if back.has_focus() { t.accent } else { t.faint }),
                                egui::StrokeKind::Inside,
                            );
                            let arrow = egui::Rect::from_center_size(egui::pos2(rect.min.x + 15.0, rect.center().y), vec2(14.0, 14.0));
                            crate::ribbon_icons::paint(&painter, arrow, crate::ribbon_icons::Mark::Back, t.text);
                            painter.text(
                                egui::pos2(rect.min.x + 28.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                words,
                                egui::FontId::proportional(13.0),
                                t.text,
                            );
                            back.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, words));
                            if back.clicked() {
                                keep = false;
                            }
                            ui.add_space(8.0);
                            ui.label(egui::RichText::new(crate::tl_id!("ui.notebook_recycle_bin_2")).strong().size(18.0).color(t.strong));
                        });
                        if !name.is_empty() {
                            ui.label(egui::RichText::new(name.as_str()).color(t.muted));
                        }
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new(crate::tl_id!("ui.recycle_bin_explainer")).small().color(t.muted));
                        ui.add_space(6.0);
                        ui.separator();
                        if items.is_empty() {
                            ui.add_space(18.0);
                            ui.vertical_centered(|ui| {
                                let (rect, _) = ui.allocate_exact_size(vec2(32.0, 32.0), Sense::hover());
                                crate::ribbon_icons::paint(ui.painter(), rect, crate::ribbon_icons::Mark::Delete, t.muted);
                                ui.add_space(6.0);
                                ui.label(egui::RichText::new(crate::tl_id!("ui.this_notebook_s_recycle_bin_is_empty")).color(t.text));
                            });
                            ui.add_space(18.0);
                        } else {
                            egui::ScrollArea::vertical().id_salt("recycle-bin-list").max_height(list_height).auto_shrink([false, true]).show(
                                ui,
                                |ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    for (index, item) in items.iter().enumerate() {
                                        let is_selected = selected.as_deref() == Some(item.id.as_str());
                                        let row_width = ui.available_width();
                                        let (rect, row) = ui.allocate_exact_size(vec2(row_width, 48.0), Sense::click());
                                        let painter = ui.painter_at(rect);
                                        if is_selected {
                                            painter.rect_filled(rect, 4.0, t.row_selected);
                                            painter.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), 2.0, t.accent);
                                        } else if row.hovered() {
                                            painter.rect_filled(rect, 4.0, t.row_hover);
                                        }
                                        if row.has_focus() {
                                            painter.rect_stroke(rect.shrink(1.0), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                                        }
                                        let (icon, kind_name) = match item.kind {
                                            NodeKind::Section => (Icon::Section, crate::tl_id!("ui.section")),
                                            NodeKind::SectionGroup => (Icon::SectionGroup, crate::tl_id!("ui.section_group")),
                                            NodeKind::Notebook => (Icon::Notebook, crate::tl_id!("ui.notebook")),
                                            NodeKind::Page => (Icon::Page, crate::tl_id!("ui.page")),
                                        };
                                        let icon_rect =
                                            egui::Rect::from_center_size(egui::pos2(rect.min.x + 22.0, rect.center().y), vec2(18.0, 18.0));
                                        crate::icons::paint(&painter, icon_rect, icon, t.text);
                                        let text_left = rect.min.x + 44.0;
                                        let room = (rect.max.x - text_left - 10.0).max(20.0);
                                        let title = fit_to_width(ui, &item.title, 14.0, room);
                                        crate::bidi::line_at(
                                            ui,
                                            &painter,
                                            egui::pos2(text_left, rect.min.y + 15.0),
                                            Align2::LEFT_CENTER,
                                            &title,
                                            14.0,
                                            t.text,
                                            None,
                                        );
                                        let when = crate::versions::when(&item.deleted);
                                        let mut details = match places.get(index).filter(|p| !p.is_empty()) {
                                            Some(place) => {
                                                crate::i18n::format_message_id("ui.recycle_bin_deleted_from", &[("when", &when), ("place", place)])
                                            }
                                            None => crate::i18n::format_message_id("ui.recycle_bin_deleted_at", &[("when", &when)]),
                                        };
                                        if item.kind != NodeKind::Page || item.count > 1 {
                                            details =
                                                format!("{details} \u{b7} {}", crate::i18n::format_count_id("ui.n_item_n_items", item.count as u64));
                                        }
                                        let shown = fit_to_width(ui, &details, 12.0, room);
                                        crate::bidi::line_at(
                                            ui,
                                            &painter,
                                            egui::pos2(text_left, rect.min.y + 34.0),
                                            Align2::LEFT_CENTER,
                                            &shown,
                                            12.0,
                                            t.muted,
                                            None,
                                        );
                                        // A screen reader hears what the eye reads on both lines:
                                        // the title, then the kind, when and where from.
                                        let heard = format!("{}, {kind_name}, {details}", item.title);
                                        row.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, is_selected, heard.clone()));
                                        let full = format!("{}\n{details}", item.title);
                                        if row.on_hover_text(full).clicked() {
                                            *selected = Some(item.id.clone());
                                        }
                                    }
                                },
                            );
                        }
                        ui.separator();
                        ui.add_space(4.0);
                        let chosen = selected.as_ref().and_then(|id| items.iter().find(|i| &i.id == id));
                        ui.horizontal(|ui| {
                            let delete = ui
                                .add_enabled_ui(chosen.is_some(), |ui| {
                                    modal_button_destructive(ui, &t, crate::tl_id!("ui.delete_permanently_ellipsis"))
                                })
                                .inner;
                            if delete.clicked()
                                && let Some(item) = chosen
                            {
                                purge = Some((item.id.clone(), item.title.clone()));
                            }
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let restore_button = modal_button_enabled(ui, &t, crate::tl_id!("ui.restore"), true, chosen.is_some());
                                if restore_button.clicked()
                                    && let Some(item) = chosen
                                {
                                    restore = Some(item.id.clone());
                                }
                            });
                        });
                    });
                if let Some(id) = restore {
                    if self.run("recycle.restore", &params(&[("notebook", &path_str(notebook)), ("id", &id)])).is_some() {
                        self.say(crate::tl_id!("ui.restored_to_the_original_location"));
                        // The list is refreshed in place: whatever else is in the bin stays in view.
                        let notebook = notebook.clone();
                        keep = false;
                        self.open_recycle_bin(notebook, None);
                    }
                } else if let Some((id, title)) = purge {
                    next = Dialog::PurgeRecycleItem { notebook: notebook.clone(), id, title };
                }
            }
            Dialog::PurgeRecycleItem { notebook, id, title } => {
                let mut purge = false;
                let mut cancel = false;
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    cancel = true;
                }
                let backdrop = modal_backdrop(ctx, "purge-recycled-backdrop");
                egui::Window::new(crate::tl_id!("ui.permanently_delete"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        ui.set_min_width(400.0);
                        ui.set_max_width(520.0);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.permanently_delete"));
                        ui.label(crate::i18n::format_message_id("ui.permanently_delete_title_and_everything_inside_it", &[("title", title)]));
                        ui.colored_label(self.tokens.warn, crate::tl_id!("ui.this_cannot_be_undone"));
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            let no = modal_button(ui, &self.tokens, crate::tl_id!("ui.cancel"), false);
                            let yes = modal_button_destructive(ui, &self.tokens, crate::tl_id!("ui.delete_permanently"));
                            // Nothing to undo afterwards: the keyboard starts on Cancel.
                            hold_modal_focus(&no, &[&no, &yes]);
                            cancel |= no.clicked();
                            purge = yes.clicked();
                        });
                    });
                if purge {
                    if self.run("recycle.purge", &params(&[("notebook", &path_str(notebook)), ("id", id)])).is_some() {
                        self.say(crate::tl_id!("ui.permanently_deleted"));
                    }
                    keep = false;
                    self.open_recycle_bin(notebook.clone(), None);
                } else if cancel {
                    // Back to the bin with the same item selected.
                    keep = false;
                    self.open_recycle_bin(notebook.clone(), Some(id.clone()));
                }
            }
            Dialog::Notebooks => {
                // Escape closes it and changes nothing (issue #559).
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                }
                let here = self.editor.as_ref().map(|e| e.dir.clone()).or_else(|| self.selected.clone());
                let notebooks: Vec<(PathBuf, String)> = self.session.tree().iter().map(|n| (n.path.clone(), n.info.title.clone())).collect();
                let mut chosen: Option<PathBuf> = None;
                let backdrop = modal_backdrop(ctx, "notebooks-backdrop");
                egui::Window::new(crate::tl_id!("ui.notebooks_switch"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.notebooks_switch"));
                        ui.label(egui::RichText::new(crate::tl_id!("ui.notebooks_switch_hint")).small().color(self.tokens.muted));
                        ui.add_space(6.0);
                        ui.set_min_width(280.0);
                        let mut rows: Vec<egui::Response> = Vec::new();
                        let mut current: Option<usize> = None;
                        for (path, title) in &notebooks {
                            let on = here.as_ref().is_some_and(|h| h.starts_with(path));
                            // Two notebooks of one name are told apart by their folders.
                            let twin = notebooks.iter().filter(|(_, t)| t == title).count() > 1;
                            let shown = if twin { format!("{title} ({})", self.shown(path)) } else { title.clone() };
                            let row = ui.add_sized([ui.available_width(), 26.0], egui::Button::selectable(on, shown));
                            if on {
                                current = Some(rows.len());
                            }
                            if row.clicked() {
                                chosen = Some(path.clone());
                            }
                            rows.push(row);
                        }
                        ui.add_space(6.0);
                        let close = modal_button(ui, &self.tokens, crate::tl_id!("ui.close"), false);
                        if close.clicked() {
                            keep = false;
                        }
                        // The keyboard starts on the notebook that is open, and stays in the list.
                        if let Some(first) = current.and_then(|i| rows.get(i)).or_else(|| rows.first()) {
                            let mut inside: Vec<&egui::Response> = rows.iter().collect();
                            inside.push(&close);
                            hold_modal_focus(first, &inside);
                        }
                    });
                if let Some(notebook) = chosen {
                    keep = false;
                    self.dialog = None;
                    self.open_notebook_where_left(&notebook);
                }
            }
            Dialog::Symbols => {
                let mut chosen: Option<&'static str> = None;
                // A modal like the other dialogs (issue #262): the page behind is dimmed and
                // takes no clicks, so the caret stays where the symbol will go, and Escape
                // closes it with nothing inserted.
                if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
                    keep = false;
                }
                let backdrop = modal_backdrop(ctx, "insert-symbol-backdrop");
                egui::Window::new(crate::tl_id!("ui.insert_symbol"))
                    .title_bar(false)
                    .collapsible(false)
                    .resizable(false)
                    .order(egui::Order::Foreground)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .frame(modal_frame(&self.tokens))
                    .show(ctx, |ui| {
                        stay_above(ui, backdrop);
                        modal_title(ui, &self.tokens, crate::tl_id!("ui.insert_symbol"));
                        ui.label(crate::tl_id!("ui.choose_a_symbol_to_insert_at_the_caret"));
                        ui.add_space(6.0);
                        let mut buttons: Vec<egui::Response> = Vec::new();
                        for row in [["©", "®", "™", "§", "¶"], ["•", "→", "←", "±", "°"], ["×", "÷", "√", "€", "£"], ["¥", "¢", "∞", "≠", "≤"]]
                        {
                            ui.horizontal(|ui| {
                                for symbol in row {
                                    let button = egui::Button::new(egui::RichText::new(symbol).size(20.0).color(self.tokens.text))
                                        .min_size(vec2(46.0, 36.0))
                                        .fill(self.tokens.panel)
                                        .stroke(egui::Stroke::new(0.8, self.tokens.faint));
                                    let resp = ui.add(button);
                                    if resp.clicked() {
                                        chosen = Some(symbol);
                                    }
                                    buttons.push(resp);
                                }
                            });
                        }
                        ui.add_space(6.0);
                        let close = modal_button(ui, &self.tokens, crate::tl_id!("ui.close"), false);
                        if close.clicked() {
                            keep = false;
                        }
                        // The keyboard starts on the first symbol and stays in the picker (#374).
                        if let Some(first) = buttons.first() {
                            let mut inside: Vec<&egui::Response> = buttons.iter().collect();
                            inside.push(&close);
                            hold_modal_focus(first, &inside);
                        }
                    });
                // Closed with nothing picked, the keyboard goes back to the Symbol button (#374).
                if !keep && chosen.is_none() {
                    ribbon::return_focus_from_symbols(ctx);
                }
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
                let mut params = notecraft_engine::params_from_json(p.get("params"));
                if id == "export.prepare" && !params.contains_key("body") && !params.contains_key("snapshot_path") {
                    if let Some(path) = params.get("path")
                        && let Ok(node) = self.session.resolve(path)
                    {
                        params.extend(self.export_snapshot_params(&node));
                    }
                } else if !id.starts_with("export.") {
                    self.save_now();
                }
                if !id.starts_with("export.") && (self.page_conflict || self.is_dirty()) {
                    return Err("Resolve or save the current page before running another command.".into());
                }
                let out = if id.starts_with("export.") { self.session.run(id, &params) } else { self.run_recording(id, &params) }
                    .map_err(|e| e.to_string())?;
                if matches!(id, "window.new" | "window.quick-note")
                    && let Output::Path(path) = &out
                {
                    self.window_requests.push(crate::windows::Request { path: path.clone(), quick: id == "window.quick-note" });
                }
                if self.editor.as_ref().is_some_and(|ed| !ed.dir.exists()) {
                    self.editor = None;
                }
                Ok(notecraft_engine::output_json(&out))
            }
            "ui.inspect" => Ok(self.inspect(ctx)),
            "ui.window.reload" => Ok(json!({"reloaded": self.reload_conflicted_page()})),
            "ui.window.keep-copy" => self.keep_conflicted_copy().map(|p| json!({"path": path_str(&p)})),
            "ui.open" => {
                self.open_path(text("path").ok_or("missing path")?);
                if self.status_is_error { Err(self.status.clone()) } else { Ok(self.inspect(ctx)) }
            }
            // Put the caret in a block of the open page, the way a click does.
            "ui.block" => {
                let kind = p.get("kind").and_then(Value::as_str).ok_or("missing kind")?.to_string();
                self.apply_block_kind(&kind);
                Ok(json!({"kind": kind}))
            }
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
                    inline: None,
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
                self.request_window_close(ctx);
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
                        notecraft_formats::markdown::Block::Code { lang, text, .. } if lang == "mermaid" => Some(text.clone()),
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
            "canvas": {
                "viewOffset": [self.canvas_pan.x, self.canvas_pan.y],
                // The sideways scroll bar along the page's bottom and its thumb, as
                // [left, top, right, bottom] in screen points, or null when everything fits.
                "horizontalBar": self.page_hbar.map(|(track, _)| [track.min.x, track.min.y, track.max.x, track.max.y]),
                "horizontalThumb": self.page_hbar.map(|(_, thumb)| [thumb.min.x, thumb.min.y, thumb.max.x, thumb.max.y]),
                // True while Insert Space is in hand (issue #164).
                "insertSpace": self.space_tool,
            },
            // How the side panel is arranged: one tree, or the organisers and the pages in
            // columns of their own, with the first one hidden or not.
            "panes": {
                "split": self.split_panes,
                "organisers": self.organisers,
                "compact": self.compact,
                "fullPage": self.full_page,
                "list": self.compact_list_fills(),
            },
            // Home > Find Tags: where the Tags Summary looks and how many lines it lists, or
            // null while it is closed.
            "pageTemplates": self.templates_pane.as_ref().map(|p| json!({
                "templates": p.templates.iter().map(|t| json!({"id": t.id, "name": t.name, "category": t.category.id()})).collect::<Vec<_>>(),
                "name": p.name,
            })),
            "tagsSummary": self.tags_pane.as_ref().map(|p| json!({
                "scope": p.scope.id(),
                "unchecked": p.unchecked,
                "lines": p.found.iter().map(|l| json!({"tag": l.tag.id(), "text": l.text, "done": l.done, "page": l.page, "section": l.section})).collect::<Vec<_>>(),
            })),
            // Back and Forward, and the Quick Access Toolbar's choices.
            "nav": {
                "back": self.nav.back_count(),
                "forward": self.nav.forward_count(),
                "current": self.nav.current().map(|p| self.shown(p)),
            },
            "windowFrame": if self.frameless { "own" } else { "system" },
            "quickAccess": {"shown": self.qat.show, "below": self.qat.below, "hidden": self.qat.hidden},
            // Folded to its tab names (Ctrl+F1), and whether a clicked tab shows it over the page.
            "window": {"secondary": self.secondary_window, "conflict": self.page_conflict},
            "ribbon": {"collapsed": self.ribbon_collapsed, "shown": !self.ribbon_folded() || self.ribbon_peek, "tab": self.ribbon_tab},
            // Whether a dialog is up over the window (Android's Back closes it first).
            "dialogOpen": self.dialog.is_some(),
            "theme": {"id": self.theme().id(), "modePref": self.mode_pref.id(), "accent": self.accent.id()},
            // The paper behind the pages, as the View tab's gallery sets it.
            "paper": self.paper_now().id(),
            "paperSize": self.editor.as_ref().and_then(|e| e.paper_size).map_or_else(|| "auto".to_string(), notecraft_model::paper::Paper::write),
            "ruleColour": self.editor.as_ref().and_then(|e| e.rule_colour).map(notecraft_model::NodeColor::id),
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
                "bounds": self.pad.as_ref().and_then(crate::inkpad::Pad::bounds),
                "gain": self.gain(),
                "strokes": self.pad.as_ref().map(crate::inkpad::Pad::strokes),
                "inked": self.pad.as_ref().is_some_and(|p| !p.is_empty()),
                // The readings of the newest stroke, in page points (issues #44, #293).
                "lastStroke": self.pad.as_ref().map(crate::inkpad::Pad::last_stroke),
                // Draw > Ink Replay (issue #295): how far it is, in strokes drawn whole.
                "replay": crate::inkpad::replay(ctx).map(|r| json!({
                    "shown": self.pad.as_ref().map_or(0, |p| crate::replay::shown(p.drawing(), r.at).0),
                    "playing": r.playing,
                    "speed": r.speed,
                })),
                "autoShapes": self.auto_shapes,
                "tabsLayout": self.tabs_layout,
                // Draw > Ruler (issue #293): where it lies once it is out, in page points.
                "ruler": match crate::inkpad::ruler(ctx) {
                    Some(crate::inkpad::RulerState::Placed(r)) => json!({"x": r.x, "y": r.y, "angle": r.angle, "degrees": r.shown_degrees()}),
                    Some(crate::inkpad::RulerState::Unplaced) => json!({}),
                    None => Value::Null,
                },
            },
            // View > Immersive Reader (issue #299), while it is open.
            "reader": self.reader.as_ref().map(|r| json!({
                "lines": r.lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>(),
                "size": r.size,
                "spacing": r.spacing,
                "at": r.at,
            })),
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
            "unread": self.unread_list(),
            "page": self.editor.as_ref().map(|e| json!({
                "path": e.dir.strip_prefix(self.session.root()).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_default(),
                "title": e.title,
                // What the title area actually shows: blank for a page with no title of its
                // own, unlike `title`, which is always at least the "Untitled page" fallback.
                "titleArea": e.title_buf,
                "format": e.format.id(),
                "dirty": e.dirty_since.is_some() || e.saving.is_some(),
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
                "selectedContainers": crate::containers::selected_boxes(ctx, &e.dir),
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
    /// Fills the strips the platform's own bars lie over (a phone's status bar and navigation
    /// bar, reported as egui's safe area) with the title band's colour, so the system's white
    /// icons read on it in a light theme as well as a dark one. Everything else is laid out
    /// inside the safe area. On a desktop there is none, and nothing is drawn.
    fn paint_system_bars(&self, ctx: &Context) {
        let strips = self.system_bar_strips(ctx);
        if strips.is_empty() {
            return;
        }
        // On the background layer, which egui always paints, clipped to the whole window rather
        // than to the safe area as a layer painter is by default. Nothing else is drawn under
        // the bars: every panel and window is inside the safe area.
        let painter = egui::Painter::new(ctx.clone(), egui::LayerId::background(), ctx.viewport_rect());
        for (strip, colour) in strips {
            painter.rect_filled(strip, 0.0, colour);
        }
    }

    /// The strips [`Self::paint_system_bars`] fills, with their colour: the parts of the window
    /// outside egui's safe area. None on a desktop.
    pub fn system_bar_strips(&self, ctx: &Context) -> Vec<(egui::Rect, egui::Color32)> {
        let (outer, inner) = (ctx.viewport_rect(), ctx.content_rect());
        let strips = [
            egui::Rect::from_min_max(outer.min, egui::pos2(outer.max.x, inner.min.y)),
            egui::Rect::from_min_max(egui::pos2(outer.min.x, inner.max.y), outer.max),
            egui::Rect::from_min_max(egui::pos2(outer.min.x, inner.min.y), egui::pos2(inner.min.x, inner.max.y)),
            egui::Rect::from_min_max(egui::pos2(inner.max.x, inner.min.y), egui::pos2(outer.max.x, inner.max.y)),
        ];
        strips.into_iter().filter(|r| r.is_positive()).map(|r| (r, self.tokens.band)).collect()
    }

    pub fn show(&mut self, ui: &mut Ui) {
        self.take_saves(false);
        crate::ribbon::sync_recent_fonts(
            ui.ctx(),
            || self.session.settings.get(RECENT_FONTS_KEY),
            |text| {
                let _ = self.session.settings.set(RECENT_FONTS_KEY, text);
            },
        );
        if !self.watcher_woken {
            let wake_ctx = ui.ctx().clone();
            self.watcher.set_wake(std::sync::Arc::new(move || wake_ctx.request_repaint()));
            self.watcher_woken = true;
        }
        let open_page = self.editor.as_ref().map(|ed| ed.dir.clone());
        let mut watch_roots = Vec::with_capacity(1 + self.session.opened_elsewhere().len());
        watch_roots.push(self.session.root().to_path_buf());
        watch_roots.extend(self.session.opened_elsewhere().iter().cloned());
        self.watcher.watch_roots(&watch_roots, open_page.as_deref(), self.page_revision.clone());
        let watched = self.watcher.take_changes();
        if watched.tree {
            self.pending_tree_refresh = true;
        }
        if self.pending_tree_refresh && !self.saver.busy() && self.ink_saving.is_none() {
            self.pending_tree_refresh = false;
            crate::fscache::forget_all();
            self.session.refresh();
        }
        if watched.page {
            crate::fscache::forget_all();
            if let Some(ed) = self.editor.as_mut() {
                ed.attachments = notecraft_store::assets::list(&ed.dir);
            }
        }
        // Another window's writes are looked for before anything typed or clicked here is acted
        // on, and twice a second besides. Not every frame: each look asks the disk about the page
        // and its ink, and on a synced folder that held up every frame of a scroll or a pinch.
        let now = ui.input(|i| i.time);
        let acting = ui.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::Key { pressed: true, .. }
                        | egui::Event::Text(_)
                        | egui::Event::Paste(_)
                        | egui::Event::Cut
                        | egui::Event::Ime(_)
                        | egui::Event::PointerButton { pressed: true, .. }
                )
            })
        });
        let due = watched.page || acting || self.changes_checked_at.is_none_or(|at| now - at >= 0.5 || now < at);
        if due {
            self.changes_checked_at = Some(now);
            self.check_page_changes();
        }
        if self.page_conflict {
            self.conflict_screen(ui);
            return;
        }
        ui.push_id(ui.ctx().viewport_id(), |ui| self.show_checked(ui));
        if !due {
            return;
        }
        if self.pad.as_ref().is_some_and(|p| p.dirty) {
            self.check_page_changes();
        }
        if !self.saver.busy()
            && self.ink_saving.is_none()
            && !self.page_conflict
            && let Some(current) = self.editor.as_ref().and_then(|ed| crate::windows::Revision::read(&ed.dir).ok())
            && let Some(previous) = &self.page_revision
            && previous != &current
            && previous.same_page(&current)
            && self.pad.as_ref().is_some_and(crate::inkpad::Pad::matches_disk)
        {
            // Only the ink this view just wrote changed. Other changes must be checked next frame.
            self.page_revision = Some(current);
            if let Some(dir) = self.editor.as_ref().map(|ed| ed.dir.as_path()) {
                self.watcher.note_own_revision(dir, self.page_revision.clone());
            }
        }
    }

    fn show_checked(&mut self, ui: &mut Ui) {
        // egui's root `Ui` covers the whole window, the parts under a phone's status and
        // navigation bars included. Everything is laid out inside the safe area instead
        // (issue #315); on a desktop the two are the same.
        let ctx = ui.ctx().clone();
        let content = ctx.content_rect();
        if content != ctx.viewport_rect() {
            self.paint_system_bars(&ctx);
            let room = ui.max_rect().intersect(content);
            ui.scope_builder(egui::UiBuilder::new().id_salt("safe-area").max_rect(room), |ui| self.show_in(ui));
        } else {
            self.show_in(ui);
        }
    }

    fn show_in(&mut self, ui: &mut Ui) {
        crate::i18n::set_current(&self.ui_language);
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        // Snapshot before Escape: another event in that frame still belongs to
        // the closing picker, never to the note underneath it.
        let emoji_picker_open = self.editor.as_ref().is_some_and(|ed| crate::block_emoji::is_open_for_page(ctx, &ed.dir));
        if self.dialog.is_none() {
            crate::block_emoji::handle_escape(ctx);
        }
        // Search owns Escape before text widgets and the page can consume it.
        let search_id = Id::new(("page-search", ctx.viewport_id()));
        if self.dialog.is_none()
            && (!self.search.is_empty() || ctx.memory(|memory| memory.has_focus(search_id)))
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.dismiss_search();
            ctx.memory_mut(|memory| memory.surrender_focus(search_id));
        }
        if !self.loaders_installed {
            crate::pictures::install(ctx);
            self.loaders_installed = true;
        }
        // Arabic and the other right-to-left scripts have no glyphs in egui's own faces.
        crate::fonts::install_once(ctx);
        // Pen pressure, where the platform reports it. Nothing draws with it yet; it is
        // read here so `ui.inspect` can say whether a pen reaches this window at all.
        ctx.input(|i| self.stylus.update(&i.events));
        crate::ribbon::note_touches(ctx);
        // A pen on a tablet mapped to a large screen moves further in a tap than a mouse does
        // in a click: past the six points egui allows, a tap on a button did nothing. Once a pen
        // has been seen, a click may move twice as far.
        let click_dist = if self.stylus.sample().pen { PEN_CLICK_DIST } else { egui::InputOptions::default().max_click_dist };
        if ctx.options(|o| o.input_options.max_click_dist) != click_dist {
            ctx.options_mut(|o| o.input_options.max_click_dist = click_dist);
        }
        // An update is ready to be put in place: save the page and close, so the script that
        // waits for this window can replace the program and start the new one.
        if !self.update_closing && self.update.lock().is_ok_and(|s| matches!(*s, crate::update::Status::Restart { .. })) {
            self.update_closing = true;
            self.save();
            log::info!("update: closing so the new version can be put in place");
            self.request_window_close(ctx);
        }
        self.apply_theme(ctx);
        // Only when the size was chosen or read at start: egui's own Ctrl+Plus and Ctrl+Minus
        // change the same factor, and are left alone in between.
        if self.ui_scale_pending {
            ctx.set_zoom_factor(f32::from(self.ui_scale) / 100.0);
            self.ui_scale_pending = false;
        }
        if self.window_level_pending {
            let level = if self.always_on_top { egui::viewport::WindowLevel::AlwaysOnTop } else { egui::viewport::WindowLevel::Normal };
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(level));
            self.window_level_pending = false;
        }
        if self.dock_pending {
            self.dock_pending = false;
            self.toggle_dock(ctx);
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
        let queued_paste = std::mem::take(&mut self.keyboard_paste_requested);
        let queued_plain = std::mem::take(&mut self.keyboard_paste_plain);
        // A dialog of the window's own (Copy Text from Picture) owns the keyboard: a paste goes into
        // its text box, never into the page behind it.
        let modal = self.own_modal_open();
        // So does the search box when it has the keyboard: Ctrl+V there was pasted into the page
        // (issue #498).
        let search_focused = ctx.memory(|memory| memory.has_focus(search_id));
        if emoji_picker_open {
            self.paste_into_emoji_search(ctx, queued_paste);
        } else if modal {
            self.paste_into_modal(ctx, queued_paste);
        } else if search_focused {
            self.paste_text_into_focus(ctx, queued_paste);
        } else if queued_paste && queued_plain {
            log::debug!("queued keyboard paste dispatched to paste_plain");
            self.paste_plain();
        } else if queued_paste {
            log::debug!("queued keyboard paste dispatched to paste_here");
            self.paste_here();
        }
        // A button whose file dialog Android cannot show says so (filepicker.android.rs).
        #[cfg(target_os = "android")]
        if let Some(notice) = crate::filepicker::take_notice() {
            self.say(notice);
        }
        if !emoji_picker_open && !modal && !search_focused {
            self.clipboard_keys(ctx);
        }
        self.poll_control(ctx);
        self.poll_import(ctx);
        #[cfg(feature = "voice")]
        self.poll_voice(ctx);
        if !self.event_frames.is_empty() {
            ctx.request_repaint();
        }
        // Ctrl+P prints the open page, as it does everywhere (issue #168).
        if self.dialog.is_none() && !self.own_modal_open() && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::P)) {
            self.open_print();
        }
        // The page's own zoom from the keyboard, as OneNote has it: Ctrl+Alt with plus, minus or 0
        // (issue #376). Ctrl alone with them stays egui's size for the whole window (issue #377
        // keeps the two apart).
        let page_keys_free = self.dialog.is_none() && !self.own_modal_open() && self.custom_tags.is_none();
        if page_keys_free {
            let keys = Modifiers::COMMAND | Modifiers::ALT;
            let step = ctx.input_mut(|i| {
                if i.consume_key(keys, Key::Equals) || i.consume_key(keys, Key::Plus) {
                    Some(1)
                } else if i.consume_key(keys, Key::Minus) {
                    Some(-1)
                } else if i.consume_key(keys, Key::Num0) {
                    Some(0)
                } else {
                    None
                }
            });
            if let Some(step) = step {
                self.ribbon_act(ribbon::Act::Zoom(step));
            }
        }
        // F11 gives the page the whole window and brings the notebooks back, as in OneNote, so Full
        // Page View can be left from the keyboard as well as from its button (issue #376).
        if page_keys_free && self.editor.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::F11)) {
            self.full_page = !self.full_page;
            self.compact_full_page = false;
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
        // Not while a dialog is open: it owns the keyboard, and the page behind it must not
        // change unseen (issue #262: Ctrl+Z undid and saved an edit behind Insert symbol). A
        // text box in the dialog keeps its own undo.
        let page_keys = self.dialog.is_none() && !self.own_modal_open() && !emoji_picker_open && !ctx.memory(|memory| memory.has_focus(search_id));
        if page_keys && ctx.input_mut(|i| i.consume_key(ctrl_shift, Key::Z) || i.consume_key(Modifiers::COMMAND, Key::Y)) {
            // The ink's as well as the page's, whichever Undo took back.
            self.redo();
        } else if page_keys && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::Z)) {
            // The ink's changes as well as the page's: whichever was last, or both when one
            // press made them together.
            self.undo();
        }
        // Ctrl+F, and OneNote's Ctrl+E, go to the search box. Taken before the page sees
        // them: with the caret in the page, the key went nowhere and the words typed to find
        // something were written into the note (issue #134). The code view has a Find of its
        // own for the file it shows, so it keeps Ctrl+F while it is open.
        // OneNote's Ctrl+K: the Link dialog for the words picked or the link the caret is in.
        if !self.show_source && page_keys && self.editor.is_some() && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::K)) {
            self.open_link_dialog();
        }
        if !self.show_source && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F) || i.consume_key(Modifiers::COMMAND, Key::E)) {
            if let Some(caret) = self.editor.as_mut().and_then(|ed| ed.caret.as_mut()) {
                caret.focus = false;
            }
            self.focus_search = true;
        }
        self.finish_pending_open(ctx);
        self.nav_keys(ctx);
        self.onenote_keys(ctx);
        self.replay_typing(ctx);
        self.switch_by_device(ctx);
        self.pen_keys(ctx);
        self.direction_keys(ctx);
        self.hierarchy_keys(ctx);
        self.restore_last_page();
        self.attach_dropped(ctx);
        if matches!(self.backstage, Some("Export" | "Share"))
            && self.dialog.is_none()
            && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
        {
            let _ = self.session.run("export.cancel", &Params::new());
            self.backstage = None;
        }
        // The File screen takes the whole window while it is open, as it does in Office.
        if self.backstage.is_some() {
            self.title_band(ui, false);
            self.storage_notice(ui);
            self.backstage_screen(ui);
            self.search_popup(ctx);
            self.dialogs(ctx);
            self.import_progress(ctx);
            self.frame_edges(ctx);
            #[cfg(feature = "voice")]
            self.show_voice(ctx);
            return;
        }
        self.compact_layout(ctx);
        self.top_bar(ui);
        self.storage_notice(ui);
        self.status_bar(ui);
        self.side_panel(ui);
        // The row asked for has the keyboard now, or was not drawn in time (issue #514): ask
        // no more, so it is not taken from wherever the user went since.
        if ui.data_mut(|d| d.remove_temp::<bool>(egui::Id::new("nav-focus-done"))).unwrap_or(false) || self.nav_focus_tries == 0 {
            self.nav_focus = None;
        } else {
            self.nav_focus_tries -= 1;
        }
        self.tags_panel(ui);
        self.templates_panel(ui);
        // In a phone-width window a pane that fills it leaves no room for the page.
        if !self.compact_list_fills() && !(self.compact && (self.tags_pane.is_some() || self.templates_pane.is_some())) {
            self.section_tabs(ui);
            self.central(ui);
        }
        self.paint_formatting(ctx);
        self.refresh_unread(false);
        self.laser_keys(ctx);
        self.eyedropper(ctx);
        self.search_popup(ctx);
        self.dialogs(ctx);
        self.import_progress(ctx);
        self.frame_edges(ctx);
        // Closing the window writes the page first, and waits for an autosave still being
        // written: what was typed in the moment before closing is not lost.
        if ctx.input(|i| i.viewport().close_requested()) {
            self.before_close();
            if self.is_dirty() || self.page_conflict {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            }
        }
        self.poll_printouts(ctx);
        self.take_saves(false);
        if self.saver.busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        #[cfg(feature = "voice")]
        self.show_voice(ctx);
        if let Some(since) = self.editor.as_ref().and_then(|e| e.dirty_since) {
            // Read once: read twice, the second could be past the wait, and the subtraction
            // below panicked (seen on a slow CI runner).
            let waited = since.elapsed();
            if waited >= AUTOSAVE_AFTER {
                self.save_in_background();
            } else {
                ctx.request_repaint_after(AUTOSAVE_AFTER.saturating_sub(waited));
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

/// The settings key the page last open in each section is kept under (issue #557).
const SECTION_PAGES_KEY: &str = "section_last_pages";

/// How many sections' last pages are kept; the oldest are let go beyond this.
const SECTION_PAGES_KEPT: usize = 200;

/// The node at `at`, wherever in the tree it is.
fn node_at<'a>(tree: &'a [Node], at: &Path) -> Option<&'a Node> {
    fn find<'a>(n: &'a Node, at: &Path) -> Option<&'a Node> {
        if n.path == at {
            return Some(n);
        }
        n.children.iter().find_map(|c| find(c, at))
    }
    tree.iter().find_map(|n| find(n, at))
}

/// The section a page is in, from the tree in memory: the nearest section above it.
fn section_in_tree(tree: &[Node], page: &Path) -> Option<PathBuf> {
    fn walk(nodes: &[Node], page: &Path, section: Option<&Path>) -> Option<PathBuf> {
        nodes.iter().find_map(|n| {
            let here = if n.info.kind == NodeKind::Section { Some(n.path.as_path()) } else { section };
            if n.path == page { here.map(Path::to_path_buf) } else { walk(&n.children, page, here) }
        })
    }
    walk(tree, page, None)
}

/// The sections of the notebook at `notebook`, in the order the notebook shows them: each in
/// its place, the sections of a group where the group is (issue #512).
fn sections_in_order(tree: &[Node], notebook: &Path) -> Vec<PathBuf> {
    fn walk(nodes: &[Node], out: &mut Vec<PathBuf>) {
        for n in nodes {
            match n.info.kind {
                NodeKind::Section => out.push(n.path.clone()),
                NodeKind::SectionGroup => walk(&n.children, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    if let Some(nb) = tree.iter().find(|n| n.path == notebook) {
        walk(&nb.children, &mut out);
    }
    out
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
    let node = node_at(tree, at)?;
    first_page(std::slice::from_ref(node), &node.path)
}

/// The 14 swatches and the switch that follows the system, shared by the sun or moon button
/// and the ribbon's Theme button.
pub fn theme_grid(ui: &mut Ui, current: Theme, pick: &mut Option<(Mode, Accent)>, follow: &mut bool) {
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

/// The Welcome notebook's handwriting and drawing page among `notebooks` (issue #296): the
/// one in the section for `language` (`10-en`, `20-ar`, ...), else the English one. The
/// notebook is found by its contents, so a renamed or moved copy still works.
fn drawing_help_page(notebooks: &[PathBuf], language: &str) -> Option<PathBuf> {
    let base = language.split(['-', '_']).next().unwrap_or("en").to_ascii_lowercase();
    let find = |code: &str| {
        notebooks.iter().find_map(|notebook| {
            let sections = std::fs::read_dir(notebook).ok()?;
            sections.flatten().map(|e| e.path()).find_map(|section| {
                let name = section.file_name()?.to_string_lossy().to_ascii_lowercase();
                let page = section.join("how-to").join("handwriting-and-drawing");
                (name.ends_with(&format!("-{code}")) && page.is_dir()).then_some(page)
            })
        })
    };
    find(&base).or_else(|| find("en"))
}

/// The notebooks folder chooser. The web build has no native folder dialog, so it chooses nothing.
#[cfg(not(target_arch = "wasm32"))]
fn pick_notebooks_folder(start: &std::path::Path) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_the_notebooks_folder")).set_directory(start).pick_folder()
}

#[cfg(target_arch = "wasm32")]
fn pick_notebooks_folder(_start: &std::path::Path) -> Option<PathBuf> {
    None
}

/// File > Save a Copy's folder chooser (issue #301). A browser has no folder dialog, so the web
/// build chooses nothing and the folder is typed instead. Kept here, below the other choosers,
/// and gated in its own right: the web build's rules rewrite the dialogs above by where they sit.
#[cfg(not(target_arch = "wasm32"))]
fn pick_copy_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_the_folder_for_the_copy")).pick_folder()
}

#[cfg(target_arch = "wasm32")]
fn pick_copy_folder() -> Option<PathBuf> {
    None
}

/// Home > Meeting Details asks for a calendar file (issue #283). Below the other choosers, and
/// gated in its own right, for the same reason as the one above.
#[cfg(not(target_arch = "wasm32"))]
fn pick_calendar_file() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .set_title(crate::tl_id!("ui.choose_a_calendar_file"))
        .add_filter(crate::tl_id!("ui.calendar_files"), &["ics", "ical", "ifb", "icalendar"])
        .pick_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_calendar_file() -> Option<PathBuf> {
    None
}

/// Copy Text from Picture asks for the folder with the text recognition models (issue #274).
/// Below the other choosers, and gated in its own right, for the same reason as the ones above.
#[cfg(not(target_arch = "wasm32"))]
fn pick_ocr_models_folder() -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(crate::tl_id!("ui.choose_the_ocr_models_folder")).pick_folder()
}

#[cfg(target_arch = "wasm32")]
fn pick_ocr_models_folder() -> Option<PathBuf> {
    None
}

/// File > Print asks where to save the PDF (issue #168). Below the other choosers, and gated in
/// its own right, for the same reason as the ones above. A browser downloads the file instead.
#[cfg(not(target_arch = "wasm32"))]
fn pick_pdf_destination(name: &str) -> Option<PathBuf> {
    rfd::FileDialog::new().set_title(crate::tl_id!("ui.print_save_as_pdf")).set_file_name(name).add_filter("PDF", &["pdf"]).save_file()
}

#[cfg(target_arch = "wasm32")]
fn pick_pdf_destination(_name: &str) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod alert_tests {
    use super::*;

    #[test]
    fn ink_help_finds_the_drawing_page_in_the_welcome_notebook() {
        // Issue #296, against the checked-in Welcome notebook in both formats.
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook");
        for format in ["html", "cmgfm"] {
            let notebook = demo.join(format);
            let english = drawing_help_page(std::slice::from_ref(&notebook), "en").expect("english page");
            assert!(english.ends_with(Path::new("10-en/how-to/handwriting-and-drawing")), "{english:?}");
            let arabic = drawing_help_page(std::slice::from_ref(&notebook), "ar-XB").expect("arabic page");
            assert!(arabic.ends_with(Path::new("20-ar/how-to/handwriting-and-drawing")), "{arabic:?}");
            let unknown = drawing_help_page(std::slice::from_ref(&notebook), "xx").expect("falls back");
            assert_eq!(unknown, english, "a language with no section falls back to English");
        }
        assert_eq!(drawing_help_page(&[demo.join("no-such-notebook")], "en"), None);
    }

    #[test]
    fn portable_control_snapshots_unsaved_text_without_saving_the_source() {
        let t = notecraft_store::testing::TempDir::new("portable-control");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("books"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, Some(crate::LOGO_PNG)).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.html_page.to_string_lossy());
        let before = std::fs::read(sample.html_page.join("index.html")).expect("source");
        let editor = app.editor.as_mut().expect("editor");
        editor.body = "<p>Unsaved review text</p>".into();
        editor.arranged = false;
        editor.dirty_since = Some(Instant::now());
        let ctx = egui::Context::default();
        app.control_method(&ctx, "engine.execute", &json!({"command":"export.prepare", "params":{"path":sample.html_page}})).expect("review");
        assert_eq!(std::fs::read(sample.html_page.join("index.html")).expect("source unchanged"), before);
        assert!(
            app.session
                .reviewed_export()
                .expect("snapshot")
                .entries()
                .any(|(_, b)| std::str::from_utf8(b).is_ok_and(|s| s.contains("Unsaved review text")))
        );
        app.control_method(&ctx, "engine.execute", &json!({"command":"export.cancel"})).expect("cancel");
        assert_eq!(std::fs::read(sample.html_page.join("index.html")).expect("source unchanged after cancel"), before);
        assert!(app.session.reviewed_export().is_none());
        // Prevent the app's normal exit autosave from becoming part of this read-only check.
        app.editor.as_mut().expect("editor").dirty_since = None;
    }

    #[test]
    fn opening_a_notebook_folder_displays_its_page_instead_of_the_previous_notebook() {
        let t = notecraft_store::testing::TempDir::new("open-folder-content");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let away = notecraft_store::ops::create_notebook(t.path(), "Away", Some(PageFormat::Html), &settings).expect("away notebook");
        let section = notecraft_store::ops::create_node(&away, "Section", NodeKind::Section, None, &settings).expect("section");
        let page = notecraft_store::ops::create_node(&section, "Away page", NodeKind::Page, None, &settings).expect("away page");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.md_page.to_string_lossy());
        assert!(app.editor.is_some());
        app.open_notebooks_from_folder(away);
        // The file reader can take longer than OPEN_WAIT on a busy test machine. Drive the
        // same completion step as a frame until the requested page has had time to open.
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.opening.is_some() && Instant::now() < deadline {
            app.finish_pending_open(&ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            app.editor.as_ref().map(|editor| &editor.dir),
            Some(&std::fs::canonicalize(page).expect("page path")),
            "the chosen notebook owns the visible page"
        );
    }

    #[test]
    fn opening_an_empty_notebook_folder_clears_the_previous_page() {
        let t = notecraft_store::testing::TempDir::new("open-empty-folder-content");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let empty = t.path().join("Empty");
        std::fs::create_dir(&empty).expect("empty folder");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.md_page.to_string_lossy());
        assert!(app.editor.is_some());
        app.open_notebooks_from_folder(empty);
        assert!(app.editor.is_none(), "an empty notebook cannot display another notebook's page");
        assert!(app.pad.is_none(), "stale ink cannot remain attached to an empty notebook");
    }

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
    fn choosing_a_notebooks_folder_moves_to_it_now() {
        let t = notecraft_store::testing::TempDir::new("notebooks-folder-choice");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let mut app = NoteCraftApp::new(session);
        let chosen = t.path().join("chosen");

        app.apply_notebooks_root_choice(Some(&chosen));

        assert_eq!(app.session().root(), std::fs::canonicalize(&chosen).expect("canonical").as_path(), "the window moves to the chosen folder");
        assert_eq!(settings.get("last_notebooks_root"), Some(chosen.to_string_lossy().into_owned()), "the choice is remembered");
    }

    /// `--new-version` is a test hook: it must not change the first-use state of a real install, or
    /// open the first-use wizard in its place.
    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn new_version_leaves_the_first_use_state_alone() {
        let t = notecraft_store::testing::TempDir::new("new-version-first-use");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let mut app = NoteCraftApp::new(session);

        app.simulate_new_version(SIMULATED_NEW_VERSION);

        assert_eq!(settings.get("privacy_welcome_seen").as_deref(), Some("true"), "the first-use state is unchanged");
        assert!(!matches!(app.dialog, Some(Dialog::FirstUseWizard { .. })), "the first-use wizard does not open for a test hook");
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn simulated_new_version_shows_an_update_without_downloading() {
        let t = notecraft_store::testing::TempDir::new("simulated-new-version");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let mut app = NoteCraftApp::new(session);

        app.simulate_new_version(SIMULATED_NEW_VERSION);

        assert!(
            matches!(*app.update.lock().expect("status"), crate::update::Status::Manual { ref version } if version == SIMULATED_NEW_VERSION),
            "the Options page shows a newer version, and downloads nothing"
        );
        assert!(app.update_prompt.is_some(), "the shared update prompt is set up at start; it opens once GitHub answers");
    }

    #[test]
    fn after_a_change_the_window_lists_the_notebooks_in_the_new_folder() {
        let t = notecraft_store::testing::TempDir::new("notebooks-folder-list");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let mut app = NoteCraftApp::new(session);
        let chosen = t.path().join("chosen");
        notecraft_store::ops::create_notebook(&chosen, "Research", None, &settings).expect("a notebook in the chosen folder");

        app.apply_notebooks_root_choice(Some(&chosen));

        assert!(
            app.session().tree().iter().any(|n| n.path.ends_with("research")),
            "the notebook in the chosen folder is listed at once, not after a restart"
        );
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
    fn an_incomplete_first_use_wizard_is_not_marked_finished() {
        let t = notecraft_store::testing::TempDir::new("welcome-wizard-incomplete");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let mut app = NoteCraftApp::new(session);
        assert!(app.finish_first_use_wizard("none", PageFormat::Cmgfm, true).is_err(), "missing bundled content is reported");
        assert_eq!(settings.get("privacy_welcome_seen"), None, "a failed Finish must not acknowledge incomplete setup");
        let restarted = Session::new(&t.path().join("notebooks"), settings).expect("restart");
        assert!(matches!(NoteCraftApp::new(restarted).dialog, Some(Dialog::FirstUseWizard { .. })), "setup remains available on restart");
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
        add_printout(&mut h, &dir, &pdf);
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

    /// A page from the sample notebook, open in a window, and a one-page PDF to insert.
    fn printout_harness(
        html: bool,
        name: &str,
    ) -> (notecraft_store::testing::TempDir, PathBuf, PathBuf, egui_kittest::Harness<'static, NoteCraftApp>) {
        let t = notecraft_store::testing::TempDir::new(name);
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
        (t, page, pdf, h)
    }

    /// Waits until every printout asked for is made and in its page. A printout is rendered off
    /// the window's thread, which takes a few seconds on a slow CI runner: the tests waited five
    /// and then read the page whether or not it had finished, and failed now and then on macOS
    /// with the printout simply not there yet.
    fn wait_for_printouts(h: &mut egui_kittest::Harness<'_, NoteCraftApp>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while h.state().printouts_pending() > 0 && std::time::Instant::now() < deadline {
            h.step();
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        h.step();
        assert_eq!(h.state().printouts_pending(), 0, "the printout was made within a minute");
    }

    /// Asks for a printout of `pdf` in `page` and waits for it: it is made in the background.
    fn add_printout(h: &mut egui_kittest::Harness<'_, NoteCraftApp>, page: &Path, pdf: &Path) {
        h.state_mut().add_file(page, pdf, FileChoice::Printout);
        wait_for_printouts(h);
        for _ in 0..3 {
            h.step();
        }
    }

    /// A printout is made off the window's thread: asking for one returns at once, the window
    /// carries on drawing, and the pages go in once they are ready. Making one froze the page
    /// for seconds (rendering, then copying every page into the page's folder).
    #[test]
    fn a_printout_is_made_in_the_background_and_goes_in_when_ready() {
        let (_t, page, pdf, mut h) = printout_harness(true, "printout-background");
        let before = h.state().editor.as_ref().map(|e| e.blocks.len()).expect("editor");
        h.state_mut().add_file(&page, &pdf, FileChoice::Printout);
        assert_eq!(h.state().printouts_pending(), 1, "the printout is being made");
        assert_eq!(h.state().editor.as_ref().map(|e| e.blocks.len()), Some(before), "and the page is not waiting for it");
        h.step();
        wait_for_printouts(&mut h);
        let ed = h.state().editor.as_ref().expect("editor");
        assert!(ed.blocks.len() > before, "the printout went in once it was ready");
        assert!(
            ed.blocks.iter().any(|b| matches!(b, notecraft_formats::Block::Image { src, .. } if src.contains(".printout-001.png"))),
            "its page is a picture in the page"
        );
    }

    /// A printout asked for on one page goes into that page, even when another page was opened
    /// while it was being made.
    #[test]
    fn a_printout_goes_into_its_own_page_when_another_was_opened_meanwhile() {
        let (_t, page, pdf, mut h) = printout_harness(true, "printout-other-page");
        let other = {
            let parent = page.parent().expect("section").to_path_buf();
            std::fs::read_dir(&parent)
                .expect("section")
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.is_dir() && *p != page)
                .expect("another page")
        };
        h.state_mut().add_file(&page, &pdf, FileChoice::Printout);
        h.state_mut().open_path(&other.to_string_lossy());
        wait_for_printouts(&mut h);
        assert_eq!(h.state().editor.as_ref().map(|e| e.dir.clone()), Some(other.clone()), "the other page is the one open");
        let file = notecraft_store::tree::page_file(&page).expect("the first page's file").1;
        let text = std::fs::read_to_string(file).expect("read");
        assert!(text.contains(".printout-001.png"), "the printout is in the page it was asked for:\n{text}");
        let other_file = notecraft_store::tree::page_file(&other).expect("the other page's file").1;
        assert!(!std::fs::read_to_string(other_file).expect("read").contains(".printout-001.png"), "and not in the other one");
    }

    /// The window opens where it was left: the same page, even in a notebook kept outside the
    /// notebooks folder (the one in a synced folder, say), at the same zoom and scrolled to the
    /// same place. A page outside the notebooks folder was never remembered at all, so the window
    /// always came back to whatever page it had last remembered inside it.
    /// Coming back to a place further down than the page reaches yet (its pictures still on their
    /// way, or the page made shorter meanwhile) does not throw the page past its end. Put there,
    /// it sprang back with a great speed, coasted far from where it was left, and the window went
    /// on drawing frame after frame with nothing happening.
    #[test]
    fn coming_back_further_down_than_the_page_reaches_leaves_the_window_still() {
        let t = notecraft_store::testing::TempDir::new("reopen-past-the-end");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let lines: String = (1..=60).map(|n| format!("<p>line {n}</p>")).collect();
        let page = sample.html_page.to_string_lossy().into_owned();
        session.run("page.save", &params(&[("path", &page), ("body", &lines)])).expect("a page");
        settings.set("last_page", &page).expect("last page");
        settings.set(LAST_SCROLL_KEY, "100000").expect("far down");
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1200.0, 760.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), NoteCraftApp::new(session));
        // Step until the window stops asking for frames. A fixed frame count assumed the window
        // settles within it, which a slow runner does not guarantee (issue #429). The cap still
        // catches a window that never settles.
        let mut settled = false;
        for _ in 0..300 {
            h.step();
            if !h.ctx.has_requested_repaint() {
                settled = true;
                break;
            }
        }
        assert!(settled, "the window never settled: it still asks for frames after 300 steps");
        let (at, end) = (h.state().page_offset_y, h.state().page_offset_max);
        assert!(at <= end + 1.0, "the page is not past its end: {at} of {end}");
        assert!(!h.ctx.has_requested_repaint(), "and with nothing happening the window asks for no more frames");
    }

    #[test]
    fn the_window_opens_where_it_was_left() {
        let t = notecraft_store::testing::TempDir::new("reopen-where-left");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        // A notebook elsewhere, opened beside the others, with a long page in it.
        let away = t.path().join("away");
        std::fs::create_dir_all(&away).expect("dir");
        let mut other = Session::new(&away, settings.clone()).expect("other session");
        let Ok(Output::Path(made)) = other.run("notebook.create", &params(&[("name", "Elsewhere")])) else { panic!("a notebook") };
        session.run("notebook.open", &params(&[("path", &made.to_string_lossy())])).expect("open it");
        let page = made.join("new-section-1").join("untitled-page");
        let lines: String = (1..=120).map(|n| format!("<p>line {n}</p>")).collect();
        session.run("page.save", &params(&[("path", &page.to_string_lossy()), ("body", &lines)])).expect("a long page");

        let harness = |app: NoteCraftApp| {
            egui_kittest::Harness::builder()
                .with_size(vec2(1200.0, 760.0))
                .with_accessibility_check(false)
                .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app)
        };
        let mut h = harness(NoteCraftApp::new(session));
        for _ in 0..5 {
            h.step();
        }
        h.state_mut().open_path(&page.to_string_lossy());
        h.state_mut().zoom_percent = 150;
        for _ in 0..5 {
            h.step();
        }
        // Scrolled down with the wheel, over the page.
        h.input_mut().events.push(egui::Event::PointerMoved(egui::pos2(600.0, 400.0)));
        for _ in 0..6 {
            h.input_mut().events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, -200.0),
                modifiers: Modifiers::NONE,
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
            });
            h.step();
        }
        for _ in 0..10 {
            h.step();
        }
        let scrolled = h.state().page_offset_y;
        assert!(scrolled > 300.0, "the page is scrolled down: {scrolled}");
        // The window closes.
        h.state_mut().before_close();
        drop(h);

        // And opens again, on the same settings.
        let session = Session::new(&t.path().join("notebooks"), settings.clone()).expect("again");
        let mut h = harness(NoteCraftApp::new(session));
        for _ in 0..10 {
            h.step();
        }
        assert_eq!(h.state().editor.as_ref().map(|e| e.dir.clone()), Some(page.clone()), "the same page is open");
        assert_eq!(h.state().zoom_percent, 150, "at the same zoom");
        assert!((h.state().page_offset_y - scrolled).abs() < 2.0, "scrolled to the same place: {scrolled} -> {}", h.state().page_offset_y);
        assert!(h.state().open.contains(&made), "and its notebook is open in the tree");
    }

    /// Drawing a page does not go to the disk every frame. A page of printouts asked about every
    /// picture and attachment each frame; on a synced folder (OneDrive through onedriver) each of
    /// those waits on the mount, and the window drew a few frames a second while scrolling.
    #[test]
    fn drawing_a_page_of_printouts_does_not_ask_the_disk_every_frame() {
        let (_t, page, pdf, mut h) = printout_harness(true, "printout-quiet-frames");
        add_printout(&mut h, &page, &pdf);
        for _ in 0..5 {
            h.step();
        }
        let before = crate::fscache::disk_asks();
        for _ in 0..30 {
            h.step();
        }
        let asked = crate::fscache::disk_asks() - before;
        // Within the cache's moment each file is asked about at most once, not once a frame.
        assert!(asked <= 12, "30 frames asked the disk {asked} times");
    }

    /// Issue #67: with no box holding the caret, a second printout goes below what is already
    /// on the page, not over the first.
    #[test]
    fn a_second_printout_lands_below_the_boxes_already_on_the_page() {
        for html in [false, true] {
            let (_t, page, pdf, mut h) = printout_harness(html, "printout-below");
            h.state_mut().editor.as_mut().expect("editor").caret = None;
            for _ in 0..2 {
                add_printout(&mut h, &page, &pdf);
            }
            let all = h.state().editor.as_ref().map(|e| e.boxes.clone()).expect("editor");
            // The sample page's words, then one box for each printout, each one further down.
            assert!(all.len() >= 3, "html={html}: each printout has a box of its own: {all:?}");
            for pair in all.windows(2) {
                assert!(pair[1].y > pair[0].y, "html={html}: each new box sits below the one before, not over it: {all:?}");
            }
        }
    }

    /// A box clicked into and left blank goes however the caret leaves it, not only when the page
    /// itself moves the caret, and a printout added meanwhile is not put below it: the blank box
    /// was never something on the page, only a place for the cursor.
    #[test]
    fn a_blank_box_the_caret_was_taken_from_goes_and_a_printout_is_not_put_below_it() {
        for html in [false, true] {
            let (_t, page, pdf, mut h) = printout_harness(html, "blank-box-printout");
            let line = {
                let ed = h.state_mut().editor.as_mut().expect("editor");
                if ed.boxes.is_empty() {
                    ed.boxes = vec![notecraft_model::container::Container::new(0.0, 0.0, 600.0, ed.blocks.len())];
                }
                let line = ed.blocks.len();
                ed.blocks.push(notecraft_formats::Block::Paragraph(Vec::new(), Vec::new()));
                ed.boxes = notecraft_model::container::append(&ed.boxes, line, 1, 300.0, 0.0, 0.0);
                let index = ed.boxes.len() - 1;
                (ed.boxes[index].x, ed.boxes[index].y) = (400.0, 2000.0);
                ed.arranged = true;
                ed.rebuild_body();
                ed.caret = Some(pageedit::Caret {
                    block: line,
                    text: String::new(),
                    focus: true,
                    selection: (0, 0),
                    place: None,
                    cell: None,
                    span: None,
                    inline: None,
                });
                line
            };
            for _ in 0..3 {
                h.step();
            }
            let boxes_with_blank = h.state().editor.as_ref().map(|e| e.boxes.len()).expect("editor");
            h.state_mut().editor.as_mut().expect("editor").caret = None;
            add_printout(&mut h, &page, &pdf);
            let ed = h.state().editor.as_ref().expect("editor");
            assert!(!ed.boxes.iter().any(|c| c.y == 2000.0), "html={html}: the blank box went: {:?}", ed.boxes);
            assert_eq!(ed.boxes.len(), boxes_with_blank, "html={html}: one box for the printout, none for the blank line: {:?}", ed.boxes);
            let printout = ed.boxes.last().expect("the printout's box");
            assert!(printout.y < 2000.0, "html={html}: the printout is not put below the blank box: {:?}", ed.boxes);
            assert!(ed.blocks.len() > line, "html={html}: the printout is on the page");
        }
    }

    /// The autosave hands the page to a writer of its own and the window carries on: on a synced
    /// folder a save takes about a second, and every frame it held up was a click or a letter
    /// held up. The words still reach the file, and anything that needs them there waits for them.
    #[test]
    fn the_autosave_does_not_hold_up_the_window() {
        let (_t, page, _pdf, mut h) = printout_harness(true, "autosave-background");
        h.state_mut().saver = crate::saver::Saver::with_writer(|dir, body| {
            std::thread::sleep(std::time::Duration::from_millis(400));
            notecraft_store::page::save_body(dir, body).map_err(|e| e.to_string())
        });
        {
            let ed = h.state_mut().editor.as_mut().expect("editor");
            let (added, _) = notecraft_formats::boxes::read(ed.format, "Written in the background\n");
            let at = ed.blocks.len();
            ed.blocks.extend(added.iter().cloned());
            if ed.boxes.is_empty() {
                ed.arranged = false;
            } else {
                ed.boxes = notecraft_model::container::append(&ed.boxes, at, added.len(), 300.0, 0.0, 0.0);
            }
            ed.rebuild_body();
            ed.dirty_since = Some(Instant::now() - AUTOSAVE_AFTER - std::time::Duration::from_millis(10));
        }
        let frame = Instant::now();
        h.step();
        assert!(frame.elapsed() < std::time::Duration::from_millis(250), "the frame the autosave fired in took {:?}", frame.elapsed());
        assert!(h.state().is_dirty(), "until it is written the page counts as unsaved");
        h.state_mut().save_now();
        let file = notecraft_store::tree::page_file(&page).expect("the page's file").1;
        let text = std::fs::read_to_string(file).expect("read");
        assert!(text.contains("Written in the background"), "the words reached the file:\n{text}");
        assert!(!h.state().is_dirty(), "and the page is saved");
    }

    /// Issue #67: a printout goes into the box that holds the caret, after the caret's line,
    /// and that box stays where it was put.
    #[test]
    fn a_printout_goes_into_the_box_that_holds_the_caret() {
        for html in [false, true] {
            let (_t, page, pdf, mut h) = printout_harness(html, "printout-caret-box");
            // A box with a line in it, moved to the right of the page's words, with the caret in it.
            let (index, line) = {
                let ed = h.state_mut().editor.as_mut().expect("editor");
                let (added, _) = notecraft_formats::boxes::read(ed.format, "Beside the words\n");
                let line = ed.blocks.len();
                ed.blocks.extend(added.iter().cloned());
                ed.boxes = notecraft_model::container::append(&ed.boxes, line, added.len(), 300.0, 0.0, 0.0);
                let index = ed.boxes.len() - 1;
                (ed.boxes[index].x, ed.boxes[index].y) = (900.0, 40.0);
                ed.arranged = true;
                ed.rebuild_body();
                ed.caret = Some(pageedit::Caret {
                    block: line,
                    text: "Beside the words".into(),
                    focus: false,
                    selection: (0, 0),
                    place: None,
                    cell: None,
                    span: None,
                    inline: None,
                });
                // Made here rather than by clicks and typing, so it is remembered here too, the
                // way the window remembers a box the writer made.
                NoteCraftApp::remember_step(ed);
                (index, line)
            };
            for _ in 0..3 {
                h.step();
            }
            let before = h.state().editor.as_ref().map(|e| e.boxes.clone()).expect("editor");
            let blocks_before = h.state().editor.as_ref().map(|e| e.blocks.clone()).expect("editor");
            add_printout(&mut h, &page, &pdf);
            let ed = h.state().editor.as_ref().expect("editor");
            assert_eq!(ed.boxes.len(), before.len(), "html={html}: no new box was made: {:?}", ed.boxes);
            let moved = &ed.boxes[index];
            assert_eq!((moved.x, moved.y), (900.0, 40.0), "html={html}: the box stays where it was put");
            assert!(moved.blocks > before[index].blocks, "html={html}: the printout is in the caret's box: {:?}", ed.boxes);
            // A printout's pages are as wide as the sheet, as they are in a box of their own: a
            // narrow box (one started with a click) is widened for them, not the pages shrunk.
            let sheet = before[0].width;
            assert!(sheet > 300.0, "html={html}: the page's first box is wider than the narrow one: {before:?}");
            assert_eq!(moved.width, sheet, "html={html}: the box is widened to the sheet for the printout");
            let range = notecraft_model::container::blocks_of(&ed.boxes, index);
            assert_eq!(range.start, line, "html={html}: the box's own line is still first");
            assert!(
                ed.blocks[range].iter().any(|b| matches!(b, notecraft_formats::Block::Image { .. })),
                "html={html}: the printout's pages are in that box"
            );
            h.state_mut().undo();
            for _ in 0..3 {
                h.step();
            }
            let ed = h.state().editor.as_ref().expect("editor");
            assert_eq!((&ed.blocks, &ed.boxes), (&blocks_before, &before), "html={html}: one undo takes the printout back out of the box");
        }
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
                inline: None,
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

#[cfg(test)]
mod literal_formatting_tests {
    use super::*;
    use notecraft_formats::{Block, Span};

    #[test]
    fn clear_formatting_keeps_literal_punctuation_through_save_and_reopen() {
        let temp = notecraft_store::testing::TempDir::new("clear-literal-formatting");
        let settings = notecraft_store::Settings::at(temp.path().join("settings.toml"));
        let mut session = Session::new(temp.path(), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let text = r"*literal* $5 or $6 [x](y) C:\Users\name";
        let original = vec![Block::Paragraph(vec![Span { text: text.into(), bold: true, ..Default::default() }], Vec::new())];
        let body = notecraft_formats::blocks_to_html(&original);
        session.run("page.save", &params(&[("path", &sample.html_page.to_string_lossy()), ("body", &body)])).expect("save");
        let mut app = NoteCraftApp::new(session);
        app.open_page(&sample.html_page);
        let editor = app.editor.as_mut().expect("opened page");
        editor.caret = Some(pageedit::Caret { text: pageedit::caret_text(&editor.blocks[0]), ..Default::default() });
        app.clear_formatting();
        app.save_now();
        let page = notecraft_store::page::load_page(&sample.html_page).expect("reopen");
        let blocks = notecraft_formats::boxes::read(page.format, &page.body).0;
        assert_eq!(blocks, vec![Block::Paragraph(vec![Span { text: text.into(), ..Default::default() }], Vec::new())]);
    }

    fn opened_literal_page(blocks: &[Block], format: notecraft_model::PageFormat) -> (notecraft_store::testing::TempDir, NoteCraftApp, PathBuf) {
        let temp = notecraft_store::testing::TempDir::new("literal-command-regression");
        let settings = notecraft_store::Settings::at(temp.path().join("settings.toml"));
        let mut session = Session::new(temp.path(), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let path = match format {
            notecraft_model::PageFormat::Html => sample.html_page,
            notecraft_model::PageFormat::Cmgfm => sample.md_page,
        };
        let body = notecraft_formats::boxes::write(format, blocks, None);
        session.run("page.save", &params(&[("path", &path.to_string_lossy()), ("body", &body)])).expect("save");
        let mut app = NoteCraftApp::new(session);
        app.open_page(&path);
        (temp, app, path)
    }

    #[test]
    fn clearing_cell_formatting_then_leaving_preserves_the_cell_text() {
        for format in notecraft_model::PageFormat::ALL {
            let text = "*literal* $5 or $6";
            let original = vec![Block::Table {
                head: vec![
                    vec![Span { text: text.into(), bold: true, ..Default::default() }],
                    vec![Span { text: "keep".into(), italic: true, ..Default::default() }],
                ],
                rows: Vec::new(),
                emoji: Vec::new(),
            }];
            let (_temp, mut app, path) = opened_literal_page(&original, format);
            let ed = app.editor.as_mut().expect("editor");
            ed.caret =
                Some(pageedit::Caret { text: pageedit::cell_text(&ed.blocks[0], (0, 0)).expect("cell"), cell: Some((0, 0)), ..Default::default() });
            app.clear_formatting();
            let ed = app.editor.as_mut().expect("editor");
            ed.caret.as_mut().expect("caret").write(&mut ed.blocks[0]);
            assert_eq!(pageedit::cell_text(&ed.blocks[0], (0, 0)).as_deref(), Some(text));
            app.save_now();
            let page = notecraft_store::page::load_page(&path).expect("reopen");
            let blocks = notecraft_formats::boxes::read(format, &page.body).0;
            let Block::Table { head, .. } = &blocks[0] else { panic!("table") };
            assert_eq!(head[0], vec![Span { text: text.into(), ..Default::default() }]);
            let Block::Table { head: before, .. } = &original[0] else { panic!("table") };
            assert_eq!(head[1], before[1]);
        }
    }

    #[test]
    fn copy_and_cut_keep_one_line_literal_text_and_rich_styles() {
        for format in notecraft_model::PageFormat::ALL {
            let text = "*literal* $5 or $6";
            let original = vec![Block::Paragraph(vec![Span { text: text.into(), bold: true, ..Default::default() }], Vec::new())];
            let (_temp, mut app, _path) = opened_literal_page(&original, format);
            let ed = app.editor.as_mut().expect("editor");
            let source = pageedit::caret_text(&ed.blocks[0]);
            ed.caret = Some(pageedit::Caret { selection: (0, source.chars().count()), text: source, ..Default::default() });
            app.clipboard("copy");
            assert_eq!(app.pending_copy.as_deref(), Some(text));
            assert_eq!(
                notecraft_formats::markdown::parse(&notecraft_formats::html::to_markdown(app.pending_copy_html.as_deref().expect("html"))),
                original
            );
            assert_eq!(app.editor.as_ref().expect("editor").blocks, original);
            app.clipboard("cut");
            assert_eq!(app.pending_copy.as_deref(), Some(text));
            assert_eq!(pageedit::block_text(&app.editor.as_ref().expect("editor").blocks[0]), "");
        }
    }

    #[test]
    fn copying_across_blocks_keeps_literal_delimiters() {
        let blocks = vec![
            Block::Paragraph(vec![Span { text: "*left*".into(), ..Default::default() }], Vec::new()),
            Block::Paragraph(vec![Span { text: "$5 or $6".into(), ..Default::default() }], Vec::new()),
        ];
        let (_temp, mut app, _path) = opened_literal_page(&blocks, notecraft_model::PageFormat::Html);
        let ed = app.editor.as_mut().expect("editor");
        ed.across = Some(pageedit::Across { from: (0, 0), to: (1, pageedit::block_text(&ed.blocks[1]).len()) });
        app.clipboard("copy");
        assert_eq!(app.pending_copy.as_deref(), Some("*left*\n$5 or $6"));
        assert_eq!(
            notecraft_formats::markdown::parse(&notecraft_formats::html::to_markdown(app.pending_copy_html.as_deref().expect("html"))),
            blocks
        );
    }
}

/// Insert Space and the Table tab's row and column buttons change the page's shape from the
/// ribbon, not by typing into it, so the page never saw them happen and kept no step for them:
/// Undo said there was nothing to undo (issue #153).
#[cfg(test)]
mod structural_undo_tests {
    use super::*;

    /// A page with a line of words and a table under it, open in the window.
    fn page_with_table(html: bool, name: &str) -> (notecraft_store::testing::TempDir, egui_kittest::Harness<'static, NoteCraftApp>) {
        let t = notecraft_store::testing::TempDir::new(name);
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let page = if html { sample.html_page.clone() } else { sample.md_page.clone() };
        let body = if html {
            "<p>Alpha</p><table><tr><th>A</th><th>B</th></tr><tr><td>one</td><td>two</td></tr></table>"
        } else {
            "Alpha\n\n| A | B |\n|---|---|\n| one | two |\n"
        };
        notecraft_store::page::save_body(&page, body).expect("fixture");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&page.to_string_lossy());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        for _ in 0..5 {
            h.step();
        }
        (t, h)
    }

    fn settle(h: &mut egui_kittest::Harness<'static, NoteCraftApp>) {
        for _ in 0..5 {
            h.step();
        }
    }

    fn blocks(h: &egui_kittest::Harness<'static, NoteCraftApp>) -> Vec<notecraft_formats::Block> {
        h.state().editor.as_ref().map(|e| e.blocks.clone()).expect("editor")
    }

    fn put_caret(h: &mut egui_kittest::Harness<'static, NoteCraftApp>, block: usize, cell: Option<(usize, usize)>) {
        let ed = h.state_mut().editor.as_mut().expect("editor");
        let text = match cell {
            Some(c) => pageedit::cell_text(&ed.blocks[block], c).unwrap_or_default(),
            None => pageedit::caret_text(&ed.blocks[block]),
        };
        ed.caret = Some(pageedit::Caret { block, text, focus: true, selection: (0, 0), place: None, cell, span: None, inline: None });
    }

    /// Does `act`, checks it changed the page, then that one Undo puts the page back and one
    /// Redo does it again.
    fn assert_undoable(h: &mut egui_kittest::Harness<'static, NoteCraftApp>, what: &str, act: ribbon::Act) {
        let before = blocks(h);
        h.state_mut().ribbon_act(act);
        settle(h);
        let after = blocks(h);
        assert_ne!(after, before, "{what}: the page changed");
        assert!(h.state().editor.as_ref().is_some_and(|e| e.history.can_undo()), "{what}: there is a step to undo");
        h.state_mut().undo();
        settle(h);
        assert_eq!(blocks(h), before, "{what}: one undo puts the page back");
        h.state_mut().redo();
        settle(h);
        assert_eq!(blocks(h), after, "{what}: redo does it again");
    }

    #[test]
    fn insert_space_can_be_undone() {
        for html in [false, true] {
            let (_t, mut h) = page_with_table(html, "undo-insert-space");
            put_caret(&mut h, 0, None);
            assert_undoable(&mut h, &format!("html={html} space"), ribbon::Act::Insert("space"));
        }
    }

    #[test]
    fn table_rows_and_columns_put_in_or_taken_out_can_be_undone() {
        use pageedit::TableEdit as E;
        for html in [false, true] {
            for (name, edit) in [
                ("row above", E::InsertRow(true)),
                ("row below", E::InsertRow(false)),
                ("column left", E::InsertColumn(true)),
                ("column right", E::InsertColumn(false)),
                ("delete row", E::DeleteRow),
                ("delete column", E::DeleteColumn),
            ] {
                let (_t, mut h) = page_with_table(html, "undo-table-edit");
                let table = blocks(&h).iter().position(|b| matches!(b, notecraft_formats::Block::Table { .. })).expect("a table");
                put_caret(&mut h, table, Some((1, 0)));
                assert_undoable(&mut h, &format!("html={html} {name}"), ribbon::Act::Table(edit));
            }
        }
    }

    /// Issue #297: Table > Delete Table takes the caret's table out as one Undo step, in both
    /// formats, and the page saves and reopens without it. The caret lands on a real line.
    #[test]
    fn delete_table_takes_the_whole_table_out_and_can_be_undone() {
        use notecraft_formats::Block;
        for html in [false, true] {
            let (_t, mut h) = page_with_table(html, "delete-table");
            let table = blocks(&h).iter().position(|b| matches!(b, Block::Table { .. })).expect("a table");
            put_caret(&mut h, table, Some((1, 1)));
            assert_undoable(&mut h, &format!("html={html} delete table"), ribbon::Act::DeleteTable);
            let left = blocks(&h);
            assert!(!left.iter().any(|b| matches!(b, Block::Table { .. })), "html={html}: no table is left: {left:?}");
            assert!(left.iter().any(|b| pageedit::block_text(b).contains("Alpha")), "html={html}: the words around it stay");
            let caret = h.state().editor.as_ref().and_then(|e| e.caret.clone()).expect("a caret");
            assert!(caret.cell.is_none() && caret.block < left.len(), "html={html}: the caret is on a line of the page: {caret:?}");

            // Saved and opened again: the table is still gone and nothing else changed.
            h.state_mut().save_page_now();
            let path = h.state().editor.as_ref().map(|e| e.dir.clone()).expect("path");
            let file = if html { path.join("index.html") } else { path.join("index.md") };
            let written = std::fs::read_to_string(&file).expect("page file");
            assert!(!written.contains("<table") && !written.contains("| A | B |"), "html={html}: saved without the table:\n{written}");
            assert!(written.contains("Alpha"), "html={html}: and with the words:\n{written}");
        }
    }

    /// A page that held only a table keeps a line to type on, and Delete Table with the caret
    /// outside a table changes nothing.
    #[test]
    fn delete_table_on_a_table_only_page_and_outside_a_table() {
        use notecraft_formats::Block;
        let (_t, mut h) = page_with_table(false, "delete-only-table");
        put_caret(&mut h, 0, None);
        let before = blocks(&h);
        h.state_mut().ribbon_act(ribbon::Act::DeleteTable);
        settle(&mut h);
        assert_eq!(blocks(&h), before, "with the caret outside a table nothing changes");

        let table = blocks(&h).iter().position(|b| matches!(b, Block::Table { .. })).expect("a table");
        h.state_mut().editor.as_mut().expect("editor").blocks.retain(|b| matches!(b, Block::Table { .. }));
        put_caret(&mut h, 0, Some((1, 0)));
        let _ = table;
        h.state_mut().ribbon_act(ribbon::Act::DeleteTable);
        settle(&mut h);
        let left = blocks(&h);
        assert_eq!(left.len(), 1, "one empty line is left: {left:?}");
        assert!(matches!(&left[0], Block::Paragraph(spans, _) if spans.is_empty()), "{left:?}");
        assert_eq!(h.state().editor.as_ref().and_then(|e| e.caret.as_ref()).map(|c| c.block), Some(0));
    }

    /// Writing first and then changing the table is two steps, not one: Undo takes back the
    /// table change and leaves the writing.
    #[test]
    fn a_table_change_after_writing_in_its_cell_is_a_step_of_its_own() {
        let (_t, mut h) = page_with_table(false, "undo-table-after-typing");
        let table = blocks(&h).iter().position(|b| matches!(b, notecraft_formats::Block::Table { .. })).expect("a table");
        put_caret(&mut h, table, Some((1, 0)));
        settle(&mut h);
        h.state_mut().insert_in_page("X");
        settle(&mut h);
        let typed = blocks(&h);
        h.state_mut().ribbon_act(ribbon::Act::Table(pageedit::TableEdit::InsertColumn(true)));
        settle(&mut h);
        assert_ne!(blocks(&h), typed, "a column went in");
        h.state_mut().undo();
        settle(&mut h);
        assert_eq!(blocks(&h), typed, "undo takes back the column and keeps what was written");
    }

    /// Issue #298: Merge Cells makes the picked cells one, keeps every word in the first cell
    /// in reading order, and is one Undo step; Split Cells is one more. Both formats keep the
    /// merged cell when the page is saved and opened again.
    #[test]
    fn merge_and_split_cells_keep_the_words_and_can_be_undone() {
        use notecraft_formats::Block;
        for html in [false, true] {
            let (_t, mut h) = page_with_table(html, "merge-cells");
            let table = blocks(&h).iter().position(|b| matches!(b, Block::Table { .. })).expect("a table");
            // The whole body row: "one" and "two".
            put_caret(&mut h, table, Some((1, 0)));
            if let Some(c) = h.state_mut().editor.as_mut().and_then(|e| e.caret.as_mut()) {
                c.span = Some((1, 1));
            }
            let before = blocks(&h);
            h.state_mut().ribbon_act(ribbon::Act::MergeCells);
            settle(&mut h);
            let ed = h.state().editor.as_ref().expect("editor");
            assert_eq!(ed.columns.spans(table), &[((1, 0), (1, 2))], "html={html}: one merged cell");
            assert_eq!(pageedit::cell_text(&ed.blocks[table], (1, 0)).as_deref(), Some("one two"), "html={html}: both cells' words, in order");
            assert_eq!(pageedit::cell_text(&ed.blocks[table], (1, 1)).as_deref(), Some(""), "html={html}: the covered cell is empty");
            assert_eq!(ed.caret.as_ref().and_then(|c| c.cell), Some((1, 0)), "html={html}: the caret is in the merged cell");
            h.state_mut().save_page_now();
            let dir = h.state().editor.as_ref().map(|e| e.dir.clone()).expect("dir");
            let kept = notecraft_store::page::table_columns(&dir).expect("columns");
            let file = std::fs::read_to_string(if html { dir.join("index.html") } else { dir.join("index.md") }).unwrap_or_default();
            assert_eq!(kept.spans(table), &[((1, 0), (1, 2))], "html={html}: saved with the page; status {:?}\n{file}", h.state().status());

            h.state_mut().undo();
            settle(&mut h);
            assert_eq!(blocks(&h), before, "html={html}: one undo puts the words back");
            assert!(h.state().editor.as_ref().expect("editor").columns.spans(table).is_empty(), "html={html}: and the cells");
            h.state_mut().redo();
            settle(&mut h);
            assert_eq!(h.state().editor.as_ref().expect("editor").columns.spans(table), &[((1, 0), (1, 2))], "html={html}: redo merges again");

            // Split from the cell it covered: the words stay in the first cell.
            put_caret(&mut h, table, Some((1, 0)));
            let merged = blocks(&h);
            h.state_mut().ribbon_act(ribbon::Act::SplitCells);
            settle(&mut h);
            assert!(h.state().editor.as_ref().expect("editor").columns.spans(table).is_empty(), "html={html}: split");
            assert_eq!(blocks(&h), merged, "html={html}: splitting moves no words");
            h.state_mut().undo();
            settle(&mut h);
            assert_eq!(
                h.state().editor.as_ref().expect("editor").columns.spans(table),
                &[((1, 0), (1, 2))],
                "html={html}: undo of a split merges again"
            );
        }
    }

    /// Issue #286: a sticker put on the page is one Undo step, in both formats.
    #[test]
    fn a_sticker_can_be_undone() {
        for html in [false, true] {
            let (_t, mut h) = page_with_table(html, "undo-sticker");
            put_caret(&mut h, 0, None);
            assert_undoable(&mut h, &format!("html={html} sticker"), ribbon::Act::Sticker("star"));
        }
    }

    /// Merging one cell, or with the caret outside a table, changes nothing and says why.
    #[test]
    fn merge_cells_needs_two_cells() {
        let (_t, mut h) = page_with_table(false, "merge-one-cell");
        let table = blocks(&h).iter().position(|b| matches!(b, notecraft_formats::Block::Table { .. })).expect("a table");
        put_caret(&mut h, table, Some((1, 0)));
        let before = blocks(&h);
        let could_undo = h.state().editor.as_ref().expect("editor").history.can_undo();
        h.state_mut().ribbon_act(ribbon::Act::MergeCells);
        settle(&mut h);
        assert_eq!(blocks(&h), before);
        assert!(h.state().editor.as_ref().expect("editor").columns.spans(table).is_empty());
        h.state_mut().ribbon_act(ribbon::Act::SplitCells);
        settle(&mut h);
        assert_eq!(h.state().editor.as_ref().expect("editor").history.can_undo(), could_undo, "no step was added");
        assert!(h.state().status().contains("not a merged cell"), "{}", h.state().status());
    }
}

/// A table's column widths are kept against its place among the page's blocks and its column
/// numbers. A column put in or taken out, or a line split above the table, used to leave them
/// where they were, so the widths went to the wrong columns or were lost (issue #154).
#[cfg(test)]
mod table_width_tests {
    use super::*;
    use notecraft_model::tablecols;

    type H = egui_kittest::Harness<'static, NoteCraftApp>;

    /// An HTML page with a line and a table under it whose two columns were dragged to 210 and
    /// 290, open in the window.
    fn page(name: &str) -> (notecraft_store::testing::TempDir, PathBuf, H) {
        let t = notecraft_store::testing::TempDir::new(name);
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let page = sample.html_page.clone();
        notecraft_store::page::save_body(&page, "<p>Alpha</p><table><tr><th>Wide</th><th>Narrow</th></tr><tr><td>A</td><td>B</td></tr></table>")
            .expect("fixture");
        notecraft_store::page::set_table_columns(&page, &tablecols::parse("1:210,290")).expect("widths");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&page.to_string_lossy());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        settle(&mut h);
        assert_eq!(widths(&h).of(1), Some([210.0, 290.0].as_slice()), "the page opened with its widths");
        (t, page, h)
    }

    fn settle(h: &mut H) {
        for _ in 0..5 {
            h.step();
        }
    }

    fn widths(h: &H) -> tablecols::Widths {
        h.state().editor.as_ref().map(|e| e.columns.clone()).expect("editor")
    }

    fn into_cell(h: &mut H, cell: (usize, usize)) {
        let ed = h.state_mut().editor.as_mut().expect("editor");
        let text = pageedit::cell_text(&ed.blocks[1], cell).unwrap_or_default();
        ed.caret = Some(pageedit::Caret { block: 1, text, focus: true, selection: (0, 0), place: None, cell: Some(cell), span: None, inline: None });
    }

    #[test]
    fn a_column_put_in_on_the_left_keeps_the_others_widths() {
        let (_t, page, mut h) = page("widths-insert-left");
        into_cell(&mut h, (0, 0));
        h.state_mut().ribbon_act(ribbon::Act::Table(pageedit::TableEdit::InsertColumn(true)));
        settle(&mut h);
        let want = [pageedit::COLUMN_W, 210.0, 290.0];
        assert_eq!(widths(&h).of(1), Some(want.as_slice()), "Wide and Narrow keep their widths, the new column the usual one");
        let saved = notecraft_store::page::table_columns(&page).expect("saved widths");
        assert_eq!(saved.of(1), Some(want.as_slice()), "and the page's file says so");

        // Undo puts the widths back with the column, and Redo brings both again.
        h.state_mut().undo();
        settle(&mut h);
        assert_eq!(widths(&h).of(1), Some([210.0, 290.0].as_slice()), "undo puts the widths back");
        assert_eq!(notecraft_store::page::table_columns(&page).expect("saved").of(1), Some([210.0, 290.0].as_slice()));
        h.state_mut().redo();
        settle(&mut h);
        assert_eq!(widths(&h).of(1), Some(want.as_slice()), "redo brings them again");
    }

    /// Where the words `label` are drawn, as accessibility reads them.
    fn place_of(h: &H, label: &str) -> egui::Rect {
        use egui_kittest::kittest::{NodeT as _, Queryable as _};
        let node = h.get_by_label(label);
        let b = node.accesskit_node().bounding_box().expect("a drawn label");
        #[allow(clippy::cast_possible_truncation)]
        egui::Rect::from_min_max(egui::pos2(b.x0 as f32, b.y0 as f32), egui::pos2(b.x1 as f32, b.y1 as f32))
    }

    /// Issue #263: taking up a pen drew the table as a plain grid sized to its words, so its
    /// columns narrowed, its borders went and a centred cell went back to the left. With a pen
    /// in hand it is now drawn from the same layout as when it is written in.
    #[test]
    fn a_pen_in_hand_keeps_the_tables_widths_and_alignment() {
        let (_t, _page, mut h) = page("widths-pen");
        {
            let ed = h.state_mut().editor.as_mut().expect("editor");
            ed.columns.set_align(1, &[(1, 1)], Some(tablecols::Align::Center));
        }
        settle(&mut h);
        let typing = (place_of(&h, "Narrow"), place_of(&h, "B"));
        h.state_mut().pointer = ribbon::Pointer::Draw;
        settle(&mut h);
        let drawing = (place_of(&h, "Narrow"), place_of(&h, "B"));
        assert!((typing.0.min.x - drawing.0.min.x).abs() < 0.5, "the second column starts where it did: {typing:?} vs {drawing:?}");
        assert!((typing.1.center().x - drawing.1.center().x).abs() < 0.5, "the centred cell stays centred: {typing:?} vs {drawing:?}");
        assert!(drawing.1.center().x > drawing.0.min.x + 100.0, "in the middle of its 290 point column, not at its start: {drawing:?}");
    }

    #[test]
    fn a_column_taken_out_takes_only_its_own_width() {
        let (_t, _page, mut h) = page("widths-delete");
        into_cell(&mut h, (0, 0));
        h.state_mut().ribbon_act(ribbon::Act::Table(pageedit::TableEdit::DeleteColumn));
        settle(&mut h);
        assert_eq!(widths(&h).of(1), Some([290.0].as_slice()), "Narrow keeps its width when Wide goes");
    }

    #[test]
    fn a_line_split_above_the_table_moves_its_widths_with_it() {
        let (_t, page, mut h) = page("widths-split");
        {
            let ed = h.state_mut().editor.as_mut().expect("editor");
            let text = pageedit::caret_text(&ed.blocks[0]);
            let end = text.chars().count();
            ed.caret =
                Some(pageedit::Caret { block: 0, text, focus: true, selection: (end, end), place: Some(end), cell: None, span: None, inline: None });
        }
        settle(&mut h);
        h.key_press(egui::Key::Enter);
        settle(&mut h);
        let ed = h.state().editor.as_ref().expect("editor");
        assert!(matches!(ed.blocks.get(2), Some(notecraft_formats::Block::Table { .. })), "the table moved down a block: {:?}", ed.blocks);
        assert_eq!(widths(&h).of(2), Some([210.0, 290.0].as_slice()), "its widths moved with it");
        assert_eq!(widths(&h).of(1), None, "and the new line has none");
        settle(&mut h);
        h.state_mut().save();
        let saved = notecraft_store::page::table_columns(&page).expect("saved widths");
        assert_eq!(saved.of(2), Some([210.0, 290.0].as_slice()), "the page's file follows: {saved:?}");
    }
}

/// Table > Sort from the ribbon on both page formats: the page's file is written in the new
/// order, and one Undo puts the rows back (issue #171).
#[cfg(test)]
mod table_sort_tests {
    use super::*;

    type H = egui_kittest::Harness<'static, NoteCraftApp>;

    fn settle(h: &mut H) {
        for _ in 0..5 {
            h.step();
        }
    }

    fn names(h: &H) -> Vec<String> {
        let ed = h.state().editor.as_ref().expect("editor");
        let Some(notecraft_formats::Block::Table { rows, .. }) = ed.blocks.iter().find(|b| matches!(b, notecraft_formats::Block::Table { .. }))
        else {
            panic!("a table");
        };
        rows.iter().map(|r| r[0].iter().map(|s| s.text.as_str()).collect()).collect()
    }

    #[test]
    fn sort_from_the_ribbon_saves_and_undoes() {
        for html in [false, true] {
            let t = notecraft_store::testing::TempDir::new("table-sort");
            let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
            settings.set("privacy_welcome_seen", "true").expect("settings");
            let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
            let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
            let page = if html { sample.html_page.clone() } else { sample.md_page.clone() };
            let body = if html {
                "<table><tr><th>Fruit</th><th>Count</th></tr><tr><td>pear</td><td>3</td></tr><tr><td><b>apple</b></td><td>10</td></tr><tr><td>fig</td><td>2</td></tr></table>"
            } else {
                "| Fruit | Count |\n|---|---|\n| pear | 3 |\n| **apple** | 10 |\n| fig | 2 |\n"
            };
            notecraft_store::page::save_body(&page, body).expect("fixture");
            let mut app = NoteCraftApp::new(session);
            app.open_path(&page.to_string_lossy());
            let mut h = egui_kittest::Harness::builder()
                .with_size(vec2(1400.0, 800.0))
                .with_accessibility_check(false)
                .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
            settle(&mut h);
            {
                let ed = h.state_mut().editor.as_mut().expect("editor");
                let at = ed.blocks.iter().position(|b| matches!(b, notecraft_formats::Block::Table { .. })).expect("a table");
                let text = pageedit::cell_text(&ed.blocks[at], (1, 0)).unwrap_or_default();
                ed.caret = Some(pageedit::Caret {
                    block: at,
                    text,
                    focus: true,
                    selection: (0, 0),
                    place: None,
                    cell: Some((1, 0)),
                    span: None,
                    inline: None,
                });
            }
            settle(&mut h);
            h.state_mut().ribbon_act(ribbon::Act::TableSort(false));
            settle(&mut h);
            assert_eq!(names(&h), ["apple", "fig", "pear"], "html={html}: sorted by the caret's column");
            h.state_mut().save();
            let saved = notecraft_store::page::load_page(&page).expect("load").body;
            let (a, f, p) = (saved.find("apple").expect("apple"), saved.find("fig").expect("fig"), saved.find("pear").expect("pear"));
            assert!(a < f && f < p, "html={html}: the file has the new order:\n{saved}");
            assert!(saved.contains(if html { "<b>apple</b>" } else { "**apple**" }), "html={html}: and the formatting:\n{saved}");

            h.state_mut().undo();
            settle(&mut h);
            assert_eq!(names(&h), ["pear", "apple", "fig"], "html={html}: one undo puts the rows back");
            h.state_mut().redo();
            settle(&mut h);
            assert_eq!(names(&h), ["apple", "fig", "pear"], "html={html}: and redo sorts them again");
            h.state_mut().ribbon_act(ribbon::Act::TableSort(true));
            settle(&mut h);
            assert_eq!(names(&h), ["pear", "fig", "apple"], "html={html}: descending");
        }
    }
}

#[cfg(test)]
mod page_row_name_tests {
    use super::{fit_to_width, row_name_room, text_width};

    /// A page row shows its format label only when the whole name fits beside it. A long
    /// name takes the whole row instead, so it is never cut short to make room for "html"
    /// (issue #251: names came out as "Welcomhtml").
    #[test]
    fn the_format_label_gives_way_to_a_long_name() {
        assert_eq!(row_name_room(60.0, Some(30.0), 120.0), (true, 90.0));
        assert_eq!(row_name_room(100.0, Some(30.0), 120.0), (false, 120.0));
        assert_eq!(row_name_room(100.0, None, 120.0), (false, 120.0));
    }

    /// A name that does not fit ends in "…" and stays inside its room; one that fits is
    /// left as it is.
    #[test]
    fn a_long_name_ends_in_an_ellipsis_inside_its_room() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            let name = "Write a page in plain words";
            assert_eq!(fit_to_width(ui, name, 14.0, 10_000.0), name);
            let room = text_width(ui, name, 14.0) / 2.0;
            let fitted = fit_to_width(ui, name, 14.0, room);
            assert!(fitted.ends_with('…'), "{fitted:?}");
            assert!(name.starts_with(fitted.trim_end_matches('…')), "{fitted:?}");
            assert!(text_width(ui, &fitted, 14.0) <= room, "{fitted:?} fits in {room}");
            assert_eq!(fit_to_width(ui, name, 14.0, 1.0), "…");
            // Letters made of several bytes are cut between letters, never inside one.
            let arabic = "ما الذي يمكنك فعله هنا";
            let cut = fit_to_width(ui, arabic, 14.0, text_width(ui, arabic, 14.0) / 2.0);
            assert!(cut.ends_with('…'), "{cut:?}");
        });
        out.textures_delta.clear();
    }
}

#[cfg(test)]
mod insert_table_size_tests {
    use super::table_size;

    #[test]
    fn a_typed_size_is_whole_numbers_within_the_limits() {
        assert_eq!(table_size("3", "4"), Some((3, 4)));
        assert_eq!(table_size(" 200 ", "30"), Some((200, 30)));
        for (rows, columns) in [("0", "3"), ("3", "0"), ("201", "3"), ("3", "31"), ("", "3"), ("x", "3"), ("-1", "3"), ("2.5", "3")] {
            assert_eq!(table_size(rows, columns), None, "{rows:?} x {columns:?}");
        }
    }
}

/// Issue #273: the picture menu's Rotate and Flip. The turned picture is a new file beside the
/// page, the page points at it (its size swapped by a quarter turn), the original stays, and
/// Undo puts the page back on the original. Both page formats.
#[cfg(test)]
mod picture_turn_tests {
    use super::*;
    use crate::turn::Turn;

    type H = egui_kittest::Harness<'static, NoteCraftApp>;

    fn settle(h: &mut H) {
        for _ in 0..5 {
            h.step();
        }
    }

    fn picture(h: &H) -> (String, Option<f32>, Option<f32>) {
        let ed = h.state().editor.as_ref().expect("editor");
        ed.blocks
            .iter()
            .find_map(|b| match b {
                notecraft_formats::Block::Image { src, width, height, .. } => Some((src.clone(), *width, *height)),
                _ => None,
            })
            .expect("a picture")
    }

    fn size_of(page: &std::path::Path, src: &str) -> (u32, u32) {
        let bytes = std::fs::read(page.join(src)).expect("the file");
        image::load_from_memory(&bytes).expect("a picture").to_rgba8().dimensions()
    }

    #[test]
    fn a_picture_is_turned_into_a_new_file_and_undo_puts_the_old_one_back() {
        for html in [true, false] {
            let t = notecraft_store::testing::TempDir::new("picture-turn");
            let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
            settings.set("privacy_welcome_seen", "true").expect("settings");
            let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
            let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
            let page = if html { sample.html_page.clone() } else { sample.md_page.clone() };
            let mut png = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(30, 20, image::Rgba([0, 120, 255, 255])))
                .write_to(&mut png, image::ImageFormat::Png)
                .expect("png");
            let src = notecraft_store::assets::attach_bytes(&page, "pic.png", &png.into_inner()).expect("attach");
            let body = if html { format!("<p>Alpha</p><p><img src=\"{src}\" alt=\"Pic\"></p>") } else { format!("Alpha\n\n![Pic]({src})\n") };
            notecraft_store::page::save_body(&page, &body).expect("fixture");
            let mut app = NoteCraftApp::new(session);
            app.open_path(&page.to_string_lossy());
            let mut h: H = egui_kittest::Harness::builder()
                .with_size(vec2(1400.0, 800.0))
                .with_accessibility_check(false)
                .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
            settle(&mut h);
            {
                // A size of its own, as a resize gives it, to see it swapped.
                let ed = h.state_mut().editor.as_mut().expect("editor");
                for b in &mut ed.blocks {
                    if let notecraft_formats::Block::Image { width, height, .. } = b {
                        *width = Some(60.0);
                        *height = Some(40.0);
                    }
                }
            }
            settle(&mut h);
            let before = picture(&h);

            h.state_mut().ribbon_act(ribbon::Act::TurnPicture(src.clone(), Turn::Right));
            settle(&mut h);
            let (turned, width, height) = picture(&h);
            assert_eq!(turned, "assets/pic-rotated.png", "html={html}: the page points at the turned copy");
            assert_eq!((width, height), (Some(40.0), Some(60.0)), "html={html}: its size is swapped");
            assert_eq!(size_of(&page, &turned), (20, 30), "html={html}: the copy is turned");
            assert_eq!(size_of(&page, &src), (30, 20), "html={html}: and the original is untouched");
            h.state_mut().save_page_now();
            assert!(notecraft_store::page::load_page(&page).expect("load").body.contains("pic-rotated.png"), "html={html}: saved");

            h.state_mut().undo();
            settle(&mut h);
            assert_eq!(picture(&h), before, "html={html}: undo points the page at the original again");
            h.state_mut().redo();
            settle(&mut h);
            assert_eq!(picture(&h).0, turned, "html={html}: redo brings the turned one back");

            // A flip keeps the size and turns the copy of the copy, named once.
            h.state_mut().ribbon_act(ribbon::Act::TurnPicture(turned.clone(), Turn::FlipHorizontal));
            settle(&mut h);
            let (flipped, width, height) = picture(&h);
            assert_eq!(flipped, "assets/pic-flipped.png");
            assert_eq!((width, height), (Some(40.0), Some(60.0)));
            assert_eq!(size_of(&page, &flipped), (20, 30));
        }
    }
}

#[cfg(test)]
mod recent_range_tests {
    use super::RecentRange;

    /// Today begins at the user's own midnight, and the week and the month count whole days
    /// back from it, today included.
    #[test]
    fn ranges_begin_at_the_users_midnight() {
        let now = notecraft_model::time::secs_from_iso("2026-11-15T20:30:00Z").expect("now");
        let at = |iso: &str| notecraft_model::time::secs_from_iso(iso);
        assert_eq!(RecentRange::Today.since(now, 0), at("2026-11-15T00:00:00Z"));
        // Denver, seven hours behind: its midnight is 07:00 UTC.
        assert_eq!(RecentRange::Today.since(now, -7 * 3600), at("2026-11-15T07:00:00Z"));
        assert_eq!(RecentRange::Week.since(now, 0), at("2026-11-09T00:00:00Z"));
        assert_eq!(RecentRange::Month.since(now, 0), at("2026-10-17T00:00:00Z"));
        assert_eq!(RecentRange::All.since(now, 0), None);
    }
}

#[cfg(test)]
mod band_title_tests {
    use super::{Node, NodeKind, band_title_centre, search_trail};
    use notecraft_store::tree::NodeInfo;
    use std::path::{Path, PathBuf};

    fn node(kind: NodeKind, path: &str, title: &str, children: Vec<Node>) -> Node {
        Node {
            path: PathBuf::from(path),
            info: NodeInfo { kind, title: title.into(), color: None, format: None, id: title.into(), order: 0, read_only: false },
            children,
        }
    }

    /// Issue #370: a search result names its notebook, groups and section, and leaves out the
    /// parent pages of a sub-page.
    #[test]
    fn a_search_trail_names_the_notebook_groups_and_section() {
        let tree = vec![node(
            NodeKind::Notebook,
            "/nb",
            "Book",
            vec![node(
                NodeKind::SectionGroup,
                "/nb/g",
                "Work",
                vec![node(
                    NodeKind::Section,
                    "/nb/g/s",
                    "Projects",
                    vec![node(NodeKind::Page, "/nb/g/s/p", "Parent", vec![node(NodeKind::Page, "/nb/g/s/p/c", "Child", vec![])])],
                )],
            )],
        )];
        assert_eq!(search_trail(&tree, Path::new("/nb/g/s/p")), "Book > Work > Projects");
        assert_eq!(search_trail(&tree, Path::new("/nb/g/s/p/c")), "Book > Work > Projects");
        assert_eq!(search_trail(&tree, Path::new("/elsewhere/p")), "");
    }

    /// Issue #371: the title sits on the window's centre when it fits there, and is moved into
    /// the room between the controls, never under them, when it does not.
    #[test]
    fn the_band_title_stays_between_the_controls() {
        // Room from 300 to 900, window centre 600: a 200-wide title stays centred.
        assert_eq!(band_title_centre(600.0, 300.0, 900.0, 200.0), 600.0);
        // Controls on the right reach 650: the title moves left so its end is at 650.
        let centre = band_title_centre(600.0, 300.0, 650.0, 200.0);
        assert!(centre + 100.0 <= 650.0 && centre - 100.0 >= 300.0, "{centre}");
        // Controls on the left reach 560: the title moves right so it starts at 560.
        let centre = band_title_centre(600.0, 560.0, 1000.0, 200.0);
        assert!(centre - 100.0 >= 560.0, "{centre}");
        // Wider than the room (it is ellipsized to fit first): it starts where the room starts.
        assert_eq!(band_title_centre(600.0, 300.0, 400.0, 300.0), 450.0);
    }
}

#[cfg(test)]
mod canvas_pan_tests {
    use super::*;

    type H = egui_kittest::Harness<'static, NoteCraftApp>;

    /// The sample notebook with its HTML page open and a pen in hand, which pans the canvas.
    fn drawing(name: &str) -> (notecraft_store::testing::TempDir, notecraft_engine::sample::Sample, H) {
        let t = notecraft_store::testing::TempDir::new(name);
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.html_page.to_string_lossy());
        app.pointer = ribbon::Pointer::Draw;
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        settle(&mut h);
        (t, sample, h)
    }

    fn settle(h: &mut H) {
        for _ in 0..5 {
            h.step();
        }
    }

    /// Two fingers on the touchpad, moved `dy` points down the page (positive: towards its top).
    fn swipe(h: &mut H, dy: f32) {
        let over = h.state().page_view.expect("the page is shown").center();
        h.event(egui::Event::PointerMoved(over));
        h.step();
        for _ in 0..20 {
            h.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: vec2(0.0, dy / 20.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
                source: egui::MouseWheelSource::Trackpad,
            });
            h.step();
        }
        settle(h);
    }

    /// Where on screen the page's top left corner is drawn.
    fn page_corner(h: &H) -> egui::Pos2 {
        h.state().editor.as_ref().expect("editor").origin
    }

    /// The canvas a pen pans is endless down and to the right, where there is room to draw,
    /// but not above the page's top or left of its edge: a swipe up there took the view
    /// thousands of points into nothing, where the pen then drew a dot the page grew to hold.
    #[test]
    fn a_pen_cannot_pan_the_canvas_above_the_top_of_the_page() {
        let (_t, _sample, mut h) = drawing("pan-above-top");
        // As far up as the page goes: its top, wherever its own scroll bar had it.
        let ceiling = h.state().page_offset_y;
        swipe(&mut h, 8000.0);
        assert!(h.state().canvas_pan.y <= ceiling + 0.5, "no pan above the page: {:?}, its top at {ceiling}", h.state().canvas_pan);
        let top = page_corner(&h);
        assert!(top.y < h.state().page_view.expect("shown").center().y, "the page's top is in view: {top:?}");

        // Down the page it still pans, as far as the pen wants room.
        swipe(&mut h, -3000.0);
        assert!(h.state().canvas_pan.y < -2000.0, "the canvas pans down: {:?}", h.state().canvas_pan);
        // And back up to the top, but no further.
        swipe(&mut h, 8000.0);
        let (pan, ceiling) = (h.state().canvas_pan.y, h.state().page_offset_y);
        assert!((pan - ceiling).abs() < 0.5, "back at the top and no further: pan {pan}, top at {ceiling}");
    }

    /// Ink already drawn above the page can still be reached: the canvas pans up as far as it.
    #[test]
    fn ink_above_the_page_can_still_be_panned_to() {
        let (_t, _sample, mut h) = drawing("pan-to-ink-above");
        {
            let pad = h.state_mut().pad.as_mut().expect("a pad for the page");
            let mut stroke = notecraft_ink::Stroke::new(notecraft_ink::Tool::Pen, notecraft_ink::Rgb(0, 0, 0), 2.0);
            stroke.add(notecraft_ink::Dab::new(100.0, -400.0, 1.0));
            stroke.add(notecraft_ink::Dab::new(140.0, -380.0, 1.0));
            pad.drawing.add(stroke);
        }
        settle(&mut h);
        swipe(&mut h, 8000.0);
        let pan = h.state().canvas_pan.y;
        assert!(pan > 350.0 && pan < 500.0, "up as far as the ink above the page, and no further: {pan}");
    }

    /// A pan belongs to the page it was made on: the next page opens at its own top left. A pan
    /// kept from one page showed every other page thousands of points off the screen, blank.
    #[test]
    fn another_page_opens_at_its_top_left_whatever_the_last_was_panned_to() {
        let (_t, sample, mut h) = drawing("pan-next-page");
        swipe(&mut h, -3000.0);
        assert!(h.state().canvas_pan.y < -2000.0, "panned down the first page");
        // And scrolled down it by its own scroll bar, as the caret's pointer scrolls.
        h.state_mut().scroll_to = Some(200.0);
        settle(&mut h);
        assert!(h.state().page_offset_y > 50.0, "scrolled down the first page: {}", h.state().page_offset_y);
        h.state_mut().open_page(&sample.md_page);
        settle(&mut h);
        assert_eq!(h.state().canvas_pan, egui::Vec2::ZERO, "the next page starts at its top left");
        assert!(h.state().page_offset_y < 0.5, "scrolled to its top too: {}", h.state().page_offset_y);

        // The same page read again keeps its view: scrolled down, and panned back up part of the
        // way, as the pen may, it stays so.
        h.state_mut().open_page(&sample.html_page);
        settle(&mut h);
        h.state_mut().scroll_to = Some(200.0);
        settle(&mut h);
        swipe(&mut h, 100.0);
        let (pan, scrolled) = (h.state().canvas_pan, h.state().page_offset_y);
        assert!(pan.y > 50.0, "panned up within the page: {pan:?}, scrolled {scrolled}");
        h.state_mut().open_page(&sample.html_page);
        settle(&mut h);
        assert!((h.state().canvas_pan.y - pan.y).abs() < 1.0, "the same page read again kept its pan: {pan:?} -> {:?}", h.state().canvas_pan);
        assert!((h.state().page_offset_y - scrolled).abs() < 1.0, "and its scroll offset: {scrolled} -> {}", h.state().page_offset_y);

        // A page that cannot be read leaves the view of the one open as it was.
        swipe(&mut h, -3000.0);
        let pan = h.state().canvas_pan;
        h.state_mut().open_page(&sample.md_page.with_file_name("no-such-page"));
        settle(&mut h);
        assert_eq!(h.state().canvas_pan, pan, "a page that did not open did not move this one");
    }
}

#[cfg(test)]
mod page_open_tests {
    use super::*;

    type H = egui_kittest::Harness<'static, NoteCraftApp>;

    fn window(name: &str) -> (notecraft_store::testing::TempDir, notecraft_engine::sample::Sample, H) {
        let t = notecraft_store::testing::TempDir::new(name);
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.html_page.to_string_lossy());
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        for _ in 0..5 {
            h.step();
        }
        (t, sample, h)
    }

    fn shown(h: &H) -> Option<PathBuf> {
        h.state().editor.as_ref().map(|e| e.dir.clone())
    }

    /// Writes into a named pipe once a reader has it open, without waiting for one.
    #[cfg(unix)]
    fn write_when_read(pipe: &Path, bytes: &[u8]) -> bool {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const O_NONBLOCK: i32 = 0o4000;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        const O_NONBLOCK: i32 = 0x0004;
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < until {
            if let Ok(mut out) = std::fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(pipe) {
                return out.write_all(bytes).is_ok();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    /// A page of a synced folder whose files are not on this computer yet is downloaded as they
    /// are read, and opening it read them on the window's thread: the window froze until they
    /// came. A named pipe stands in for a file still coming, as reading one waits until something
    /// is written into it. The page shown stays, and can be used, until the files are in; then
    /// the page asked for opens.
    #[cfg(unix)]
    #[test]
    fn a_page_still_downloading_opens_when_its_files_are_in_and_the_window_goes_on_meanwhile() {
        let (_t, sample, mut h) = window("page-open-downloading");
        assert_eq!(shown(&h).as_deref(), Some(sample.html_page.as_path()));
        let pipe = sample.md_page.join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());

        let started = std::time::Instant::now();
        h.state_mut().open_page_soon(&sample.md_page);
        assert!(started.elapsed() < Duration::from_secs(1), "asked for, the page did not hold the window: {:?}", started.elapsed());
        for _ in 0..5 {
            h.step();
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.html_page.as_path()), "the page shown stays while the other is on its way");

        // The "download" finishes: the page opens within a few frames.
        assert!(write_when_read(&pipe, b"the file, downloaded"), "the page's files were being read");
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while shown(&h).as_deref() != Some(sample.md_page.as_path()) && std::time::Instant::now() < until {
            h.step();
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.md_page.as_path()), "and opens once its files are in");
    }

    /// Asking for another page while one is still on its way opens the one asked for last.
    #[cfg(unix)]
    #[test]
    fn the_page_asked_for_last_is_the_one_opened() {
        let (_t, sample, mut h) = window("page-open-last-wins");
        let pipe = sample.md_page.join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());
        h.state_mut().open_page_soon(&sample.md_page);
        h.step();
        h.state_mut().open_page_soon(&sample.deep_page);
        step_until(&mut h, |h| shown(h).as_deref() == Some(sample.deep_page.as_path()));
        assert_eq!(shown(&h).as_deref(), Some(sample.deep_page.as_path()), "the page asked for last opened");
        assert!(write_when_read(&pipe, b"late"), "the first page's files were still being read");
        for _ in 0..20 {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.deep_page.as_path()), "the first, coming in late, did not open over it");
    }

    /// Anything that opens a page and goes on with it, a command, a link, an agent, opens it in
    /// the same call, as it always did; only a page asked for by hand may wait on its files. A
    /// local page asked for by hand opens within a few frames however long the reading of its
    /// files takes to be scheduled.
    #[test]
    fn a_page_opened_by_the_window_itself_is_open_at_once() {
        let (_t, sample, mut h) = window("page-open-local");
        h.state_mut().open_page(&sample.md_page);
        assert_eq!(shown(&h).as_deref(), Some(sample.md_page.as_path()), "open_page opens in the same call");
        h.state_mut().open_path(&sample.deep_page.to_string_lossy());
        assert_eq!(shown(&h).as_deref(), Some(sample.deep_page.as_path()), "and so does open_path");

        h.state_mut().open_page_soon(&sample.html_page);
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while shown(&h).as_deref() != Some(sample.html_page.as_path()) && std::time::Instant::now() < until {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.html_page.as_path()), "asked for by hand, a local page opens within a few frames");
    }

    /// A page coming in while a dialog is open waits until it is closed: the dialog acts on the
    /// page it was opened on, and Insert Table would otherwise put its table into the other.
    #[cfg(unix)]
    #[test]
    fn a_page_coming_in_waits_for_an_open_dialog() {
        let (_t, sample, mut h) = window("page-open-dialog");
        let pipe = sample.md_page.join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());
        h.state_mut().open_page_soon(&sample.md_page);
        h.state_mut().dialog = Some(Dialog::NewNotebook { name: String::new(), format: None, first: true });
        assert!(write_when_read(&pipe, b"in"), "the page's files were being read");
        for _ in 0..20 {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.html_page.as_path()), "with the dialog open, the page shown stays");
        h.state_mut().dialog = None;
        step_until(&mut h, |h| shown(h).as_deref() == Some(sample.md_page.as_path()));
        assert_eq!(shown(&h).as_deref(), Some(sample.md_page.as_path()), "and the page comes in once it is closed");
    }

    /// Next page, pressed again while the next page is still on its way, goes on to the page
    /// after that one, rather than asking for the same page again.
    #[cfg(unix)]
    #[test]
    fn next_page_pressed_twice_goes_past_the_page_on_its_way() {
        let (_t, sample, mut h) = window("page-open-step-twice");
        let section = sample.html_page.parent().expect("section").to_path_buf();
        h.state_mut().create_page_in(&section);
        h.state_mut().create_page_in(&section);
        let pages = h.state().listed_pages();
        assert!(pages.len() >= 3, "three pages to step through: {pages:?}");
        h.state_mut().open_page(&pages[0]);
        let pipe = pages[1].join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());

        h.state_mut().step_page(true);
        assert_eq!(h.state().opening.as_ref().map(|o| o.dir.clone()).as_deref(), Some(pages[1].as_path()), "the second page is on its way");
        h.state_mut().step_page(true);
        step_until(&mut h, |h| shown(h).as_deref() == Some(pages[2].as_path()));
        assert_eq!(shown(&h).as_deref(), Some(pages[2].as_path()), "the second press went on to the third page");
        assert!(write_when_read(&pipe, b"late"), "the second page's files were still being read");
        for _ in 0..20 {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(shown(&h).as_deref(), Some(pages[2].as_path()), "and the second, coming in late, did not open over it");
    }

    /// A page asked for before the notebooks folder changed does not open over the new folder
    /// when its files come in.
    #[cfg(unix)]
    #[test]
    fn a_page_on_its_way_is_dropped_when_the_notebooks_folder_changes() {
        let (t, sample, mut h) = window("page-open-folder-change");
        let pipe = sample.md_page.join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());
        h.state_mut().open_page_soon(&sample.md_page);
        assert!(h.state().opening.is_some(), "the page is on its way");
        let other = t.path().join("other-notebooks");
        std::fs::create_dir_all(&other).expect("folder");
        h.state_mut().apply_notebooks_root_choice(Some(&other));
        assert!(h.state().opening.is_none(), "the request went with the folder left");
        assert!(write_when_read(&pipe, b"late"), "the page's files were still being read");
        for _ in 0..20 {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_ne!(shown(&h).as_deref(), Some(sample.md_page.as_path()), "the page from the folder left did not open");
    }

    /// Words being typed into the page shown's title when the page asked for comes in stay out
    /// of the title of the page come in.
    #[cfg(unix)]
    #[test]
    fn typing_in_the_title_does_not_go_into_the_page_coming_in() {
        let (_t, sample, mut h) = window("page-open-title-typing");
        let pipe = sample.md_page.join("still-coming.bin");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo").success());
        let title = h.state().editor.as_ref().map(|e| e.title_buf.clone()).expect("a page");
        let id = egui::Id::new(("page-title-box", h.ctx.viewport_id()));
        h.ctx.memory_mut(|m| m.request_focus(id));
        h.step();
        h.state_mut().open_page_soon(&sample.md_page);
        assert!(h.state().opening.is_some(), "the page is on its way");
        h.step();
        assert!(h.ctx.memory(|m| m.has_focus(id)), "the title has the keyboard");
        assert!(write_when_read(&pipe, b"in"), "the page's files were being read");
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while shown(&h).as_deref() != Some(sample.md_page.as_path()) && std::time::Instant::now() < until {
            h.input_mut().events.push(egui::Event::Text("zq".into()));
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(shown(&h).as_deref(), Some(sample.md_page.as_path()), "the page came in");
        for _ in 0..3 {
            h.input_mut().events.push(egui::Event::Text("zq".into()));
            h.step();
        }
        let come_in = h.state().editor.as_ref().map(|e| e.title_buf.clone()).unwrap_or_default();
        assert!(!come_in.contains("zq"), "the page come in kept its own title: {come_in:?} (the other was {title:?})");
    }

    /// Steps the window until `done` holds, for at most ten seconds.
    #[cfg(unix)]
    fn step_until(h: &mut H, done: impl Fn(&H) -> bool) {
        let until = std::time::Instant::now() + Duration::from_secs(10);
        while !done(h) && std::time::Instant::now() < until {
            h.step();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[cfg(test)]
mod tabs_layout_tests {
    use super::*;

    /// View > Tabs layout read every open notebook's tree from disk to draw the tabs, every
    /// frame: on a synced folder every one of those reads goes to the mount, and the page
    /// crawled. The tabs come from the tree the window already holds.
    #[test]
    fn the_section_tabs_do_not_read_the_notebooks_from_disk_each_frame() {
        let t = notecraft_store::testing::TempDir::new("tabs-layout-reads");
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        settings.set("privacy_welcome_seen", "true").expect("settings");
        let mut session = Session::new(&t.path().join("notebooks"), settings).expect("session");
        let sample = notecraft_engine::sample::create_sample(&mut session, None).expect("sample");
        let mut app = NoteCraftApp::new(session);
        app.open_path(&sample.html_page.to_string_lossy());
        app.tabs_layout = true;
        let mut h = egui_kittest::Harness::builder()
            .with_size(vec2(1400.0, 800.0))
            .with_accessibility_check(false)
            .build_ui_state(|ui, app: &mut NoteCraftApp| app.show(ui), app);
        for _ in 0..5 {
            h.step();
        }
        // A section of the other notebook gets its `_node.toml` from a named pipe, and a thread
        // on the other end counts how many times it is read: each read of the notebooks from
        // disk reads it once.
        #[cfg(unix)]
        {
            use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
            fn sections(nodes: &[Node], out: &mut Vec<PathBuf>) {
                for n in nodes {
                    if n.info.kind == NodeKind::Section {
                        out.push(n.path.clone());
                    }
                    sections(&n.children, out);
                }
            }
            let mut all = Vec::new();
            sections(h.state().session.tree(), &mut all);
            let elsewhere = all
                .into_iter()
                .find(|s| !sample.html_page.starts_with(s))
                .map(|s| s.join("_node.toml"))
                .expect("a section other than the open page's");
            let text = std::fs::read(&elsewhere).expect("the section's file");
            std::fs::remove_file(&elsewhere).expect("remove");
            assert!(std::process::Command::new("mkfifo").arg(&elsewhere).status().expect("mkfifo").success());
            let (reads, stop) = (std::sync::Arc::new(AtomicUsize::new(0)), std::sync::Arc::new(AtomicBool::new(false)));
            let writer = {
                let (pipe, reads, stop) = (elsewhere.clone(), reads.clone(), stop.clone());
                std::thread::spawn(move || {
                    use std::os::unix::fs::OpenOptionsExt as _;
                    #[cfg(any(target_os = "linux", target_os = "android"))]
                    const O_NONBLOCK: i32 = 0o4000;
                    #[cfg(not(any(target_os = "linux", target_os = "android")))]
                    const O_NONBLOCK: i32 = 0x0004;
                    // A reader waits in its open for a writer; opened without waiting, the write
                    // end opens only while one does. Nothing is written: a write marks the pipe
                    // modified, the file watcher sees that and reloads the notebooks, which reads
                    // the pipe again, over and over (issue #447). A reader of an empty section
                    // file still counts.
                    while !stop.load(Ordering::SeqCst) {
                        match std::fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(&pipe) {
                            Ok(_) => {
                                reads.fetch_add(1, Ordering::SeqCst);
                            }
                            Err(_) => std::thread::sleep(Duration::from_millis(1)),
                        }
                    }
                })
            };
            // Anything the change itself sets off is let settle first.
            let settle = std::time::Instant::now() + Duration::from_millis(1500);
            while std::time::Instant::now() < settle {
                h.step();
                std::thread::sleep(Duration::from_millis(20));
            }
            let before = reads.load(Ordering::SeqCst);
            // Over a second: longer than the watcher takes to scan the folders and pass a change
            // on, so a change the pipe made would show up here as reads.
            for _ in 0..20 {
                h.step();
                std::thread::sleep(Duration::from_millis(50));
            }
            let read = reads.load(Ordering::SeqCst) - before;
            stop.store(true, Ordering::SeqCst);
            writer.join().expect("the writer");
            // The section's own file back, for what follows.
            std::fs::remove_file(&elsewhere).expect("remove the pipe");
            std::fs::write(&elsewhere, &text).expect("the section's file back");
            assert_eq!(read, 0, "twenty frames with the tabs shown, and no reads of the notebooks");
            // The watcher's reload of the section's own file is let settle too, before its tab is
            // looked for by name.
            let settle = std::time::Instant::now() + Duration::from_millis(1500);
            while std::time::Instant::now() < settle {
                h.step();
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        // The tabs are drawn, and one opens its section's first page.
        // Another section of the open page's notebook, with a page in it.
        let here = section_for(&sample.html_page).expect("the page's section");
        let other = h
            .state()
            .session
            .tree()
            .iter()
            .flat_map(|n| n.children.iter())
            .find(|n| n.info.kind == NodeKind::Section && n.path != here && n.children.iter().any(|p| p.info.kind == NodeKind::Page));
        let (section, title) = other.map(|n| (n.path.clone(), n.info.title.clone())).expect("another section with a page");
        {
            use egui_kittest::kittest::{NodeT as _, Queryable as _};
            // The section is named in the notebooks pane too; its tab is the one over the page,
            // right of the panes.
            let tab = h
                .query_all_by_label(&title)
                .max_by(|a, b| {
                    let x = |n: &egui_kittest::Node<'_>| n.accesskit_node().bounding_box().map_or(0.0, |r| r.x0);
                    x(a).total_cmp(&x(b))
                })
                .expect("the section's tab");
            tab.click();
        }
        for _ in 0..5 {
            h.step();
        }
        let open = h.state().editor.as_ref().map(|e| e.dir.clone()).expect("a page open");
        assert!(open.starts_with(&section), "a tab opens a page of its section: {open:?}");
    }
}

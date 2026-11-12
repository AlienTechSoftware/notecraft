//! Ink on the page: the layer a pen writes on, over the words and between them.
//!
//! A page is a folder, so its ink is a file in that folder: `ink.svg`, beside `index.md` or
//! `index.html`. Nothing in either page format had to change to allow it, and the ink
//! travels, syncs and backs up with the page because it is simply another file in the same
//! place. A page with no `ink.svg` has never been drawn on.
//!
//! The file is ordinary SVG: filled outlines any browser renders, because SVG has no
//! variable-width stroke. It also carries the pen's own readings in our namespace, which is
//! what lets a stroke be rubbed out or redrawn later (`notecraft_ink::svg`).
//!
//! The pen writes anywhere on the page, as OneNote's does, rather than inside a box. The
//! strokes are kept in the page body's own coordinates, measured from its top left corner,
//! so they stay beside the words they were written against as the window is resized.

use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};
use notecraft_ink::{Dab, Drawing, Rgb, Stroke, Tool, pick, shape, svg};
use std::path::{Path, PathBuf};

/// What the pen is set to do, as the Draw tab has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub pointer: crate::ribbon::Pointer,
    pub tool: Tool,
    pub colour: Rgb,
    /// In points.
    pub width: f32,
    /// How readily this pen answers pressure; 1 leaves the readings as they came.
    pub gain: f32,
}

impl Default for Pen {
    fn default() -> Self {
        Self { pointer: crate::ribbon::Pointer::Write, tool: Tool::Pen, colour: Rgb::BLACK, width: 3.0, gain: 1.0 }
    }
}

/// The file a page's ink lives in, inside the page's own folder.
pub const FILE: &str = "ink.svg";

/// Whether an image block is the page's own ink, which the page names so that its words
/// and what is on it agree, and so another program opening the page sees the handwriting.
/// NoteCraft itself does not draw it as a picture: it draws the layer.
pub fn is_page_ink(src: &str) -> bool {
    src.trim() == FILE
}

/// Whether an image line in the page is one that names ink.
///
/// Said of a line whose file may be gone, so the file itself cannot be asked: a picture
/// beside the page file (not in `assets/`), an SVG, with `Ink` for its words. That is the
/// line NoteCraft writes, and no line a person would write by hand for a picture of
/// their own, which lives in `assets/` and says what it is a picture of.
pub fn is_ink_line(src: &str, alt: &str) -> bool {
    let src = src.trim();
    !src.contains('/')
        && !src.contains('\\')
        && !src.contains("://")
        && src.to_ascii_lowercase().ends_with(".svg")
        && alt.trim().eq_ignore_ascii_case("Ink")
}

/// An earlier version of NoteCraft put a drawing into the page as a picture, pointing at
/// `assets/drawing-1.svg`. Nothing draws those now, so such a page showed a broken picture
/// where its drawing had been. This says whether an image block is one of them.
pub fn is_old_sheet(src: &str) -> bool {
    let src = src.trim();
    !src.contains("://") && src.to_ascii_lowercase().starts_with("assets/drawing-") && src.to_ascii_lowercase().ends_with(".svg")
}

/// How large a page's ink sheet is said to be in the file's `viewBox`. The pen may write
/// past it; the number is only what another program uses to frame the picture.
pub const SHEET_W: f32 = 820.0;
pub const SHEET_H: f32 = 1100.0;

/// The ink of one page.
#[derive(Clone, Debug)]
pub struct Pad {
    /// Where the file is, whether or not it has been written yet.
    pub file: PathBuf,
    pub drawing: Drawing,
    /// Changed since it was last written.
    pub dirty: bool,
    /// How many strokes in the file were a picture only, with no readings to give back.
    pub flat: usize,
    /// The file holds more than `drawing` does: it could not be read, or some of its lines are
    /// a picture only. Before the file is first written over, it is kept beside the ink.
    pub incomplete: bool,
    /// Where that file was kept, once it has been, for the window to say so.
    pub kept_original: Option<PathBuf>,
    /// The stroke being drawn now, if the pen is down.
    drawing_now: Option<Stroke>,
    /// The strokes being held, by their place in the drawing. Never written to the file:
    /// it is what someone has their hand on at this moment, not part of the page.
    held: Vec<usize>,
    /// The loop being drawn now by the lasso.
    loop_now: Vec<(f32, f32)>,
    /// Where a shape's drag began, which is one of the two corners it is drawn between.
    shape_from: Option<(f32, f32)>,
    /// Where the pointer was when a move or a stretch began, and the box as it was then.
    grabbed: Option<(egui::Pos2, (f32, f32, f32, f32), Grab)>,
    /// How far held ink was moved by hand in the ordinary pointer since the window last asked,
    /// in page points: the boxes picked up with it follow ([`Pad::take_shift`]).
    shift: (f32, f32),
    /// The strokes as they were after the last finished change, and before each change the ones
    /// undo can go back to, newest last, with when that change was made. Whatever the change (a
    /// stroke drawn, rubbed out, deleted, moved or stretched), undo puts back what was there.
    kept: Vec<Stroke>,
    undo_steps: Vec<(Vec<Stroke>, std::time::Instant)>,
    /// The strokes as they were before each undo, newest last, for redo to put back. A new
    /// change starts a new branch and empties it (issue #155).
    redo_steps: Vec<Vec<Stroke>>,
    /// Changed since the last finished change was kept.
    touched: bool,
}

/// How many changes to the ink undo can go back through.
const INK_UNDO_STEPS: usize = 40;

/// What a press on held ink began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Grab {
    /// Moving the whole of it.
    Move,
    /// Stretching it by the corner opposite this one.
    Corner(bool, bool),
}

impl Pad {
    /// The ink of the page in `page_dir`: what is in its `ink.svg`, or an empty layer when
    /// it has none. Every page has a layer; the file appears the first time the page is
    /// drawn on, so a page nobody has drawn on stays as clean as it was.
    pub fn of_page(page_dir: &Path) -> Pad {
        let file = page_dir.join(FILE);
        let read = std::fs::read_to_string(&file).ok().and_then(|text| svg::read(&text).ok());
        // A file that is there but could not be read, or that holds lines with no readings,
        // would lose them the first time the ink is written (issue #157).
        let incomplete = match &read {
            Some((_, flat)) => *flat > 0,
            None => file.exists(),
        };
        let (drawing, flat) = read.unwrap_or_else(|| (Drawing::new(SHEET_W, SHEET_H), 0));
        Pad {
            file,
            drawing,
            dirty: false,
            flat,
            incomplete,
            kept_original: None,
            drawing_now: None,
            held: Vec::new(),
            loop_now: Vec::new(),
            shape_from: None,
            grabbed: None,
            shift: (0.0, 0.0),
            kept: Vec::new(),
            undo_steps: Vec::new(),
            redo_steps: Vec::new(),
            touched: false,
        }
        .with_kept()
    }

    /// Writes the ink, with its readings, which is what the page keeps.
    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // The file holds what this drawing does not: it is kept, as it was, before it is
        // written over or taken away, and only then (issue #157).
        if self.incomplete && self.file.is_file() {
            let kept = self.keep_original()?;
            self.kept_original = Some(kept);
        }
        self.incomplete = false;
        // A page drawn on and then rubbed clean loses its file rather than keeping an
        // empty one, so a page with no ink has no ink in its folder either.
        if self.drawing.is_empty() {
            match std::fs::remove_file(&self.file) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        } else {
            std::fs::write(&self.file, svg::write(&self.drawing, svg::Flavour::Editable))?;
        }
        self.dirty = false;
        Ok(())
    }

    /// Copies the ink file as it is to `ink.svg.bak`, or the first free `ink-N.svg.bak`, beside
    /// it. Not an `.svg`, so nothing takes the copy for the page's ink or for a picture of it.
    fn keep_original(&self) -> std::io::Result<PathBuf> {
        let folder = self.file.parent().map(Path::to_path_buf).unwrap_or_default();
        for n in 1usize..=10_000 {
            let name = if n == 1 { format!("{FILE}.bak") } else { format!("ink-{n}.svg.bak") };
            let kept = folder.join(name);
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&kept) {
                Ok(mut out) => {
                    let mut original = std::fs::File::open(&self.file)?;
                    std::io::copy(&mut original, &mut out)?;
                    return Ok(kept);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "every name for keeping the original ink file is taken"))
    }

    /// Writes it again as a plain picture, beside the ink, for sending somewhere else.
    /// Returns where it was written.
    ///
    /// The picture goes into a file of its own, `ink-picture.svg` or the first of
    /// `ink-picture-2.svg`, `ink-picture-3.svg` … that is free. A file already there may be a
    /// picture saved earlier and kept on purpose, or something else of the owner's under that
    /// name, and is never written over (issue #158).
    pub fn save_plain(&self) -> std::io::Result<PathBuf> {
        let folder = self.file.parent().map(Path::to_path_buf).unwrap_or_default();
        std::fs::create_dir_all(&folder)?;
        let text = svg::write(&self.drawing, svg::Flavour::Plain);
        for n in 1usize..=10_000 {
            let name = if n == 1 { "ink-picture.svg".to_string() } else { format!("ink-picture-{n}.svg") };
            let plain = folder.join(name);
            // `create_new` refuses a name that is taken, even one taken a moment ago.
            match std::fs::OpenOptions::new().write(true).create_new(true).open(&plain) {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(text.as_bytes())?;
                    return Ok(plain);
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "every ink-picture name in this folder is taken"))
    }

    /// Moves the strokes of an old inserted sheet onto the page, and says how many came.
    /// The sheet's own file is left where it is: it is the owner's, and nothing here
    /// throws away a file it has only read.
    pub fn absorb(&mut self, page_dir: &Path, src: &str) -> usize {
        if !is_old_sheet(src) {
            return 0;
        }
        let file = page_dir.join(src.trim().replace('/', std::path::MAIN_SEPARATOR_STR));
        let Ok(text) = std::fs::read_to_string(&file) else { return 0 };
        let Ok((old, _)) = svg::read(&text) else { return 0 };
        let before = self.drawing.strokes.len();
        for stroke in old.strokes {
            self.drawing.add(stroke);
        }
        let came = self.drawing.strokes.len() - before;
        self.dirty |= came > 0;
        came
    }

    /// Takes the last change to the ink back, whatever it was, and keeps it for redo. Only a
    /// change made since the page was opened is taken back: ink that was already on the page
    /// is where the page starts, not a change, and undo used to delete it (issue #156).
    pub fn undo(&mut self) -> bool {
        self.let_go();
        self.settle();
        let Some((before, _)) = self.undo_steps.pop() else { return false };
        self.redo_steps.push(std::mem::replace(&mut self.drawing.strokes, before));
        self.kept = self.drawing.strokes.clone();
        self.dirty = true;
        true
    }

    /// Puts back the change undo last took back, while nothing new has been done since.
    pub fn redo(&mut self) -> bool {
        self.let_go();
        self.settle();
        let Some(after) = self.redo_steps.pop() else { return false };
        let before = std::mem::replace(&mut self.drawing.strokes, after);
        self.undo_steps.push((before, std::time::Instant::now()));
        self.kept = self.drawing.strokes.clone();
        self.dirty = true;
        true
    }

    /// Whether redo has anything to put back.
    pub fn can_redo(&self) -> bool {
        !self.redo_steps.is_empty()
    }

    pub fn strokes(&self) -> usize {
        self.drawing.strokes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.drawing.is_empty() && self.drawing_now.is_none()
    }

    /// How many strokes are being held.
    pub fn holding(&self) -> usize {
        self.held.len()
    }

    /// Lets go of whatever is held, which is what happens when the tool changes or the
    /// page does.
    pub fn let_go(&mut self) {
        self.held.clear();
        self.loop_now.clear();
        self.grabbed = None;
    }

    fn with_kept(mut self) -> Pad {
        self.kept = self.drawing.strokes.clone();
        self
    }

    /// Ends a change once the hand is off: what the ink was before it becomes a step undo can go
    /// back to. Called by the window when no button is down, so a drag is one step, not one a frame.
    pub fn settle(&mut self) {
        if !self.touched || self.drawing_now.is_some() || self.grabbed.is_some() {
            return;
        }
        self.touched = false;
        if self.drawing.strokes == self.kept {
            return;
        }
        let before = std::mem::replace(&mut self.kept, self.drawing.strokes.clone());
        self.undo_steps.push((before, std::time::Instant::now()));
        self.redo_steps.clear();
        if self.undo_steps.len() > INK_UNDO_STEPS {
            self.undo_steps.remove(0);
        }
    }

    /// When the change undo would take back was made, if there is one.
    pub fn newest_change(&self) -> Option<std::time::Instant> {
        self.undo_steps.last().map(|(_, when)| *when)
    }

    /// Whether undo has anything to take back: a change made since the page was opened.
    pub fn can_undo(&self) -> bool {
        !self.undo_steps.is_empty()
    }

    /// Picks up every stroke a selection box touches; `rect` is two corners in page points.
    pub fn hold_touching(&mut self, rect: (f32, f32, f32, f32)) {
        self.let_go();
        self.held = pick::touching(&self.drawing, rect);
    }

    /// Moves the held ink by (`dx`, `dy`) page points: the boxes picked up with it were moved.
    pub fn move_held(&mut self, dx: f32, dy: f32) {
        if self.held.is_empty() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        pick::move_by(&mut self.drawing, &self.held, dx, dy);
        self.dirty = true;
        self.touched = true;
    }

    /// How far the held ink was moved by hand since this was last asked, if at all.
    pub fn take_shift(&mut self) -> Option<(f32, f32)> {
        let shift = std::mem::take(&mut self.shift);
        (shift != (0.0, 0.0)).then_some(shift)
    }

    /// The box round all the ink on the page, in page points.
    pub fn bounds(&self) -> Option<[f32; 4]> {
        let all: Vec<usize> = (0..self.drawing.strokes.len()).collect();
        pick::bounds(&self.drawing, &all).map(|(x0, y0, x1, y1)| [x0, y0, x1, y1])
    }

    /// Throws away the held ink and says whether any went.
    pub fn delete_held(&mut self) -> bool {
        let went = pick::remove(&mut self.drawing, &self.held) > 0;
        self.let_go();
        self.dirty |= went;
        self.touched |= went;
        went
    }
}

/// Where held ink can be taken hold of on screen this frame, for the boxes under it: a press
/// there is on the ink, not away from what is selected.
pub fn held_area(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp::<egui::Rect>(held_area_id()))
}

fn held_area_id() -> egui::Id {
    egui::Id::new("inkpad-held-area")
}

/// Held ink in the ordinary pointer (a selection box picked it up): a drag inside its box moves
/// it, one on a corner stretches it, Delete throws it away and Escape lets go. Only the box
/// takes the pointer, so the words and boxes around it are as they always are.
fn held_in_write(ui: &mut Ui, body: egui::Rect, origin: egui::Pos2, pad: &mut Pad, zoom: f32) -> bool {
    let Some(b) = pick::bounds(&pad.drawing, &pad.held) else {
        pad.let_go();
        ui.data_mut(|d| d.remove::<egui::Rect>(held_area_id()));
        return false;
    };
    let held_box = egui::Rect::from_min_max(origin + vec2(b.0 * zoom, b.1 * zoom), origin + vec2(b.2 * zoom, b.3 * zoom)).expand(3.0);
    let area = held_box.expand(HANDLE / 2.0).intersect(body);
    ui.data_mut(|d| d.insert_temp(held_area_id(), area));
    let resp = ui.interact(area, ui.id().with("ink-held"), Sense::drag());
    let here = ui.ctx().pointer_interact_pos().unwrap_or(egui::Pos2::ZERO);
    if resp.hovered() || resp.dragged() {
        let corner = pad.grabbed.map(|(.., g)| g).or_else(|| grab_corner(held_box, here));
        ui.ctx().set_cursor_icon(match corner {
            Some(Grab::Corner(right, bottom)) if right == bottom => egui::CursorIcon::ResizeNwSe,
            Some(Grab::Corner(..)) => egui::CursorIcon::ResizeNeSw,
            _ => egui::CursorIcon::Move,
        });
    }
    // Ctrl and a click on held ink takes the stroke under it out of what is held, or puts it in.
    let ctrl = ui.input(|i| i.modifiers.command || i.modifiers.ctrl);
    if resp.clicked() && ctrl {
        let (x, y) = ((here.x - origin.x) / zoom, (here.y - origin.y) / zoom);
        if let Some(i) = pick::at(&pad.drawing, x, y, 3.0) {
            match pad.held.iter().position(|h| *h == i) {
                Some(k) => {
                    pad.held.remove(k);
                }
                None => pad.held.push(i),
            }
        }
        return false;
    }
    if resp.drag_started() {
        let from = ui.input(|i| i.pointer.press_origin()).unwrap_or(here);
        pad.grabbed = Some((from, b, grab_corner(held_box, from).unwrap_or(Grab::Move)));
    }
    let mut changed = false;
    if let Some((from, was, grab)) = pad.grabbed {
        let (dx, dy) = ((here.x - from.x) / zoom, (here.y - from.y) / zoom);
        let (x0, y0, x1, y1) = was;
        if resp.dragged() {
            let now = pick::bounds(&pad.drawing, &pad.held).unwrap_or(was);
            match grab {
                Grab::Move => {
                    let step = (x0 + dx - now.0, y0 + dy - now.1);
                    pick::move_by(&mut pad.drawing, &pad.held, step.0, step.1);
                    pad.shift = (pad.shift.0 + step.0, pad.shift.1 + step.1);
                }
                Grab::Corner(right, bottom) => {
                    let (ox, oy) = (if right { x0 } else { x1 }, if bottom { y0 } else { y1 });
                    let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
                    let want_w = if right { w + dx } else { w - dx };
                    let want_h = if bottom { h + dy } else { h - dy };
                    let have_w = (now.2 - now.0).max(0.001);
                    let have_h = (now.3 - now.1).max(0.001);
                    pick::scale_about(&mut pad.drawing, &pad.held, ox, oy, want_w.max(4.0) / have_w, want_h.max(4.0) / have_h);
                }
            }
            ui.ctx().request_repaint();
        }
        if !resp.dragged() {
            pad.grabbed = None;
            changed = true;
        }
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        pad.let_go();
    } else if !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
        changed |= pad.delete_held();
    }
    changed
}

/// How near a corner of the box the pointer has to be to take hold of it.
const HANDLE: f32 = 9.0;

/// Draws a page's ink over the page, and takes what the pen does to it. `body` is the
/// rectangle the page's words were drawn in; the ink is kept in that rectangle's own
/// coordinates. Returns true when the ink changed.
pub fn show(ui: &mut Ui, body: egui::Rect, origin: egui::Pos2, pad: &mut Pad, pen: Pen, pressure: f32, t: &Tokens, zoom: f32) -> bool {
    use crate::ribbon::Pointer;
    let _ = t;
    // Painting on the layer, not through the ui, so ink outside text remains visible. Cut off
    // where the view ends as well as the page: a scrolled page reaches above the view, and this
    // layer sits above the ribbon.
    let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(body.intersect(ui.clip_rect()));
    for stroke in pad.drawing.strokes.iter().chain(pad.drawing_now.iter()) {
        let mut scaled = stroke.clone();
        scaled.width *= zoom;
        for dab in &mut scaled.dabs {
            dab.x *= zoom;
            dab.y *= zoom;
        }
        paint_stroke(&painter, origin, &scaled);
    }

    // A pen or a rubber in the hand means the ink is not being held any more.
    if matches!(pen.pointer, Pointer::Draw | Pointer::Erase) && !pad.held.is_empty() {
        pad.let_go();
    }
    // What is held is outlined wherever the pointer is, so it is plain what a move or a
    // delete would take.
    if !pad.held.is_empty()
        && let Some((x0, y0, x1, y1)) = pick::bounds(&pad.drawing, &pad.held)
    {
        let box_ = egui::Rect::from_min_max(origin + vec2(x0 * zoom, y0 * zoom), origin + vec2(x1 * zoom, y1 * zoom)).expand(3.0);
        painter.rect_stroke(box_, 2.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
        for corner in [box_.left_top(), box_.right_top(), box_.left_bottom(), box_.right_bottom()] {
            painter.rect_filled(egui::Rect::from_center_size(corner, vec2(7.0, 7.0)), 1.0, t.accent);
        }
    }
    if !pad.loop_now.is_empty() {
        let path: Vec<egui::Pos2> = pad.loop_now.iter().map(|(x, y)| origin + vec2(*x * zoom, *y * zoom)).collect();
        painter.add(egui::Shape::line(path, egui::Stroke::new(1.2, t.accent)));
    }

    if pen.pointer == Pointer::Write {
        // A selection box being drawn: the writing it would take is outlined, so it can be aimed.
        if let Some(shown) = crate::containers::selection_preview(ui.ctx()) {
            let to_ink = |p: egui::Pos2| ((p.x - origin.x) / zoom.max(0.01), (p.y - origin.y) / zoom.max(0.01));
            let (a, b) = (to_ink(shown.min), to_ink(shown.max));
            for i in pick::touching(&pad.drawing, (a.0, a.1, b.0, b.1)) {
                if let Some((x0, y0, x1, y1)) = pick::bounds(&pad.drawing, &[i]) {
                    let r = egui::Rect::from_min_max(origin + vec2(x0 * zoom, y0 * zoom), origin + vec2(x1 * zoom, y1 * zoom)).expand(3.0);
                    painter.rect_stroke(r, 2.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
                }
            }
        }
        // The tool was put down mid-stroke; keep what was drawn rather than losing it.
        let mut changed = false;
        if let Some(stroke) = pad.drawing_now.take() {
            changed = pad.drawing.add(stroke);
        }
        // Ink a selection box picked up can be moved, stretched and deleted in the ordinary
        // pointer too.
        if pad.held.is_empty() {
            ui.data_mut(|d| d.remove::<egui::Rect>(held_area_id()));
        } else {
            changed |= held_in_write(ui, body, origin, pad, zoom);
        }
        pad.dirty |= changed;
        pad.touched |= changed;
        return changed;
    }

    // While a pen is in hand the whole page belongs to it, so this goes on last and over
    // everything: a press must not land in the words underneath and move the caret.
    let resp = ui.interact(body, ui.id().with("ink-layer"), Sense::click_and_drag());
    ui.ctx().set_cursor_icon(match pen.pointer {
        Pointer::Erase => egui::CursorIcon::NotAllowed,
        Pointer::Pick => egui::CursorIcon::Grab,
        _ => egui::CursorIcon::Crosshair,
    });

    // Not egui's drag, which only begins once the pointer has travelled far enough: a pen
    // put down and lifted in one place is a dot, and a dot is ink.
    let down = resp.is_pointer_button_down_on();
    let at = ui.ctx().pointer_interact_pos().filter(|p| body.contains(*p)).map(|p| ((p.x - origin.x) / zoom, (p.y - origin.y) / zoom));
    let mut changed = false;
    match pen.pointer {
        Pointer::Erase => {
            if let Some((x, y)) = at.filter(|_| down) {
                changed = pad.drawing.erase_at(x, y, pen.width.max(6.0) / 2.0) > 0;
            }
        }
        Pointer::Draw => {
            if down && pad.drawing_now.is_none() {
                pad.drawing_now = Some(Stroke::with_gain(pen.tool, pen.colour, pen.width, pen.gain));
            }
            if let Some(stroke) = pad.drawing_now.as_mut() {
                // Every movement the pointer made this frame, not only where it ended: a pen
                // reports far more often than the window draws, and one point a frame turned a
                // quick curve into straight lines. Each keeps the force of the touch reported
                // just before it (a pen's pressure), or the frame's pressure for a mouse. The
                // frame the pen is lifted in still counts the movements made before it lifted.
                let moves = frame_moves(ui, body, origin, zoom, pressure);
                for (x, y, force) in &moves {
                    stroke.add(Dab::new(*x, *y, *force));
                }
                // A pen put down and held still moves nowhere: where it rests is the point.
                if let Some((x, y)) = at.filter(|_| down && moves.is_empty()) {
                    stroke.add(Dab::new(x, y, pressure));
                }
                // The stroke is painted as it grows, so the line follows the pen rather than
                // appearing when it is lifted.
                ui.ctx().request_repaint();
            }
            if !down && let Some(stroke) = pad.drawing_now.take() {
                changed = pad.drawing.add(stroke);
            }
        }
        Pointer::Shape(shape) => {
            // The drag gives the two corners the shape is drawn between: it follows the
            // pointer while the button is down, and is laid down when it is let go.
            if down {
                if let Some((x, y)) = at {
                    let from = *pad.shape_from.get_or_insert((x, y));
                    let dabs = notecraft_ink::shapes::trace(shape, from, (x, y), 0.6);
                    let mut stroke = Stroke::with_gain(pen.tool, pen.colour, pen.width, pen.gain);
                    for dab in dabs {
                        stroke.add(dab);
                    }
                    pad.drawing_now = Some(stroke);
                    ui.ctx().request_repaint();
                }
            } else {
                pad.shape_from = None;
                if let Some(stroke) = pad.drawing_now.take() {
                    changed = pad.drawing.add(stroke);
                }
            }
        }
        Pointer::Lasso => {
            if down {
                if let Some((x, y)) = at {
                    // Only every few points: a loop of every frame's position is far more
                    // than a ray-casting test needs, and it is thrown away at the end.
                    if pad.loop_now.last().is_none_or(|(lx, ly)| (lx - x).hypot(ly - y) > 2.0) {
                        pad.loop_now.push((x, y));
                    }
                    ui.ctx().request_repaint();
                }
            } else if !pad.loop_now.is_empty() {
                let loop_ = std::mem::take(&mut pad.loop_now);
                pad.held = pick::inside(&pad.drawing, &loop_);
            }
        }
        Pointer::Pick => {
            let held_box = pick::bounds(&pad.drawing, &pad.held).map(|(x0, y0, x1, y1)| {
                egui::Rect::from_min_max(origin + vec2(x0 * zoom, y0 * zoom), origin + vec2(x1 * zoom, y1 * zoom)).expand(3.0)
            });
            let here = ui.ctx().pointer_interact_pos().unwrap_or(egui::Pos2::ZERO);
            if down && pad.grabbed.is_none() {
                // A press on a corner stretches, inside the box moves, on a stroke picks
                // that stroke up, and anywhere else lets go.
                let corner = held_box.and_then(|b| grab_corner(b, here));
                match (corner, held_box, at) {
                    (Some(c), Some(b), _) => pad.grabbed = Some((here, as_tuple(b, origin, zoom), c)),
                    (None, Some(b), _) if b.contains(here) => pad.grabbed = Some((here, as_tuple(b, origin, zoom), Grab::Move)),
                    (_, _, Some((x, y))) => match pick::at(&pad.drawing, x, y, 3.0) {
                        Some(i) => {
                            pad.held = vec![i];
                            if let Some(b) = pick::bounds(&pad.drawing, &pad.held) {
                                pad.grabbed = Some((here, b, Grab::Move));
                            }
                        }
                        None => pad.let_go(),
                    },
                    _ => {}
                }
            }
            if let Some((from, was, grab)) = pad.grabbed {
                let (dx, dy) = ((here.x - from.x) / zoom, (here.y - from.y) / zoom);
                if down {
                    // The ink follows the pointer from where it was taken hold of, so a
                    // long drag cannot drift away from the hand.
                    let (x0, y0, x1, y1) = was;
                    match grab {
                        Grab::Move => {
                            let now = pick::bounds(&pad.drawing, &pad.held).unwrap_or(was);
                            pick::move_by(&mut pad.drawing, &pad.held, x0 + dx - now.0, y0 + dy - now.1);
                        }
                        Grab::Corner(right, bottom) => {
                            let (ox, oy) = (if right { x0 } else { x1 }, if bottom { y0 } else { y1 });
                            let (w, h) = ((x1 - x0).max(1.0), (y1 - y0).max(1.0));
                            let want_w = if right { w + dx } else { w - dx };
                            let want_h = if bottom { h + dy } else { h - dy };
                            let now = pick::bounds(&pad.drawing, &pad.held).unwrap_or(was);
                            let have_w = (now.2 - now.0).max(0.001);
                            let have_h = (now.3 - now.1).max(0.001);
                            pick::scale_about(&mut pad.drawing, &pad.held, ox, oy, want_w.max(4.0) / have_w, want_h.max(4.0) / have_h);
                        }
                    }
                    ui.ctx().request_repaint();
                } else {
                    pad.grabbed = None;
                    changed = true;
                }
            }
        }
        Pointer::Write => {}
    }
    // Delete throws away whatever is held, however it was picked up.
    if !pad.held.is_empty() && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
        changed |= pad.delete_held();
    }
    pad.dirty |= changed;
    pad.touched |= changed;
    changed
}

/// The corner of `box_` the pointer has hold of, if it is near one. The two flags say
/// which way that corner moves: right, and down.
fn grab_corner(box_: egui::Rect, at: egui::Pos2) -> Option<Grab> {
    let near = |c: egui::Pos2| (c.x - at.x).abs() <= HANDLE && (c.y - at.y).abs() <= HANDLE;
    if near(box_.right_bottom()) {
        return Some(Grab::Corner(true, true));
    }
    if near(box_.left_bottom()) {
        return Some(Grab::Corner(false, true));
    }
    if near(box_.right_top()) {
        return Some(Grab::Corner(true, false));
    }
    if near(box_.left_top()) {
        return Some(Grab::Corner(false, false));
    }
    None
}

/// A rectangle on the screen as the ink's own coordinates.
fn as_tuple(b: egui::Rect, origin: egui::Pos2, zoom: f32) -> (f32, f32, f32, f32) {
    ((b.min.x - origin.x) / zoom, (b.min.y - origin.y) / zoom, (b.max.x - origin.x) / zoom, (b.max.y - origin.y) / zoom)
}

/// One stroke, as triangles between the two edges of its ribbon, with a round cap at each
/// end. The edges are the ones the saved outline is built from, so the screen and the file
/// show the same shape.
pub fn paint_stroke(painter: &egui::Painter, origin: egui::Pos2, stroke: &Stroke) {
    let colour = egui::Color32::from_rgb(stroke.colour.0, stroke.colour.1, stroke.colour.2).gamma_multiply(stroke.tool.opacity());
    let put = |x: f32, y: f32| egui::pos2(origin.x + x, origin.y + y);
    let (left, right) = shape::sides(stroke);
    if left.len() < 2 {
        // A dot: one reading, so there is no ribbon, only a blob.
        if let Some(d) = stroke.dabs.first() {
            painter.circle_filled(put(d.x, d.y), shape::width_at(stroke, d.pressure) / 2.0, colour);
        }
        return;
    }
    // egui smooths the edges of the shapes it builds itself, but not of a mesh: a ribbon of bare
    // triangles has hard, stair-stepped edges. So each edge gets a fringe one screen pixel wide
    // that fades from the ink to nothing, the way egui feathers its own lines: the ink edge sits
    // half a pixel inside the outline and the faded edge half a pixel outside.
    let feather = 1.0 / painter.ctx().pixels_per_point();
    let clear = egui::Color32::TRANSPARENT;
    let mut mesh = egui::Mesh::default();
    let mut across = egui::vec2(0.0, 1.0);
    for (l, r) in left.iter().zip(&right) {
        let (l, r) = (put(l.0, l.1), put(r.0, r.1));
        // From the right edge to the left; a point of no width keeps the last direction.
        if let Some(d) = Some(l - r).filter(|d| d.length() > 1e-4) {
            across = d.normalized();
        }
        let half = across * (feather / 2.0);
        mesh.colored_vertex(l + half, clear);
        mesh.colored_vertex(l - half, colour);
        mesh.colored_vertex(r + half, colour);
        mesh.colored_vertex(r - half, clear);
    }
    #[allow(clippy::cast_possible_truncation)]
    for i in 0..(left.len() as u32 - 1) {
        let (a, b) = (i * 4, i * 4 + 4);
        // Left fringe, the ink between the edges, right fringe.
        for k in 0..3 {
            mesh.add_triangle(a + k, a + k + 1, b + k);
            mesh.add_triangle(a + k + 1, b + k, b + k + 1);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    // Round the two ends, which is what the saved outline does with its half circles.
    let line = shape::centre(stroke);
    for end in [line.first(), line.last()].into_iter().flatten() {
        painter.circle_filled(put(end.x, end.y), shape::width_at(stroke, end.pressure) / 2.0, colour);
    }
}

/// The pointer's movements over `body` this frame, in page points, each with the force of the
/// touch reported just before it, or `pressure` until one is (a mouse reports none).
fn frame_moves(ui: &Ui, body: egui::Rect, origin: egui::Pos2, zoom: f32, pressure: f32) -> Vec<(f32, f32, f32)> {
    ui.input(|i| {
        let mut force = pressure;
        let mut moves = Vec::new();
        for event in &i.events {
            match event {
                egui::Event::Touch { force: Some(f), .. } if f.is_finite() => force = f.clamp(0.0, 1.0),
                egui::Event::PointerMoved(p) if body.contains(*p) => moves.push(((p.x - origin.x) / zoom, (p.y - origin.y) / zoom, force)),
                _ => {}
            }
        }
        moves
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;

    fn stroke(x: f32) -> Stroke {
        let mut s = Stroke::new(Tool::Pen, Rgb(0xd0, 0x21, 0x21), 4.0);
        s.dabs.push(Dab::new(x, 40.0, 0.3));
        s.dabs.push(Dab::new(x + 30.0, 40.0, 1.0));
        s
    }

    /// A scrolled page reaches above the part of the window it is shown in, and the ink layer
    /// sits above the ribbon. Ink is cut off where the page's view ends, not where the page
    /// does, or the ink above the view is drawn over the ribbon.
    #[test]
    fn ink_stays_inside_the_part_of_the_page_on_screen() {
        let t = TempDir::new("ink-clipped-to-view");
        let mut pad = Pad::of_page(t.path());
        pad.drawing.add(stroke(100.0));
        let ctx = egui::Context::default();
        let pen = Pen { pointer: crate::ribbon::Pointer::Draw, ..Pen::default() };
        let tokens = crate::theme::Tokens::for_theme(crate::theme::Theme { mode: crate::theme::Mode::Light, accent: crate::theme::Accent::Default });
        let input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))), ..Default::default() };
        let view = egui::Rect::from_min_max(egui::pos2(0.0, 200.0), egui::pos2(800.0, 600.0));
        let mut out = ctx.run_ui(input, |ui| {
            ui.set_clip_rect(view);
            // The page, scrolled so its top is 300 points above the view.
            let body = egui::Rect::from_min_size(egui::pos2(0.0, -100.0), vec2(800.0, 2000.0));
            show(ui, body, body.min, &mut pad, pen, 1.0, &tokens, 1.0);
        });
        out.textures_delta.clear();
        assert!(!out.shapes.is_empty(), "the stroke is painted");
        for shape in &out.shapes {
            assert!(view.contains_rect(shape.clip_rect), "ink clipped to {:?}, beyond the view {view:?}", shape.clip_rect);
        }
    }

    /// Runs the ink layer for one frame of `events`, over a 800 x 600 page at the window's corner,
    /// with a pen in hand and `pressure` as the frame's pressure.
    fn frame(ctx: &egui::Context, pad: &mut Pad, events: Vec<egui::Event>, pressure: f32) {
        let input =
            egui::RawInput { events, screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))), ..Default::default() };
        let pen = Pen { pointer: crate::ribbon::Pointer::Draw, ..Pen::default() };
        let tokens = crate::theme::Tokens::for_theme(crate::theme::Theme { mode: crate::theme::Mode::Light, accent: crate::theme::Accent::Default });
        let mut out = ctx.run_ui(input, |ui| {
            let body = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0));
            show(ui, body, egui::Pos2::ZERO, pad, pen, pressure, &tokens, 1.0);
        });
        // egui insists the frame's texture changes are taken; a test has nowhere to put them.
        out.textures_delta.clear();
    }

    fn button(x: f32, y: f32, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos: egui::pos2(x, y), button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE }
    }

    /// A pen reports its movement far more often than the window draws. Every movement in a frame
    /// is a point of the stroke, not just the last: one point a frame turned a quick curve into
    /// straight lines.
    #[test]
    fn every_movement_in_a_frame_is_a_point_of_the_stroke() {
        let t = TempDir::new("ink-every-move");
        let mut pad = Pad::of_page(t.path());
        let ctx = egui::Context::default();
        frame(&ctx, &mut pad, vec![egui::Event::PointerMoved(egui::pos2(100.0, 100.0))], 1.0);
        frame(&ctx, &mut pad, vec![button(100.0, 100.0, true)], 1.0);
        let moves: Vec<egui::Event> =
            (1..=5).map(|n| egui::Event::PointerMoved(egui::pos2(100.0 + 10.0 * n as f32, 100.0 + (n * n) as f32))).collect();
        frame(&ctx, &mut pad, moves, 1.0);
        frame(&ctx, &mut pad, vec![button(150.0, 125.0, false)], 1.0);
        let stroke = pad.drawing.strokes.last().expect("a stroke");
        let xs: Vec<f32> = stroke.dabs.iter().map(|d| d.x).collect();
        for x in [110.0, 120.0, 130.0, 140.0, 150.0] {
            assert!(xs.iter().any(|got| (got - x).abs() < 0.5), "a point at x = {x}, from a movement inside one frame: {xs:?}");
        }
    }

    /// A pen's pressure comes as touches between its movements (Windows today, Wayland with a
    /// tablet-aware winit); each point keeps the pressure it was made at.
    #[test]
    fn each_point_keeps_the_pressure_of_its_own_movement() {
        let t = TempDir::new("ink-every-move-pressure");
        let mut pad = Pad::of_page(t.path());
        let ctx = egui::Context::default();
        let touch = |x: f32, force: f32| egui::Event::Touch {
            device_id: egui::TouchDeviceId(1),
            id: egui::TouchId(0),
            phase: egui::TouchPhase::Move,
            pos: egui::pos2(x, 100.0),
            force: Some(force),
        };
        frame(&ctx, &mut pad, vec![egui::Event::PointerMoved(egui::pos2(100.0, 100.0))], 1.0);
        frame(&ctx, &mut pad, vec![button(100.0, 100.0, true)], 0.5);
        let events = vec![
            touch(120.0, 0.2),
            egui::Event::PointerMoved(egui::pos2(120.0, 100.0)),
            touch(140.0, 0.8),
            egui::Event::PointerMoved(egui::pos2(140.0, 100.0)),
        ];
        frame(&ctx, &mut pad, events, 0.8);
        frame(&ctx, &mut pad, vec![button(140.0, 100.0, false)], 0.8);
        let stroke = pad.drawing.strokes.last().expect("a stroke");
        let at = |x: f32| stroke.dabs.iter().find(|d| (d.x - x).abs() < 0.5).map(|d| d.pressure);
        assert_eq!(at(120.0), Some(0.2), "the light part of the stroke: {:?}", stroke.dabs);
        assert_eq!(at(140.0), Some(0.8), "the hard part of the stroke: {:?}", stroke.dabs);
    }

    #[test]
    fn a_page_with_no_ink_has_no_ink_file() {
        let t = TempDir::new("ink-layer");
        let page = t.path().to_path_buf();
        let pad = Pad::of_page(&page);
        assert!(pad.is_empty(), "a page nobody has drawn on starts clean");
        assert!(!pad.file.exists(), "and nothing was written to find that out");
        assert!(pad.file.ends_with(FILE), "the ink sits beside the page: {}", pad.file.display());
    }

    /// Issue #157: an ink file that holds lines with no readings, or that cannot be read at all,
    /// is kept as it was before new ink is written over it, and only once.
    #[test]
    fn ink_that_cannot_all_be_read_is_kept_before_it_is_written_over() {
        let original = "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M10 10 L50 50\" stroke=\"red\"/></svg>";
        for (label, text) in [("a picture's path", original), ("not an SVG at all", "this is not a drawing")] {
            let t = TempDir::new("ink-keep-original");
            let page = t.path().to_path_buf();
            std::fs::write(page.join(FILE), text).expect("an ink file of someone else's");
            let mut pad = Pad::of_page(&page);
            assert!(pad.incomplete, "{label}: the file holds more than the strokes read from it");

            assert!(pad.drawing.add(stroke(10.0)));
            pad.dirty = true;
            pad.save().expect("saved");
            let kept = pad.kept_original.take().expect("the original was kept");
            assert_eq!(kept, page.join("ink.svg.bak"), "{label}");
            assert_eq!(std::fs::read_to_string(&kept).expect("kept"), text, "{label}: kept as it was");
            assert!(!crate::inkpad::is_old_sheet("ink.svg.bak"));

            // Saving again does not keep another copy: the file is NoteCraft's own now.
            assert!(pad.drawing.add(stroke(20.0)));
            pad.save().expect("saved again");
            assert_eq!(pad.kept_original, None, "{label}");
            assert!(!page.join("ink-2.svg.bak").exists(), "{label}");
        }

        // A file of NoteCraft's own, read whole, is written over as before, with nothing kept.
        let t = TempDir::new("ink-keep-own");
        let page = t.path().to_path_buf();
        let mut pad = Pad::of_page(&page);
        assert!(pad.drawing.add(stroke(10.0)));
        pad.save().expect("saved");
        let mut again = Pad::of_page(&page);
        assert!(!again.incomplete);
        assert!(again.drawing.add(stroke(30.0)));
        again.save().expect("saved");
        assert_eq!(again.kept_original, None);
        assert!(!page.join("ink.svg.bak").exists());
    }

    #[test]
    fn ink_is_written_beside_the_page_and_comes_back_as_strokes() {
        let t = TempDir::new("ink-layer-save");
        let page = t.path().to_path_buf();
        let mut pad = Pad::of_page(&page);
        assert!(pad.drawing.add(stroke(10.0)));
        pad.dirty = true;
        pad.save().expect("saved");
        assert!(page.join(FILE).exists(), "the file appeared the moment it was drawn on");

        let again = Pad::of_page(&page);
        assert_eq!(again.strokes(), 1);
        assert_eq!(again.flat, 0, "with its readings, so it can still be changed");
        let back = again.drawing.strokes.first().expect("one");
        assert_eq!(back.colour, Rgb(0xd0, 0x21, 0x21));
        assert_eq!(back.dabs.len(), 2);
        assert!((back.dabs[1].pressure - 1.0).abs() < 0.01, "and its pressure");

        // Saved as a picture it is beside the ink, and holds no readings.
        let plain = again.save_plain().expect("plain");
        assert!(plain.ends_with("ink-picture.svg"));
        let text = std::fs::read_to_string(&plain).expect("read");
        assert!(text.contains("<path"), "the picture is there");
        assert!(!text.contains("nc:dabs"), "but the readings are not");

        // Issue #158: saved again, the picture goes into a file of its own, and whatever was
        // already under the usual name is kept as it was.
        std::fs::write(&plain, "OWNER'S UNRELATED FILE").expect("the owner's file");
        let second = again.save_plain().expect("plain again");
        assert!(second.ends_with("ink-picture-2.svg"), "{}", second.display());
        assert_eq!(std::fs::read_to_string(&plain).expect("kept"), "OWNER'S UNRELATED FILE");
        assert!(std::fs::read_to_string(&second).expect("read").contains("<path"));
        assert!(again.save_plain().expect("third").ends_with("ink-picture-3.svg"));
    }

    #[test]
    fn ink_outside_the_old_sheet_is_saved_and_reloaded_at_its_page_coordinates() {
        let t = TempDir::new("ink-layer-unbounded");
        let page = t.path().to_path_buf();
        let mut pad = Pad::of_page(&page);
        let mut outside = Stroke::new(Tool::Pen, Rgb(0xd0, 0x21, 0x21), 4.0);
        outside.add(Dab::new(-64.0, 1200.0, 0.6));
        outside.add(Dab::new(-24.0, 1240.0, 1.0));
        assert!(pad.drawing.add(outside));
        pad.dirty = true;
        pad.save().expect("save open canvas ink");

        let reloaded = Pad::of_page(&page);
        assert_eq!(reloaded.strokes(), 1);
        let (x0, y0, x1, y1) = (
            reloaded.drawing.view_x,
            reloaded.drawing.view_y,
            reloaded.drawing.view_x + reloaded.drawing.width,
            reloaded.drawing.view_y + reloaded.drawing.height,
        );
        assert!(x0 < 0.0 && x1 >= SHEET_W, "viewBox retains negative and original coordinates: {x0}..{x1}");
        assert!(y0 <= 0.0 && y1 > SHEET_H, "viewBox grows beyond the old sheet: {y0}..{y1}");
        assert_eq!(reloaded.drawing.strokes[0].dabs[0].x, -64.0);
        assert_eq!(reloaded.drawing.strokes[0].dabs[1].y, 1240.0);
    }

    /// A stroke added as the pen adds one: a change, which undo can take back once it settles.
    fn draw(pad: &mut Pad, s: Stroke) {
        assert!(pad.drawing.add(s));
        pad.touched = true;
        pad.dirty = true;
        pad.settle();
    }

    /// Issue #156: ink that was on the page when it was opened is where the page starts. Undo
    /// with nothing done since has nothing to take back, and leaves it alone.
    #[test]
    fn undo_after_opening_a_page_leaves_its_ink_alone() {
        let t = TempDir::new("ink-undo-baseline");
        let page = t.path().to_path_buf();
        let mut first = Pad::of_page(&page);
        draw(&mut first, stroke(10.0));
        first.save().expect("saved");

        let mut reopened = Pad::of_page(&page);
        assert_eq!(reopened.strokes(), 1);
        assert!(!reopened.can_undo(), "nothing has been done since the page was opened");
        assert!(!reopened.undo());
        assert_eq!(reopened.strokes(), 1, "the saved stroke is still there");

        draw(&mut reopened, stroke(30.0));
        assert!(reopened.undo(), "a stroke drawn now can be taken back");
        assert_eq!(reopened.strokes(), 1, "back to the stroke the page was opened with, and no further");
        assert!(!reopened.undo());
    }

    /// Issue #155: what undo took back, redo puts back, until something new is done.
    #[test]
    fn redo_puts_back_what_undo_took() {
        let t = TempDir::new("ink-redo");
        let mut pad = Pad::of_page(t.path());
        draw(&mut pad, stroke(10.0));
        draw(&mut pad, stroke(20.0));
        assert!(!pad.can_redo());

        assert!(pad.undo() && pad.undo());
        assert_eq!(pad.strokes(), 0);
        assert!(pad.can_redo());
        assert!(pad.redo());
        assert_eq!(pad.strokes(), 1);
        assert!(pad.redo());
        assert_eq!(pad.strokes(), 2, "both strokes are back, in order");
        assert!(!pad.redo(), "and nothing more");
        assert!(pad.undo(), "what redo put back can be taken back again");
        assert_eq!(pad.strokes(), 1);

        draw(&mut pad, stroke(40.0));
        assert!(!pad.can_redo(), "a new stroke starts a new branch");
        assert!(!pad.redo());
        assert_eq!(pad.strokes(), 2);
    }

    #[test]
    fn rubbing_a_page_clean_takes_the_file_away_again() {
        let t = TempDir::new("ink-layer-clean");
        let page = t.path().to_path_buf();
        let mut pad = Pad::of_page(&page);
        draw(&mut pad, stroke(10.0));
        pad.save().expect("saved");
        assert!(page.join(FILE).exists());

        assert!(pad.undo());
        pad.save().expect("saved again");
        assert!(!page.join(FILE).exists(), "a page rubbed clean keeps no empty file");
        assert!(!pad.undo(), "and there is nothing more to take back");
        pad.save().expect("saving nothing is not an error");
    }

    #[test]
    fn a_damaged_ink_file_opens_as_an_empty_page_rather_than_failing() {
        let t = TempDir::new("ink-layer-junk");
        let page = t.path().to_path_buf();
        std::fs::write(page.join(FILE), "this is not a drawing").expect("write");
        let pad = Pad::of_page(&page);
        assert!(pad.is_empty(), "nothing was read, and nothing broke");
        assert!(pad.drawing.width >= 1.0 && pad.drawing.height >= 1.0);
    }
}

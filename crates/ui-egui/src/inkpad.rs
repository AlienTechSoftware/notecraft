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
    /// The stroke being drawn now, if the pen is down.
    drawing_now: Option<Stroke>,
    /// The strokes being held, by their place in the drawing. Never written to the file:
    /// it is what someone has their hand on at this moment, not part of the page.
    held: Vec<usize>,
    /// The loop being drawn now by the lasso.
    loop_now: Vec<(f32, f32)>,
    /// Where the pointer was when a move or a stretch began, and the box as it was then.
    grabbed: Option<(egui::Pos2, (f32, f32, f32, f32), Grab)>,
}

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
        let (drawing, flat) =
            std::fs::read_to_string(&file).ok().and_then(|text| svg::read(&text).ok()).unwrap_or_else(|| (Drawing::new(SHEET_W, SHEET_H), 0));
        Pad { file, drawing, dirty: false, flat, drawing_now: None, held: Vec::new(), loop_now: Vec::new(), grabbed: None }
    }

    /// Writes the ink, with its readings, which is what the page keeps.
    pub fn save(&mut self) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
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

    /// Writes it again as a plain picture, beside the ink, for sending somewhere else.
    /// Returns where it was written.
    pub fn save_plain(&self) -> std::io::Result<PathBuf> {
        let plain = self.file.with_file_name("ink-picture.svg");
        if let Some(parent) = plain.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&plain, svg::write(&self.drawing, svg::Flavour::Plain))?;
        Ok(plain)
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

    /// Takes the last stroke back.
    pub fn undo(&mut self) -> bool {
        let went = self.drawing.undo();
        self.dirty |= went;
        went
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

    /// Throws away the held ink and says whether any went.
    pub fn delete_held(&mut self) -> bool {
        let went = pick::remove(&mut self.drawing, &self.held) > 0;
        self.let_go();
        self.dirty |= went;
        went
    }
}

/// How near a corner of the box the pointer has to be to take hold of it.
const HANDLE: f32 = 9.0;

/// Draws a page's ink over the page, and takes what the pen does to it. `body` is the
/// rectangle the page's words were drawn in; the ink is kept in that rectangle's own
/// coordinates. Returns true when the ink changed.
pub fn show(ui: &mut Ui, body: egui::Rect, pad: &mut Pad, pen: Pen, pressure: f32, t: &Tokens) -> bool {
    use crate::ribbon::Pointer;
    let _ = t;
    // Painting on the layer, not through the ui, so a stroke that runs past the words is
    // still drawn: the ui's own box ends where the text does.
    let painter = ui.ctx().layer_painter(ui.layer_id()).with_clip_rect(body);
    for stroke in pad.drawing.strokes.iter().chain(pad.drawing_now.iter()) {
        paint_stroke(&painter, body.min, stroke);
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
        let box_ = egui::Rect::from_min_max(body.min + vec2(x0, y0), body.min + vec2(x1, y1)).expand(3.0);
        painter.rect_stroke(box_, 2.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Outside);
        for corner in [box_.left_top(), box_.right_top(), box_.left_bottom(), box_.right_bottom()] {
            painter.rect_filled(egui::Rect::from_center_size(corner, vec2(7.0, 7.0)), 1.0, t.accent);
        }
    }
    if !pad.loop_now.is_empty() {
        let path: Vec<egui::Pos2> = pad.loop_now.iter().map(|(x, y)| body.min + vec2(*x, *y)).collect();
        painter.add(egui::Shape::line(path, egui::Stroke::new(1.2, t.accent)));
    }

    if pen.pointer == Pointer::Write {
        pad.let_go();
        // The tool was put down mid-stroke; keep what was drawn rather than losing it.
        if let Some(stroke) = pad.drawing_now.take() {
            let kept = pad.drawing.add(stroke);
            pad.dirty |= kept;
            return kept;
        }
        return false;
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
    let at = ui.ctx().pointer_interact_pos().filter(|p| body.contains(*p)).map(|p| (p.x - body.min.x, p.y - body.min.y));
    let mut changed = false;
    match pen.pointer {
        Pointer::Erase => {
            if let Some((x, y)) = at.filter(|_| down) {
                changed = pad.drawing.erase_at(x, y, pen.width.max(6.0) / 2.0) > 0;
            }
        }
        Pointer::Draw => {
            if down {
                if pad.drawing_now.is_none() {
                    pad.drawing_now = Some(Stroke::with_gain(pen.tool, pen.colour, pen.width, pen.gain));
                }
                if let (Some(stroke), Some((x, y))) = (pad.drawing_now.as_mut(), at) {
                    stroke.add(Dab::new(x, y, pressure));
                    // The stroke is painted as it grows, so the line follows the pen
                    // rather than appearing when it is lifted.
                    ui.ctx().request_repaint();
                }
            } else if let Some(stroke) = pad.drawing_now.take() {
                changed = pad.drawing.add(stroke);
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
            let held_box = pick::bounds(&pad.drawing, &pad.held)
                .map(|(x0, y0, x1, y1)| egui::Rect::from_min_max(body.min + vec2(x0, y0), body.min + vec2(x1, y1)).expand(3.0));
            let here = ui.ctx().pointer_interact_pos().unwrap_or(egui::Pos2::ZERO);
            if down && pad.grabbed.is_none() {
                // A press on a corner stretches, inside the box moves, on a stroke picks
                // that stroke up, and anywhere else lets go.
                let corner = held_box.and_then(|b| grab_corner(b, here));
                match (corner, held_box, at) {
                    (Some(c), Some(b), _) => pad.grabbed = Some((here, as_tuple(b, body), c)),
                    (None, Some(b), _) if b.contains(here) => pad.grabbed = Some((here, as_tuple(b, body), Grab::Move)),
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
                let (dx, dy) = (here.x - from.x, here.y - from.y);
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
fn as_tuple(b: egui::Rect, body: egui::Rect) -> (f32, f32, f32, f32) {
    (b.min.x - body.min.x, b.min.y - body.min.y, b.max.x - body.min.x, b.max.y - body.min.y)
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
    let mut mesh = egui::Mesh::default();
    for (l, r) in left.iter().zip(&right) {
        mesh.colored_vertex(put(l.0, l.1), colour);
        mesh.colored_vertex(put(r.0, r.1), colour);
    }
    #[allow(clippy::cast_possible_truncation)]
    for i in 0..(left.len() as u32 - 1) {
        let (a, b, c, d) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, c, d);
    }
    painter.add(egui::Shape::mesh(mesh));
    // Round the two ends, which is what the saved outline does with its half circles.
    let line = shape::centre(stroke);
    for end in [line.first(), line.last()].into_iter().flatten() {
        painter.circle_filled(put(end.x, end.y), shape::width_at(stroke, end.pressure) / 2.0, colour);
    }
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

    #[test]
    fn a_page_with_no_ink_has_no_ink_file() {
        let t = TempDir::new("ink-layer");
        let page = t.path().to_path_buf();
        let pad = Pad::of_page(&page);
        assert!(pad.is_empty(), "a page nobody has drawn on starts clean");
        assert!(!pad.file.exists(), "and nothing was written to find that out");
        assert!(pad.file.ends_with(FILE), "the ink sits beside the page: {}", pad.file.display());
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
    }

    #[test]
    fn rubbing_a_page_clean_takes_the_file_away_again() {
        let t = TempDir::new("ink-layer-clean");
        let page = t.path().to_path_buf();
        let mut pad = Pad::of_page(&page);
        pad.drawing.add(stroke(10.0));
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

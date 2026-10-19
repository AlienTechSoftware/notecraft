//! A sheet of the page that takes ink: drawn on screen, written to a file beside the page.
//!
//! A drawing is an ordinary picture as far as the page is concerned. It is an SVG file in
//! the page's own folder, and the page body holds `![Drawing](assets/drawing-1.svg)` like
//! any other image, so a page with a drawing in it opens in anything, and nothing about
//! either page format had to change to allow ink.
//!
//! What makes it more than a picture is that the file also carries the pen's own readings
//! (`notecraft_ink::svg`), so when NoteCraft opens that page again the strokes come back as
//! strokes and can be rubbed out or added to. Anything else sees the picture.
//!
//! The strokes are drawn as triangles from the same two edges the SVG's outline is built
//! from, so what is on the screen and what is in the file are the same shape.

use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};
use notecraft_ink::{Dab, Drawing, Rgb, Stroke, Tool, shape, svg};
use std::path::{Path, PathBuf};

/// What the pen is set to do, as the Draw tab has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pen {
    pub pointer: crate::ribbon::Pointer,
    pub tool: Tool,
    pub colour: Rgb,
    /// In points.
    pub width: f32,
}

impl Default for Pen {
    fn default() -> Self {
        Self { pointer: crate::ribbon::Pointer::Write, tool: Tool::Pen, colour: Rgb::BLACK, width: 3.0 }
    }
}

/// How big a new drawing is, in points.
pub const NEW_WIDTH: f32 = 720.0;
pub const NEW_HEIGHT: f32 = 300.0;

/// A drawing open in a page, and the file it came from.
#[derive(Clone, Debug)]
pub struct Pad {
    /// The source as the page body writes it, for matching the block it belongs to.
    pub src: String,
    /// Where the file is on disk.
    pub file: PathBuf,
    pub drawing: Drawing,
    /// Changed since it was last written.
    pub dirty: bool,
    /// How many strokes in the file were a picture only, with no readings to give back.
    pub flat: usize,
    /// The stroke being drawn now, if the pen is down.
    drawing_now: Option<Stroke>,
}

/// The name a new drawing gets in a page's folder, avoiding any already there.
pub fn next_name(page_dir: &Path) -> String {
    for n in 1..1000 {
        let name = format!("assets/drawing-{n}.svg");
        if !page_dir.join("assets").join(format!("drawing-{n}.svg")).exists() {
            return name;
        }
    }
    "assets/drawing.svg".to_string()
}

/// Is this what the page wrote for a drawing of ours?
pub fn is_drawing(src: &str) -> bool {
    let src = src.trim();
    !src.contains("://") && src.to_ascii_lowercase().ends_with(".svg")
}

impl Pad {
    /// Reads the drawing a page's image block names. `None` when the file is not there or
    /// is not an SVG at all; a file that is SVG but holds no readings opens as an empty
    /// sheet with `flat` saying how many strokes could not come back.
    pub fn open(page_dir: &Path, src: &str) -> Option<Pad> {
        let file = file_of(page_dir, src)?;
        let text = std::fs::read_to_string(&file).ok()?;
        let (drawing, flat) = svg::read(&text).ok()?;
        Some(Pad { src: src.to_string(), file, drawing, dirty: false, flat, drawing_now: None })
    }

    /// Makes a new, empty drawing and writes it, so the page can point at a real file.
    pub fn create(page_dir: &Path, src: &str) -> std::io::Result<Pad> {
        let file = file_of(page_dir, src).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a drawing of ours"))?;
        let mut pad = Pad { src: src.to_string(), file, drawing: Drawing::new(NEW_WIDTH, NEW_HEIGHT), dirty: true, flat: 0, drawing_now: None };
        pad.save()?;
        Ok(pad)
    }

    /// Writes the drawing with its readings, which is what a page keeps.
    pub fn save(&mut self) -> std::io::Result<()> {
        self.write(svg::Flavour::Editable)?;
        self.dirty = false;
        Ok(())
    }

    /// Writes it again as a plain picture, beside the first, for sending somewhere else.
    /// Returns where it was written.
    pub fn save_plain(&self) -> std::io::Result<PathBuf> {
        let plain = self.file.with_extension("plain.svg");
        if let Some(parent) = plain.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&plain, svg::write(&self.drawing, svg::Flavour::Plain))?;
        Ok(plain)
    }

    fn write(&self, flavour: svg::Flavour) -> std::io::Result<()> {
        if let Some(parent) = self.file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.file, svg::write(&self.drawing, flavour))
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
}

/// The file an image source names, as long as it stays inside the page's own folder. A
/// source that climbs out with `..`, or names another drive, is refused.
fn file_of(page_dir: &Path, src: &str) -> Option<PathBuf> {
    let src = src.trim();
    if !is_drawing(src) {
        return None;
    }
    let rel = Path::new(src);
    if rel.is_absolute()
        || rel.components().any(|c| matches!(c, std::path::Component::ParentDir | std::path::Component::Prefix(_) | std::path::Component::RootDir))
    {
        return None;
    }
    Some(page_dir.join(rel))
}

/// Draws the pad and takes what the pen does to it. Returns true when the drawing changed.
///
/// `pressure` is what the stylus reports for this frame; a mouse gives 1.
pub fn show(ui: &mut Ui, pad: &mut Pad, pen: Pen, pressure: f32, t: &Tokens) -> bool {
    use crate::ribbon::Pointer;
    let sheet = pad.drawing.width.max(1.0);
    let room = ui.available_width().max(80.0);
    // The sheet is shown whole, never wider than the page: one scale for both directions,
    // so nothing is squashed.
    let scale = (room / sheet).min(1.0);
    let size = vec2(pad.drawing.width * scale, pad.drawing.height * scale);
    let inking = pen.pointer != Pointer::Write;
    let (rect, resp) = ui.allocate_exact_size(size, if inking { Sense::drag() } else { Sense::click() });
    let enabled = ui.is_enabled();
    let said = format!("Drawing, {} strokes", pad.strokes());
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Image, enabled, &said));

    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, t.editor);
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, if inking { t.accent } else { t.faint }), egui::StrokeKind::Inside);

    let mut changed = false;
    if inking {
        ui.ctx().set_cursor_icon(if pen.pointer == Pointer::Erase { egui::CursorIcon::NotAllowed } else { egui::CursorIcon::Crosshair });
        // Not egui's drag, which only begins once the pointer has travelled far enough: a
        // pen that is put down and lifted again in one place is a dot, and a dot is ink.
        // What matters is whether the button went down on this sheet and is still down.
        let down = resp.is_pointer_button_down_on();
        let at = ui.ctx().pointer_interact_pos().filter(|p| rect.contains(*p)).map(|p| ((p.x - rect.min.x) / scale, (p.y - rect.min.y) / scale));
        match pen.pointer {
            Pointer::Erase => {
                if let Some((x, y)) = at.filter(|_| down) {
                    changed |= pad.drawing.erase_at(x, y, pen.width.max(6.0) / 2.0) > 0;
                }
            }
            Pointer::Draw => {
                if down {
                    if pad.drawing_now.is_none() {
                        pad.drawing_now = Some(Stroke::new(pen.tool, pen.colour, pen.width));
                    }
                    if let (Some(stroke), Some((x, y))) = (pad.drawing_now.as_mut(), at) {
                        stroke.add(Dab::new(x, y, pressure));
                    }
                } else if let Some(stroke) = pad.drawing_now.take() {
                    changed |= pad.drawing.add(stroke);
                }
            }
            Pointer::Write => {}
        }
        pad.dirty |= changed;
    } else if let Some(stroke) = pad.drawing_now.take() {
        // The tool was put down mid-stroke; keep what was drawn rather than losing it.
        changed |= pad.drawing.add(stroke);
        pad.dirty |= changed;
    }

    for stroke in pad.drawing.strokes.iter().chain(pad.drawing_now.iter()) {
        paint_stroke(&painter, rect.min, scale, stroke);
    }
    if pad.drawing.is_empty() && pad.drawing_now.is_none() {
        let say = if pad.flat > 0 {
            "This drawing is a picture only: its strokes cannot be changed here"
        } else {
            "Pick a pen in the Draw tab and write here"
        };
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, say, egui::FontId::proportional(12.0), t.muted);
    }
    changed
}

/// One stroke, as triangles between the two edges of its ribbon, with a round cap at each
/// end. The edges are the ones the saved outline is built from, so the screen and the file
/// show the same shape.
fn paint_stroke(painter: &egui::Painter, origin: egui::Pos2, scale: f32, stroke: &Stroke) {
    let colour = egui::Color32::from_rgb(stroke.colour.0, stroke.colour.1, stroke.colour.2).gamma_multiply(stroke.tool.opacity());
    let put = |x: f32, y: f32| egui::pos2(origin.x + x * scale, origin.y + y * scale);
    let (left, right) = shape::sides(stroke);
    if left.len() < 2 {
        // A dot: one reading, so there is no ribbon, only a blob.
        if let Some(d) = stroke.dabs.first() {
            painter.circle_filled(put(d.x, d.y), shape::width_at(stroke, d.pressure) / 2.0 * scale, colour);
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
        painter.circle_filled(put(end.x, end.y), shape::width_at(stroke, end.pressure) / 2.0 * scale, colour);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;

    #[test]
    fn only_a_drawing_of_ours_is_one() {
        assert!(is_drawing("assets/drawing-1.svg"));
        assert!(is_drawing("assets/Drawing-1.SVG"), "however it is spelled");
        assert!(!is_drawing("assets/koala.png"));
        assert!(!is_drawing("https://example.test/a.svg"), "not one from the web");
        assert!(!is_drawing(""));
    }

    #[test]
    fn a_source_cannot_reach_outside_the_pages_folder() {
        let dir = Path::new("/notebooks/page");
        assert!(file_of(dir, "assets/drawing-1.svg").is_some());
        for bad in ["../elsewhere/drawing.svg", "assets/../../drawing.svg", "/etc/drawing.svg", "C:/windows/drawing.svg"] {
            assert!(file_of(dir, bad).is_none(), "{bad} was allowed");
        }
    }

    #[test]
    fn a_drawing_is_made_drawn_in_saved_and_opened_again() {
        let t = TempDir::new("inkpad");
        let page = t.path().join("page");
        std::fs::create_dir_all(&page).expect("page folder");
        let src = next_name(&page);
        assert_eq!(src, "assets/drawing-1.svg");

        let mut pad = Pad::create(&page, &src).expect("made");
        assert!(pad.file.exists(), "the file is written at once, so the page can point at it");
        assert_eq!(pad.strokes(), 0);

        let mut stroke = Stroke::new(Tool::Pen, Rgb(0xd0, 0x21, 0x21), 4.0);
        for (x, p) in [(10.0, 0.3), (40.0, 0.8), (70.0, 1.0)] {
            stroke.dabs.push(Dab::new(x, 40.0, p));
        }
        assert!(pad.drawing.add(stroke));
        pad.dirty = true;
        pad.save().expect("saved");
        assert!(!pad.dirty);

        let again = Pad::open(&page, &src).expect("opened");
        assert_eq!(again.strokes(), 1, "the stroke came back as a stroke");
        assert_eq!(again.flat, 0);
        let back = again.drawing.strokes.first().expect("one");
        assert_eq!(back.colour, Rgb(0xd0, 0x21, 0x21));
        assert_eq!(back.dabs.len(), 3, "with its readings, so it can still be changed");
        assert!((back.dabs[2].pressure - 1.0).abs() < 0.01, "and its pressure");

        // The second drawing in the same page gets its own name.
        assert_eq!(next_name(&page), "assets/drawing-2.svg");

        // Saved as a picture it is beside the first, and holds no readings.
        let plain = again.save_plain().expect("plain");
        let text = std::fs::read_to_string(&plain).expect("read");
        assert!(text.contains("<path"), "the picture is there");
        assert!(!text.contains("nc:dabs"), "but the readings are not");
    }

    #[test]
    fn opening_what_is_not_there_or_not_ours_gives_nothing() {
        let t = TempDir::new("inkpad-missing");
        let page = t.path().to_path_buf();
        assert!(Pad::open(&page, "assets/drawing-1.svg").is_none(), "no file");
        assert!(Pad::open(&page, "../x.svg").is_none(), "outside the folder");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets/junk.svg"), "not svg at all").expect("write");
        assert!(Pad::open(&page, "assets/junk.svg").is_none(), "not a drawing");
    }

    #[test]
    fn undo_takes_the_last_stroke_back() {
        let t = TempDir::new("inkpad-undo");
        let page = t.path().to_path_buf();
        let mut pad = Pad::create(&page, "assets/drawing-1.svg").expect("made");
        let mut s = Stroke::new(Tool::Pen, Rgb::BLACK, 3.0);
        s.dabs.push(Dab::new(1.0, 1.0, 1.0));
        s.dabs.push(Dab::new(9.0, 9.0, 1.0));
        pad.drawing.add(s);
        assert!(pad.undo());
        assert_eq!(pad.strokes(), 0);
        assert!(!pad.undo(), "and there is nothing more to take back");
    }
}

//! The page as a sheet of containers, the level between the page and its paragraphs.
//!
//! OneNote puts boxes on a page and the writer puts them where they like. NoteCraft keeps
//! the blocks themselves in reading order in the page's own file, and the boxes are a layer
//! over them: a place, a width, and how many blocks each one holds
//! ([`notecraft_model::container`]). So a page is still the same document anywhere else,
//! and the arrangement travels in the page's metadata.
//!
//! What this module does: draws each container where it belongs, lets one be moved by its
//! handle and resized by its right edge, and says when either changed so the page can be
//! written back.

use crate::pageedit::{self, Caret, Changed};
use crate::theme::Tokens;
use egui::{Rect, Sense, Ui, pos2, vec2};
use notecraft_formats::markdown::Block;
use notecraft_model::{Container, container};

/// The height of the bar at the top of a container, which is what it is dragged by. Wide
/// enough to hit without aiming: a thin strip is what makes dragging feel unreliable.
const HANDLE: f32 = 14.0;
/// How wide the strip on the right edge is, for resizing. Same reason.
const GRIP: f32 = 10.0;
/// The gap between a container's edge and the words inside it.
const PAD: f32 = 6.0;

/// What a frame of the canvas did.
#[derive(Default)]
pub struct Moved {
    pub changed: Changed,
    /// True when a container was moved, resized, added or removed.
    pub boxes: bool,
    /// The top left of the page's own area, so a script can work out where a box is drawn.
    pub origin: egui::Pos2,
}

/// Draws the page as containers. `boxes` is adjusted in place when one is dragged, resized
/// or added, and when the blocks inside one are split or joined.
pub fn show(
    ui: &mut Ui,
    blocks: &mut Vec<Block>,
    boxes: &mut Vec<Container>,
    caret: &mut Option<Caret>,
    page_dir: &std::path::Path,
    t: &Tokens,
    page: notecraft_model::Dir,
    borders: bool,
) -> Moved {
    let mut out = Moved::default();
    let area = ui.available_rect_before_wrap();
    out.origin = area.min;
    *boxes = container::fit(boxes, blocks.len(), (area.width() - 2.0 * PAD).max(container::MIN_WIDTH));

    // Which container the caret is in, so that a block added or removed inside it is
    // counted against the right one.
    let caret_box = caret.as_ref().and_then(|c| container::place(boxes, c.block)).map(|(i, _)| i);
    let before = blocks.len();
    let mut bottom = area.min.y;

    for i in 0..boxes.len() {
        let Some(b) = boxes.get(i).copied() else { continue };
        let range = container::blocks_of(boxes, i);
        let at = pos2(area.min.x + b.x, area.min.y + b.y);
        let editing = caret.as_ref().is_some_and(|c| range.contains(&c.block));

        // The blocks of this container, drawn in a column of its own width.
        let inner = Rect::from_min_size(pos2(at.x + PAD, at.y + HANDLE), vec2(b.width, 0.0));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(inner.min, vec2(b.width, area.height()))).layout(*ui.layout()));
        child.set_width(b.width);
        let changed = pageedit::show_blocks(&mut child, blocks, caret, page_dir, t, page, range.clone(), borders);
        out.changed.blocks |= changed.blocks;
        out.changed.caret |= changed.caret;
        if changed.link.is_some() {
            out.changed.link = changed.link;
        }
        let used = child.min_rect().height().max(18.0);
        let frame = Rect::from_min_size(at, vec2(b.width + 2.0 * PAD, used + HANDLE + PAD));
        bottom = bottom.max(frame.max.y);

        let hovered = ui.rect_contains_pointer(frame);
        // The handle along the top and the grip down the right edge. Both widgets exist on
        // every frame, whether or not they are drawn: a widget that comes and goes with the
        // pointer loses the drag the moment the pointer leaves it, which is what made
        // moving a box feel like it slipped out of the hand.
        let bar = Rect::from_min_size(frame.min, vec2(frame.width(), HANDLE));
        let drag = ui.interact(bar, ui.id().with(("container-move", i)), Sense::drag()).on_hover_cursor(egui::CursorIcon::Move);
        let edge = Rect::from_min_size(pos2(frame.max.x - GRIP, frame.min.y + HANDLE), vec2(GRIP, (frame.height() - HANDLE).max(GRIP)));
        let grip = ui.interact(edge, ui.id().with(("container-size", i)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

        // Drawn only when the pointer is in the container, the caret is in it, or it is
        // being dragged, the way a container shows its edges in OneNote.
        let busy = drag.dragged() || grip.dragged();
        if hovered || editing || busy {
            ui.painter().rect_stroke(frame, 3.0, egui::Stroke::new(1.0, t.faint), egui::StrokeKind::Inside);
            ui.painter().rect_filled(bar, 3.0, if drag.hovered() || drag.dragged() { t.row_hover } else { t.faint });
            for n in 0..3 {
                let c = pos2(bar.center().x + (n as f32 - 1.0) * 5.0, bar.center().y);
                ui.painter().circle_filled(c, 1.2, t.muted);
            }
            if grip.hovered() || grip.dragged() {
                ui.painter().rect_filled(edge.shrink2(vec2(2.0, 4.0)), 2.0, t.row_hover);
            }
        }
        if drag.dragged() {
            let d = drag.drag_delta();
            if let Some(c) = boxes.get_mut(i) {
                *c = Container::new(c.x + d.x, c.y + d.y, c.width, c.blocks);
            }
            out.boxes = true;
        }
        if grip.dragged() {
            if let Some(c) = boxes.get_mut(i) {
                *c = Container::new(c.x, c.y, c.width + grip.drag_delta().x, c.blocks);
            }
            out.boxes = true;
        }
    }

    // A block was added or taken away inside a container: it belongs to that one.
    let now = blocks.len();
    if now != before {
        let which = caret_box.unwrap_or(0).min(boxes.len().saturating_sub(1));
        if let Some(c) = boxes.get_mut(which) {
            let grew = now as i64 - before as i64;
            c.blocks = (c.blocks as i64 + grew).max(0) as usize;
        }
        out.boxes = true;
    }

    // The containers were drawn in child uis, which do not move this ui's cursor. Claim the
    // space they took, so the page scrolls to their bottom and so the room under them really
    // is under them: allocated at the cursor it would sit on top of the first container and
    // take every click meant for a block.
    ui.allocate_rect(Rect::from_min_max(area.min, pos2(area.max.x, bottom)), Sense::hover());
    // Bare page: a click starts a box of its own where the pointer is, which is how
    // OneNote treats a click on open paper - the words begin where you pointed, not at
    // the end of whatever was written last.
    let rest = (area.max.y - bottom).max(40.0);
    let below = ui.allocate_rect(Rect::from_min_size(pos2(area.min.x, bottom), vec2(area.width(), rest.max(260.0))), Sense::click());
    if below.clicked() {
        out.changed.caret = true;
        match below.interact_pointer_pos() {
            Some(at) => {
                let where_ = pos2((at.x - area.min.x - PAD).max(0.0), (at.y - area.min.y - HANDLE).max(0.0));
                start_a_box(blocks, boxes, caret, where_, (area.width() - 2.0 * PAD).max(container::MIN_WIDTH));
                out.boxes = true;
                out.changed.blocks = true;
            }
            None => {
                pageedit::click_below(blocks, caret);
            }
        }
    }
    if below.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
    out
}

/// Starts an empty box at `at`, in the page's own coordinates, and puts the caret in it.
///
/// The block goes on the end of the page's blocks, because the file keeps its blocks in
/// reading order whatever the boxes do with them; the box that holds it is placed where
/// the pointer was.
fn start_a_box(blocks: &mut Vec<Block>, boxes: &mut Vec<Container>, caret: &mut Option<Caret>, at: egui::Pos2, room: f32) {
    // A box of its own needs a width that leaves space to its right, so the page does not
    // turn into one column the moment somebody clicks near the middle.
    let width = (room - at.x).clamp(container::MIN_WIDTH, room).min(360.0_f32.max(container::MIN_WIDTH));
    blocks.push(Block::Paragraph(Vec::new()));
    boxes.push(Container { x: at.x, y: at.y, width, blocks: 1 });
    *caret =
        Some(pageedit::Caret { block: blocks.len() - 1, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;
    use notecraft_formats::markdown;

    fn page(md: &str) -> Vec<Block> {
        markdown::parse(md)
    }

    #[test]
    fn a_page_with_no_arrangement_is_one_container() {
        let t = Theme::DEFAULT.tokens();
        let mut blocks = page("one\n\ntwo\n\nthree\n");
        let mut boxes: Vec<Container> = Vec::new();
        let mut caret = None;
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            let out = show(ui, &mut blocks, &mut boxes, &mut caret, std::path::Path::new("."), &t, notecraft_model::Dir::Ltr, true);
            assert!(!out.boxes, "nothing was dragged");
        });
        frame.textures_delta.clear();
        assert_eq!(boxes.len(), 1, "one box holding the page");
        assert_eq!(boxes.first().map(|b| b.blocks), Some(3));
    }

    #[test]
    fn the_blocks_of_each_container_are_the_ones_it_says() {
        let t = Theme::DEFAULT.tokens();
        let mut blocks = page("one\n\ntwo\n\nthree\n");
        let mut boxes = vec![Container::new(0.0, 0.0, 200.0, 1), Container::new(220.0, 0.0, 200.0, 2)];
        let mut caret = None;
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            show(ui, &mut blocks, &mut boxes, &mut caret, std::path::Path::new("."), &t, notecraft_model::Dir::Ltr, true);
        });
        frame.textures_delta.clear();
        // The arrangement is kept as it was, because it accounts for every block.
        assert_eq!(boxes.len(), 2);
        assert_eq!(container::blocks_of(&boxes, 0), 0..1);
        assert_eq!(container::blocks_of(&boxes, 1), 1..3);
    }

    #[test]
    fn an_arrangement_that_does_not_match_the_page_is_not_trusted() {
        let t = Theme::DEFAULT.tokens();
        let mut blocks = page("one\n\ntwo\n\nthree\n\nfour\n");
        // Three blocks accounted for, four on the page: the page was edited elsewhere.
        let mut boxes = vec![Container::new(0.0, 0.0, 200.0, 1), Container::new(220.0, 0.0, 200.0, 2)];
        let mut caret = None;
        let ctx = egui::Context::default();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            show(ui, &mut blocks, &mut boxes, &mut caret, std::path::Path::new("."), &t, notecraft_model::Dir::Ltr, true);
        });
        frame.textures_delta.clear();
        assert_eq!(boxes.len(), 1, "everything in one box rather than losing a block");
        assert_eq!(boxes.first().map(|b| b.blocks), Some(4));
    }
}

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
//!
//! A container can also be selected, by a click on its handle or a right click anywhere in it.
//! Selecting it selects everything in it: the whole box is shaded, and Delete (the key, or the
//! item on its right-click menu) takes the box away together with the blocks it holds. Cut and
//! Copy put those blocks on the clipboard as Markdown.

use crate::floating::{self, Glyph};
use crate::pageedit::{self, Caret, Changed};
use crate::ribbon::Act;
use crate::ribbon_icons::Mark;
use crate::theme::Tokens;
use egui::{Id, Key, Pos2, Rect, Sense, Ui, pos2, vec2};
use notecraft_formats::markdown::Block;
use notecraft_model::{Container, container};

/// The height of the bar at the top of a container, which is what it is dragged by. Wide
/// enough to hit without aiming: a thin strip is what makes dragging feel unreliable.
const HANDLE: f32 = 14.0;
/// How wide the strip on the right edge is, for resizing. Same reason.
const GRIP: f32 = 10.0;
/// The gap between a container's edge and the words inside it.
const PAD: f32 = 6.0;

/// Which container is selected. Kept in the ui's own store, as the page's menu keeps its place.
fn selected_id() -> Id {
    Id::new("container-selected")
}

/// Where the selected container's menu is open, if it is.
fn menu_id() -> Id {
    Id::new("container-menu")
}

fn claimed_id() -> Id {
    Id::new("container-took-right-click")
}

/// The container that had words selected in it before any button went down. A text box moves
/// its selection to the pointer the moment a button is pressed, right button included, so by the
/// time the right click is complete the words are no longer selected and cannot be asked.
fn words_id() -> Id {
    Id::new("container-words-selected")
}

/// The container that is selected on `page`, if there is one. The selection is kept with the
/// page it was made on: another page opened since has nothing selected, rather than inheriting
/// a box number and a Delete key that would take a box out of a page nobody picked from.
pub fn selected(ctx: &egui::Context, page: &std::path::Path) -> Option<usize> {
    let key = page.to_string_lossy();
    ctx.data(|d| d.get_temp::<(usize, String)>(selected_id())).filter(|(_, on)| *on == key).map(|(i, _)| i)
}

/// True when a container took this frame's right click, so the page's own menu stays shut.
pub fn took_right_click(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<u64>(claimed_id())) == Some(ctx.cumulative_pass_nr())
}

/// Takes container `at` off the page together with the blocks it holds. False, and nothing
/// changed, when there is no such container or the counts do not fit the blocks.
pub fn remove_container(blocks: &mut Vec<Block>, boxes: &mut Vec<Container>, at: usize) -> bool {
    if at >= boxes.len() {
        return false;
    }
    let range = container::blocks_of(boxes, at);
    if range.end > blocks.len() {
        return false;
    }
    blocks.drain(range);
    boxes.remove(at);
    true
}

/// What a frame of the canvas did.
#[derive(Default)]
pub struct Moved {
    pub changed: Changed,
    /// True when a container was moved, resized, added or removed.
    pub boxes: bool,
    /// The top left of the page's own area, so a script can work out where a box is drawn.
    pub origin: egui::Pos2,
    /// What the box's menu asked for besides its own Cut, Copy and Delete: the page's entries
    /// (Paste, Table, Link and the rest), which the window runs once this frame's borrow is over.
    pub acts: Vec<Act>,
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
    cols: &mut notecraft_model::tablecols::Widths,
    shown_title: &str,
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

    // The selection, and the menu that goes with it. A container that has gone (the page
    // was changed under it) cannot stay selected.
    let page_key = page_dir.to_string_lossy().into_owned();
    let mut sel = selected(ui.ctx(), page_dir).filter(|i| *i < boxes.len());
    let mut menu_at = ui.data(|d| d.get_temp::<Pos2>(menu_id()));
    let mut sel_frame: Option<Rect> = None;
    let mut delete_at: Option<usize> = None;
    // An empty box that was clicked: it has no words to answer a click, so the click starts some.
    let mut start_in: Option<usize> = None;
    let right_click = ui.input(|i| i.pointer.secondary_clicked());
    let pointer = ui.ctx().pointer_latest_pos();
    let mut opened_now = false;
    let words_in = ui.data(|d| d.get_temp::<(usize, usize, usize, usize)>(words_id()));
    // The open paper: the whole page area, made first so that every box and every block, which
    // are made after it, sit on top of it and take their own clicks. A click that reaches it
    // landed on no box at all.
    let paper =
        ui.interact(Rect::from_min_size(area.min, vec2(area.width(), area.height().min(20_000.0))), ui.id().with("open-paper"), Sense::click());
    let selection_was_up = sel.is_some() || menu_at.is_some();
    // A page with nothing on it shows nothing, which looks like a page that did not load. Say what
    // to do: the paper takes a click, and the words start where it was made.
    if blocks.is_empty() && caret.is_none() {
        ui.painter().text(
            pos2(area.min.x + PAD + 14.0, area.min.y + 30.0),
            egui::Align2::LEFT_TOP,
            "Click anywhere on the page to start writing.",
            egui::FontId::proportional(14.0),
            t.muted,
        );
    }
    let mut frames: Vec<Rect> = Vec::with_capacity(boxes.len());

    for i in 0..boxes.len() {
        let Some(b) = boxes.get(i).copied() else { continue };
        let range = container::blocks_of(boxes, i);
        let at = pos2(area.min.x + b.x, area.min.y + b.y);
        let editing = caret.as_ref().is_some_and(|c| range.contains(&c.block));

        // A place kept behind the words for the selection's shading, filled in below once the
        // box's size is known. Shading painted after the words washes them out.
        let shading = ui.painter().add(egui::Shape::Noop);
        // The blocks of this container, drawn in a column of its own width.
        let inner = Rect::from_min_size(pos2(at.x + PAD, at.y + HANDLE), vec2(b.width, 0.0));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(inner.min, vec2(b.width, area.height()))).layout(*ui.layout()));
        child.set_width(b.width);
        let changed = pageedit::show_blocks(&mut child, blocks, caret, page_dir, t, page, range.clone(), borders, cols, shown_title);
        out.changed.blocks |= changed.blocks;
        out.changed.caret |= changed.caret;
        out.changed.columns |= changed.columns;
        if changed.link.is_some() {
            out.changed.link = changed.link;
        }
        let used = child.min_rect().height().max(18.0);
        let frame = Rect::from_min_size(at, vec2(b.width + 2.0 * PAD, used + HANDLE + PAD));
        frames.push(frame);
        bottom = bottom.max(frame.max.y);

        let hovered = ui.rect_contains_pointer(frame);
        // A box with nothing in it has no block under the pointer, so a click inside it went
        // nowhere and no caret appeared. The whole of it, below the handle, is a place to start
        // writing: the click gives it a line and puts the caret there.
        if range.is_empty() {
            let hole = Rect::from_min_max(pos2(frame.min.x, frame.min.y + HANDLE), frame.max);
            let start = ui.interact(hole, ui.id().with(("container-start", i)), Sense::click());
            if start.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            if start.clicked() {
                start_in = Some(i);
            }
        }
        // The handle along the top and the grip down the right edge. Both widgets exist on
        // every frame, whether or not they are drawn: a widget that comes and goes with the
        // pointer loses the drag the moment the pointer leaves it, which is what made
        // moving a box feel like it slipped out of the hand.
        let bar = Rect::from_min_size(frame.min, vec2(frame.width(), HANDLE));
        let drag = ui.interact(bar, ui.id().with(("container-move", i)), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Move);
        let edge = Rect::from_min_size(pos2(frame.max.x - GRIP, frame.min.y + HANDLE), vec2(GRIP, (frame.height() - HANDLE).max(GRIP)));
        let grip = ui.interact(edge, ui.id().with(("container-size", i)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

        // A click on the handle selects the container. A right click anywhere in it does too,
        // unless the caret is in it with words selected: then the right click belongs to
        // those words, and the page's menu cuts and copies them.
        let words_selected = words_in.is_some_and(|w| w.0 == i);
        // A right click on selected words: the text box is about to forget the selection, so
        // ask for it back. The page's own menu opens, and its Cut, Copy and the toolbar's Bold
        // find the words where they were.
        if right_click
            && words_selected
            && pointer.is_some_and(|p| frame.contains(p))
            && let Some((_, block_at, primary, secondary)) = words_in
        {
            ui.data_mut(|d| d.insert_temp(pageedit::restore_selection_id(), (block_at, primary, secondary)));
        }
        let right_here = right_click && pointer.is_some_and(|p| frame.contains(p)) && !words_selected;
        if drag.clicked() || right_here {
            sel = Some(i);
            *caret = None;
            out.changed.caret = true;
        }
        if right_here {
            menu_at = pointer;
            opened_now = true;
            // Read before the store is locked: asking the context for it inside would wait on itself.
            let pass = ui.ctx().cumulative_pass_nr();
            ui.data_mut(|d| d.insert_temp(claimed_id(), pass));
        }
        let is_sel = sel == Some(i);
        if is_sel {
            sel_frame = Some(frame);
            // Everything in the box is selected: shade it all, behind the words.
            ui.painter().set(shading, egui::Shape::rect_filled(frame, 3.0, t.row_selected));
            ui.painter().rect_stroke(frame, 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
            if caret.is_none() && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
                delete_at = Some(i);
            }
        }

        // Drawn only when the pointer is in the container, the caret is in it, or it is
        // being dragged, the way a container shows its edges in OneNote.
        let busy = drag.dragged() || grip.dragged();
        if hovered || editing || busy || is_sel {
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

    // Ctrl+C and Ctrl+X on a selected box, as the menu's Cut and Copy: the whole of it goes on the
    // clipboard. With the caret in some words they are those words', and the text box takes them.
    if let Some(i) = sel
        && caret.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
    {
        // The window's own Copy and Cut events, or the keys themselves, which is how a script or a
        // remote session sends them. Both in one frame do it once.
        let pressed = |i: &egui::InputState, which: Key| {
            i.events.iter().any(|e| matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if *key == which && modifiers.command))
        };
        let (copy_key, cut_key) = ui.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)) || pressed(i, Key::C),
                i.events.iter().any(|e| matches!(e, egui::Event::Cut)) || pressed(i, Key::X),
            )
        });
        if copy_key || cut_key {
            let held = blocks.get(container::blocks_of(boxes, i)).unwrap_or_default();
            let markdown = notecraft_formats::render::blocks_to_markdown(held);
            pageedit::remember_copied(&markdown, held);
            ui.ctx().copy_text(markdown);
            if cut_key {
                delete_at = Some(i);
            }
        }
    }
    // The selected container's menu, where the right click fell.
    let mut menu_rect: Option<Rect> = None;
    if let (Some(i), Some(at)) = (sel, menu_at) {
        let mut picked = false;
        let mut copy = false;
        let area = egui::Area::new(menu_id().with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |menu| {
                egui::Frame::popup(menu.style()).inner_margin(egui::Margin::same(4)).show(menu, |menu| {
                    menu.set_width(floating::MENU_W);
                    menu.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    if floating::menu_row(menu, Some(Glyph::Mark(Mark::Cut)), "Cut", t).clicked() {
                        copy = true;
                        delete_at = Some(i);
                        picked = true;
                    }
                    if floating::menu_row(menu, Some(Glyph::Mark(Mark::Copy)), "Copy", t).clicked() {
                        copy = true;
                        picked = true;
                    }
                    floating::menu_separator(menu, t);
                    if floating::menu_row(menu, Some(Glyph::Mark(Mark::Delete)), "Delete", t).clicked() {
                        delete_at = Some(i);
                        picked = true;
                    }
                    // The page's own entries stay reachable from here: a right click on a
                    // box's words used to open the page's menu, and it still offers all of it.
                    floating::menu_separator(menu, t);
                    picked |= crate::pagemenu::items(menu, &mut out.acts, false, t);
                });
            })
            .response;
        menu_rect = Some(area.rect);
        if copy {
            let held = blocks.get(container::blocks_of(boxes, i)).unwrap_or_default();
            let markdown = notecraft_formats::render::blocks_to_markdown(held);
            pageedit::remember_copied(&markdown, held);
            ui.ctx().copy_text(markdown);
        }
        let outside = ui.input(|i| i.pointer.any_click()) && !opened_now && !area.rect.contains(pointer.unwrap_or_default());
        if picked || outside || ui.input(|i| i.key_pressed(Key::Escape)) {
            menu_at = None;
        }
    }
    // A press outside the selected container and its menu, or Escape, lets go of the selection.
    // So does the caret arriving in a container: the words are being edited, not the box.
    if sel.is_some() {
        let away = |p: Pos2| !sel_frame.is_some_and(|f| f.contains(p)) && !menu_rect.is_some_and(|m| m.contains(p));
        let pressed_away = ui.input(|i| i.pointer.primary_pressed()) && !opened_now && pointer.is_some_and(away);
        if pressed_away || caret.is_some() || ui.input(|i| i.key_pressed(Key::Escape)) {
            sel = None;
            menu_at = None;
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

    // A click in an empty box: give it a line to write in, and put the caret there.
    if let Some(i) = start_in
        && delete_at.is_none()
        && container::blocks_of(boxes, i).is_empty()
    {
        let at = container::blocks_of(boxes, i).start.min(blocks.len());
        blocks.insert(at, Block::Paragraph(Vec::new()));
        if let Some(c) = boxes.get_mut(i) {
            c.blocks += 1;
        }
        *caret = Some(pageedit::Caret { block: at, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None });
        out.changed.blocks = true;
        out.changed.caret = true;
        out.boxes = true;
        ui.ctx().request_repaint();
    }

    // Delete (the key, or the menu's Cut and Delete): the container and everything in it go.
    // Done after the drawing above so the blocks are not taken out from under it.
    if let Some(i) = delete_at
        && remove_container(blocks, boxes, i)
    {
        out.changed.blocks = true;
        out.changed.caret = true;
        out.boxes = true;
        *caret = None;
        sel = None;
        menu_at = None;
        ui.ctx().request_repaint();
    }
    // A menu belongs to a selection: with none, there is no menu.
    if sel.is_none() {
        menu_at = None;
    }
    // Remember where words are selected, but only while no button is down: a press moves the
    // selection, and what is wanted is how it stood before.
    if !ui.input(|i| i.pointer.any_down()) {
        let words = caret
            .as_ref()
            .filter(|c| c.selection.0 != c.selection.1)
            .and_then(|c| container::place(boxes, c.block).map(|(i, _)| (i, c.block, c.selection.0, c.selection.1)));
        ui.data_mut(|d| match words {
            Some(w) => {
                d.insert_temp(words_id(), w);
            }
            None => {
                d.remove::<(usize, usize, usize, usize)>(words_id());
            }
        });
    }
    ui.data_mut(|d| {
        match sel {
            Some(i) => {
                d.insert_temp(selected_id(), (i, page_key.clone()));
            }
            None => {
                d.remove::<(usize, String)>(selected_id());
            }
        }
        match menu_at {
            Some(p) => {
                d.insert_temp(menu_id(), p);
            }
            None => d.remove::<Pos2>(menu_id()),
        }
    });

    // The containers were drawn in child uis, which do not move this ui's cursor. Claim the
    // space they took, so the page scrolls to their bottom and so the room under them really
    // is under them: allocated at the cursor it would sit on top of the first container and
    // take every click meant for a block.
    ui.allocate_rect(Rect::from_min_max(area.min, pos2(area.max.x, bottom)), Sense::hover());
    // Open paper beside or between the boxes: a click there starts a box of its own where the pointer
    // is, as it does under them. It does nothing when it only lets go of a selected box or a menu,
    // or when it landed in a box's own margin.
    if paper.clicked()
        && !selection_was_up
        && let Some(at) = paper.interact_pointer_pos()
        && at.y < bottom
        && !frames.iter().any(|f| f.expand(3.0).contains(at))
    {
        let where_ = pos2((at.x - area.min.x - PAD).max(0.0), (at.y - area.min.y - HANDLE).max(0.0));
        start_a_box(blocks, boxes, caret, where_, (area.width() - 2.0 * PAD).max(container::MIN_WIDTH));
        out.changed.caret = true;
        out.changed.blocks = true;
        out.boxes = true;
        ui.ctx().request_repaint();
    }
    if paper.hovered() && at_open_paper(ui, &frames) {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
    }
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

/// Whether the pointer is over open paper, not over a box.
fn at_open_paper(ui: &Ui, frames: &[Rect]) -> bool {
    ui.ctx().pointer_latest_pos().is_some_and(|p| !frames.iter().any(|f| f.expand(3.0).contains(p)))
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
    fn removing_a_container_takes_its_blocks_with_it() {
        let mut blocks = page("one\n\ntwo\n\nthree\n\nfour\n");
        let mut boxes = vec![Container::new(0.0, 0.0, 300.0, 1), Container::new(0.0, 50.0, 300.0, 2), Container::new(0.0, 120.0, 300.0, 1)];
        assert!(remove_container(&mut blocks, &mut boxes, 1));
        assert_eq!(boxes.iter().map(|b| b.blocks).collect::<Vec<_>>(), vec![1, 1]);
        let md = notecraft_formats::render::blocks_to_markdown(&blocks);
        assert!(md.contains("one") && md.contains("four"), "the others stay: {md}");
        assert!(!md.contains("two") && !md.contains("three"), "its own blocks are gone: {md}");
        assert_eq!(boxes.iter().map(|b| b.blocks).sum::<usize>(), blocks.len(), "every block is still in a box");
    }

    #[test]
    fn removing_a_container_that_is_not_there_changes_nothing() {
        let mut blocks = page("one\n\ntwo\n");
        let mut boxes = vec![Container::new(0.0, 0.0, 300.0, 2)];
        assert!(!remove_container(&mut blocks, &mut boxes, 3));
        assert_eq!((blocks.len(), boxes.len()), (2, 1));
        // Counts that do not fit the blocks are refused too, rather than cutting the wrong ones.
        let mut odd = vec![Container::new(0.0, 0.0, 300.0, 5)];
        assert!(!remove_container(&mut blocks, &mut odd, 0));
        assert_eq!(blocks.len(), 2);
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
            let out = show(
                ui,
                &mut blocks,
                &mut boxes,
                &mut caret,
                std::path::Path::new("."),
                &t,
                notecraft_model::Dir::Ltr,
                true,
                &mut Default::default(),
                "",
            );
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
            show(
                ui,
                &mut blocks,
                &mut boxes,
                &mut caret,
                std::path::Path::new("."),
                &t,
                notecraft_model::Dir::Ltr,
                true,
                &mut Default::default(),
                "",
            );
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
            show(
                ui,
                &mut blocks,
                &mut boxes,
                &mut caret,
                std::path::Path::new("."),
                &t,
                notecraft_model::Dir::Ltr,
                true,
                &mut Default::default(),
                "",
            );
        });
        frame.textures_delta.clear();
        assert_eq!(boxes.len(), 1, "everything in one box rather than losing a block");
        assert_eq!(boxes.first().map(|b| b.blocks), Some(4));
    }
}

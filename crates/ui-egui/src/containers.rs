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
const INITIAL_PAGE_LINES: f32 = 20.0;
const EMPTY_LINE_HEIGHT: f32 = 22.0;
const TRAILING_LINE_HEIGHT: f32 = 22.0;

/// Which container is selected. Kept in the ui's own store, as the page's menu keeps its place.
fn selected_id() -> Id {
    Id::new("container-selected")
}

fn selected_image_id() -> Id {
    Id::new("image-selected")
}

fn selected_object_id() -> Id {
    Id::new("embedded-object-selected")
}

pub fn selected_image(ctx: &egui::Context, page: &std::path::Path) -> Option<usize> {
    selected_object(ctx, page).and_then(|(i, kind)| (kind == "image").then_some(i))
}

pub fn selected_object(ctx: &egui::Context, page: &std::path::Path) -> Option<(usize, String)> {
    let key = page.to_string_lossy();
    ctx.data(|d| d.get_temp::<(usize, String, String)>(selected_object_id())).filter(|(_, _, on)| *on == key).map(|(i, kind, _)| (i, kind))
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

/// What the ribbon's Cut or Copy asked of the selected container (1 copy, 2 cut), until the next frame
/// takes it. The container does it as it does Ctrl+C and Ctrl+X, so there is one way to take a box.
fn ribbon_clip_id() -> Id {
    Id::new("container-ribbon-clipboard")
}

/// Asks the selected container to be copied or cut, as the ribbon's Cut and Copy buttons do.
pub fn ask_clipboard(ctx: &egui::Context, cut: bool) {
    ctx.data_mut(|d| d.insert_temp(ribbon_clip_id(), if cut { 2u8 } else { 1u8 }));
    ctx.request_repaint();
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
    log::debug!("REMOVE_CONTAINER: i={} blocks={:?}", at, range);
    blocks.drain(range);
    boxes.remove(at);
    true
}

/// What a frame of the canvas did.
#[derive(Default)]
pub struct Moved {
    /// A copy the page asked for: the HTML and its plain text. The window puts it on the
    /// clipboard it holds, and falls back to plain text when that fails.
    pub copy: Option<(String, String)>,
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
    page: Option<notecraft_model::Dir>,
    borders: bool,
    cols: &mut notecraft_model::tablecols::Widths,
    justified: &notecraft_model::justify::Justified,
    across: &mut Option<pageedit::Across>,
    shown_title: &str,
    zoom: f32,
) -> Moved {
    let mut out = Moved::default();
    let area = ui.available_rect_before_wrap();
    out.origin = area.min;
    let fitted = container::fit(boxes, blocks.len(), (area.width() / zoom - 2.0 * PAD).max(container::MIN_WIDTH));
    if fitted != *boxes {
        // Issue #18: the arrangement did not account for the blocks, so it was thrown away and
        // replaced by one box. Something changed the blocks without changing the boxes.
        log::warn!(
            "FIT_COLLAPSE: boxes={} counted={} blocks={} -> one box {:?}",
            boxes.len(),
            boxes.iter().map(|c| c.blocks).sum::<usize>(),
            blocks.len(),
            fitted.first()
        );
    }
    *boxes = fitted;
    for (i, c) in boxes.iter().enumerate().filter(|(_, c)| c.blocks == 0) {
        log::debug!("EMPTY_BOX: i={i} pos=({:.0},{:.0}) width={:.0} holds no block", c.x, c.y, c.width);
    }

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
    let mut delete_object: Option<usize> = None;
    // An empty box that was clicked: it has no words to answer a click, so the click starts some.
    let mut start_in: Option<usize> = None;
    let right_click = ui.input(|i| i.pointer.secondary_clicked());
    let pointer = ui.ctx().pointer_latest_pos();
    let mut opened_now = false;
    let words_in = ui.data(|d| d.get_temp::<(usize, usize, usize, usize)>(words_id()));
    let object_clicked = ui.data_mut(|d| d.remove_temp::<(usize, String)>(crate::preview::object_clicked_id()));
    let object_double_clicked = ui.data_mut(|d| d.remove_temp::<(usize, String)>(crate::preview::object_double_clicked_id()));
    ui.data_mut(|d| d.remove::<Vec<(usize, Rect, String)>>(crate::preview::object_hits_id()));
    let mut object_sel = selected_object(ui.ctx(), page_dir);
    // Selection must never rewrite the page arrangement. Image hit-testing is visual state;
    // the persisted container geometry changes only after an actual drag or resize.
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
    // With no containers there is no child editor to own the empty page. Give
    // the whole available sheet one topmost hit target so the first click can
    // create the first box, including on a page whose last box was deleted.
    if boxes.is_empty() {
        let empty_area = Rect::from_min_size(area.min, vec2(area.width(), area.height().max(260.0)));
        let empty_paper = ui.allocate_rect(empty_area, Sense::click());
        if empty_paper.clicked()
            && !selection_was_up
            && let Some(at) = empty_paper.interact_pointer_pos()
        {
            let where_ = pos2((at.x - area.min.x - PAD).max(0.0), (at.y - area.min.y - HANDLE).max(0.0));
            start_a_box(blocks, boxes, caret, where_, (area.width() - 2.0 * PAD).max(container::MIN_WIDTH));
            out.changed.caret = true;
            out.changed.blocks = true;
            out.boxes = true;
            ui.ctx().request_repaint();
        }
        if empty_paper.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
        }
        return out;
    }
    let mut frames: Vec<Rect> = Vec::with_capacity(boxes.len());

    for i in 0..boxes.len() {
        let Some(b) = boxes.get(i).copied() else { continue };
        let range = container::blocks_of(boxes, i);
        let image_object = range.len() == 1 && blocks.get(range.start).is_some_and(block_has_image);
        let standalone_image = range.len() == 1 && matches!(blocks.get(range.start), Some(Block::Image { .. }));
        let contains_image = blocks.get(range.clone()).is_some_and(|held| held.iter().any(block_has_image));
        log::debug!(
            "CONTAINER: i={} pos=({:.0},{:.0}) width={:.0} blocks={:?} image_object={} contains_image={}",
            i,
            b.x,
            b.y,
            b.width,
            range,
            image_object,
            contains_image
        );
        // Positions and widths are stored in page-space points, like the content. Scale them
        // together so the outline remains attached to its text as the page zooms.
        let at = pos2(area.min.x + b.x * zoom, area.min.y + b.y * zoom);
        let pad = PAD * zoom;
        let handle = HANDLE * zoom;
        let grip_width = GRIP * zoom;
        let editing = caret.as_ref().is_some_and(|c| range.contains(&c.block));

        // A place kept behind the words for the selection's shading, filled in below once the
        // box's size is known. Shading painted after the words washes them out.
        let shading = ui.painter().add(egui::Shape::Noop);
        // The blocks of this container, drawn in a column of its own width.
        let inner = Rect::from_min_size(pos2(at.x + pad, at.y + handle), vec2(b.width * zoom, 0.0));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(inner.min, vec2(b.width * zoom, area.height()))).layout(*ui.layout()));
        child.set_width(b.width * zoom);
        let changed =
            pageedit::show_blocks(&mut child, blocks, caret, page_dir, t, page, range.clone(), borders, cols, justified, across, shown_title, zoom);
        out.changed.blocks |= changed.blocks;
        out.changed.caret |= changed.caret;
        out.changed.columns |= changed.columns;
        if changed.link.is_some() {
            out.changed.link = changed.link;
        }
        if changed.select_object.is_some() {
            out.changed.select_object = changed.select_object;
        }
        if let Some((block, kind)) = object_clicked.as_ref().filter(|(block, _)| range.contains(block)) {
            object_sel = Some((*block, kind.clone()));
            sel = None;
            *caret = None;
            out.changed.caret = true;
            log::debug!("embedded object selected: container={i} block={block} kind={kind}");
            if kind == "image" {
                let now = ui.input(|input| input.time);
                let prior = ui.data(|data| data.get_temp::<(usize, f64)>(crate::preview::object_click_state_id()));
                let is_double = prior.is_some_and(|(previous, at)| previous == *block && now - at <= 0.75);
                if is_double {
                    ui.data_mut(|data| data.remove::<(usize, f64)>(crate::preview::object_click_state_id()));
                    if let Some(href) = blocks.get(*block).and_then(|block| crate::preview::object_link(block, page_dir)) {
                        out.changed.link = Some(href);
                    }
                } else {
                    ui.data_mut(|data| data.insert_temp(crate::preview::object_click_state_id(), (*block, now)));
                }
            }
        }
        if let Some((block, _)) = object_double_clicked.as_ref().filter(|(block, _)| range.contains(block))
            && let Some(href) = blocks.get(*block).and_then(|block| crate::preview::object_link(block, page_dir))
        {
            out.changed.link = Some(href);
        }
        let used = container_content_height(blocks, &range, child.min_rect().height().max(18.0));
        // The move handle sits above the content frame, so it can never cover its top border.
        let frame_at = pos2(at.x, at.y + handle);
        let frame = Rect::from_min_size(frame_at, vec2(b.width * zoom + 2.0 * pad, used + pad));
        frames.push(frame);
        bottom = bottom.max(frame.max.y);

        let hovered = ui.rect_contains_pointer(frame);
        // A box with nothing in it has no block under the pointer, so a click inside it went
        // nowhere and no caret appeared. The whole of it, below the handle, is a place to start
        // writing: the click gives it a line and puts the caret there.
        if range.is_empty() {
            let hole = frame;
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
        // A standalone picture behaves like an object rather than a text container: the
        // whole frame is its move target, so the user does not have to find a tiny title bar.
        // Pictures own their visible rectangle. The parent container must not also claim that
        // rectangle: two overlapping interactions made a picture click intermittently select the
        // whole page/container instead of the picture.
        let drawn_image_rect = standalone_image
            .then(|| {
                ui.data(|data| data.get_temp::<Vec<(usize, Rect, String)>>(crate::preview::object_hits_id()))
                    .and_then(|hits| hits.iter().find(|(block, _, kind)| *block == range.start && kind == "image").map(|(_, rect, _)| *rect))
            })
            .flatten();
        let image_rect = drawn_image_rect.unwrap_or_else(|| Rect::from_min_size(inner.min, vec2(b.width * zoom, 480.0 * zoom)));
        let resize_handle_size = (6.0 * zoom).max(4.0);
        let pointer_on_resize_handle = image_object
            && object_sel.as_ref().is_some_and(|(_, kind)| image_can_resize(kind))
            && pointer.is_some_and(|p| resize_handle_rects(image_rect, resize_handle_size).iter().any(|rect| rect.contains(p)));
        if contains_image {
            log::debug!(
                "image candidate: container={i} blocks={}..{} standalone={} frame={:?} image_rect={:?} pointer={:?}",
                range.start,
                range.end,
                image_object,
                frame,
                image_rect,
                pointer
            );
        }
        let image_hit = image_object
            .then(|| ui.interact(image_rect, ui.id().with(("image-object", i)), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Move));
        if let Some(response) = &image_hit
            && !pointer_on_resize_handle
            && (response.clicked() || response.secondary_clicked() || response.dragged())
        {
            if response.clicked() {
                let now = ui.input(|input| input.time);
                let prior = ui.data(|data| data.get_temp::<(usize, f64)>(crate::preview::object_click_state_id()));
                let is_double = prior.is_some_and(|(previous, at)| previous == range.start && now - at <= 0.75);
                if is_double {
                    ui.data_mut(|data| data.remove::<(usize, f64)>(crate::preview::object_click_state_id()));
                    if let Some(href) = blocks.get(range.start).and_then(|block| crate::preview::object_link(block, page_dir)) {
                        out.changed.link = Some(href);
                    }
                } else {
                    ui.data_mut(|data| data.insert_temp(crate::preview::object_click_state_id(), (range.start, now)));
                }
            }
            object_sel = Some((range.start, "image".to_string()));
            sel = None;
            *caret = None;
            out.changed.caret = true;
            log::debug!("image hit: container={i} block={}", range.start);
        }
        if let Some(response) = &image_hit
            && (response.hovered() || response.clicked() || response.dragged())
        {
            log::debug!(
                "image interaction: container={i} block={} hovered={} clicked={} dragged={} selected={:?}",
                range.start,
                response.hovered(),
                response.clicked(),
                response.dragged(),
                object_sel
            );
        }
        let mut resize_drag_active = false;
        let bar = Rect::from_min_size(at, vec2(frame.width(), handle));
        let drag = ui.interact(bar, ui.id().with(("container-move", i)), Sense::click_and_drag()).on_hover_cursor(egui::CursorIcon::Move);
        let edge = Rect::from_min_size(pos2(frame.max.x - grip_width, frame.min.y), vec2(grip_width, frame.height().max(grip_width)));
        let grip = ui.interact(edge, ui.id().with(("container-size", i)), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);

        // A click or right click on the handle selects the container, as in OneNote. A right
        // click on the words belongs to the words: the page's menu opens and the box stays
        // as it was.
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
        let image_right_click = image_hit.as_ref().is_some_and(|response| response.secondary_clicked());
        let right_here = right_click && pointer.is_some_and(|p| bar.contains(p)) && !words_selected && !image_right_click;
        if drag.clicked() || right_here {
            sel = Some(i);
            *caret = None;
            out.changed.caret = true;
        }
        if right_here {
            log::debug!("container {}: right-clicked, selected={:?}", i, sel);
            menu_at = pointer;
            opened_now = true;
            // Read before the store is locked: asking the context for it inside would wait on itself.
            let pass = ui.ctx().cumulative_pass_nr();
            ui.data_mut(|d| d.insert_temp(claimed_id(), pass));
        }
        let is_sel = sel == Some(i);
        if let Some((block, kind)) = object_sel.as_ref().filter(|(block, _)| range.contains(block)) {
            let hits = ui.data(|d| d.get_temp::<Vec<(usize, Rect, String)>>(crate::preview::object_hits_id())).unwrap_or_default();
            let object_rect = hits.iter().find(|(at, _, _)| at == block).map(|(_, rect, _)| *rect).unwrap_or(image_rect);
            log::debug!("embedded object affordance: block={block} kind={kind} rect={object_rect:?}");
            ui.painter().rect_stroke(object_rect, 1.0 * zoom, egui::Stroke::new(1.5 * zoom, t.accent), egui::StrokeKind::Inside);
            let image_rect = object_rect;
            let size = resize_handle_size;

            // Create interactive handle widgets for images (every frame, not just when hovered)
            if image_can_resize(kind) {
                let handle_positions = [
                    image_rect.left_top(),
                    image_rect.center_top(),
                    image_rect.right_top(),
                    image_rect.left_center(),
                    image_rect.right_center(),
                    image_rect.left_bottom(),
                    image_rect.center_bottom(),
                    image_rect.right_bottom(),
                ];

                let stroke = egui::Stroke::new(1.5 * zoom, t.accent);

                for (idx, point) in handle_positions.iter().enumerate() {
                    let handle_rect = Rect::from_center_size(*point, vec2(size * 2.0, size * 2.0));
                    let handle_response = ui.interact(handle_rect, ui.id().with(("image-resize", i, idx)), Sense::click_and_drag());

                    // Set cursor icon on hover
                    let cursor = match idx {
                        0 => egui::CursorIcon::ResizeNwSe,       // left_top
                        1 => egui::CursorIcon::ResizeVertical,   // center_top
                        2 => egui::CursorIcon::ResizeNeSw,       // right_top
                        3 => egui::CursorIcon::ResizeHorizontal, // left_center
                        4 => egui::CursorIcon::ResizeHorizontal, // right_center
                        5 => egui::CursorIcon::ResizeNeSw,       // left_bottom
                        6 => egui::CursorIcon::ResizeVertical,   // center_bottom
                        7 => egui::CursorIcon::ResizeNwSe,       // right_bottom
                        _ => egui::CursorIcon::Default,
                    };

                    if handle_response.hovered() {
                        ui.ctx().set_cursor_icon(cursor);
                    }

                    if handle_response.hovered() || handle_response.drag_started() || handle_response.dragged() || handle_response.drag_stopped() {
                        log::debug!(
                            "image resize handle: container={} block={} idx={} rect={:?} pointer={:?} hovered={} pressed={} drag_started={} dragged={} drag_stopped={} delta={:?} standalone={} selected={:?}",
                            i,
                            *block,
                            idx,
                            handle_rect,
                            pointer,
                            handle_response.hovered(),
                            handle_response.is_pointer_button_down_on(),
                            handle_response.drag_started(),
                            handle_response.dragged(),
                            handle_response.drag_stopped(),
                            handle_response.drag_delta(),
                            standalone_image,
                            object_sel,
                        );
                    }
                    resize_drag_active |= handle_response.drag_started() || handle_response.dragged();

                    let resize_state_id = ui.id().with(("image-resize-state", i, idx));
                    if handle_response.drag_started() {
                        let width = blocks
                            .get(*block)
                            .and_then(|block| match block {
                                Block::Image { width, .. } => *width,
                                _ => None,
                            })
                            .unwrap_or(image_rect.width() / zoom);
                        let height = blocks
                            .get(*block)
                            .and_then(|block| match block {
                                Block::Image { height, .. } => *height,
                                _ => None,
                            })
                            .unwrap_or(image_rect.height() / zoom);
                        if let (Some(container), Some(pointer)) = (boxes.get(i), pointer) {
                            ui.data_mut(|data| {
                                data.insert_temp(resize_state_id, ImageResizeStart { x: container.x, y: container.y, width, height, pointer });
                            });
                        }
                    }

                    // Handle resize drag
                    if handle_response.dragged() && standalone_image {
                        let frame_delta = handle_response.drag_delta();
                        let block_idx = *block;
                        let shift = ui.input(|input| input.modifiers.shift);
                        let Some(start) = ui.data(|data| data.get_temp::<ImageResizeStart>(resize_state_id)) else {
                            continue;
                        };
                        let delta = pointer.map_or(frame_delta, |current| current - start.pointer);
                        log::debug!(
                            "resize drag: idx={} frame_delta={:?} total_delta={:?} start_pointer={:?} pointer={:?} zoom={}",
                            idx,
                            frame_delta,
                            delta,
                            start.pointer,
                            pointer,
                            zoom
                        );
                        let geometry = resize_image(start.width, start.height, delta / zoom, idx, shift);
                        if let Some(c) = boxes.get_mut(i) {
                            c.x = start.x + geometry.offset.x;
                            c.y = start.y + geometry.offset.y;
                            c.width = geometry.width;
                        }
                        if let Some(Block::Image { width, height, .. }) = blocks.get_mut(block_idx) {
                            *width = Some(geometry.width);
                            *height = Some(geometry.height);
                            log::debug!("image resize: block={block_idx} width={} height={}", geometry.width, geometry.height);
                        }
                        out.boxes = true;
                        out.changed.blocks = true;
                    }
                    if handle_response.drag_stopped() {
                        ui.data_mut(|data| data.remove::<ImageResizeStart>(resize_state_id));
                    }

                    // Draw handle
                    ui.painter().rect_filled(Rect::from_center_size(*point, vec2(size, size)), 1.0, t.editor);
                    ui.painter().rect_stroke(Rect::from_center_size(*point, vec2(size, size)), 1.0, stroke, egui::StrokeKind::Inside);
                }
            }
        }
        if let Some(response) = &image_hit
            && response.dragged()
            && !resize_drag_active
        {
            let d = response.drag_delta();
            if let Some(c) = boxes.get_mut(i) {
                *c = Container::new(c.x + d.x / zoom, c.y + d.y / zoom, c.width, c.blocks);
            }
            out.boxes = true;
            log::debug!("image moved: container={i} block={} delta={d:?}", range.start);
        }
        if is_sel {
            sel_frame = Some(frame);
            // Everything in the box is selected: shade it all, behind the words.
            ui.painter().set(shading, egui::Shape::rect_filled(frame, 3.0, t.row_selected));
            ui.painter().rect_stroke(frame, 3.0 * zoom, egui::Stroke::new(1.5 * zoom, t.accent), egui::StrokeKind::Inside);
            if caret.is_none() && !ui.ctx().egui_wants_keyboard_input() && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace)) {
                delete_at = Some(i);
            }
        }

        // Drawn only when the pointer is in the container, the caret is in it, or it is
        // being dragged, the way a container shows its edges in OneNote.
        let busy = drag.dragged() || grip.dragged();
        if hovered || editing || busy || is_sel {
            log::debug!("render: i={} image_object={} hovered={} editing={} busy={} is_sel={}", i, image_object, hovered, editing, busy, is_sel);
            // Skip faint stroke for images; the selection affordance shows the picture boundary
            if !image_object {
                log::debug!("draw faint stroke for non-image");
                ui.painter().rect_stroke(frame, 3.0 * zoom, egui::Stroke::new(1.0 * zoom, t.container_border), egui::StrokeKind::Inside);
            }
            // Images: affordance is drawn in selection block above (on object_rect, the picture rect)
            if !image_object {
                ui.painter().rect_filled(bar, 3.0 * zoom, if drag.hovered() || drag.dragged() { t.row_hover } else { t.faint });
                for n in 0..3 {
                    let c = pos2(bar.center().x + (n as f32 - 1.0) * 5.0 * zoom, bar.center().y);
                    ui.painter().circle_filled(c, 1.2 * zoom, t.muted);
                }
            }
            if grip.hovered() || grip.dragged() {
                ui.painter().rect_filled(edge.shrink2(vec2(2.0, 4.0)), 2.0, t.row_hover);
            }
        }
        if drag.dragged() {
            let d = drag.drag_delta();
            if let Some(c) = boxes.get_mut(i) {
                *c = Container::new(c.x + d.x / zoom, c.y + d.y / zoom, c.width, c.blocks);
            }
            out.boxes = true;
        }
        if grip.dragged() {
            if let Some(c) = boxes.get_mut(i) {
                *c = Container::new(c.x, c.y, c.width + grip.drag_delta().x / zoom, c.blocks);
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
        let asked = ui.data_mut(|d| d.remove_temp::<u8>(ribbon_clip_id()));
        let (copy_key, cut_key) = (copy_key || asked == Some(1), cut_key || asked == Some(2));
        if copy_key || cut_key {
            // The heading that merely repeats the page's title is not content. It is the
            // title showing through, the title area is already drawing it, and the editor
            // hides it where it stands. Copying it carries a second copy of the title into
            // whatever box the words are pasted into, and that copy is not hidden, because
            // the rule that hides one only looks at the first block of the page. That is
            // the page with two titles the owner found.
            let whole = container::blocks_of(boxes, i);
            let range = if pageedit::is_title_echo(blocks, whole.start, shown_title) { whole.start + 1..whole.end } else { whole };
            let held = blocks.get(range).unwrap_or_default();
            let markdown = notecraft_formats::render::blocks_to_markdown(held);
            pageedit::remember_copied(&markdown, held);
            let html = notecraft_formats::render::blocks_to_html(held);
            let text = notecraft_formats::render::blocks_to_text(held);
            out.copy = Some((html, text));
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
            let html = notecraft_formats::render::blocks_to_html(held);
            let text = notecraft_formats::render::blocks_to_text(held);
            out.copy = Some((html, text));
        }
        let outside = ui.input(|i| i.pointer.any_click()) && !opened_now && !area.rect.contains(pointer.unwrap_or_default());
        if picked || outside || ui.input(|i| i.key_pressed(Key::Escape)) {
            menu_at = None;
        }
    }
    if let Some((block, kind)) = out.changed.select_object.take() {
        object_sel = Some((block, kind.clone()));
        *caret = None;
        out.changed.caret = true;
        ui.ctx().request_repaint();
        log::debug!("keyboard selected embedded object: block={block} kind={kind}");
    }

    // A press outside the selected container and its menu, or Escape, lets go of the selection.
    // So does the caret arriving in a container: the words are being edited, not the box.
    if sel.is_some() {
        let away = |p: Pos2| !sel_frame.is_some_and(|f| f.contains(p)) && !menu_rect.is_some_and(|m| m.contains(p));
        // Only a press on the page lets go of it: the ribbon's buttons act on the selected box, so a
        // press there must leave it selected.
        // A list the ribbon has opened over the page (the styles) is not the page either: a press on one of
        // its rows is a pick for the selected box.
        let over_a_popup = |p: Pos2| ui.ctx().layer_id_at(p).is_some_and(|l| l.order == egui::Order::Foreground);
        let pressed_away =
            ui.input(|i| i.pointer.primary_pressed()) && !opened_now && pointer.is_some_and(|p| area.contains(p) && away(p) && !over_a_popup(p));
        if pressed_away || caret.is_some() || ui.input(|i| i.key_pressed(Key::Escape)) {
            sel = None;
            menu_at = None;
        }
    }
    if object_sel.is_some() && (caret.is_some() || ui.input(|i| i.key_pressed(Key::Escape))) {
        object_sel = None;
    }
    if let Some((block, _)) = object_sel {
        let direction = ui.input(|i| {
            if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowUp) {
                Some(false)
            } else if i.key_pressed(Key::ArrowDown) {
                Some(true)
            } else {
                None
            }
        });
        if let Some(down) = direction {
            let target = if down {
                pageedit::next_writable_block(blocks, block, shown_title)
            } else {
                pageedit::previous_writable_block(blocks, block, shown_title)
            };
            if let Some(target) = target {
                let text = pageedit::text_for_caret(blocks, target);
                *caret =
                    Some(pageedit::Caret { block: target, text, focus: true, selection: (0, 0), place: down.then_some(0), cell: None, span: None });
                object_sel = None;
                out.changed.caret = true;
                log::debug!(
                    "keyboard moved from embedded object: image_block={block} direction={} caret_block={target}",
                    if down { "down" } else { "up" }
                );
            }
        } else if ui.input(|i| i.key_pressed(Key::ArrowRight)) {
            let target = pageedit::next_writable_block(blocks, block, shown_title);
            let target = target.or_else(|| {
                let at = block.saturating_add(1).min(blocks.len());
                blocks.insert(at, Block::Paragraph(Vec::new()));
                out.changed.blocks = true;
                Some(at)
            });
            if let Some(target) = target {
                let text = pageedit::text_for_caret(blocks, target);
                *caret = Some(pageedit::Caret { block: target, text, focus: true, selection: (0, 0), place: Some(0), cell: None, span: None });
                object_sel = None;
                out.changed.caret = true;
                log::debug!("keyboard moved after embedded object: image_block={block} caret_block={target}");
            }
        }
    }
    // Embedded objects have their own selection, separate from the containing box.
    // Both keys should remove the selected object, just as they remove selected text.
    if let Some((block, _)) = object_sel
        && caret.is_none()
        && !ui.ctx().egui_wants_keyboard_input()
        && ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
    {
        delete_object = Some(block);
        object_sel = None;
    }
    ui.data_mut(|d| {
        if let Some((block, kind)) = object_sel {
            d.insert_temp(selected_object_id(), (block, kind.clone(), page_key.clone()));
            if kind == "image" {
                d.insert_temp(selected_image_id(), (block, page_key.clone()));
            } else {
                d.remove::<(usize, String)>(selected_image_id());
            }
        } else {
            d.remove::<(usize, String, String)>(selected_object_id());
            d.remove::<(usize, String)>(selected_image_id());
        }
    });

    // A block was added or taken away inside a container: it belongs to that one.
    let now = blocks.len();
    if now != before {
        let which = caret_box.unwrap_or(0).min(boxes.len().saturating_sub(1));
        log::debug!("BLOCK_COUNT: {before} -> {now} credited to box {which} (caret_box={caret_box:?})");
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
        log::debug!("FILL_EMPTY_BOX: i={i} inserts empty paragraph at block {at} (triggered by click)");
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

    // Remove an embedded block after it has been drawn, so the current frame's
    // hit testing and selection affordance remain stable.
    if let Some(block) = delete_object
        && let Some((container_at, _)) = container::place(boxes, block)
        && block < blocks.len()
    {
        // Check if this block is the only one in its container.
        if let Some(container) = boxes.get(container_at) {
            if container.blocks <= 1 {
                // Remove the whole container if it only has this one block.
                remove_container(blocks, boxes, container_at);
            } else {
                // Remove just this block; the container remains and owns the others.
                blocks.remove(block);
                // Update the container's block count.
                if let Some(c) = boxes.get_mut(container_at) {
                    c.blocks = c.blocks.saturating_sub(1);
                }
            }
        }
        out.changed.blocks = true;
        out.changed.caret = true;
        out.boxes = true;
        *caret = None;
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

fn block_has_image(block: &Block) -> bool {
    match block {
        Block::Image { .. } => true,
        Block::Paragraph(spans) => spans.iter().any(|span| span.image.is_some()),
        _ => false,
    }
}

fn image_can_resize(kind: &str) -> bool {
    kind == "image"
}

/// Keep a date-only new page usable without serializing empty paragraphs into the file.
fn container_content_height(blocks: &[Block], range: &std::ops::Range<usize>, used: f32) -> f32 {
    let minimum = if range.len() == 1 && pageedit::is_page_stamp(blocks, range.start) { INITIAL_PAGE_LINES * EMPTY_LINE_HEIGHT } else { 0.0 };
    used.max(minimum) + TRAILING_LINE_HEIGHT
}

fn resize_handle_rects(rect: Rect, size: f32) -> [Rect; 8] {
    [
        rect.left_top(),
        rect.center_top(),
        rect.right_top(),
        rect.left_center(),
        rect.right_center(),
        rect.left_bottom(),
        rect.center_bottom(),
        rect.right_bottom(),
    ]
    .map(|point| Rect::from_center_size(point, vec2(size * 2.0, size * 2.0)))
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ImageResize {
    offset: egui::Vec2,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct ImageResizeStart {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    pointer: egui::Pos2,
}

fn resize_image(width: f32, height: f32, delta: egui::Vec2, handle: usize, shift: bool) -> ImageResize {
    const MIN: f32 = 16.0;
    let aspect = (width / height).max(f32::EPSILON);
    let corner = matches!(handle, 0 | 2 | 5 | 7);
    let preserve_aspect = corner && !shift;
    let mut new_width = width;
    let mut new_height = height;
    let mut offset = egui::Vec2::ZERO;

    match handle {
        0 | 3 | 5 => {
            new_width = width - delta.x;
            offset.x = delta.x;
        }
        2 | 4 | 7 => new_width = width + delta.x,
        _ => {}
    }
    match handle {
        0..=2 => {
            new_height = height - delta.y;
            offset.y = delta.y;
        }
        5..=7 => new_height = height + delta.y,
        _ => {}
    }

    if preserve_aspect {
        let width_change = new_width - width;
        let height_change = new_height - height;
        let signed = if width_change.abs() >= height_change.abs() * aspect { width_change } else { height_change * aspect };
        new_width = (width + signed).max(MIN);
        new_height = (new_width / aspect).max(MIN);
        if matches!(handle, 0 | 3 | 5) {
            offset.x = width - new_width;
        }
        if matches!(handle, 0..=2) {
            offset.y = height - new_height;
        }
    } else {
        new_width = new_width.max(MIN);
        new_height = new_height.max(MIN);
    }

    ImageResize { offset, width: new_width, height: new_height }
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
    let block_idx = blocks.len() - 1;
    let box_idx = boxes.len();
    boxes.push(Container { x: at.x, y: at.y, width, blocks: 1 });
    log::debug!("CREATE_BOX: i={} at_pos=({:.0},{:.0}) width={:.0} holds_block={} (triggered by click)", box_idx, at.x, at.y, width, block_idx);
    *caret = Some(pageedit::Caret { block: block_idx, text: String::new(), focus: true, selection: (0, 0), place: None, cell: None, span: None });
}

#[cfg(test)]
mod tests {
    #[test]
    fn image_detection_covers_standalone_and_inline_images() {
        use notecraft_formats::markdown::{Block, Span};
        assert!(block_has_image(&Block::Image { alt: "picture".into(), src: "assets/picture.png".into(), width: None, height: None }));
        assert!(block_has_image(&Block::Paragraph(vec![Span {
            image: Some("assets/picture.png".into()),
            text: "picture".into(),
            ..Span::default()
        }])));
        assert!(!block_has_image(&Block::Paragraph(vec![Span { text: "ordinary text".into(), ..Span::default() }])));
    }

    #[test]
    fn only_image_objects_get_resize_handles() {
        assert!(image_can_resize("image"));
        assert!(!image_can_resize("link"));
        assert!(!image_can_resize("file"));
    }

    #[test]
    fn a_date_only_new_page_gets_a_twenty_line_writing_surface() {
        let blocks = page("Monday, 2 November 2026, 05:05\n");
        assert_eq!(container_content_height(&blocks, &(0..1), 24.0), 462.0);
    }

    #[test]
    fn an_existing_multi_block_page_keeps_its_content_height() {
        let blocks = page("Monday, 2 November 2026, 05:05\n\ntext\n");
        assert_eq!(container_content_height(&blocks, &(0..2), 46.0), 68.0);
    }

    #[test]
    fn a_block_with_an_inline_image_keeps_one_line_below_the_object() {
        let blocks = page("text ![picture](assets/picture.png)\n");
        assert_eq!(container_content_height(&blocks, &(0..1), 80.0), 102.0);
    }

    #[test]
    fn image_resize_preserves_aspect_ratio_for_corners() {
        let resized = resize_image(200.0, 100.0, egui::vec2(40.0, 0.0), 7, false);
        assert_eq!(resized, ImageResize { offset: egui::Vec2::ZERO, width: 240.0, height: 120.0 });
    }

    #[test]
    fn image_resize_allows_free_corner_scaling_with_shift() {
        let resized = resize_image(200.0, 100.0, egui::vec2(40.0, 20.0), 7, true);
        assert_eq!(resized, ImageResize { offset: egui::Vec2::ZERO, width: 240.0, height: 120.0 });
    }

    #[test]
    fn image_resize_moves_left_and_top_edges_with_the_picture() {
        let resized = resize_image(200.0, 100.0, egui::vec2(20.0, 10.0), 0, true);
        assert_eq!(resized, ImageResize { offset: egui::vec2(20.0, 10.0), width: 180.0, height: 90.0 });
    }

    #[test]
    fn image_resize_changes_only_the_horizontal_edge_axis() {
        let left = resize_image(200.0, 100.0, egui::vec2(20.0, 35.0), 3, false);
        let right = resize_image(200.0, 100.0, egui::vec2(20.0, 35.0), 4, false);
        assert_eq!(left, ImageResize { offset: egui::vec2(20.0, 0.0), width: 180.0, height: 100.0 });
        assert_eq!(right, ImageResize { offset: egui::Vec2::ZERO, width: 220.0, height: 100.0 });
    }

    #[test]
    fn image_resize_changes_only_the_vertical_edge_axis() {
        let top = resize_image(200.0, 100.0, egui::vec2(35.0, 20.0), 1, false);
        let bottom = resize_image(200.0, 100.0, egui::vec2(35.0, 20.0), 6, false);
        assert_eq!(top, ImageResize { offset: egui::vec2(0.0, 20.0), width: 200.0, height: 80.0 });
        assert_eq!(bottom, ImageResize { offset: egui::Vec2::ZERO, width: 200.0, height: 120.0 });
    }

    #[test]
    fn image_resize_each_edge_stretches_only_in_its_direction() {
        let left = resize_image(200.0, 100.0, egui::vec2(-30.0, 45.0), 3, false);
        let right = resize_image(200.0, 100.0, egui::vec2(30.0, 45.0), 4, false);
        let top = resize_image(200.0, 100.0, egui::vec2(45.0, -20.0), 1, false);
        let bottom = resize_image(200.0, 100.0, egui::vec2(45.0, 20.0), 6, false);

        assert_eq!(left, ImageResize { offset: egui::vec2(-30.0, 0.0), width: 230.0, height: 100.0 });
        assert_eq!(right, ImageResize { offset: egui::Vec2::ZERO, width: 230.0, height: 100.0 });
        assert_eq!(top, ImageResize { offset: egui::vec2(0.0, -20.0), width: 200.0, height: 120.0 });
        assert_eq!(bottom, ImageResize { offset: egui::Vec2::ZERO, width: 200.0, height: 120.0 });
    }

    #[test]
    fn image_resize_edges_shrink_without_changing_the_other_dimension() {
        let left = resize_image(200.0, 100.0, egui::vec2(35.0, -45.0), 3, false);
        let right = resize_image(200.0, 100.0, egui::vec2(-35.0, -45.0), 4, false);
        let top = resize_image(200.0, 100.0, egui::vec2(-45.0, 25.0), 1, false);
        let bottom = resize_image(200.0, 100.0, egui::vec2(-45.0, -25.0), 6, false);

        assert_eq!(left, ImageResize { offset: egui::vec2(35.0, 0.0), width: 165.0, height: 100.0 });
        assert_eq!(right, ImageResize { offset: egui::Vec2::ZERO, width: 165.0, height: 100.0 });
        assert_eq!(top, ImageResize { offset: egui::vec2(0.0, 25.0), width: 200.0, height: 75.0 });
        assert_eq!(bottom, ImageResize { offset: egui::Vec2::ZERO, width: 200.0, height: 75.0 });
    }

    #[test]
    fn image_resize_edges_enforce_the_minimum_dimension() {
        let left = resize_image(20.0, 100.0, egui::vec2(100.0, 50.0), 3, false);
        let right = resize_image(20.0, 100.0, egui::vec2(-100.0, 50.0), 4, false);
        let top = resize_image(100.0, 20.0, egui::vec2(50.0, 100.0), 1, false);
        let bottom = resize_image(100.0, 20.0, egui::vec2(50.0, -100.0), 6, false);

        assert_eq!(left.width, 16.0);
        assert_eq!(left.height, 100.0);
        assert_eq!(right.width, 16.0);
        assert_eq!(right.height, 100.0);
        assert_eq!(top.width, 100.0);
        assert_eq!(top.height, 16.0);
        assert_eq!(bottom.width, 100.0);
        assert_eq!(bottom.height, 16.0);
    }

    #[test]
    fn image_resize_corners_are_the_only_handles_that_change_both_dimensions() {
        for handle in [0, 2, 5, 7] {
            let resized = resize_image(200.0, 100.0, egui::vec2(30.0, 20.0), handle, true);
            assert_ne!(resized.width, 200.0, "corner {handle} changes width");
            assert_ne!(resized.height, 100.0, "corner {handle} changes height");
        }
    }

    /// A box that holds the heading repeating the page title does not carry it when copied.
    ///
    /// The editor hides that heading where it stands, because the title area is already
    /// drawing it, but the rule that hides one only looks at the first block of the page.
    /// A copy pasted anywhere else is no longer the first block, so it was drawn, and the
    /// page showed its title twice. The owner found a page with two boxes doing exactly
    /// that, one showing the heading and one not.
    #[test]
    fn a_copied_box_leaves_the_title_behind() {
        use notecraft_formats::markdown::{self, Block};
        let blocks = markdown::parse(
            "# My page

words

more
",
        );
        assert!(matches!(blocks.first(), Some(Block::Heading { level: 1, .. })), "the page starts with its title: {blocks:?}");
        assert!(crate::pageedit::is_title_echo(&blocks, 0, "My page"), "and that heading repeats the title");
        assert!(!crate::pageedit::is_title_echo(&blocks, 1, "My page"), "the words after it are not the title");
        // A box starting at block 0 hands on everything after the heading.
        let whole = 0..blocks.len();
        let kept = if crate::pageedit::is_title_echo(&blocks, whole.start, "My page") { whole.start + 1..whole.end } else { whole };
        let held = blocks.get(kept).unwrap_or_default();
        let markdown = notecraft_formats::render::blocks_to_markdown(held);
        assert!(
            !markdown.contains("My page"),
            "the title is not in what was copied:
{markdown}"
        );
        assert!(
            markdown.contains("words") && markdown.contains("more"),
            "but the words are:
{markdown}"
        );
    }

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
                Some(notecraft_model::Dir::Ltr),
                true,
                &mut Default::default(),
                &Default::default(),
                &mut None,
                "",
                1.0,
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
                Some(notecraft_model::Dir::Ltr),
                true,
                &mut Default::default(),
                &Default::default(),
                &mut None,
                "",
                1.0,
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
                Some(notecraft_model::Dir::Ltr),
                true,
                &mut Default::default(),
                &Default::default(),
                &mut None,
                "",
                1.0,
            );
        });
        frame.textures_delta.clear();
        assert_eq!(boxes.len(), 1, "everything in one box rather than losing a block");
        assert_eq!(boxes.first().map(|b| b.blocks), Some(4));
    }
}

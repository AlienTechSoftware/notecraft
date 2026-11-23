//! The page's own menu, under the right button.
//!
//! Nothing is laid over the page to carry it. A widget that senses clicks would take the
//! primary click away from the words, the ink and the caret - the same mistake that made
//! seven of eight pens unusable an hour earlier - and a widget that does not sense clicks
//! cannot carry a context menu at all. So the secondary click is watched for directly and
//! the menu is drawn where it fell.
//!
//! It is drawn the way OneNote draws one: a picture beside each entry, and the mini toolbar
//! floating over it ([`crate::floating`]).

use crate::floating::{self, Glyph};
use crate::icons::Icon;
use crate::ribbon::Act;
use crate::ribbon_icons::Mark;
use crate::theme::Tokens;
use egui::Ui;

/// Where the menu is open, if it is. Kept in the ui's own store so it survives frames
/// without the window having to hold it.
fn state(ui: &Ui) -> Option<egui::Pos2> {
    ui.data(|d| d.get_temp::<egui::Pos2>(id()))
}

fn id() -> egui::Id {
    egui::Id::new("page-menu")
}

/// Draws the page's menu when the page has been right-clicked, and puts what was picked
/// into `acts`.
pub fn show(ui: &mut Ui, body: egui::Rect, acts: &mut Vec<Act>, t: &Tokens, locked: bool) {
    // A right click on a container belongs to the container's own menu. Close this one if it
    // was open, and leave the click alone.
    if crate::containers::took_right_click(ui.ctx()) {
        ui.data_mut(|d| d.remove::<egui::Pos2>(id()));
        return;
    }
    let pointer = ui.ctx().pointer_latest_pos();
    let opened_now = ui.input(|i| i.pointer.secondary_clicked());
    if opened_now && let Some(at) = pointer.filter(|p| body.contains(*p)) {
        log::debug!("context menu: right-clicked at {:?}", at);
        ui.data_mut(|d| d.insert_temp(id(), at));
    }
    let Some(at) = state(ui) else { return };

    let mut picked = false;
    let area = egui::Area::new(id().with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(at)
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
                ui.set_width(floating::MENU_W);
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                // A locked page's menu offers only Unlock for now (issue #441).
                if !locked {
                    picked = items_with(ui, acts, true, t, true);
                }
                // The page's lock, as the navigator's menus offer it (issue #441).
                let (label, icon) =
                    if locked { (crate::tl_id!("ui.unlock_read_only"), Icon::Unlock) } else { (crate::tl_id!("ui.lock_read_only"), Icon::Lock) };
                floating::menu_separator(ui, t);
                if floating::menu_row(ui, Some(Glyph::Icon(icon)), label, t).clicked() {
                    acts.push(Act::SetReadOnly(!locked));
                    picked = true;
                }
            });
        })
        .response;
    // The mini toolbar floats over the menu, as it does in OneNote.
    let fonts = ui.ctx().data(|d| d.get_temp::<floating::FontBoxes>(egui::Id::new(floating::FONT_BOXES_KEY))).unwrap_or_default();
    // A locked page has no formatting bar: its buttons are edits (issue #441).
    let (bar_picked, bar_rect) = if locked {
        (false, egui::Rect::NOTHING)
    } else {
        let bar = floating::show_bar(ui.ctx(), id().with("bar"), area.rect, &floating::text_bar_with_fonts(fonts), t, acts);
        (bar.picked, bar.rect)
    };
    picked |= bar_picked;

    let away = |p: egui::Pos2| !area.rect.contains(p) && !bar_rect.contains(p);
    let out = ui.input(|i| i.pointer.any_click()) && !opened_now && away(pointer.unwrap_or_default());
    if picked || out || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        ui.data_mut(|d| d.remove::<egui::Pos2>(id()));
    }
}

/// The page's menu entries: what a right click on the page offers, each with its picture.
/// `clipboard` puts Paste, Cut and Copy first; a container's menu has its own Cut and Copy for
/// the whole box, so it asks for the rest only. True when one was picked.
pub fn items(ui: &mut Ui, acts: &mut Vec<Act>, clipboard: bool, t: &Tokens) -> bool {
    items_with(ui, acts, clipboard, t, true)
}

/// [`items`], with the edit entries greyed out when `edits` is false (a locked page). Copy is not
/// an edit, so it stays enabled either way.
pub fn items_with(ui: &mut Ui, acts: &mut Vec<Act>, clipboard: bool, t: &Tokens, edits: bool) -> bool {
    let mut picked = false;
    let mut item = |ui: &mut Ui, glyph: Glyph, label: &str, act: Act, edit: bool| {
        let enabled = !edit || edits;
        if floating::menu_row_if(ui, enabled, Some(glyph), label, t).clicked() && enabled {
            acts.push(act);
            picked = true;
        }
    };
    item(ui, Glyph::Mark(Mark::Paste), "Paste", Act::Paste, true);
    if clipboard {
        item(ui, Glyph::Mark(Mark::Cut), "Cut", Act::Clipboard("cut"), true);
        item(ui, Glyph::Mark(Mark::Copy), "Copy", Act::Clipboard("copy"), false);
    }
    floating::menu_separator(ui, t);
    item(ui, Glyph::Icon(Icon::Table), "Table", Act::Insert("table"), true);
    item(ui, Glyph::Icon(Icon::Link), "Link", Act::Insert("link"), true);
    item(ui, Glyph::Icon(Icon::Equation), "Equation", Act::Insert("equation"), true);
    item(ui, Glyph::Icon(Icon::Calendar), "Date and time", Act::Insert("datetime"), true);
    floating::menu_separator(ui, t);
    item(ui, Glyph::Icon(Icon::Picture), "Picture from a file", Act::AddPicture, true);
    item(ui, Glyph::Icon(Icon::Attach), "Attach a file", Act::AddFile, true);
    picked
}

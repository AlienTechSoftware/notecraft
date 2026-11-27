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

/// Where the menu is open, if it is: the point or the block it was opened over. Kept in the
/// ui's own store so it survives frames without the window having to hold it.
fn state(ui: &Ui) -> Option<egui::Rect> {
    ui.data(|d| d.get_temp::<egui::Rect>(id()))
}

fn id() -> egui::Id {
    egui::Id::new("page-menu")
}

/// The widget that had the keyboard when the menu was opened from it, to give it back.
fn return_id() -> egui::Id {
    id().with("return-focus")
}

/// Draws the page's menu when the page has been right-clicked, or asked for its menu from the
/// keyboard (Shift+F10 or the Menu key while the caret is in the page, issue #380), and puts
/// what was picked into `acts`.
pub fn show(ui: &mut Ui, body: egui::Rect, acts: &mut Vec<Act>, t: &Tokens, locked: bool) {
    // A right click on a container belongs to the container's own menu. Close this one if it
    // was open, and leave the click alone.
    if crate::containers::took_right_click(ui.ctx()) {
        ui.data_mut(|d| d.remove::<egui::Rect>(id()));
        return;
    }
    let pointer = ui.ctx().pointer_latest_pos();
    let opened_now = ui.input(|i| i.pointer.secondary_clicked());
    if opened_now && let Some(at) = pointer.filter(|p| body.contains(*p)) {
        log::debug!("context menu: right-clicked at {:?}", at);
        ui.data_mut(|d| d.insert_temp(id(), egui::Rect::from_pos(at)));
    }
    // From the keyboard: the words with the caret have it. The menu opens under the line the
    // caret is on, where the words are, not wherever the pointer was left.
    // `read_response` is only asked on the frame the key is down: on another frame it would let go
    // of the keyboard after a click on the ribbon, as a widget drawn again would.
    if state(ui).is_none()
        && floating::menu_keys_down(ui.ctx())
        && let Some((focused, rect)) =
            ui.ctx().memory(|m| m.focused()).and_then(|f| ui.ctx().read_response(f).map(|r| (f, r.rect))).filter(|(_, r)| body.intersects(*r))
        && floating::menu_keys_pressed(ui.ctx())
    {
        let line = egui::Rect::from_min_max(rect.min, egui::pos2(rect.max.x, rect.min.y + rect.height().min(22.0)));
        ui.data_mut(|d| {
            d.insert_temp(id(), line);
            d.insert_temp(return_id(), focused);
        });
        floating::focus_first_row(ui.ctx(), id());
    }
    let Some(at) = state(ui) else { return };

    let mut picked = false;
    let Some(menu) = floating::point_menu(ui.ctx(), id(), at, |ui| {
        floating::menu_heading(ui, crate::tl_id!("ui.this_page"), t);
        // Opened over the date and time below the title: changing or hiding them comes first, as
        // OneNote's calendar and clock beside them do (issue #541).
        if !locked && let Some((block, _)) = crate::pageedit::stamp_place(ui.ctx()).filter(|(_, stamp)| stamp.intersects(at.expand(1.0))) {
            if floating::menu_row(ui, Some(Glyph::Icon(Icon::Calendar)), crate::tl_id!("ui.change_page_date"), t).clicked() {
                acts.push(Act::EditPageStamp(block));
                picked = true;
            }
            floating::menu_separator(ui, t);
        }
        // A locked page's menu offers only Unlock for now (issue #441).
        if !locked {
            picked |= items_with(ui, acts, true, t, true);
        }
        // The page's lock, as the navigator's menus offer it (issue #441).
        let (label, icon) =
            if locked { (crate::tl_id!("ui.unlock_read_only"), Icon::Unlock) } else { (crate::tl_id!("ui.lock_read_only"), Icon::Lock) };
        floating::menu_separator(ui, t);
        if floating::menu_row(ui, Some(Glyph::Icon(icon)), label, t).clicked() {
            acts.push(Act::SetReadOnly(!locked));
            picked = true;
        }
    }) else {
        return;
    };
    let area = menu.response;
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
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if picked || out || escaped {
        ui.data_mut(|d| d.remove::<egui::Rect>(id()));
        // Opened from the keyboard: the keyboard goes back to the words it came from. A click
        // outside that gave the keyboard to something else leaves it there.
        if let Some(back) = ui.data(|d| d.get_temp::<egui::Id>(return_id())) {
            ui.data_mut(|d| d.remove::<egui::Id>(return_id()));
            // (The menu's own rows let go of the keyboard on a click outside them, earlier in this
            // pass, so a widget that has it now is the one the click landed on.)
            let taken_elsewhere = out && ui.ctx().memory(|m| m.focused()).is_some_and(|f| f != back);
            if !taken_elsewhere {
                ui.memory_mut(|m| m.request_focus(back));
            }
        }
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

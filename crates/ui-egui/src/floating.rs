//! Floating pieces that sit over the page: icon rows for context menus, and the mini toolbar.
//!
//! OneNote draws the same two things in many places: over a right click on text, on a box, on a
//! table, on a picture, and beside a selection. So they are built once here, from data. A menu
//! is a list of [`menu_row`]s; the mini toolbar is a [`Bar`], rows of [`BarButton`]s that each
//! carry the [`Act`] they ask for, drawn by [`show_bar`] over or under whatever it is anchored
//! to. A new place that wants one builds a `Bar` and hands it to `show_bar`; nothing here knows
//! about the page, a box or a table.
//!
//! What is not here yet, because the editor cannot do it: font and size boxes, font colour,
//! highlighter and Styles in [`text_bar`]. A control that does nothing is not drawn.

use crate::icons::{self, Icon};
use crate::ribbon::Act;
use crate::ribbon_icons::{self, Mark};
use crate::theme::Tokens;
use egui::{Align2, Area, Context, Frame, Id, Order, Painter, Rect, Response, Sense, Ui, vec2};
use notecraft_model::Tag;

/// A picture on a menu row or a bar button: one of the ribbon's marks, or one of the app's icons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Glyph {
    Mark(Mark),
    Icon(Icon),
}

impl Glyph {
    pub fn paint(self, painter: &Painter, rect: Rect, t: &Tokens) {
        match self {
            Glyph::Mark(m) => ribbon_icons::paint(painter, rect, m, m.color(t.text, t.accent)),
            Glyph::Icon(i) => icons::paint(painter, rect, i, t.text),
        }
    }
}

/// How wide a context menu is.
pub const MENU_W: f32 = 232.0;
const ROW_H: f32 = 26.0;
/// The column the pictures sit in, to the left of the words.
const ICON_COL: f32 = 30.0;
const GLYPH: f32 = 16.0;

/// A row's or object's own menu (issue #380): it opens from the row itself, not wherever the
/// pointer happens to be, so it is clear which row it is for and it stays inside the window,
/// turning to open upwards or the other way near an edge. A secondary click opens it, and so do
/// Shift+F10 and the Menu key while the row has the keyboard; Escape closes it and gives the
/// keyboard back to the row.
pub fn row_menu<R>(resp: &Response, add: impl FnOnce(&mut Ui) -> R) -> Option<egui::InnerResponse<R>> {
    let by_keys = resp.has_focus()
        && resp.ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10) || i.consume_key(egui::Modifiers::NONE, egui::Key::Menu));
    let set = if resp.secondary_clicked() || by_keys {
        Some(egui::SetOpenCommand::Bool(true))
    } else if resp.clicked() {
        Some(egui::SetOpenCommand::Bool(false))
    } else {
        None
    };
    let id = egui::Popup::default_response_id(resp);
    let was_open = egui::Popup::is_id_open(&resp.ctx, id);
    let escaped = was_open && resp.ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let shown = egui::Popup::menu(resp).open_memory(set).show(add);
    if escaped && !egui::Popup::is_id_open(&resp.ctx, id) {
        resp.request_focus();
    }
    shown
}

/// A round button, `size` points across, with one glyph in it: an opener such as a picture's (>). The
/// corner radius is more than half the size, so it is a true circle at any size. `enabled` greys it.
pub fn round_button(ui: &mut Ui, glyph: &str, size: f32, enabled: bool) -> Response {
    let button = egui::Button::new(glyph).corner_radius(egui::CornerRadius::same(u8::MAX));
    // The exact size of the emoji header's "+" (add_sized, not min_size), so its row does not move.
    ui.add_enabled_ui(enabled, |ui| ui.add_sized([size, size], button)).inner
}

/// Where a menu that opens at a point goes (issue #380): under and after the point, and, when that
/// would run past the window, before it or above it instead, so its last row is never cut off by
/// the window's edge. The first frame measures the menu; the next one places it.
const POINT_MENU_ALIGNS: [egui::RectAlign; 4] =
    [egui::RectAlign::BOTTOM_START, egui::RectAlign::BOTTOM_END, egui::RectAlign::TOP_START, egui::RectAlign::TOP_END];

/// A menu for something on the page (the page, a box, a picture), opened at `anchor`: where the
/// right click fell, or, from the keyboard, the thing itself. It is an egui menu, so the arrow
/// keys move between its rows, and it is kept inside the window by turning to open the other
/// way near an edge. The caller decides when it opens and closes.
pub fn point_menu<R>(ctx: &Context, id: Id, anchor: Rect, add: impl FnOnce(&mut Ui) -> R) -> Option<egui::InnerResponse<R>> {
    egui::Popup::new(id, ctx.clone(), egui::PopupAnchor::ParentRect(anchor), egui::LayerId::new(Order::Foreground, id))
        .kind(egui::PopupKind::Menu)
        .open(true)
        .align(POINT_MENU_ALIGNS[0])
        .align_alternatives(&POINT_MENU_ALIGNS[1..])
        .gap(0.0)
        .layout(egui::Layout::top_down(egui::Align::Min))
        .frame(Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(4)))
        .show(|ui| {
            ui.set_width(MENU_W);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            add(ui)
        })
}

/// Whether Shift+F10 or the Menu key was pressed this frame, without taking the key: for a
/// check that must not run on other frames (egui's `read_response` lets go of the keyboard on a
/// click elsewhere, so it is only asked once the key is known to be down).
pub fn menu_keys_down(ctx: &Context) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(e, egui::Event::Key { key: egui::Key::F10, pressed: true, modifiers, .. } if modifiers.shift_only())
                || matches!(e, egui::Event::Key { key: egui::Key::Menu, pressed: true, modifiers, .. } if modifiers.is_none())
        })
    })
}

/// The keys that open the menu of whatever has the keyboard: Shift+F10, or the Menu key.
pub fn menu_keys_pressed(ctx: &Context) -> bool {
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, egui::Key::F10) || i.consume_key(egui::Modifiers::NONE, egui::Key::Menu))
}

fn first_row_id(layer: Id) -> Id {
    Id::new(("menu-first-row", layer))
}

/// Asks the menu whose area is `menu` to put the keyboard on its first row that can be used,
/// the next time it is drawn: a menu opened from the keyboard is then ready for Enter and the
/// arrow keys (issue #380).
pub fn focus_first_row(ctx: &Context, menu: Id) {
    ctx.data_mut(|d| d.insert_temp(first_row_id(menu), true));
}

/// The first usable row of a menu takes the keyboard when the menu asked for it.
fn take_first_row(ui: &Ui, resp: &Response) {
    if ui.is_sizing_pass() || !ui.is_enabled() || !ui.is_visible() {
        return;
    }
    let key = first_row_id(ui.layer_id().id);
    if ui.data_mut(|d| d.remove_temp::<bool>(key)).unwrap_or(false) {
        resp.request_focus();
    }
}

/// The line at the top of a menu that names what it acts on (issue #380), so a menu opened over
/// a page, a picture or a box says which one, in the quiet colour of a heading.
pub fn menu_heading(ui: &mut Ui, text: &str, t: &Tokens) {
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    let painter = ui.painter_at(rect);
    let galley = painter.layout(text.to_owned(), egui::FontId::proportional(11.5), t.muted, rect.width() - 12.0);
    let shown = galley.rows.first().map(|r| r.text()).unwrap_or_default();
    let cut = shown.chars().count() < text.chars().count();
    let line = if cut { format!("{}…", shown.trim_end()) } else { text.to_owned() };
    painter.text(egui::pos2(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER, line, egui::FontId::proportional(11.5), t.muted);
    let name = text.to_owned();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.clone()));
}

/// A row that opens a further menu (issue #380): the picture column and the words as every
/// other row, and a chevron in the column on the right where a shortcut would go. Hovering it,
/// Enter, or the Right arrow opens the further menu; Left or Escape comes back.
// i18n-ui-text: label
pub fn menu_submenu_row<R>(ui: &mut Ui, glyph: Option<Glyph>, label: &str, t: &Tokens, add: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let resp = menu_row(ui, glyph, label, t);
    let at = egui::pos2(resp.rect.max.x - 12.0, resp.rect.center().y);
    ui.painter_at(resp.rect).add(egui::Shape::line(
        vec![egui::pos2(at.x - 2.0, at.y - 4.0), egui::pos2(at.x + 2.0, at.y), egui::pos2(at.x - 2.0, at.y + 4.0)],
        egui::Stroke::new(1.3, t.muted),
    ));
    egui::containers::menu::SubMenu::new().show(ui, &resp, add).map(|r| r.inner)
}

/// One row of a context menu: a picture in its own column, then the words. `glyph` is `None`
/// for a row that has no picture; its words still start where the others' do, so a column of
/// mixed rows lines up as it does in Office.
// i18n-ui-text: label
pub fn menu_row(ui: &mut Ui, glyph: Option<Glyph>, label: &str, t: &Tokens) -> Response {
    let label = crate::tl!(label);
    // Context-menu parents can report the full remaining window width. Cap the row so a
    // four-item menu does not become a large empty ribbon across the page.
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    take_first_row(ui, &resp);
    let painter = ui.painter_at(rect);
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    // The row the keyboard is on is ringed as well as lit (issue #380), so a menu opened from
    // the keyboard shows where Enter will go.
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    if let Some(glyph) = glyph {
        glyph.paint(&painter, Rect::from_center_size(egui::pos2(rect.min.x + ICON_COL / 2.0, rect.center().y), vec2(GLYPH, GLYPH)), t);
    }
    painter.text(egui::pos2(rect.min.x + ICON_COL + 2.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp
}

/// A menu row that is greyed out and does nothing when `enabled` is false (issue #441: a locked
/// page's edits stay in its menu, shown as unavailable).
pub fn menu_row_if(ui: &mut Ui, enabled: bool, glyph: Option<Glyph>, label: &str, t: &Tokens) -> Response {
    if enabled {
        return menu_row(ui, glyph, label, t);
    }
    let muted = Tokens { text: t.muted, ..*t };
    ui.add_enabled_ui(false, |ui| menu_row(ui, glyph, label, &muted)).inner
}

/// A menu row with its keyboard shortcut right-aligned in a column of its own (issue #380), so
/// the shortcut is not run into the words of the label.
// i18n-ui-text: label
pub fn menu_row_shortcut(ui: &mut Ui, glyph: Option<Glyph>, label: &str, shortcut: &str, t: &Tokens) -> Response {
    let resp = menu_row(ui, glyph, label, t);
    ui.painter_at(resp.rect).text(
        egui::pos2(resp.rect.max.x - 10.0, resp.rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        egui::FontId::proportional(12.0),
        t.muted,
    );
    resp
}

/// A menu row for an action that removes something (issue #380): the picture and the words
/// in the warning colour, so Delete never reads as one more ordinary entry.
// i18n-ui-text: label
pub fn menu_row_danger(ui: &mut Ui, glyph: Option<Glyph>, label: &str, t: &Tokens) -> Response {
    let label = crate::tl!(label);
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    take_first_row(ui, &resp);
    let painter = ui.painter_at(rect);
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    // The row the keyboard is on is ringed as well as lit (issue #380), so a menu opened from
    // the keyboard shows where Enter will go.
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    let icon = Rect::from_center_size(egui::pos2(rect.min.x + ICON_COL / 2.0, rect.center().y), vec2(GLYPH, GLYPH));
    match glyph {
        Some(Glyph::Mark(m)) => ribbon_icons::paint(&painter, icon, m, t.warn),
        Some(Glyph::Icon(i)) => icons::paint(&painter, icon, i, t.warn),
        None => {}
    }
    painter.text(egui::pos2(rect.min.x + ICON_COL + 2.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), t.warn);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp
}

/// A menu row with a tick in the picture column when `checked`, the way OneNote lists the
/// things a menu can turn on and off.
// i18n-ui-text: label
pub fn menu_check_row(ui: &mut Ui, checked: bool, label: &str, t: &Tokens) -> Response {
    let label = crate::tl!(label);
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    take_first_row(ui, &resp);
    let painter = ui.painter_at(rect);
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    // The row the keyboard is on is ringed as well as lit (issue #380), so a menu opened from
    // the keyboard shows where Enter will go.
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    if checked {
        let c = egui::pos2(rect.min.x + ICON_COL / 2.0, rect.center().y);
        painter.add(egui::Shape::line(vec![c + vec2(-4.5, 0.5), c + vec2(-1.5, 3.5), c + vec2(4.5, -3.5)], egui::Stroke::new(1.8, t.text)));
    }
    painter.text(egui::pos2(rect.min.x + ICON_COL + 2.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::CheckBox, enabled, checked, name.clone()));
    resp
}

/// A thin rule between groups of rows, starting where the words do, as Office draws it.
pub fn menu_separator(ui: &mut Ui, t: &Tokens) {
    let width = ui.available_width().min(MENU_W);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 7.0), Sense::hover());
    let y = rect.center().y.round() + 0.5;
    ui.painter().hline(rect.min.x + ICON_COL..=rect.max.x, y, egui::Stroke::new(1.0, t.rule_line));
}

/// One button on a mini toolbar.
#[derive(Clone, Debug, PartialEq)]
pub struct BarButton {
    pub glyph: Glyph,
    /// What assistive tools and tests call it.
    pub label: &'static str,
    /// What the pointer shows when it rests on it.
    pub tip: &'static str,
    /// What it asks the window to do.
    pub act: Act,
}

/// The font name and font size boxes shown on a mini toolbar, as on the ribbon. The ribbon keeps
/// these up to date in egui memory under [`FONT_BOXES_KEY`], and the page menu reads them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontBoxes {
    pub family: String,
    pub size: String,
    pub families: Vec<String>,
    pub families_note: Option<String>,
    pub sizes: Vec<String>,
    /// Recently used families, most recent first, shared with the ribbon's family box.
    pub recent: Vec<String>,
    /// The family shown is not installed, so the box shows its warning mark.
    pub family_missing: bool,
    /// The installed families grouped by script, with a heading for each.
    pub sections: std::sync::Arc<Vec<(String, Vec<String>)>>,
}

/// The egui memory key under which the ribbon stores the [`FontBoxes`] for this frame.
pub const FONT_BOXES_KEY: &str = "notecraft-font-boxes";

/// A mini toolbar: rows of buttons. Plain data, so any place can build its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    /// The font boxes, drawn in a row above the buttons, when there are any.
    pub fonts: Option<FontBoxes>,
    pub rows: Vec<Vec<BarButton>>,
}

// i18n-ui-text: label,tip
fn button(glyph: Glyph, label: &'static str, tip: &'static str, act: Act) -> BarButton {
    BarButton { glyph, label, tip, act }
}

/// The toolbar OneNote shows over a right click on text, with what NoteCraft can do. The
/// editor has no fonts, sizes, colours or highlighter to offer, so they are left off rather
/// than drawn and ignored.
pub fn text_bar() -> Bar {
    let m = Glyph::Mark;
    let i = Glyph::Icon;
    Bar {
        fonts: None,
        rows: vec![
            vec![
                button(m(Mark::Bold), "Bold", "Bold (Ctrl+B)", Act::Mark("bold")),
                button(m(Mark::Italic), "Italic", "Italic (Ctrl+I)", Act::Mark("italic")),
                button(m(Mark::Strike), "Strikethrough", "Strike through", Act::Mark("strike")),
                button(i(Icon::Code), "Code", "Code, in one line", Act::Mark("code")),
                button(i(Icon::ListBullet), "Bullets", "A bullet list", Act::Block("bullet")),
                button(i(Icon::ListNumber), "Numbering", "A numbered list", Act::Block("number")),
            ],
            vec![
                button(m(Mark::IndentLess), "Decrease indentation", "Decrease indentation (Shift+Tab)", Act::Indent(false)),
                button(m(Mark::IndentMore), "Increase indentation", "Increase indentation (Tab)", Act::Indent(true)),
                button(i(Icon::Quote), "Quote", "Quote this paragraph", Act::Block("quote")),
                button(m(Mark::ToDo), "To Do", "Tag this line as a to-do", Act::Tag(Tag::ToDo)),
                button(m(Mark::Important), "Important", "Tag this line as important", Act::Tag(Tag::Important)),
                button(m(Mark::ClearFormat), "Clear Formatting", "Take the marks off", Act::ClearFormatting),
            ],
        ],
    }
}

/// The mini toolbar with the font boxes on top, as on the ribbon.
pub fn text_bar_with_fonts(fonts: FontBoxes) -> Bar {
    Bar { fonts: Some(fonts), ..text_bar() }
}

const BTN: f32 = 26.0;

/// What drawing a bar did.
pub struct BarOut {
    /// Where it is on the screen, so the caller can tell a click on it from a click away from it.
    pub rect: Rect,
    /// True when a button was pressed. Its act is in the `acts` the caller passed.
    pub picked: bool,
}

/// Draws `bar` over `anchor` (a menu, a box, a selection), or under it when there is no room
/// above, and pushes the act of a pressed button into `acts`. `id` must differ between bars that
/// can be open at once.
pub fn show_bar(ctx: &Context, id: Id, anchor: Rect, bar: &Bar, t: &Tokens, acts: &mut Vec<Act>) -> BarOut {
    let rows = bar.rows.len() as f32 + if bar.fonts.is_some() { 1.0 } else { 0.0 };
    let room_above = anchor.min.y - ctx.content_rect().min.y;
    let want = rows * (BTN + 2.0) + 14.0;
    let (pivot, at) = if room_above >= want + 4.0 {
        (Align2::LEFT_BOTTOM, egui::pos2(anchor.min.x, anchor.min.y - 4.0))
    } else {
        (Align2::LEFT_TOP, egui::pos2(anchor.min.x, anchor.max.y + 4.0))
    };
    let mut picked = false;
    let area = Area::new(id).order(Order::Foreground).pivot(pivot).fixed_pos(at).constrain(true).show(ctx, |ui| {
        Frame::popup(ui.style()).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
            if let Some(fonts) = &bar.fonts {
                ui.horizontal(|ui| {
                    let families = crate::combo::show(
                        ui,
                        t.text,
                        crate::combo::Spec {
                            id: id.with("font-family"),
                            box_label: crate::tl_id!("ui.font_family_box"),
                            arrow_label: crate::tl_id!("ui.font_family"),
                            shown: &fonts.family,
                            width: 96.0,
                            items: &fonts.families,
                            empty_note: fonts.families_note.as_deref(),
                            accept: &|typed| notecraft_formats::markdown::valid_font_family(typed),
                            filter: true,
                            recent: &fonts.recent,
                            missing: fonts.family_missing,
                            preview: true,
                            sections: &fonts.sections,
                            add_font: Some(crate::tl_id!("ui.add_font")),
                            usage: "floating-family",
                            tokens: *t,
                        },
                    );
                    if families.add_font {
                        // The Add Font row: pick font files, install them, and read the list again.
                        if let Some(report) = crate::font_install::pick_and_install()
                            && !report.copied.is_empty()
                        {
                            crate::fonts::rescan_in_background(ui.ctx().clone());
                        }
                    }
                    if let Some(name) = families.chosen {
                        crate::ribbon::remember_family(ui.ctx(), &name);
                        acts.push(Act::FontFamily(name));
                        picked = true;
                    }
                    let size = crate::combo::show(
                        ui,
                        t.text,
                        crate::combo::Spec {
                            id: id.with("font-size"),
                            box_label: crate::tl_id!("ui.font_size_box"),
                            arrow_label: crate::tl_id!("ui.font_size"),
                            shown: &fonts.size,
                            width: 38.0,
                            items: &fonts.sizes,
                            empty_note: None,
                            accept: &|typed| crate::font_picker::size_is_usable(typed, &crate::ribbon::FONT_SIZES),
                            filter: false,
                            recent: &[],
                            missing: false,
                            preview: false,
                            sections: &[],
                            add_font: None,
                            usage: "floating-size",
                            tokens: *t,
                        },
                    );
                    if let Some(chosen) = size.chosen.and_then(|typed| typed.parse::<u8>().ok()) {
                        acts.push(Act::FontSize(chosen));
                        picked = true;
                    }
                });
            }
            for row in &bar.rows {
                ui.horizontal(|ui| {
                    for b in row {
                        if bar_button(ui, b, t).clicked() {
                            acts.push(b.act.clone());
                            picked = true;
                        }
                    }
                });
            }
        });
    });
    BarOut { rect: area.response.rect, picked }
}

fn bar_button(ui: &mut Ui, b: &BarButton, t: &Tokens) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(BTN, BTN), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    // The row the keyboard is on is ringed as well as lit (issue #380), so a menu opened from
    // the keyboard shows where Enter will go.
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    b.glyph.paint(&painter, Rect::from_center_size(rect.center(), vec2(GLYPH, GLYPH)), t);
    let enabled = ui.is_enabled();
    let name = crate::tl!(b.label).to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(crate::tl!(b.tip))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn the_text_bar_has_distinct_buttons_that_each_ask_for_something() {
        let bar = text_bar();
        let all: Vec<&BarButton> = bar.rows.iter().flatten().collect();
        assert!(all.len() >= 10, "a toolbar with something on it: {}", all.len());
        let labels: std::collections::HashSet<&str> = all.iter().map(|b| b.label).collect();
        assert_eq!(labels.len(), all.len(), "no two buttons share a name");
        for b in &all {
            assert!(!b.tip.is_empty(), "{} says what it does", b.label);
        }
        for (i, a) in all.iter().enumerate() {
            for b in all.iter().skip(i + 1) {
                assert_ne!(a.act, b.act, "{} and {} would do the same thing", a.label, b.label);
            }
        }
    }

    #[test]
    fn a_bar_can_be_built_from_data_and_drawn_anywhere() {
        let t = Theme::DEFAULT.tokens();
        let bar = Bar { fonts: None, rows: vec![vec![button(Glyph::Mark(Mark::Cut), "Cut", "Cut", Act::Clipboard("cut"))]] };
        let ctx = Context::default();
        let mut acts = Vec::new();
        let mut out = None;
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            out = Some(show_bar(ui.ctx(), Id::new("test-bar"), Rect::from_min_size(egui::pos2(100.0, 200.0), vec2(50.0, 50.0)), &bar, &t, &mut acts));
        });
        frame.textures_delta.clear();
        let out = out.expect("the bar was drawn");
        assert!(out.rect.width() > 0.0 && out.rect.height() > 0.0, "it has a size: {:?}", out.rect);
        assert!(!out.picked && acts.is_empty(), "nothing was pressed");
    }

    #[test]
    fn a_bar_with_no_room_above_goes_below() {
        let t = Theme::DEFAULT.tokens();
        let bar = text_bar();
        let ctx = Context::default();
        let mut acts = Vec::new();
        let anchor = Rect::from_min_size(egui::pos2(100.0, 4.0), vec2(50.0, 50.0));
        let mut out = None;
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            out = Some(show_bar(ui.ctx(), Id::new("test-bar-below"), anchor, &bar, &t, &mut acts));
        });
        frame.textures_delta.clear();
        let out = out.expect("the bar was drawn");
        assert!(out.rect.min.y >= anchor.max.y, "it sits under the anchor when the top of the window is too close: {:?}", out.rect);
    }
}

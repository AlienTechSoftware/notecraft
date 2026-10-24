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

/// One row of a context menu: a picture in its own column, then the words. `glyph` is `None`
/// for a row that has no picture; its words still start where the others' do, so a column of
/// mixed rows lines up as it does in Office.
pub fn menu_row(ui: &mut Ui, glyph: Option<Glyph>, label: &str, t: &Tokens) -> Response {
    // Context-menu parents can report the full remaining window width. Cap the row so a
    // four-item menu does not become a large empty ribbon across the page.
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
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

/// A menu row with a tick in the picture column when `checked`, the way OneNote lists the
/// things a menu can turn on and off.
pub fn menu_check_row(ui: &mut Ui, checked: bool, label: &str, t: &Tokens) -> Response {
    let width = ui.available_width().min(MENU_W);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_H), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    if checked {
        let c = egui::pos2(rect.min.x + ICON_COL / 2.0, rect.center().y);
        painter.add(egui::Shape::line(vec![c + vec2(-4.5, 0.5), c + vec2(-1.5, 3.5), c + vec2(4.5, -3.5)], egui::Stroke::new(1.8, t.text)));
    }
    painter.text(egui::pos2(rect.min.x + ICON_COL + 2.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, checked, name.clone()));
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

/// A mini toolbar: rows of buttons. Plain data, so any place can build its own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bar {
    pub rows: Vec<Vec<BarButton>>,
}

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
                button(m(Mark::IndentLess), "Decrease indent", "Less indent (Shift+Tab)", Act::Indent(false)),
                button(m(Mark::IndentMore), "Increase indent", "More indent (Tab)", Act::Indent(true)),
                button(i(Icon::Quote), "Quote", "Quote this paragraph", Act::Block("quote")),
                button(m(Mark::ToDo), "To Do", "Tag this line as a to-do", Act::Tag(Tag::ToDo)),
                button(m(Mark::Important), "Important", "Tag this line as important", Act::Tag(Tag::Important)),
                button(m(Mark::ClearFormat), "Clear Formatting", "Take the marks off", Act::ClearFormatting),
            ],
        ],
    }
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
    let rows = bar.rows.len() as f32;
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
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    b.glyph.paint(&painter, Rect::from_center_size(rect.center(), vec2(GLYPH, GLYPH)), t);
    let enabled = ui.is_enabled();
    let name = b.label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(b.tip)
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
        let bar = Bar { rows: vec![vec![button(Glyph::Mark(Mark::Cut), "Cut", "Cut", Act::Clipboard("cut"))]] };
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

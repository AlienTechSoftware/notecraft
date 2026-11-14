//! An editable combo box: a value box that shows the current value, with a drop-down arrow whose
//! list opens under the box. The font name and font size controls use it, so the arrow, the list
//! and the typing behave the same everywhere.
//!
//! The arrow is a generous hit region with a tooltip and an accessible label. The list highlights
//! the current value and scrolls to it once each time it opens. A typed value is applied when the
//! box loses focus, if the caller accepts it.
//!
//! With `filter` on (the font name box), typing lists only the items that contain the text, Up and
//! Down move the highlight through them, and Enter applies the highlighted item (an item that
//! equals the typed text wins). Escape restores the shown value.

use egui::{Align, Area, Color32, Frame, Id, Key, Modifiers, Order, Rect, ScrollArea, Shape, Stroke, Ui, WidgetInfo, WidgetType, vec2};

/// What the user chose this frame: a value picked from the list, or typed and accepted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub chosen: Option<String>,
}

/// The parts of one combo box that the caller supplies.
pub struct Spec<'a> {
    /// Keeps the box's typed text, the open state and the scroll state apart from other combos.
    pub id: Id,
    /// The accessible name of the value box.
    pub box_label: &'a str,
    /// The accessible name and tooltip of the drop-down arrow.
    pub arrow_label: &'a str,
    /// The value shown in the box and highlighted in the list.
    pub shown: &'a str,
    /// The width of the value box in points.
    pub width: f32,
    /// The items of the list, in order.
    pub items: &'a [String],
    /// Shown in the list instead of items when there are none (for example, the font catalog is
    /// unavailable).
    pub empty_note: Option<&'a str>,
    /// Whether a typed value is accepted when the box loses focus.
    pub accept: &'a dyn Fn(&str) -> bool,
    /// Whether typing lists only the items that contain the text, with Up/Down/Enter to pick.
    pub filter: bool,
}

/// The items that contain `query` (ignoring case), in list order. Every item when `query` is empty.
fn matching<'a>(items: &'a [String], query: &str) -> Vec<&'a String> {
    let query = query.trim().to_lowercase();
    items.iter().filter(|item| query.is_empty() || item.to_lowercase().contains(&query)).collect()
}

/// Shows the combo box and returns what was chosen this frame.
pub fn show(ui: &mut Ui, text: Color32, spec: Spec<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    let typed_id = spec.id.with("typed");
    let open_id = spec.id.with("open");
    let scroll_id = spec.id.with("scroll");
    let highlight_id = spec.id.with("highlight");
    let list_rect_id = spec.id.with("list-rect");

    let mut typed = ui.data(|d| d.get_temp::<String>(typed_id)).unwrap_or_else(|| spec.shown.to_owned());
    let mut highlight = ui.data(|d| d.get_temp::<usize>(highlight_id)).unwrap_or(0);
    let was_focused = ui.memory(|m| m.has_focus(typed_id));
    // Up and Down are read before the box sees them, so the highlight moves and the caret does not.
    let (down, up) = if spec.filter && was_focused {
        ui.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp)))
    } else {
        (false, false)
    };
    let cancelled = ui.input(|i| i.key_pressed(Key::Escape));
    let entered = ui.input(|i| i.key_pressed(Key::Enter));
    let value_box = ui.add(egui::TextEdit::singleline(&mut typed).id(typed_id).desired_width(spec.width));
    value_box.widget_info(|| WidgetInfo::labeled(WidgetType::TextInput, true, spec.box_label));
    if value_box.changed() {
        highlight = 0;
    }
    // Typing lists the matches while the box has focus and its text is not the shown value.
    let typing = spec.filter && typed.trim() != spec.shown && (was_focused || value_box.has_focus());
    let hits = if typing { matching(spec.items, &typed) } else { Vec::new() };
    if down {
        highlight = (highlight + 1).min(hits.len().saturating_sub(1));
    }
    if up {
        highlight = highlight.saturating_sub(1);
    }
    highlight = highlight.min(hits.len().saturating_sub(1));

    // Read the entry before resetting an unfocused box. Enter also loses focus; Escape cancels
    // rather than applying the unfinished entry.
    if value_box.lost_focus() {
        // Formatting may return focus to the page in this same frame. The key belongs to this
        // field, not to the editor which happens to run next.
        if entered || cancelled {
            ui.input_mut(|i| i.consume_key(Modifiers::NONE, if cancelled { Key::Escape } else { Key::Enter }));
        }
        // A press on the list is not a finished entry: the list's own click applies its row.
        let list_rect: Option<Rect> = ui.data(|d| d.get_temp(list_rect_id));
        let on_list = match (ui.input(|i| i.pointer.hover_pos()), list_rect) {
            (Some(p), Some(r)) => r.contains(p),
            _ => false,
        };
        if !cancelled && !on_list {
            let exact = spec.items.iter().find(|item| item.eq_ignore_ascii_case(typed.trim()));
            let value = match (spec.filter && entered, exact, hits.get(highlight)) {
                (true, Some(item), _) => item.clone(),
                (true, None, Some(hit)) => (*hit).clone(),
                _ => typed.trim().to_owned(),
            };
            if value != spec.shown && (spec.accept)(&value) {
                outcome.chosen = Some(value);
            }
        }
    }
    if !value_box.has_focus() {
        typed = spec.shown.to_owned();
    }
    ui.data_mut(|d| {
        d.insert_temp(typed_id, typed);
        d.insert_temp(highlight_id, highlight);
    });

    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    let mut scroll = ui.data(|d| d.get_temp::<bool>(scroll_id)).unwrap_or(false);
    let arrow = ui.add(egui::Button::new("").min_size(vec2(18.0, 20.0)));
    {
        let c = arrow.rect.center();
        let points = vec![c + vec2(-4.0, -2.0), c + vec2(4.0, -2.0), c + vec2(0.0, 3.0)];
        ui.painter().add(Shape::convex_polygon(points, text, Stroke::NONE));
    }
    arrow.widget_info(|| {
        let mut info = WidgetInfo::labeled(WidgetType::Button, true, spec.arrow_label);
        info.current_text_value = Some(spec.shown.to_owned());
        info
    });
    let arrow = arrow.on_hover_text(spec.arrow_label);
    if arrow.clicked() {
        open = !open;
        scroll = open;
    }

    // The list: the matches while typing, otherwise all items when the arrow has opened it.
    let show_list = typing || open;
    let mut list_rect = None;
    if show_list {
        let rows: Vec<&String> = if typing { hits } else { spec.items.iter().collect() };
        let list = Area::new(spec.id.with("list")).order(Order::Foreground).fixed_pos(value_box.rect.left_bottom()).show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(value_box.rect.width());
                if rows.is_empty() {
                    if let Some(note) = spec.empty_note {
                        ui.label(note);
                    }
                    return;
                }
                ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    for (index, item) in rows.iter().enumerate() {
                        let current = if typing { index == highlight } else { item.as_str() == spec.shown };
                        let row = ui.selectable_label(current, item.as_str());
                        if current && (scroll || typing) {
                            row.scroll_to_me(Some(Align::Center));
                        }
                        if row.clicked() {
                            outcome.chosen = Some((*item).clone());
                            open = false;
                            ui.memory_mut(|m| m.surrender_focus(typed_id));
                        }
                    }
                });
            });
        });
        list_rect = Some(list.response.rect);
        if !typing && ui.input(|i| i.pointer.any_click()) && !list.response.hovered() && !arrow.hovered() {
            open = false;
        }
    }
    scroll = false;
    ui.data_mut(|d| {
        d.insert_temp(scroll_id, scroll);
        d.insert_temp(open_id, open);
        if let Some(r) = list_rect {
            d.insert_temp(list_rect_id, r);
        } else {
            d.remove::<Rect>(list_rect_id);
        }
    });
    outcome
}

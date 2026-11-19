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
    /// The Add Font row was picked: the caller opens the file picker.
    pub add_font: bool,
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
    /// Families used lately, most recent first. Shown under Recently Used Fonts above All Fonts
    /// when the box is not being typed in. Empty for the size box.
    pub recent: &'a [String],
    /// The shown value is not an installed font: a warning mark shows beside the box, with the
    /// tooltip `ui.font_missing`.
    pub missing: bool,
    /// Each family name is drawn in its own font, as a sample, once that font is loaded. The
    /// fonts load as the rows come into view.
    pub preview: bool,
    /// The list grouped by script: each heading with its families. Empty when there is no
    /// grouping (the size box, or while the catalog loads).
    pub sections: &'a [(String, Vec<String>)],
    /// The label of the Add Font row under the list, or `None` when the box has none.
    pub add_font: Option<&'a str>,
    /// Which box this is, for usage events (for example "ribbon-family"). Never a name or a path.
    pub usage: &'static str,
    /// The theme of the surrounding window. The list takes the same popup look as the language
    /// pickers, so its text is never the ribbon's foreground on a light list.
    pub tokens: crate::theme::Tokens,
}

/// A script heading in the font list: bold, on a band, so the list scrolls from one script to the
/// next and the eye finds each one.
fn section_heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    egui::Frame::default().fill(ui.visuals().faint_bg_color).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.label(egui::RichText::new(text).strong());
    });
}

/// Rows drawn beyond the visible window, in points, so a fast scroll does not show blank space.
const VIEW_MARGIN: f32 = 60.0;
/// The height a script heading takes, for the rows that are skipped without being drawn.
const HEADING_H: f32 = 28.0;

/// One row of a font list. Records a visible name so its font loads, and scrolls to the row when
/// asked to.
fn font_row(ui: &mut Ui, name: &str, current: bool, preview: bool, wanted: &mut Vec<String>, scroll_to: bool) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    // The family list: names indented under their heading, at least one row high, so a taller face
    // moves the rows below only a little. Drawn inside the list's own clip, so nothing spills out.
    // The size list is drawn as it always was.
    const INDENT: f32 = 24.0;
    let row = if preview {
        ui.horizontal(|ui| {
            ui.set_min_height(height);
            ui.add_space(INDENT);
            ui.selectable_label(current, name_text(ui, name, preview))
        })
        .inner
    } else {
        ui.add_sized(egui::vec2(ui.available_width(), height), egui::Button::selectable(current, name_text(ui, name, preview)).truncate())
    };
    if preview && ui.is_rect_visible(row.rect) {
        wanted.push(name.to_owned());
    }
    if current && scroll_to {
        row.scroll_to_me(Some(Align::Center));
    }
    row
}

/// The text of one list row: the name, drawn in its own font when `preview` is on and that font
/// is already registered. A font that is not registered yet draws in the interface font.
fn name_text(ui: &Ui, name: &str, preview: bool) -> egui::RichText {
    let text = egui::RichText::new(name);
    if preview {
        let family = crate::fonts::named_family(name);
        if ui.ctx().fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
            return text.family(family);
        }
    }
    text
}

/// Shows the combo box and returns what was chosen this frame.
pub fn show(ui: &mut Ui, text: Color32, spec: Spec<'_>) -> Outcome {
    let mut outcome = Outcome::default();
    // How the value was chosen, for usage events: typed, or picked from the list.
    let mut how: &str = "";
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
    if spec.missing {
        // An amber triangle inside the box's right edge, as WordCraft draws it.
        let c = egui::pos2(value_box.rect.right() - 8.0, value_box.rect.center().y);
        let points = vec![c + vec2(-5.0, 4.0), c + vec2(5.0, 4.0), c + vec2(0.0, -5.0)];
        ui.painter().add(Shape::convex_polygon(points, Color32::from_rgb(0xE0, 0x8A, 0x00), Stroke::NONE));
        let warning = value_box.rect.with_min_x(value_box.rect.right() - 18.0);
        ui.interact(warning, value_box.id.with("missing"), egui::Sense::hover()).on_hover_text(crate::tl_id!("ui.font_missing"));
    }
    value_box.widget_info(|| WidgetInfo::labeled(WidgetType::TextInput, true, spec.box_label));
    if value_box.changed() {
        highlight = 0;
    }
    // Typing lists the matches while the box has focus and its text is not the shown value.
    // A press on a row of the typed list takes focus from the box. The list must keep its matches
    // until the button is released, or the row under the pointer changes before the click lands.
    let last_typing: bool = ui.data(|d| d.get_temp(spec.id.with("last-typing"))).unwrap_or(false);
    let list_under_pointer: bool = match (ui.input(|i| i.pointer.hover_pos()), ui.data(|d| d.get_temp::<Rect>(list_rect_id))) {
        (Some(p), Some(r)) => r.contains(p),
        _ => false,
    };
    let pressing_list = list_under_pointer && ui.input(|i| i.pointer.primary_down());
    // Armed by a press on the typed list; it stays armed through the release, so the release
    // lands on the row that was pressed.
    let armed: bool = ui.data(|d| d.get_temp(spec.id.with("press-armed"))).unwrap_or(false);
    let released = ui.input(|i| i.pointer.primary_released());
    let keep_typing = (pressing_list && last_typing) || armed;
    let typing = spec.filter && ((typed.trim() != spec.shown && (was_focused || value_box.has_focus())) || keep_typing);
    // Typing lists the families whose names match, and also every family of a script whose heading
    // matches (typing "arabic" lists the Arabic fonts). Each heading's families come first.
    let query = typed.trim().to_lowercase();
    let mut heading_starts: Vec<(usize, &String)> = Vec::new();
    let hits: Vec<&String> = if typing {
        let mut placed = std::collections::BTreeSet::new();
        let mut flat: Vec<&String> = Vec::new();
        if !query.is_empty() {
            for (heading, names) in spec.sections {
                if heading.to_lowercase().contains(&query) {
                    heading_starts.push((flat.len(), heading));
                    for name in names {
                        if placed.insert(name.clone()) {
                            flat.push(name);
                        }
                    }
                }
            }
        }
        for index in crate::font_picker::matches(&typed, spec.items) {
            let name = &spec.items[index];
            if !placed.contains(name) {
                flat.push(name);
            }
        }
        flat
    } else {
        Vec::new()
    };
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
        // Enter commits even with the pointer over the list; a click away does not commit on the list.
        if !cancelled && (entered || !on_list) {
            // Enter takes the highlighted match. The ranking puts an exact name first, so the highlight is
            // the name shown selected. A click away takes only an exact family name: half-typed text
            // ("geo") is not applied as a font just because the box lost focus.
            let exact = spec
                .items
                .iter()
                .find(|item| item.as_str() == typed.trim())
                .or_else(|| spec.items.iter().find(|item| item.eq_ignore_ascii_case(typed.trim())));
            let value: Option<String> = match (spec.filter, entered, exact, hits.get(highlight)) {
                (true, true, _, Some(hit)) => Some((*hit).clone()),
                (true, false, Some(item), _) => Some(item.clone()),
                (true, false, None, _) => None,
                _ => Some(typed.trim().to_owned()),
            };
            if let Some(value) = value.filter(|value| value != spec.shown && (spec.accept)(value)) {
                how = "typed";
                outcome.chosen = Some(value);
            }
        }
    }
    if !value_box.has_focus() && !keep_typing {
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
        if open {
            crate::usage::record(crate::usage::Level::Usage, "combo_opened", &[("box", spec.usage)]);
        }
    }

    // The list: the matches while typing, otherwise all items when the arrow has opened it.
    let show_list = typing || open;
    // While typing, scroll the highlighted row into view only when the highlight has moved.
    let scrolled_key = spec.id.with("scrolled-highlight");
    let scroll_highlight = typing && ui.data(|d| d.get_temp::<usize>(scrolled_key)) != Some(highlight);
    if typing {
        ui.data_mut(|d| d.insert_temp(scrolled_key, highlight));
    }
    let mut list_rect = None;
    if show_list {
        let rows: Vec<&String> = if typing { hits } else { spec.items.iter().collect() };
        // Recently used families go above the full list, when the box is not being typed in.
        let recent_rows: Vec<&String> = if typing { Vec::new() } else { spec.recent.iter().collect() };
        // The script sections replace the one full list when the box is not being typed in.
        let by_script = !typing && !spec.sections.is_empty();
        // Names whose rows are on screen this frame; their fonts load after the list is drawn.
        let mut wanted: Vec<String> = Vec::new();
        let list = Area::new(spec.id.with("list")).order(Order::Foreground).fixed_pos(value_box.rect.left_bottom()).show(ui.ctx(), |ui| {
            // The family list takes the picker colours, so its text reads on the light list.
            if spec.preview {
                crate::theme::picker_popup_style(spec.tokens).apply(ui.style_mut());
            }
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(value_box.rect.width());
                // An empty list still shows its note and the Add Font row below, so a font can be
                // added even when the installed fonts could not be read.
                if rows.is_empty()
                    && let Some(note) = spec.empty_note
                {
                    ui.label(note);
                }
                // The list is at most 280 points tall, and never taller than the space the popup has. The rows
                // scroll inside it, and nothing draws outside it: the popup's clip is intersected, not replaced.
                let height = ui.available_height().min(280.0);
                ui.set_max_height(height);
                let window = egui::Rect::from_min_size(ui.max_rect().min, egui::vec2(ui.max_rect().width(), height));
                ui.set_clip_rect(ui.clip_rect().intersect(window));
                // Only the rows on screen are laid out and their fonts asked for; the rows above and below keep
                // their space, so the scroll positions are the same as if every row were drawn.
                ScrollArea::vertical().max_height(height).auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
                    let origin = ui.min_rect().min;
                    let row_h = ui.spacing().interact_size.y;
                    let spacing = ui.spacing().item_spacing.y;
                    let width = ui.available_width();
                    let mut y = 0.0_f32;
                    // Returns whether the item is near the window; off-screen items only take their space.
                    let cull = |ui: &mut Ui, y: &mut f32, h: f32| -> (bool, f32) {
                        let top = *y;
                        *y += h + spacing;
                        let shown = top + h >= viewport.min.y - VIEW_MARGIN && top <= viewport.max.y + VIEW_MARGIN;
                        if !shown {
                            ui.add_space(h + spacing);
                        }
                        (shown, top)
                    };
                    if !recent_rows.is_empty() {
                        let (shown, _) = cull(ui, &mut y, HEADING_H);
                        if shown {
                            ui.label(egui::RichText::new(crate::tl_id!("ui.recent_fonts")).small().weak());
                        }
                        for item in &recent_rows {
                            let (shown, top) = cull(ui, &mut y, row_h);
                            let current = item.as_str() == spec.shown;
                            if current && scroll {
                                ui.scroll_to_rect(
                                    egui::Rect::from_min_size(origin + egui::vec2(0.0, top), egui::vec2(width, row_h)),
                                    Some(Align::Center),
                                );
                            }
                            if shown {
                                let row = font_row(ui, item, current, spec.preview, &mut wanted, false);
                                if row.clicked() {
                                    how = "list";
                                    outcome.chosen = Some((*item).clone());
                                    open = false;
                                    ui.memory_mut(|m| m.surrender_focus(typed_id));
                                }
                            }
                        }
                        cull(ui, &mut y, 6.0);
                    }
                    if by_script {
                        // One heading per script, so the list scrolls from language to language.
                        for (heading, names) in spec.sections {
                            let (shown, _) = cull(ui, &mut y, HEADING_H);
                            if shown {
                                section_heading(ui, heading);
                            }
                            for item in names {
                                let (shown, top) = cull(ui, &mut y, row_h);
                                let current = item.as_str() == spec.shown;
                                if current && scroll {
                                    ui.scroll_to_rect(
                                        egui::Rect::from_min_size(origin + egui::vec2(0.0, top), egui::vec2(width, row_h)),
                                        Some(Align::Center),
                                    );
                                }
                                if shown {
                                    let row = font_row(ui, item, current, spec.preview, &mut wanted, false);
                                    if row.clicked() {
                                        how = "list";
                                        outcome.chosen = Some(item.clone());
                                        open = false;
                                        ui.memory_mut(|m| m.surrender_focus(typed_id));
                                    }
                                }
                            }
                        }
                    } else {
                        if !recent_rows.is_empty() {
                            let (shown, _) = cull(ui, &mut y, HEADING_H);
                            if shown {
                                ui.label(egui::RichText::new(crate::tl_id!("ui.all_fonts")).small().weak());
                            }
                        }
                        for (index, item) in rows.iter().enumerate() {
                            if let Some((_, heading)) = heading_starts.iter().find(|(start, _)| *start == index) {
                                let (shown, _) = cull(ui, &mut y, HEADING_H);
                                if shown {
                                    section_heading(ui, heading);
                                }
                            }
                            let (shown, top) = cull(ui, &mut y, row_h);
                            let current = if typing { index == highlight } else { item.as_str() == spec.shown };
                            let wants_scroll = scroll || (current && scroll_highlight);
                            if current && wants_scroll {
                                ui.scroll_to_rect(
                                    egui::Rect::from_min_size(origin + egui::vec2(0.0, top), egui::vec2(width, row_h)),
                                    Some(Align::Center),
                                );
                            }
                            if shown {
                                let row = font_row(ui, item, current, spec.preview, &mut wanted, false);
                                if row.clicked() {
                                    how = "list";
                                    outcome.chosen = Some((*item).clone());
                                    open = false;
                                    ui.memory_mut(|m| m.surrender_focus(typed_id));
                                }
                            }
                        }
                    }
                    if let Some(label) = spec.add_font {
                        ui.separator();
                        if ui.selectable_label(false, label).clicked() {
                            outcome.add_font = true;
                            open = false;
                            ui.memory_mut(|m| m.surrender_focus(typed_id));
                        }
                    }
                });
            });
        });
        if !wanted.is_empty() {
            // Registers the fonts of the visible rows. Drawn in them from the next frame.
            let names: std::collections::BTreeSet<String> = wanted.into_iter().collect();
            crate::fonts::ensure_page_fonts(ui.ctx(), &names);
        }
        list_rect = Some(list.response.rect);
        if !typing && ui.input(|i| i.pointer.any_click()) && !list.response.hovered() && !arrow.hovered() {
            open = false;
        }
    }
    scroll = false;
    ui.data_mut(|d| {
        d.insert_temp(scroll_id, scroll);
        d.insert_temp(open_id, open);
        d.insert_temp(spec.id.with("last-typing"), typing);
        d.insert_temp(spec.id.with("press-armed"), keep_typing && !released);
        if let Some(r) = list_rect {
            d.insert_temp(list_rect_id, r);
        } else {
            d.remove::<Rect>(list_rect_id);
        }
    });
    if let Some(name) = &outcome.chosen {
        crate::usage::record(
            crate::usage::Level::Usage,
            "combo_chosen",
            &[("box", spec.usage), ("how", how), ("chars", &name.chars().count().to_string())],
        );
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

    #[test]
    fn the_add_font_row_shows_when_the_list_is_empty() {
        // An empty list (the installed fonts could not be read) still offers Add Font.
        let mut h = Harness::new_ui(|ui| {
            let items: Vec<String> = Vec::new();
            let accept = |_: &str| true;
            show(
                ui,
                egui::Color32::BLACK,
                Spec {
                    id: egui::Id::new("empty-font-list"),
                    box_label: "Font name",
                    arrow_label: "Font Family",
                    shown: "Default",
                    width: 96.0,
                    items: &items,
                    empty_note: Some("No installed fonts"),
                    accept: &accept,
                    filter: true,
                    recent: &[],
                    missing: false,
                    preview: true,
                    sections: &[],
                    add_font: Some("Add Font…"),
                    usage: "test",
                    tokens: crate::theme::Theme::all()[0].tokens(),
                },
            );
        });
        h.run();
        h.get_by_label("Font Family").click();
        h.run();
        h.get_by_label("Add Font…");
    }
}

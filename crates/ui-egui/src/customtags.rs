//! Home > More tags > Customize Tags (issue #259): the dialog where a person makes tags of their
//! own, each with a name, a symbol and a colour, puts them in order and removes them.
//!
//! Every change is kept at once; the dialog has no Save button to forget. What a rename or a
//! removal does to the lines that carry a tag is decided in the model
//! ([`notecraft_model::tag`]): a rename keeps the tag on every line, a removal leaves the
//! marker word as ordinary words.

use crate::theme::Tokens;
use egui::{Align2, Context, Sense, Ui, vec2};
use notecraft_model::tag::SYMBOLS;
use notecraft_model::{CustomTag, NodeColor};

/// The dialog while it is open: the tags as they stand and the name being typed for a new one.
#[derive(Clone, Debug, Default)]
pub struct Dialog {
    pub list: Vec<CustomTag>,
    pub new_name: String,
}

/// What a frame of the dialog did.
#[derive(Default)]
pub struct Outcome {
    /// The dialog was closed.
    pub closed: bool,
    /// The tags changed; these are the new ones, to keep.
    pub changed: Option<Vec<CustomTag>>,
    /// Something to say about the change.
    pub said: Option<String>,
}

impl Dialog {
    pub fn new(list: Vec<CustomTag>) -> Dialog {
        Dialog { list, new_name: String::new() }
    }

    pub fn show(&mut self, ctx: &Context, t: &Tokens) -> Outcome {
        let mut out = Outcome::default();
        let mut open = true;
        let before = self.list.clone();
        egui::Window::new(crate::tl_id!("ui.customize_tags_title"))
            .collapsible(false)
            .resizable(false)
            .fixed_size(vec2(540.0, 0.0))
            .open(&mut open)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                // A fixed width: rows coming and going must not move the buttons under the pointer.
                ui.set_width(540.0);
                if self.list.is_empty() {
                    ui.weak(crate::tl_id!("ui.custom_tags_none_yet"));
                }
                let mut action: Option<(usize, Change)> = None;
                let count = self.list.len();
                for (i, tag) in self.list.iter_mut().enumerate() {
                    if let Some(change) = row(ui, i, count, tag, t) {
                        action = Some((i, change));
                    }
                }
                if let Some((i, change)) = action {
                    match change {
                        Change::Up if i > 0 => self.list.swap(i, i - 1),
                        Change::Down if i + 1 < self.list.len() => self.list.swap(i, i + 1),
                        Change::Remove => {
                            let gone = self.list.remove(i);
                            out.said = Some(crate::i18n::format_message_id("ui.tag_name_removed", &[("name", &gone.name)]));
                        }
                        _ => {}
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    let label = ui.label(crate::tl_id!("ui.new_tag_name"));
                    let edit = ui.add(egui::TextEdit::singleline(&mut self.new_name).desired_width(200.0)).labelled_by(label.id);
                    let typed = !self.new_name.trim().is_empty();
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.add_enabled(typed, egui::Button::new(crate::tl_id!("ui.add_tag"))).clicked() || enter) && typed {
                        let tag = CustomTag::named(&self.new_name, &self.list);
                        out.said = Some(crate::i18n::format_message_id("ui.tag_name_added", &[("name", &tag.name)]));
                        self.list.push(tag);
                        self.new_name.clear();
                    }
                });
                ui.add_space(4.0);
                ui.label(egui::RichText::new(crate::tl_id!("ui.custom_tags_explained")).color(t.muted).size(11.5));
                ui.add_space(4.0);
                ui.allocate_ui_with_layout(vec2(ui.available_width(), 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(crate::tl_id!("ui.close")).clicked() {
                        out.closed = true;
                    }
                });
            });
        if !open {
            out.closed = true;
        }
        if self.list != before {
            // A name cleared while it is being retyped is kept as the marker word meanwhile, so
            // the tag never drops off the lines that carry it.
            let keep: Vec<CustomTag> =
                self.list.iter().map(|c| if c.name.trim().is_empty() { CustomTag { name: c.id.clone(), ..c.clone() } } else { c.clone() }).collect();
            out.changed = Some(keep);
        }
        out
    }
}

enum Change {
    Up,
    Down,
    Remove,
}

/// One tag: its symbol and colour, its name, and the buttons that move and remove it.
fn row(ui: &mut Ui, i: usize, count: usize, tag: &mut CustomTag, t: &Tokens) -> Option<Change> {
    let mut change = None;
    ui.horizontal(|ui| {
        let name = tag.name.clone();
        // The symbol, drawn as the page draws it, with the list of symbols under it.
        let symbol_label = crate::i18n::format_message_id("ui.tag_symbol_for_name", &[("name", &name)]);
        let resp = picker(ui, 40.0, &symbol_label, t, |painter, rect| {
            crate::ribbon::paint_symbol(
                painter,
                egui::Rect::from_center_size(rect.center() - vec2(5.0, 0.0), vec2(18.0, 18.0)),
                &tag.symbol,
                tag.color,
            )
        });
        egui::Popup::menu(&resp).show(|ui| {
            egui::Grid::new(("custom-tag-symbols", i)).spacing(vec2(2.0, 2.0)).show(ui, |ui| {
                for (n, s) in SYMBOLS.iter().enumerate() {
                    if ui.selectable_label(tag.symbol == *s, egui::RichText::new(*s).size(16.0)).clicked() {
                        tag.symbol = s.to_string();
                    }
                    if n % 8 == 7 {
                        ui.end_row();
                    }
                }
            });
        });
        let colour_label = crate::i18n::format_message_id("ui.tag_colour_for_name", &[("name", &name)]);
        let colour_name = crate::tl!(tag.color.label()).to_string();
        let resp = picker(ui, 96.0, &colour_label, t, |painter, rect| {
            let swatch = egui::Rect::from_min_size(rect.left_center() + vec2(6.0, -6.0), vec2(12.0, 12.0));
            painter.rect_filled(swatch, 2.0, colour_of(painter.ctx(), tag.color));
            painter.text(swatch.right_center() + vec2(6.0, 0.0), Align2::LEFT_CENTER, &colour_name, egui::FontId::proportional(12.5), t.text);
        });
        egui::Popup::menu(&resp).show(|ui| {
            for c in NodeColor::ALL {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, colour_of(ui.ctx(), c));
                    ui.selectable_value(&mut tag.color, c, crate::tl!(c.label()));
                });
            }
        });
        let label = ui.label(egui::RichText::new(crate::tl_id!("ui.tag_name")).color(t.muted).size(11.5));
        ui.add(egui::TextEdit::singleline(&mut tag.name).desired_width(140.0)).labelled_by(label.id);
        ui.weak(format!("#{}", tag.id)).on_hover_text(crate::tl_id!("ui.the_word_written_in_the_page"));
        let small = |ui: &mut Ui, enabled: bool, glyph: &str, label: String| {
            let resp = ui.add_enabled(enabled, egui::Button::new(glyph)).on_hover_text(&label);
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
            resp.clicked()
        };
        if small(ui, i > 0, "↑", crate::i18n::format_message_id("ui.move_name_up", &[("name", &name)])) {
            change = Some(Change::Up);
        }
        if small(ui, i + 1 < count, "↓", crate::i18n::format_message_id("ui.move_name_down", &[("name", &name)])) {
            change = Some(Change::Down);
        }
        if small(ui, true, "×", crate::i18n::format_message_id("ui.remove_name", &[("name", &name)])) {
            change = Some(Change::Remove);
        }
    });
    change
}

fn colour_of(ctx: &Context, c: NodeColor) -> egui::Color32 {
    let (r, g, b) = c.rgb(ctx.global_style().visuals.dark_mode);
    egui::Color32::from_rgb(r, g, b)
}

/// A button that opens a list: what is chosen, drawn by `paint`, and an arrow, named for a
/// screen reader by `label`.
fn picker(ui: &mut Ui, width: f32, label: &str, t: &Tokens, paint: impl FnOnce(&egui::Painter, egui::Rect)) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.0, if resp.hovered() { t.accent } else { t.faint }), egui::StrokeKind::Inside);
    paint(&painter, rect);
    let c = egui::pos2(rect.max.x - 9.0, rect.center().y + 2.0);
    painter.line_segment([egui::pos2(c.x - 3.5, c.y - 3.0), c], egui::Stroke::new(1.3, t.text));
    painter.line_segment([c, egui::pos2(c.x + 3.5, c.y - 3.0)], egui::Stroke::new(1.3, t.text));
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, label));
    resp.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    /// Every symbol a custom tag can take draws in the app's own fonts, not as an empty box.
    #[test]
    fn every_tag_symbol_draws() {
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        for s in notecraft_model::tag::SYMBOLS.into_iter().chain(["↑", "↓", "×"]) {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), s)), "{s} would draw as a box");
        }
    }
}

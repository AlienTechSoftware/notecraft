//! View > Tabs layout (issues #268 and #33): the open notebook's sections as coloured tabs
//! across the top of the page, as OneNote 2016 shows them. The tree in the side panel stays,
//! so this is a second way to move between sections, not a replacement for the first.
//!
//! A section group is a tab of its own that opens a list of what is inside it. A row of tabs
//! wider than the page scrolls sideways, and in a right-to-left interface it runs from the
//! right.

use crate::theme::Tokens;
use egui::{Sense, Ui, vec2};
use notecraft_model::{NodeColor, NodeKind};
use notecraft_store::tree::Node;
use std::path::{Path, PathBuf};

/// One tab: a section, or a group of them.
#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub path: PathBuf,
    pub title: String,
    pub colour: Option<NodeColor>,
    /// For a section group, what is inside it (sections and groups, flattened in order).
    pub inside: Option<Vec<Tab>>,
}

/// The tabs for a notebook: its sections and section groups, in the notebook's order.
pub fn tabs(notebook: &Node) -> Vec<Tab> {
    notebook
        .children
        .iter()
        .filter_map(|n| match n.info.kind {
            NodeKind::Section => Some(Tab { path: n.path.clone(), title: n.info.title.clone(), colour: n.info.color, inside: None }),
            NodeKind::SectionGroup => Some(Tab { path: n.path.clone(), title: n.info.title.clone(), colour: n.info.color, inside: Some(flatten(n)) }),
            _ => None,
        })
        .collect()
}

/// Every section below a group, groups within it included, in order.
fn flatten(group: &Node) -> Vec<Tab> {
    let mut out = Vec::new();
    for n in &group.children {
        match n.info.kind {
            NodeKind::Section => out.push(Tab { path: n.path.clone(), title: n.info.title.clone(), colour: n.info.color, inside: None }),
            NodeKind::SectionGroup => out.extend(flatten(n)),
            _ => {}
        }
    }
    out
}

/// Whether the interface language reads right to left.
fn rtl_interface() -> bool {
    let code = crate::i18n::current_code();
    matches!(code.split('-').next().unwrap_or(""), "ar" | "fa" | "ur" | "he")
}

fn colour_of(ui: &Ui, c: Option<NodeColor>, t: &Tokens) -> egui::Color32 {
    match c {
        Some(c) => {
            let (r, g, b) = c.rgb(ui.visuals().dark_mode);
            egui::Color32::from_rgb(r, g, b)
        }
        None => t.accent,
    }
}

/// One tab, drawn: a rounded top in the section's colour. Returns the click.
/// `name` is what a screen reader hears; `group` adds a chevron after the words, the sign that
/// the tab opens a list rather than a section (issue #369).
fn tab(ui: &mut Ui, label: &str, name: &str, group: bool, colour: egui::Color32, current: bool, t: &Tokens) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let chevron = if group { 14.0 } else { 0.0 };
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + 26.0 + chevron;
    let height = if current { 28.0 } else { 25.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width.clamp(60.0, 240.0), 28.0), Sense::click());
    let body = egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - height), rect.max);
    let fill = if current { crate::theme::mix(t.editor, colour, 0.28) } else { crate::theme::mix(t.panel, colour, 0.16) };
    let radius = egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 };
    ui.painter().rect_filled(body, radius, fill);
    // The section's own colour along the top edge, as OneNote's tabs carry it.
    ui.painter().rect_filled(egui::Rect::from_min_size(body.min, vec2(body.width(), 3.0)), egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, colour);
    if resp.hovered() || resp.has_focus() {
        ui.painter().rect_stroke(body, radius, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    }
    let shown = if ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x > 214.0 {
        let mut s: String = label.chars().take(28).collect();
        s.push('\u{2026}');
        s
    } else {
        label.to_string()
    };
    let ink = if current { t.strong } else { t.text };
    let words = body.center() + vec2(-chevron / 2.0, 1.0);
    ui.painter().text(words, egui::Align2::CENTER_CENTER, shown, font, ink);
    if group {
        let c = egui::pos2(body.max.x - 13.0, body.center().y + 1.0);
        let stroke = egui::Stroke::new(1.3, ink);
        ui.painter().line_segment([c + vec2(-3.5, -1.5), c + vec2(0.0, 2.0)], stroke);
        ui.painter().line_segment([c + vec2(0.0, 2.0), c + vec2(3.5, -1.5)], stroke);
    }
    // The open section's tab is joined to the page under it by a bar in the accent colour,
    // a mark that does not rely on telling two tints apart.
    if current {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(egui::pos2(body.min.x + 4.0, body.max.y - 2.5), egui::pos2(body.max.x - 4.0, body.max.y)),
            1.0,
            t.accent,
        );
    }
    let name = name.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, name.clone()));
    resp.on_hover_text(label)
}

/// Draws the row of tabs. `here` is the open page's section. Returns the section chosen.
///
/// Issue #369: a button at the end of the row lists every section and group, so a tab that has
/// scrolled out of sight is still one click away, and the open section's tab is scrolled into
/// view whenever the open section changes.
pub fn show(ui: &mut Ui, tabs: &[Tab], here: Option<&Path>, t: &Tokens) -> Option<PathBuf> {
    let mut chosen = None;
    let rtl = rtl_interface();
    let layout = if rtl { egui::Layout::right_to_left(egui::Align::Max) } else { egui::Layout::left_to_right(egui::Align::Max) };
    // The overflow button is laid out first, from the far end, so the tabs never take its room.
    let outer = if rtl { egui::Layout::left_to_right(egui::Align::Max) } else { egui::Layout::right_to_left(egui::Align::Max) };
    let moved = {
        let id = ui.id().with("section-tabs-shown");
        let now = here.map(Path::to_path_buf);
        ui.data_mut(|d| {
            let before = d.get_temp::<Option<PathBuf>>(id);
            d.insert_temp(id, now.clone());
            before != Some(now)
        })
    };
    ui.with_layout(outer, |ui| {
        if let Some(picked) = overflow(ui, tabs, here, t) {
            chosen = Some(picked);
        }
        ui.add_space(4.0);
        egui::ScrollArea::horizontal().id_salt("section-tabs").auto_shrink([false, true]).show(ui, |ui| {
            ui.with_layout(layout, |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for t_ in tabs {
                    let colour = colour_of(ui, t_.colour, t);
                    match &t_.inside {
                        None => {
                            let current = here == Some(t_.path.as_path());
                            let resp = tab(ui, &t_.title, &t_.title, false, colour, current, t);
                            if current && moved {
                                resp.scroll_to_me(None);
                            }
                            if resp.clicked() {
                                chosen = Some(t_.path.clone());
                            }
                        }
                        Some(inside) => {
                            let current = here.is_some_and(|h| h.starts_with(&t_.path));
                            let name = crate::i18n::format_message_id("ui.section_group_tab", &[("name", &t_.title)]);
                            let resp = tab(ui, &t_.title, &name, true, colour, current, t);
                            if current && moved {
                                resp.scroll_to_me(None);
                            }
                            let popup = egui::Popup::menu(&resp).id(ui.id().with(("group-tab", &t_.path)));
                            popup.show(|ui| {
                                if inside.is_empty() {
                                    ui.label(crate::tl_id!("ui.this_group_has_no_sections"));
                                }
                                for s in inside {
                                    if ui.selectable_label(here == Some(s.path.as_path()), &s.title).clicked() {
                                        chosen = Some(s.path.clone());
                                        ui.close();
                                    }
                                }
                            });
                        }
                    }
                }
            });
        });
    });
    chosen
}

/// The button at the end of the tab row and its list of every section, groups included with
/// their sections under them. Returns the section chosen from the list.
fn overflow(ui: &mut Ui, tabs: &[Tab], here: Option<&Path>, t: &Tokens) -> Option<PathBuf> {
    let mut chosen = None;
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
    let label = crate::tl_id!("ui.all_sections");
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let open = egui::Popup::is_id_open(ui.ctx(), ui.id().with("all-sections"));
    let fill = if resp.hovered() || resp.has_focus() || open { t.row_hover } else { crate::theme::mix(t.panel, t.text, 0.06) };
    ui.painter().rect_filled(rect, 4.0, fill);
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.faint), egui::StrokeKind::Inside);
    let c = rect.center();
    let stroke = egui::Stroke::new(1.5, t.text);
    ui.painter().line_segment([c + vec2(-5.0, -2.0), c + vec2(0.0, 3.0)], stroke);
    ui.painter().line_segment([c + vec2(0.0, 3.0), c + vec2(5.0, -2.0)], stroke);
    let resp = resp.on_hover_text(label);
    egui::Popup::menu(&resp).id(ui.id().with("all-sections")).show(|ui| {
        ui.set_min_width(200.0);
        // A notebook with many sections scrolls inside the list rather than running past the
        // bottom of the window.
        let room = (ui.ctx().viewport_rect().bottom() - ui.cursor().top() - 16.0).max(120.0);
        egui::ScrollArea::vertical().id_salt("all-sections-list").max_height(room).show(ui, |ui| {
            for t_ in tabs {
                match &t_.inside {
                    None => {
                        if ui.selectable_label(here == Some(t_.path.as_path()), &t_.title).clicked() {
                            chosen = Some(t_.path.clone());
                            ui.close();
                        }
                    }
                    Some(inside) => {
                        ui.label(egui::RichText::new(&t_.title).small().strong().color(t.muted));
                        for s in inside {
                            ui.horizontal(|ui| {
                                ui.add_space(12.0);
                                if ui.selectable_label(here == Some(s.path.as_path()), &s.title).clicked() {
                                    chosen = Some(s.path.clone());
                                    ui.close();
                                }
                            });
                        }
                    }
                }
            }
        });
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::tree::NodeInfo;

    fn node(kind: NodeKind, title: &str, children: Vec<Node>) -> Node {
        Node {
            path: PathBuf::from(format!("/nb/{title}")),
            info: NodeInfo { kind, title: title.into(), color: None, format: None, id: title.into(), order: 0, read_only: false },
            children,
        }
    }

    #[test]
    fn sections_and_groups_become_tabs_in_order() {
        let nb = node(
            NodeKind::Notebook,
            "Book",
            vec![
                node(NodeKind::Section, "Work", vec![node(NodeKind::Page, "p", vec![])]),
                node(
                    NodeKind::SectionGroup,
                    "Archive",
                    vec![node(NodeKind::Section, "2024", vec![]), node(NodeKind::SectionGroup, "Old", vec![node(NodeKind::Section, "2019", vec![])])],
                ),
                node(NodeKind::Section, "Ideas", vec![]),
            ],
        );
        let tabs = tabs(&nb);
        let titles: Vec<&str> = tabs.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["Work", "Archive", "Ideas"]);
        let inside: Vec<&str> = tabs[1].inside.as_ref().expect("a group").iter().map(|t| t.title.as_str()).collect();
        assert_eq!(inside, ["2024", "2019"], "a group's sections, groups within it included");
        assert!(tabs[0].inside.is_none() && tabs[2].inside.is_none());
    }
}

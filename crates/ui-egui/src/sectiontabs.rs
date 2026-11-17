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
fn tab(ui: &mut Ui, label: &str, colour: egui::Color32, current: bool, t: &Tokens) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + 26.0;
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
    ui.painter().text(body.center() + vec2(0.0, 1.0), egui::Align2::CENTER_CENTER, shown, font, if current { t.strong } else { t.text });
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, true, current, name.clone()));
    resp.on_hover_text(label)
}

/// Draws the row of tabs. `here` is the open page's section. Returns the section chosen.
pub fn show(ui: &mut Ui, tabs: &[Tab], here: Option<&Path>, t: &Tokens) -> Option<PathBuf> {
    let mut chosen = None;
    let layout = if rtl_interface() { egui::Layout::right_to_left(egui::Align::Max) } else { egui::Layout::left_to_right(egui::Align::Max) };
    egui::ScrollArea::horizontal().id_salt("section-tabs").auto_shrink([false, true]).show(ui, |ui| {
        ui.with_layout(layout, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for t_ in tabs {
                let colour = colour_of(ui, t_.colour, t);
                match &t_.inside {
                    None => {
                        let current = here == Some(t_.path.as_path());
                        if tab(ui, &t_.title, colour, current, t).clicked() {
                            chosen = Some(t_.path.clone());
                        }
                    }
                    Some(inside) => {
                        let current = here.is_some_and(|h| h.starts_with(&t_.path));
                        let label = crate::i18n::format_message_id("ui.section_group_tab", &[("name", &t_.title)]);
                        let resp = tab(ui, &label, colour, current, t);
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
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::tree::NodeInfo;

    fn node(kind: NodeKind, title: &str, children: Vec<Node>) -> Node {
        Node {
            path: PathBuf::from(format!("/nb/{title}")),
            info: NodeInfo { kind, title: title.into(), color: None, format: None, id: title.into(), order: 0 },
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

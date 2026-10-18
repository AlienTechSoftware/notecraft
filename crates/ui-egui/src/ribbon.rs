//! The ribbon: a tab strip and, under it, the groups of controls for the chosen tab, the way
//! Office and OneNote arrange them. It follows the shape the sibling Craft apps use (a tab
//! strip, groups of buttons, a label under each group) without sharing their code.
//!
//! Every control here asks for an [`Act`]; the window carries it out. Nothing in this module
//! touches the notebook, so the whole ribbon is tested by asking what the buttons return.

use crate::icons::{self, Icon};
use crate::ribbon_icons::{self, Mark};
use crate::theme::Tokens;
use egui::{Align2, Rect, Sense, Ui, vec2};
use notecraft_model::{PageFormat, Tag};

/// The tabs, in order. "Source" only appears while the page source is shown, the way Office
/// shows a tab for what is selected.
pub const TABS: [&str; 3] = ["Home", "Insert", "View"];
pub const SOURCE_TAB: &str = "Source";

/// What a ribbon control asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// Put marks around the selection, or at the caret: bold, italic, strikethrough, code.
    Mark(&'static str),
    /// Make the block under the caret this kind: `p`, `h1`, `h2`, `h3`, `quote`, `bullet`,
    /// `number`, `todo`.
    Block(&'static str),
    /// Put a snippet in the page: a table, a link, an equation, a diagram, the date.
    Insert(&'static str),
    /// Add a file to the page (opens the same question a dropped file asks).
    AddFile,
    /// Add a picture to the page.
    AddPicture,
    ToggleSource,
    ToggleFullPage,
    FocusSearch,
    OpenThemes,
    /// Lines or squares behind the page, as on paper.
    RuleLines(RuleLines),
    /// Show or hide the page's title line.
    ToggleTitle,
    /// One tree, or two columns: the organisers and then the pages, as OneNote has it.
    TogglePanes,
    /// Keep this window above the others, or stop.
    ToggleAlwaysOnTop,
    /// Larger (1), smaller (-1) or back to 100% (0).
    Zoom(i8),
    /// Cut or copy what is selected.
    Clipboard(&'static str),
    /// More or less indent on a list item.
    Indent(bool),
    /// Which way the line the caret is in reads. `None` lets the words decide again.
    Direction(Option<notecraft_model::Dir>),
    /// Which way the whole page reads. `None` lets every line decide for itself.
    PageDirection(Option<notecraft_model::Dir>),
    /// Which way the selection, or what is typed next, reads: an isolated run inside the line.
    Embedding(notecraft_model::Dir),
    /// Open the window with the direction controls that are not everyday ones.
    OpenAdvancedDirection,
    /// Take the marks off the selection.
    ClearFormatting,
    /// Put a tag on the line the caret is in, or take it off again.
    Tag(Tag),
    /// Something OneNote has that NoteCraft does not: say so plainly.
    NotBuilt(&'static str),
    /// The user picked another tab.
    Tab(String),
}

/// What is drawn behind the page: nothing, ruled lines, or squares. OneNote calls this
/// Rule Lines, and keeps it per page; here it is how the pages are viewed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RuleLines {
    #[default]
    None,
    /// Ruled paper, in the four spacings OneNote offers.
    Narrow,
    College,
    Standard,
    Wide,
    /// Squared paper, in four sizes.
    GridSmall,
    GridMedium,
    GridLarge,
    GridHuge,
    /// NoteCraft's own two: the same paper with every fifth line drawn stronger, which is
    /// easier to count down and across than a plain grid.
    CraftLines,
    CraftGrid,
}

impl RuleLines {
    /// Every choice, in the order the menu shows them: the ruled ones, then the squares.
    pub const RULED: [RuleLines; 4] = [RuleLines::Narrow, RuleLines::College, RuleLines::Standard, RuleLines::Wide];
    pub const GRIDS: [RuleLines; 4] = [RuleLines::GridSmall, RuleLines::GridMedium, RuleLines::GridLarge, RuleLines::GridHuge];
    /// NoteCraft's own, offered under the other two.
    pub const CRAFT: [RuleLines; 2] = [RuleLines::CraftLines, RuleLines::CraftGrid];

    pub fn id(self) -> &'static str {
        match self {
            RuleLines::None => "none",
            RuleLines::Narrow => "narrow",
            RuleLines::College => "college",
            RuleLines::Standard => "standard",
            RuleLines::Wide => "wide",
            RuleLines::GridSmall => "grid-small",
            RuleLines::GridMedium => "grid-medium",
            RuleLines::GridLarge => "grid-large",
            RuleLines::GridHuge => "grid-huge",
            RuleLines::CraftLines => "lines",
            RuleLines::CraftGrid => "grid",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RuleLines::None => "None",
            RuleLines::Narrow => "Narrow ruled",
            RuleLines::College => "College ruled",
            RuleLines::Standard => "Standard ruled",
            RuleLines::Wide => "Wide ruled",
            RuleLines::GridSmall => "Small squares",
            RuleLines::GridMedium => "Medium squares",
            RuleLines::GridLarge => "Large squares",
            RuleLines::GridHuge => "Very large squares",
            RuleLines::CraftLines => "NoteCraft ruled",
            RuleLines::CraftGrid => "NoteCraft squares",
        }
    }

    /// The name under a thumbnail, where the shelf above it already says what kind of paper
    /// this is. The full `label` stays the button's name, so it is still what a screen
    /// reader says and what the ribbon's button shows.
    pub fn short(self) -> &'static str {
        match self {
            RuleLines::None => "None",
            RuleLines::Narrow => "Narrow",
            RuleLines::College => "College",
            RuleLines::Standard => "Standard",
            RuleLines::Wide => "Wide",
            RuleLines::GridSmall => "Small",
            RuleLines::GridMedium => "Medium",
            RuleLines::GridLarge => "Large",
            RuleLines::GridHuge => "Very large",
            RuleLines::CraftLines => "Ruled",
            RuleLines::CraftGrid => "Squares",
        }
    }

    /// How far apart the lines are, in points. The ruled spacings follow the paper the
    /// names come from; the squares are the same four steps without the wide one.
    pub fn step(self) -> f32 {
        match self {
            RuleLines::None => 0.0,
            RuleLines::Narrow => 16.0,
            RuleLines::College => 20.0,
            RuleLines::Standard => 24.0,
            RuleLines::Wide => 32.0,
            RuleLines::GridSmall => 12.0,
            RuleLines::GridMedium => 18.0,
            RuleLines::GridLarge => 24.0,
            RuleLines::GridHuge => 32.0,
            RuleLines::CraftLines | RuleLines::CraftGrid => 24.0,
        }
    }

    /// True for the squared papers, which draw the upright lines as well.
    pub fn squares(self) -> bool {
        matches!(self, RuleLines::GridSmall | RuleLines::GridMedium | RuleLines::GridLarge | RuleLines::GridHuge | RuleLines::CraftGrid)
    }

    /// True where every fifth line is drawn stronger: NoteCraft's own two papers.
    pub fn fives(self) -> bool {
        matches!(self, RuleLines::CraftLines | RuleLines::CraftGrid)
    }

    pub fn on(self) -> bool {
        self != RuleLines::None
    }

    pub fn parse(text: &str) -> Option<RuleLines> {
        let want = text.trim();
        if want == "none" {
            return Some(RuleLines::None);
        }
        // "lines" and "grid" are NoteCraft's own two, and the ids the first version wrote.
        RuleLines::RULED.into_iter().chain(RuleLines::GRIDS).chain(RuleLines::CRAFT).find(|r| r.id() == want)
    }
}

/// Draws a paper's lines inside `area`. The page draws itself at full size; the gallery in
/// the ribbon draws the same papers at `scale`, so a thumbnail is the paper itself made
/// small and cannot drift from what the page will show.
pub fn paint_paper(painter: &egui::Painter, area: egui::Rect, rules: RuleLines, scale: f32, line: egui::Color32, strong: egui::Color32) {
    if !rules.on() {
        return;
    }
    let step = (rules.step() * scale).max(4.0);
    let fives = rules.fives();
    let colour = |n: usize| if fives && n.is_multiple_of(5) { strong } else { line };
    let mut y = area.min.y + step;
    let mut n = 0;
    while y < area.max.y {
        n += 1;
        painter.line_segment([egui::pos2(area.min.x, y), egui::pos2(area.max.x, y)], egui::Stroke::new(1.0, colour(n)));
        y += step;
    }
    if rules.squares() {
        let mut x = area.min.x + step;
        let mut n = 0;
        while x < area.max.x {
            n += 1;
            painter.line_segment([egui::pos2(x, area.min.y), egui::pos2(x, area.max.y)], egui::Stroke::new(1.0, colour(n)));
            x += step;
        }
    }
}

/// What the View tab shows about the window.
#[derive(Clone, Copy, Debug, Default)]
pub struct View {
    /// True when the side panel is two columns.
    pub split_panes: bool,
    /// True when the window is kept above the others.
    pub always_on_top: bool,
    pub full_page: bool,
    pub rules: RuleLines,
    pub hide_title: bool,
    pub zoom_percent: u32,
}

/// How tall the ribbon is: the tab strip, then the groups.
pub const TAB_STRIP_H: f32 = 26.0;
pub const GROUP_H: f32 = 78.0;
const BIG: f32 = 46.0;
const SMALL_W: f32 = 30.0;
const SMALL_H: f32 = 18.0;

/// Draws the ribbon. `tab` is the chosen tab, `source` whether the page source is shown.
/// Returns every act the user asked for this frame (usually none or one).
pub fn show(ui: &mut Ui, tab: &str, source: bool, view: &View, format: Option<PageFormat>, t: &Tokens) -> Vec<Act> {
    let mut acts = Vec::new();
    tab_strip(ui, tab, source, t, &mut acts);
    ui.add_space(2.0);
    let chosen = resolve_tab(tab, source);
    ui.horizontal_top(|ui| {
        ui.add_space(4.0);
        match chosen {
            "Insert" => insert_tab(ui, t, &mut acts),
            "View" => view_tab(ui, source, view, t, &mut acts),
            SOURCE_TAB => source_tab(ui, format, t, &mut acts),
            _ => home_tab(ui, t, &mut acts),
        }
    });
    acts
}

/// The tab that is really shown: the Source tab disappears with the source pane.
pub fn resolve_tab(tab: &str, source: bool) -> &str {
    if tab == SOURCE_TAB && !source {
        return "Home";
    }
    if TABS.contains(&tab) || (tab == SOURCE_TAB && source) { tab } else { "Home" }
}

/// Every tab that can be chosen now.
pub fn tabs(source: bool) -> Vec<&'static str> {
    let mut v = TABS.to_vec();
    if source {
        v.push(SOURCE_TAB);
    }
    v
}

fn tab_strip(ui: &mut Ui, tab: &str, source: bool, t: &Tokens, acts: &mut Vec<Act>) {
    let chosen = resolve_tab(tab, source);
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        for name in tabs(source) {
            let selected = name == chosen;
            let galley = ui.painter().layout_no_wrap(name.to_string(), egui::FontId::proportional(13.0), t.text);
            let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, TAB_STRIP_H), Sense::click());
            let painter = ui.painter_at(rect);
            if selected {
                painter.rect_filled(rect.shrink2(vec2(0.0, 2.0)), 4.0, t.row_selected);
                painter.line_segment(
                    [egui::pos2(rect.min.x + 6.0, rect.max.y - 2.0), egui::pos2(rect.max.x - 6.0, rect.max.y - 2.0)],
                    egui::Stroke::new(2.0, t.accent),
                );
            } else if resp.hovered() {
                painter.rect_filled(rect.shrink2(vec2(0.0, 2.0)), 4.0, t.row_hover);
            }
            painter.text(rect.center(), Align2::CENTER_CENTER, name, egui::FontId::proportional(13.0), if selected { t.strong } else { t.text });
            let enabled = ui.is_enabled();
            let label = name.to_string();
            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label.clone()));
            if resp.clicked() {
                acts.push(Act::Tab(name.to_string()));
            }
        }
    });
}

/// One group of the ribbon: its controls, a line on the right, and its name underneath.
fn group(ui: &mut Ui, title: &str, t: &Tokens, add: impl FnOnce(&mut Ui)) {
    // The controls first; the group's name and its divider are painted around whatever they
    // took, so a group is exactly as wide as it needs to be, as in Office.
    let inner = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            ui.horizontal_top(|ui| {
                ui.add_space(5.0);
                add(ui);
                ui.add_space(5.0);
            });
        })
        .response
        .rect;
    let painter = ui.painter();
    painter.text(egui::pos2(inner.center().x, inner.min.y + GROUP_H - 9.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(10.5), t.muted);
    // A crisp divider between groups, on a whole pixel so it does not blur, and dark enough
    // to read on a light or a dark surface, the way Office separates its groups.
    let x = (inner.max.x + 6.0).round() + 0.5;
    painter.line_segment(
        [egui::pos2(x, inner.min.y + 2.0), egui::pos2(x, inner.min.y + GROUP_H - 6.0)],
        egui::Stroke::new(1.0, crate::theme::mix(t.panel, t.text, 0.22)),
    );
    ui.add_space(13.0);
}

/// A big button: an icon with its name under it.
fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(BIG.max(label.len() as f32 * 6.5 + 10.0), GROUP_H - 18.0), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    icons::paint(&painter, Rect::from_center_size(egui::pos2(rect.center().x, rect.min.y + 18.0), vec2(22.0, 22.0)), icon, t.accent);
    painter.text(egui::pos2(rect.center().x, rect.min.y + 40.0), Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), t.text);
    let enabled = ui.is_enabled();
    // The name a test or a screen reader hears: one line, no matter how the label wraps.
    let name = label.replace('\n', " ");
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(tip)
}

/// A small button: an icon, or a letter for the marks that have one.
fn small(ui: &mut Ui, icon: Option<Icon>, letter: &str, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    // Wide enough for whatever it says: a letter, or a word like "Copy".
    let width = if icon.is_some() || letter.is_empty() {
        SMALL_W
    } else {
        ui.painter().layout_no_wrap(letter.to_string(), egui::FontId::proportional(12.5), t.text).size().x + 14.0
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(width.max(SMALL_W), SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if on || resp.hovered() {
        painter.rect_filled(rect, 4.0, if on { t.row_selected } else { t.row_hover });
    }
    match icon {
        Some(i) => icons::paint(&painter, Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), i, t.accent),
        None => {
            painter.text(rect.center(), Align2::CENTER_CENTER, letter, egui::FontId::proportional(12.5), t.strong);
        }
    }
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// Rows of small buttons inside a group.
fn rows(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
        add(ui);
    });
}

/// The styles OneNote lists in its Styles gallery, and what each one is here.
pub const STYLES: [(&str, &str); 11] = [
    ("Heading 1", "h1"),
    ("Heading 2", "h2"),
    ("Heading 3", "h3"),
    ("Heading 4", "h4"),
    ("Heading 5", "h5"),
    ("Heading 6", "h6"),
    ("Page Title", "title"),
    ("Citation", "citation"),
    ("Quote", "quote"),
    ("Code", "codeblock"),
    ("Normal", "p"),
];

/// OneNote's tag list. Every one of them works: To Do is a task item, the rest are a
/// marker word at the start of the line (`notecraft_model::Tag`).
pub fn tag_label(tag: Tag) -> String {
    match tag.shortcut() {
        Some(keys) => format!("{} ({keys})", tag.label()),
        None => tag.label().to_string(),
    }
}

fn home_tab(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Clipboard", t, |ui| {
        if big_mark(ui, Mark::Paste, "Paste", "Ctrl+V pastes into the page", t).clicked() {
            acts.push(Act::NotBuilt("Paste from the ribbon"));
        }
        rows(ui, |ui| {
            if mark_button(ui, Mark::Cut, "Cut", "Cut the selection", false, t).clicked() {
                acts.push(Act::Clipboard("cut"));
            }
            if mark_button(ui, Mark::Copy, "Copy", "Copy the selection", false, t).clicked() {
                acts.push(Act::Clipboard("copy"));
            }
            if mark_button(ui, Mark::Painter, "Format Painter", "Not built", false, t).clicked() {
                acts.push(Act::NotBuilt("Format Painter"));
            }
        });
    });
    group(ui, "Basic Text", t, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                if mark_button(ui, Mark::Bold, "Bold", "Bold (Ctrl+B)", false, t).clicked() {
                    acts.push(Act::Mark("bold"));
                }
                if mark_button(ui, Mark::Italic, "Italic", "Italic (Ctrl+I)", false, t).clicked() {
                    acts.push(Act::Mark("italic"));
                }
                if mark_button(ui, Mark::Strike, "Strikethrough", "Strike through", false, t).clicked() {
                    acts.push(Act::Mark("strike"));
                }
                if small(ui, Some(Icon::Code), "", "Code", "Code, in one line", false, t).clicked() {
                    acts.push(Act::Mark("code"));
                }
                if small(ui, Some(Icon::Link), "", "Link", "A link", false, t).clicked() {
                    acts.push(Act::Insert("link"));
                }
            });
            ui.horizontal(|ui| {
                if small(ui, Some(Icon::ListBullet), "", "Bullets", "A bullet list", false, t).clicked() {
                    acts.push(Act::Block("bullet"));
                }
                if small(ui, Some(Icon::ListNumber), "", "Numbering", "A numbered list", false, t).clicked() {
                    acts.push(Act::Block("number"));
                }
                if mark_button(ui, Mark::IndentLess, "Decrease indent", "Less indent (Shift+Tab)", false, t).clicked() {
                    acts.push(Act::Indent(false));
                }
                if mark_button(ui, Mark::IndentMore, "Increase indent", "More indent (Tab)", false, t).clicked() {
                    acts.push(Act::Indent(true));
                }
                align_menu(ui, t, acts);
                if small(ui, Some(Icon::Quote), "", "Quote", "Quote this paragraph", false, t).clicked() {
                    acts.push(Act::Block("quote"));
                }
                if mark_button(ui, Mark::ClearFormat, "Clear formatting", "Take the marks off", false, t).clicked() {
                    acts.push(Act::ClearFormatting);
                }
            });
        });
    });
    // Page direction is NoteCraft's own: OneNote has no such thing, and a notebook written
    // in Arabic wants one. The paragraph's direction lives where OneNote keeps it, in the
    // alignment menu above; the rarer run direction is behind the launcher.
    group(ui, "Direction", t, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Page").size(10.5).color(t.muted));
                if small(ui, Some(Icon::DirLtr), "", "Page left to right", "The whole page reads left to right", false, t).clicked() {
                    acts.push(Act::PageDirection(Some(notecraft_model::Dir::Ltr)));
                }
                if small(ui, Some(Icon::DirRtl), "", "Page right to left", "The whole page reads right to left (Arabic, Hebrew)", false, t).clicked()
                {
                    acts.push(Act::PageDirection(Some(notecraft_model::Dir::Rtl)));
                }
                if small(ui, None, "Auto", "Page direction from the words", "Every paragraph decides for itself", false, t).clicked() {
                    acts.push(Act::PageDirection(None));
                }
            });
            ui.horizontal(|ui| {
                if small(ui, None, "More...", "Advanced direction", "Direction for part of a line, and the options that are not built yet", false, t)
                    .clicked()
                {
                    acts.push(Act::OpenAdvancedDirection);
                }
            });
        });
    });
    group(ui, "Styles", t, |ui| {
        gallery(ui, "styles", t, acts, |ui, acts| {
            // The box shows the first two, as OneNote's does.
            for (label, kind) in STYLES.iter().take(2) {
                if style_row(ui, label, kind, t).clicked() {
                    acts.push(Act::Block(leak(kind)));
                }
            }
        });
    });
    group(ui, "Tags", t, |ui| {
        gallery(ui, "tags", t, acts, |ui, acts| {
            for tag in [Tag::ToDo, Tag::Important] {
                if tag_row(ui, tag, t).clicked() {
                    acts.push(Act::Tag(tag));
                }
            }
        });
        if big(ui, Icon::Todo, "To Do\nTag", "A to-do box on this line (Ctrl+1)", t).clicked() {
            acts.push(Act::Block("todo"));
        }
        if big(ui, Icon::Search, "Find\nTags", "Not built", t).clicked() {
            acts.push(Act::NotBuilt("Find Tags"));
        }
    });
    group(ui, "Email", t, |ui| {
        if big(ui, Icon::Attach, "Send\nPage", "Not built", t).clicked() {
            acts.push(Act::NotBuilt("Send Page"));
        }
    });
    group(ui, "Meetings", t, |ui| {
        if big(ui, Icon::Page, "Meeting\nDetails", "Not built", t).clicked() {
            acts.push(Act::NotBuilt("Meeting Details"));
        }
    });
}

/// A static name for a style, so an act can carry it.
fn leak(kind: &str) -> &'static str {
    STYLES.iter().find(|(_, k)| *k == kind).map(|(_, k)| *k).unwrap_or("p")
}

/// A gallery like OneNote's: a bordered box with the first entries, and an arrow that opens
/// the whole list beside it.
fn gallery(ui: &mut Ui, which: &'static str, t: &Tokens, acts: &mut Vec<Act>, rows_in_box: impl FnOnce(&mut Ui, &mut Vec<Act>)) {
    let height = GROUP_H - 20.0;
    let box_rect = egui::Frame::NONE
        .stroke(egui::Stroke::new(1.0, t.faint))
        .corner_radius(3.0)
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.set_min_size(vec2(130.0, height));
            ui.set_max_width(190.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                rows_in_box(ui, acts);
            });
        })
        .response
        .rect;
    // The arrow that opens the whole gallery.
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, height), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    let c = egui::pos2(rect.center().x, rect.max.y - 8.0);
    painter.line_segment([egui::pos2(c.x - 4.0, c.y - 2.0), c], egui::Stroke::new(1.4, t.text));
    painter.line_segment([c, egui::pos2(c.x + 4.0, c.y - 2.0)], egui::Stroke::new(1.4, t.text));
    let enabled = ui.is_enabled();
    let name = if which == "styles" { "More styles" } else { "More tags" };
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
    let resp = resp.on_hover_text(name);
    // The list opens over the box it belongs to, as OneNote's galleries do: same left edge,
    // same top, so the two rows in the box are the first two rows of the list.
    let open_id = egui::Id::new(("ribbon-gallery", which));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(box_rect.min)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(box_rect.width().max(210.0));
                    ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
                    if which == "styles" {
                        for (label, kind) in STYLES {
                            if style_row(ui, label, kind, t).clicked() {
                                acts.push(Act::Block(leak(kind)));
                                picked = true;
                            }
                        }
                        ui.separator();
                        if ui.button("Clear formatting").clicked() {
                            acts.push(Act::ClearFormatting);
                            picked = true;
                        }
                    } else {
                        ui.label(egui::RichText::new("Tags").strong());
                        for tag in Tag::ALL {
                            if tag_row(ui, tag, t).clicked() {
                                acts.push(Act::Tag(tag));
                                picked = true;
                            }
                        }
                        ui.separator();
                        ui.add_enabled(false, egui::Label::new(egui::RichText::new("Customize Tags... (not built)").color(t.muted)));
                    }
                });
            })
            .response;
        // A click anywhere else closes it, the way a menu does.
        let clicked_away =
            ui.input(|i| i.pointer.any_click()) && !area.rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) && !resp.clicked();
        if picked || clicked_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// OneNote's alignment button: a menu of alignment, then the paragraph's direction under a
/// separator, in that order. Alignment itself is not built and says so.
fn align_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = small(ui, Some(Icon::DirLtr), "", "Alignment and direction", "Alignment, and which way this paragraph reads", false, t);
    let open_id = egui::Id::new("ribbon-align-menu");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
                    for (label, what) in [("Align Left", "Align left"), ("Center", "Centring"), ("Align Right", "Align right")] {
                        if ui.button(label).clicked() {
                            acts.push(Act::NotBuilt(what));
                            picked = true;
                        }
                    }
                    ui.separator();
                    if ui.button("Left-to-Right").clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Ltr)));
                        picked = true;
                    }
                    if ui.button("Right-to-Left").clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Rtl)));
                        picked = true;
                    }
                    if ui.button("Direction from the words").clicked() {
                        acts.push(Act::Direction(None));
                        picked = true;
                    }
                });
            })
            .response;
        let clicked_away =
            ui.input(|i| i.pointer.any_click()) && !area.rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) && !resp.clicked();
        if picked || clicked_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// One line of the Styles gallery, drawn the way the style looks.
fn style_row(ui: &mut Ui, label: &str, kind: &str, t: &Tokens) -> egui::Response {
    let (size, color, italics) = match kind {
        "h1" | "title" => (17.0, t.accent, false),
        "h2" => (15.0, t.accent, false),
        "h3" | "h4" => (13.5, t.muted, kind == "h4"),
        "h5" | "h6" => (12.5, t.accent, kind == "h6"),
        "quote" | "citation" => (12.5, t.muted, true),
        "codeblock" => (12.5, t.text, false),
        _ => (13.0, t.text, false),
    };
    let mut text = egui::RichText::new(label).size(size).color(color);
    if italics {
        text = text.italics();
    }
    if kind == "codeblock" {
        text = text.monospace();
    }
    ui.add(egui::Button::new(text).frame(false).min_size(vec2(130.0, 22.0)))
}

/// One line of the Tags gallery: OneNote's own mark and the name, with its shortcut.
fn tag_row(ui: &mut Ui, tag: Tag, t: &Tokens) -> egui::Response {
    // A fixed width: the gallery box is as wide as its rows, not as wide as the ribbon.
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().clamp(150.0, 230.0), 24.0), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    let mark = mark_for(tag);
    ribbon_icons::paint(
        &painter,
        Rect::from_min_size(egui::pos2(rect.min.x + 2.0, rect.center().y - 9.0), vec2(18.0, 18.0)),
        mark,
        mark.color(t.text, t.accent),
    );
    painter.text(egui::pos2(rect.min.x + 26.0, rect.center().y), Align2::LEFT_CENTER, tag_label(tag), egui::FontId::proportional(12.5), t.text);
    let enabled = ui.is_enabled();
    let name = tag_label(tag);
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp
}

/// OneNote's mark for a tag.
pub fn mark_for(tag: Tag) -> Mark {
    match tag {
        Tag::ToDo => Mark::ToDo,
        Tag::Important => Mark::Important,
        Tag::Question => Mark::Question,
        Tag::Remember => Mark::Remember,
        Tag::Definition => Mark::Definition,
        Tag::Highlight => Mark::Highlighter,
        Tag::Contact => Mark::Contact,
        Tag::Address => Mark::Address,
        Tag::Phone => Mark::Phone,
        Tag::Website => Mark::Website,
        Tag::Idea => Mark::Idea,
        Tag::Critical => Mark::Critical,
    }
}

/// A small button that draws one of the ribbon's own marks.
fn mark_button(ui: &mut Ui, mark: Mark, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(SMALL_W, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if on || resp.hovered() {
        painter.rect_filled(rect, 4.0, if on { t.row_selected } else { t.row_hover });
    }
    ribbon_icons::paint(&painter, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), mark, mark.color(t.text, t.accent));
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

fn insert_tab(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Tables", t, |ui| {
        if big(ui, Icon::Table, "Table", "A table", t).clicked() {
            acts.push(Act::Insert("table"));
        }
    });
    group(ui, "Files", t, |ui| {
        if big(ui, Icon::Picture, "Picture", "A picture from a file", t).clicked() {
            acts.push(Act::AddPicture);
        }
        if big(ui, Icon::Attach, "File", "A file: copied in, or linked where it is", t).clicked() {
            acts.push(Act::AddFile);
        }
    });
    group(ui, "Links", t, |ui| {
        if big(ui, Icon::Link, "Link", "A link to a page or an address", t).clicked() {
            acts.push(Act::Insert("link"));
        }
    });
    group(ui, "Symbols", t, |ui| {
        if big(ui, Icon::Equation, "Equation", "An equation, written in TeX", t).clicked() {
            acts.push(Act::Insert("equation"));
        }
        if big(ui, Icon::Diagram, "Diagram", "A Mermaid diagram, kept as its source", t).clicked() {
            acts.push(Act::Insert("diagram"));
        }
    });
    group(ui, "Time", t, |ui| {
        if big(ui, Icon::Page, "Date", "Today's date and time", t).clicked() {
            acts.push(Act::Insert("date"));
        }
    });
}

/// One paper in the gallery: the paper itself, drawn small, with its name under it. Picking
/// a paper is picking a look, so the look is what is shown; the name is there to say it.
fn paper_swatch(ui: &mut Ui, r: RuleLines, here: bool, t: &Tokens) -> egui::Response {
    // Half size. At that scale the closest two papers still differ by two lines, and
    // NoteCraft's own show their stronger fifth line, which is the whole point of them.
    const SCALE: f32 = 0.5;
    const PAPER: egui::Vec2 = egui::vec2(78.0, 66.0);
    const CAPTION: f32 = 15.0;
    let (rect, resp) = ui.allocate_exact_size(PAPER + vec2(0.0, CAPTION), egui::Sense::click());
    let enabled = ui.is_enabled();
    let label = r.label();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, label));
    let paper = egui::Rect::from_min_size(rect.min, PAPER);
    let painter = ui.painter().with_clip_rect(paper);
    painter.rect_filled(paper, 2.0, t.editor);
    paint_paper(&painter, paper, r, SCALE, t.rule_line, t.rule_line_strong);
    // A page with no lines needs something to look at, so it says so in the middle.
    if !r.on() {
        painter.text(paper.center(), egui::Align2::CENTER_CENTER, "Plain", egui::FontId::proportional(12.0), t.muted);
    }
    let (edge, width) = if here {
        (t.accent, 2.0)
    } else if resp.hovered() {
        (t.muted, 1.0)
    } else {
        (t.faint, 1.0)
    };
    ui.painter().rect_stroke(paper, 2.0, egui::Stroke::new(width, edge), egui::StrokeKind::Inside);
    // The name is kept inside the thumbnail's own width, so a long one cannot run into the
    // paper beside it.
    let colour = if here { t.accent } else { t.text };
    let name = egui::pos2(paper.center().x, paper.max.y + 1.0);
    ui.painter().with_clip_rect(rect).text(name, egui::Align2::CENTER_TOP, r.short(), egui::FontId::proportional(11.0), colour);
    resp.on_hover_text(label)
}

/// OneNote's Rule Lines button: the paper now, and a gallery of every paper under it.
fn rule_lines_menu(ui: &mut Ui, view: &View, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big(ui, Icon::Table, view.rules.label(), "Lines or squares behind the page, as on paper", t);
    let open_id = egui::Id::new("ribbon-rule-lines");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                    let shelf = |ui: &mut Ui, title: &str, papers: &[RuleLines], acts: &mut Vec<Act>, picked: &mut bool| {
                        ui.label(egui::RichText::new(title).strong());
                        ui.horizontal(|ui| {
                            for r in papers {
                                if paper_swatch(ui, *r, view.rules == *r, t).clicked() {
                                    acts.push(Act::RuleLines(*r));
                                    *picked = true;
                                }
                            }
                        });
                    };
                    shelf(ui, "No lines", &[RuleLines::None], acts, &mut picked);
                    shelf(ui, "Rule lines", &RuleLines::RULED, acts, &mut picked);
                    shelf(ui, "Grid lines", &RuleLines::GRIDS, acts, &mut picked);
                    shelf(ui, "NoteCraft's own", &RuleLines::CRAFT, acts, &mut picked);
                    ui.label(egui::RichText::new("Every fifth line stronger, to count by.").small().color(t.muted));
                    ui.separator();
                    ui.add_enabled(false, egui::Label::new(egui::RichText::new("Rule line colour (not built)").color(t.muted)));
                    ui.add_enabled(false, egui::Label::new(egui::RichText::new("Always create pages with rule lines (not built)").color(t.muted)));
                });
            })
            .response;
        let clicked_away =
            ui.input(|i| i.pointer.any_click()) && !area.rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) && !resp.clicked();
        if picked || clicked_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

fn view_tab(ui: &mut Ui, source: bool, view: &View, t: &Tokens, acts: &mut Vec<Act>) {
    // The groups OneNote's View tab has, in its order: Views, Layout, Tools, Page Setup,
    // Zoom, Window. What NoteCraft does not have says so rather than being left out.
    group(ui, "Views", t, |ui| {
        if big(
            ui,
            Icon::Page,
            "Normal
view",
            "The page beside the notebooks",
            t,
        )
        .clicked()
            && view.full_page
        {
            acts.push(Act::ToggleFullPage);
        }
        if big(ui, if view.full_page { Icon::ExitFullPage } else { Icon::FullPage }, "Full page", "Give the page the whole window", t).clicked() {
            acts.push(Act::ToggleFullPage);
        }
        rows(ui, |ui| {
            not_built(ui, "Dock to desktop", t);
        });
    });
    group(ui, "Layout", t, |ui| {
        rows(ui, |ui| {
            // What OneNote calls the navigation pane: notebooks and sections in one
            // column, the pages of the chosen section in the next.
            if small(ui, None, "Split", "Separate pages", "Notebooks and sections in one column, pages in the next", view.split_panes, t).clicked() {
                acts.push(Act::TogglePanes);
            }
            not_built(ui, "Tabs layout", t);
        });
    });
    group(ui, "Tools", t, |ui| {
        if big(ui, Icon::Search, "Search", "Search all pages", t).clicked() {
            acts.push(Act::FocusSearch);
        }
        rows(ui, |ui| {
            not_built(ui, "Immersive reader", t);
            not_built(ui, "Live captions", t);
        });
    });
    group(ui, "Page Setup", t, |ui| {
        rule_lines_menu(ui, view, t, acts);
        rows(ui, |ui| {
            if small(ui, None, "T", "Hide page title", "Hide the page title", view.hide_title, t).clicked() {
                acts.push(Act::ToggleTitle);
            }
            not_built(ui, "Page colour", t);
            not_built(ui, "Paper size", t);
        });
    });
    group(ui, "Zoom", t, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                if small(ui, None, "-", "Zoom out", "Smaller", false, t).clicked() {
                    acts.push(Act::Zoom(-1));
                }
                if small(ui, None, "+", "Zoom in", "Larger", false, t).clicked() {
                    acts.push(Act::Zoom(1));
                }
            });
            if small(ui, None, &format!("{}%", view.zoom_percent), "Zoom to 100%", "Back to 100%", false, t).clicked() {
                acts.push(Act::Zoom(0));
            }
            not_built(ui, "Page width", t);
        });
    });
    group(ui, "Window", t, |ui| {
        rows(ui, |ui| {
            // Always on top is the window manager's business, and eframe can ask for it.
            if small(ui, None, "Top", "Always on top", "Keep this window above the others", view.always_on_top, t).clicked() {
                acts.push(Act::ToggleAlwaysOnTop);
            }
            not_built(ui, "New window", t);
            not_built(ui, "New quick note", t);
        });
    });
    group(ui, "Appearance", t, |ui| {
        if big(ui, Icon::Sun, "Theme", "Light or dark, and the accent colour", t).clicked() {
            acts.push(Act::OpenThemes);
        }
    });
    group(ui, "Source", t, |ui| {
        if big(ui, Icon::Code, if source { "Hide" } else { "Show" }, "The page source, beside the page", t).clicked() {
            acts.push(Act::ToggleSource);
        }
    });
    group(ui, "Window", t, |ui| {
        rows(ui, |ui| {
            not_built(ui, "New window", t);
            not_built(ui, "Always on top", t);
            not_built(ui, "Dock to desktop", t);
        });
    });
}

/// A control that is listed so nobody goes looking for it, and says it is not built.
fn not_built(ui: &mut Ui, label: &str, t: &Tokens) {
    let text = egui::RichText::new(format!("{label} (not built)")).size(10.5).color(t.muted);
    ui.add_enabled(false, egui::Label::new(text));
}

fn source_tab(ui: &mut Ui, format: Option<PageFormat>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Source", t, |ui| {
        if big(ui, Icon::Code, "Hide", "Close the source pane", t).clicked() {
            acts.push(Act::ToggleSource);
        }
    });
    group(ui, "This page", t, |ui| {
        let name = format.map(PageFormat::label).unwrap_or("No page");
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(name).size(12.0).color(t.text));
            ui.label(egui::RichText::new("The format is set when the page is created").size(10.0).color(t.muted));
        });
    });
}

/// A big button that draws one of the ribbon's own marks.
fn big_mark(ui: &mut Ui, mark: Mark, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(BIG, GROUP_H - 18.0), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    ribbon_icons::paint(
        &painter,
        Rect::from_center_size(egui::pos2(rect.center().x, rect.min.y + 18.0), vec2(24.0, 24.0)),
        mark,
        mark.color(t.text, t.accent),
    );
    painter.text(egui::pos2(rect.center().x, rect.min.y + 40.0), Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(tip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    fn draw(tab: &str, source: bool) -> Vec<Act> {
        let t = Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut acts = Vec::new();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            acts = show(ui, tab, source, &View { zoom_percent: 100, ..View::default() }, Some(PageFormat::Html), &t);
        });
        out.textures_delta.clear();
        acts
    }

    #[test]
    fn the_tabs_are_office_like_and_source_comes_and_goes() {
        assert_eq!(tabs(false), vec!["Home", "Insert", "View"]);
        assert_eq!(tabs(true), vec!["Home", "Insert", "View", "Source"]);
        assert_eq!(resolve_tab("Insert", false), "Insert");
        assert_eq!(resolve_tab("Source", true), "Source");
        assert_eq!(resolve_tab("Source", false), "Home", "the Source tab goes when the source pane does");
        assert_eq!(resolve_tab("Nonsense", false), "Home");
        assert_eq!(resolve_tab("", true), "Home");
    }

    #[test]
    fn every_paper_has_a_name_an_id_and_a_spacing() {
        use std::collections::HashSet;
        let all: Vec<RuleLines> = std::iter::once(RuleLines::None).chain(RuleLines::RULED).chain(RuleLines::GRIDS).chain(RuleLines::CRAFT).collect();
        assert_eq!(all.len(), 11, "none, four ruled, four grids, two of ours");
        let mut ids = HashSet::new();
        for r in &all {
            assert!(ids.insert(r.id()), "two papers share an id: {}", r.id());
            assert!(!r.label().is_empty());
            assert_eq!(RuleLines::parse(r.id()), Some(*r), "{} reads back", r.id());
            assert_eq!(r.on(), *r != RuleLines::None);
            assert!(r.step() > 8.0 || *r == RuleLines::None, "{} has a spacing", r.id());
        }
        // Only the squares draw upright lines, and only ours mark every fifth.
        assert!(RuleLines::GRIDS.iter().all(|r| r.squares()) && RuleLines::RULED.iter().all(|r| !r.squares()));
        assert!(RuleLines::CraftGrid.squares() && !RuleLines::CraftLines.squares());
        assert!(RuleLines::CRAFT.iter().all(|r| r.fives()));
        assert!(RuleLines::RULED.iter().chain(RuleLines::GRIDS.iter()).all(|r| !r.fives()), "OneNote's papers are evenly weighted");
        // The ids the first version wrote are NoteCraft's own two, so a settings file
        // written before the other papers existed keeps the paper it had.
        assert_eq!(RuleLines::parse("lines"), Some(RuleLines::CraftLines));
        assert_eq!(RuleLines::parse("grid"), Some(RuleLines::CraftGrid));
        assert_eq!(RuleLines::parse("nonsense"), None);
    }

    #[test]
    fn every_tab_draws_without_panicking_and_asks_for_nothing_by_itself() {
        for tab in ["Home", "Insert", "View", "Source", "junk"] {
            for source in [false, true] {
                assert_eq!(draw(tab, source), Vec::new(), "{tab} asked for something nobody clicked");
            }
        }
    }

    /// The acts are plain values, so the window's side of the ribbon is testable on its own.
    #[test]
    fn a_page_starts_with_no_lines_behind_it() {
        assert_eq!(RuleLines::default(), RuleLines::None);
        assert_eq!(RuleLines::parse("squares"), None);
        assert!(!RuleLines::None.on() && !RuleLines::None.squares() && !RuleLines::None.fives());
    }

    #[test]
    fn the_acts_name_what_they_do() {
        assert_eq!(Act::Mark("bold"), Act::Mark("bold"));
        assert_ne!(Act::Mark("bold"), Act::Mark("italic"));
        assert_ne!(Act::Block("h1"), Act::Insert("h1"));
        assert_eq!(Act::Tab("Insert".into()), Act::Tab("Insert".to_string()));
    }
}

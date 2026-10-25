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
use notecraft_ink::rack::{self, Nib};
use notecraft_ink::{Rgb, Tool};
use notecraft_model::{PageFormat, Tag};

/// The tabs, in order. "Source" only appears while the page source is shown, the way Office
/// shows a tab for what is selected.
pub const TABS: [&str; 5] = ["Home", "Insert", "Draw", "History", "View"];
pub const SOURCE_TAB: &str = "Source";
/// Shown only while the caret is in a table, the way Office shows a tab for the thing in
/// hand. It goes when the caret leaves, and the ribbon falls back to Home.
pub const TABLE_TAB: &str = "Table";

/// What a ribbon control asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    /// The File tab: open the File screen.
    File,
    /// Show the saved versions of the open page.
    PageVersions,
    /// Open the active notebook's recycle bin.
    RecycleBin,
    /// Open the built-in symbol picker.
    Symbols,
    /// Put marks around the selection, or at the caret: bold, italic, strikethrough, code.
    Mark(&'static str),
    /// Make the block under the caret this kind: `p`, `h1`, `h2`, `h3`, `quote`, `bullet`,
    /// `number`, `todo`.
    Block(&'static str),
    /// Put a snippet in the page: a table, a link, an equation, a diagram, the date.
    Insert(&'static str),
    /// Put this TeX in the page as an equation: the one picked from the Equation gallery.
    Equation(&'static str),
    /// Add a file to the page (opens the same question a dropped file asks).
    AddFile,
    /// Pick a document and insert it as a printout.
    AddPrintout,
    /// Add a picture to the page.
    AddPicture,
    ToggleSource,
    ToggleFullPage,
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
    /// Set the page zoom to this percentage.
    ZoomTo(u32),
    /// Fit the page width to the available reading area.
    PageWidth,
    /// Cut or copy what is selected.
    Clipboard(&'static str),
    /// Put the clipboard into the page: a picture as a file beside it, anything else as
    /// words. Its own act rather than `Clipboard("paste")`, because a picture is not a
    /// clipboard operation on the text at all.
    Paste,
    /// More or less indent on a list item.
    Indent(bool),
    /// Which way the line the caret is in reads. `None` lets the words decide again.
    Direction(Option<notecraft_model::Dir>),
    /// Stretch the paragraph the caret is in to fill the column, at this strength, or stop
    /// when it is already at that strength. An Arabic paragraph is stretched by kashida, a
    /// Latin one by its spaces.
    Justify(notecraft_model::justify::Strength),
    /// Which way the whole page reads. `None` lets every line decide for itself.
    PageDirection(Option<notecraft_model::Dir>),
    /// Which way the selection, or what is typed next, reads: an isolated run inside the line.
    Embedding(notecraft_model::Dir),
    /// Take the marks off the selection.
    ClearFormatting,
    /// Put a tag on the line the caret is in, or take it off again.
    Tag(Tag),
    /// Something OneNote has that NoteCraft does not: say so plainly.
    NotBuilt(&'static str),
    /// Change the shape of the table the caret is in.
    Table(crate::pageedit::TableEdit),
    /// Pick a block of cells of the table the caret is in.
    TableSelect(crate::pageedit::TableSpan),
    /// Draw the lines round the cells of a table, or stop drawing them.
    TableBorders(bool),
    /// The colour behind the open page. `None` is the theme's own paper.
    PageColour(Option<notecraft_model::NodeColor>),
    /// Take back the last change to the page, or put it back.
    Undo,
    Redo,
    /// Hand the table the caret is in to GridCraft.
    ToGridCraft,
    /// Read the spreadsheet back into the table it came from.
    FromGridCraft,
    /// Start a spreadsheet beside the page and open it in GridCraft.
    NewGridCraft,
    /// A table of this many rows by this many columns, from the size picker.
    InsertTable(usize, usize),
    /// What the pointer does on the page: write, draw or rub out.
    InkPointer(Pointer),
    /// The pen at this place on the rack was picked up.
    InkPen(usize),
    /// The rack itself changed: a tool added, taken off, moved or altered.
    InkRack(RackEdit),
    /// How readily the pen answers pressure, 0 to 100 along the wedge.
    InkSensitivity(u8),
    /// Take the last stroke back.
    InkUndo,
    /// Put a drawing on the page at the caret.
    InkInsert,
    /// Save the drawing beside the page as a plain picture, with no pen readings in it.
    InkExportPlain,
    /// The user picked another tab.
    Tab(String),
}

/// What the pointer does on a page. The Draw tab sets it; everywhere else it is `Write`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pointer {
    /// The caret: the ordinary page, where a click puts the caret and a drag selects.
    #[default]
    Write,
    /// Lay down ink with the pen that is chosen.
    Draw,
    /// Rub out whatever the pointer passes over.
    Erase,
    /// Pick ink up: click a stroke to hold it, then move, stretch or delete it.
    Pick,
    /// Pick ink up by drawing a loop round it.
    Lasso,
    /// Draw a regular shape: the drag gives the two corners it is drawn between.
    Shape(notecraft_ink::shapes::Shape),
}

impl Pointer {
    pub fn id(self) -> &'static str {
        match self {
            Pointer::Write => "write",
            Pointer::Draw => "draw",
            Pointer::Erase => "erase",
            Pointer::Pick => "select",
            Pointer::Lasso => "lasso",
            Pointer::Shape(s) => s.id(),
        }
    }
}

/// What the user asked to do to the rack itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RackEdit {
    /// Put a new tool of this kind on the end.
    Add(Tool),
    /// Take the one at this place off.
    Remove(usize),
    /// Move it one place left (true) or right.
    Move(usize, bool),
    /// Change the one at this place to this.
    Set(usize, Nib),
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
pub struct View<'a> {
    /// True when the side panel is two columns.
    pub split_panes: bool,
    /// True when the window is kept above the others.
    pub always_on_top: bool,
    pub full_page: bool,
    pub rules: RuleLines,
    pub hide_title: bool,
    pub zoom_percent: u32,
    /// What the pointer does on the page.
    pub pointer: Pointer,
    /// The tools on the rack, and which of them is in hand.
    pub rack: &'a [Nib],
    pub nib: usize,
    /// True while the caret is in a table, which is when the Table tab is offered.
    pub in_table: bool,
    /// Whether tables are drawn with lines round their cells.
    pub table_borders: bool,
    /// True when GridCraft is installed, which is when a table can be handed to it.
    pub gridcraft: bool,
    /// The colour behind the open page, when it carries one of its own.
    pub page_colour: Option<notecraft_model::NodeColor>,
    /// Whether the page has anything to take back, and anything to put back.
    pub can_undo: bool,
    pub can_redo: bool,
    /// How readily the pen answers pressure, 0 to 100.
    pub sensitivity: u8,
    /// A page is open for page-specific History commands.
    pub has_page: bool,
    /// The style of the block the caret is in (a [`STYLES`] kind), or "" when there is no caret.
    pub style: &'static str,
}

impl View<'_> {
    /// The tool in hand, or the first on the rack if the saved number is out of range.
    pub fn in_hand(&self) -> Nib {
        self.rack.get(self.nib).or_else(|| self.rack.first()).copied().unwrap_or_else(|| rack::new_nib(Tool::Pen))
    }
}

/// How tall the ribbon is: the tab strip, then the groups.
pub const TAB_STRIP_H: f32 = 28.0;
/// The ribbon's own height under the tabs: the buttons, then the group names along the
/// bottom. Measured against OneNote at 100%, where a group is about 95 pixels tall with
/// the name taking the last 16 of them.
pub const GROUP_H: f32 = 88.0;
/// How far down a big button its icon, its first line of writing, and its second sit.
const ICON_Y: f32 = 20.0;
const LABEL_Y: f32 = 41.0;
const LINE_H: f32 = 12.0;
/// The narrowest a big button may be, and the room its writing gets either side.
const BIG: f32 = 50.0;
const BIG_PAD: f32 = 12.0;
const SMALL_W: f32 = 30.0;
const SMALL_H: f32 = 18.0;
/// Where the group's name sits above the ribbon's bottom edge.
const GROUP_NAME_Y: f32 = 10.0;
/// How far the tool in use sinks into the rack.
const DROP: f32 = 4.0;

/// Draws the ribbon. `tab` is the chosen tab, `source` whether the page source is shown.
/// Returns every act the user asked for this frame (usually none or one).
pub fn show(ui: &mut Ui, tab: &str, source: bool, view: &View<'_>, format: Option<PageFormat>, t: &Tokens) -> Vec<Act> {
    let mut acts = Vec::new();
    ui.add_space(3.0);
    let chosen = resolve_tab(tab, source, view.in_table);
    // A tab with many groups is wider than a narrow window: the Draw tab alone has eleven,
    // as OneNote's does. Rather than clipping the last of them, the groups scroll sideways.
    // Dragging must not scroll: the controls here are clicked and dragged themselves, and a
    // drag that scrolled the ribbon instead would take the pen out of the user's hand.
    egui::ScrollArea::horizontal()
        .id_salt(("ribbon", chosen))
        // The ribbon is an Office-style command band, not a document scroller.  A
        // visible horizontal scrollbar is painted across its bottom edge when a tab
        // is wider than the window, which looks like a stray black artifact.  Keep
        // the scroll area (overflow tabs still scroll) but do not paint that bar.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .scroll_source(egui::scroll_area::ScrollSource { drag: egui::scroll_area::DragScroll::Never, ..egui::scroll_area::ScrollSource::default() })
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(4.0);
                match chosen {
                    "Insert" => insert_tab(ui, view, t, &mut acts),
                    "Draw" => draw_tab(ui, view, t, &mut acts),
                    "History" => history_tab(ui, view, t, &mut acts),
                    "View" => view_tab(ui, source, view, t, &mut acts),
                    TABLE_TAB => table_tab(ui, view, t, &mut acts),
                    SOURCE_TAB => source_tab(ui, format, t, &mut acts),
                    _ => home_tab(ui, view, t, &mut acts),
                }
            });
        });
    acts
}

/// The tab that is really shown: the Source tab disappears with the source pane.
pub fn resolve_tab(tab: &str, source: bool, in_table: bool) -> &str {
    if tab == SOURCE_TAB && !source {
        return "Home";
    }
    // The Table tab goes with the caret: leaving a table leaves the tab, so the ribbon
    // never offers commands for a table that is no longer in hand.
    if tab == TABLE_TAB && !in_table {
        return "Home";
    }
    if TABS.contains(&tab) || (tab == SOURCE_TAB && source) || (tab == TABLE_TAB && in_table) { tab } else { "Home" }
}

/// Every tab that can be chosen now.
pub fn tabs(source: bool, in_table: bool) -> Vec<&'static str> {
    let mut v = TABS.to_vec();
    if in_table {
        v.push(TABLE_TAB);
    }
    if source {
        v.push(SOURCE_TAB);
    }
    v
}

/// The tab names, drawn on the coloured band at the top of the window, File first, as in
/// Office. The chosen tab is a raised tab in the ribbon's own colour that runs down into the
/// ribbon below it; the others are white writing on the band, lit when the pointer is over
/// one.
pub fn tab_strip(ui: &mut Ui, tab: &str, source: bool, in_table: bool, t: &Tokens, acts: &mut Vec<Act>) {
    let chosen = resolve_tab(tab, source, in_table);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(4.0);
        let names = std::iter::once(FILE_TAB).chain(tabs(source, in_table));
        for name in names {
            let selected = name == chosen;
            let font = egui::FontId::proportional(12.5);
            let galley = ui.painter().layout_no_wrap(name.to_string(), font.clone(), t.text);
            let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 26.0, TAB_STRIP_H), Sense::click());
            let painter = ui.painter_at(rect);
            if selected {
                // Raised: the tab and the ribbon are one surface, so there is no line between.
                let top = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };
                painter.rect_filled(egui::Rect::from_min_max(egui::pos2(rect.min.x, rect.min.y + 3.0), rect.max), top, t.panel);
            } else if resp.hovered() {
                painter.rect_filled(rect.shrink2(vec2(1.0, 3.0)), 3.0, t.band_hover);
            }
            let colour = if selected { t.accent } else { t.band_text };
            painter.text(rect.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, name, font, colour);
            let enabled = ui.is_enabled();
            let label = name.to_string();
            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label.clone()));
            if resp.clicked() {
                acts.push(if name == FILE_TAB { Act::File } else { Act::Tab(name.to_string()) });
            }
        }
    });
}

/// The name of the first tab, which opens the File screen instead of a ribbon.
pub const FILE_TAB: &str = "File";

/// One group of the ribbon: its controls, a line on the right, and its name underneath.
fn group(ui: &mut Ui, title: &str, t: &Tokens, add: impl FnOnce(&mut Ui)) {
    // The controls first; the group's name and its divider are painted around whatever they
    // took, so a group is exactly as wide as it needs to be, as in Office.
    let inner = ui
        .scope(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 3.0);
            ui.horizontal_top(|ui| {
                ui.add_space(6.0);
                add(ui);
                ui.add_space(6.0);
            });
        })
        .response
        .rect;
    let painter = ui.painter();
    painter.text(
        egui::pos2(inner.center().x, inner.min.y + GROUP_H - GROUP_NAME_Y),
        Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(10.0),
        t.muted,
    );
    // The name is painted, so it is no widget by itself and a screen reader would skip
    // straight from one group's buttons to the next with nothing to say where it is. It is
    // given its name here, the way `bidi::show` gives a painted line its words.
    let named = ui.interact(inner, ui.id().with(("group", title)), Sense::hover());
    let (enabled, label) = (ui.is_enabled(), title.to_owned());
    named.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &label));
    // A crisp divider between groups, on a whole pixel so it does not blur, and dark enough
    // to read on a light or a dark surface, the way Office separates its groups.
    let x = (inner.max.x + 7.0).round() + 0.5;
    painter.line_segment(
        [egui::pos2(x, inner.min.y + 3.0), egui::pos2(x, inner.min.y + GROUP_H - 5.0)],
        egui::Stroke::new(1.0, crate::theme::mix(t.panel, t.text, 0.16)),
    );
    ui.add_space(9.0);
}

/// A big button: an icon with its name under it.
/// A big button: the mark it makes above what it is called, as Office draws one. A label
/// with a newline in it is written on two lines, which is how "Panning Hand" and "Full
/// Page View" fit without making the button wide.
fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, false, false, t)
}

/// The same, with the small chevron Office puts on a button that opens something.
fn big_menu(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, true, false, t)
}

fn pictures_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let response = big_inner(ui, Icon::Picture, "Pictures", "Choose how to insert a picture", true, false, t);
    let arrow_rect = Rect::from_min_max(egui::pos2(response.rect.left(), response.rect.bottom() - 28.0), response.rect.right_bottom());
    let arrow = ui.interact(arrow_rect, ui.id().with("pictures-menu-arrow"), Sense::click());
    if arrow.hovered() {
        ui.painter().rect_filled(arrow_rect, 3.0, t.row_hover);
        chevron(&ui.painter_at(arrow_rect), arrow_rect.center(), t.text);
    }
    egui::Popup::menu(&arrow).show(|ui| {
        egui::Frame::popup(ui.style()).corner_radius(8.0).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_width(228.0);
            if picture_menu_row(ui, Icon::Picture, "Picture from a file", t, true).clicked() {
                acts.push(Act::AddPicture);
                ui.close();
            }
            let _ = picture_menu_row(ui, Icon::Attach, "From Camera", t, false);
            let _ = picture_menu_row(ui, Icon::Link, "From Online", t, false);
        });
    });
    response.clicked() && !arrow.clicked()
}

fn picture_menu_row(ui: &mut Ui, icon: Icon, label: &str, t: &Tokens, enabled: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(212.0, 36.0), Sense::click());
    if response.hovered() && enabled {
        ui.painter().rect_filled(rect, 6.0, crate::theme::mix(t.panel, t.accent, 0.12));
    }
    let color = if !enabled {
        t.muted
    } else if icon == Icon::Picture {
        t.ok
    } else {
        t.accent
    };
    let painter = ui.painter_at(rect);
    let icon_rect = Rect::from_center_size(egui::pos2(rect.left() + 20.0, rect.center().y), vec2(20.0, 20.0));
    icons::paint_ribbon(ui, icon_rect, icon, color);
    painter.text(
        egui::pos2(rect.left() + 43.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        egui::FontId::proportional(13.5),
        if enabled { t.text } else { t.muted },
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
    response
}

/// The small chevron Office puts under anything that opens a menu, and under the tool in
/// use on the rack.
fn chevron(painter: &egui::Painter, at: egui::Pos2, colour: egui::Color32) {
    painter.add(egui::Shape::line(
        vec![egui::pos2(at.x - 3.5, at.y - 1.5), egui::pos2(at.x, at.y + 2.0), egui::pos2(at.x + 3.5, at.y - 1.5)],
        egui::Stroke::new(1.2, colour),
    ));
}

fn big_inner(ui: &mut Ui, icon: Icon, label: &str, tip: &str, menu: bool, on: bool, t: &Tokens) -> egui::Response {
    let lines: Vec<&str> = label.split('\n').collect();
    let font = egui::FontId::proportional(11.0);
    let widest = lines.iter().map(|l| ui.painter().layout_no_wrap((*l).to_string(), font.clone(), t.text).size().x).fold(0.0_f32, f32::max);
    let (rect, resp) = ui.allocate_exact_size(vec2(BIG.max(widest + BIG_PAD), GROUP_H - 18.0), Sense::click());
    let painter = ui.painter_at(rect);
    if on {
        painter.rect_filled(rect, 4.0, crate::theme::mix(t.panel, t.accent, 0.10));
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    let colour = if !ui.is_enabled() {
        t.muted
    } else {
        match icon {
            Icon::Picture | Icon::Clip => t.ok,
            Icon::Attach => t.warn,
            Icon::Link => t.link,
            Icon::InsertSpace => t.accent,
            Icon::Calendar | Icon::Clock => t.strong,
            _ => t.accent,
        }
    };
    icons::paint_ribbon(ui, Rect::from_center_size(egui::pos2(rect.center().x, rect.min.y + ICON_Y), vec2(28.0, 28.0)), icon, colour);
    for (i, line) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = rect.min.y + LABEL_Y + i as f32 * LINE_H;
        painter.text(egui::pos2(rect.center().x, y), Align2::CENTER_CENTER, line, font.clone(), t.text);
    }
    if menu {
        // A chevron under the writing: this one opens something rather than doing it.
        #[allow(clippy::cast_precision_loss)]
        let y = rect.min.y + LABEL_Y + lines.len() as f32 * LINE_H - 1.0;
        chevron(&painter, egui::pos2(rect.center().x, y), t.muted);
    }
    let enabled = ui.is_enabled();
    // The name a test or a screen reader hears: one line, no matter how the label wraps.
    let name = label.replace('\n', " ");
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// A small button: an icon, or a letter for the marks that have one.
/// `big`, with the button shown as the one in use.
fn big_on(ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, false, on, t)
}

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
        Some(i) => icons::paint_ribbon(ui, Rect::from_center_size(rect.center(), vec2(17.0, 17.0)), i, t.accent),
        None => {
            painter.text(rect.center(), Align2::CENTER_CENTER, letter, egui::FontId::proportional(12.5), t.strong);
        }
    }
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// A compact Office-style row control with a clear icon and its visible label.
fn small_labeled_icon(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let text = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), t.text).size();
    let (rect, resp) = ui.allocate_exact_size(vec2(text.x + 34.0, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    icons::paint_ribbon(ui, Rect::from_center_size(egui::pos2(rect.min.x + 13.0, rect.center().y), vec2(18.0, 18.0)), icon, t.accent);
    painter.text(egui::pos2(rect.min.x + 27.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, false, name.clone()));
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
pub const STYLES: [(&str, &str); 12] = [
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
    ("123|code", "codenumbers"),
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

fn home_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    // Undo comes first, as it does in OneNote's own Home tab: it is the button a writer
    // reaches for in a hurry, and a page editor without one is not safe to write in.
    group(ui, "Undo", t, |ui| {
        rows(ui, |ui| {
            if row_button(ui, Icon::Undo, "Undo", "Ctrl+Z takes back the last change", view.can_undo, t).clicked() {
                acts.push(Act::Undo);
            }
            if row_button(ui, Icon::Redo, "Redo", "Ctrl+Y puts it back", view.can_redo, t).clicked() {
                acts.push(Act::Redo);
            }
        });
    });
    group(ui, "Clipboard", t, |ui| {
        if big_mark(ui, Mark::Paste, "Paste", "Ctrl+V pastes into the page", t).clicked() {
            acts.push(Act::Paste);
        }
        rows(ui, |ui| {
            if mark_row(ui, Mark::Cut, "Cut", "Cut the selection", t).clicked() {
                acts.push(Act::Clipboard("cut"));
            }
            if mark_row(ui, Mark::Copy, "Copy", "Copy the selection", t).clicked() {
                acts.push(Act::Clipboard("copy"));
            }
            if mark_row(ui, Mark::Painter, "Format Painter", "Not built", t).clicked() {
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
    // in Arabic wants one. The paragraph's direction is in the alignment menu above, where
    // OneNote and Word keep it, and here as well: a writer who has come looking for the
    // direction controls should find all of them in one place rather than be sent back to
    // a menu under an alignment button. The rarer run direction is in the More menu.
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
                if small(ui, Some(Icon::DirAuto), "Auto", "Page direction from the words", "Every paragraph decides for itself", false, t).clicked() {
                    acts.push(Act::PageDirection(None));
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Para").size(10.5).color(t.muted));
                if small(ui, Some(Icon::ParaLtr), "", "Paragraph left to right", "This paragraph reads left to right", false, t).clicked() {
                    acts.push(Act::Direction(Some(notecraft_model::Dir::Ltr)));
                }
                if small(ui, Some(Icon::ParaRtl), "", "Paragraph right to left", "This paragraph reads right to left (Arabic, Hebrew)", false, t)
                    .clicked()
                {
                    acts.push(Act::Direction(Some(notecraft_model::Dir::Rtl)));
                }
                if small(
                    ui,
                    Some(Icon::ParaAuto),
                    "Auto",
                    "Paragraph direction from the words",
                    "This paragraph decides from its own first letter",
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::Direction(None));
                }
            });
            run_direction_menu(ui, t, acts);
        });
    });
    group(ui, "Styles", t, |ui| {
        gallery(ui, "styles", view.style, t, acts, |ui, acts| {
            // The box shows two rows, as OneNote's does: the first two, or, when the caret is in
            // a later style, that one and the next (the last two for the last), so the current
            // style is in sight. Ordinary text keeps the headings in the box.
            let first = STYLES.iter().position(|(_, k)| *k == view.style && *k != "p").filter(|i| *i >= 2).map_or(0, |i| i.min(STYLES.len() - 2));
            for (label, kind) in STYLES.iter().skip(first).take(2) {
                if style_row(ui, label, kind, *kind == view.style, t).clicked() {
                    acts.push(Act::Block(leak(kind)));
                }
            }
        });
    });
    group(ui, "Tags", t, |ui| {
        gallery(ui, "tags", "", t, acts, |ui, acts| {
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
fn gallery(ui: &mut Ui, which: &'static str, current: &str, t: &Tokens, acts: &mut Vec<Act>, rows_in_box: impl FnOnce(&mut Ui, &mut Vec<Act>)) {
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
                            if style_row(ui, label, kind, kind == current, t).clicked() {
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
        if picked || away(ui, &area, &resp) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// The equations the Equation button offers, each a name and the TeX it puts in the page. They are
/// drawn in the list, so a person picks by looking. Every one of them is something the engine
/// draws in full; a test checks it.
pub const EQUATIONS: &[(&str, &str)] = &[
    ("Fraction", r"\frac{a}{b}"),
    ("Square root", r"\sqrt{x}"),
    ("Root of any degree", r"\sqrt[n]{x}"),
    ("Superscript and subscript", r"x_i^2"),
    ("Brackets", r"\left( \frac{a}{b} \right)"),
    ("Sum", r"\sum_{i=1}^{n} i"),
    ("Integral", r"\int_{a}^{b} f(x)\,dx"),
    ("Limit", r"\lim_{x \to 0} \frac{\sin x}{x}"),
    ("Binomial", r"\binom{n}{k}"),
    ("Accents", r"\hat{x} + \vec{v} + \bar{y}"),
    ("Matrix", r"\begin{pmatrix} a & b \\ c & d \end{pmatrix}"),
    ("Cases", r"f(x) = \begin{cases} x^2 & x \ge 0 \\ -x & x < 0 \end{cases}"),
    ("Aligned equations", r"\begin{aligned} a &= b + c \\ d &= e + f \end{aligned}"),
    ("Quadratic formula", r"x = \frac{-b \pm \sqrt{b^2-4ac}}{2a}"),
    ("Pythagorean theorem", r"a^2 + b^2 = c^2"),
    ("Euler's identity", r"e^{i\pi} + 1 = 0"),
];

/// The Equation button: a list of equations to start from, each drawn as it will look.
fn equation_gallery(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big_menu(ui, Icon::Equation, "Equation", "An equation, written in TeX: pick one to start from", t);
    let open_id = egui::Id::new("ribbon-equation-gallery");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        // Under the button, but kept inside the window: the button is far to the right.
        const WIDTH: f32 = 330.0;
        let at = egui::pos2(resp.rect.left().min(ui.ctx().content_rect().right() - WIDTH - 6.0).max(4.0), resp.rect.bottom());
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(WIDTH - 20.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    for (name, tex) in EQUATIONS {
                        // The row is the name and the formula; the whole of it is the button.
                        let inner = ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(vec2(150.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(*name).color(t.text)).selectable(false));
                            });
                            crate::mathview::inline(ui, tex, 15.0, t.text);
                        });
                        let row = ui.interact(inner.response.rect, open_id.with(("row", *name)), Sense::click());
                        if row.hovered() {
                            ui.painter().rect_filled(inner.response.rect.expand(2.0), 3.0, t.row_hover.gamma_multiply(0.6));
                        }
                        let (enabled, label) = (ui.is_enabled(), format!("Equation: {name}"));
                        row.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &label));
                        if row.clicked() {
                            acts.push(Act::Equation(tex));
                            picked = true;
                        }
                    }
                });
            })
            .response;
        if picked || away(ui, &area, &resp) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// OneNote's alignment button: a menu of alignment, then the paragraph's direction under a
/// separator, in that order. Left, centre and right are not built and say so; Justify is,
/// and is the one of the four NoteCraft has an answer for that no other note taking
/// application does (see `notecraft_text::kashida`).
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
                    // Word's three, under the names Word gives them, so a writer of Arabic
                    // meets what they already know. They decide how many kashidas a line
                    // may take; `docs/rtl-in-word-and-onenote.md` has the comparison.
                    for strength in notecraft_model::justify::Strength::ALL {
                        let label = match strength {
                            notecraft_model::justify::Strength::Low => "Justify Low",
                            notecraft_model::justify::Strength::Medium => "Justify Medium",
                            notecraft_model::justify::Strength::High => "Justify High",
                        };
                        if ui
                            .button(label)
                            .on_hover_text("Fill the column. Arabic is stretched at the joins of its letters, not at its spaces")
                            .clicked()
                        {
                            acts.push(Act::Justify(strength));
                            picked = true;
                        }
                    }
                    ui.separator();
                    if direction_row(ui, Some(Icon::ParaLtr), "Left-to-Right", t).clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Ltr)));
                        picked = true;
                    }
                    if direction_row(ui, Some(Icon::ParaRtl), "Right-to-Left", t).clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Rtl)));
                        picked = true;
                    }
                    if direction_row(ui, None, "Direction from the words", t).clicked() {
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

/// One direction choice in a menu, with the icon that identifies its scope: paragraph mark
/// for paragraph flow or caret-and-triangle for the direction of a text run.
fn direction_row(ui: &mut Ui, icon: Option<Icon>, label: &str, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width().max(150.0), 24.0), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    if let Some(icon) = icon {
        icons::paint_ribbon(ui, Rect::from_min_size(egui::pos2(rect.min.x + 4.0, rect.center().y - 8.0), vec2(16.0, 16.0)), icon, t.text);
    }
    painter.text(egui::pos2(rect.min.x + 28.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.5), t.text);
    let enabled = ui.is_enabled();
    let said = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &said));
    resp
}

/// The less common direction for a selected or newly typed run, kept in a compact menu like
/// the paragraph direction entries in OneNote's alignment menu.
fn run_direction_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = small(ui, None, "More...", "Text run direction", "Set the direction of selected text, or the text typed next", false, t);
    let open_id = egui::Id::new("ribbon-run-direction-menu");
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
                egui::Frame::popup(ui.style()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                    ui.set_width(220.0);
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    ui.label(egui::RichText::new("Selected text / next typing").small().color(t.muted));
                    ui.separator();
                    if direction_row(ui, Some(Icon::CaretLtr), "Left-to-Right", t).clicked() {
                        acts.push(Act::Embedding(notecraft_model::Dir::Ltr));
                        picked = true;
                    }
                    if direction_row(ui, Some(Icon::CaretRtl), "Right-to-Left", t).clicked() {
                        acts.push(Act::Embedding(notecraft_model::Dir::Rtl));
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
fn style_row(ui: &mut Ui, label: &str, kind: &str, current: bool, t: &Tokens) -> egui::Response {
    let (size, color, italics) = match kind {
        "h1" | "title" => (17.0, t.accent, false),
        "h2" => (15.0, t.accent, false),
        "h3" | "h4" => (13.5, t.muted, kind == "h4"),
        "h5" | "h6" => (12.5, t.accent, kind == "h6"),
        "quote" | "citation" => (12.5, t.muted, true),
        "codeblock" | "codenumbers" => (12.5, t.text, false),
        _ => (13.0, t.text, false),
    };
    let mut text = egui::RichText::new(label).size(size).color(color);
    if italics {
        text = text.italics();
    }
    if matches!(kind, "codeblock" | "codenumbers") {
        text = text.monospace();
    }
    let galley = egui::WidgetText::from(text).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    let width = ui.available_width().max(130.0).min(galley.size().x + 16.0).max(130.0);
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
    // The style the caret is in is boxed, as OneNote boxes it; the one under the pointer is shaded.
    if current {
        ui.painter().rect_filled(rect, 3.0, t.row_selected);
        ui.painter().rect_stroke(rect, 3.0, egui::Stroke::new(1.0, t.text), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.row_hover);
    }
    let at = egui::pos2(rect.left() + 6.0, rect.center().y - galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, current, name.clone()));
    resp
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
/// A small button with its name beside the mark, as Office's Cut, Copy and Format Painter
/// are drawn: a row of those reads as a list of actions, where bare icons read as a puzzle.
fn mark_row(ui: &mut Ui, mark: Mark, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let font = egui::FontId::proportional(11.0);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + SMALL_W + 8.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    let left = Rect::from_center_size(egui::pos2(rect.min.x + 11.0, rect.center().y), vec2(15.0, 15.0));
    ribbon_icons::paint(&painter, left, mark, mark.color(t.text, t.accent));
    painter.text(egui::pos2(rect.min.x + 23.0, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp.on_hover_text(tip)
}

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

/// OneNote's Insert tab, group for group and in its order. What NoteCraft can do is
/// wired; what it cannot is greyed and says so, so the tab is an honest map of where the
/// work is rather than a short list of what happens to exist.
fn insert_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Insert", t, |ui| {
        if big(ui, Icon::InsertSpace, "Insert\nSpace", "Add a blank paragraph at the caret", t).clicked() {
            acts.push(Act::Insert("space"));
        }
    });
    group(ui, "Tables", t, |ui| {
        table_menu(ui, view, t, acts);
    });
    group(ui, "Files", t, |ui| {
        if big(ui, Icon::Page, "File\nPrintout", "Pick a PDF or Office document and insert its pages as pictures", t).clicked() {
            acts.push(Act::AddPrintout);
        }
        if big(ui, Icon::Attach, "File\nAttachment", "A file: copied in, or linked where it is", t).clicked() {
            acts.push(Act::AddFile);
        }
        if view.gridcraft {
            if big(ui, Icon::Spreadsheet, "Spreadsheet", "A new spreadsheet beside the page, opened in GridCraft", t).clicked() {
                acts.push(Act::NewGridCraft);
            }
        } else {
            soon(ui, Icon::Spreadsheet, "Spreadsheet", t);
        }
    });
    group(ui, "Images", t, |ui| {
        soon(ui, Icon::Clip, "Screen\nClipping", t);
        if pictures_menu(ui, t, acts) {
            acts.push(Act::AddPicture);
        }
    });
    group(ui, "Media", t, |ui| {
        soon(ui, Icon::Video, "Online\nVideo", t);
    });
    group(ui, "Links", t, |ui| {
        if big(ui, Icon::Link, "Link", "A link to a page or an address", t).clicked() {
            acts.push(Act::Insert("link"));
        }
    });
    group(ui, "Recording", t, |ui| {
        soon(ui, Icon::Mic, "Transcribe", t);
        soon(ui, Icon::Video, "Record\nVideo", t);
    });
    group(ui, "Time Stamp", t, |ui| {
        if big(ui, Icon::Calendar, "Date", "Today's date", t).clicked() {
            acts.push(Act::Insert("date"));
        }
        if big(ui, Icon::Clock, "Time", "The time now", t).clicked() {
            acts.push(Act::Insert("time"));
        }
        if big(ui, Icon::Calendar, "Date &\nTime", "Both, as the page's own stamp is written", t).clicked() {
            acts.push(Act::Insert("datetime"));
        }
    });
    group(ui, "Pages", t, |ui| {
        soon(ui, Icon::PageStack, "Page\nTemplates", t);
    });
    group(ui, "Symbols", t, |ui| {
        equation_gallery(ui, t, acts);
        if big_menu(ui, Icon::Symbol, "Symbol", "Insert a common symbol", t).clicked() {
            acts.push(Act::Symbols);
        }
    });
    group(ui, "More", t, |ui| {
        // NoteCraft's own addition to OneNote's group: a diagram kept as its source, so it
        // can be read and changed in any editor rather than being a picture of one.
        if big(ui, Icon::Diagram, "Diagram", "A Mermaid diagram, kept as its source", t).clicked() {
            acts.push(Act::Insert("diagram"));
        }
        soon(ui, Icon::Sticker, "Stickers", t);
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

/// One page colour in the Format Background menu: a pale wash of it, as the page is
/// drawn, with a ring round the one the page is using.
fn colour_swatch(ui: &mut Ui, colour: Option<notecraft_model::NodeColor>, here: bool, t: &Tokens) -> egui::Response {
    let side = 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let dark = t.editor.r() < 128;
    let fill = match colour {
        Some(c) => {
            let (r, g, b) = c.rgb(dark);
            crate::theme::mix(t.editor, egui::Color32::from_rgb(r, g, b), 0.16)
        }
        None => t.editor,
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, fill);
    let (edge, width) = if here { (t.accent, 2.0) } else { (t.faint, 1.0) };
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(width, edge), egui::StrokeKind::Inside);
    if colour.is_none() {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "/", egui::FontId::proportional(13.0), t.muted);
    }
    let name = colour.map(notecraft_model::NodeColor::label).unwrap_or("No colour");
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, name));
    resp.on_hover_text(name)
}

/// OneNote's Table button: a grid to pick the size from, the way OneNote picks one.
///
/// Running the pointer over the grid lights the cells up to it and names the size, and a
/// click puts a table of that size on the page. OneNote's menu ends with a dialog and a
/// new Excel spreadsheet; the dialog is not built, and the spreadsheet is GridCraft's.
fn table_menu(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    const MOST: usize = 8;
    const SIDE: f32 = 17.0;
    const GAP: f32 = 3.0;

    let resp = big_menu(ui, Icon::Table, "Table", "A table: pick its size", t);
    let open_id = egui::Id::new("ribbon-insert-table");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if !open {
        ui.data_mut(|d| d.insert_temp(open_id, open));
        return;
    }
    let mut picked = false;
    let area = egui::Area::new(open_id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(resp.rect.left_bottom())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                let across = MOST as f32 * (SIDE + GAP) - GAP;
                // The size is named above the grid, as OneNote names it, so the rect is
                // taken first and written into once the pointer has been read.
                let (naming, _) = ui.allocate_exact_size(vec2(across, 18.0), Sense::hover());
                let (grid, _) = ui.allocate_exact_size(vec2(across, across), Sense::hover());
                // Every square is its own button, named for the size it picks: the grid
                // then answers the keyboard and a screen reader, not only the pointer.
                let mut over = None;
                let mut chose = None;
                let mut cells = Vec::with_capacity(MOST * MOST);
                for r in 0..MOST {
                    for c in 0..MOST {
                        let at = grid.min + vec2(c as f32 * (SIDE + GAP), r as f32 * (SIDE + GAP));
                        let cell = egui::Rect::from_min_size(at, egui::vec2(SIDE, SIDE));
                        let resp = ui.interact(cell, open_id.with((r, c)), Sense::click());
                        let name = format!("{} x {} Table", c + 1, r + 1);
                        let enabled = ui.is_enabled();
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
                        if resp.hovered() {
                            over = Some((r, c));
                        }
                        if resp.clicked() {
                            chose = Some((r, c));
                        }
                        cells.push((r, c, cell));
                    }
                }
                let paint = ui.painter();
                for (r, c, cell) in cells {
                    let lit = over.is_some_and(|(hr, hc)| r <= hr && c <= hc);
                    let (fill, line) = if lit { (t.accent.gamma_multiply(0.25), t.accent) } else { (t.editor, t.table_line) };
                    paint.rect_filled(cell, 1.0, fill);
                    paint.rect_stroke(cell, 1.0, egui::Stroke::new(1.0, line), egui::StrokeKind::Inside);
                }
                let title = match over {
                    Some((r, c)) => format!("{} x {} Table", c + 1, r + 1),
                    None => "Table".to_string(),
                };
                paint.text(naming.left_top(), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(13.0), t.text);
                if let Some((r, c)) = chose {
                    acts.push(Act::InsertTable(r + 1, c + 1));
                    picked = true;
                }
                ui.separator();
                ui.add_enabled(false, egui::Label::new(egui::RichText::new("Insert Table... (not built)").color(t.muted)));
                if view.gridcraft {
                    if ui.button("New GridCraft Spreadsheet").clicked() {
                        acts.push(Act::NewGridCraft);
                        picked = true;
                    }
                } else {
                    ui.add_enabled(false, egui::Label::new(egui::RichText::new("New GridCraft Spreadsheet (not installed)").color(t.muted)));
                }
            });
        })
        .response;
    let clicked_away =
        ui.input(|i| i.pointer.any_click()) && !area.rect.contains(ui.ctx().pointer_latest_pos().unwrap_or_default()) && !resp.clicked();
    if picked || clicked_away || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
}

/// OneNote's Rule Lines button: the paper now, and a gallery of every paper under it.
fn rule_lines_menu(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    let open_id = egui::Id::new("ribbon-rule-lines");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    let resp = big_inner(ui, Icon::Ruler, "Format\nBackground", "Lines or squares behind the page, as on paper", true, open, t);
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).corner_radius(8.0).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(8.0, 5.0);
                    let shelf = |ui: &mut Ui, title: &str, papers: &[RuleLines], acts: &mut Vec<Act>, picked: &mut bool| {
                        ui.label(egui::RichText::new(title).strong().color(t.text));
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
                    ui.add_space(1.0);
                    ui.label(egui::RichText::new("Every fifth line is slightly stronger for easier counting.").small().color(t.muted));
                    ui.separator();
                    // OneNote's Page Color, under the paper: a pale wash behind the words.
                    ui.label(egui::RichText::new("Page colour").strong().color(t.text));
                    ui.horizontal(|ui| {
                        if colour_swatch(ui, None, view.page_colour.is_none(), t).clicked() {
                            acts.push(Act::PageColour(None));
                            picked = true;
                        }
                        for c in notecraft_model::NodeColor::ALL {
                            if colour_swatch(ui, Some(c), view.page_colour == Some(c), t).clicked() {
                                acts.push(Act::PageColour(Some(c)));
                                picked = true;
                            }
                        }
                    });
                    ui.label(egui::RichText::new("The paper and colour apply to this page only.").small().color(t.muted));
                    ui.separator();
                    ui.add_enabled(false, egui::Label::new(egui::RichText::new("Rule line colour (not built)").color(t.muted)));
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

/// How readily the pen answers pressure, drawn as the wedge a volume control uses: thin
/// at the quiet end, thick at the loud one, so which way is "more ink" needs no label.
///
/// This is not thickness. Thickness is how wide the tool is at a full press; this is how
/// hard you have to press to get there. Turned up, a light hand already lays down a broad
/// line; turned down, it takes a firm press. The pen's own readings are kept as they came
/// either way, so a stroke drawn at one setting can be re-weighted later.
fn sensitivity(ui: &mut Ui, now: u8, t: &Tokens, acts: &mut Vec<Act>) {
    rows(ui, |ui| {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Sensitivity").size(10.0).color(t.muted));
        let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::click_and_drag());
        let share = f32::from(now.min(100)) / 100.0;
        let painter = ui.painter_at(rect);
        let track = rect.shrink2(vec2(6.0, 3.0));
        // The wedge: nothing at the left, full at the right.
        let wedge = vec![egui::pos2(track.min.x, track.max.y), egui::pos2(track.max.x, track.max.y), egui::pos2(track.max.x, track.min.y)];
        painter.add(egui::Shape::convex_polygon(wedge.clone(), crate::theme::mix(t.panel, t.text, 0.14), egui::Stroke::NONE));
        // The part that is turned on is filled in the accent, cut at the handle.
        let x = track.min.x + track.width() * share;
        let h = track.max.y - track.height() * share;
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(track.min.x, track.max.y), egui::pos2(x, track.max.y), egui::pos2(x, h)],
            t.accent,
            egui::Stroke::NONE,
        ));
        painter.circle(egui::pos2(x, rect.center().y), 5.0, t.panel, egui::Stroke::new(1.4, t.accent));

        if (resp.dragged() || resp.clicked())
            && let Some(at) = resp.interact_pointer_pos()
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let picked = (((at.x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0) * 100.0).round() as u8;
            if picked != now {
                acts.push(Act::InkSensitivity(picked));
            }
        }
        let enabled = ui.is_enabled();
        let said = format!("Pressure sensitivity {now} of 100");
        resp.widget_info(move || egui::WidgetInfo::slider(enabled, f64::from(now), &said));
        resp.on_hover_text("How readily the pen answers pressure: turned up, a light hand lays down more ink");
        ui.label(egui::RichText::new(format!("{now}")).size(10.5).color(t.text));
    });
}

/// OneNote's Add button: a new tool of whatever kind, put on the end of the rack.
fn add_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big_menu(ui, Icon::Plus, "Add", "Put another tool on the rack", t);
    let id = egui::Id::new("ribbon-add-pen");
    let mut open = ui.data(|d| d.get_temp::<bool>(id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
    }
    if open {
        let mut picked = false;
        let area = egui::Area::new(id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(resp.rect.left_bottom())
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(170.0);
                    ui.spacing_mut().item_spacing.y = 1.0;
                    for tool in Tool::ALL {
                        let (row, hit) = ui.allocate_exact_size(vec2(162.0, 28.0), Sense::click());
                        let painter = ui.painter_at(row);
                        if hit.hovered() {
                            painter.rect_filled(row, 4.0, t.row_hover);
                        }
                        // The tool itself beside its name, drawn by the code the rack
                        // uses, so a menu cannot show one thing and the rack another.
                        let glyph = Rect::from_min_size(row.min, vec2(26.0, row.height()));
                        paint_tool(&painter, glyph, tool, rack::new_nib(tool).colour, 0.0);
                        painter.text(
                            egui::pos2(row.min.x + 32.0, row.center().y),
                            Align2::LEFT_CENTER,
                            tool.label(),
                            egui::FontId::proportional(12.5),
                            t.text,
                        );
                        let enabled = ui.is_enabled();
                        let name = format!("Add {}", tool.label());
                        hit.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
                        if hit.clicked() {
                            acts.push(Act::InkRack(RackEdit::Add(tool)));
                            picked = true;
                        }
                    }
                    ui.separator();
                    not_built(ui, "Laser Pointer", t);
                });
            })
            .response;
        if picked || away(ui, &area, &resp) {
            open = false;
        }
    }
    ui.data_mut(|d| d.insert_temp(id, open));
}

/// True when the pointer was clicked anywhere but in this popup or on the button that
/// opened it, or Escape was pressed: either way the popup should close.
///
/// The button is tested by where it is, not by whether it reported a click this frame. A
/// press and its release land on different frames, so on the release the button has not
/// "clicked" and the popup it had just opened closed itself again.
fn away(ui: &Ui, area: &egui::Response, button: &egui::Response) -> bool {
    let at = ui.ctx().pointer_latest_pos().unwrap_or_default();
    let out = ui.input(|i| i.pointer.any_click()) && !area.rect.contains(at) && !button.rect.contains(at);
    out || ui.input(|i| i.key_pressed(egui::Key::Escape))
}

/// One tool on the rack, drawn as the tool: a pen standing on its nib, the way OneNote
/// racks them. The barrel is the colour it writes in, so the rack is read by looking.
///
/// Each is built the same way - a tapered silhouette, a darker grip, a bright band where
/// the two meet, one soft highlight down the left, and a thin outline - so seven of them
/// in a row look like one set of pens rather than seven drawings. The nib is what tells
/// them apart: a cone for a pen, bare wood and lead for a pencil, a long slit nib for a
/// fountain pen, a soft bulb for a brush, a slanted chisel for a highlighter.
fn pen_on_rack(ui: &mut Ui, i: usize, nib: Nib, chosen: bool, rack_len: usize, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(27.0, GROUP_H - 20.0), Sense::click());
    let enabled = ui.is_enabled();
    let name = format!("{} {}", nib.tool.label(), swatch_name(nib.colour));
    let label = name.clone();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, chosen, label.clone()));
    let painter = ui.painter_at(rect);
    if chosen {
        painter.rect_filled(rect, 4.0, crate::theme::mix(t.panel, t.accent, 0.10));
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    // The one in use sinks into the rack, the way a pen lifted out of a tray sits lower
    // than its neighbours, and it eases down rather than jumping.
    let drop = ui.ctx().animate_bool_with_time(resp.id.with("picked"), chosen, 0.12) * DROP;
    paint_tool(&painter, rect, nib.tool, nib.colour, drop);

    // The chevron on the tool in hand opens its editor, as OneNote's does; a right click
    // opens the same editor and the rest of the rack's own menu.
    let mut picked_up = resp.clicked();
    let chev = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.max.y - 5.0), vec2(14.0, 11.0));
    let mut modify = false;
    if chosen {
        let hit = ui.interact(chev, resp.id.with("chevron"), Sense::click());
        chevron(&painter, chev.center(), if hit.hovered() { t.accent } else { t.muted });
        if hit.clicked() {
            modify = true;
            picked_up = false;
        }
    }
    // The menu hangs off the pen's own response. A second interact over the same
    // rectangle looked harmless and was not: later widgets win in egui, so it swallowed
    // every left click and only the eraser, which had no such overlay, could be picked up.
    resp.context_menu(|ui| {
        if ui.button("Modify Tool").clicked() {
            modify = true;
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(i > 0, egui::Button::new("Move Left")).clicked() {
            acts.push(Act::InkRack(RackEdit::Move(i, true)));
            ui.close();
        }
        if ui.add_enabled(i + 1 < rack_len, egui::Button::new("Move Right")).clicked() {
            acts.push(Act::InkRack(RackEdit::Move(i, false)));
            ui.close();
        }
        ui.separator();
        if ui.button(format!("Add Another {}", nib.tool.label())).clicked() {
            acts.push(Act::InkRack(RackEdit::Add(nib.tool)));
            ui.close();
        }
        if ui.add_enabled(rack_len > 1, egui::Button::new(format!("Delete {}", nib.tool.label()))).clicked() {
            acts.push(Act::InkRack(RackEdit::Remove(i)));
            ui.close();
        }
    });

    let open_id = egui::Id::new(("modify-tool", i));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if modify {
        open = true;
        picked_up = true;
    }
    if open && modify_tool(ui, open_id, rect, i, nib, rack_len, t, acts) {
        open = false;
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    resp.on_hover_text(name);
    picked_up
}

/// OneNote's Modify Tool popup: the stroke this tool makes, the thicknesses as dots, the
/// colours as a grid, and a way to take the tool off the rack. Returns true when it should
/// close.
#[allow(clippy::too_many_arguments)]
fn modify_tool(ui: &mut Ui, id: egui::Id, under: Rect, i: usize, nib: Nib, rack_len: usize, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let mut done = false;
    let area = egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(under.left_bottom())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(196.0);
                // The stroke this tool makes, drawn by the code that draws the page's ink,
                // so the sample cannot say one thing and the page another.
                let (sample, _) = ui.allocate_exact_size(vec2(180.0, 26.0), Sense::hover());
                let mut stroke = notecraft_ink::Stroke::new(nib.tool, nib.colour, nib.width());
                for (at, press) in [(0.04, 0.12), (0.3, 0.55), (0.62, 1.0), (0.96, 0.3)] {
                    stroke.dabs.push(notecraft_ink::Dab::new(sample.width() * at, 13.0 + (at * 9.0).sin() * 3.0, press));
                }
                crate::inkpad::paint_stroke(&ui.painter_at(sample), sample.min, &stroke);
                ui.separator();

                ui.label(egui::RichText::new("Thickness").size(10.5).color(t.muted));
                ui.horizontal(|ui| {
                    for tenths in rack::THICKNESSES {
                        let here = nib.tenths == tenths;
                        let (dot, resp) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                        let painter = ui.painter_at(dot);
                        if here {
                            painter.circle_stroke(dot.center(), 10.0, egui::Stroke::new(1.3, t.accent));
                        } else if resp.hovered() {
                            painter.circle_filled(dot.center(), 10.0, t.row_hover);
                        }
                        // Each dot is the width it stands for, so the choice is made by eye.
                        let r = (f32::from(tenths) / 10.0 / 2.0).clamp(1.0, 7.0);
                        painter.circle_filled(dot.center(), r, t.text);
                        let enabled = ui.is_enabled();
                        let name = format!("{:.1} pt", f32::from(tenths) / 10.0);
                        resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, here, name.clone()));
                        if resp.on_hover_text(format!("{:.1} pt", f32::from(tenths) / 10.0)).clicked() {
                            acts.push(Act::InkRack(RackEdit::Set(i, Nib::new(nib.tool, nib.colour, tenths))));
                        }
                    }
                });

                ui.add_space(2.0);
                ui.label(egui::RichText::new("Colours").size(10.5).color(t.muted));
                for row in rack::PALETTE {
                    ui.horizontal(|ui| {
                        for colour in row {
                            let here = nib.colour == colour;
                            let (cell, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::click());
                            let painter = ui.painter_at(cell);
                            painter.rect_filled(cell.shrink(2.0), 3.0, egui::Color32::from_rgb(colour.0, colour.1, colour.2));
                            let edge = if here { t.accent } else { crate::theme::mix(t.panel, t.text, 0.25) };
                            painter.rect_stroke(
                                cell.shrink(2.0),
                                3.0,
                                egui::Stroke::new(if here { 2.0 } else { 0.8 }, edge),
                                egui::StrokeKind::Outside,
                            );
                            let enabled = ui.is_enabled();
                            let name = format!("{} {}", swatch_name(colour), colour.hex());
                            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, here, name.clone()));
                            if resp.on_hover_text(colour.hex()).clicked() {
                                acts.push(Act::InkRack(RackEdit::Set(i, Nib::new(nib.tool, colour, nib.tenths))));
                            }
                        }
                    });
                }
                ui.separator();
                not_built(ui, "More Colours", t);
                not_built(ui, "Eyedropper", t);
                if ui.add_enabled(rack_len > 1, egui::Button::new("Delete Tool")).clicked() {
                    acts.push(Act::InkRack(RackEdit::Remove(i)));
                    done = true;
                }
            });
        })
        .response;
    let at = ui.ctx().pointer_latest_pos().unwrap_or_default();
    let outside = ui.input(|i| i.pointer.any_click()) && !area.rect.contains(at) && !under.contains(at);
    done || outside || ui.input(|i| i.key_pressed(egui::Key::Escape))
}

/// The eraser, racked beside the pens because that is where a hand reaches for it, and
/// drawn like them: a pale sleeve a hand holds, and the worn rubber below it.
fn eraser_on_rack(ui: &mut Ui, chosen: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(27.0, GROUP_H - 20.0), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, chosen, "Eraser"));
    let painter = ui.painter_at(rect);
    if chosen {
        painter.rect_filled(rect, 4.0, crate::theme::mix(t.panel, t.accent, 0.10));
        painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }

    let drop = ui.ctx().animate_bool_with_time(resp.id.with("picked"), chosen, 0.12) * DROP;
    let cx = rect.center().x;
    let tall = 38.0;
    let top = rect.center().y - tall / 2.0 - 2.0 + drop;
    let foot = top + tall;
    // A block rubber seen square on: a pale grey body with a flat foot, and the band of
    // rubber at the bottom in the lilac OneNote gives it. No dome and no wedge; the
    // shape's whole job is to say "block" where the pens say "nib".
    let body = egui::Color32::from_rgb(0xdf, 0xe1, 0xe5);
    let body_edge = egui::Color32::from_rgb(0x8f, 0x94, 0x9d);
    let rubber = egui::Color32::from_rgb(0xe4, 0xc4, 0xee);
    let rubber_edge = egui::Color32::from_rgb(0xa9, 0x84, 0xb8);
    let w = 6.4;
    let band = foot - 10.0;
    let whole = egui::Rect::from_min_max(egui::pos2(cx - w, top), egui::pos2(cx + w, foot));
    painter.rect_filled(egui::Rect::from_min_max(whole.min, egui::pos2(whole.max.x, band)), 2.0, body);
    painter.rect_filled(egui::Rect::from_min_max(egui::pos2(whole.min.x, band - 2.0), whole.max), 2.0, rubber);
    painter.line_segment([egui::pos2(whole.min.x, band), egui::pos2(whole.max.x, band)], egui::Stroke::new(1.0, rubber_edge));
    painter.line_segment(
        [egui::pos2(cx - w * 0.45, top + 3.0), egui::pos2(cx - w * 0.45, band - 3.0)],
        egui::Stroke::new(1.2, egui::Color32::from_white_alpha(120)),
    );
    painter.rect_stroke(whole, 2.0, egui::Stroke::new(0.9, body_edge), egui::StrokeKind::Inside);
    if chosen {
        chevron(&painter, egui::pos2(rect.center().x, rect.max.y - 5.0), t.muted);
    }
    resp.on_hover_text("Rub out whatever the pointer passes over")
}

/// Draws one drawing tool upright inside `rect`, the way OneNote draws its rack.
///
/// The structure is the same for every tool, which is what makes seven of them look like
/// one set: a pale grey body for the part a hand holds, the writing colour in the lower
/// barrel, a bright collar where the two meet, and a nib below that. Only the nib differs:
/// a fine cone for a pen, sharpened wood for a pencil, a slit nib for a fountain pen, a
/// soft point for a brush, a slanted chisel for a highlighter. So the rack is read first
/// by colour and then by shape, as a tray of real pens is.
fn paint_tool(painter: &egui::Painter, rect: Rect, tool: Tool, colour: Rgb, drop: f32) {
    let ink = egui::Color32::from_rgb(colour.0, colour.1, colour.2);
    let mix = crate::theme::mix;
    let darker = |c: egui::Color32, amount: f32| mix(c, egui::Color32::BLACK, amount);
    let body = egui::Color32::from_rgb(0xee, 0xf0, 0xf3);
    let body_edge = egui::Color32::from_rgb(0x6f, 0x76, 0x81);
    let collar = egui::Color32::from_rgb(0x8d, 0x94, 0x9f);
    let shine = egui::Color32::from_white_alpha(130);

    let cx = rect.center().x;
    // Short and wide rather than long and thin, so the nib is a good third of the whole
    // and the eye lands on it: the tip is what says what mark the tool makes.
    let tall = (rect.height() - 14.0).clamp(16.0, 38.0);
    let top = rect.center().y - tall / 2.0 - 2.0 + drop;
    let foot = top + tall;
    let at = |x: f32, y: f32| egui::pos2(cx + x, y);
    let shape = |pts: Vec<egui::Pos2>, fill: egui::Color32, edge: egui::Color32| egui::Shape::convex_polygon(pts, fill, egui::Stroke::new(0.8, edge));

    // The pale part a hand holds, down to `to`, tapering from `w` to `lw`.
    let hold = |painter: &egui::Painter, w: f32, lw: f32, to: f32| {
        let at_to = w + (lw - w) * ((to - top) / (foot - top));
        painter.add(shape(vec![at(-w, top), at(w, top), at(at_to, to), at(-at_to, to)], body, body_edge));
        painter.line_segment([at(-w * 0.42, top + 3.0), at(-at_to * 0.42, to - 2.0)], egui::Stroke::new(1.2, shine));
        // The clip, down the right of the part a hand holds.
        painter.rect_filled(egui::Rect::from_min_max(at(w * 0.34, top + 2.5), at(w * 0.78, to - 3.0)), 1.0, collar);
        painter.line_segment([at(-at_to, to), at(at_to, to)], egui::Stroke::new(2.0, collar));
        at_to
    };

    match tool {
        Tool::Highlighter => {
            // Fatter and squarer than a pen, with a chisel cut across the bottom.
            let (w, lw) = (7.4, 6.9);
            let grip = top + 9.0;
            let nib = foot - 13.0;
            let g = hold(painter, w, lw, grip);
            let _ = g;
            painter.add(shape(vec![at(-lw, grip), at(lw, grip), at(lw, nib), at(-lw, nib)], ink, darker(ink, 0.35)));
            painter.add(shape(vec![at(-lw, nib), at(lw, nib), at(lw, foot - 6.0), at(-lw, foot)], darker(ink, 0.12), darker(ink, 0.4)));
        }
        Tool::Pencil => {
            // Painted body, bare wood, then the lead.
            let (w, lw) = (5.4, 5.0);
            let wood = foot - 15.0;
            painter.add(shape(vec![at(-w, top), at(w, top), at(lw, wood), at(-lw, wood)], ink, darker(ink, 0.4)));
            painter.line_segment([at(-w * 0.42, top + 3.0), at(-lw * 0.42, wood - 2.0)], egui::Stroke::new(1.2, shine));
            painter.line_segment([at(-w, top + 7.0), at(w, top + 7.0)], egui::Stroke::new(1.8, collar));
            painter.add(shape(
                vec![at(-lw, wood), at(lw, wood), at(1.5, foot - 3.6), at(-1.5, foot - 3.6)],
                egui::Color32::from_rgb(0xe8, 0xd0, 0xa6),
                egui::Color32::from_rgb(0xb2, 0x95, 0x66),
            ));
            painter.add(shape(
                vec![at(-1.5, foot - 3.6), at(1.5, foot - 3.6), at(0.0, foot)],
                egui::Color32::from_gray(60),
                egui::Color32::from_gray(40),
            ));
        }
        Tool::FountainPen => {
            let (w, lw) = (5.4, 5.0);
            let grip = top + 10.0;
            let nib = foot - 16.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-5.1, grip), at(5.1, grip), at(4.4, nib), at(-4.4, nib)], ink, darker(ink, 0.42)));
            // A long tapered nib, with its slit and breather hole.
            painter.add(shape(vec![at(-4.4, nib), at(4.4, nib), at(0.8, foot), at(-0.8, foot)], collar, body_edge));
            painter.line_segment([at(0.0, nib + 3.0), at(0.0, foot - 1.0)], egui::Stroke::new(0.9, egui::Color32::from_gray(90)));
            painter.circle_filled(at(0.0, nib + 2.5), 1.0, egui::Color32::from_gray(90));
        }
        Tool::BrushPen => {
            let (w, lw) = (5.6, 5.1);
            let grip = top + 10.0;
            let ferrule = foot - 16.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-5.3, grip), at(5.3, grip), at(4.7, ferrule), at(-4.7, ferrule)], ink, darker(ink, 0.42)));
            painter.line_segment([at(-3.4, ferrule), at(3.4, ferrule)], egui::Stroke::new(1.8, collar));
            // A loaded brush: full at the ferrule, drawn to a fine point.
            painter.add(shape(
                vec![at(-4.6, ferrule), at(4.6, ferrule), at(2.6, ferrule + 7.5), at(0.6, foot), at(-0.6, foot), at(-2.6, ferrule + 7.5)],
                darker(ink, 0.15),
                darker(ink, 0.45),
            ));
        }
        Tool::Pen => {
            let (w, lw) = (5.4, 5.0);
            let grip = top + 11.0;
            let nib = foot - 14.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-5.1, grip), at(5.1, grip), at(4.7, nib), at(-4.7, nib)], ink, darker(ink, 0.42)));
            painter.add(shape(vec![at(-4.7, nib), at(4.7, nib), at(0.9, foot), at(-0.9, foot)], darker(ink, 0.25), darker(ink, 0.5)));
        }
    }
}

/// A plain name for a colour, so a pen can be told from its neighbour by a screen reader
/// and by a test. Only the colours on the rack need one.
fn swatch_name(colour: Rgb) -> &'static str {
    match (colour.0, colour.1, colour.2) {
        (0x20, 0x20, 0x20) => "black",
        (0xd0, 0x21, 0x21) => "red",
        (0x18, 0x55, 0xc4) => "blue",
        (0x16, 0x7f, 0x3d) => "green",
        (0x55, 0x55, 0x55) => "grey",
        (0xff, 0xdd, 0x33) => "yellow",
        (0x6f, 0xe3, 0x9a) => "mint",
        _ => "custom",
    }
}

/// A big button that is drawn but does nothing, for what OneNote has and NoteCraft does
/// not. It keeps the tab's shape honest: the control is where a OneNote user looks for it,
/// greyed, saying plainly that it is not here yet rather than being left out.
/// A small button with an icon and a word, greyed when there is nothing for it to do.
fn row_button(ui: &mut Ui, icon: Icon, label: &str, tip: &str, live: bool, t: &Tokens) -> egui::Response {
    let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
    let resp = ui.add_enabled_ui(live, |ui| small(ui, Some(icon), "", label, tip, false, if live { t } else { &dim })).inner;
    if live { resp } else { resp.on_hover_text(format!("{label}: nothing to {}", label.to_lowercase())) }
}

/// OneNote's Shapes group: a row of regular shapes, and the one in hand is lit.
///
/// A shape drawn here is ink like any other stroke, so the eraser, Select and Undo Stroke
/// all work on it the day it is drawn.
fn shapes_group(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    use notecraft_ink::shapes::Shape;
    group(ui, "Shapes", t, |ui| {
        let in_hand = |s: Shape| view.pointer == Pointer::Shape(s);
        let mut row = |ui: &mut Ui, shapes: &[Shape]| {
            ui.vertical(|ui| {
                for s in shapes {
                    if shape_button(ui, *s, in_hand(*s), t).clicked() {
                        acts.push(Act::InkPointer(Pointer::Shape(*s)));
                    }
                }
            });
        };
        row(ui, &[Shape::Line, Shape::Arrow, Shape::Rectangle]);
        row(ui, &[Shape::RoundedRectangle, Shape::Square, Shape::Ellipse]);
        row(ui, &[Shape::Circle, Shape::Diamond, Shape::Triangle]);
        row(ui, &[Shape::Star]);
    });
}

/// One shape on the Shapes row: the shape itself, drawn small, with its name beside it.
fn shape_button(ui: &mut Ui, shape: notecraft_ink::shapes::Shape, here: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(86.0, 20.0), Sense::click());
    if here || resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, if here { t.row_selected } else { t.row_hover });
    }
    // The shape is traced with the same code that draws it on the page, so the button
    // cannot show one thing and the pen draw another.
    let box_ = egui::Rect::from_min_size(egui::pos2(rect.min.x + 4.0, rect.min.y + 4.0), vec2(12.0, 12.0));
    let dabs = notecraft_ink::shapes::trace(shape, (box_.min.x, box_.min.y), (box_.max.x, box_.max.y), 1.0);
    let points: Vec<egui::Pos2> = dabs.iter().map(|d| egui::pos2(d.x, d.y)).collect();
    if points.len() >= 2 {
        ui.painter().add(egui::Shape::line(points, egui::Stroke::new(1.2, if here { t.accent } else { t.text })));
    }
    let at = egui::pos2(rect.min.x + 24.0, rect.center().y);
    ui.painter().text(at, egui::Align2::LEFT_CENTER, shape.label(), egui::FontId::proportional(11.5), t.text);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, shape.label()));
    resp.on_hover_text(format!("{}: drag on the page to draw one", shape.label()))
}

fn soon(ui: &mut Ui, icon: Icon, label: &str, t: &Tokens) {
    let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
    let resp = ui.add_enabled_ui(false, |ui| big(ui, icon, label, "Not built yet", &dim)).inner;
    resp.on_hover_text(format!("{label}: not built yet"));
}

/// OneNote's History tab, with the same groups and controls. NoteCraft does not yet keep
/// unread flags or authors, so those controls are visible but disabled.
fn history_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Unread", t, |ui| {
        soon(ui, Icon::PageStack, "Next\nUnread", t);
        soon(ui, Icon::Todo, "Mark as\nRead", t);
    });
    group(ui, "Authors", t, |ui| {
        soon(ui, Icon::Clock, "Recent\nEdits", t);
        soon(ui, Icon::Search, "Find by\nAuthor", t);
        soon(ui, Icon::Page, "Hide\nAuthors", t);
    });
    group(ui, "History", t, |ui| {
        if ui
            .add_enabled_ui(view.has_page, |ui| big(ui, Icon::PageStack, "Page\nVersions", "See and restore saved copies of this page", t))
            .inner
            .clicked()
        {
            acts.push(Act::PageVersions);
        }
        if ui
            .add_enabled_ui(view.has_page, |ui| {
                big(ui, Icon::Notebook, "Notebook\nRecycle Bin", "Restore pages, sections and groups removed from this notebook", t)
            })
            .inner
            .clicked()
        {
            acts.push(Act::RecycleBin);
        }
    });
}

/// OneNote's Draw tab, group for group, and built the way OneNote builds it: big buttons
/// with the mark each one makes above its name. What NoteCraft cannot do yet is greyed and
/// says so, rather than being left out, so the tab is an honest map of the ink work.
fn draw_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, "Undo", t, |ui| {
        // Named apart from the group they sit in, so a screen reader and a test can tell
        // the Undo group from the Undo button inside it.
        if big(
            ui,
            Icon::Undo,
            "Undo
stroke",
            "Take the last stroke back",
            t,
        )
        .clicked()
        {
            acts.push(Act::InkUndo);
        }
        soon(
            ui,
            Icon::Redo,
            "Redo
stroke",
            t,
        );
    });
    group(ui, "Selection", t, |ui| {
        if big_on(ui, Icon::Pointer, "Select", "Click a stroke to pick it up, then move, stretch or delete it", view.pointer == Pointer::Pick, t)
            .clicked()
        {
            acts.push(Act::InkPointer(Pointer::Pick));
        }
        if big_on(
            ui,
            Icon::Lasso,
            "Lasso
Select",
            "Draw a loop round ink to pick it up",
            view.pointer == Pointer::Lasso,
            t,
        )
        .clicked()
        {
            acts.push(Act::InkPointer(Pointer::Lasso));
        }
        // OneNote has no such button: there, the pointer is always the caret until a pen
        // is picked up. Ours needs one, because picking ink up and writing words are both
        // things a plain click does, and only the person knows which they meant.
        if big_on(ui, Icon::DirLtr, "Type", "Put the pointer back to the words and the caret", view.pointer == Pointer::Write, t).clicked() {
            acts.push(Act::InkPointer(Pointer::Write));
        }
    });
    group(ui, "Drawing Tools", t, |ui| {
        if eraser_on_rack(ui, view.pointer == Pointer::Erase, t).clicked() {
            acts.push(Act::InkPointer(Pointer::Erase));
        }
        for (i, nib) in view.rack.iter().enumerate() {
            let chosen = view.pointer == Pointer::Draw && view.nib == i;
            if pen_on_rack(ui, i, *nib, chosen, view.rack.len(), t, acts) {
                acts.push(Act::InkPen(i));
            }
        }
        add_menu(ui, t, acts);
        sensitivity(ui, view.sensitivity, t, acts);
    });
    group(ui, "Input Mode", t, |ui| {
        soon(
            ui,
            Icon::Hand,
            "Panning
Hand",
            t,
        );
    });
    group(ui, "Stencils", t, |ui| {
        soon(ui, Icon::Ruler, "Ruler", t);
    });
    group(ui, "Edit", t, |ui| {
        if big(ui, Icon::Pen, "Draw\nhere", "Put a sheet to draw on into the page", t).clicked() {
            acts.push(Act::InkInsert);
        }
        if big(ui, Icon::Picture, "Save\npicture", "Write the drawing again with no pen readings in it, for sending on", t).clicked() {
            acts.push(Act::InkExportPlain);
        }
        soon(
            ui,
            Icon::InsertSpace,
            "Insert
Space",
            t,
        );
        // OneNote's Format Background is this tab's way to the paper, and we have the
        // paper, so it goes to the same gallery the View tab opens.
        rule_lines_menu(ui, view, t, acts);
    });
    shapes_group(ui, view, t, acts);
    group(ui, "Shapes", t, |ui| {
        soon(
            ui,
            Icon::Shapes,
            "Automatic
Shapes",
            t,
        );
    });
    group(ui, "Text", t, |ui| {
        soon(
            ui,
            Icon::InkToText,
            "Ink to
Text",
            t,
        );
    });
    group(ui, "Math", t, |ui| {
        soon(
            ui,
            Icon::Equation,
            "Ink to
Math",
            t,
        );
    });
    group(ui, "Replay", t, |ui| {
        soon(
            ui,
            Icon::Replay,
            "Ink
Replay",
            t,
        );
    });
    group(ui, "Mode", t, |ui| {
        if big(ui, Icon::FullPage, "Full Page\nView", "Hide everything but the page", t).clicked() {
            acts.push(Act::ToggleFullPage);
        }
    });
    group(ui, "Help", t, |ui| {
        soon(
            ui,
            Icon::Help,
            "Ink
Help",
            t,
        );
    });
}

/// OneNote's Table tab, which appears while the caret is in a table. What NoteCraft can
/// do to a table is wired; the rest is greyed where a OneNote user looks for it.
fn table_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    use crate::pageedit::TableEdit as E;
    use crate::pageedit::TableSpan as S;
    group(ui, "Select", t, |ui| {
        if big(ui, Icon::Table, "Select\nTable", "Pick the whole table", t).clicked() {
            acts.push(Act::TableSelect(S::Whole));
        }
        if big(ui, Icon::TableColumn, "Select\nColumns", "Pick the caret's column", t).clicked() {
            acts.push(Act::TableSelect(S::Column));
        }
        if big(ui, Icon::TableRow, "Select\nRows", "Pick the caret's row", t).clicked() {
            acts.push(Act::TableSelect(S::Row));
        }
    });
    group(ui, "Delete", t, |ui| {
        soon(ui, Icon::Table, "Delete\nTable", t);
        if big(ui, Icon::TableColumn, "Delete\nColumns", "Take the caret's column out", t).clicked() {
            acts.push(Act::Table(E::DeleteColumn));
        }
        if big(ui, Icon::TableRow, "Delete\nRows", "Take the caret's row out", t).clicked() {
            acts.push(Act::Table(E::DeleteRow));
        }
    });
    group(ui, "Insert", t, |ui| {
        if big(ui, Icon::TableRow, "Insert Row\nAbove", "A row above the caret's", t).clicked() {
            acts.push(Act::Table(E::InsertRow(true)));
        }
        if big(ui, Icon::TableRow, "Insert Row\nBelow", "A row below the caret's", t).clicked() {
            acts.push(Act::Table(E::InsertRow(false)));
        }
        if big(ui, Icon::TableColumn, "Insert Column\nLeft", "A column left of the caret's", t).clicked() {
            acts.push(Act::Table(E::InsertColumn(true)));
        }
        if big(ui, Icon::TableColumn, "Insert Column\nRight", "A column right of the caret's", t).clicked() {
            acts.push(Act::Table(E::InsertColumn(false)));
        }
    });
    group(ui, "Merge", t, |ui| {
        soon(ui, Icon::Table, "Merge\nCells", t);
        soon(ui, Icon::Table, "Split\nCells", t);
    });
    group(ui, "Format", t, |ui| {
        // One button that says what it will do, the way OneNote's does: it reads Hide
        // Borders while they are drawn, and Show Borders once they are not.
        let hiding = view.table_borders;
        let label = if hiding { "Hide\nBorders" } else { "Show\nBorders" };
        let tip = if hiding { "Draw tables without lines" } else { "Draw the lines round the cells again" };
        if big_on(ui, Icon::Table, label, tip, !hiding, t).clicked() {
            acts.push(Act::TableBorders(!hiding));
        }
        soon(ui, Icon::Picture, "Shading", t);
    });
    group(ui, "Alignment", t, |ui| {
        soon(ui, Icon::DirLtr, "Align\nLeft", t);
        soon(ui, Icon::DirLtr, "Center", t);
        soon(ui, Icon::DirRtl, "Align\nRight", t);
    });
    group(ui, "Data", t, |ui| {
        soon(ui, Icon::ListNumber, "Sort", t);
    });
    group(ui, "Convert", t, |ui| {
        // Where OneNote converts a table to an Excel spreadsheet, NoteCraft hands it to
        // GridCraft, the open spreadsheet from the same workshop. Greyed when GridCraft
        // is not installed, because there is nowhere to hand it to.
        if view.gridcraft {
            if big(ui, Icon::Spreadsheet, "Convert to\nGridCraft", "Write the table out and open it in GridCraft", t).clicked() {
                acts.push(Act::ToGridCraft);
            }
            // The table went out as a file; this brings back what the spreadsheet made of
            // it. Nothing watches the file, because a page being edited should not change
            // under the writer's hands.
            if big(ui, Icon::Spreadsheet, "Read back\nfrom it", "Put what the spreadsheet holds back into this table", t).clicked() {
                acts.push(Act::FromGridCraft);
            }
        } else {
            let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
            let resp = ui.add_enabled_ui(false, |ui| big(ui, Icon::Spreadsheet, "Convert to\nGridCraft", "GridCraft is not installed", &dim)).inner;
            resp.on_hover_text("Convert to GridCraft: GridCraft is not installed");
        }
    });
}

fn view_tab(ui: &mut Ui, source: bool, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
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
        if big(ui, if view.full_page { Icon::ExitFullPage } else { Icon::FullPage }, "Full Page\nView", "Give the page the whole window", t).clicked()
        {
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
        ui.horizontal(|ui| {
            if big(ui, Icon::ZoomOut, "Zoom Out", "Make the page smaller", t).clicked() {
                acts.push(Act::Zoom(-1));
            }
            if big(ui, Icon::ZoomIn, "Zoom In", "Make the page larger", t).clicked() {
                acts.push(Act::Zoom(1));
            }
            rows(ui, |ui| {
                egui::ComboBox::from_id_salt("page-zoom-percent").width(76.0).selected_text(format!("{}%", view.zoom_percent)).show_ui(ui, |ui| {
                    for percent in [50, 60, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250] {
                        if ui.selectable_label(view.zoom_percent == percent, format!("{percent}%")).clicked() {
                            acts.push(Act::ZoomTo(percent));
                        }
                    }
                });
                if small(ui, None, "100%", "Zoom to 100%", "Show the page at 100%", false, t).clicked() {
                    acts.push(Act::Zoom(0));
                }
                if small_labeled_icon(ui, Icon::PageWidth, "Page Width", "Fit the page to the available width", t).clicked() {
                    acts.push(Act::PageWidth);
                }
            });
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
/// Something OneNote has that NoteCraft does not, among the small controls: drawn as a
/// greyed chip the size of a control rather than as a line of writing, so a row of them
/// reads as buttons that are off and not as a paragraph.
fn not_built(ui: &mut Ui, label: &str, t: &Tokens) {
    let font = egui::FontId::proportional(10.5);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.muted).size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, SMALL_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, crate::theme::mix(t.panel, t.text, 0.14)), egui::StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, t.muted);
    let enabled = ui.is_enabled();
    let name = format!("{label} (not built)");
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    resp.on_hover_text(format!("{label}: not built yet"));
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
        assert_eq!(tabs(false, false), vec!["Home", "Insert", "Draw", "History", "View"]);
        assert_eq!(tabs(true, false), vec!["Home", "Insert", "Draw", "History", "View", "Source"]);
        assert_eq!(resolve_tab("Insert", false, false), "Insert");
        assert_eq!(resolve_tab("Draw", false, false), "Draw");
        assert_eq!(resolve_tab("Source", true, false), "Source");
        assert_eq!(resolve_tab("Source", false, false), "Home", "the Source tab goes when the source pane does");
        assert_eq!(resolve_tab("Nonsense", false, false), "Home");
        assert_eq!(resolve_tab("", true, false), "Home");

        // The Table tab goes with the caret: it is offered only while the caret is in a
        // table, and leaving one takes the tab and its commands away.
        assert_eq!(tabs(false, true), vec!["Home", "Insert", "Draw", "History", "View", "Table"]);
        assert_eq!(resolve_tab("Table", false, true), "Table");
        assert_eq!(resolve_tab("Table", false, false), "Home", "out of the table, out of the tab");
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
        for tab in ["Home", "Insert", "History", "View", "Source", "junk"] {
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

//! The ribbon: a tab strip and, under it, the groups of controls for the chosen tab, the way
//! Office and OneNote arrange them. It follows the shape the sibling Craft apps use (a tab
//! strip, groups of buttons, a label under each group) without sharing their code.
//!
//! Every control here asks for an [`Act`]; the window carries it out. Nothing in this module
//! touches the notebook, so the whole ribbon is tested by asking what the buttons return.

use crate::floating::{self, Glyph};
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
pub const FONT_SIZES: [u8; 16] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];
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
    /// Apply a point size to the selected text.
    FontSize(u8),
    /// Apply an installed font family to the selected text.
    FontFamily(String),
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
    /// Snap boxes to the page's grid when they are moved or resized, or stop.
    ToggleSnap,
    /// Format Painter: pick up the formatting at the caret for the next words picked. True keeps
    /// painting until Escape (a double-click, as in Office).
    FormatPainter(bool),
    /// Fold the ribbon away to its tab names, or put it back (Ctrl+F1).
    ToggleRibbon,
    /// Sort the table's rows by the caret's column: true for descending (issue #171).
    TableSort(bool),
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
    /// Align the words in the caret's cell, or in every picked cell (issue #170).
    TableAlign(notecraft_model::tablecols::Align),
    /// A colour behind the caret's cell, or behind every picked cell; `None` takes it away
    /// (issue #169).
    TableShade(Option<notecraft_model::NodeColor>),
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

    pub fn translated_label(self) -> &'static str {
        crate::tl!(self.label())
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

    pub fn translated_short(self) -> &'static str {
        crate::tl!(self.short())
    }

    pub fn translated_mid(self) -> &'static str {
        match self {
            RuleLines::None => crate::tl_id!("ui.none"),
            RuleLines::Narrow => crate::tl_id!("ui.narrow_ruled"),
            RuleLines::College => crate::tl_id!("ui.college_ruled"),
            RuleLines::Standard => crate::tl_id!("ui.standard_ruled"),
            RuleLines::Wide => crate::tl_id!("ui.wide_ruled"),
            RuleLines::GridSmall => crate::tl_id!("ui.small_squares"),
            RuleLines::GridMedium => crate::tl_id!("ui.medium_squares"),
            RuleLines::GridLarge => crate::tl_id!("ui.large_squares"),
            RuleLines::GridHuge => crate::tl_id!("ui.very_large_squares"),
            RuleLines::CraftLines => crate::tl_id!("ui.notecraft_ruled"),
            RuleLines::CraftGrid => crate::tl_id!("ui.notecraft_squares"),
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

/// Paints rule lines in a viewport, one of them through `anchor` and the rest a step apart, so
/// they stay put on the page as it pans and a box snapped to the page's grid sits on a line.
/// The window passes the page's origin, which is where box positions are counted from.
pub fn paint_paper_canvas(
    painter: &egui::Painter,
    area: egui::Rect,
    rules: RuleLines,
    scale: f32,
    anchor: egui::Pos2,
    line: egui::Color32,
    strong: egui::Color32,
) {
    if !rules.on() {
        return;
    }
    let step = (rules.step() * scale).max(4.0);
    let index = |screen: f32, anchor: f32| {
        let line = ((screen - anchor) / step).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
        {
            line.clamp(i64::MIN as f32, i64::MAX as f32) as i64
        }
    };
    let colour = |n: i64| if rules.fives() && n.rem_euclid(5) == 0 { strong } else { line };

    let mut y = first_grid_line(area.min.y, anchor.y, step);
    while y < area.max.y {
        painter.line_segment([egui::pos2(area.min.x, y), egui::pos2(area.max.x, y)], egui::Stroke::new(1.0, colour(index(y, anchor.y))));
        y += step;
    }
    if rules.squares() {
        let mut x = first_grid_line(area.min.x, anchor.x, step);
        while x < area.max.x {
            painter.line_segment([egui::pos2(x, area.min.y), egui::pos2(x, area.max.y)], egui::Stroke::new(1.0, colour(index(x, anchor.x))));
            x += step;
        }
    }
}

/// The first line at or after `min` on a grid `step` apart that has a line through `anchor`.
fn first_grid_line(min: f32, anchor: f32, step: f32) -> f32 {
    anchor + ((min - anchor) / step).ceil() * step
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
    /// Whether boxes snap to the page's grid when they are moved or resized.
    pub snap: bool,
    pub zoom_percent: u32,
    /// What the pointer does on the page.
    pub pointer: Pointer,
    /// The tools on the rack, and which of them is in hand.
    pub rack: &'a [Nib],
    pub nib: usize,
    /// True while the caret is in a table, which is when the Table tab is offered.
    pub in_table: bool,
    /// Whether the table the caret is in is drawn with lines round its cells.
    pub table_borders: bool,
    /// How the words in the caret's cell, or in every picked cell, are aligned; `None` when
    /// the picked cells differ.
    pub table_align: Option<notecraft_model::tablecols::Align>,
    /// The colour behind the caret's cell, or behind every picked cell: `Some(None)` for none,
    /// and `None` when the picked cells differ.
    pub table_shade: Option<Option<notecraft_model::NodeColor>>,
    /// True when GridCraft is installed, which is when a table can be handed to it.
    pub gridcraft: bool,
    /// The colour behind the open page, when it carries one of its own.
    pub page_colour: Option<notecraft_model::NodeColor>,
    /// Whether the page has anything to take back, and anything to put back.
    pub can_undo: bool,
    pub can_redo: bool,
    /// How readily the pen answers pressure, 0 to 100.
    pub sensitivity: u8,
    /// Whether the current input path has reported a pressure-capable pen.
    pub pressure_available: bool,
    /// A page is open for page-specific History commands.
    pub has_page: bool,
    /// The Format Painter is holding formatting to put on the next words picked.
    pub painting: bool,
    /// The style of the block the caret is in (a [`STYLES`] kind), or "" when there is no caret.
    pub style: &'static str,
    pub font_family: Option<&'a str>,
    pub font_size: Option<&'a str>,
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
pub const GROUP_H: f32 = 92.0;
/// How far down a big button its icon, its first line of writing, and its second sit.
const ICON_Y: f32 = 20.0;
const LABEL_Y: f32 = 41.0;
const LINE_H: f32 = 14.0;
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
pub fn tab_strip(ui: &mut Ui, tab: &str, source: bool, in_table: bool, collapsed: bool, t: &Tokens, acts: &mut Vec<Act>) {
    // Folded, no tab is the raised one: there is no ribbon under it to run into.
    let chosen = if collapsed { "" } else { resolve_tab(tab, source, in_table) };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(4.0);
        let names = std::iter::once(FILE_TAB).chain(tabs(source, in_table));
        for name in names {
            let selected = name == chosen;
            let display_name = crate::tl!(name);
            let font = egui::FontId::proportional(13.5);
            let galley = ui.painter().layout_no_wrap(display_name.to_string(), font.clone(), t.text);
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
            painter.text(rect.center() + vec2(0.0, 1.0), Align2::CENTER_CENTER, display_name, font, colour);
            let enabled = ui.is_enabled();
            let label = display_name.to_string();
            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, selected, label.clone()));
            if resp.clicked() {
                acts.push(if name == FILE_TAB { Act::File } else { Act::Tab(name.to_string()) });
            }
        }
        // At the far end, the control that folds the ribbon away and puts it back, as Office
        // has one; Ctrl+F1 does the same (issue #136).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(6.0);
            if ribbon_fold(ui, collapsed, t).clicked() {
                acts.push(Act::ToggleRibbon);
            }
        });
    });
}

/// The fold control at the end of the tab names: a chevron up to fold the ribbon away, down
/// to put it back.
fn ribbon_fold(ui: &mut Ui, collapsed: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, TAB_STRIP_H - 6.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.band_hover);
    }
    let c = rect.center();
    let stroke = egui::Stroke::new(1.6, t.band_text);
    let points = if collapsed {
        [egui::pos2(c.x - 4.5, c.y - 2.0), egui::pos2(c.x, c.y + 2.5), egui::pos2(c.x + 4.5, c.y - 2.0)]
    } else {
        [egui::pos2(c.x - 4.5, c.y + 2.0), egui::pos2(c.x, c.y - 2.5), egui::pos2(c.x + 4.5, c.y + 2.0)]
    };
    ui.painter().add(egui::Shape::line(points.to_vec(), stroke));
    let label = if collapsed { crate::tl_id!("ui.show_the_ribbon_ctrl_f1") } else { crate::tl_id!("ui.collapse_the_ribbon_ctrl_f1") }.to_string();
    let enabled = ui.is_enabled();
    let name = label.clone();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    resp.on_hover_text(label)
}

/// The name of the first tab, which opens the File screen instead of a ribbon.
pub const FILE_TAB: &str = "File";

/// One group of the ribbon: its controls, a line on the right, and its name underneath.
// i18n-ui-text: title
fn group(ui: &mut Ui, title: &str, t: &Tokens, add: impl FnOnce(&mut Ui)) {
    let display_title = crate::tl!(title);
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
        display_title,
        egui::FontId::proportional(11.5),
        t.muted,
    );
    // The name is painted, so it is no widget by itself and a screen reader would skip
    // straight from one group's buttons to the next with nothing to say where it is. It is
    // given its name here, the way `bidi::show` gives a painted line its words.
    let named = ui.interact(inner, ui.id().with(("group", title)), Sense::hover());
    let (enabled, label) = (ui.is_enabled(), display_title.to_owned());
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
// i18n-ui-text: label,tip
fn big(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, false, false, t)
}

/// The same, with the small chevron Office puts on a button that opens something.
// i18n-ui-text: label,tip
fn big_menu(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, true, false, t)
}

fn pictures_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let response = big_inner(ui, Icon::Picture, crate::tl_id!("ui.pictures"), crate::tl_id!("ui.choose_how_to_insert_a_picture"), true, false, t);
    let arrow_rect = Rect::from_min_max(egui::pos2(response.rect.left(), response.rect.bottom() - 28.0), response.rect.right_bottom());
    let arrow = ui.interact(arrow_rect, ui.id().with("pictures-menu-arrow"), Sense::click());
    if arrow.hovered() {
        ui.painter().rect_filled(arrow_rect, 3.0, t.row_hover);
        chevron(&ui.painter_at(arrow_rect), arrow_rect.center(), t.text);
    }
    egui::Popup::menu(&arrow).show(|ui| {
        egui::Frame::popup(ui.style()).corner_radius(8.0).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_width(228.0);
            if picture_menu_row(ui, Icon::Picture, crate::tl_id!("ui.picture_from_a_file"), t, true).clicked() {
                acts.push(Act::AddPicture);
                ui.close();
            }
            let _ = picture_menu_row(ui, Icon::Attach, crate::tl_id!("ui.from_camera"), t, false);
            let _ = picture_menu_row(ui, Icon::Link, crate::tl_id!("ui.from_online"), t, false);
        });
    });
    response.clicked() && !arrow.clicked()
}

// i18n-ui-text: label
fn picture_menu_row(ui: &mut Ui, icon: Icon, label: &str, t: &Tokens, enabled: bool) -> egui::Response {
    let label = crate::tl!(label);
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

// i18n-ui-text: label,tip
fn big_inner(ui: &mut Ui, icon: Icon, label: &str, tip: &str, menu: bool, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let lines: Vec<&str> = label.split('\n').collect();
    let font = egui::FontId::proportional(12.0);
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
// i18n-ui-text: label,tip
fn big_on(ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    big_inner(ui, icon, label, tip, false, on, t)
}

// i18n-ui-text: letter,label,tip
fn small(ui: &mut Ui, icon: Option<Icon>, letter: &str, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let letter = crate::tl!(letter);
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
// i18n-ui-text: label,tip
fn small_labeled_icon(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
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
    let label = crate::tl!(tag.label());
    match tag.shortcut() {
        Some(keys) => format!("{label} ({keys})"),
        None => label.to_string(),
    }
}

fn home_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    // Undo comes first, as it does in OneNote's own Home tab: it is the button a writer
    // reaches for in a hurry, and a page editor without one is not safe to write in.
    group(ui, crate::tl_id!("ui.undo"), t, |ui| {
        rows(ui, |ui| {
            if row_button(ui, Icon::Undo, crate::tl_id!("ui.undo"), crate::tl_id!("ui.ctrl_z_takes_back_the_last_change"), view.can_undo, t).clicked()
            {
                acts.push(Act::Undo);
            }
            if row_button(ui, Icon::Redo, crate::tl_id!("ui.redo"), crate::tl_id!("ui.ctrl_y_puts_it_back"), view.can_redo, t).clicked() {
                acts.push(Act::Redo);
            }
        });
    });
    group(ui, crate::tl_id!("ui.clipboard"), t, |ui| {
        if big_mark(ui, Mark::Paste, crate::tl_id!("ui.paste"), crate::tl_id!("ui.ctrl_v_pastes_into_the_page"), t).clicked() {
            acts.push(Act::Paste);
        }
        rows(ui, |ui| {
            if mark_row(ui, Mark::Cut, crate::tl_id!("ui.cut"), crate::tl_id!("ui.cut_the_selection"), t).clicked() {
                acts.push(Act::Clipboard("cut"));
            }
            if mark_row(ui, Mark::Copy, crate::tl_id!("ui.copy"), crate::tl_id!("ui.copy_the_selection"), t).clicked() {
                acts.push(Act::Clipboard("copy"));
            }
            // Click to copy the formatting at the caret to the next words picked; double-click to
            // keep painting until Escape (issue #165).
            let painter =
                mark_row_on(ui, Mark::Painter, crate::tl_id!("ui.format_painter"), crate::tl_id!("ui.format_painter_tip"), view.painting, t);
            if painter.double_clicked() {
                acts.push(Act::FormatPainter(true));
            } else if painter.clicked() {
                acts.push(Act::FormatPainter(false));
            }
        });
    });
    group(ui, crate::tl_id!("ui.basic_text"), t, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                // The font name: the same combo box as the size, listing the installed fonts.
                // The font list is read in the background. While it is not ready the list says so,
                // and the window asks to be drawn again until it is.
                let (families, families_note): (Vec<String>, Option<String>) = {
                    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
                    {
                        match crate::fonts::catalog_if_ready() {
                            Some(Ok(catalog)) => (
                                catalog.families().iter().filter(|name| notecraft_formats::markdown::valid_font_family(name)).cloned().collect(),
                                None,
                            ),
                            Some(Err(error)) => {
                                log::warn!("Could not discover installed fonts: {error}");
                                (Vec::new(), Some(crate::tl_id!("ui.font_catalog_unavailable").to_string()))
                            }
                            None => {
                                ui.ctx().request_repaint_after(std::time::Duration::from_millis(150));
                                (Vec::new(), Some(crate::tl_id!("ui.loading_fonts").to_string()))
                            }
                        }
                    }
                    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
                    {
                        (Vec::new(), Some(crate::tl_id!("ui.font_catalog_unavailable").to_string()))
                    }
                };
                let family_out = crate::combo::show(
                    ui,
                    t.text,
                    crate::combo::Spec {
                        id: ui.id().with("font-family"),
                        box_label: crate::tl_id!("ui.font_family_box"),
                        arrow_label: crate::tl_id!("ui.font_family"),
                        shown: view.font_family.unwrap_or(""),
                        width: 96.0,
                        items: &families,
                        empty_note: families_note.as_deref(),
                        accept: &|typed| notecraft_formats::markdown::valid_font_family(typed),
                    },
                );
                if let Some(name) = family_out.chosen {
                    acts.push(Act::FontFamily(name));
                }
                // OneNote's size control: an editable box showing the size, and a drop-down arrow.
                // The box shows the selected size; with no selection it keeps the last size seen.
                let last_id = ui.id().with("font-size-last");
                if let Some(now) = view.font_size.filter(|s| s.parse::<u8>().is_ok()) {
                    ui.data_mut(|d| d.insert_temp(last_id, now.to_owned()));
                }
                let shown_owned: String =
                    view.font_size.map(str::to_owned).or_else(|| ui.data(|d| d.get_temp::<String>(last_id))).unwrap_or_default();
                let shown = shown_owned.as_str();
                let sizes: Vec<String> = FONT_SIZES.iter().map(|size| size.to_string()).collect();
                let size_out = crate::combo::show(
                    ui,
                    t.text,
                    crate::combo::Spec {
                        id: ui.id().with("font-size"),
                        box_label: crate::tl_id!("ui.font_size_box"),
                        arrow_label: crate::tl_id!("ui.font_size"),
                        shown,
                        width: 38.0,
                        items: &sizes,
                        empty_note: None,
                        accept: &|typed| typed.parse::<u8>().is_ok_and(|size| FONT_SIZES.contains(&size)),
                    },
                );
                let boxes = crate::floating::FontBoxes {
                    family: view.font_family.unwrap_or("").to_owned(),
                    size: shown.to_owned(),
                    families: families.clone(),
                    families_note: families_note.clone(),
                    sizes: sizes.clone(),
                };
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(crate::floating::FONT_BOXES_KEY), boxes));
                if let Some(size) = size_out.chosen.and_then(|typed| typed.parse::<u8>().ok()) {
                    acts.push(Act::FontSize(size));
                }
                if mark_button(ui, Mark::Bold, crate::tl_id!("ui.bold"), crate::tl_id!("ui.bold_ctrl_b"), false, t).clicked() {
                    acts.push(Act::Mark("bold"));
                }
                if mark_button(ui, Mark::Italic, crate::tl_id!("ui.italic"), crate::tl_id!("ui.italic_ctrl_i"), false, t).clicked() {
                    acts.push(Act::Mark("italic"));
                }
                if mark_button(ui, Mark::Strike, crate::tl_id!("ui.strikethrough"), crate::tl_id!("ui.strike_through"), false, t).clicked() {
                    acts.push(Act::Mark("strike"));
                }
                if small(ui, Some(Icon::Code), "", crate::tl_id!("ui.code_2"), crate::tl_id!("ui.code_in_one_line"), false, t).clicked() {
                    acts.push(Act::Mark("code"));
                }
                if small(ui, Some(Icon::Link), "", crate::tl_id!("ui.link"), crate::tl_id!("ui.a_link"), false, t).clicked() {
                    acts.push(Act::Insert("link"));
                }
            });
            ui.horizontal(|ui| {
                if small(ui, Some(Icon::ListBullet), "", crate::tl_id!("ui.bullets"), crate::tl_id!("ui.a_bullet_list"), false, t).clicked() {
                    acts.push(Act::Block("bullet"));
                }
                if small(ui, Some(Icon::ListNumber), "", crate::tl_id!("ui.numbering"), crate::tl_id!("ui.a_numbered_list"), false, t).clicked() {
                    acts.push(Act::Block("number"));
                }
                if mark_button(
                    ui,
                    Mark::IndentLess,
                    crate::tl_id!("ui.decrease_indentation"),
                    crate::tl_id!("ui.decrease_indentation_shift_tab"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::Indent(false));
                }
                if mark_button(ui, Mark::IndentMore, crate::tl_id!("ui.increase_indentation"), crate::tl_id!("ui.increase_indentation_tab"), false, t)
                    .clicked()
                {
                    acts.push(Act::Indent(true));
                }
                align_menu(ui, t, acts);
                if small(ui, Some(Icon::Quote), "", crate::tl_id!("ui.quote_2"), crate::tl_id!("ui.quote_this_paragraph"), false, t).clicked() {
                    acts.push(Act::Block("quote"));
                }
                if mark_button(ui, Mark::ClearFormat, crate::tl_id!("ui.clear_formatting_2"), crate::tl_id!("ui.take_the_marks_off"), false, t)
                    .clicked()
                {
                    acts.push(Act::ClearFormatting);
                }
            });
            ui.horizontal(|ui| {
                if mark_button(ui, Mark::Underline, crate::tl_id!("ui.underline"), crate::tl_id!("ui.underline"), false, t).clicked() {
                    acts.push(Act::Mark("underline"));
                }
                // i18n-exempt: x2 is the compact mathematical symbol for subscript.
                if small(ui, None, "x2", crate::tl_id!("ui.subscript"), crate::tl_id!("ui.subscript"), false, t).clicked() {
                    acts.push(Act::Mark("subscript"));
                }
                // i18n-exempt: x^2 is the compact mathematical symbol for superscript.
                if small(ui, None, "x^2", crate::tl_id!("ui.superscript"), crate::tl_id!("ui.superscript"), false, t).clicked() {
                    acts.push(Act::Mark("superscript"));
                }
            });
        });
    });
    // Page direction is NoteCraft's own: OneNote has no such thing, and a notebook written
    // in Arabic wants one. It stays visible in the ribbon as page setup rather than being
    // hidden in a Word-style dialog. Paragraph direction remains available beside it,
    // while the alignment menu remains the primary place for paragraph alignment and
    // direction.
    group(ui, crate::tl_id!("ui.page_setup"), t, |ui| {
        rows(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::tl_id!("ui.page")).size(11.5).color(t.muted));
                if small(
                    ui,
                    Some(Icon::DirLtr),
                    "",
                    crate::tl_id!("ui.page_left_to_right"),
                    crate::tl_id!("ui.the_whole_page_reads_left_to_right"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::PageDirection(Some(notecraft_model::Dir::Ltr)));
                }
                if small(
                    ui,
                    Some(Icon::DirRtl),
                    "",
                    crate::tl_id!("ui.page_right_to_left"),
                    crate::tl_id!("ui.the_whole_page_reads_right_to_left_arabic_hebrew"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::PageDirection(Some(notecraft_model::Dir::Rtl)));
                }
                if small(
                    ui,
                    Some(Icon::DirAuto),
                    crate::tl_id!("ui.auto"),
                    crate::tl_id!("ui.page_direction_from_the_words"),
                    crate::tl_id!("ui.every_paragraph_decides_for_itself"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::PageDirection(None));
                }
            });
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(crate::tl_id!("ui.para")).size(11.5).color(t.muted));
                if small(
                    ui,
                    Some(Icon::ParaLtr),
                    "",
                    crate::tl_id!("ui.paragraph_left_to_right"),
                    crate::tl_id!("ui.this_paragraph_reads_left_to_right"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::Direction(Some(notecraft_model::Dir::Ltr)));
                }
                if small(
                    ui,
                    Some(Icon::ParaRtl),
                    "",
                    crate::tl_id!("ui.paragraph_right_to_left"),
                    crate::tl_id!("ui.this_paragraph_reads_right_to_left_arabic_hebrew"),
                    false,
                    t,
                )
                .clicked()
                {
                    acts.push(Act::Direction(Some(notecraft_model::Dir::Rtl)));
                }
                if small(
                    ui,
                    Some(Icon::ParaAuto),
                    crate::tl_id!("ui.auto"),
                    crate::tl_id!("ui.paragraph_direction_from_the_words"),
                    crate::tl_id!("ui.this_paragraph_decides_from_its_own_first_letter"),
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
    group(ui, crate::tl_id!("ui.styles"), t, |ui| {
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
    group(ui, crate::tl_id!("ui.tags"), t, |ui| {
        gallery(ui, "tags", "", t, acts, |ui, acts| {
            for tag in [Tag::ToDo, Tag::Important] {
                if tag_row(ui, tag, t).clicked() {
                    acts.push(Act::Tag(tag));
                }
            }
        });
        if big(ui, Icon::Todo, crate::tl_id!("ui.to_do_tag_2"), crate::tl_id!("ui.a_to_do_box_on_this_line_ctrl_1"), t).clicked() {
            acts.push(Act::Block("todo"));
        }
        if big(ui, Icon::Search, crate::tl_id!("ui.find_tags_3"), crate::tl_id!("ui.not_built"), t).clicked() {
            acts.push(Act::NotBuilt("Find Tags"));
        }
    });
    group(ui, crate::tl_id!("ui.email"), t, |ui| {
        if big(ui, Icon::Attach, crate::tl_id!("ui.send_page_3"), crate::tl_id!("ui.not_built"), t).clicked() {
            acts.push(Act::NotBuilt("Send Page"));
        }
    });
    group(ui, crate::tl_id!("ui.meetings"), t, |ui| {
        if big(ui, Icon::Page, crate::tl_id!("ui.meeting_details_3"), crate::tl_id!("ui.not_built"), t).clicked() {
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
    let name = if which == crate::tl_id!("ui.styles_2") { crate::tl_id!("ui.more_styles") } else { crate::tl_id!("ui.more_tags") };
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
                        if ui.button(crate::tl_id!("ui.clear_formatting_2")).clicked() {
                            acts.push(Act::ClearFormatting);
                            picked = true;
                        }
                    } else {
                        ui.label(egui::RichText::new(crate::tl_id!("ui.tags")).strong());
                        for tag in Tag::ALL {
                            if tag_row(ui, tag, t).clicked() {
                                acts.push(Act::Tag(tag));
                                picked = true;
                            }
                        }
                        ui.separator();
                        ui.add_enabled(false, egui::Label::new(egui::RichText::new(crate::tl_id!("ui.customize_tags_not_built")).color(t.muted)));
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
    let resp = big_menu(ui, Icon::Equation, crate::tl_id!("ui.equation"), crate::tl_id!("ui.an_equation_written_in_tex_pick_one_to_start_from"), t);
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
                                ui.add(egui::Label::new(egui::RichText::new(crate::tl!(name)).color(t.text)).selectable(false));
                            });
                            crate::mathview::inline(ui, tex, 15.0, t.text);
                        });
                        let row = ui.interact(inner.response.rect, open_id.with(("row", *name)), Sense::click());
                        if row.hovered() {
                            ui.painter().rect_filled(inner.response.rect.expand(2.0), 3.0, t.row_hover.gamma_multiply(0.6));
                        }
                        let label = crate::i18n::format_message_id("ui.equation_name", &[("name", crate::tl!(name))]);
                        let (enabled, label) = (ui.is_enabled(), label);
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
    let resp = small(
        ui,
        Some(Icon::ParaLtr),
        "",
        crate::tl_id!("ui.alignment_and_direction"),
        crate::tl_id!("ui.alignment_and_which_way_this_paragraph_reads"),
        false,
        t,
    );
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
                    for (icon, label, what) in [
                        (Icon::AlignLeft, "Align Left", "Align left"),
                        (Icon::AlignCenter, "Center", "Centring"),
                        (Icon::AlignRight, "Align Right", "Align right"),
                    ] {
                        if floating::menu_row(ui, Some(Glyph::Icon(icon)), label, t).clicked() {
                            acts.push(Act::NotBuilt(what));
                            picked = true;
                        }
                    }
                    // Word's three, under the names Word gives them, so a writer of Arabic
                    // meets what they already know. They decide how many kashidas a line
                    // may take; `docs/rtl-in-word-and-onenote.md` has the comparison.
                    for (strength, icon, label) in [
                        (notecraft_model::justify::Strength::Low, Icon::JustifyLow, "Justify Low"),
                        (notecraft_model::justify::Strength::Medium, Icon::JustifyMedium, "Justify Medium"),
                        (notecraft_model::justify::Strength::High, Icon::JustifyHigh, "Justify High"),
                    ] {
                        if floating::menu_row(ui, Some(Glyph::Icon(icon)), label, t).clicked() {
                            acts.push(Act::Justify(strength));
                            picked = true;
                        }
                    }
                    ui.separator();
                    if direction_row(ui, Some(Icon::ParaLtr), crate::tl_id!("ui.left_to_right"), t).clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Ltr)));
                        picked = true;
                    }
                    if direction_row(ui, Some(Icon::ParaRtl), crate::tl_id!("ui.right_to_left"), t).clicked() {
                        acts.push(Act::Direction(Some(notecraft_model::Dir::Rtl)));
                        picked = true;
                    }
                    if direction_row(ui, None, crate::tl_id!("ui.direction_from_the_words"), t).clicked() {
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
// i18n-ui-text: label
fn direction_row(ui: &mut Ui, icon: Option<Icon>, label: &str, t: &Tokens) -> egui::Response {
    floating::menu_row(ui, icon.map(Glyph::Icon), crate::tl!(label), t)
}

/// The less common direction for a selected or newly typed run, kept in a compact menu like
/// the paragraph direction entries in OneNote's alignment menu.
fn run_direction_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = small(
        ui,
        None,
        crate::tl_id!("ui.more"),
        crate::tl_id!("ui.text_run_direction"),
        crate::tl_id!("ui.set_the_direction_of_selected_text_or_the_text_typed_next"),
        false,
        t,
    );
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
                    ui.label(egui::RichText::new(crate::tl_id!("ui.selected_text_next_typing")).small().color(t.muted));
                    ui.separator();
                    if direction_row(ui, Some(Icon::CaretLtr), crate::tl_id!("ui.left_to_right"), t).clicked() {
                        acts.push(Act::Embedding(notecraft_model::Dir::Ltr));
                        picked = true;
                    }
                    if direction_row(ui, Some(Icon::CaretRtl), crate::tl_id!("ui.right_to_left"), t).clicked() {
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
// i18n-ui-text: label
fn style_row(ui: &mut Ui, label: &str, kind: &str, current: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
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
    let label = tag_label(tag);
    painter.text(egui::pos2(rect.min.x + 26.0, rect.center().y), Align2::LEFT_CENTER, &label, egui::FontId::proportional(12.5), t.text);
    let enabled = ui.is_enabled();
    let name = label;
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
// i18n-ui-text: label,tip
fn mark_row(ui: &mut Ui, mark: Mark, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    mark_row_on(ui, mark, label, tip, false, t)
}

/// [`mark_row`] that can be lit, for a command that stays on until it is used (Format Painter).
// i18n-ui-text: label,tip
fn mark_row_on(ui: &mut Ui, mark: Mark, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let font = egui::FontId::proportional(11.0);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x + SMALL_W + 8.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if on || resp.hovered() {
        painter.rect_filled(rect, 4.0, if on { t.row_selected } else { t.row_hover });
    }
    let left = Rect::from_center_size(egui::pos2(rect.min.x + 11.0, rect.center().y), vec2(15.0, 15.0));
    ribbon_icons::paint(&painter, left, mark, mark.color(t.text, t.accent));
    painter.text(egui::pos2(rect.min.x + 23.0, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

// i18n-ui-text: label,tip
fn mark_button(ui: &mut Ui, mark: Mark, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
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
    group(ui, crate::tl_id!("ui.insert"), t, |ui| {
        if big(ui, Icon::InsertSpace, crate::tl_id!("ui.insert_space_2"), crate::tl_id!("ui.add_a_blank_paragraph_at_the_caret"), t).clicked() {
            acts.push(Act::Insert("space"));
        }
    });
    group(ui, crate::tl_id!("ui.tables"), t, |ui| {
        table_menu(ui, view, t, acts);
    });
    group(ui, crate::tl_id!("ui.files"), t, |ui| {
        if big(
            ui,
            Icon::Page,
            crate::tl_id!("ui.file_printout_2"),
            crate::tl_id!("ui.pick_a_pdf_or_office_document_and_insert_its_pages_as_pictures"),
            t,
        )
        .clicked()
        {
            acts.push(Act::AddPrintout);
        }
        if big(ui, Icon::Attach, crate::tl_id!("ui.file_attachment_2"), crate::tl_id!("ui.a_file_copied_in_or_linked_where_it_is"), t).clicked() {
            acts.push(Act::AddFile);
        }
        if view.gridcraft {
            if big(
                ui,
                Icon::Spreadsheet,
                crate::tl_id!("ui.spreadsheet"),
                crate::tl_id!("ui.a_new_spreadsheet_beside_the_page_opened_in_gridcraft"),
                t,
            )
            .clicked()
            {
                acts.push(Act::NewGridCraft);
            }
        } else {
            soon(ui, Icon::Spreadsheet, "Spreadsheet", t);
        }
    });
    group(ui, crate::tl_id!("ui.images"), t, |ui| {
        soon(ui, Icon::Clip, "Screen\nClipping", t);
        if pictures_menu(ui, t, acts) {
            acts.push(Act::AddPicture);
        }
    });
    group(ui, crate::tl_id!("ui.media"), t, |ui| {
        soon(ui, Icon::Video, "Online\nVideo", t);
    });
    group(ui, crate::tl_id!("ui.links"), t, |ui| {
        if big(ui, Icon::Link, crate::tl_id!("ui.link"), crate::tl_id!("ui.a_link_to_a_page_or_an_address"), t).clicked() {
            acts.push(Act::Insert("link"));
        }
    });
    group(ui, crate::tl_id!("ui.recording"), t, |ui| {
        soon(ui, Icon::Mic, "Transcribe", t);
        soon(ui, Icon::Video, "Record\nVideo", t);
    });
    group(ui, crate::tl_id!("ui.time_stamp"), t, |ui| {
        if big(ui, Icon::Calendar, crate::tl_id!("ui.date"), crate::tl_id!("ui.today_s_date"), t).clicked() {
            acts.push(Act::Insert("date"));
        }
        if big(ui, Icon::Clock, crate::tl_id!("ui.time"), crate::tl_id!("ui.the_time_now"), t).clicked() {
            acts.push(Act::Insert("time"));
        }
        if big(ui, Icon::Calendar, crate::tl_id!("ui.date_time_2"), crate::tl_id!("ui.both_as_the_page_s_own_stamp_is_written"), t).clicked() {
            acts.push(Act::Insert("datetime"));
        }
    });
    group(ui, crate::tl_id!("ui.pages"), t, |ui| {
        soon(ui, Icon::PageStack, "Page\nTemplates", t);
    });
    group(ui, crate::tl_id!("ui.symbols"), t, |ui| {
        equation_gallery(ui, t, acts);
        if big_menu(ui, Icon::Symbol, crate::tl_id!("ui.symbol"), crate::tl_id!("ui.insert_a_common_symbol"), t).clicked() {
            acts.push(Act::Symbols);
        }
    });
    group(ui, crate::tl_id!("ui.more_2"), t, |ui| {
        // NoteCraft's own addition to OneNote's group: a diagram kept as its source, so it
        // can be read and changed in any editor rather than being a picture of one.
        if big(ui, Icon::Diagram, crate::tl_id!("ui.diagram"), crate::tl_id!("ui.a_mermaid_diagram_kept_as_its_source"), t).clicked() {
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
    let label = r.translated_label();
    let caption = r.translated_mid();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, label));
    let paper = egui::Rect::from_min_size(rect.min, PAPER);
    let painter = ui.painter().with_clip_rect(paper);
    painter.rect_filled(paper, 2.0, t.editor);
    paint_paper(&painter, paper, r, SCALE, t.rule_line, t.rule_line_strong);
    // A page with no lines needs something to look at, so it says so in the middle.
    if !r.on() {
        painter.text(paper.center(), egui::Align2::CENTER_CENTER, crate::tl_id!("ui.plain"), egui::FontId::proportional(12.0), t.muted);
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
    ui.painter().with_clip_rect(rect).text(name, egui::Align2::CENTER_TOP, caption, egui::FontId::proportional(11.0), colour);
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
    let name = colour.map(notecraft_model::NodeColor::label).unwrap_or(crate::tl_id!("ui.no_colour"));
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, name));
    resp.on_hover_text(name)
}

/// Shared by the direct Page colour menu and the combined Format Background gallery.
fn page_colour_palette(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let mut picked = false;
    ui.label(egui::RichText::new(crate::tl_id!("ui.page_colour")).strong().color(t.text));
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
    ui.label(egui::RichText::new(crate::tl_id!("ui.the_paper_and_colour_apply_to_this_page_only")).small().color(t.muted));
    picked
}

/// Table > Shading: a colour behind the caret's cell, or behind every picked cell (issue #169).
/// The palette is the named colours a page or section can carry, washed the way the cell will
/// be drawn, with No Colour first; the colour the cells have now is ringed.
fn shading_menu(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big_menu(ui, Icon::Picture, crate::tl_id!("ui.table_shading"), crate::tl_id!("ui.table_shading_tip"), t);
    let open_id = egui::Id::new("ribbon-table-shading");
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
                ui.label(egui::RichText::new(crate::tl_id!("ui.table_shading")).strong().color(t.text));
                ui.horizontal(|ui| {
                    let now = view.table_shade;
                    if shade_swatch(ui, None, now == Some(None), t).clicked() {
                        acts.push(Act::TableShade(None));
                        picked = true;
                    }
                    for c in notecraft_model::NodeColor::ALL {
                        if shade_swatch(ui, Some(c), now == Some(Some(c)), t).clicked() {
                            acts.push(Act::TableShade(Some(c)));
                            picked = true;
                        }
                    }
                });
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

/// One colour in the Shading palette, drawn as a cell shaded with it would be.
fn shade_swatch(ui: &mut Ui, colour: Option<notecraft_model::NodeColor>, here: bool, t: &Tokens) -> egui::Response {
    let side = 22.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
    let fill = colour.map_or(t.editor, |c| crate::pageedit::cell_shade(t, c));
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, fill);
    let (edge, width) = if here { (t.accent, 2.0) } else { (t.faint, 1.0) };
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(width, edge), egui::StrokeKind::Inside);
    if colour.is_none() {
        painter.text(rect.center(), egui::Align2::CENTER_CENTER, "/", egui::FontId::proportional(13.0), t.muted);
    }
    let name = colour.map(notecraft_model::NodeColor::label).unwrap_or(crate::tl_id!("ui.no_colour"));
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

    let resp = big_menu(ui, Icon::Table, crate::tl_id!("ui.table_3"), crate::tl_id!("ui.a_table_pick_its_size"), t);
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
                        let name = crate::i18n::format_message_id(
                            "ui.columns_x_rows_table",
                            &[("columns", &(c + 1).to_string()), ("rows", &(r + 1).to_string())],
                        );
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
                    Some((r, c)) => crate::i18n::format_message_id(
                        "ui.columns_x_rows_table",
                        &[("columns", &(c + 1).to_string()), ("rows", &(r + 1).to_string())],
                    ),
                    None => crate::tl_id!("ui.table_3").to_string(),
                };
                paint.text(naming.left_top(), egui::Align2::LEFT_TOP, title, egui::FontId::proportional(13.0), t.text);
                if let Some((r, c)) = chose {
                    acts.push(Act::InsertTable(r + 1, c + 1));
                    picked = true;
                }
                ui.separator();
                ui.add_enabled(false, egui::Label::new(egui::RichText::new(crate::tl_id!("ui.insert_table_not_built")).color(t.muted)));
                if view.gridcraft {
                    if floating::menu_row(ui, Some(Glyph::Icon(Icon::Spreadsheet)), crate::tl_id!("ui.new_gridcraft_spreadsheet"), t).clicked() {
                        acts.push(Act::NewGridCraft);
                        picked = true;
                    }
                } else {
                    ui.add_enabled(
                        false,
                        egui::Label::new(egui::RichText::new(crate::tl_id!("ui.new_gridcraft_spreadsheet_not_installed")).color(t.muted)),
                    );
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
    let resp = big_inner(
        ui,
        Icon::Ruler,
        crate::tl_id!("ui.format_background_2"),
        crate::tl_id!("ui.lines_or_squares_behind_the_page_as_on_paper"),
        true,
        open,
        t,
    );
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
                    ui.label(
                        egui::RichText::new(crate::tl_id!("ui.every_fifth_line_is_slightly_stronger_for_easier_counting")).small().color(t.muted),
                    );
                    ui.separator();
                    picked |= page_colour_palette(ui, view, t, acts);
                    ui.separator();
                    ui.add_enabled(false, egui::Label::new(egui::RichText::new(crate::tl_id!("ui.rule_line_colour_not_built")).color(t.muted)));
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
fn sensitivity(ui: &mut Ui, now: u8, pressure_available: bool, t: &Tokens, acts: &mut Vec<Act>) {
    rows(ui, |ui| {
        ui.add_space(4.0);
        let label = if pressure_available { "Sensitivity" } else { "Pressure off" };
        ui.label(egui::RichText::new(label).size(10.0).color(if pressure_available { t.muted } else { t.warn }));
        let (rect, resp) = ui.allocate_exact_size(vec2(96.0, 20.0), Sense::click_and_drag());
        let share = f32::from(now.min(100)) / 100.0;
        let painter = ui.painter_at(rect);
        let track = rect.shrink2(vec2(6.0, 3.0));
        let accent = if pressure_available { t.accent } else { t.muted };
        // The wedge: nothing at the left, full at the right.
        let wedge = vec![egui::pos2(track.min.x, track.max.y), egui::pos2(track.max.x, track.max.y), egui::pos2(track.max.x, track.min.y)];
        painter.add(egui::Shape::convex_polygon(wedge, crate::theme::mix(t.panel, t.text, 0.14), egui::Stroke::NONE));
        // The part that is turned on is filled in the accent, cut at the handle.
        let x = track.min.x + track.width() * share;
        let h = track.max.y - track.height() * share;
        painter.add(egui::Shape::convex_polygon(
            vec![egui::pos2(track.min.x, track.max.y), egui::pos2(x, track.max.y), egui::pos2(x, h)],
            accent,
            egui::Stroke::NONE,
        ));
        painter.circle(egui::pos2(x, rect.center().y), 5.0, t.panel, egui::Stroke::new(1.4, accent));

        if pressure_available
            && (resp.dragged() || resp.clicked())
            && let Some(at) = resp.interact_pointer_pos()
        {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let picked = (((at.x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0) * 100.0).round() as u8;
            if picked != now {
                acts.push(Act::InkSensitivity(picked));
            }
        }
        let enabled = pressure_available && ui.is_enabled();
        let said = if pressure_available {
            crate::i18n::format_message_id("ui.pressure_sensitivity_now_of_100", &[("now", &now.to_string())])
        } else {
            crate::tl_id!("ui.pressure_sensitivity_unavailable").to_owned()
        };
        resp.widget_info(move || egui::WidgetInfo::slider(enabled, f64::from(now), &said));
        let tip = if pressure_available {
            "How readily the pen answers pressure: turned up, a light hand lays down more ink"
        } else {
            "No pen pressure readings have reached this window yet. Use a pressure-capable pen with Windows Ink enabled."
        };
        resp.on_hover_text(crate::tl!(tip));
        ui.label(
            egui::RichText::new(if pressure_available { format!("{now}") } else { crate::tl_id!("ui.off").to_owned() })
                .size(11.5)
                .color(if pressure_available { t.text } else { t.warn }),
        );
    });
}

/// OneNote's Add button: a new tool of whatever kind, put on the end of the rack.
fn add_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big_menu(ui, Icon::Plus, crate::tl_id!("ui.add"), crate::tl_id!("ui.put_another_tool_on_the_rack"), t);
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
                            crate::tl!(tool.label()),
                            egui::FontId::proportional(12.5),
                            t.text,
                        );
                        let enabled = ui.is_enabled();
                        let tool_label = crate::tl!(tool.label());
                        let name = crate::i18n::format_message_id("ui.add_tool", &[("tool", tool_label)]);
                        hit.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
                        if hit.clicked() {
                            acts.push(Act::InkRack(RackEdit::Add(tool)));
                            picked = true;
                        }
                    }
                    ui.separator();
                    not_built(ui, crate::tl_id!("ui.laser_pointer"), t);
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
    let name =
        crate::i18n::format_message_id("ui.tool_colour", &[("tool", crate::tl!(nib.tool.label())), ("colour", crate::tl!(swatch_name(nib.colour)))]);
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
        if ui.button(crate::tl_id!("ui.modify_tool")).clicked() {
            modify = true;
            ui.close();
        }
        ui.separator();
        if ui.add_enabled(i > 0, egui::Button::new(crate::tl_id!("ui.move_left"))).clicked() {
            acts.push(Act::InkRack(RackEdit::Move(i, true)));
            ui.close();
        }
        if ui.add_enabled(i + 1 < rack_len, egui::Button::new(crate::tl_id!("ui.move_right"))).clicked() {
            acts.push(Act::InkRack(RackEdit::Move(i, false)));
            ui.close();
        }
        ui.separator();
        let tool = crate::tl!(nib.tool.label());
        let label = crate::i18n::format_message_id("ui.add_another_tool", &[("tool", tool)]);
        if ui.button(label).clicked() {
            acts.push(Act::InkRack(RackEdit::Add(nib.tool)));
            ui.close();
        }
        let label = crate::i18n::format_message_id("ui.delete_tool_2", &[("tool", crate::tl!(nib.tool.label()))]);
        if ui.add_enabled(rack_len > 1, egui::Button::new(label)).clicked() {
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
    let more_colours_id = id.with("more-colours-open");
    let mut more_colours = ui.data(|d| d.get_temp::<bool>(more_colours_id)).unwrap_or(false);
    let area = egui::Area::new(id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(under.left_bottom())
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                const POPUP_WIDTH: f32 = 196.0;
                ui.set_min_width(POPUP_WIDTH);
                // The stroke this tool makes, drawn by the code that draws the page's ink,
                // so the sample cannot say one thing and the page another.
                let (sample, _) = ui.allocate_exact_size(vec2(180.0, 26.0), Sense::hover());
                let mut stroke = notecraft_ink::Stroke::new(nib.tool, nib.colour, nib.width());
                for (at, press) in [(0.04, 0.12), (0.3, 0.55), (0.62, 1.0), (0.96, 0.3)] {
                    stroke.dabs.push(notecraft_ink::Dab::new(sample.width() * at, 13.0 + (at * 9.0).sin() * 3.0, press));
                }
                crate::inkpad::paint_stroke(&ui.painter_at(sample), sample.min, &stroke);
                ui.separator();

                ui.label(egui::RichText::new(crate::tl_id!("ui.thickness")).size(11.5).color(t.muted));
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
                        let size = format!("{:.1}", f32::from(tenths) / 10.0);
                        let name = crate::i18n::format_message_id("ui.size_pt", &[("size", &size)]);
                        let tip = name.clone();
                        resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, here, name.clone()));
                        if resp.on_hover_text(tip).clicked() {
                            acts.push(Act::InkRack(RackEdit::Set(i, Nib::new(nib.tool, nib.colour, tenths))));
                        }
                    }
                });

                ui.add_space(2.0);
                ui.label(egui::RichText::new(crate::tl_id!("ui.colours")).size(11.5).color(t.muted));
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
                            let name = crate::i18n::format_message_id(
                                "ui.colour_hex",
                                &[("colour", crate::tl!(swatch_name(colour))), ("hex", &colour.hex())],
                            );
                            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, here, name.clone()));
                            if resp.on_hover_text(colour.hex()).clicked() {
                                acts.push(Act::InkRack(RackEdit::Set(i, Nib::new(nib.tool, colour, nib.tenths))));
                            }
                        }
                    });
                }
                ui.separator();
                if popup_action_button(ui, crate::tl_id!("ui.more_colours"), true, t.accent, crate::tl_id!("ui.choose_a_custom_ink_colour")).clicked()
                {
                    more_colours = !more_colours;
                }
                if more_colours {
                    let mut colour = egui::Color32::from_rgb(nib.colour.0, nib.colour.1, nib.colour.2);
                    if egui::color_picker::color_picker_color32(ui, &mut colour, egui::color_picker::Alpha::Opaque) {
                        acts.push(Act::InkRack(RackEdit::Set(
                            i,
                            Nib::new(nib.tool, notecraft_ink::Rgb(colour.r(), colour.g(), colour.b()), nib.tenths),
                        )));
                        more_colours = false;
                        done = true;
                    }
                }
                popup_action_button(ui, crate::tl_id!("ui.eyedropper"), false, t.accent, crate::tl_id!("ui.eyedropper_not_built"));
                if popup_action_button(ui, crate::tl_id!("ui.delete_tool"), rack_len > 1, t.warn, crate::tl_id!("ui.remove_this_tool_from_the_rack"))
                    .clicked()
                {
                    acts.push(Act::InkRack(RackEdit::Remove(i)));
                    done = true;
                }
            });
        })
        .response;
    ui.data_mut(|d| d.insert_temp(more_colours_id, more_colours));
    let at = ui.ctx().pointer_latest_pos().unwrap_or_default();
    let outside = ui.input(|i| i.pointer.any_click()) && !area.rect.contains(at) && !under.contains(at);
    done || outside || ui.input(|i| i.key_pressed(egui::Key::Escape))
}

/// The eraser, racked beside the pens because that is where a hand reaches for it, and
/// drawn like them: a pale sleeve a hand holds, and the worn rubber below it.
fn eraser_on_rack(ui: &mut Ui, chosen: bool, t: &Tokens) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(27.0, GROUP_H - 20.0), Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, chosen, crate::tl_id!("ui.eraser")));
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
    resp.on_hover_text(crate::tl_id!("ui.rub_out_whatever_the_pointer_passes_over"))
}

/// Draws one drawing tool upright inside `rect`, the way OneNote draws its rack.
///
/// The rack shares a visual language, but each tool must be recognisable before its colour is
/// read: the marker stays broad and squared, the pencil shows wood and lead, the fountain pen
/// shows a metal nib and breather hole, the brush has a flexible bristle point, and the pen has
/// a small ball tip. Keeping these silhouettes distinct prevents the rack from reading as a row
/// of crayons while preserving the marker treatment users already recognise.
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
            // A compact hexagonal coloured pencil: the writing colour is the lacquered body,
            // but the wood and graphite make it unmistakably different from a marker.
            let (w, lw) = (4.7, 4.1);
            let wood = foot - 13.0;
            let lacquer = crate::theme::mix(ink, egui::Color32::WHITE, 0.08);
            painter.add(shape(vec![at(-w, top), at(w, top), at(lw, wood), at(-lw, wood)], lacquer, darker(ink, 0.42)));
            painter.line_segment([at(-w * 0.42, top + 3.0), at(-lw * 0.42, wood - 2.0)], egui::Stroke::new(1.0, shine));
            painter.line_segment([at(-w, top + 7.0), at(w, top + 7.0)], egui::Stroke::new(1.5, collar));
            painter.add(shape(
                vec![at(-lw, wood), at(lw, wood), at(1.35, foot - 3.2), at(-1.35, foot - 3.2)],
                egui::Color32::from_rgb(0xe8, 0xd0, 0xa6),
                egui::Color32::from_rgb(0xb2, 0x95, 0x66),
            ));
            painter.add(shape(
                vec![at(-1.35, foot - 3.2), at(1.35, foot - 3.2), at(0.0, foot)],
                egui::Color32::from_gray(60),
                egui::Color32::from_gray(40),
            ));
        }
        Tool::FountainPen => {
            // A fountain pen has a shorter barrel and a broad metal nib, unlike the small
            // ball tip of the ordinary pen.
            let (w, lw) = (4.8, 4.4);
            let grip = top + 8.0;
            let nib = foot - 14.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-4.5, grip), at(4.5, grip), at(3.9, nib), at(-3.9, nib)], darker(ink, 0.08), darker(ink, 0.48)));
            painter.line_segment([at(-4.1, grip + 3.0), at(4.1, grip + 3.0)], egui::Stroke::new(1.8, collar));
            // A broad split nib with a visible slit and breather hole.
            painter.add(shape(
                vec![at(-3.9, nib), at(3.9, nib), at(1.3, foot), at(-1.3, foot)],
                egui::Color32::from_rgb(0xc8, 0xcc, 0xd2),
                body_edge,
            ));
            painter.line_segment([at(0.0, nib + 1.0), at(0.0, foot - 1.0)], egui::Stroke::new(0.8, egui::Color32::from_gray(75)));
            painter.circle_filled(at(0.0, nib + 3.4), 1.15, egui::Color32::from_gray(75));
        }
        Tool::BrushPen => {
            // A loaded brush has a soft ferrule and a visibly flexible, tapered bristle bundle.
            let (w, lw) = (5.8, 5.2);
            let grip = top + 8.0;
            let ferrule = foot - 15.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-5.4, grip), at(5.4, grip), at(4.8, ferrule), at(-4.8, ferrule)], darker(ink, 0.12), darker(ink, 0.5)));
            painter.line_segment([at(-3.7, ferrule), at(3.7, ferrule)], egui::Stroke::new(1.8, collar));
            painter.add(shape(
                vec![at(-4.8, ferrule), at(4.8, ferrule), at(2.4, ferrule + 7.0), at(0.55, foot), at(-0.55, foot), at(-2.4, ferrule + 7.0)],
                darker(ink, 0.08),
                darker(ink, 0.5),
            ));
            for offset in [-2.0, 0.0, 2.0] {
                painter.line_segment([at(offset * 0.72, ferrule + 1.5), at(offset * 0.18, foot - 1.0)], egui::Stroke::new(0.55, shine));
            }
        }
        Tool::Pen => {
            // The ordinary pen is the clean, slim ballpoint: narrow barrel, small metal cone,
            // and a round tip rather than the broad point shared by the other tools.
            let (w, lw) = (4.2, 3.8);
            let grip = top + 9.0;
            let nib = foot - 12.0;
            hold(painter, w, lw, grip);
            painter.add(shape(vec![at(-3.9, grip), at(3.9, grip), at(3.5, nib), at(-3.5, nib)], ink, darker(ink, 0.48)));
            painter.line_segment([at(-3.4, grip + 3.0), at(3.4, grip + 3.0)], egui::Stroke::new(1.0, shine));
            painter.add(shape(
                vec![at(-3.5, nib), at(3.5, nib), at(0.65, foot - 2.2), at(-0.65, foot - 2.2)],
                egui::Color32::from_rgb(0xb9, 0xbd, 0xc5),
                body_edge,
            ));
            painter.circle_filled(at(0.0, foot - 1.0), 0.8, egui::Color32::from_gray(55));
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
// i18n-ui-text: label,tip
fn row_button(ui: &mut Ui, icon: Icon, label: &str, tip: &str, live: bool, t: &Tokens) -> egui::Response {
    let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
    let resp = ui.add_enabled_ui(live, |ui| small(ui, Some(icon), "", label, tip, false, if live { t } else { &dim })).inner;
    if live {
        resp
    } else {
        let label = crate::tl!(label);
        let action = label.to_lowercase();
        resp.on_hover_text(crate::i18n::format_message_id("ui.label_nothing_to_action", &[("label", label), ("action", &action)]))
    }
}

/// OneNote's Shapes group: a row of regular shapes, and the one in hand is lit.
///
/// A shape drawn here is ink like any other stroke, so the eraser, Select and Undo Stroke
/// all work on it the day it is drawn.
fn shapes_group(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    use notecraft_ink::shapes::Shape;
    group(ui, crate::tl_id!("ui.shapes"), t, |ui| {
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
    let shape_label = crate::tl!(shape.label());
    ui.painter().text(at, egui::Align2::LEFT_CENTER, shape_label, egui::FontId::proportional(11.5), t.text);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, shape_label));
    resp.on_hover_text(crate::i18n::format_message_id("ui.shape_drag_on_the_page_to_draw_one", &[(crate::tl_id!("ui.shape"), shape_label)]))
}

fn soon(ui: &mut Ui, icon: Icon, label: &str, t: &Tokens) {
    let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
    let resp = ui.add_enabled_ui(false, |ui| big(ui, icon, label, crate::tl_id!("ui.not_built_yet"), &dim)).inner;
    resp.on_hover_text(crate::i18n::format_message_id("ui.label_not_built_yet", &[(crate::tl_id!("ui.label"), crate::tl!(label))]));
}

/// OneNote's History tab, with the same groups and controls. NoteCraft does not yet keep
/// unread flags or authors, so those controls are visible but disabled.
fn history_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, crate::tl_id!("ui.unread"), t, |ui| {
        soon(ui, Icon::PageStack, "Next\nUnread", t);
        soon(ui, Icon::Todo, "Mark as\nRead", t);
    });
    group(ui, crate::tl_id!("ui.authors"), t, |ui| {
        soon(ui, Icon::Clock, "Recent\nEdits", t);
        soon(ui, Icon::Search, "Find by\nAuthor", t);
        soon(ui, Icon::Page, "Hide\nAuthors", t);
    });
    group(ui, crate::tl_id!("ui.history"), t, |ui| {
        if ui
            .add_enabled_ui(view.has_page, |ui| {
                big(ui, Icon::PageStack, crate::tl_id!("ui.page_versions_4"), crate::tl_id!("ui.see_and_restore_saved_copies_of_this_page"), t)
            })
            .inner
            .clicked()
        {
            acts.push(Act::PageVersions);
        }
        if ui
            .add_enabled_ui(view.has_page, |ui| {
                big(
                    ui,
                    Icon::Notebook,
                    crate::tl_id!("ui.notebook_recycle_bin_3"),
                    crate::tl_id!("ui.restore_pages_sections_and_groups_removed_from_this_notebook"),
                    t,
                )
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
    group(ui, crate::tl_id!("ui.undo"), t, |ui| {
        // Named apart from the group they sit in, so a screen reader and a test can tell
        // the Undo group from the Undo button inside it.
        if big(ui, Icon::Undo, crate::tl_id!("ui.undo_stroke"), crate::tl_id!("ui.take_the_last_stroke_back"), t).clicked() {
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
    group(ui, crate::tl_id!("ui.selection"), t, |ui| {
        if big_on(
            ui,
            Icon::Pointer,
            crate::tl_id!("ui.select"),
            crate::tl_id!("ui.click_a_stroke_to_pick_it_up_then_move_stretch_or_delete_it"),
            view.pointer == Pointer::Pick,
            t,
        )
        .clicked()
        {
            acts.push(Act::InkPointer(Pointer::Pick));
        }
        if big_on(
            ui,
            Icon::Lasso,
            crate::tl_id!("ui.lasso_select"),
            crate::tl_id!("ui.draw_a_loop_round_ink_to_pick_it_up"),
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
        if big_on(
            ui,
            Icon::DirLtr,
            crate::tl_id!("ui.type"),
            crate::tl_id!("ui.put_the_pointer_back_to_the_words_and_the_caret"),
            view.pointer == Pointer::Write,
            t,
        )
        .clicked()
        {
            acts.push(Act::InkPointer(Pointer::Write));
        }
    });
    group(ui, crate::tl_id!("ui.drawing_tools"), t, |ui| {
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
        sensitivity(ui, view.sensitivity, view.pressure_available, t, acts);
    });
    group(ui, crate::tl_id!("ui.input_mode"), t, |ui| {
        soon(
            ui,
            Icon::Hand,
            "Panning
Hand",
            t,
        );
    });
    group(ui, crate::tl_id!("ui.stencils"), t, |ui| {
        soon(ui, Icon::Ruler, "Ruler", t);
    });
    group(ui, crate::tl_id!("ui.edit"), t, |ui| {
        if big(ui, Icon::Pen, crate::tl_id!("ui.draw_here_2"), crate::tl_id!("ui.put_a_sheet_to_draw_on_into_the_page"), t).clicked() {
            acts.push(Act::InkInsert);
        }
        if big(
            ui,
            Icon::Picture,
            crate::tl_id!("ui.save_picture_2"),
            crate::tl_id!("ui.write_the_drawing_again_with_no_pen_readings_in_it_f_09e93dbd9f"),
            t,
        )
        .clicked()
        {
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
    group(ui, crate::tl_id!("ui.shapes"), t, |ui| {
        soon(
            ui,
            Icon::Shapes,
            "Automatic
Shapes",
            t,
        );
    });
    group(ui, crate::tl_id!("ui.text"), t, |ui| {
        soon(
            ui,
            Icon::InkToText,
            "Ink to
Text",
            t,
        );
    });
    group(ui, crate::tl_id!("ui.math"), t, |ui| {
        soon(
            ui,
            Icon::Equation,
            "Ink to
Math",
            t,
        );
    });
    group(ui, crate::tl_id!("ui.replay"), t, |ui| {
        soon(
            ui,
            Icon::Replay,
            "Ink
Replay",
            t,
        );
    });
    group(ui, crate::tl_id!("ui.mode"), t, |ui| {
        if big(ui, Icon::FullPage, crate::tl_id!("ui.full_page_view_2"), crate::tl_id!("ui.hide_everything_but_the_page"), t).clicked() {
            acts.push(Act::ToggleFullPage);
        }
    });
    group(ui, crate::tl_id!("ui.help"), t, |ui| {
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
    group(ui, crate::tl_id!("ui.select"), t, |ui| {
        if big(ui, Icon::Table, crate::tl_id!("ui.select_table_2"), crate::tl_id!("ui.pick_the_whole_table"), t).clicked() {
            acts.push(Act::TableSelect(S::Whole));
        }
        if big(ui, Icon::TableColumn, crate::tl_id!("ui.select_columns_2"), crate::tl_id!("ui.pick_the_caret_s_column"), t).clicked() {
            acts.push(Act::TableSelect(S::Column));
        }
        if big(ui, Icon::TableRow, crate::tl_id!("ui.select_rows_2"), crate::tl_id!("ui.pick_the_caret_s_row"), t).clicked() {
            acts.push(Act::TableSelect(S::Row));
        }
    });
    group(ui, crate::tl_id!("ui.delete"), t, |ui| {
        soon(ui, Icon::Table, "Delete\nTable", t);
        if big(ui, Icon::TableColumn, crate::tl_id!("ui.delete_columns_2"), crate::tl_id!("ui.take_the_caret_s_column_out"), t).clicked() {
            acts.push(Act::Table(E::DeleteColumn));
        }
        if big(ui, Icon::TableRow, crate::tl_id!("ui.delete_rows_2"), crate::tl_id!("ui.take_the_caret_s_row_out"), t).clicked() {
            acts.push(Act::Table(E::DeleteRow));
        }
    });
    group(ui, crate::tl_id!("ui.insert"), t, |ui| {
        if big(ui, Icon::TableRow, crate::tl_id!("ui.insert_row_above_2"), crate::tl_id!("ui.a_row_above_the_caret_s"), t).clicked() {
            acts.push(Act::Table(E::InsertRow(true)));
        }
        if big(ui, Icon::TableRow, crate::tl_id!("ui.insert_row_below_2"), crate::tl_id!("ui.a_row_below_the_caret_s"), t).clicked() {
            acts.push(Act::Table(E::InsertRow(false)));
        }
        if big(ui, Icon::TableColumn, crate::tl_id!("ui.insert_column_left_2"), crate::tl_id!("ui.a_column_left_of_the_caret_s"), t).clicked() {
            acts.push(Act::Table(E::InsertColumn(true)));
        }
        if big(ui, Icon::TableColumn, crate::tl_id!("ui.insert_column_right_2"), crate::tl_id!("ui.a_column_right_of_the_caret_s"), t).clicked() {
            acts.push(Act::Table(E::InsertColumn(false)));
        }
    });
    group(ui, crate::tl_id!("ui.merge"), t, |ui| {
        soon(ui, Icon::Table, "Merge\nCells", t);
        soon(ui, Icon::Table, "Split\nCells", t);
    });
    group(ui, crate::tl_id!("ui.format"), t, |ui| {
        // One button that says what it will do, the way OneNote's does: it reads Hide
        // Borders while they are drawn, and Show Borders once they are not.
        let hiding = view.table_borders;
        let label = if hiding { "Hide\nBorders" } else { "Show\nBorders" };
        let tip = if hiding { "Draw tables without lines" } else { "Draw the lines round the cells again" };
        if big_on(ui, Icon::Table, label, tip, !hiding, t).clicked() {
            acts.push(Act::TableBorders(!hiding));
        }
        shading_menu(ui, view, t, acts);
    });
    group(ui, crate::tl_id!("ui.alignment"), t, |ui| {
        // The words in the caret's cell, or in every cell picked (issue #170). The one lit is
        // how those cells are aligned now; none is lit when they differ.
        use notecraft_model::tablecols::Align as A;
        for (align, icon, label, tip) in [
            (A::Left, Icon::DirLtr, crate::tl_id!("ui.table_align_left"), crate::tl_id!("ui.table_align_left_tip")),
            (A::Center, Icon::DirLtr, crate::tl_id!("ui.table_align_center"), crate::tl_id!("ui.table_align_center_tip")),
            (A::Right, Icon::DirRtl, crate::tl_id!("ui.table_align_right"), crate::tl_id!("ui.table_align_right_tip")),
        ] {
            if big_on(ui, icon, label, tip, view.table_align == Some(align), t).clicked() {
                acts.push(Act::TableAlign(align));
            }
        }
    });
    group(ui, crate::tl_id!("ui.data"), t, |ui| {
        // Whole rows by the caret's column, or only the picked rows; the heading stays on top.
        if big(ui, Icon::ListNumber, crate::tl_id!("ui.sort_ascending_2"), crate::tl_id!("ui.sort_the_rows_by_the_caret_s_column_a_to_z"), t)
            .clicked()
        {
            acts.push(Act::TableSort(false));
        }
        if big(ui, Icon::ListNumber, crate::tl_id!("ui.sort_descending_2"), crate::tl_id!("ui.sort_the_rows_by_the_caret_s_column_z_to_a"), t)
            .clicked()
        {
            acts.push(Act::TableSort(true));
        }
    });
    group(ui, crate::tl_id!("ui.convert"), t, |ui| {
        // Where OneNote converts a table to an Excel spreadsheet, NoteCraft hands it to
        // GridCraft, the open spreadsheet from the same workshop. Greyed when GridCraft
        // is not installed, because there is nowhere to hand it to.
        if view.gridcraft {
            if big(
                ui,
                Icon::Spreadsheet,
                crate::tl_id!("ui.convert_to_gridcraft_2"),
                crate::tl_id!("ui.write_the_table_out_and_open_it_in_gridcraft"),
                t,
            )
            .clicked()
            {
                acts.push(Act::ToGridCraft);
            }
            // The table went out as a file; this brings back what the spreadsheet made of
            // it. Nothing watches the file, because a page being edited should not change
            // under the writer's hands.
            if big(
                ui,
                Icon::Spreadsheet,
                crate::tl_id!("ui.read_back_from_it_2"),
                crate::tl_id!("ui.put_what_the_spreadsheet_holds_back_into_this_table"),
                t,
            )
            .clicked()
            {
                acts.push(Act::FromGridCraft);
            }
        } else {
            let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
            let resp = ui
                .add_enabled_ui(false, |ui| {
                    big(ui, Icon::Spreadsheet, crate::tl_id!("ui.convert_to_gridcraft_2"), crate::tl_id!("ui.gridcraft_is_not_installed"), &dim)
                })
                .inner;
            resp.on_hover_text(crate::tl_id!("ui.convert_to_gridcraft_gridcraft_is_not_installed"));
        }
    });
}

fn view_tab(ui: &mut Ui, source: bool, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    // The groups OneNote's View tab has, in its order: Views, Layout, Tools, Page Setup,
    // Zoom, Window. What NoteCraft does not have says so rather than being left out.
    group(ui, crate::tl_id!("ui.views"), t, |ui| {
        if big(ui, Icon::Page, crate::tl_id!("ui.normal_view"), crate::tl_id!("ui.the_page_beside_the_notebooks"), t).clicked() && view.full_page {
            acts.push(Act::ToggleFullPage);
        }
        if big(
            ui,
            if view.full_page { Icon::ExitFullPage } else { Icon::FullPage },
            crate::tl_id!("ui.full_page_view_2"),
            crate::tl_id!("ui.give_the_page_the_whole_window"),
            t,
        )
        .clicked()
        {
            acts.push(Act::ToggleFullPage);
        }
        rows(ui, |ui| {
            not_built(ui, crate::tl_id!("ui.dock_to_desktop"), t);
        });
    });
    group(ui, crate::tl_id!("ui.layout"), t, |ui| {
        rows(ui, |ui| {
            // What OneNote calls the navigation pane: notebooks and sections in one
            // column, the pages of the chosen section in the next.
            if small(
                ui,
                None,
                crate::tl_id!("ui.split"),
                crate::tl_id!("ui.separate_pages"),
                crate::tl_id!("ui.notebooks_and_sections_in_one_column_pages_in_the_next"),
                view.split_panes,
                t,
            )
            .clicked()
            {
                acts.push(Act::TogglePanes);
            }
            not_built(ui, crate::tl_id!("ui.tabs_layout"), t);
        });
    });
    group(ui, crate::tl_id!("ui.tools"), t, |ui| {
        rows(ui, |ui| {
            not_built(ui, crate::tl_id!("ui.immersive_reader"), t);
            not_built(ui, crate::tl_id!("ui.live_captions"), t);
        });
    });
    group(ui, crate::tl_id!("ui.page_setup"), t, |ui| {
        rule_lines_menu(ui, view, t, acts);
        rows(ui, |ui| {
            // i18n-exempt: T is the compact symbol on the page-title toggle.
            if small(ui, None, "T", crate::tl_id!("ui.hide_page_title"), crate::tl_id!("ui.hide_the_page_title"), view.hide_title, t).clicked() {
                acts.push(Act::ToggleTitle);
            }
            if small(
                ui,
                None,
                // i18n-exempt: # is the compact symbol on the snap-to-grid toggle.
                "#",
                crate::tl_id!("ui.snap_to_grid"),
                crate::tl_id!("ui.snap_boxes_to_the_page_s_grid_hold_alt_to_place_one_freely"),
                view.snap,
                t,
            )
            .clicked()
            {
                acts.push(Act::ToggleSnap);
            }
            ui.add_enabled_ui(view.has_page, |ui| {
                ui.menu_button(crate::tl_id!("ui.page_colour"), |ui| {
                    if page_colour_palette(ui, view, t, acts) {
                        ui.close();
                    }
                });
            });
            not_built(ui, crate::tl_id!("ui.paper_size"), t);
        });
    });
    group(ui, crate::tl_id!("ui.zoom"), t, |ui| {
        ui.horizontal(|ui| {
            if big(ui, Icon::ZoomOut, crate::tl_id!("ui.zoom_out"), crate::tl_id!("ui.make_the_page_smaller"), t).clicked() {
                acts.push(Act::Zoom(-1));
            }
            if big(ui, Icon::ZoomIn, crate::tl_id!("ui.zoom_in"), crate::tl_id!("ui.make_the_page_larger"), t).clicked() {
                acts.push(Act::Zoom(1));
            }
            rows(ui, |ui| {
                egui::ComboBox::from_label(crate::tl_id!("ui.zoom")).width(76.0).selected_text(format!("{}%", view.zoom_percent)).show_ui(ui, |ui| {
                    for percent in [50, 60, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250] {
                        if ui.selectable_label(view.zoom_percent == percent, format!("{percent}%")).clicked() {
                            acts.push(Act::ZoomTo(percent));
                        }
                    }
                });
                // i18n-exempt: 100% is a numeric zoom value, not translated copy.
                if small(ui, None, "100%", crate::tl_id!("ui.zoom_to_100"), crate::tl_id!("ui.show_the_page_at_100"), false, t).clicked() {
                    acts.push(Act::Zoom(0));
                }
                if small_labeled_icon(ui, Icon::PageWidth, crate::tl_id!("ui.page_width"), crate::tl_id!("ui.fit_the_page_to_the_available_width"), t)
                    .clicked()
                {
                    acts.push(Act::PageWidth);
                }
            });
        });
    });
    group(ui, crate::tl_id!("ui.window"), t, |ui| {
        rows(ui, |ui| {
            // Always on top is the window manager's business, and eframe can ask for it.
            if small(
                ui,
                None,
                crate::tl_id!("ui.top"),
                crate::tl_id!("ui.always_on_top"),
                crate::tl_id!("ui.keep_this_window_above_the_others"),
                view.always_on_top,
                t,
            )
            .clicked()
            {
                acts.push(Act::ToggleAlwaysOnTop);
            }
            not_built(ui, crate::tl_id!("ui.new_window"), t);
            not_built(ui, crate::tl_id!("ui.new_quick_note"), t);
        });
    });
    group(ui, crate::tl_id!("ui.appearance"), t, |ui| {
        if big(ui, Icon::Sun, crate::tl_id!("ui.theme"), crate::tl_id!("ui.light_or_dark_and_the_accent_colour"), t).clicked() {
            acts.push(Act::OpenThemes);
        }
    });
    group(ui, crate::tl_id!("ui.source"), t, |ui| {
        if big(
            ui,
            Icon::Code,
            if source { crate::tl_id!("ui.hide") } else { crate::tl_id!("ui.show") },
            crate::tl_id!("ui.the_page_source_beside_the_page"),
            t,
        )
        .clicked()
        {
            acts.push(Act::ToggleSource);
        }
    });
    group(ui, crate::tl_id!("ui.window"), t, |ui| {
        rows(ui, |ui| {
            not_built(ui, crate::tl_id!("ui.new_window"), t);
            not_built(ui, crate::tl_id!("ui.always_on_top"), t);
            not_built(ui, crate::tl_id!("ui.dock_to_desktop"), t);
        });
    });
}

/// A control that is listed so nobody goes looking for it, and says it is not built.
/// Something OneNote has that NoteCraft does not, among the small controls: drawn as a
/// greyed chip the size of a control rather than as a line of writing, so a row of them
/// reads as buttons that are off and not as a paragraph.
// i18n-ui-text: label
fn not_built(ui: &mut Ui, label: &str, t: &Tokens) {
    let label = crate::tl!(label);
    let font = egui::FontId::proportional(11.5);
    let width = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.muted).size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, SMALL_H), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect, 4.0, egui::Stroke::new(1.0, crate::theme::mix(t.panel, t.text, 0.14)), egui::StrokeKind::Inside);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, font, t.muted);
    let enabled = ui.is_enabled();
    let name = crate::i18n::format_message_id("ui.label_not_built", &[("label", label)]);
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name));
    resp.on_hover_text(crate::i18n::format_message_id("ui.label_not_built_yet", &[("label", crate::tl!(label))]));
}

/// A compact action in the ink-tool popup. The same treatment is used for unavailable
/// actions and the destructive tool removal, with the accent colour making the state clear
/// without overpowering the swatches above it.
// i18n-ui-text: label,tip
fn popup_action_button(ui: &mut Ui, label: &str, enabled: bool, colour: egui::Color32, tip: &str) -> egui::Response {
    let fg = if enabled { colour } else { ui.visuals().widgets.noninteractive.fg_stroke.color };
    let fill = crate::theme::mix(ui.visuals().panel_fill, colour, if enabled { 0.10 } else { 0.04 });
    let stroke = crate::theme::mix(ui.visuals().panel_fill, colour, if enabled { 0.45 } else { 0.18 });
    let button = egui::Button::new(egui::RichText::new(label).size(11.0).color(fg))
        .min_size(vec2(196.0, 22.0))
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, stroke));
    ui.add_enabled(enabled, button).on_hover_text(tip)
}

fn source_tab(ui: &mut Ui, format: Option<PageFormat>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, crate::tl_id!("ui.source"), t, |ui| {
        if big(ui, Icon::Code, crate::tl_id!("ui.hide"), crate::tl_id!("ui.close_the_source_pane"), t).clicked() {
            acts.push(Act::ToggleSource);
        }
    });
    group(ui, crate::tl_id!("ui.this_page"), t, |ui| {
        let name = format.map(PageFormat::label).unwrap_or("No page");
        ui.vertical(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(name).size(12.0).color(t.text));
            ui.label(egui::RichText::new(crate::tl_id!("ui.the_format_is_set_when_the_page_is_created")).size(10.0).color(t.muted));
        });
    });
}

/// A big button that draws one of the ribbon's own marks.
// i18n-ui-text: label,tip
fn big_mark(ui: &mut Ui, mark: Mark, label: &str, tip: &str, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
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
mod grid_line_tests {
    use super::first_grid_line;

    /// Issue #79: the paper's lines go through the anchor and every step from it, whichever side
    /// of the viewport's edge the anchor is on, so they move with the page as it pans.
    #[test]
    fn grid_lines_go_through_the_anchor_and_move_with_it() {
        for (min, anchor) in [(100.0, 100.0), (100.0, 107.0), (100.0, 40.0), (100.0, 400.0)] {
            let first = first_grid_line(min, anchor, 18.0);
            assert!(first >= min - 0.001 && first < min + 18.0, "the first line is in the viewport: min={min} anchor={anchor} first={first}");
            let steps = (first - anchor) / 18.0;
            assert!((steps - steps.round()).abs() < 0.001, "it is a whole number of steps from the anchor: {first} vs {anchor}");
        }
        // Panning the page by 3 points moves its lines by the same 3 points, the same way.
        let before = first_grid_line(100.0, 107.0, 18.0);
        let after = first_grid_line(100.0, 110.0, 18.0);
        assert!((after - before - 3.0).abs() < 0.001, "lines follow the page: {before} -> {after}");
    }
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

    /// SUP-009: the font size list is OneNote's whole sizes (the page format stores whole
    /// numbers, so 9.5 and 10.5 wait for a format change).
    #[test]
    fn the_font_size_list_is_onenotes_whole_sizes() {
        assert_eq!(FONT_SIZES.as_slice(), [8u8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72].as_slice());
    }
}

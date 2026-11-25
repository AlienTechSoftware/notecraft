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
use egui::{Align2, Color32, Rect, Sense, Ui, vec2};
use notecraft_ink::rack::{self, Nib};
use notecraft_ink::{Rgb, Tool};
use notecraft_model::{PageFormat, Tag};

/// The tabs, in order. "Source" only appears while the page source is shown, the way Office
/// shows a tab for what is selected.
pub const TABS: [&str; 5] = ["Home", "Insert", "Draw", "History", "View"];
pub const SOURCE_TAB: &str = "Source";
/// The families grouped by script, with a translated heading for each, and the families of no
/// listed script under Other. Built once for each catalogue read and kept in egui memory; rebuilding
/// them on every frame cloned thousands of names while the list was closed.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn script_sections(ctx: &egui::Context, families: &[String]) -> std::sync::Arc<Vec<(String, Vec<String>)>> {
    let Some(Ok(catalog)) = crate::fonts::catalog_if_ready() else {
        return std::sync::Arc::new(Vec::new());
    };
    let key = egui::Id::new("notecraft-font-sections");
    // The cache holds the catalogue it was built from, so a newer catalogue is never mistaken for it.
    let cached = ctx.data(|d| d.get_temp::<(std::sync::Arc<crate::fonts::system::Catalog>, std::sync::Arc<Vec<(String, Vec<String>)>>)>(key));
    if let Some((built_from, sections)) = cached
        && std::sync::Arc::ptr_eq(&built_from, &catalog)
    {
        return sections;
    }
    let mut sections: Vec<(String, Vec<String>)> = crate::font_picker::sections(families, |name| catalog.scripts(name))
        .into_iter()
        .map(|(script, names)| (script_heading(script), names))
        .collect();
    // A family that covers none of the scripts (a symbol or decorative face) still needs a place in
    // the list, so it is listed under Other.
    if !sections.is_empty() {
        let placed: std::collections::BTreeSet<&String> = sections.iter().flat_map(|(_, names)| names.iter()).collect();
        let other: Vec<String> = families.iter().filter(|name| !placed.contains(name)).cloned().collect();
        if !other.is_empty() {
            sections.push((crate::tl_id!("ui.script_other").to_string(), other));
        }
    }
    let sections = std::sync::Arc::new(sections);
    ctx.data_mut(|d| d.insert_temp(key, (catalog.clone(), sections.clone())));
    sections
}

/// Without the desktop font catalogue there are no script sections.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn script_sections(_ctx: &egui::Context, _families: &[String]) -> std::sync::Arc<Vec<(String, Vec<String>)>> {
    std::sync::Arc::new(Vec::new())
}

/// The families used lately, most recent first, shared by the ribbon and the floating toolbar. Kept
/// for this session in egui memory.
pub fn recent_families(ctx: &egui::Context) -> Vec<String> {
    ctx.data(|d| d.get_temp(egui::Id::new("notecraft-recent-fonts"))).unwrap_or_default()
}

/// Keeps the recent list in step with the saved one. The first call reads the saved list; after that
/// a change to the list is saved. `load` reads the saved text and `save` writes it.
pub fn sync_recent_fonts(ctx: &egui::Context, load: impl FnOnce() -> Option<String>, save: impl FnOnce(&str)) {
    let seeded_id = egui::Id::new("notecraft-recent-fonts-seeded");
    let stored_id = egui::Id::new("notecraft-recent-fonts-stored");
    if !ctx.data(|d| d.get_temp::<bool>(seeded_id)).unwrap_or(false) {
        ctx.data_mut(|d| d.insert_temp(seeded_id, true));
        let saved = crate::font_picker::parse_list(&load().unwrap_or_default());
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("notecraft-recent-fonts"), saved));
    }
    let now = recent_families(ctx);
    let stored: Option<Vec<String>> = ctx.data(|d| d.get_temp(stored_id));
    if stored.as_ref() != Some(&now) {
        save(&crate::font_picker::format_list(&now));
        ctx.data_mut(|d| d.insert_temp(stored_id, now));
    }
}

/// Forgets the recent fonts, as the reset of local settings does. The saved list goes with the settings file.
pub fn clear_recent_fonts(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.remove::<Vec<String>>(egui::Id::new("notecraft-recent-fonts"));
        d.remove::<Vec<String>>(egui::Id::new("notecraft-recent-fonts-stored"));
    });
}

/// Records a family as used now, from either toolbar.
pub fn remember_family(ctx: &egui::Context, name: &str) {
    let mut recent = recent_families(ctx);
    crate::font_picker::remember(&mut recent, name);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("notecraft-recent-fonts"), recent));
}

/// The translated heading of a script's section in the font list.
pub fn script_heading(script: crate::font_picker::Script) -> String {
    use crate::font_picker::Script;
    match script {
        Script::Latin => crate::tl_id!("ui.script_latin").to_string(),
        Script::Greek => crate::tl_id!("ui.script_greek").to_string(),
        Script::Cyrillic => crate::tl_id!("ui.script_cyrillic").to_string(),
        Script::Arabic => crate::tl_id!("ui.script_arabic").to_string(),
        Script::Hebrew => crate::tl_id!("ui.script_hebrew").to_string(),
        Script::Devanagari => crate::tl_id!("ui.script_devanagari").to_string(),
        Script::Thai => crate::tl_id!("ui.script_thai").to_string(),
        Script::Han => crate::tl_id!("ui.script_han").to_string(),
        Script::Kana => crate::tl_id!("ui.script_kana").to_string(),
        Script::Hangul => crate::tl_id!("ui.script_hangul").to_string(),
    }
}

pub const FONT_SIZES: [u8; 16] = [8, 9, 10, 11, 12, 14, 16, 18, 20, 22, 24, 26, 28, 36, 48, 72];
/// Shown only while the caret is in a table, the way Office shows a tab for the thing in
/// hand. It goes when the caret leaves, and the ribbon falls back to Home.
pub const TABLE_TAB: &str = "Table";

/// The dictation button in the tab strip, and how it should look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mic {
    /// Built without dictation.
    #[default]
    Hidden,
    /// Ready to start dictating.
    Ready,
    /// Words are finishing or waiting for review; the button waits too.
    Busy,
    /// Listening now: the button finishes.
    Live,
}

/// A microphone: a capsule on a stand.
fn paint_mic(painter: &egui::Painter, center: egui::Pos2, colour: egui::Color32) {
    let capsule = Rect::from_center_size(center + vec2(0.0, -3.0), vec2(6.0, 10.0));
    painter.rect_filled(capsule, 3.0, colour);
    let stroke = egui::Stroke::new(1.5, colour);
    let cup: Vec<egui::Pos2> = (0..=12)
        .map(|k| {
            let a = std::f32::consts::PI * k as f32 / 12.0;
            center + vec2(5.5 * a.cos(), -1.0 + 5.5 * a.sin())
        })
        .collect();
    painter.add(egui::Shape::line(cup, stroke));
    painter.line_segment([center + vec2(0.0, 4.5), center + vec2(0.0, 7.5)], stroke);
    painter.line_segment([center + vec2(-3.0, 7.5), center + vec2(3.0, 7.5)], stroke);
}

/// What a ribbon control asks the window to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    NewWindow,
    /// Lock the page open in the editor read only, or unlock it (issue #441).
    SetReadOnly(bool),
    QuickNote,
    /// Start dictating at the caret, or finish.
    #[cfg(feature = "voice")]
    Dictate,
    /// Open the dictation setup screen.
    #[cfg(feature = "voice")]
    DictationSettings,
    /// The File tab: open the File screen.
    File,
    SendPage,
    /// Show the saved versions of the open page.
    PageVersions,
    /// History > Next Unread (issue #291).
    NextUnread,
    /// History > Mark as Read, or as unread when it is read (issue #291).
    ToggleRead,
    /// History > Recent Edits: the pages changed lately, to go to one (issue #279).
    RecentEdits,
    /// History > Find by Author: the pages each person changed (issue #292).
    FindByAuthor,
    /// History > Hide Authors: who changed a page is shown under its title, or not.
    ToggleHideAuthors,
    /// Open the active notebook's recycle bin.
    RecycleBin,
    /// Open the built-in symbol picker.
    Symbols,
    /// Put marks around the selection, or at the caret: bold, italic, strikethrough, code.
    Mark(&'static str),
    /// Apply a point size to the selected text.
    FontSize(u8),
    /// Home > Font Colour: `#rrggbb`, or `None` for Automatic (issue #260).
    TextColour(Option<String>),
    /// Home > Text Highlight Colour: `#rrggbb`, or `None` for No Colour (issue #260).
    TextHighlight(Option<String>),
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
    /// Save picture as... exports an image file from the page.
    SavePictureAs(String),
    /// Rotate or flip the picture with this source (issue #273).
    TurnPicture(String, crate::turn::Turn),
    /// Copy Text from Picture (issue #274): the picture's source, and whether every page of the
    /// printout it belongs to is read rather than this page alone.
    CopyTextFromPicture {
        src: String,
        whole_printout: bool,
    },
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
    /// Set the caret's paragraphs against the left, the middle or the right (issue #258), or
    /// give them back their own alignment when they already have that one.
    Align(notecraft_model::justify::Align),
    /// Which way the whole page reads. `None` lets every line decide for itself.
    PageDirection(Option<notecraft_model::Dir>),
    /// Which way the selection, or what is typed next, reads: an isolated run inside the line.
    Embedding(notecraft_model::Dir),
    /// Take the marks off the selection.
    ClearFormatting,
    /// Put a tag on the line the caret is in, or take it off again.
    Tag(Tag),
    /// Take up the Insert Space tool, or put it down (issue #164).
    InsertSpaceTool,
    /// Open the Tags Summary: the tagged notes, and a way to each of them (issue #166).
    FindTags,
    /// Home > More tags > Customize Tags: the dialog where a person defines their own tags.
    CustomizeTags,
    /// Gather what has drifted apart on the page, boxes and ink, back together (issue #23).
    CollectAll,
    /// Draw > Ink Help: open the drawing page of the Welcome notebook (issue #296).
    InkHelp,
    /// Draw > Ruler: put the ruler out on the page, or away (issue #293).
    ToggleRuler,
    /// Draw > Automatic Shapes: turn shape recognition on or off (issue #294).
    ToggleAutoShapes,
    /// Draw > Ink Replay: replay the page's ink in the order it was drawn, or stop (issue #295).
    ToggleReplay,
    /// View > Tabs layout: sections as tabs across the top of the page, or not (#268, #33).
    ToggleTabsLayout,
    /// View > Immersive Reader: the page as plain, large reading (issue #299).
    ImmersiveReader,
    /// Open the Page Templates pane: new pages from templates, and this page kept as one
    /// (issue #173).
    PageTemplates,
    /// Something OneNote has that NoteCraft does not: say so plainly.
    NotBuilt(&'static str),
    /// Change the shape of the table the caret is in.
    Table(crate::pageedit::TableEdit),
    /// Table > Delete Table: the whole table the caret is in.
    DeleteTable,
    /// View > Dock to Desktop: a narrow window at the side of the screen, or back (issue #267).
    ToggleDock,
    /// Home > Meeting Details: pick a meeting from a calendar file (issue #283).
    MeetingDetails,
    /// Insert > Stickers: put this sticker on the page (issue #286).
    Sticker(&'static str),
    /// Table > Merge Cells: the picked cells become one (issue #298).
    MergeCells,
    /// Table > Split Cells: the merged cell the caret is in becomes its cells again (issue #298).
    SplitCells,
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
    /// View > Paper Size: the sheet the page is written for, as the page file writes it
    /// ([`notecraft_model::paper::Paper::write`]), or `None` for Auto (issue #269).
    PaperSize(Option<String>),
    /// Colour the open page's rule and grid lines, or `None` for the theme's own (issue #288).
    RuleColour(Option<notecraft_model::NodeColor>),
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
    /// Insert > Table > Insert Table...: the dialog for a size typed in.
    InsertTableDialog,
    /// Home > Alignment and direction > Paragraph Spacing Options: the dialog for the room
    /// before and after the paragraphs the caret or the selection is in, and their line height.
    ParagraphSpacing,
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
    /// Put back the stroke Undo stroke last took back.
    InkRedo,
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
    /// Draw > Panning Hand: a drag with the main button (or one finger) moves the page, and
    /// nothing on it is drawn, picked or written in.
    Pan,
    /// Draw > Add > Laser Pointer (issue #289): a red dot follows the pointer and a drag leaves
    /// a short trail that fades, to point at something while presenting. Nothing is drawn on
    /// the page, nothing is saved and there is nothing to undo.
    Laser,
    /// The Eyedropper of the rack's tool at this place (issue #290): the colour of the ink
    /// under the pointer is shown beside it, and a click gives that tool that colour.
    Eyedropper(usize),
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
            Pointer::Pan => "pan",
            Pointer::Laser => "laser",
            Pointer::Eyedropper(_) => "eyedropper",
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
    /// View > Dock to Desktop is on (issue #267).
    pub docked: bool,
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
    /// True while the Insert Space tool is in hand.
    pub space_tool: bool,
    /// True while Draw > Ruler is out on the page (issue #293).
    pub ruler: bool,
    /// True while Draw > Automatic Shapes is on (issue #294).
    pub auto_shapes: bool,
    /// True while Draw > Ink Replay is playing the page's ink (issue #295).
    pub replaying: bool,
    /// True while View > Tabs layout shows the sections as tabs (issues #268, #33).
    pub tabs_layout: bool,
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
    /// The colour of the open page's rule and grid lines, `None` for the theme's (issue #288).
    pub rule_colour: Option<notecraft_model::NodeColor>,
    /// The open page's sheet; `None` is Auto (issue #269).
    pub paper_size: Option<notecraft_model::paper::Paper>,
    /// Whether the page has anything to take back, and anything to put back.
    pub can_undo: bool,
    pub can_redo: bool,
    /// Whether the drawing has a stroke that Undo stroke took back, for Draw > Redo stroke.
    pub can_ink_redo: bool,
    /// How readily the pen answers pressure, 0 to 100.
    pub sensitivity: u8,
    /// Whether the current input path has reported a pressure-capable pen.
    pub pressure_available: bool,
    /// A page is open for page-specific History commands.
    pub has_page: bool,
    pub has_recycle_notebook: bool,
    /// The open page was marked unread by hand (issue #291).
    pub page_marked_unread: bool,
    /// History > Hide Authors is on (issue #292).
    pub hide_authors: bool,
    /// The Format Painter is holding formatting to put on the next words picked.
    pub painting: bool,
    /// The style of the block the caret is in (a [`STYLES`] kind), or "" when there is no caret.
    pub style: &'static str,
    pub font_family: Option<&'a str>,
    /// The marks of the words at the caret or of the whole selection, which Home's mark
    /// buttons show as pressed (issue #255).
    pub marks: crate::pageedit::Marks,
    pub font_size: Option<&'a str>,
    /// The text colour and highlight of the words at the caret or the whole selection, when
    /// they are all the same (issue #260). `Some(None)` is Automatic or No Colour.
    pub text_colour: Option<Option<&'a str>>,
    pub text_highlight: Option<Option<&'a str>>,
    /// The selected words use more than one font colour, or highlight: the pickers say Mixed
    /// rather than Automatic or No colour (issue #373).
    pub text_colour_mixed: bool,
    pub text_highlight_mixed: bool,
    /// Whether there is a page open for editing. Without one (no page, or a locked page) Home's
    /// text controls are shown as unavailable (issue #373), rather than looking ready and doing
    /// nothing; with one and no caret they set what the next words typed look like.
    pub text_editable: bool,
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
    // A mouse drag must not scroll: the controls here are clicked and dragged themselves, and a
    // drag that scrolled the ribbon instead would take the pen out of the user's hand. On a
    // touch screen a finger drag is the only way to reach the groups past the edge, so there
    // it scrolls (egui's OnTouch); a control under the finger still takes its own drag.
    // Where an overflow arrow or the wheel asked the ribbon to go, last frame (issue #277).
    let jump_id = egui::Id::new(("ribbon-jump", chosen));
    let jump = ui.data_mut(|d| d.remove_temp::<f32>(jump_id));
    let mut area = egui::ScrollArea::horizontal();
    if let Some(x) = jump {
        area = area.horizontal_scroll_offset(x);
    }
    let out = area
        .id_salt(("ribbon", chosen))
        // The ribbon is an Office-style command band, not a document scroller.  A
        // visible horizontal scrollbar is painted across its bottom edge when a tab
        // is wider than the window, which looks like a stray black artifact.  Keep
        // the scroll area (overflow tabs still scroll) but do not paint that bar.
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .scroll_source(egui::scroll_area::ScrollSource { drag: finger_drag(ui.ctx()), ..egui::scroll_area::ScrollSource::default() })
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
    if let Some(x) = overflow_arrows(ui, &out, t) {
        // Ask for another frame only when the arrows move: asking every frame keeps the window redrawing.
        let last_id = egui::Id::new("ribbon-arrows-last");
        if ui.data(|d| d.get_temp::<f32>(last_id)) != Some(x) {
            ui.ctx().request_repaint();
        }
        ui.data_mut(|d| {
            d.insert_temp(last_id, x);
            d.insert_temp(jump_id, x);
        });
    } else {
        ui.data_mut(|d| d.remove::<f32>(egui::Id::new("ribbon-arrows-last")));
    }
    acts
}

/// How much of the ribbon an arrow moves it by: most of what is in view, so a group cut at
/// the edge comes whole into sight and nothing is skipped.
const OVERFLOW_STEP: f32 = 0.7;

/// The arrows at the ends of a ribbon too wide for the window (issue #277). The ribbon
/// scrolls sideways but shows no scroll bar, so nothing said that groups lay past the edge:
/// an arrow appears at each end that has more beyond it, over a fade into the ribbon, and a
/// click (or Enter on it from the keyboard) brings the next stretch into view. The mouse wheel
/// over the ribbon scrolls it sideways as well.
/// Returns where the ribbon should scroll to, when that changed.
fn overflow_arrows<R>(ui: &mut Ui, out: &egui::scroll_area::ScrollAreaOutput<R>, t: &Tokens) -> Option<f32> {
    let view = out.inner_rect;
    let spare = (out.content_size.x - view.width()).max(0.0);
    if spare < 1.0 {
        return None;
    }
    let was = out.state.offset().x;
    let mut offset = was;
    // A vertical wheel over a ribbon that only scrolls sideways moves it sideways.
    let wheel = ui.input(|i| if i.pointer.hover_pos().is_some_and(|p| view.contains(p)) { i.smooth_scroll_delta.y } else { 0.0 });
    if wheel != 0.0 {
        offset -= wheel;
    }
    let side = 22.0;
    let arrow = |ui: &mut Ui, left: bool| -> bool {
        let rect = if left {
            egui::Rect::from_min_size(view.min, vec2(side, view.height()))
        } else {
            egui::Rect::from_min_size(egui::pos2(view.max.x - side, view.min.y), vec2(side, view.height()))
        };
        let id = ui.id().with(("ribbon-overflow", left));
        let resp = ui.interact(rect, id, Sense::click());
        let painter = ui.painter();
        // A fade from the ribbon's colour, so the words under the arrow do not show through.
        let fade = rect.expand2(vec2(10.0, 0.0)).translate(vec2(if left { 10.0 } else { -10.0 }, 0.0));
        let mut mesh = egui::Mesh::default();
        let (solid, clear) = (t.panel, t.panel.gamma_multiply(0.0));
        let (l, r) = if left { (solid, clear) } else { (clear, solid) };
        mesh.colored_vertex(fade.left_top(), l);
        mesh.colored_vertex(fade.right_top(), r);
        mesh.colored_vertex(fade.right_bottom(), r);
        mesh.colored_vertex(fade.left_bottom(), l);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(mesh));
        if resp.hovered() || resp.has_focus() {
            painter.rect_filled(rect.shrink2(vec2(2.0, 8.0)), 4.0, t.row_hover);
        }
        let c = rect.center();
        let d = if left { -1.0 } else { 1.0 };
        let stroke = egui::Stroke::new(2.0, t.accent);
        painter.line_segment([egui::pos2(c.x - 3.0 * d, c.y - 7.0), egui::pos2(c.x + 3.0 * d, c.y)], stroke);
        painter.line_segment([egui::pos2(c.x + 3.0 * d, c.y), egui::pos2(c.x - 3.0 * d, c.y + 7.0)], stroke);
        let name =
            if left { crate::tl_id!("ui.show_more_of_the_ribbon_to_the_left") } else { crate::tl_id!("ui.show_more_of_the_ribbon_to_the_right") };
        let enabled = ui.is_enabled();
        resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name));
        resp.on_hover_text(name).clicked()
    };
    if offset > 0.5 && arrow(ui, true) {
        offset -= view.width() * OVERFLOW_STEP;
    }
    if offset < spare - 0.5 && arrow(ui, false) {
        offset += view.width() * OVERFLOW_STEP;
    }
    let offset = offset.clamp(0.0, spare);
    ((offset - was).abs() > 0.1).then_some(offset)
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
pub fn tab_strip(ui: &mut Ui, tab: &str, source: bool, in_table: bool, collapsed: bool, mic: Mic, t: &Tokens, acts: &mut Vec<Act>) {
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
        if mic != Mic::Hidden {
            ui.add_space(12.0);
            let live = mic == Mic::Live;
            let label = if live { crate::tl_id!("voice.finish_dictating_0f7d9f4b") } else { crate::tl_id!("voice.dictate_3eaff69c") };
            let font = egui::FontId::proportional(12.5);
            let galley = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.band_text);
            let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 40.0, TAB_STRIP_H), Sense::click());
            let painter = ui.painter_at(rect);
            let pill = rect.shrink2(vec2(2.0, 4.0));
            if live || (resp.hovered() && mic == Mic::Ready) {
                painter.rect_filled(pill, 10.0, t.band_hover);
            }
            let ink = if mic == Mic::Busy { t.band_text.gamma_multiply(0.5) } else { t.band_text };
            paint_mic(&painter, egui::pos2(rect.min.x + 16.0, rect.center().y), ink);
            painter.text(egui::pos2(rect.min.x + 28.0, rect.center().y + 1.0), Align2::LEFT_CENTER, label, font, ink);
            let enabled = mic != Mic::Busy;
            resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
            let tip = match mic {
                Mic::Live => crate::tl_id!("voice.finish_dictating_enter_esc_cancels_2151a686"),
                Mic::Busy => crate::tl_id!("voice.insert_or_discard_the_dictated_words_first_df8983ce"),
                _ => crate::tl_id!("voice.dictate_at_the_cursor_or_at_the_end_of_the_page_12635b46"),
            };
            let resp = resp.on_hover_text(tip);
            #[cfg(feature = "voice")]
            {
                if resp.clicked() && enabled {
                    acts.push(Act::Dictate);
                }
                let (gear, settings) = ui.allocate_exact_size(vec2(22.0, TAB_STRIP_H), Sense::click());
                let hover = settings.hovered();
                ui.painter().text(
                    gear.center(),
                    Align2::CENTER_CENTER,
                    "\u{22ef}",
                    egui::FontId::proportional(14.0),
                    if hover { t.band_text } else { t.band_text.gamma_multiply(0.7) },
                );
                settings.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, crate::tl_id!("voice.dictation_setup_b70a0aa8")));
                if settings.on_hover_text(crate::tl_id!("voice.dictation_setup_b70a0aa8")).clicked() {
                    acts.push(Act::DictationSettings);
                }
            }
            #[cfg(not(feature = "voice"))]
            let _ = (resp, acts.len());
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

/// A big button with two parts, as Office draws one (issue #374): the picture does the usual
/// thing at once, and the name with its chevron opens the other ways. Each part is a button of
/// its own, so a pointer, the keyboard and a screen reader can tell them apart: the second is
/// named "<name> options". Hovering one part lights it and draws the line between the two.
/// Returns the action part and the part that opens the menu.
// i18n-ui-text: label,tip
fn big_split(ui: &mut Ui, icon: Icon, label: &str, tip: &str, t: &Tokens) -> (egui::Response, egui::Response) {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let font = egui::FontId::proportional(12.0);
    let widest = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x;
    let (rect, _) = ui.allocate_exact_size(vec2(BIG.max(widest + BIG_PAD), GROUP_H - 18.0), Sense::hover());
    let split_y = rect.min.y + ICON_Y + 15.0;
    let (top, bottom) =
        (Rect::from_min_max(rect.min, egui::pos2(rect.max.x, split_y)), Rect::from_min_max(egui::pos2(rect.min.x, split_y), rect.max));
    let main = ui.interact(top, ui.id().with((label, "split-main")), Sense::click());
    let open = ui.interact(bottom, ui.id().with((label, "split-open")), Sense::click());
    let painter = ui.painter_at(rect);
    let enabled = ui.is_enabled();
    let lit = |r: &egui::Response| r.hovered() || r.has_focus();
    if lit(&main) || lit(&open) {
        // The whole button is faintly lit, the part that will answer more strongly, and the line
        // between them says there are two.
        painter.rect_filled(rect, 4.0, t.row_hover.gamma_multiply(0.5));
        painter.rect_filled(if lit(&main) { top } else { bottom }, 4.0, t.row_hover);
        painter.hline(rect.min.x + 4.0..=rect.max.x - 4.0, split_y.round() + 0.5, egui::Stroke::new(1.0, crate::theme::mix(t.panel, t.text, 0.18)));
    }
    for part in [&main, &open] {
        if part.has_focus() {
            painter.rect_stroke(part.rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }
    }
    let colour = if enabled { if icon == Icon::Picture { t.ok } else { t.accent } } else { t.muted };
    icons::paint_ribbon(ui, Rect::from_center_size(egui::pos2(rect.center().x, rect.min.y + ICON_Y), vec2(28.0, 28.0)), icon, colour);
    painter.text(egui::pos2(rect.center().x, rect.min.y + LABEL_Y), Align2::CENTER_CENTER, label, font, t.text);
    chevron(&painter, egui::pos2(rect.center().x, rect.min.y + LABEL_Y + LINE_H - 1.0), t.muted);
    let (name, options) = (label.to_string(), crate::i18n::format_message_id("ui.name_options", &[("name", label)]));
    main.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    let options_tip = options.clone();
    open.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, options.clone()));
    (main.on_hover_text(tip), open.on_hover_text(options_tip))
}

/// A picker opened from the keyboard puts the keyboard on its first choice (issue #374), so
/// Enter or the arrows work at once. `resp` is the button that opened it.
fn opened_by_keys(resp: &egui::Response) -> bool {
    resp.clicked() && !resp.clicked_by(egui::PointerButton::Primary)
}

fn first_choice_id(open_id: egui::Id) -> egui::Id {
    open_id.with("first-choice")
}

/// Gives the first choice of the picker `open_id` the keyboard, when it was opened from it.
fn first_choice(ui: &Ui, open_id: egui::Id, choice: &egui::Response) {
    if !ui.is_sizing_pass() && ui.data_mut(|d| d.remove_temp::<bool>(first_choice_id(open_id))).unwrap_or(false) {
        choice.request_focus();
    }
}

fn symbols_button_id() -> egui::Id {
    egui::Id::new("ribbon-symbol-button")
}

/// The Symbol picker closed with nothing picked: the keyboard goes back to the Symbol button that
/// opened it (issue #374), not to nowhere.
pub fn return_focus_from_symbols(ctx: &egui::Context) {
    if let Some(button) = ctx.data(|d| d.get_temp::<egui::Id>(symbols_button_id())) {
        ctx.memory_mut(|m| m.request_focus(button));
    }
}

/// The title along the top of a picker (issue #374): what the list is for, in the heading style.
fn picker_title(ui: &mut Ui, title: &str, t: &Tokens) {
    ui.add(egui::Label::new(egui::RichText::new(title).strong().color(t.text)).selectable(false));
    ui.add_space(2.0);
}

fn pictures_menu(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let (main, arrow) = big_split(ui, Icon::Picture, crate::tl_id!("ui.pictures"), crate::tl_id!("ui.choose_how_to_insert_a_picture"), t);
    let popup_id = egui::Popup::default_response_id(&arrow);
    let was_open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    if opened_by_keys(&arrow) && !was_open {
        ui.data_mut(|d| d.insert_temp(first_choice_id(popup_id), true));
    }
    egui::Popup::menu(&arrow).show(|ui| {
        egui::Frame::popup(ui.style()).corner_radius(8.0).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_width(228.0);
            picker_title(ui, crate::tl_id!("ui.insert_a_picture_from"), t);
            let file = picture_menu_row(ui, Icon::Picture, crate::tl_id!("ui.picture_from_a_file"), t, true);
            first_choice(ui, popup_id, &file);
            if file.clicked() {
                acts.push(Act::AddPicture);
                ui.close();
            }
            let _ = picture_menu_row(ui, Icon::Attach, crate::tl_id!("ui.from_camera"), t, false);
            let _ = picture_menu_row(ui, Icon::Link, crate::tl_id!("ui.from_online"), t, false);
        });
    });
    // Escape closes the list and gives the keyboard back to the part that opened it.
    if was_open && !egui::Popup::is_id_open(ui.ctx(), popup_id) && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        arrow.request_focus();
    }
    main.clicked()
}

// i18n-ui-text: label
fn picture_menu_row(ui: &mut Ui, icon: Icon, label: &str, t: &Tokens, enabled: bool) -> egui::Response {
    let label = crate::tl!(label);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(vec2(212.0, 36.0), sense);
    if (response.hovered() || response.has_focus()) && enabled {
        ui.painter().rect_filled(rect, 6.0, crate::theme::mix(t.panel, t.accent, 0.12));
    }
    if response.has_focus() {
        ui.painter().rect_stroke(rect.shrink(0.5), 6.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
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
    // A source that is not there yet says so where it is pointed at, briefly (issue #374).
    if enabled { response } else { response.on_hover_text(crate::i18n::format_message_id("ui.label_not_built_yet", &[("label", label)])) }
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
    } else if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    // The button the keyboard is on is ringed, as every other control in the ribbon is (#373).
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
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

/// The back of a small ribbon control (issue #373), the same for every one of them: a control
/// that is on is filled and edged in the accent, so a pressed Bold reads at a glance and not only
/// by a faint tint; the one under the pointer is shaded; the one the keyboard is on is ringed.
fn small_back(painter: &egui::Painter, rect: Rect, resp: &egui::Response, on: bool, t: &Tokens) {
    if on {
        painter.rect_filled(rect, 4.0, t.row_selected);
        painter.rect_stroke(
            rect.shrink(0.5),
            4.0,
            egui::Stroke::new(1.0, crate::theme::mix(t.row_selected, t.accent, 0.7)),
            egui::StrokeKind::Inside,
        );
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
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
    small_back(&painter, rect, &resp, on, t);
    let live = ui.is_enabled();
    match icon {
        Some(i) => icons::paint_ribbon(ui, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), i, if live { t.accent } else { t.muted }),
        None => {
            painter.text(rect.center(), Align2::CENTER_CENTER, letter, egui::FontId::proportional(12.5), if live { t.strong } else { t.muted });
        }
    }
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// A setting that is on or off, shown as a box with a tick and its name (issue #376), so the
/// View tab's toggles say what they are and whether they are on, rather than a bare letter or
/// an empty square.
// i18n-ui-text: label,tip
fn small_check(ui: &mut Ui, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let text = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.0), t.text).size();
    let (rect, resp) = ui.allocate_exact_size(vec2(text.x + 30.0, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    let boxed = Rect::from_center_size(egui::pos2(rect.min.x + 11.0, rect.center().y), vec2(12.0, 12.0));
    if on {
        painter.rect_filled(boxed, 2.0, t.accent);
        let c = boxed.center();
        // The accent is built to stand at least 3:1 against the panel, so the panel's colour is
        // the tick that stays visible on it in every theme (white does not on a light accent).
        painter.add(egui::Shape::line(vec![c + vec2(-3.5, 0.2), c + vec2(-1.0, 2.8), c + vec2(3.5, -2.5)], egui::Stroke::new(1.6, t.panel)));
    } else {
        painter.rect_stroke(boxed, 2.0, egui::Stroke::new(1.2, t.muted), egui::StrokeKind::Inside);
    }
    painter.text(egui::pos2(rect.min.x + 23.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), t.text);
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
    small_back(&painter, rect, &resp, false, t);
    icons::paint_ribbon(ui, Rect::from_center_size(egui::pos2(rect.min.x + 13.0, rect.center().y), vec2(18.0, 18.0)), icon, t.accent);
    painter.text(egui::pos2(rect.min.x + 27.0, rect.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, false, name.clone()));
    resp.on_hover_text(tip)
}

/// The name at the start of a row of choices that act on one thing (issue #373): "Page" or
/// "Paragraph" before the direction buttons, in a column of its own wide enough for the longer
/// of the two, so the buttons after them line up and each row says what it changes.
// i18n-ui-text: text
fn scope_label(ui: &mut Ui, text: &str, t: &Tokens) {
    let text = crate::tl!(text);
    let font = egui::FontId::proportional(11.5);
    let widest = [crate::tl_id!("ui.page"), crate::tl_id!("ui.paragraph_row")]
        .iter()
        .map(|w| ui.painter().layout_no_wrap(crate::tl!(w).to_string(), font.clone(), t.muted).size().x)
        .fold(0.0_f32, f32::max);
    let (rect, resp) = ui.allocate_exact_size(vec2(widest + 6.0, SMALL_H), Sense::hover());
    ui.painter().text(egui::pos2(rect.min.x, rect.center().y), Align2::LEFT_CENTER, text, font, t.muted);
    let name = text.to_owned();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, true, name.clone()));
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
    // Issue #373: the font and its size share the top row; the marks and the colours the second;
    // lists, indentation, alignment and quote the third. Every small control is one size and
    // takes the keyboard the same way, and with nothing to format they are all unavailable.
    group(ui, crate::tl_id!("ui.basic_text"), t, |ui| {
        if !view.text_editable {
            ui.disable();
        }
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
                // Warn only once the installed list is known; while it loads, nothing is missing yet.
                // The default setting shows as its translated label, which is not a font name, so it is never missing.
                let named_family = view.font_family.filter(|family| *family != crate::tl_id!("ui.default"));
                let family_missing = !families.is_empty() && crate::font_picker::is_missing(named_family.unwrap_or(""), &families);
                // Record a missing font once, when it first shows, not on every frame.
                let missing_key = egui::Id::new("notecraft-missing-font-reported");
                if family_missing {
                    let shown = named_family.unwrap_or("");
                    if ui.data(|d| d.get_temp::<String>(missing_key)).as_deref() != Some(shown) {
                        crate::usage::record(crate::usage::Level::Warning, "font_missing_shown", &[("family", shown)]);
                        ui.data_mut(|d| d.insert_temp(missing_key, shown.to_owned()));
                    }
                } else {
                    ui.data_mut(|d| d.remove::<String>(missing_key));
                }
                // The list grouped by script, with a translated heading for each. Built once per catalogue read.
                let sections = script_sections(ui.ctx(), &families);
                // Recently used families, most recent first. Kept for this session in egui memory.
                let recent_families: Vec<String> = recent_families(ui.ctx());
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
                        filter: true,
                        recent: &recent_families,
                        missing: family_missing,
                        preview: true,
                        sections: &sections,
                        add_font: Some(crate::tl_id!("ui.add_font")),
                        usage: "ribbon-family",
                        tokens: *t,
                    },
                );
                if family_out.add_font {
                    // The Add Font row: pick font files, install them, and read the list again.
                    if let Some(report) = crate::font_install::pick_and_install()
                        && !report.copied.is_empty()
                    {
                        crate::fonts::rescan_in_background(ui.ctx().clone());
                    }
                }
                if let Some(name) = family_out.chosen {
                    remember_family(ui.ctx(), &name);
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
                        accept: &|typed| crate::font_picker::size_is_usable(typed, &FONT_SIZES),
                        filter: false,
                        recent: &[],
                        missing: false,
                        preview: false,
                        sections: &[],
                        add_font: None,
                        usage: "ribbon-size",
                        tokens: *t,
                    },
                );
                let boxes = crate::floating::FontBoxes {
                    family: view.font_family.unwrap_or("").to_owned(),
                    size: shown.to_owned(),
                    families: families.clone(),
                    families_note: families_note.clone(),
                    sizes: sizes.clone(),
                    recent: recent_families.clone(),
                    family_missing,
                    sections: sections.clone(),
                };
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(crate::floating::FONT_BOXES_KEY), boxes));
                if let Some(size) = size_out.chosen.and_then(|typed| typed.parse::<u8>().ok()) {
                    acts.push(Act::FontSize(size));
                }
                if mark_button(ui, Mark::ClearFormat, crate::tl_id!("ui.clear_formatting_2"), crate::tl_id!("ui.take_the_marks_off"), false, t)
                    .clicked()
                {
                    acts.push(Act::ClearFormatting);
                }
            });
            ui.horizontal(|ui| {
                if mark_button(ui, Mark::Bold, crate::tl_id!("ui.bold"), crate::tl_id!("ui.bold_ctrl_b"), view.marks.bold, t).clicked() {
                    acts.push(Act::Mark("bold"));
                }
                if mark_button(ui, Mark::Italic, crate::tl_id!("ui.italic"), crate::tl_id!("ui.italic_ctrl_i"), view.marks.italic, t).clicked() {
                    acts.push(Act::Mark("italic"));
                }
                if mark_button(ui, Mark::Underline, crate::tl_id!("ui.underline"), crate::tl_id!("ui.underline"), view.marks.underline, t).clicked() {
                    acts.push(Act::Mark("underline"));
                }
                if mark_button(ui, Mark::Strike, crate::tl_id!("ui.strikethrough"), crate::tl_id!("ui.strike_through"), view.marks.strike, t)
                    .clicked()
                {
                    acts.push(Act::Mark("strike"));
                }
                // i18n-exempt: x2 is the compact mathematical symbol for subscript.
                if small(ui, None, "x2", crate::tl_id!("ui.subscript"), crate::tl_id!("ui.subscript"), view.marks.subscript, t).clicked() {
                    acts.push(Act::Mark("subscript"));
                }
                // i18n-exempt: x^2 is the compact mathematical symbol for superscript.
                if small(ui, None, "x^2", crate::tl_id!("ui.superscript"), crate::tl_id!("ui.superscript"), view.marks.superscript, t).clicked() {
                    acts.push(Act::Mark("superscript"));
                }
                if small(ui, Some(Icon::Code), "", crate::tl_id!("ui.code_2"), crate::tl_id!("ui.code_in_one_line"), false, t).clicked() {
                    acts.push(Act::Mark("code"));
                }
                // The colours last in the row, so the controls before them keep their places (#260).
                colour_palette(ui, false, view.text_colour, view.text_colour_mixed, t, acts);
                colour_palette(ui, true, view.text_highlight, view.text_highlight_mixed, t, acts);
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
                if small(ui, Some(Icon::Link), "", crate::tl_id!("ui.link"), crate::tl_id!("ui.a_link"), false, t).clicked() {
                    acts.push(Act::Insert("link"));
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
                scope_label(ui, crate::tl_id!("ui.page"), t);
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
                scope_label(ui, crate::tl_id!("ui.paragraph_row"), t);
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
        if big(ui, Icon::Search, crate::tl_id!("ui.find_tags_3"), crate::tl_id!("ui.list_the_tagged_notes_and_go_to_them"), t).clicked() {
            acts.push(Act::FindTags);
        }
    });
    group(ui, crate::tl_id!("ui.email"), t, |ui| {
        if big(ui, Icon::Attach, crate::tl_id!("ui.send_page_3"), crate::tl_id!("ui.portable_send_hint"), t).clicked() {
            acts.push(Act::SendPage);
        }
    });
    group(ui, crate::tl_id!("ui.meetings"), t, |ui| {
        if big(ui, Icon::Calendar, crate::tl_id!("ui.meeting_details_3"), crate::tl_id!("ui.meeting_details_tip"), t).clicked() {
            acts.push(Act::MeetingDetails);
        }
    });
}

/// View > Paper Size (issue #269): Auto or a sheet, which way it is turned, and its margins.
/// Every change is an act at once, so the page shows it while the menu stays open.
fn paper_size_menu(ui: &mut Ui, now: Option<notecraft_model::paper::Paper>, t: &Tokens, acts: &mut Vec<Act>) {
    use notecraft_model::paper::{MAX_SIDE, MIN_SIDE, Paper, Size};
    ui.set_min_width(250.0);
    ui.label(egui::RichText::new(crate::tl_id!("ui.size")).strong());
    if ui.selectable_label(now.is_none(), crate::tl_id!("ui.auto_the_page_grows_with_what_is_on_it")).clicked() {
        acts.push(Act::PaperSize(None));
    }
    let base = now.unwrap_or_else(|| Paper::new(Size::Letter));
    egui::Grid::new("paper-sizes").num_columns(2).spacing(vec2(6.0, 1.0)).show(ui, |ui| {
        for (n, size) in Size::STANDARD.into_iter().enumerate() {
            let here = now.is_some_and(|p| p.size == size);
            if ui.selectable_label(here, crate::tl!(size.label())).clicked() {
                acts.push(Act::PaperSize(Some(Paper { size, ..base }.write())));
            }
            if n % 2 == 1 {
                ui.end_row();
            }
        }
        let custom = now.is_some_and(|p| matches!(p.size, Size::Custom(..)));
        if ui.selectable_label(custom, crate::tl_id!("ui.custom_size")).clicked() && !custom {
            let (w, h) = base.sheet_mm();
            acts.push(Act::PaperSize(Some(Paper { size: Size::Custom(w, h), landscape: false, ..base }.write())));
        }
    });
    let Some(paper) = now else {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(crate::tl_id!("ui.auto_pages_have_no_edges_choose_a_size_to_see_the_sheet")).color(t.muted).size(11.0));
        return;
    };
    let mut next = paper;
    if let Size::Custom(w, h) = paper.size {
        let (mut w, mut h) = (w, h);
        ui.horizontal(|ui| {
            mm_field(ui, &mut w, MIN_SIDE..=MAX_SIDE, crate::tl_id!("ui.width_mm"));
            mm_field(ui, &mut h, MIN_SIDE..=MAX_SIDE, crate::tl_id!("ui.height_mm"));
        });
        next.size = Size::Custom(w, h);
    } else {
        ui.separator();
        ui.label(egui::RichText::new(crate::tl_id!("ui.orientation")).strong());
        ui.horizontal(|ui| {
            if ui.selectable_label(!paper.landscape, crate::tl_id!("ui.portrait")).clicked() {
                next.landscape = false;
            }
            if ui.selectable_label(paper.landscape, crate::tl_id!("ui.landscape")).clicked() {
                next.landscape = true;
            }
        });
    }
    ui.separator();
    ui.label(egui::RichText::new(crate::tl_id!("ui.margins_mm")).strong());
    let (w, h) = paper.sheet_mm();
    let [mut top, mut right, mut bottom, mut left] = paper.margins;
    egui::Grid::new("paper-margins").num_columns(2).show(ui, |ui| {
        mm_field(ui, &mut top, 0.0..=h / 2.0, crate::tl_id!("ui.top_margin_mm"));
        mm_field(ui, &mut bottom, 0.0..=h / 2.0, crate::tl_id!("ui.bottom_margin_mm"));
        ui.end_row();
        mm_field(ui, &mut left, 0.0..=w / 2.0, crate::tl_id!("ui.left_margin_mm"));
        mm_field(ui, &mut right, 0.0..=w / 2.0, crate::tl_id!("ui.right_margin_mm"));
        ui.end_row();
    });
    next.margins = [top, right, bottom, left];
    if ui.button(crate::tl_id!("ui.restore_default_margins")).clicked() {
        next.margins = Paper::DEFAULT_MARGINS;
    }
    if next != paper {
        acts.push(Act::PaperSize(Some(next.write())));
    }
}

/// A length in millimetres, labelled for a screen reader, that can be dragged or typed.
// i18n-ui-text: label
fn mm_field(ui: &mut Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, label: &str) {
    ui.horizontal(|ui| {
        let name = ui.label(egui::RichText::new(label).size(11.5));
        ui.add(egui::DragValue::new(value).range(range).speed(0.5).max_decimals(1)).labelled_by(name.id);
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
    // The button that opens the whole gallery (issue #373): edged as part of the box, with
    // Office's "more" mark (a bar over a chevron), lit under the pointer and ringed for the keyboard,
    // so it reads as the way into the full list and not as a stray strip.
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, height), Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.0, t.faint), egui::StrokeKind::Inside);
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect.shrink(1.0), 3.0, t.row_hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    let ink = if ui.is_enabled() { t.text } else { t.muted };
    let c = egui::pos2(rect.center().x, rect.max.y - 9.0);
    painter.line_segment([egui::pos2(c.x - 4.0, c.y - 5.0), egui::pos2(c.x + 4.0, c.y - 5.0)], egui::Stroke::new(1.4, ink));
    painter.line_segment([egui::pos2(c.x - 4.0, c.y - 2.0), c], egui::Stroke::new(1.4, ink));
    painter.line_segment([c, egui::pos2(c.x + 4.0, c.y - 2.0)], egui::Stroke::new(1.4, ink));
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
        // From the keyboard, the keyboard goes into the list: to the style the caret is in, or
        // the first row.
        if open && opened_by_keys(&resp) {
            ui.data_mut(|d| d.insert_temp(first_choice_id(open_id), true));
        }
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
                        picker_title(ui, crate::tl_id!("ui.styles"), t);
                        let has_current = STYLES.iter().any(|(_, k)| *k == current);
                        for (n, (label, kind)) in STYLES.into_iter().enumerate() {
                            let row = style_row(ui, label, kind, kind == current, t);
                            if kind == current || (!has_current && n == 0) {
                                first_choice(ui, open_id, &row);
                            }
                            if row.clicked() {
                                acts.push(Act::Block(leak(kind)));
                                picked = true;
                            }
                        }
                        floating::menu_separator(ui, t);
                        if floating::menu_row(ui, Some(Glyph::Mark(Mark::ClearFormat)), crate::tl_id!("ui.clear_formatting_2"), t).clicked() {
                            acts.push(Act::ClearFormatting);
                            picked = true;
                        }
                    } else {
                        picker_title(ui, crate::tl_id!("ui.tags"), t);
                        for (n, tag) in Tag::all().into_iter().enumerate() {
                            let row = tag_row(ui, tag, t);
                            if n == 0 {
                                first_choice(ui, open_id, &row);
                            }
                            if row.clicked() {
                                acts.push(Act::Tag(tag));
                                picked = true;
                            }
                        }
                        floating::menu_separator(ui, t);
                        if floating::menu_row(ui, None, crate::tl_id!("ui.customize_tags"), t)
                            .on_hover_text(crate::tl_id!("ui.make_your_own_tags_with_a_name_symbol_and_colour"))
                            .clicked()
                        {
                            acts.push(Act::CustomizeTags);
                            picked = true;
                        }
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
        if open && opened_by_keys(&resp) {
            ui.data_mut(|d| d.insert_temp(first_choice_id(open_id), true));
        }
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
                    picker_title(ui, crate::tl_id!("ui.start_from_an_equation"), t);
                    for (name, tex) in EQUATIONS {
                        // The row is the name and the formula; the whole of it is the button.
                        let inner = ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(vec2(150.0, 20.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.add(egui::Label::new(egui::RichText::new(crate::tl!(name)).color(t.text)).selectable(false));
                            });
                            crate::mathview::inline(ui, tex, 15.0, t.text);
                        });
                        let row = ui.interact(inner.response.rect, open_id.with(("row", *name)), Sense::click());
                        first_choice(ui, open_id, &row);
                        if row.hovered() || row.has_focus() {
                            ui.painter().rect_filled(inner.response.rect.expand(2.0), 3.0, t.row_hover.gamma_multiply(0.6));
                        }
                        if row.has_focus() {
                            ui.painter().rect_stroke(
                                inner.response.rect.expand(2.0),
                                3.0,
                                egui::Stroke::new(1.5, t.accent),
                                egui::StrokeKind::Inside,
                            );
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

/// Insert > Stickers (issue #286): a grid of the stickers, each a button named for screen
/// readers. Picking one puts it on the page and closes the gallery.
fn sticker_gallery(ui: &mut Ui, t: &Tokens, acts: &mut Vec<Act>) {
    let resp = big_menu(ui, Icon::Sticker, crate::tl_id!("ui.stickers"), crate::tl_id!("ui.stickers_tip"), t);
    let open_id = egui::Id::new("ribbon-sticker-gallery");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    if resp.clicked() {
        open = !open;
        if open && opened_by_keys(&resp) {
            ui.data_mut(|d| d.insert_temp(first_choice_id(open_id), true));
        }
    }
    if open {
        let mut picked = false;
        const CELL: f32 = 56.0;
        const COLUMNS: usize = 4;
        let width = CELL * COLUMNS as f32 + 24.0;
        let at = egui::pos2(resp.rect.left().min(ui.ctx().content_rect().right() - width - 6.0).max(4.0), resp.rect.bottom());
        let area = egui::Area::new(open_id.with("area"))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    picker_title(ui, crate::tl_id!("ui.stickers_gallery"), t);
                    egui::Grid::new(open_id.with("grid")).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
                        for (n, sticker) in crate::stickers::ALL.iter().enumerate() {
                            let (rect, cell) = ui.allocate_exact_size(vec2(CELL, CELL), Sense::click());
                            first_choice(ui, open_id, &cell);
                            if cell.hovered() || cell.has_focus() {
                                ui.painter().rect_filled(rect, 6.0, t.row_hover);
                            }
                            if cell.has_focus() {
                                ui.painter().rect_stroke(rect.shrink(0.5), 6.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                            }
                            egui::Image::from_bytes(format!("bytes://sticker-{}.svg", sticker.id), sticker.svg)
                                .fit_to_exact_size(vec2(CELL - 12.0, CELL - 12.0))
                                .paint_at(ui, rect.shrink(6.0));
                            let label = crate::tl!(sticker.label);
                            let enabled = ui.is_enabled();
                            cell.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
                            if cell.on_hover_text(label).clicked() {
                                acts.push(Act::Sticker(sticker.id));
                                picked = true;
                            }
                            if n % COLUMNS == COLUMNS - 1 {
                                ui.end_row();
                            }
                        }
                    });
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
                    for (icon, label, align) in [
                        (Icon::AlignLeft, crate::tl_id!("ui.align_left_ctrl_l"), notecraft_model::justify::Align::Left),
                        (Icon::AlignCenter, crate::tl_id!("ui.center"), notecraft_model::justify::Align::Center),
                        (Icon::AlignRight, crate::tl_id!("ui.align_right_ctrl_r"), notecraft_model::justify::Align::Right),
                    ] {
                        if floating::menu_row(ui, Some(Glyph::Icon(icon)), label, t).clicked() {
                            acts.push(Act::Align(align));
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
                    if floating::menu_row(ui, None, crate::tl_id!("ui.paragraph_spacing_options"), t).clicked() {
                        acts.push(Act::ParagraphSpacing);
                        picked = true;
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

/// How a style's row in the gallery is drawn (issue #373): the page's own sizes for its headings
/// and words, brought down to fit a row by one scale, so Heading 1 is the biggest, every level
/// is smaller than the one before, and Normal is the size of ordinary words. Headings take the
/// page's heading colour, as they do on the page.
fn style_look(kind: &str, t: &Tokens) -> (f32, Color32, bool) {
    let fit = |page: f32| 11.5 + (page - crate::preview::BODY_SIZE) * 0.42;
    match kind {
        "title" => (fit(30.0), t.strong, false),
        "h1" => (fit(crate::preview::heading_size(1)), t.strong, false),
        "h2" => (fit(crate::preview::heading_size(2)), t.strong, false),
        "h3" => (fit(crate::preview::heading_size(3)), t.strong, false),
        // The page draws levels 4 to 6 at one size; the gallery keeps them apart by a step.
        "h4" => (fit(crate::preview::heading_size(4)), t.strong, false),
        "h5" => (fit(crate::preview::heading_size(5)) - 0.25, t.strong, false),
        "h6" => (fit(crate::preview::heading_size(6)) - 0.5, t.strong, false),
        "quote" | "citation" => (fit(crate::preview::BODY_SIZE), t.muted, true),
        "codeblock" | "codenumbers" => (fit(crate::preview::BODY_SIZE), t.text, false),
        _ => (fit(crate::preview::BODY_SIZE), t.text, false),
    }
}

/// One line of the Styles gallery, drawn the way the style looks.
// i18n-ui-text: label
fn style_row(ui: &mut Ui, label: &str, kind: &str, current: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let (size, color, italics) = style_look(kind, t);
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
    } else if resp.hovered() || resp.has_focus() {
        ui.painter().rect_filled(rect, 3.0, t.row_hover);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
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
    if resp.hovered() || resp.has_focus() {
        painter.rect_filled(rect, 3.0, t.row_hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 3.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    paint_tag(&painter, Rect::from_min_size(egui::pos2(rect.min.x + 2.0, rect.center().y - 9.0), vec2(18.0, 18.0)), tag, t.text, t.accent);
    // Issue #373: icon, name and shortcut in three columns, the shortcut right-aligned in the quiet
    // colour as a menu draws it, so the names line up and the keys can be scanned on their own.
    // A screen reader still hears the name with its keys.
    let name_text = crate::tl!(tag.label());
    let keys = tag.shortcut();
    let keys_w = keys.map_or(0.0, |k| painter.layout_no_wrap(k.to_string(), egui::FontId::proportional(11.5), t.muted).size().x + 8.0);
    let name_room = (rect.width() - 26.0 - keys_w - 4.0).max(20.0);
    painter.with_clip_rect(Rect::from_min_size(egui::pos2(rect.min.x + 26.0, rect.min.y), vec2(name_room, rect.height()))).text(
        egui::pos2(rect.min.x + 26.0, rect.center().y),
        Align2::LEFT_CENTER,
        name_text,
        egui::FontId::proportional(12.5),
        t.text,
    );
    if let Some(keys) = keys {
        painter.text(egui::pos2(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER, keys, egui::FontId::proportional(11.5), t.muted);
    }
    let label = tag_label(tag);
    let enabled = ui.is_enabled();
    let name = label;
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    resp
}

/// OneNote's mark for a tag. A custom tag has no mark of its own; it shows its symbol.
pub fn mark_for(tag: Tag) -> Option<Mark> {
    Some(match tag {
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
        Tag::Custom(_) => return None,
    })
}

/// Draws a tag's mark in `rect`: OneNote's own for a built-in tag, the person's symbol in its
/// colour for one they defined (issue #259).
pub fn paint_tag(painter: &egui::Painter, rect: Rect, tag: Tag, text: Color32, accent: Color32) {
    match (mark_for(tag), tag.custom_look()) {
        (Some(mark), _) => ribbon_icons::paint(painter, rect, mark, mark.color(text, accent)),
        (None, Some((symbol, colour))) => paint_symbol(painter, rect, symbol, colour),
        (None, None) => {}
    }
}

/// A custom tag's symbol in its colour; a plain dot when it has none.
pub fn paint_symbol(painter: &egui::Painter, rect: Rect, symbol: &str, colour: notecraft_model::NodeColor) {
    let (r, g, b) = colour.rgb(painter.ctx().global_style().visuals.dark_mode);
    let colour = Color32::from_rgb(r, g, b);
    if symbol.is_empty() {
        painter.circle_filled(rect.center(), rect.height() * 0.3, colour);
    } else {
        painter.text(rect.center(), Align2::CENTER_CENTER, symbol, egui::FontId::proportional(rect.height() * 0.9), colour);
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
    small_back(&painter, rect, &resp, on, t);
    let left = Rect::from_center_size(egui::pos2(rect.min.x + 11.0, rect.center().y), vec2(15.0, 15.0));
    ribbon_icons::paint(&painter, left, mark, mark.color(t.text, t.accent));
    painter.text(egui::pos2(rect.min.x + 23.0, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    let enabled = ui.is_enabled();
    let name = label.to_string();
    resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, on, name.clone()));
    resp.on_hover_text(tip)
}

/// Home's Font Colour (`highlight` false) or Text Highlight Colour palette (issue #260): a
/// small button with the letter or the marker and the colour now in use, which opens the
/// palette: Automatic or No Colour first, then the colours by name.
fn colour_palette(ui: &mut Ui, highlight: bool, current: Option<Option<&str>>, mixed: bool, t: &Tokens, acts: &mut Vec<Act>) {
    let (label, tip) = if highlight {
        (crate::tl_id!("ui.text_highlight_colour"), crate::tl_id!("ui.text_highlight_colour_tip"))
    } else {
        (crate::tl_id!("ui.font_colour"), crate::tl_id!("ui.font_colour_tip"))
    };
    let (rect, resp) = ui.allocate_exact_size(vec2(SMALL_W, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    small_back(&painter, rect, &resp, false, t);
    let t = &if ui.is_enabled() { *t } else { Tokens { text: t.muted, ..*t } };
    let bar_colour = current.flatten().and_then(crate::textcolour::parse);
    let glyph = rect.center() - vec2(0.0, 3.0);
    if highlight {
        // A marker nib: a slanted block over the bar.
        let nib = [glyph + vec2(-5.0, 4.0), glyph + vec2(1.0, -6.0), glyph + vec2(5.0, -3.0), glyph + vec2(-1.0, 7.0)];
        painter.add(egui::Shape::convex_polygon(nib.to_vec(), egui::Color32::TRANSPARENT, egui::Stroke::new(1.3, t.text)));
    } else {
        painter.text(glyph, Align2::CENTER_CENTER, "A", egui::FontId::proportional(13.0), t.text);
    }
    let bar = Rect::from_center_size(egui::pos2(rect.center().x, rect.max.y - 5.0), vec2(14.0, 3.5));
    match bar_colour {
        Some(c) => {
            painter.rect_filled(bar, 1.0, c);
        }
        None => {
            painter.rect_stroke(bar, 1.0, egui::Stroke::new(1.0, t.muted), egui::StrokeKind::Inside);
        }
    }
    let name = label.to_string();
    let enabled = ui.is_enabled();
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, name.clone()));
    let resp = resp.on_hover_text(tip);
    egui::Popup::menu(&resp).id(ui.id().with(("colour-palette", highlight))).show(|ui| {
        ui.set_min_width(180.0);
        let none = if highlight { crate::tl_id!("ui.no_colour") } else { crate::tl_id!("ui.automatic_colour") };
        if ui.selectable_label(current == Some(None), none).clicked() {
            acts.push(if highlight { Act::TextHighlight(None) } else { Act::TextColour(None) });
            ui.close();
        }
        let list = if highlight { crate::textcolour::highlights().to_vec() } else { crate::textcolour::font_colours().to_vec() };
        // The name of the colour under the pointer or the keyboard, else of the one in use, is
        // written under the swatches (issue #373), so a colour is picked by name as well as by look.
        let mut named: Option<&str> = list.iter().find(|(_, hex)| current.flatten() == Some(*hex)).map(|(name, _)| *name);
        egui::Grid::new(("colour-grid", highlight)).spacing(vec2(4.0, 4.0)).show(ui, |ui| {
            for (i, (name, hex)) in list.iter().enumerate() {
                let chosen = current.flatten() == Some(*hex);
                let (r, swatch) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
                let fill = crate::textcolour::parse(hex).unwrap_or(t.text);
                ui.painter().rect_filled(r.shrink(2.0), 3.0, fill);
                let edge = if chosen || swatch.hovered() || swatch.has_focus() { t.accent } else { t.container_border };
                ui.painter().rect_stroke(r.shrink(1.0), 3.0, egui::Stroke::new(if chosen { 2.0 } else { 1.0 }, edge), egui::StrokeKind::Inside);
                let n = (*name).to_string();
                swatch.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, true, chosen, n.clone()));
                if swatch.hovered() || swatch.has_focus() {
                    named = Some(name);
                }
                if swatch.on_hover_text(*name).clicked() {
                    let value = Some((*hex).to_string());
                    acts.push(if highlight { Act::TextHighlight(value) } else { Act::TextColour(value) });
                    ui.close();
                }
                if i % 5 == 4 {
                    ui.end_row();
                }
            }
        });
        ui.add_space(2.0);
        // With nothing named: a colour outside the palette by its value, a selection of more than
        // one colour as Mixed, and otherwise the default.
        let caption = match (named, current) {
            (Some(n), _) => crate::tl!(n).to_owned(),
            (None, Some(Some(hex))) => hex.to_owned(),
            (None, None) if mixed => crate::tl_id!("ui.mixed").to_owned(),
            _ => (if highlight { crate::tl_id!("ui.no_colour") } else { crate::tl_id!("ui.automatic_colour") }).to_owned(),
        };
        // Painted, not a label: the swatch already carries the name for a screen reader.
        let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
        ui.painter().text(line.left_center(), Align2::LEFT_CENTER, caption, egui::FontId::proportional(12.0), t.text);
    });
}

// i18n-ui-text: label,tip
fn mark_button(ui: &mut Ui, mark: Mark, label: &str, tip: &str, on: bool, t: &Tokens) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let (rect, resp) = ui.allocate_exact_size(vec2(SMALL_W, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    small_back(&painter, rect, &resp, on, t);
    let colour = if ui.is_enabled() { mark.color(t.text, t.accent) } else { t.muted };
    ribbon_icons::paint(&painter, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), mark, colour);
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
        // OneNote's tool: a drag on the page adds room there, or takes it away (issue #164).
        if big_on(ui, Icon::InsertSpace, crate::tl_id!("ui.insert_space_2"), crate::tl_id!("ui.insert_space_tip"), view.space_tool, t).clicked() {
            acts.push(Act::InsertSpaceTool);
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
            // Built, but GridCraft is what makes the spreadsheet: without it the button is
            // unavailable and says why, as Table > Convert to GridCraft does (#306).
            let dim = Tokens { accent: t.muted, text: t.muted, ..*t };
            let resp = ui
                .add_enabled_ui(false, |ui| {
                    big(ui, Icon::Spreadsheet, crate::tl_id!("ui.spreadsheet"), crate::tl_id!("ui.gridcraft_is_not_installed"), &dim)
                })
                .inner;
            resp.on_disabled_hover_text(crate::tl_id!("ui.spreadsheet_gridcraft_is_not_installed"));
        }
    });
    group(ui, crate::tl_id!("ui.images"), t, |ui| {
        soon(ui, Icon::Clip, "Screen\nClipping", t);
        if pictures_menu(ui, t, acts) {
            acts.push(Act::AddPicture);
        }
    });
    group(ui, crate::tl_id!("ui.media"), t, |ui| {
        if big(ui, Icon::Video, crate::tl_id!("ui.online_video_2"), crate::tl_id!("ui.online_video_tip"), t).clicked() {
            acts.push(Act::Insert("online-video"));
        }
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
    // Issue #374: three similar stamps as one compact stack rather than three large buttons, each
    // saying on its tip exactly what it puts on the page now.
    group(ui, crate::tl_id!("ui.time_stamp"), t, |ui| {
        let now = notecraft_model::time::iso_now();
        let tip = |what: &str, example: Option<String>| {
            crate::i18n::format_message_id("ui.stamp_tip_example", &[("what", crate::tl!(what)), ("example", &example.unwrap_or_default())])
        };
        rows(ui, |ui| {
            let date_tip = tip(crate::tl_id!("ui.today_s_date"), notecraft_model::time::date_from_iso(&now));
            if small_labeled_icon(ui, Icon::Calendar, crate::tl_id!("ui.date"), &date_tip, t).clicked() {
                acts.push(Act::Insert("date"));
            }
            let time_tip = tip(crate::tl_id!("ui.the_time_now"), notecraft_model::time::clock_from_iso(&now));
            if small_labeled_icon(ui, Icon::Clock, crate::tl_id!("ui.time"), &time_tip, t).clicked() {
                acts.push(Act::Insert("time"));
            }
            let both_tip = tip(crate::tl_id!("ui.both_as_the_page_s_own_stamp_is_written"), notecraft_model::time::long_from_iso(&now));
            if small_labeled_icon(ui, Icon::Calendar, crate::tl_id!("ui.date_and_time"), &both_tip, t).clicked() {
                acts.push(Act::Insert("datetime"));
            }
        });
    });
    group(ui, crate::tl_id!("ui.pages"), t, |ui| {
        // It opens the list of templates beside the page, so it carries the chevron of every
        // other button that opens a choice rather than acting at once (issue #374).
        if big_menu(ui, Icon::PageStack, crate::tl_id!("ui.page_templates"), crate::tl_id!("ui.page_templates_tip"), t).clicked() {
            acts.push(Act::PageTemplates);
        }
    });
    group(ui, crate::tl_id!("ui.symbols"), t, |ui| {
        equation_gallery(ui, t, acts);
        let symbol = big_menu(ui, Icon::Symbol, crate::tl_id!("ui.symbol"), crate::tl_id!("ui.insert_a_common_symbol"), t);
        if symbol.clicked() {
            ui.data_mut(|d| d.insert_temp(symbols_button_id(), symbol.id));
            acts.push(Act::Symbols);
        }
    });
    group(ui, crate::tl_id!("ui.more_2"), t, |ui| {
        // NoteCraft's own addition to OneNote's group: a diagram kept as its source, so it
        // can be read and changed in any editor rather than being a picture of one.
        if big(ui, Icon::Diagram, crate::tl_id!("ui.diagram"), crate::tl_id!("ui.a_mermaid_diagram_kept_as_its_source"), t).clicked() {
            acts.push(Act::Insert("diagram"));
        }
        sticker_gallery(ui, t, acts);
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

/// The Rule Line Colour row of the paper gallery (issue #288): Automatic (the theme's own lines)
/// and the named colours, each swatch drawn as a short rule in the colour the lines will be.
fn rule_colour_palette(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) -> bool {
    let mut picked = false;
    ui.label(egui::RichText::new(crate::tl_id!("ui.rule_line_colour")).strong().color(t.text));
    ui.add_enabled_ui(view.has_page, |ui| {
        ui.horizontal(|ui| {
            for colour in std::iter::once(None).chain(notecraft_model::NodeColor::ALL.into_iter().map(Some)) {
                let here = view.rule_colour == colour;
                let side = 22.0;
                let (rect, resp) = ui.allocate_exact_size(vec2(side, side), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect, 3.0, t.editor);
                let (_, strong) = crate::theme::rule_lines_in(t, colour);
                for k in 1..4 {
                    #[allow(clippy::cast_precision_loss)]
                    let y = rect.min.y + side * k as f32 / 4.0;
                    painter.hline(rect.min.x + 3.0..=rect.max.x - 3.0, y, egui::Stroke::new(1.5, strong));
                }
                let (edge, width) = if here { (t.accent, 2.0) } else { (t.faint, 1.0) };
                painter.rect_stroke(rect, 3.0, egui::Stroke::new(width, edge), egui::StrokeKind::Inside);
                // Named apart from the page colour swatches beside them: "Blue lines".
                let name = match colour {
                    Some(c) => crate::i18n::format_message_id("ui.colour_lines", &[("colour", crate::tl!(c.label()))]),
                    None => crate::tl_id!("ui.automatic_rule_lines").to_string(),
                };
                let enabled = ui.is_enabled();
                let said = name.clone();
                resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, here, &said));
                if resp.on_hover_text(name).clicked() {
                    acts.push(Act::RuleColour(colour));
                    picked = true;
                }
            }
        });
    });
    picked
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
    // The size the grid shows: the last square pointed at or stepped to with the arrows. It stays
    // when the pointer leaves the grid, so the readout above it does not flick back to "Table"
    // (issue #374).
    let cursor_id = open_id.with("cursor");
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id).unwrap_or(false));
    let mut cursor = ui.data(|d| d.get_temp::<(usize, usize)>(cursor_id));
    let opening = resp.clicked() && !open;
    if resp.clicked() {
        open = !open;
    }
    if !open {
        ui.data_mut(|d| {
            d.insert_temp(open_id, open);
            d.remove::<(usize, usize)>(cursor_id);
        });
        return;
    }
    if opening {
        cursor = Some((0, 0));
    }
    // The arrows step the size, Enter takes it, Escape goes back to the Table button.
    let (mut down, mut right) = (0i32, 0i32);
    ui.input_mut(|i| {
        for (key, dr, dc) in [(egui::Key::ArrowDown, 1, 0), (egui::Key::ArrowUp, -1, 0), (egui::Key::ArrowRight, 0, 1), (egui::Key::ArrowLeft, 0, -1)]
        {
            if i.consume_key(egui::Modifiers::NONE, key) {
                down += dr;
                right += dc;
            }
        }
    });
    let stepped = down != 0 || right != 0;
    if stepped {
        let (r, c) = cursor.unwrap_or((0, 0));
        #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
        let step = |at: usize, by: i32| (at as i32 + by).clamp(0, MOST as i32 - 1) as usize;
        cursor = Some((step(r, down), step(c, right)));
    }
    let mut picked = false;
    let area = egui::Area::new(open_id.with("area"))
        .order(egui::Order::Foreground)
        .fixed_pos(resp.rect.left_bottom())
        .constrain(true)
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
                let mut focused_now = None;
                let mut chose = None;
                // While a square has the keyboard, pointing at another moves the keyboard to it, so
                // Enter takes the size the readout shows.
                let grid_has_keys = ui.memory(|m| m.focused()).is_some_and(|f| (0..MOST).any(|r| (0..MOST).any(|c| f == open_id.with((r, c)))));
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
                            if grid_has_keys && !resp.has_focus() && cursor != Some((r, c)) {
                                resp.request_focus();
                            }
                        }
                        // A square reached with Tab is the size in hand, as one stepped to is.
                        if resp.gained_focus() && !opening && !stepped {
                            focused_now = Some((r, c));
                        }
                        if (opening || stepped) && cursor == Some((r, c)) {
                            resp.request_focus();
                        }
                        // The arrows are the grid's own: egui is told not to move the keyboard to
                        // a neighbour with them as well, or each step went one square too far.
                        if resp.has_focus() {
                            let arrows = egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, ..Default::default() };
                            ui.memory_mut(|m| m.set_focus_lock_filter(resp.id, arrows));
                        }
                        if resp.clicked() {
                            chose = Some((r, c));
                        }
                        cells.push((r, c, cell));
                    }
                }
                if let Some(at) = over.or(focused_now) {
                    cursor = Some(at);
                }
                let paint = ui.painter();
                for (r, c, cell) in cells {
                    let lit = cursor.is_some_and(|(hr, hc)| r <= hr && c <= hc);
                    let (fill, line) = if lit { (t.accent.gamma_multiply(0.25), t.accent) } else { (t.editor, t.table_line) };
                    paint.rect_filled(cell, 1.0, fill);
                    paint.rect_stroke(cell, 1.0, egui::Stroke::new(1.0, line), egui::StrokeKind::Inside);
                }
                let title = match cursor {
                    Some((r, c)) => crate::i18n::format_message_id(
                        "ui.columns_x_rows_table",
                        &[("columns", &(c + 1).to_string()), ("rows", &(r + 1).to_string())],
                    ),
                    None => crate::tl_id!("ui.table_3").to_string(),
                };
                paint.text(naming.left_top(), egui::Align2::LEFT_TOP, &title, egui::FontId::proportional(13.0), t.text);
                // The readout is read out too, so a screen reader hears the size as it changes.
                let readout = ui.interact(naming, open_id.with("readout"), Sense::hover());
                readout.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, true, &title));
                if let Some((r, c)) = chose {
                    acts.push(Act::InsertTable(r + 1, c + 1));
                    picked = true;
                }
                ui.separator();
                if floating::menu_row(ui, Some(Glyph::Icon(Icon::Table)), crate::tl_id!("ui.insert_table_2"), t).clicked() {
                    acts.push(Act::InsertTableDialog);
                    picked = true;
                }
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
    let escaped = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escaped {
        // Back where the grid was opened from, so the keyboard carries on from there.
        resp.request_focus();
    }
    if picked || clicked_away || escaped {
        open = false;
    }
    ui.data_mut(|d| {
        d.insert_temp(open_id, open);
        match cursor.filter(|_| open) {
            Some(at) => {
                d.insert_temp(cursor_id, at);
            }
            None => d.remove::<(usize, usize)>(cursor_id),
        }
    });
}

/// OneNote's Rule Lines button: the paper now, and a gallery of every paper under it.
/// A small row that names a setting, shows what it is set to and opens its choices (issue #376):
/// a little picture of the value, the setting's name, the value in the quiet colour, and the
/// chevron a menu button has. A screen reader hears the name and then the value.
// i18n-ui-text: label,tip
fn value_row(ui: &mut Ui, label: &str, value: &str, tip: &str, open: bool, t: &Tokens, preview: impl FnOnce(&egui::Painter, Rect)) -> egui::Response {
    let label = crate::tl!(label);
    let tip = crate::tl!(tip);
    let font = egui::FontId::proportional(12.0);
    let quiet = egui::FontId::proportional(11.5);
    let name_w = ui.painter().layout_no_wrap(label.to_string(), font.clone(), t.text).size().x;
    let value_w = ui.painter().layout_no_wrap(value.to_string(), quiet.clone(), t.muted).size().x;
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0 + name_w + 6.0 + value_w + 16.0, SMALL_H), Sense::click());
    let painter = ui.painter_at(rect);
    if open {
        painter.rect_filled(rect, 4.0, t.row_selected);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }
    if resp.has_focus() {
        painter.rect_stroke(rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
    }
    preview(&painter, Rect::from_center_size(egui::pos2(rect.min.x + 11.0, rect.center().y), vec2(16.0, 14.0)));
    let (text, quiet_colour) = if ui.is_enabled() { (t.text, t.muted) } else { (t.muted, t.faint) };
    painter.text(egui::pos2(rect.min.x + 24.0, rect.center().y), Align2::LEFT_CENTER, label, font, text);
    painter.text(egui::pos2(rect.min.x + 24.0 + name_w + 6.0, rect.center().y), Align2::LEFT_CENTER, value, quiet, quiet_colour);
    chevron(&painter, egui::pos2(rect.max.x - 7.0, rect.center().y - 1.0), quiet_colour);
    let enabled = ui.is_enabled();
    let (name, said) = (label.to_string(), value.to_string());
    resp.widget_info(move || {
        let mut info = egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &name);
        info.current_text_value = Some(said.clone());
        info
    });
    resp.on_hover_text(tip)
}

/// View > Page Setup > Page colour, showing the colour the page has.
fn page_colour_row(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    let value = view.page_colour.map_or_else(|| crate::tl_id!("ui.no_colour").to_string(), |c| crate::tl!(c.label()).to_string());
    let popup_id = ui.id().with("page-colour-row");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let dark = t.editor.r() < 128;
    let resp = value_row(ui, crate::tl_id!("ui.page_colour"), &value, crate::tl_id!("ui.page_colour_tip"), open, t, |painter, area| {
        let fill = view.page_colour.map_or(t.editor, |c| {
            let (r, g, b) = c.rgb(dark);
            egui::Color32::from_rgb(r, g, b)
        });
        painter.rect_filled(area, 2.0, fill);
        painter.rect_stroke(area, 2.0, egui::Stroke::new(1.0, t.muted), egui::StrokeKind::Inside);
        if view.page_colour.is_none() {
            painter.line_segment([area.left_bottom() + vec2(3.0, -3.0), area.right_top() + vec2(-3.0, 3.0)], egui::Stroke::new(1.0, t.muted));
        }
    });
    egui::Popup::menu(&resp).id(popup_id).show(|ui| {
        if page_colour_palette(ui, view, t, acts) {
            ui.close();
        }
    });
}

/// View > Page Setup > Rule lines, showing the paper the page is on.
fn rule_lines_row(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    let popup_id = ui.id().with("rule-lines-row");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let rules = view.rules;
    let resp = value_row(
        ui,
        crate::tl_id!("ui.rule_lines"),
        rules.translated_label(),
        crate::tl_id!("ui.lines_or_squares_behind_the_page_as_on_paper"),
        open,
        t,
        |painter, area| {
            painter.rect_filled(area, 1.0, t.editor);
            paint_paper(&painter.with_clip_rect(area), area, rules, 0.3, t.rule_line_strong, t.rule_line_strong);
            painter.rect_stroke(area, 1.0, egui::Stroke::new(1.0, t.muted), egui::StrokeKind::Inside);
        },
    );
    egui::Popup::menu(&resp).id(popup_id).show(|ui| {
        let mut picked = false;
        for papers in [&[RuleLines::None][..], &RuleLines::RULED[..], &RuleLines::GRIDS[..], &RuleLines::CRAFT[..]] {
            ui.horizontal(|ui| {
                for r in papers {
                    if paper_swatch(ui, *r, view.rules == *r, t).clicked() {
                        acts.push(Act::RuleLines(*r));
                        picked = true;
                    }
                }
            });
        }
        ui.separator();
        picked |= rule_colour_palette(ui, view, t, acts);
        if picked {
            ui.close();
        }
    });
}

/// What a page's paper size is called on its row: Auto, the sheet and which way up, or a custom
/// size in millimetres.
pub fn paper_value(paper: Option<notecraft_model::paper::Paper>) -> String {
    use notecraft_model::paper::Size;
    match paper {
        None => crate::tl_id!("ui.auto").to_string(),
        Some(p) => match p.size {
            Size::Custom(w, h) => crate::i18n::format_message_id("ui.custom_paper_mm", &[("w", &format!("{w:.0}")), ("h", &format!("{h:.0}"))]),
            size => {
                let turn = if p.landscape { crate::tl_id!("ui.landscape") } else { crate::tl_id!("ui.portrait") };
                format!("{}, {turn}", crate::tl!(size.label()))
            }
        },
    }
}

/// View > Page Setup > Paper size, showing the sheet the page is written for.
fn paper_size_row(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    let popup_id = ui.id().with("paper-size-row");
    let open = egui::Popup::is_id_open(ui.ctx(), popup_id);
    let paper = view.paper_size;
    let resp = value_row(
        ui,
        crate::tl_id!("ui.paper_size"),
        &paper_value(paper),
        crate::tl_id!("ui.the_sheet_this_page_is_written_for_its_size_turn_and_margins"),
        open,
        t,
        |painter, area| {
            // A sheet the shape of the paper, or a dashed edge for Auto, which has none.
            let sheet = paper.map_or(area.shrink2(vec2(3.0, 1.0)), |p| {
                let (w, h) = p.sheet_mm();
                let scale = (area.width() / w).min(area.height() / h);
                Rect::from_center_size(area.center(), vec2(w * scale, h * scale))
            });
            painter.rect_filled(sheet, 1.0, t.editor);
            if paper.is_some() {
                painter.rect_stroke(sheet, 1.0, egui::Stroke::new(1.0, t.muted), egui::StrokeKind::Inside);
            } else {
                for side in [
                    [sheet.left_top(), sheet.right_top()],
                    [sheet.right_top(), sheet.right_bottom()],
                    [sheet.right_bottom(), sheet.left_bottom()],
                    [sheet.left_bottom(), sheet.left_top()],
                ] {
                    painter.add(egui::Shape::dashed_line(&side, egui::Stroke::new(1.0, t.muted), 2.0, 2.0));
                }
            }
        },
    );
    // It stays open while sizes are tried and margins dragged; a click away closes it.
    egui::Popup::menu(&resp)
        .id(popup_id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| paper_size_menu(ui, paper, t, acts));
}

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
            .constrain(true)
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
                    picked |= rule_colour_palette(ui, view, t, acts);
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
        ui.add_space(2.0);
        // Issue #375: one named control. Its heading says whether pressure is on, which depends on
        // the pen, not on a setting: "Pressure off" until a pen's force readings arrive. Why it is
        // off, for this system, is a button away rather than only on a hover.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (label, colour) =
                if pressure_available { (crate::tl_id!("ui.pressure_on"), t.text) } else { (crate::tl_id!("ui.pressure_off"), t.muted) };
            ui.label(egui::RichText::new(label).size(10.5).color(colour));
            if !pressure_available {
                let why =
                    ui.add(egui::Button::new(egui::RichText::new(crate::tl_id!("ui.pressure_why")).size(10.5).color(t.accent)).frame(false).small());
                egui::Popup::menu(&why).id(ui.id().with("pressure-why")).show(|ui| {
                    ui.set_max_width(280.0);
                    ui.label(pressure_unavailable_tip());
                });
            }
        });
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
        if resp.has_focus() {
            painter.rect_stroke(rect.shrink(0.5), 4.0, egui::Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
        }

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
        // The arrow keys move it five at a time, as a slider does, once it has the keyboard.
        if pressure_available && resp.has_focus() {
            let step = ui.input(|i| i32::from(i.key_pressed(egui::Key::ArrowRight)) - i32::from(i.key_pressed(egui::Key::ArrowLeft)));
            if step != 0 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let picked = (i32::from(now) + step * 5).clamp(0, 100) as u8;
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
        if pressure_available {
            resp.on_hover_text(crate::tl_id!("ui.pressure_sensitivity_tip"));
        } else {
            resp.on_hover_text(pressure_unavailable_tip());
        }
        // The value only when there is one: "Pressure off" already says it is off.
        if pressure_available {
            ui.label(egui::RichText::new(format!("{now}")).size(11.5).color(t.text));
        }
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
                    // Not a tool for the rack: it draws nothing, so it is picked up here and put
                    // down with Escape or any other tool (issue #289).
                    let (row, hit) = ui.allocate_exact_size(vec2(162.0, 28.0), Sense::click());
                    let painter = ui.painter_at(row);
                    if hit.hovered() {
                        painter.rect_filled(row, 4.0, t.row_hover);
                    }
                    let dot = egui::pos2(row.min.x + 13.0, row.center().y);
                    painter.circle_filled(dot, 7.0, crate::inkpad::LASER.gamma_multiply(0.3));
                    painter.circle_filled(dot, 4.0, crate::inkpad::LASER);
                    painter.text(
                        egui::pos2(row.min.x + 32.0, row.center().y),
                        Align2::LEFT_CENTER,
                        crate::tl_id!("ui.laser_pointer"),
                        egui::FontId::proportional(12.5),
                        t.text,
                    );
                    let enabled = ui.is_enabled();
                    hit.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, crate::tl_id!("ui.laser_pointer")));
                    if hit.on_hover_text(crate::tl_id!("ui.point_at_the_page_without_drawing_on_it")).clicked() {
                        acts.push(Act::InkPointer(Pointer::Laser));
                        picked = true;
                    }
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
    // Escape closes it and gives the keyboard back to the button it came from (issue #374).
    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
    if escaped {
        button.request_focus();
    }
    out || escaped
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
    let (stand, band) = rack_spots(rect);
    paint_tool(&painter, stand, nib.tool, nib.colour, drop);
    let dark = crate::theme::luminance(t.panel) < crate::inkpad::DARK_PAGE;
    crate::inkpad::paint_stroke(&painter, band.min, &rack_swatch(nib, band, dark));
    if chosen {
        chosen_badge(&painter, rect, t);
    }

    // The chevron on the tool in hand opens its editor, as OneNote's does; a right click
    // opens the same editor and the rest of the rack's own menu. It is a control of its own,
    // named for the tool, so picking the tool up and opening its properties can each be done
    // from the keyboard (issue #375).
    let mut picked_up = resp.clicked();
    let chev = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.max.y - 5.0), vec2(14.0, 11.0));
    let mut modify = false;
    if chosen {
        let hit = ui.interact(chev, resp.id.with("chevron"), Sense::click());
        chevron(&painter, chev.center(), if hit.hovered() || hit.has_focus() { t.accent } else { t.muted });
        let said = crate::i18n::format_message_id("ui.modify_named_tool", &[("tool", &name)]);
        let tip = said.clone();
        hit.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, &said));
        if hit.on_hover_text(tip).clicked() {
            modify = true;
            picked_up = false;
        }
    }
    // The menu hangs off the pen's own response. A second interact over the same
    // rectangle looked harmless and was not: later widgets win in egui, so it swallowed
    // every left click and only the eraser, which had no such overlay, could be picked up.
    floating::row_menu(&resp, |ui| {
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
    // The click that opened it (Modify Tool in the right-click menu lies outside the popup)
    // must not count as a click away from it, or it closed the moment it opened.
    if open && modify_tool(ui, open_id, rect, i, nib, rack_len, t, acts) && !modify {
        open = false;
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    // The first nine pens answer Alt and their number (issue #44).
    let name = if i < 9 { format!("{name} (Alt+{})", i + 1) } else { name };
    resp.on_hover_text(name);
    picked_up
}

/// The tick in the corner of the tool in hand (issue #375): with the outline, it marks the chosen
/// tool by shape as well as by colour, so two tools of one colour, or a colour close to the
/// accent, still read apart.
fn chosen_badge(painter: &egui::Painter, slot: Rect, t: &Tokens) {
    let c = egui::pos2(slot.max.x - 5.5, slot.min.y + 5.5);
    painter.circle_filled(c, 4.6, t.accent);
    painter.add(egui::Shape::line(vec![c + vec2(-2.2, 0.1), c + vec2(-0.6, 1.8), c + vec2(2.3, -1.6)], egui::Stroke::new(1.3, t.panel)));
}

/// How tall the strip under a pen on the rack is, where the stroke it makes is drawn.
const RACK_SWATCH_H: f32 = 10.0;

/// Where in a rack slot the tool stands, and the strip under it for its stroke. The strip
/// sits above the chevron the tool in hand shows, so neither covers the other; the eraser
/// stands in the same place so the rack keeps one line.
fn rack_spots(slot: Rect) -> (Rect, Rect) {
    let band = Rect::from_min_size(egui::pos2(slot.min.x + 3.0, slot.max.y - 12.0 - RACK_SWATCH_H), vec2(slot.width() - 6.0, RACK_SWATCH_H));
    let stand = Rect::from_min_max(slot.min, egui::pos2(slot.max.x, band.min.y - 2.0));
    (stand, band)
}

/// The short stroke drawn under a pen on the rack: its own tool and colour, and a width
/// that grows with the pen's (issue #375). Two black pens of different sizes look alike
/// standing up; their strokes do not.
///
/// Fine pens are drawn at their own width. A broad one cannot be, in a strip ten points
/// tall, so above three points the width grows by the square root of the rest, scaled so the
/// broadest pen fills the strip: every size still draws wider than the one below it, and
/// none spills out. `dark` is a
/// dark ribbon, where black ink is drawn light, as a dark page draws it.
fn rack_swatch(nib: Nib, band: Rect, dark: bool) -> notecraft_ink::Stroke {
    let w = nib.width();
    // The root is scaled so the broadest pen a rack can hold just fills the strip; capping it
    // instead drew every pen past about 28 pt alike.
    let (exact, room) = (3.0, RACK_SWATCH_H - 2.0);
    let widest = f32::from(rack::MAX_TENTHS) / 10.0;
    let shown = if w <= exact { w } else { exact + (room - exact) * ((w - exact) / (widest - exact)).sqrt().min(1.0) };
    // On a dark ribbon black ink is shown light, as a dark page shows it.
    let mut stroke = notecraft_ink::Stroke::new(nib.tool, crate::inkpad::shown_colour(nib.colour, dark), shown);
    let (left, right) = (shown / 2.0 + 1.0, band.width() - shown / 2.0 - 1.0);
    let mid = band.height() / 2.0;
    // A slight wave, so it reads as a stroke and not a rule; the swing is kept inside the strip.
    let swing = ((band.height() - shown) / 2.0 - 0.5).clamp(0.0, 1.5);
    for k in 0..=6u8 {
        let at = f32::from(k) / 6.0;
        stroke.dabs.push(notecraft_ink::Dab::new(left + (right - left) * at, mid - (at * std::f32::consts::TAU).sin() * swing, 1.0));
    }
    stroke
}

/// What to say when no pressure has reached the window, by what this system needs for it.
fn pressure_unavailable_tip() -> &'static str {
    if cfg!(target_os = "windows") {
        crate::tl_id!("ui.pressure_unavailable_tip_windows")
    } else if cfg!(target_os = "linux") {
        crate::tl_id!("ui.pressure_unavailable_tip_linux")
    } else {
        crate::tl_id!("ui.pressure_unavailable_tip_other")
    }
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
        .constrain(true)
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

                // Each heading says what is chosen now, so the dots and swatches need not be read by
                // their ring alone (issue #375).
                let size_now = crate::i18n::format_message_id("ui.size_pt", &[("size", &format!("{:.1}", nib.width()))]);
                ui.label(egui::RichText::new(crate::i18n::format_message_id("ui.thickness_now", &[("size", &size_now)])).size(11.5).color(t.muted));
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
                ui.label(
                    egui::RichText::new(crate::i18n::format_message_id("ui.colour_now", &[("colour", crate::tl!(swatch_name(nib.colour)))]))
                        .size(11.5)
                        .color(t.muted),
                );
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
                            let tip = name.clone();
                            resp.widget_info(move || egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, here, name.clone()));
                            if resp.on_hover_text(tip).clicked() {
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
                if popup_action_button(ui, crate::tl_id!("ui.eyedropper"), true, t.accent, crate::tl_id!("ui.take_a_colour_from_ink_on_the_page"))
                    .clicked()
                {
                    acts.push(Act::InkPointer(Pointer::Eyedropper(i)));
                    done = true;
                }
                // The rack's own actions, together and apart from the tool's look: move it, add another
                // like it, and, behind a second step, take it off the rack (issue #375).
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.add_enabled(i > 0, egui::Button::new(crate::tl_id!("ui.move_left")).small()).clicked() {
                        acts.push(Act::InkRack(RackEdit::Move(i, true)));
                    }
                    if ui.add_enabled(i + 1 < rack_len, egui::Button::new(crate::tl_id!("ui.move_right")).small()).clicked() {
                        acts.push(Act::InkRack(RackEdit::Move(i, false)));
                    }
                    let another = crate::i18n::format_message_id("ui.add_another_tool", &[("tool", crate::tl!(nib.tool.label()))]);
                    if ui.add(egui::Button::new(another).small()).clicked() {
                        acts.push(Act::InkRack(RackEdit::Add(nib.tool)));
                        done = true;
                    }
                });
                let confirm_id = id.with("confirm-delete");
                let confirming = ui.data(|d| d.get_temp::<bool>(confirm_id)).unwrap_or(false);
                if confirming {
                    ui.label(egui::RichText::new(crate::tl_id!("ui.delete_tool_confirm")).size(11.5).color(t.text));
                    ui.horizontal(|ui| {
                        if popup_action_button(
                            ui,
                            crate::tl_id!("ui.delete_tool"),
                            rack_len > 1,
                            t.warn,
                            crate::tl_id!("ui.remove_this_tool_from_the_rack"),
                        )
                        .clicked()
                        {
                            acts.push(Act::InkRack(RackEdit::Remove(i)));
                            ui.data_mut(|d| d.remove::<bool>(confirm_id));
                            done = true;
                        }
                        if popup_action_button(ui, crate::tl_id!("ui.keep_tool"), true, t.accent, crate::tl_id!("ui.keep_tool_tip")).clicked() {
                            ui.data_mut(|d| d.remove::<bool>(confirm_id));
                        }
                    });
                } else if popup_action_button(
                    ui,
                    crate::tl_id!("ui.delete_tool_ellipsis"),
                    rack_len > 1,
                    t.muted,
                    crate::tl_id!("ui.remove_this_tool_from_the_rack"),
                )
                .clicked()
                {
                    ui.data_mut(|d| d.insert_temp(confirm_id, true));
                }
            });
        })
        .response;
    ui.data_mut(|d| d.insert_temp(more_colours_id, more_colours));
    let at = ui.ctx().pointer_latest_pos().unwrap_or_default();
    let outside = ui.input(|i| i.pointer.any_click()) && !area.rect.contains(at) && !under.contains(at);
    let close = done || outside || ui.input(|i| i.key_pressed(egui::Key::Escape));
    if close {
        ui.data_mut(|d| d.remove::<bool>(id.with("confirm-delete")));
    }
    close
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
        chosen_badge(&painter, rect, t);
    } else if resp.hovered() {
        painter.rect_filled(rect, 4.0, t.row_hover);
    }

    let drop = ui.ctx().animate_bool_with_time(resp.id.with("picked"), chosen, 0.12) * DROP;
    let cx = rect.center().x;
    // Stood where the pens stand, and as tall as they are, above the strip their strokes use.
    let (stand, _) = rack_spots(rect);
    let tall = (stand.height() - 14.0).clamp(16.0, 38.0);
    let top = stand.center().y - tall / 2.0 - 2.0 + drop;
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
/// and by a test.
fn swatch_name(colour: Rgb) -> &'static str {
    match (colour.0, colour.1, colour.2) {
        (0x20, 0x20, 0x20) => "black",
        (0xd0, 0x21, 0x21) => "red",
        (0x18, 0x55, 0xc4) => "blue",
        (0x16, 0x7f, 0x3d) => "green",
        (0x55, 0x55, 0x55) => "grey",
        (0xff, 0xdd, 0x33) => "yellow",
        (0x6f, 0xe3, 0x9a) => "mint",
        // The rest of the palette, so every swatch is read out by a name (issue #375).
        (0x5a, 0x5a, 0x5a) => "dark grey",
        (0x8c, 0x8c, 0x8c) => "mid grey",
        (0xbd, 0xbd, 0xbd) => "light grey",
        (0xe4, 0xe4, 0xe4) => "pale grey",
        (0xff, 0xff, 0xff) => "white",
        (0x8c, 0x12, 0x12) => "dark red",
        (0xe8, 0x62, 0x1f) => "orange",
        (0xf2, 0x9d, 0x12) => "amber",
        (0xfa, 0xf0, 0x8a) => "pale yellow",
        (0x0d, 0x4c, 0x27) => "dark green",
        (0x3f, 0xb5, 0x50) => "light green",
        (0x19, 0x8f, 0x8f) => "teal",
        (0x66, 0xd9, 0xd9) => "aqua",
        (0x10, 0x2a, 0x6b) => "navy",
        (0x3b, 0x8d, 0xf0) => "sky blue",
        (0x86, 0xc5, 0xff) => "light blue",
        (0x4a, 0x2a, 0x8f) => "purple",
        (0x8b, 0x63, 0xd6) => "violet",
        (0x6b, 0x1b, 0x4a) => "plum",
        (0xb0, 0x2b, 0x7a) => "magenta",
        (0xe8, 0x5a, 0xa8) => "pink",
        (0xf7, 0xa8, 0xcd) => "light pink",
        (0x6a, 0x45, 0x2a) => "brown",
        (0xa9, 0x7c, 0x50) => "tan",
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

/// OneNote's History tab, with the same groups and controls.
fn history_tab(ui: &mut Ui, view: &View<'_>, t: &Tokens, acts: &mut Vec<Act>) {
    group(ui, crate::tl_id!("ui.unread"), t, |ui| {
        if big(ui, Icon::PageStack, crate::tl_id!("ui.next_unread_2"), crate::tl_id!("ui.go_to_the_next_page_changed_since_you_last_read_it"), t)
            .clicked()
        {
            acts.push(Act::NextUnread);
        }
        let (label, tip) = if view.page_marked_unread {
            (crate::tl_id!("ui.mark_as_read_2"), crate::tl_id!("ui.mark_this_page_as_read"))
        } else {
            (crate::tl_id!("ui.mark_as_unread_2"), crate::tl_id!("ui.mark_this_page_as_unread_to_come_back_to_it"))
        };
        if ui.add_enabled_ui(view.has_page, |ui| big(ui, Icon::Todo, label, tip, t)).inner.clicked() {
            acts.push(Act::ToggleRead);
        }
    });
    group(ui, crate::tl_id!("ui.authors"), t, |ui| {
        if big(ui, Icon::Clock, crate::tl_id!("ui.recent_edits_2"), crate::tl_id!("ui.see_the_pages_changed_lately"), t).clicked() {
            acts.push(Act::RecentEdits);
        }
        if big(ui, Icon::Search, crate::tl_id!("ui.find_by_author"), crate::tl_id!("ui.find_by_author_tip"), t).clicked() {
            acts.push(Act::FindByAuthor);
        }
        if big_on(ui, Icon::Page, crate::tl_id!("ui.hide_authors"), crate::tl_id!("ui.hide_authors_tip"), view.hide_authors, t).clicked() {
            acts.push(Act::ToggleHideAuthors);
        }
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
            .add_enabled_ui(view.has_recycle_notebook, |ui| {
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
        // Follows the drawing's own redo steps, as Undo stroke follows its undo steps (#264).
        if ui
            .add_enabled_ui(view.can_ink_redo, |ui| {
                big(ui, Icon::Redo, crate::tl_id!("ui.redo_stroke"), crate::tl_id!("ui.put_back_the_stroke_undo_stroke_took_back"), t)
            })
            .inner
            .clicked()
        {
            acts.push(Act::InkRedo);
        }
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
        // A mode like the pens: on until another tool (or Type) is chosen, or it is pressed again.
        let on = view.pointer == Pointer::Pan;
        if big_on(ui, Icon::Hand, crate::tl_id!("ui.panning_hand_2"), crate::tl_id!("ui.drag_to_move_around_the_page"), on, t).clicked() {
            acts.push(Act::InkPointer(if on { Pointer::Write } else { Pointer::Pan }));
        }
    });
    group(ui, crate::tl_id!("ui.stencils"), t, |ui| {
        if big_on(ui, Icon::Ruler, crate::tl_id!("ui.ruler_2"), crate::tl_id!("ui.ruler_tip"), view.ruler, t).clicked() {
            acts.push(Act::ToggleRuler);
        }
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
        // The same tool as Insert > Insert Space, lit the same way while it is in hand (#264).
        if big_on(ui, Icon::InsertSpace, crate::tl_id!("ui.insert_space_2"), crate::tl_id!("ui.insert_space_tip"), view.space_tool, t).clicked() {
            acts.push(Act::InsertSpaceTool);
        }
        // OneNote's Format Background is this tab's way to the paper, and we have the
        // paper, so it goes to the same gallery the View tab opens.
        rule_lines_menu(ui, view, t, acts);
        // Everything on the canvas that drifted apart, boxes and ink, gathered back together
        // (issue #23). Last in the group, so the controls before it keep their places.
        if big(ui, Icon::Shapes, crate::tl_id!("ui.collect_all"), crate::tl_id!("ui.collect_all_tip"), t).clicked() {
            acts.push(Act::CollectAll);
        }
    });
    shapes_group(ui, view, t, acts);
    group(ui, crate::tl_id!("ui.shapes"), t, |ui| {
        if big_on(ui, Icon::Shapes, crate::tl_id!("ui.automatic_shapes_2"), crate::tl_id!("ui.automatic_shapes_tip"), view.auto_shapes, t).clicked() {
            acts.push(Act::ToggleAutoShapes);
        }
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
        if big_on(ui, Icon::Replay, crate::tl_id!("ui.ink_replay_2"), crate::tl_id!("ui.ink_replay_tip"), view.replaying, t).clicked() {
            acts.push(Act::ToggleReplay);
        }
    });
    group(ui, crate::tl_id!("ui.mode"), t, |ui| {
        if big(ui, Icon::FullPage, crate::tl_id!("ui.full_page_view_2"), crate::tl_id!("ui.hide_everything_but_the_page"), t).clicked() {
            acts.push(Act::ToggleFullPage);
        }
    });
    group(ui, crate::tl_id!("ui.help"), t, |ui| {
        if big(ui, Icon::Help, crate::tl_id!("ui.ink_help_2"), crate::tl_id!("ui.ink_help_tip"), t).clicked() {
            acts.push(Act::InkHelp);
        }
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
        if big(ui, Icon::Table, crate::tl_id!("ui.delete_table_2"), crate::tl_id!("ui.take_the_whole_table_out"), t).clicked() {
            acts.push(Act::DeleteTable);
        }
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
        if big(ui, Icon::Table, crate::tl_id!("ui.merge_cells"), crate::tl_id!("ui.merge_cells_tip"), t).clicked() {
            acts.push(Act::MergeCells);
        }
        if big(ui, Icon::Table, crate::tl_id!("ui.split_cells"), crate::tl_id!("ui.split_cells_tip"), t).clicked() {
            acts.push(Act::SplitCells);
        }
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
        // Issue #376: the view in use is marked, so Normal and Full Page read as a choice.
        if big_on(ui, Icon::Page, crate::tl_id!("ui.normal_view"), crate::tl_id!("ui.the_page_beside_the_notebooks"), !view.full_page, t).clicked()
            && view.full_page
        {
            acts.push(Act::ToggleFullPage);
        }
        if big_on(
            ui,
            if view.full_page { Icon::ExitFullPage } else { Icon::FullPage },
            crate::tl_id!("ui.full_page_view_2"),
            if view.full_page { crate::tl_id!("ui.full_page_on_tip") } else { crate::tl_id!("ui.full_page_off_tip") },
            view.full_page,
            t,
        )
        .clicked()
        {
            acts.push(Act::ToggleFullPage);
        }
        rows(ui, |ui| {
            if small_check(ui, crate::tl_id!("ui.dock_to_desktop"), crate::tl_id!("ui.dock_to_desktop_tip"), view.docked, t).clicked() {
                acts.push(Act::ToggleDock);
            }
        });
    });
    group(ui, crate::tl_id!("ui.layout"), t, |ui| {
        rows(ui, |ui| {
            // What OneNote calls the navigation pane: notebooks and sections in one
            // column, the pages of the chosen section in the next.
            if small_check(
                ui,
                crate::tl_id!("ui.separate_pages"),
                crate::tl_id!("ui.notebooks_and_sections_in_one_column_pages_in_the_next"),
                view.split_panes,
                t,
            )
            .clicked()
            {
                acts.push(Act::TogglePanes);
            }
            // Sections as coloured tabs across the top of the page (#268, #33).
            if small_check(ui, crate::tl_id!("ui.tabs_layout"), crate::tl_id!("ui.tabs_layout_tip"), view.tabs_layout, t).clicked() {
                acts.push(Act::ToggleTabsLayout);
            }
        });
    });
    group(ui, crate::tl_id!("ui.tools"), t, |ui| {
        rows(ui, |ui| {
            // Issue #376: a command with an icon and its name, like the others, and a tip that says
            // how it differs from Full Page View: the page as plain large type to read, not the
            // page itself with more room.
            if small_labeled_icon(ui, Icon::AlignLeft, crate::tl_id!("ui.immersive_reader"), crate::tl_id!("ui.immersive_reader_tip"), t).clicked() {
                acts.push(Act::ImmersiveReader);
            }
            not_built(ui, crate::tl_id!("ui.live_captions"), t);
        });
    });
    group(ui, crate::tl_id!("ui.page_setup"), t, |ui| {
        rule_lines_menu(ui, view, t, acts);
        // Issue #376: each part of the paper says what it is set to, with a small picture of it,
        // and opens its own choices. Three rows, so nothing runs into the group's name.
        rows(ui, |ui| {
            ui.add_enabled_ui(view.has_page, |ui| page_colour_row(ui, view, t, acts));
            ui.add_enabled_ui(view.has_page, |ui| rule_lines_row(ui, view, t, acts));
            ui.add_enabled_ui(view.has_page, |ui| paper_size_row(ui, view, t, acts));
        });
        rows(ui, |ui| {
            if small_check(ui, crate::tl_id!("ui.hide_page_title"), crate::tl_id!("ui.hide_the_page_title"), view.hide_title, t).clicked() {
                acts.push(Act::ToggleTitle);
            }
            if small_check(
                ui,
                crate::tl_id!("ui.snap_to_grid"),
                crate::tl_id!("ui.snap_boxes_to_the_page_s_grid_hold_alt_to_place_one_freely"),
                view.snap,
                t,
            )
            .clicked()
            {
                acts.push(Act::ToggleSnap);
            }
        });
    });
    // Issue #376: one cluster, in the order the issue names it: Zoom Out, the zoom now (pick
    // another), Zoom In, then Reset zoom and Page Width under the percentage.
    group(ui, crate::tl_id!("ui.zoom"), t, |ui| {
        ui.horizontal(|ui| {
            if big(ui, Icon::ZoomOut, crate::tl_id!("ui.zoom_out"), crate::tl_id!("ui.zoom_out_tip"), t).clicked() {
                acts.push(Act::Zoom(-1));
            }
            rows(ui, |ui| {
                let combo = egui::ComboBox::from_id_salt("ribbon-zoom-percent")
                    .width(76.0)
                    .selected_text(format!("{}%", view.zoom_percent))
                    .show_ui(ui, |ui| {
                        for percent in [50, 60, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250] {
                            if ui.selectable_label(view.zoom_percent == percent, format!("{percent}%")).clicked() {
                                acts.push(Act::ZoomTo(percent));
                            }
                        }
                    })
                    .response;
                // The percentage box is named for a screen reader, and says its value; on screen the
                // group's name under it already says what it is.
                let (enabled, now) = (ui.is_enabled(), format!("{}%", view.zoom_percent));
                combo.widget_info(move || {
                    let mut info = egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, enabled, crate::tl_id!("ui.zoom_level"));
                    info.current_text_value = Some(now.clone());
                    info
                });
                combo.on_hover_text(crate::tl_id!("ui.zoom_percent_tip"));
                // A reset reads as a reset, not as a second "100%" beside the percentage box;
                // there is nothing to reset at 100%.
                let reset = ui
                    .add_enabled_ui(view.zoom_percent != 100, |ui| {
                        small_labeled_icon(ui, Icon::Undo, crate::tl_id!("ui.reset_zoom"), crate::tl_id!("ui.show_the_page_at_100"), t)
                    })
                    .inner;
                if reset.clicked() {
                    acts.push(Act::Zoom(0));
                }
                if small_labeled_icon(ui, Icon::PageWidth, crate::tl_id!("ui.page_width"), crate::tl_id!("ui.fit_the_page_to_the_available_width"), t)
                    .clicked()
                {
                    acts.push(Act::PageWidth);
                }
            });
            if big(ui, Icon::ZoomIn, crate::tl_id!("ui.zoom_in"), crate::tl_id!("ui.zoom_in_tip"), t).clicked() {
                acts.push(Act::Zoom(1));
            }
        });
    });
    group(ui, crate::tl_id!("ui.window"), t, |ui| {
        rows(ui, |ui| {
            // Always on top is the window manager's business, and eframe can ask for it.
            if small_check(ui, crate::tl_id!("ui.always_on_top"), crate::tl_id!("ui.keep_this_window_above_the_others"), view.always_on_top, t)
                .clicked()
            {
                acts.push(Act::ToggleAlwaysOnTop);
            }
            if small_labeled_icon(ui, Icon::PageStack, crate::tl_id!("ui.new_window"), crate::tl_id!("ui.window_new_tip"), t).clicked() {
                acts.push(Act::NewWindow);
            }
            if small_labeled_icon(ui, Icon::Page, crate::tl_id!("ui.new_quick_note"), crate::tl_id!("ui.window_quick_tip"), t).clicked() {
                acts.push(Act::QuickNote);
            }
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
    // One Window group only (#256): it is the one above, with the working Always on top.
    // Dock to desktop is in Views, where OneNote has it.
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
    // Unavailable, and said so to screen readers and automation as well as on screen: a
    // placeholder is never an enabled button, whatever the ui around it is (#257).
    let name = crate::i18n::format_message_id("ui.label_not_built", &[("label", label)]);
    resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Button, false, &name));
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
    fn a_pen_on_the_rack_shows_the_stroke_it_makes_thin_to_thick() {
        let band = Rect::from_min_size(egui::pos2(0.0, 0.0), vec2(21.0, RACK_SWATCH_H));
        let black = notecraft_ink::Rgb(0, 0, 0);
        let mut last = 0.0;
        // Every size on the rack, then the broad ones a pen may be set to past it, up to the
        // largest: compared with their neighbours, so a cap anywhere shows as two alike.
        for tenths in rack::THICKNESSES.into_iter().chain([200, 280, 330, rack::MAX_TENTHS]) {
            let nib = Nib::new(Tool::Pen, black, tenths);
            let swatch = rack_swatch(nib, band, false);
            assert_eq!((swatch.tool, swatch.colour), (Tool::Pen, black), "the swatch is the pen's own");
            assert!(swatch.width > last, "{tenths} tenths draws wider than the size before it");
            last = swatch.width;
            for dab in &swatch.dabs {
                let half = swatch.width / 2.0;
                assert!(dab.x - half >= 0.0 && dab.x + half <= band.width(), "{tenths}: inside the rack across");
                assert!(dab.y - half >= 0.0 && dab.y + half <= band.height(), "{tenths}: inside the rack down");
            }
        }
        // A fine pen is drawn at its own width, so the two thinnest still read as different pens.
        assert!((rack_swatch(Nib::new(Tool::Pen, black, 10), band, false).width - 1.0).abs() < 1e-4);
        let marker = rack_swatch(Nib::new(Tool::Highlighter, notecraft_ink::Rgb(255, 230, 0), 140), band, false);
        assert_eq!(marker.tool, Tool::Highlighter, "a highlighter swatch is drawn as a highlighter");
        // On a dark ribbon a black pen's stroke is drawn light, or it would not be seen at all.
        let on_dark = rack_swatch(Nib::new(Tool::Pen, black, 20), band, true);
        assert_eq!(on_dark.colour, crate::inkpad::shown_colour(black, true));
        assert_ne!(on_dark.colour, black);
        let red = notecraft_ink::Rgb(220, 30, 30);
        assert_eq!(rack_swatch(Nib::new(Tool::Pen, red, 20), band, true).colour, red, "a real colour stays itself");
    }

    #[test]
    fn the_pressure_tip_names_what_this_system_needs() {
        let tip = pressure_unavailable_tip();
        if cfg!(target_os = "windows") {
            assert!(tip.contains("Windows Ink"), "{tip}");
        } else {
            assert!(!tip.contains("Windows"), "a Windows setting is no help here: {tip}");
        }
        if cfg!(target_os = "linux") {
            assert!(tip.contains("Wayland"), "{tip}");
        }
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

/// Where the last touch is kept: true when it was a finger, false when it was a pen.
fn finger_id() -> egui::Id {
    egui::Id::new("ribbon-last-touch-a-finger")
}

/// Notes, once a frame and whether the ribbon is shown or not, whether the last touch to come
/// down was a finger (a touch that reports no force) or a pen (one that reports a force).
pub fn note_touches(ctx: &egui::Context) {
    let was = ctx.data(|d| d.get_temp::<bool>(finger_id())).unwrap_or(false);
    let now = ctx.input(|i| {
        i.events.iter().fold(was, |finger, e| match e {
            egui::Event::Touch { phase: egui::TouchPhase::Start, force, .. } => force.is_none(),
            _ => finger,
        })
    });
    if now != was {
        ctx.data_mut(|d| d.insert_temp(finger_id(), now));
    }
}

/// Whether a drag on the ribbon scrolls it. egui's `OnTouch` asks whether a touch screen was
/// ever seen, and a pen on a tablet arrives as a touch too (with a force): after the first stroke
/// every drag scrolled the ribbon, and a pen's tap, which moves a pixel or two, became a scroll
/// instead of a click. The ribbon scrolls under a drag while the last touch was a finger, as
/// `OnTouch` does once a finger has been seen, and not once a pen has come down. It is decided
/// before a press, not on it: egui finds what a press is on among the last frame's widgets.
fn finger_drag(ctx: &egui::Context) -> egui::scroll_area::DragScroll {
    if ctx.data(|d| d.get_temp::<bool>(finger_id())).unwrap_or(false) {
        egui::scroll_area::DragScroll::Always
    } else {
        egui::scroll_area::DragScroll::Never
    }
}

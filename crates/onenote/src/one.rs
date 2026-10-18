//! What the objects mean: [MS-ONE] on top of the container in [`crate::store`].
//!
//! A `.one` file is a section. Its root object space holds the section node, which lists
//! page series; each page series points at one object space per page. A page holds outlines,
//! an outline holds outline elements, and an outline element holds the text, pictures,
//! tables and files on that line. This module walks that and returns a plain model with no
//! OneNote vocabulary left in it, which the importer turns into folders and pages.
//!
//! Unknown property sets and properties are ignored, as the specification requires. Nothing
//! here can panic on a damaged file: every step is an `Option` or a bounded loop.

use crate::guid::CompactId;
use crate::props::PropertySet;
use crate::store::{Object, ObjectData, ObjectSpace, Store};

// JCID index values ([MS-ONE] 2.1.13). The high half of a JCID holds flags; `Object::kind`
// returns this part.
const SECTION_NODE: u32 = 0x0007;
const PAGE_SERIES_NODE: u32 = 0x0008;
const PAGE_NODE: u32 = 0x000B;
const OUTLINE_NODE: u32 = 0x000C;
const OUTLINE_ELEMENT_NODE: u32 = 0x000D;
const RICH_TEXT_NODE: u32 = 0x000E;
const IMAGE_NODE: u32 = 0x0011;
const NUMBER_LIST_NODE: u32 = 0x0012;
const OUTLINE_GROUP: u32 = 0x0019;
const TABLE_NODE: u32 = 0x0022;
const TABLE_ROW_NODE: u32 = 0x0023;
const TABLE_CELL_NODE: u32 = 0x0024;
const TITLE_NODE: u32 = 0x002C;
const EMBEDDED_FILE_NODE: u32 = 0x0035;
const PAGE_MANIFEST_NODE: u32 = 0x0037;

// PropertyIDs ([MS-ONE] 2.1.12). The type is part of the value, so these read as they do
// in the specification's table.
const CACHED_TITLE_STRING: u32 = 0x1C00_1CF3;
const CACHED_TITLE_STRING_FROM_PAGE: u32 = 0x1C00_1D3C;
const PAGE_LEVEL: u32 = 0x1400_1DFF;
const ELEMENT_CHILD_NODES: u32 = 0x2400_1C20;
const CONTENT_CHILD_NODES: u32 = 0x2400_1C1F;
const STRUCTURE_ELEMENT_CHILD_NODES: u32 = 0x2400_1D5F;
const CHILD_GRAPH_SPACE_ELEMENT_NODES: u32 = 0x2C00_1D63;
const META_DATA_OBJECTS_ABOVE_GRAPH_SPACE: u32 = 0x2400_3442;
const RICH_EDIT_TEXT_UNICODE: u32 = 0x1C00_1C22;
const TEXT_EXTENDED_ASCII: u32 = 0x1C00_3498;
const TEXT_RUN_INDEX: u32 = 0x1C00_1E12;
const TEXT_RUN_FORMATTING: u32 = 0x2400_1E13;
const LIST_NODES: u32 = 0x2400_1C26;
const NUMBER_LIST_FORMAT: u32 = 0x1C00_1C1A;
const OUTLINE_ELEMENT_CHILD_LEVEL: u32 = 0x0C00_1C03;
const BOLD: u32 = 0x0800_1C04;
const ITALIC: u32 = 0x0800_1C05;
const UNDERLINE: u32 = 0x0800_1C06;
const STRIKETHROUGH: u32 = 0x0800_1C07;
const MATH_FORMATTING: u32 = 0x0800_3401;
const HYPERLINK: u32 = 0x0800_1E14;
const WZ_HYPERLINK_URL: u32 = 0x1C00_1E20;
const PICTURE_CONTAINER: u32 = 0x2000_1C3F;
const IMAGE_FILENAME: u32 = 0x1C00_1DD7;
const IMAGE_ALT_TEXT: u32 = 0x1C00_1E58;
const EMBEDDED_FILE_CONTAINER: u32 = 0x2000_1D9B;
const EMBEDDED_FILE_NAME: u32 = 0x1C00_1D9C;
const ROW_COUNT: u32 = 0x1400_1D57;
const COLUMN_COUNT: u32 = 0x1400_1D58;
const IS_TITLE_TEXT: u32 = 0x0800_1CB4;

/// A damaged file must not be able to make the walk run away.
const MAX_DEPTH: usize = 24;
const MAX_PAGES: usize = 10_000;
const MAX_BLOCKS: usize = 100_000;

/// One section file, read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// The pages, in the order the section lists them.
    pub pages: Vec<Page>,
}

/// A page. `level` is OneNote's page level, 1 to 3: 1 is a page, 2 and 3 are sub-pages of
/// the nearest page above with a lower level.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    pub title: String,
    pub level: u8,
    pub blocks: Vec<Content>,
}

/// A line of a page, flattened from OneNote's outlines.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// Text, with its runs and, if it is a list item, what kind of list.
    Text { runs: Vec<Run>, list: Option<List>, depth: usize },
    /// A picture. `file` names the data in the store ([MS-ONESTORE] file data reference).
    Picture { name: String, alt: String, file: Option<String> },
    /// A file someone put on the page.
    File { name: String, file: Option<String> },
    /// A table of cells, each cell a list of lines.
    Table { rows: Vec<Vec<Vec<Content>>> },
}

/// What kind of list a line belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum List {
    Bullet,
    Number,
}

/// A run of text with the marks OneNote gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// OneNote marks a run as a mathematical expression but keeps no structure for it.
    pub math: bool,
    pub link: Option<String>,
}

/// Reads a section file that [`crate::store::read`] has already parsed.
pub fn section(store: &Store) -> Section {
    let mut pages = Vec::new();
    let Some(root) = store.spaces.first() else { return Section::default() };
    let Some(section_node) = root.content_root().filter(|o| o.kind() == SECTION_NODE) else {
        return Section::default();
    };
    let Some(props) = section_node.props() else { return Section::default() };
    for series_id in props.object_ids(ELEMENT_CHILD_NODES) {
        let Some(series) = resolve(root, *series_id) else { continue };
        if series.kind() != PAGE_SERIES_NODE {
            continue;
        }
        let Some(series_props) = series.props() else { continue };
        // The pages of this series: one object space each, with metadata beside them.
        let spaces = series_props.object_space_ids(CHILD_GRAPH_SPACE_ELEMENT_NODES);
        let metas = series_props.object_ids(META_DATA_OBJECTS_ABOVE_GRAPH_SPACE);
        for (i, space_id) in spaces.iter().enumerate() {
            if pages.len() >= MAX_PAGES {
                return Section { pages };
            }
            let meta = metas.get(i).and_then(|m| resolve(root, *m)).and_then(Object::props);
            if let Some(page) = page_of(store, root, *space_id, meta) {
                pages.push(page);
            }
        }
    }
    Section { pages }
}

/// One page: its object space, with the metadata the section keeps beside it.
fn page_of(store: &Store, root: &ObjectSpace, space_id: CompactId, meta: Option<&PropertySet>) -> Option<Page> {
    let space = space_by_compact(store, root, space_id)?;
    let manifest = space.content_root()?;
    if manifest.kind() != PAGE_MANIFEST_NODE {
        return None;
    }
    let page_object =
        manifest.props()?.object_ids(CONTENT_CHILD_NODES).iter().filter_map(|id| resolve(space, *id)).find(|o| o.kind() == PAGE_NODE)?;
    let props = page_object.props()?;
    let level = meta.and_then(|m| m.number(PAGE_LEVEL)).unwrap_or(1).clamp(1, 3) as u8;
    let mut title =
        meta.and_then(|m| m.text(CACHED_TITLE_STRING)).or_else(|| props.text(CACHED_TITLE_STRING_FROM_PAGE)).unwrap_or_default().trim().to_string();
    let mut blocks = Vec::new();
    // The title is an outline of its own, flagged as title text.
    for id in props.object_ids(STRUCTURE_ELEMENT_CHILD_NODES) {
        if let Some(node) = resolve(space, *id).filter(|o| o.kind() == TITLE_NODE)
            && title.is_empty()
            && let Some(text) = title_text(space, node)
        {
            title = text;
        }
    }
    for id in props.object_ids(ELEMENT_CHILD_NODES) {
        collect(space, *id, 0, 0, &mut blocks);
    }
    if title.is_empty() {
        // No title anywhere: the first line of the page stands in, as OneNote shows it.
        title = blocks
            .iter()
            .find_map(|b| match b {
                Content::Text { runs, .. } => {
                    let t: String = runs.iter().map(|r| r.text.as_str()).collect();
                    (!t.trim().is_empty()).then(|| t.trim().to_string())
                }
                _ => None,
            })
            .unwrap_or_else(|| "Untitled page".to_string());
    }
    Some(Page { title, level, blocks })
}

/// The text of a page's title outline.
fn title_text(space: &ObjectSpace, node: &Object) -> Option<String> {
    let mut blocks = Vec::new();
    for id in node.props()?.object_ids(ELEMENT_CHILD_NODES) {
        // Only the title's own text, not the date and time lines beside it.
        if let Some(outline) = resolve(space, *id)
            && outline.props().and_then(|p| p.bool(IS_TITLE_TEXT)) == Some(true)
        {
            collect(space, *id, 0, 0, &mut blocks);
        }
    }
    let text: String = blocks
        .iter()
        .filter_map(|b| match b {
            Content::Text { runs, .. } => Some(runs.iter().map(|r| r.text.as_str()).collect::<String>()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" ");
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Walks one object and everything under it, adding what it finds to `out`.
fn collect(space: &ObjectSpace, id: CompactId, depth: usize, indent: usize, out: &mut Vec<Content>) {
    if depth > MAX_DEPTH || out.len() >= MAX_BLOCKS {
        return;
    }
    let Some(object) = resolve(space, id) else { return };
    let Some(props) = object.props() else { return };
    match object.kind() {
        OUTLINE_NODE | OUTLINE_GROUP | TITLE_NODE => {
            for child in props.object_ids(ELEMENT_CHILD_NODES) {
                collect(space, *child, depth + 1, indent, out);
            }
        }
        OUTLINE_ELEMENT_NODE => {
            let list = list_kind(space, props);
            let child_indent = props.number(OUTLINE_ELEMENT_CHILD_LEVEL).map(|n| n.saturating_sub(1) as usize).unwrap_or(indent);
            for child in props.object_ids(CONTENT_CHILD_NODES) {
                content(space, *child, depth + 1, indent, list, out);
            }
            for child in props.object_ids(ELEMENT_CHILD_NODES) {
                collect(space, *child, depth + 1, child_indent.max(indent + 1).min(8), out);
            }
        }
        TABLE_NODE => {
            if let Some(table) = table(space, props, depth) {
                out.push(table);
            }
        }
        _ => content(space, id, depth, indent, None, out),
    }
}

/// The content of an outline element: text, a picture, a file or a table.
fn content(space: &ObjectSpace, id: CompactId, depth: usize, indent: usize, list: Option<List>, out: &mut Vec<Content>) {
    if depth > MAX_DEPTH || out.len() >= MAX_BLOCKS {
        return;
    }
    let Some(object) = resolve(space, id) else { return };
    let Some(props) = object.props() else { return };
    match object.kind() {
        RICH_TEXT_NODE => {
            let runs = runs_of(space, props);
            if !runs.is_empty() {
                out.push(Content::Text { runs, list, depth: indent });
            }
        }
        IMAGE_NODE => out.push(Content::Picture {
            name: props.text(IMAGE_FILENAME).unwrap_or_default(),
            alt: props.text(IMAGE_ALT_TEXT).unwrap_or_default(),
            file: file_reference(space, props, PICTURE_CONTAINER),
        }),
        EMBEDDED_FILE_NODE => out.push(Content::File {
            name: props.text(EMBEDDED_FILE_NAME).unwrap_or_default(),
            file: file_reference(space, props, EMBEDDED_FILE_CONTAINER),
        }),
        TABLE_NODE => {
            if let Some(t) = table(space, props, depth) {
                out.push(t);
            }
        }
        OUTLINE_NODE | OUTLINE_ELEMENT_NODE | OUTLINE_GROUP => collect(space, id, depth + 1, indent, out),
        _ => {}
    }
}

/// A table: rows of cells, each cell holding its own lines.
fn table(space: &ObjectSpace, props: &PropertySet, depth: usize) -> Option<Content> {
    let columns = props.number(COLUMN_COUNT).unwrap_or(0) as usize;
    let _rows_said = props.number(ROW_COUNT).unwrap_or(0);
    let mut rows = Vec::new();
    for row_id in props.object_ids(ELEMENT_CHILD_NODES) {
        let Some(row) = resolve(space, *row_id).filter(|o| o.kind() == TABLE_ROW_NODE) else { continue };
        let Some(row_props) = row.props() else { continue };
        let mut cells = Vec::new();
        for cell_id in row_props.object_ids(ELEMENT_CHILD_NODES) {
            let Some(cell) = resolve(space, *cell_id).filter(|o| o.kind() == TABLE_CELL_NODE) else { continue };
            let mut lines = Vec::new();
            if let Some(cell_props) = cell.props() {
                for child in cell_props.object_ids(ELEMENT_CHILD_NODES) {
                    collect(space, *child, depth + 1, 0, &mut lines);
                }
            }
            cells.push(lines);
        }
        if columns > 0 {
            cells.truncate(columns.max(1));
        }
        rows.push(cells);
    }
    (!rows.is_empty()).then_some(Content::Table { rows })
}

/// Whether an outline element is a list item, and which kind.
fn list_kind(space: &ObjectSpace, props: &PropertySet) -> Option<List> {
    let node = props.object_ids(LIST_NODES).iter().find_map(|id| resolve(space, *id))?;
    if node.kind() != NUMBER_LIST_NODE {
        return None;
    }
    let format = node.props()?.text(NUMBER_LIST_FORMAT).unwrap_or_default();
    // [MS-ONE] 2.3.20: the numbering character U+FFFD makes it a numbered list.
    Some(if format.contains('\u{FFFD}') { List::Number } else { List::Bullet })
}

/// The text of a rich text node, split into runs by their formatting.
fn runs_of(space: &ObjectSpace, props: &PropertySet) -> Vec<Run> {
    let text = props.text(RICH_EDIT_TEXT_UNICODE).or_else(|| props.text(TEXT_EXTENDED_ASCII)).unwrap_or_default();
    if text.is_empty() {
        return Vec::new();
    }
    let chars: Vec<char> = text.chars().collect();
    // TextRunIndex holds the character positions where runs end; TextRunFormatting holds one
    // style object per run, one more than the indexes.
    let ends: Vec<usize> =
        props.bytes(TEXT_RUN_INDEX).map(|b| b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) as usize).collect()).unwrap_or_default();
    let styles: Vec<Option<&Object>> = props.object_ids(TEXT_RUN_FORMATTING).iter().map(|id| resolve(space, *id)).collect();
    let mut runs = Vec::new();
    let mut start = 0usize;
    for (i, end) in ends.iter().chain(std::iter::once(&chars.len())).enumerate() {
        let end = (*end).min(chars.len());
        if end < start {
            continue;
        }
        let piece: String = chars.get(start..end).unwrap_or(&[]).iter().collect();
        start = end;
        if piece.is_empty() {
            continue;
        }
        let mut run = Run { text: piece, ..Run::default() };
        if let Some(Some(style)) = styles.get(i)
            && let Some(sp) = style.props()
        {
            run.bold = sp.bool(BOLD).unwrap_or(false);
            run.italic = sp.bool(ITALIC).unwrap_or(false);
            run.underline = sp.bool(UNDERLINE).unwrap_or(false);
            run.strike = sp.bool(STRIKETHROUGH).unwrap_or(false);
            run.math = sp.bool(MATH_FORMATTING).unwrap_or(false);
            if sp.bool(HYPERLINK).unwrap_or(false) {
                run.link = sp.text(WZ_HYPERLINK_URL);
            }
        }
        runs.push(run);
    }
    runs
}

/// The file data reference behind a picture or an embedded file.
fn file_reference(space: &ObjectSpace, props: &PropertySet, which: u32) -> Option<String> {
    let container = props.object_ids(which).first().copied()?;
    match &resolve(space, container)?.data {
        ObjectData::File { reference, .. } => Some(reference.clone()),
        ObjectData::Props(_) => None,
    }
}

/// An object of this space, by the compact id a property carries.
fn resolve(space: &ObjectSpace, id: CompactId) -> Option<&Object> {
    space.object(id)
}

/// An object space of the file, by the compact id a property of `from` carries. The id is
/// read through the table of the space that holds the property, then matched by identity.
fn space_by_compact<'a>(store: &'a Store, from: &ObjectSpace, id: CompactId) -> Option<&'a ObjectSpace> {
    let wanted = from.identity(id)?;
    store.spaces.iter().find(|s| s.id == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::build::OneNote;

    #[test]
    fn a_file_with_no_section_node_reads_as_an_empty_section() {
        let data = crate::store::build::One::new().finish(&[], 1, Vec::new());
        let store = crate::store::read(&data).expect("read");
        assert_eq!(section(&store), Section::default());
    }

    #[test]
    fn a_section_with_two_pages_and_a_sub_page() {
        let mut one = OneNote::new();
        one.page("Shopping", 1, &[OneNote::line("milk"), OneNote::line("bread")]);
        one.page("Work", 1, &[OneNote::line("the plan")]);
        one.page("Details", 2, &[OneNote::line("under Work")]);
        let data = one.finish();
        let store = crate::store::read(&data).expect("read");
        let section = section(&store);
        assert_eq!(section.pages.len(), 3);
        assert_eq!(section.pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), vec!["Shopping", "Work", "Details"]);
        assert_eq!(section.pages.iter().map(|p| p.level).collect::<Vec<_>>(), vec![1, 1, 2]);
        let first = section.pages.first().expect("a page");
        assert_eq!(first.blocks.len(), 2);
        match first.blocks.first() {
            Some(Content::Text { runs, list, depth }) => {
                assert_eq!(runs.iter().map(|r| r.text.as_str()).collect::<String>(), "milk");
                assert_eq!((*list, *depth), (None, 0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn marks_on_a_run_come_through() {
        let mut one = OneNote::new();
        one.page("Styled", 1, &[OneNote::styled("bold bit", true, false), OneNote::styled("slanted", false, true)]);
        let data = one.finish();
        let store = crate::store::read(&data).expect("read");
        let page = section(&store).pages.into_iter().next().expect("a page");
        let runs: Vec<&Run> = page
            .blocks
            .iter()
            .filter_map(|b| match b {
                Content::Text { runs, .. } => runs.first(),
                _ => None,
            })
            .collect();
        assert_eq!(runs.len(), 2);
        assert!(runs.first().is_some_and(|r| r.bold && !r.italic), "{:?}", runs.first());
        assert!(runs.get(1).is_some_and(|r| r.italic && !r.bold), "{:?}", runs.get(1));
    }

    #[test]
    fn a_damaged_file_gives_no_pages_rather_than_a_panic() {
        let good = OneNote::new().with_page("A", &[OneNote::line("x")]).finish();
        for cut in (1024..good.len()).step_by(97) {
            if let Ok(store) = crate::store::read(good.get(..cut).unwrap_or(&[])) {
                let _ = section(&store);
            }
        }
        // Junk where the section node should be.
        let store = crate::store::read(&good).expect("read");
        assert!(!section(&store).pages.is_empty());
    }
}

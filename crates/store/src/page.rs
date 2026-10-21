//! Reading and writing the content of a page.

use crate::error::{Result, StoreError};
use crate::tree::{page_file, raw_title, read_node};
use notecraft_formats::{boxes, frontmatter, html};
use notecraft_model::{Container, Dir, PageFormat, container, id::new_id, justify, kv, tablecols, time::iso_now};
use std::fs;
use std::path::{Path, PathBuf};

/// Pages larger than this are refused (a guard against hostile or accidental input).
pub const MAX_PAGE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub format: PageFormat,
    /// The title shown for this page: a real title, or `tree::UNTITLED_PAGE` when it has
    /// none. For the window's title area, which is drawn blank rather than showing that
    /// fallback as if it were content, see `has_title`.
    pub title: String,
    /// True when the page carries a title of its own. `false` means `title` is only the
    /// fallback shown in the tree and the page list; the window draws its title area blank
    /// in that case rather than putting `tree::UNTITLED_PAGE` into an editable box, which
    /// would read as content someone had typed.
    pub has_title: bool,
    /// The editable part: the inside of the HTML body, or the Markdown after the front matter.
    pub body: String,
    /// Which way the whole page reads, if the writer said. `None` means each line decides
    /// for itself, from its own first letter.
    pub dir: Option<Dir>,
}

/// A saved copy of a page file, newest first. The page's own format and assets remain
/// ordinary files; versions are kept in its hidden `.notecraft/versions` folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
    pub id: String,
    pub created: String,
    pub bytes: u64,
}

const MAX_VERSIONS: usize = 50;
const MAX_VERSION_BYTES: u64 = 128 * 1024 * 1024;
const VERSION_INTERVAL_NS: u128 = 5 * 60 * 1_000_000_000;

pub fn load_page(dir: &Path) -> Result<Page> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let meta = fs::metadata(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if meta.len() > MAX_PAGE_BYTES as u64 {
        return Err(StoreError::invalid("this page is too large to open"));
    }
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = body_of(format, &text);
    let has_title = raw_title(format, &text).is_some();
    let title = read_node(dir).map(|i| i.title).unwrap_or_default();
    Ok(Page { format, title, has_title, body, dir: read_direction(&text, format) })
}

/// Saved versions of the page's main file, newest first.
pub fn versions(dir: &Path) -> Result<Vec<Version>> {
    let (_, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let versions = version_dir(dir);
    let entries = match fs::read_dir(&versions) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::io("read", &versions, e)),
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::io("read", &versions, e))?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let info = dir.join("version.toml");
        let text = match fs::read_to_string(&info) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(StoreError::io("read", &info, e)),
        };
        let values = kv::parse(&text);
        let id = values.get("id").cloned().unwrap_or_default();
        if !valid_version_id(&id) || dir.file_name().and_then(|n| n.to_str()) != Some(id.as_str()) {
            continue;
        }
        let snapshot = dir.join(file.file_name().unwrap_or_default());
        let bytes = match fs::metadata(&snapshot) {
            Ok(meta) => meta.len(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(StoreError::io("read", &snapshot, e)),
        };
        found.push(Version { id, created: values.get("created").cloned().unwrap_or_default(), bytes });
    }
    found.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(found)
}

/// Restores an earlier version, first preserving the current file as another version.
pub fn restore_version(dir: &Path, id: &str) -> Result<()> {
    if !valid_version_id(id) {
        return Err(StoreError::invalid("that page version id is not valid"));
    }
    let (_, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let snapshot = version_dir(dir).join(id).join(file.file_name().unwrap_or_default());
    if !snapshot.is_file() {
        return Err(StoreError::NotFound(format!("page version {id}")));
    }
    let contents = fs::read_to_string(&snapshot).map_err(|e| StoreError::io("read", &snapshot, e))?;
    snapshot_current(dir, &file, true)?;
    write_atomic_file(&file, &contents)
}

fn version_dir(dir: &Path) -> PathBuf {
    dir.join(".notecraft").join("versions")
}

fn valid_version_id(id: &str) -> bool {
    id.len() == 20 && id.bytes().all(|b| b.is_ascii_hexdigit())
}

fn snapshot_current(dir: &Path, file: &Path, force: bool) -> Result<()> {
    if !file.is_file() {
        return Ok(());
    }
    let versions = version_dir(dir);
    let mut existing: Vec<PathBuf> = match fs::read_dir(&versions) {
        Ok(entries) => entries.filter_map(|entry| entry.ok().map(|e| e.path())).filter(|path| path.is_dir()).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(StoreError::io("read", &versions, e)),
    };
    existing.sort_by_key(|path| path.file_name().map(|name| name.to_os_string()));
    let current = fs::read(file).map_err(|e| StoreError::io("read", file, e))?;
    if let Some(last) = existing.last() {
        let previous = last.join(file.file_name().unwrap_or_default());
        if let Ok(bytes) = fs::read(&previous)
            && bytes == current
        {
            return Ok(());
        }
        let last_id = last.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let current_id = new_id();
        let recent = last_id.get(..16).and_then(|value| u128::from_str_radix(value, 16).ok());
        let now = current_id.get(..16).and_then(|value| u128::from_str_radix(value, 16).ok());
        if !force && recent.zip(now).is_some_and(|(before, after)| after.saturating_sub(before) < VERSION_INTERVAL_NS) {
            return Ok(());
        }
    }
    fs::create_dir_all(&versions).map_err(|e| StoreError::io("create", &versions, e))?;
    let id = new_id();
    let entry = versions.join(&id);
    fs::create_dir(&entry).map_err(|e| StoreError::io("create", &entry, e))?;
    let snapshot = entry.join(file.file_name().unwrap_or_default());
    if let Err(e) = fs::write(&snapshot, &current) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("write", &snapshot, e));
    }
    let info = format!("id = {}\ncreated = {}\n", kv::quote(&id), kv::quote(&iso_now()));
    if let Err(e) = fs::write(entry.join("version.toml"), info) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("write", &entry.join("version.toml"), e));
    }
    existing.push(entry);
    let mut total = 0u64;
    let mut sizes = Vec::with_capacity(existing.len());
    for version in &existing {
        let snapshot = version.join(file.file_name().unwrap_or_default());
        let size = fs::metadata(&snapshot).map(|meta| meta.len()).unwrap_or(0);
        total = total.saturating_add(size);
        sizes.push(size);
    }
    let mut remove_count = existing.len().saturating_sub(MAX_VERSIONS);
    let mut next = 0;
    while next < existing.len() && (remove_count > 0 || total > MAX_VERSION_BYTES) {
        total = total.saturating_sub(sizes[next]);
        let _ = fs::remove_dir_all(&existing[next]);
        remove_count = remove_count.saturating_sub(1);
        next += 1;
    }
    Ok(())
}

/// The editable part of a page's file: the inside of the HTML body, or the Markdown after the
/// front matter.
fn body_of(format: PageFormat, text: &str) -> String {
    match format {
        PageFormat::Html => html::inner(text),
        PageFormat::Cmgfm => frontmatter::split_front(text).1.trim_start_matches('\n').to_string(),
    }
}

/// The name the page direction goes by in both formats.
const DIR_KEY: &str = "dir";
const DIR_META: &str = "notecraft:dir";

/// The direction written in a page's own file, if any.
fn read_direction(text: &str, format: PageFormat) -> Option<Dir> {
    let raw = match format {
        PageFormat::Html => html::meta(text, DIR_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(text).0, DIR_KEY),
    }?;
    match raw.trim().to_ascii_lowercase().as_str() {
        "rtl" => Some(Dir::Rtl),
        "ltr" => Some(Dir::Ltr),
        _ => None,
    }
}

/// Which way the whole page reads, as written in its file.
pub fn direction(dir: &Path) -> Result<Option<Dir>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(read_direction(&text, format))
}

/// Writes the page's direction into its own file, so it travels with the folder.
/// `None` takes it out again and lets every line decide for itself.
pub fn set_direction(dir: &Path, want: Option<Dir>) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = match (format, want) {
        (PageFormat::Html, Some(d)) => html::set_meta(&text, DIR_META, d.id()),
        // An HTML page keeps the meta tag with an empty value rather than losing the line.
        (PageFormat::Html, None) => html::set_meta(&text, DIR_META, ""),
        (PageFormat::Cmgfm, want) => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match want {
                Some(d) => frontmatter::set_value(&front, DIR_KEY, d.id()),
                None => frontmatter::remove_value(&front, DIR_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

/// Where older versions kept the boxes: a line in the head or the front matter. Still read, so
/// an older page keeps its arrangement, and cleared the next time the boxes are written.
const BOXES_KEY: &str = "containers";
const BOXES_META: &str = "notecraft:containers";

/// The containers a page keeps. They are elements in the page's own text
/// ([`notecraft_formats::boxes`]); a page from an older version has them on a line in its head or
/// front matter instead, and that is read when the text has none. An empty list means the page
/// has never been arranged, and is one column of blocks.
pub fn containers(dir: &Path) -> Result<Vec<Container>> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if let (_, Some(in_the_text)) = boxes::read(format, &body_of(format, &text)) {
        return Ok(in_the_text);
    }
    let raw = match format {
        PageFormat::Html => html::meta(&text, BOXES_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, BOXES_KEY),
    };
    Ok(raw.map(|r| container::parse(&r)).unwrap_or_default())
}

/// Arranges the page's blocks in these boxes, by writing them into the page's own text as
/// elements around the blocks. An empty list takes the boxes out again: one column. The counts
/// must add up to the page's blocks, and a page with markup the block model would drop is
/// refused rather than rewritten.
pub fn set_containers(dir: &Path, arrangement: &[Container]) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = body_of(format, &text);
    if format == PageFormat::Html && html::unsupported_tag(&body).is_some() {
        return Err(StoreError::invalid("this page has markup the block editor would lose, so its boxes cannot be written"));
    }
    let (blocks, _) = boxes::read(format, &body);
    let wanted = (!arrangement.is_empty()).then_some(arrangement);
    if let Some(wanted) = wanted {
        let counted: usize = wanted.iter().map(|c| c.blocks).sum();
        if counted != blocks.len() {
            return Err(StoreError::invalid(format!("the boxes hold {counted} blocks but the page has {}", blocks.len())));
        }
    }
    save_body(dir, &boxes::write(format, &blocks, wanted))?;
    clear_old_boxes(dir)
}

/// The page's file text with the line an older version kept the boxes on taken out of the head
/// or the front matter. Unchanged when there is no such line.
fn without_old_boxes(format: PageFormat, text: &str) -> String {
    match format {
        PageFormat::Html if html::meta(text, BOXES_META).is_some() => html::set_meta(text, BOXES_META, ""),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(text);
            if frontmatter::value(&front, BOXES_KEY).is_none() {
                return text.to_string();
            }
            format!("{}{rest}", frontmatter::remove_value(&front, BOXES_KEY))
        }
        _ => text.to_string(),
    }
}

/// Takes the older line out of the page's file.
fn clear_old_boxes(dir: &Path) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = without_old_boxes(format, &text);
    if out == text { Ok(()) } else { write_atomic(&file, &out) }
}

/// The page file's head as text: the `<head>` of an HTML page, the front matter of a
/// Markdown one. What the page records about itself, for the code view to show.
pub fn head_text(dir: &Path) -> Result<String> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    Ok(match format {
        PageFormat::Html => match text.find("<body") {
            Some(at) => text[..at].to_string(),
            None => text,
        },
        PageFormat::Cmgfm => frontmatter::split_front(&text).0.to_string(),
    })
}

/// The ink files in a page's folder, in name order.
///
/// A file is ink because its SVG carries NoteCraft's ink namespace, not because of what
/// it is called: an ordinary picture dropped in the folder is never mistaken for one, and
/// neither is the plain copy that Save Picture writes. Only the first few kilobytes of
/// each file are read.
pub fn ink_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.to_ascii_lowercase().ends_with(".svg") || !entry.path().is_file() {
            continue;
        }
        let Ok(head) = read_head(&entry.path(), 4096) else { continue };
        if notecraft_ink::svg::is_ink(&head) {
            found.push(name);
        }
    }
    found.sort();
    found
}

/// The first `want` bytes of a file, as text. Used to look at a head without reading a
/// drawing that may be megabytes.
fn read_head(path: &Path, want: usize) -> std::io::Result<String> {
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut buffer = vec![0u8; want];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// The paper a page is drawn on, and the colour behind it. Both are the page's own, the
/// way OneNote gives a page its rule lines and its page colour, and both live in the head
/// beside the direction.
const PAPER_KEY: &str = "paper";
const PAPER_META: &str = "notecraft:paper";
const COLOUR_KEY: &str = "page-color";
const COLOUR_META: &str = "notecraft:page-color";

/// The paper and the colour a page carries, as they are written: `(paper, colour)`. Either
/// is `None` when the page says nothing and the window's own choice stands.
pub fn appearance(dir: &Path) -> Result<(Option<String>, Option<String>)> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let read = |meta: &str, key: &str| {
        match format {
            PageFormat::Html => html::meta(&text, meta),
            PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, key),
        }
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    };
    Ok((read(PAPER_META, PAPER_KEY), read(COLOUR_META, COLOUR_KEY)))
}

/// Writes them into the page's own file. `None` takes the line out again.
pub fn set_appearance(dir: &Path, paper: Option<&str>, colour: Option<&str>) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = match format {
        PageFormat::Html => {
            let text = html::set_meta(&text, PAPER_META, paper.unwrap_or_default());
            html::set_meta(&text, COLOUR_META, colour.unwrap_or_default())
        }
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front = match paper {
                Some(p) => frontmatter::set_value(&front, PAPER_KEY, p),
                None => frontmatter::remove_value(&front, PAPER_KEY),
            };
            let front = match colour {
                Some(c) => frontmatter::set_value(&front, COLOUR_KEY, c),
                None => frontmatter::remove_value(&front, COLOUR_KEY),
            };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const COLUMNS_KEY: &str = "columns";
const COLUMNS_META: &str = "notecraft:columns";

/// The column widths the page's tables were given. Empty means every table is drawn at
/// the usual width, which is what a page says until a column is dragged.
pub fn table_columns(dir: &Path) -> Result<tablecols::Widths> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let raw = match format {
        PageFormat::Html => html::meta(&text, COLUMNS_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, COLUMNS_KEY),
    };
    Ok(raw.map(|r| tablecols::parse(&r)).unwrap_or_default())
}

/// Writes them into the page's own file, beside the arrangement of its boxes.
pub fn set_table_columns(dir: &Path, widths: &tablecols::Widths) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let line = tablecols::write(widths);
    let out = match format {
        PageFormat::Html => html::set_meta(&text, COLUMNS_META, &line),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front =
                if widths.is_empty() { frontmatter::remove_value(&front, COLUMNS_KEY) } else { frontmatter::set_value(&front, COLUMNS_KEY, &line) };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

const JUSTIFY_KEY: &str = "justify";
const JUSTIFY_META: &str = "notecraft:justify";

/// Which paragraphs of the page are justified. Empty means every paragraph is drawn ragged
/// at its far edge, which is what a page says until one is asked for.
///
/// The line names the paragraphs by their place among the page's blocks; the words of the
/// page are not touched, so the file reads the same to any other program. See
/// [`notecraft_model::justify`] and `docs/page-model.md` section 2 for why it is here and
/// not in the markup.
pub fn justified(dir: &Path) -> Result<justify::Justified> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let raw = match format {
        PageFormat::Html => html::meta(&text, JUSTIFY_META),
        PageFormat::Cmgfm => frontmatter::value(&frontmatter::split_front(&text).0, JUSTIFY_KEY),
    };
    Ok(raw.map(|r| justify::parse(&r)).unwrap_or_default())
}

/// Writes them into the page's own file, beside the table column widths.
pub fn set_justified(dir: &Path, marked: &justify::Justified) -> Result<()> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let line = justify::write(marked);
    let out = match format {
        PageFormat::Html => html::set_meta(&text, JUSTIFY_META, &line),
        PageFormat::Cmgfm => {
            let (front, rest) = frontmatter::split_front(&text);
            let front =
                if marked.is_empty() { frontmatter::remove_value(&front, JUSTIFY_KEY) } else { frontmatter::set_value(&front, JUSTIFY_KEY, &line) };
            format!("{front}{rest}")
        }
    };
    write_atomic(&file, &out)
}

/// Saves the editable part of a page, keeping its metadata and updating `updated`.
/// Returns the new `updated` timestamp.
pub fn save_body(dir: &Path, body: &str) -> Result<String> {
    if body.len() > MAX_PAGE_BYTES {
        return Err(StoreError::invalid("this page is too large to save"));
    }
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let now = iso_now();
    let out = match format {
        PageFormat::Html => html::set_meta(&html::replace_inner(&text, body), "notecraft:updated", &now),
        PageFormat::Cmgfm => {
            let (front, _) = frontmatter::split_front(&text);
            let front = if front.is_empty() { front } else { kv::replace_line(&front, "updated:", &format!("updated: {now}")) };
            format!("{front}\n{}", body.trim_start_matches('\n'))
        }
    };
    // A body with its boxes in the text says everything the older line said, so the line goes.
    let out = if boxes::read(format, body).1.is_some() { without_old_boxes(format, &out) } else { out };
    write_atomic(&file, &out)?;
    Ok(now)
}

/// Writes a file by writing a temporary file next to it and renaming it over the
/// target, so a crash cannot leave half a page.
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && crate::tree::page_file(parent).is_some_and(|(_, page)| page == path)
    {
        snapshot_current(parent, path, false)?;
    }
    write_atomic_file(path, text)
}

fn write_atomic_file(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, text).map_err(|e| StoreError::io("write", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        StoreError::io("replace", path, e)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_versions_restore_the_saved_page_file() {
        let t = crate::testing::TempDir::new("page-versions");
        let settings = crate::settings::Settings::at(t.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(t.path(), "Notebook", None, &settings).expect("notebook");
        let page = crate::ops::create_node(&notebook, "Page", notecraft_model::NodeKind::Page, None, &settings).expect("page");
        let initial = load_page(&page).expect("initial page").body;

        save_body(&page, "first version").expect("first save");
        let saved_versions = versions(&page).expect("versions");
        assert_eq!(saved_versions.len(), 1);
        save_body(&page, "second version").expect("second save");
        restore_version(&page, &saved_versions[0].id).expect("restore");
        assert_eq!(load_page(&page).expect("page").body, initial);
        assert_eq!(versions(&page).expect("versions after restore").len(), 2);
    }

    /// A file is ink because of the namespace inside it, not because of its name.
    #[test]
    fn the_ink_files_of_a_page_are_the_ones_with_our_namespace() {
        let t = crate::testing::TempDir::new("ink-files");
        let s = crate::settings::Settings::at(t.path().join("s.toml"));
        let nb = crate::ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = crate::ops::create_node(&nb, "P", notecraft_model::NodeKind::Page, None, &s).expect("page");

        let ours = "<?xml version=\"1.0\"?>
<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"><title>Ink</title></svg>";
        fs::write(page.join("ink.svg"), ours).expect("write");
        fs::write(page.join("ink-2.svg"), ours).expect("write");
        // A picture of someone's own, and the plain copy the Draw tab writes: neither is ink.
        fs::write(page.join("plan.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect/></svg>").expect("write");
        fs::write(page.join("ink-picture.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"><path d=\"M0 0\"/></svg>").expect("write");
        fs::write(page.join("notes.txt"), "words").expect("write");

        assert_eq!(ink_files(&page), vec!["ink-2.svg".to_string(), "ink.svg".to_string()], "named in order, and only ours");
    }

    use super::*;
    use crate::ops;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::NodeKind;

    fn setup(label: &str, format: PageFormat) -> (TempDir, std::path::PathBuf) {
        let t = TempDir::new(label);
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "Page", NodeKind::Page, Some(format), &s).expect("page");
        (t, page)
    }

    #[test]
    fn html_round_trip_keeps_the_head() {
        let (_t, page) = setup("page1", PageFormat::Html);
        let loaded = load_page(&page).expect("load");
        assert_eq!((loaded.format, loaded.title.as_str()), (PageFormat::Html, "Page"));
        // A new page's body is its date stamp; the title is in the head.
        assert!(!loaded.body.contains("<h1>") && loaded.body.contains("page-date"), "{}", loaded.body);
        save_body(&page, "<h1>Page</h1><p>Hello</p>").expect("save");
        let again = load_page(&page).expect("load");
        assert_eq!(again.body, "<h1>Page</h1><p>Hello</p>");
        let raw = fs::read_to_string(page.join("index.html")).expect("raw");
        assert!(raw.contains("<title>Page</title>") && raw.contains("name=\"notecraft:id\""));
        assert!(raw.contains("name=\"notecraft:updated\""));
    }

    #[test]
    fn markdown_round_trip_keeps_the_front_matter() {
        let (_t, page) = setup("page2", PageFormat::Cmgfm);
        let before = fs::read_to_string(page.join("index.md")).expect("raw");
        save_body(&page, "# Page\n\n- [ ] one\n").expect("save");
        let after = fs::read_to_string(page.join("index.md")).expect("raw");
        let (front_before, _) = frontmatter::split_front(&before);
        let (front_after, _) = frontmatter::split_front(&after);
        assert_eq!(frontmatter::value(&front_before, "id"), frontmatter::value(&front_after, "id"), "id is stable");
        assert_eq!(frontmatter::value(&front_before, "created"), frontmatter::value(&front_after, "created"));
        assert_eq!(load_page(&page).expect("load").body, "# Page\n\n- [ ] one\n");
    }

    /// A justified paragraph is a line in the page's head, and the words of the page are
    /// not touched by it: a page saved and reopened comes back with the same body and the
    /// same paragraphs justified, in both formats.
    #[test]
    fn a_justified_paragraph_survives_a_save_and_a_reopen() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("justified", format);
            assert!(justified(&dir).expect("read").is_empty(), "{format:?}: a new page has nothing justified");

            let body = match format {
                PageFormat::Html => "<p>One</p><p>Two</p><p>Three</p>",
                PageFormat::Cmgfm => "One\n\nTwo\n\nThree\n",
            };
            save_body(&dir, body).expect("save");
            let mut marked = justify::Justified::default();
            marked.set(1, Some(justify::Strength::High));
            set_justified(&dir, &marked).expect("write");

            assert_eq!(justified(&dir).expect("read"), marked, "{format:?}");
            assert_eq!(justified(&dir).expect("read").strength_of(1), Some(justify::Strength::High), "{format:?}: and the strength with it");
            // The words are untouched: no tatweel, no marker, nothing but what was typed.
            assert_eq!(load_page(&dir).expect("load").body, body, "{format:?}: the body is the same text");

            // Saving the body again keeps the line, because justification is metadata.
            save_body(&dir, body).expect("save");
            assert_eq!(justified(&dir).expect("read"), marked, "{format:?}: an edit elsewhere does not lose it");

            // And taking the mark off takes the line out again.
            marked.set(1, None);
            set_justified(&dir, &marked).expect("write");
            assert!(justified(&dir).expect("read").is_empty(), "{format:?}");
            let raw = fs::read_to_string(page_file(&dir).expect("page").1).expect("raw");
            if format == PageFormat::Cmgfm {
                assert!(!raw.contains("justify:"), "the line is gone, not left empty: {raw}");
            }
        }
    }

    #[test]
    fn saving_a_body_with_boxes_in_it_drops_the_older_line_and_a_plain_one_keeps_it() {
        for format in [PageFormat::Cmgfm, PageFormat::Html] {
            let (_t, dir) = setup("older-boxes", format);
            let file = page_file(&dir).expect("page").1;
            let text = fs::read_to_string(&file).expect("read");
            // An older page: the boxes on a line in the front matter or the head.
            let older = match format {
                PageFormat::Html => html::set_meta(&text, BOXES_META, "0,0,300,1"),
                PageFormat::Cmgfm => text.replacen("\ntitle:", "\ncontainers: 0,0,300,1\ntitle:", 1),
            };
            fs::write(&file, &older).expect("write");
            assert_eq!(containers(&dir).expect("read").len(), 1, "{format:?}: the older line is read");

            // A plain body does not touch it.
            save_body(&dir, "one\n").expect("save plain");
            let kept = fs::read_to_string(&file).expect("read");
            assert!(kept.contains("0,0,300,1"), "{format:?}: a plain save leaves the older line alone:\n{kept}");

            // A body with its boxes in it replaces it.
            let body =
                notecraft_formats::boxes::write(format, &notecraft_formats::markdown::parse("one\n"), Some(&[Container::new(10.0, 10.0, 300.0, 1)]));
            save_body(&dir, &body).expect("save boxed");
            let now = fs::read_to_string(&file).expect("read");
            assert!(!now.contains("0,0,300,1"), "{format:?}: the older line is gone:\n{now}");
            assert_eq!(
                containers(&dir).expect("read"),
                vec![Container::new(10.0, 10.0, 300.0, 1)],
                "{format:?}: the boxes in the text are what is read"
            );
        }
    }

    #[test]
    fn saving_twice_is_stable() {
        let (_t, page) = setup("page3", PageFormat::Cmgfm);
        save_body(&page, "text\n").expect("save");
        let first = fs::read_to_string(page.join("index.md")).expect("raw");
        save_body(&page, "text\n").expect("save");
        let second = fs::read_to_string(page.join("index.md")).expect("raw");
        let strip = |s: &str| s.lines().filter(|l| !l.starts_with("updated:")).collect::<Vec<_>>().join("\n");
        assert_eq!(strip(&first), strip(&second));
    }

    #[test]
    fn no_temp_file_is_left_behind() {
        let (_t, page) = setup("page4", PageFormat::Html);
        save_body(&page, "<p>x</p>").expect("save");
        let names: Vec<String> =
            fs::read_dir(&page).expect("dir").filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains("tmp-write")), "{names:?}");
    }

    #[test]
    fn a_folder_that_is_not_a_page_is_an_error() {
        let t = TempDir::new("page5");
        assert!(matches!(load_page(t.path()), Err(StoreError::NotFound(_))));
        assert!(matches!(save_body(t.path(), "x"), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn unusual_text_survives() {
        let (_t, page) = setup("page6", PageFormat::Cmgfm);
        let body = "caf\u{e9} \u{1F600}\r\nline two\n\n    indented\n--- not front matter\n";
        save_body(&page, body).expect("save");
        assert_eq!(load_page(&page).expect("load").body, body);
    }
}

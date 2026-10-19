//! Reading and writing the content of a page.

use crate::error::{Result, StoreError};
use crate::tree::{page_file, read_node};
use notecraft_formats::{boxes, frontmatter, html};
use notecraft_model::{Container, Dir, PageFormat, container, kv, tablecols, time::iso_now};
use std::fs;
use std::path::Path;

/// Pages larger than this are refused (a guard against hostile or accidental input).
pub const MAX_PAGE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page {
    pub format: PageFormat,
    pub title: String,
    /// The editable part: the inside of the HTML body, or the Markdown after the front matter.
    pub body: String,
    /// Which way the whole page reads, if the writer said. `None` means each line decides
    /// for itself, from its own first letter.
    pub dir: Option<Dir>,
}

pub fn load_page(dir: &Path) -> Result<Page> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let meta = fs::metadata(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if meta.len() > MAX_PAGE_BYTES as u64 {
        return Err(StoreError::invalid("this page is too large to open"));
    }
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = body_of(format, &text);
    let title = read_node(dir).map(|i| i.title).unwrap_or_default();
    Ok(Page { format, title, body, dir: read_direction(&text, format) })
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
    let tmp = path.with_extension("tmp-write");
    fs::write(&tmp, text).map_err(|e| StoreError::io("write", &tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        StoreError::io("replace", path, e)
    })
}

#[cfg(test)]
mod tests {
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
        assert!(loaded.body.starts_with("<h1>Page</h1>") && loaded.body.contains("page-date"), "{}", loaded.body);
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

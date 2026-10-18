//! Reading and writing the content of a page.

use crate::error::{Result, StoreError};
use crate::tree::{page_file, read_node};
use notecraft_formats::{frontmatter, html};
use notecraft_model::{PageFormat, kv, time::iso_now};
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
}

pub fn load_page(dir: &Path) -> Result<Page> {
    let (format, file) = page_file(dir).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", dir.display())))?;
    let meta = fs::metadata(&file).map_err(|e| StoreError::io("read", &file, e))?;
    if meta.len() > MAX_PAGE_BYTES as u64 {
        return Err(StoreError::invalid("this page is too large to open"));
    }
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let body = match format {
        PageFormat::Html => html::inner(&text),
        PageFormat::Cmgfm => frontmatter::split_front(&text).1.trim_start_matches('\n').to_string(),
    };
    let title = read_node(dir).map(|i| i.title).unwrap_or_default();
    Ok(Page { format, title, body })
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

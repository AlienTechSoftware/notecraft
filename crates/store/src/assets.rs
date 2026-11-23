//! Attachments: images, video, audio and other files stored beside a page, in its
//! `assets` folder, and linked from the page by a relative path (`assets/name.png`).

use crate::error::{Result, StoreError};
use crate::tree::page_file;
use notecraft_model::names;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[cfg(target_os = "windows")]
#[path = "assets.win.rs"]
mod platform;

/// Attachments larger than this are refused.
pub const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Other,
}

pub fn media_kind(name: &str) -> MediaKind {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "jxl" => MediaKind::Image,
        "mp4" | "webm" | "mov" | "mkv" | "avi" => MediaKind::Video,
        "mp3" | "wav" | "ogg" | "m4a" | "flac" => MediaKind::Audio,
        _ => MediaKind::Other,
    }
}

fn assets_dir(page_dir: &Path) -> Result<std::path::PathBuf> {
    if page_file(page_dir).is_none() {
        return Err(StoreError::invalid("attachments belong to a page"));
    }
    let dir = page_dir.join("assets");
    fs::create_dir_all(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    Ok(dir)
}

/// Where a page records which files in its `assets` folder are icons NoteCraft made for an
/// attachment, one name to a line. A file is only ever treated as a generated icon, hidden
/// from the attachment list and removed with its attachment, when it is recorded here or the
/// page shows it as an attachment's icon: a name alone, such as `report.pdf.png`, may just as
/// well be a picture the user attached (issue #103).
const ICON_RECORD: &str = ".notecraft/icons";

fn recorded_icons(page_dir: &Path) -> HashSet<String> {
    fs::read_to_string(page_dir.join(ICON_RECORD))
        .map(|text| text.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

fn set_recorded(page_dir: &Path, icon: &str, recorded: bool) -> Result<()> {
    let mut icons = recorded_icons(page_dir);
    let changed = if recorded { icons.insert(icon.to_string()) } else { icons.remove(icon) };
    if !changed {
        return Ok(());
    }
    let path = page_dir.join(ICON_RECORD);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| StoreError::io("create", parent, e))?;
    }
    let mut names: Vec<String> = icons.into_iter().collect();
    names.sort();
    let text = names.iter().map(|n| format!("{n}\n")).collect::<String>();
    fs::write(&path, text).map_err(|e| StoreError::io("write", &path, e))
}

/// Whether a file of this name in the page's `assets` folder may be written as an icon: there
/// is none yet, or the one there is an icon NoteCraft made. A user's own file is never replaced.
fn icon_slot_free(page_dir: &Path, icon: &str) -> bool {
    !page_dir.join("assets").join(icon).exists() || is_generated_icon(page_dir, icon)
}

pub fn ensure_icon(page_dir: &Path, file_name: &str) -> Result<()> {
    let dir = page_dir.join("assets");
    fs::create_dir_all(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    let safe = names::safe_file_name(file_name);
    #[cfg(target_os = "windows")]
    if icon_slot_free(page_dir, &format!("{safe}.png")) {
        let target = dir.join(format!("{safe}.png"));
        let source = dir.join(file_name);
        match platform::shell_icon(&source, &target) {
            Ok(()) => {
                set_recorded(page_dir, &format!("{safe}.png"), true)?;
                let fallback = format!("{safe}.svg");
                if dir.join(&fallback).is_file() && is_generated_icon(page_dir, &fallback) {
                    fs::remove_file(dir.join(&fallback)).map_err(|e| StoreError::io("remove", &dir.join(&fallback), e))?;
                    set_recorded(page_dir, &fallback, false)?;
                }
                log::info!("attachment icon generated from Windows Shell: source={:?} target={:?}", source, target);
                return Ok(());
            }
            Err(error) => {
                log::warn!(
                    "Windows Shell attachment icon extraction failed; using SVG fallback: source={:?} target={:?} error={error}",
                    source,
                    target
                );
            }
        }
    }
    let icon = format!("{safe}.svg");
    if !icon_slot_free(page_dir, &icon) {
        log::warn!("attachment icon not written: a file of the user's is already called {icon:?}");
        return Ok(());
    }
    let ext = file_name.rsplit_once('.').map(|(_, e)| e.to_ascii_uppercase()).unwrap_or_else(|| "FILE".into());
    let target = dir.join(&icon);
    let colour = match ext.as_str() {
        "PDF" => "#d93025",
        "DOC" | "DOCX" => "#2563eb",
        "XLS" | "XLSX" => "#16803c",
        "PPT" | "PPTX" => "#d24726",
        _ => "#6f42c1",
    };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="112" viewBox="0 0 96 112"><path fill="white" stroke="{colour}" stroke-width="6" d="M12 3h50l22 22v84H12z"/><path fill="{colour}" d="M62 3v24h22z"/><text x="48" y="78" text-anchor="middle" font-family="sans-serif" font-size="18" font-weight="700" fill="{colour}">{ext}</text></svg>"#
    );
    fs::write(&target, svg).map_err(|e| StoreError::io("write", &target, e))?;
    set_recorded(page_dir, &icon, true)?;
    log::info!("attachment icon generated from SVG fallback: target={:?}", target);
    Ok(())
}

fn unique_name(dir: &Path, name: &str) -> String {
    let taken: HashSet<String> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_lowercase()).collect(),
        Err(_) => HashSet::new(),
    };
    // The suffix goes in before the extension and within the name's length limit, so the name
    // made is one `remove` accepts as a safe name (issue #131).
    let (stem, ext) = names::split_extension(name);
    let mut candidate = name.to_string();
    let mut n = 2;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = names::fit_file_name(stem, ext, &format!("-{n}"));
        n += 1;
    }
    candidate
}

/// Stores bytes as an attachment of a page. Returns the relative link, `assets/<name>`.
pub fn attach_bytes(page_dir: &Path, name: &str, bytes: &[u8]) -> Result<String> {
    if bytes.len() as u64 > MAX_ASSET_BYTES {
        return Err(StoreError::invalid("this file is too large to attach"));
    }
    let dir = assets_dir(page_dir)?;
    let name = unique_name(&dir, &names::safe_file_name(name));
    let target = dir.join(&name);
    fs::write(&target, bytes).map_err(|e| StoreError::io("write", &target, e))?;
    Ok(format!("assets/{name}"))
}

/// Takes a file out of the page's attachments folder.
///
/// `name` is a plain file name, as [`list`] reports it: anything with a separator or a
/// `..` in it is refused, so this can never reach out of the page's own folder. The page
/// is not touched; a line that linked to the file is left as the writer wrote it, and
/// shows as a link to something that is no longer there.
/// Sets the alt text of a picture in its page, where the page shows it: the `alt` of the `<img>` in
/// an HTML page, or the `![...]` of an image in a CommonMark page. An empty text leaves an empty
/// alt. The page's other text is kept as it is. Alt text is written here, by a person.
pub fn set_alt_text(page_dir: &Path, picture: &str, text: &str) -> Result<()> {
    if picture.is_empty() || picture.starts_with('.') || picture.contains(['/', '\\']) {
        return Err(StoreError::invalid("a picture is named by its file name in the page's assets"));
    }
    if !page_dir.join("assets").join(picture).is_file() {
        return Err(StoreError::NotFound(format!("no picture {picture} in this page")));
    }
    let page = crate::page::load_page(page_dir)?;
    let target = format!("assets/{picture}");
    let (body, found) = match page.format {
        notecraft_model::PageFormat::Html => set_html_alt(&page.body, &target, text),
        notecraft_model::PageFormat::Cmgfm => set_markdown_alt(&page.body, &target, text),
    };
    if !found {
        return Err(StoreError::NotFound(format!("this page does not show {picture}")));
    }
    crate::page::save_body(page_dir, &body)?;
    Ok(())
}

/// The alt text of every picture the page shows, with the picture's name: from `<img alt>` in HTML,
/// from `![alt](...)` in CommonMark.
pub fn alt_texts(body: &str, format: notecraft_model::PageFormat) -> Vec<(String, String)> {
    match format {
        notecraft_model::PageFormat::Html => html_images(body)
            .into_iter()
            .filter_map(|tag| {
                let src = attr_value(&tag, "src")?;
                let alt = attr_value(&tag, "alt")?;
                let name = src.strip_prefix("assets/")?.to_string();
                (!alt.trim().is_empty()).then_some((name, alt))
            })
            .collect(),
        notecraft_model::PageFormat::Cmgfm => markdown_images(body)
            .into_iter()
            .filter(|(alt, target)| !alt.trim().is_empty() && target.starts_with("assets/"))
            .map(|(alt, target)| (target["assets/".len()..].to_string(), alt))
            .collect(),
    }
}

/// Where each `<img ...>` tag starts and ends in the body, in order. The tag name is matched in any
/// letter case, and only where it is a whole word (`<img>`, `<img src=...>`, `<IMG/>`).
fn html_image_spans(body: &str) -> Vec<(usize, usize)> {
    let lower = body.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(rel) = lower[from..].find("<img") {
        let start = from + rel;
        let after = lower[start + 4..].chars().next();
        if !matches!(after, Some(c) if c.is_ascii_whitespace() || c == '>' || c == '/') {
            from = start + 4;
            continue;
        }
        // The tag ends at the first `>` that is outside quotes.
        let mut quote: Option<char> = None;
        let mut end = None;
        for (i, c) in body[start..].char_indices() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), _) => {}
                (None, '"' | '\'') => quote = Some(c),
                (None, '>') => {
                    end = Some(start + i + 1);
                    break;
                }
                _ => {}
            }
        }
        let Some(end) = end else { break };
        out.push((start, end));
        from = end;
    }
    out
}

/// One attribute of an HTML tag: where it sits, its lowercase name, and its unescaped value (empty
/// when it has none, as in `<img hidden>`).
struct HtmlAttr {
    start: usize,
    end: usize,
    name: String,
    value: String,
}

/// The attributes of a tag such as `<img src="a.png" alt='x'>`. Names are case-insensitive; values may be
/// double-quoted, single-quoted or bare, and may have spaces around `=`.
fn html_attrs(tag: &str) -> Vec<HtmlAttr> {
    let bytes = tag.as_bytes();
    let mut out = Vec::new();
    let mut i = tag.find(|c: char| c.is_ascii_whitespace()).unwrap_or(tag.len());
    loop {
        while i < bytes.len() && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] == b'>' {
            break;
        }
        let start = i;
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'=' && bytes[i] != b'>' && bytes[i] != b'/' {
            i += 1;
        }
        let name = tag[start..i].to_ascii_lowercase();
        let mut j = i;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut value = String::new();
        if j < bytes.len() && bytes[j] == b'=' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                let q = bytes[j];
                let v_start = j + 1;
                j = v_start;
                while j < bytes.len() && bytes[j] != q {
                    j += 1;
                }
                value = unescape_html(&tag[v_start..j.min(tag.len())]);
                j = (j + 1).min(bytes.len());
            } else {
                let v_start = j;
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' {
                    j += 1;
                }
                value = unescape_html(&tag[v_start..j]);
            }
            i = j;
        }
        if !name.is_empty() {
            out.push(HtmlAttr { start, end: i, name, value });
        }
    }
    out
}

/// Undoes the entities the writer uses, and the common named ones.
fn unescape_html(raw: &str) -> String {
    raw.replace("&quot;", "\"").replace("&#39;", "'").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// Every `<img>` tag in the body, in order.
fn html_images(body: &str) -> Vec<String> {
    html_image_spans(body).into_iter().map(|(a, b)| body[a..b].to_string()).collect()
}

/// The value of an attribute in an HTML tag, unescaped for the few characters written here.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    html_attrs(tag).into_iter().find(|a| a.name == name).map(|a| a.value)
}

/// Replaces the alt of the `<img>` whose source is `target`. Returns the body and whether it was found.
fn set_html_alt(body: &str, target: &str, text: &str) -> (String, bool) {
    let mut out = String::new();
    let mut last = 0;
    let mut found = false;
    for (start, end) in html_image_spans(body) {
        let tag = &body[start..end];
        out.push_str(&body[last..start]);
        if attr_value(tag, "src").as_deref() == Some(target) {
            found = true;
            out.push_str(&with_html_alt(tag, text));
        } else {
            out.push_str(tag);
        }
        last = end;
    }
    out.push_str(&body[last..]);
    (out, found)
}

/// The tag with its alt set to `text`. Every existing alt is removed first, whatever its quotes or
/// case, so a tag can never end up with two; the new alt goes before the closing `>` (or `/>`).
fn with_html_alt(tag: &str, text: &str) -> String {
    let value = text.trim().replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
    let attrs = html_attrs(tag);
    let mut out = String::new();
    let mut last = 0;
    for a in attrs.iter().filter(|a| a.name == "alt") {
        out.push_str(&tag[last..a.start]);
        last = a.end;
    }
    out.push_str(&tag[last..]);
    // Put the new alt just before the tag's closing `>` (or `/>`).
    let close = out.rfind('>').unwrap_or(out.len());
    let (head, tail) = out.split_at(close);
    let head = head.trim_end().trim_end_matches('/').trim_end();
    format!("{head} alt=\"{value}\"{tail}")
}

/// One CommonMark image: where its alt text sits in the body, and its target.
struct MarkdownImage {
    alt_start: usize,
    alt_end: usize,
    target: String,
}

/// Every `![alt](target)` in the body, in order. The alt text ends at the first `](` that is not
/// escaped, so a link like `[name](assets/a.png)` is never mistaken for an image. An image with no
/// closing `)` on its line is skipped.
fn markdown_images_at(body: &str) -> Vec<MarkdownImage> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(rel) = body[i..].find("![") {
        // An image inside a link's label (an attachment card's icon) is not the picture's own alt text.
        if body[..i + rel].ends_with('[') {
            i += rel + 2;
            continue;
        }
        let start = i + rel + 2;
        let mut j = start;
        let mut alt_end = None;
        while j < body.len() {
            let rest = &body[j..];
            if rest.starts_with('\\') {
                j += rest.chars().nth(1).map_or(1, char::len_utf8) + 1;
                continue;
            }
            if rest.starts_with("](") {
                alt_end = Some(j);
                break;
            }
            j += rest.chars().next().map_or(1, char::len_utf8);
        }
        let Some(alt_end) = alt_end else {
            i = start;
            continue;
        };
        let target_start = alt_end + 2;
        match body[target_start..].find(')') {
            Some(close) if !body[target_start..target_start + close].contains('\n') => {
                let target = body[target_start..target_start + close].to_string();
                out.push(MarkdownImage { alt_start: start, alt_end, target });
                i = target_start + close + 1;
            }
            _ => i = start,
        }
    }
    out
}

/// Undoes the escapes the writer adds: a backslash before punctuation stands for that character.
fn unescape_alt(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some_and(|n| n.is_ascii_punctuation()) {
            out.push(chars.next().unwrap_or(c));
        } else {
            out.push(c);
        }
    }
    out
}

/// The alt text as it is written in Markdown: escaped so it reads back unchanged, on one line.
fn escape_alt(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| match c {
            '\\' | '[' | ']' => format!("\\{c}"),
            '\n' | '\r' => " ".to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// The `(alt, target)` of every CommonMark image in the body.
fn markdown_images(body: &str) -> Vec<(String, String)> {
    markdown_images_at(body).into_iter().map(|img| (unescape_alt(&body[img.alt_start..img.alt_end]), img.target)).collect()
}

/// Replaces the alt of every image whose target is `target`. Edits by exact span, from the end of the
/// body backwards, so a link or another image in the same line is left as it is. Returns the body
/// and whether an image was found.
fn set_markdown_alt(body: &str, target: &str, text: &str) -> (String, bool) {
    let escaped = escape_alt(text);
    let mut out = body.to_string();
    let mut found = false;
    for img in markdown_images_at(body).into_iter().rev() {
        if img.target == target {
            out.replace_range(img.alt_start..img.alt_end, &escaped);
            found = true;
        }
    }
    (out, found)
}

pub fn remove(page_dir: &Path, name: &str) -> Result<()> {
    let safe = names::safe_file_name(name);
    if name.is_empty() || safe != name {
        return Err(StoreError::invalid("that is not a file in this page's folder"));
    }
    let file = page_dir.join("assets").join(name);
    if !file.is_file() {
        return Err(StoreError::NotFound(format!("no attachment called {name}")));
    }
    // Generated application icons belong to the attachment, not to the user's file list. Which
    // ones are icons is decided while the attachment is still there, and a file of the user's
    // that only shares the name is left alone.
    let icons: Vec<String> =
        ["png", "svg"].iter().map(|extension| format!("{name}.{extension}")).filter(|icon| is_generated_icon(page_dir, icon)).collect();
    fs::remove_file(&file).map_err(|e| StoreError::io("remove", &file, e))?;
    for icon in icons {
        let path = page_dir.join("assets").join(&icon);
        if path.is_file() {
            fs::remove_file(&path).map_err(|e| StoreError::io("remove", &path, e))?;
        }
        set_recorded(page_dir, &icon, false)?;
    }
    Ok(())
}

/// Copies a file into the page's attachments. Returns the relative link.
pub fn attach_file(page_dir: &Path, source: &Path) -> Result<String> {
    let meta = fs::metadata(source).map_err(|e| StoreError::io("read", source, e))?;
    if !meta.is_file() {
        return Err(StoreError::invalid("only files can be attached"));
    }
    if meta.len() > MAX_ASSET_BYTES {
        return Err(StoreError::invalid("this file is too large to attach"));
    }
    let dir = assets_dir(page_dir)?;
    let name = unique_name(&dir, &names::safe_file_name(&source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    let target = dir.join(&name);
    fs::copy(source, &target).map_err(|e| StoreError::io("copy", &target, e))?;
    ensure_icon(page_dir, &name)?;
    Ok(format!("assets/{name}"))
}

/// The attachments of a page: file name and size, sorted by name.
pub fn list(page_dir: &Path) -> Vec<(String, u64)> {
    let owners = IconOwners::of(page_dir);
    let mut out: Vec<(String, u64)> = match fs::read_dir(page_dir.join("assets")) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().map(|m| m.len()).unwrap_or(0)))
            .filter(|(name, _)| !owners.owns(page_dir, name))
            .collect(),
        Err(_) => Vec::new(),
    };
    out.sort();
    out
}

/// Whether `name` in the page's `assets` folder is an icon NoteCraft made for the attachment
/// whose name it extends: recorded when it was made, or, for one made before icons were
/// recorded, drawn on the page as an attachment card's icon. A user's picture is never written
/// that way: it has its own name as its alt text.
fn is_generated_icon(page_dir: &Path, name: &str) -> bool {
    IconOwners::of(page_dir).owns(page_dir, name)
}

/// What says which files are generated icons, read once for a whole folder.
struct IconOwners {
    recorded: HashSet<String>,
    page: String,
}

impl IconOwners {
    fn of(page_dir: &Path) -> Self {
        let page = page_file(page_dir).and_then(|(_, file)| fs::read_to_string(file).ok()).unwrap_or_default();
        IconOwners { recorded: recorded_icons(page_dir), page }
    }

    fn owns(&self, page_dir: &Path, name: &str) -> bool {
        let Some((source, extension)) = name.rsplit_once('.') else { return false };
        if !matches!(extension.to_ascii_lowercase().as_str(), "png" | "svg") || !page_dir.join("assets").join(source).is_file() {
            return false;
        }
        self.recorded.contains(name) || self.drawn_as_card_icon(name)
    }

    /// Whether the page draws `name` as the icon of an attachment card: in HTML an `<img>` with
    /// an empty alt text, in CMGFM a picture that is the start of a link's label,
    /// `[![label](assets/name) label](assets/file)`. A picture the user attached is neither.
    fn drawn_as_card_icon(&self, name: &str) -> bool {
        if self.page.contains(&format!("<img src=\"assets/{name}\" alt=\"\"")) {
            return true;
        }
        let address = format!("](assets/{name})");
        self.page.match_indices(&address).any(|(at, _)| {
            let before = &self.page[..at];
            before.rfind("![").is_some_and(|open| before[..open].ends_with('[') && !before[open..].contains(['\n', ']']))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::NodeKind;

    fn page(t: &TempDir) -> std::path::PathBuf {
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        ops::create_node(&nb, "P", NodeKind::Page, None, &s).expect("page")
    }

    #[test]
    fn alt_text_goes_into_the_html_img_and_is_escaped() {
        let t = TempDir::new("assets-alt-html");
        let page = page(&t);
        attach_bytes(&page, "a.png", b"x").expect("attach");
        crate::page::save_body(&page, "<p>Hi</p><img src=\"assets/a.png\" width=\"40\">").expect("save");
        set_alt_text(&page, "a.png", " a \"quoted\" <b> & more ").expect("alt");
        let body = crate::page::load_page(&page).expect("load").body;
        assert!(body.contains("<img src=\"assets/a.png\" width=\"40\" alt=\"a &quot;quoted&quot; &lt;b&gt; &amp; more\">"), "{body}");
        assert_eq!(alt_texts(&body, notecraft_model::PageFormat::Html), vec![("a.png".to_string(), "a \"quoted\" <b> & more".to_string())]);
        set_alt_text(&page, "a.png", "changed").expect("alt again");
        let body = crate::page::load_page(&page).expect("load").body;
        assert_eq!(body.matches("alt=").count(), 1, "replaced, not added: {body}");
        assert!(body.contains("alt=\"changed\""), "{body}");
    }

    fn markdown_page(t: &TempDir, body: &str) -> std::path::PathBuf {
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "P", NodeKind::Page, Some(notecraft_model::PageFormat::Cmgfm), &s).expect("page");
        attach_bytes(&page, "cat.png", b"x").expect("cat");
        attach_bytes(&page, "a.png", b"x").expect("a");
        crate::page::save_body(&page, body).expect("save");
        page
    }

    #[test]
    fn a_plain_link_to_the_same_picture_is_never_edited_or_deleted() {
        let t = TempDir::new("assets-md-link");
        let body_before = "![cat](assets/cat.png) [download](assets/a.png)\n";
        let page = markdown_page(&t, body_before);
        // The page shows a.png only as a link, not as a picture: the edit is refused, and nothing changes.
        assert!(set_alt_text(&page, "a.png", "new alt").is_err(), "a link is not a picture to describe");
        let body = crate::page::load_page(&page).expect("load").body;
        assert_eq!(body, body_before, "the page is unchanged");
    }

    #[test]
    fn a_link_beside_an_image_of_the_same_file_keeps_both() {
        let t = TempDir::new("assets-md-link-and-image");
        let page = markdown_page(&t, "![old](assets/a.png) [download](assets/a.png)\n");
        set_alt_text(&page, "a.png", "new alt").expect("writes the image's alt");
        let body = crate::page::load_page(&page).expect("load").body;
        assert_eq!(body, "![new alt](assets/a.png) [download](assets/a.png)\n", "only the image's alt changed");
    }

    #[test]
    fn alt_text_with_brackets_backslashes_and_newlines_reads_back_unchanged() {
        let t = TempDir::new("assets-md-escapes");
        let page = markdown_page(&t, "![old](assets/a.png)\n");
        let text = "see ](x) and C:\\new folder [1]\nsecond line";
        set_alt_text(&page, "a.png", text).expect("writes");
        let body = crate::page::load_page(&page).expect("load").body;
        let read = alt_texts(&body, notecraft_model::PageFormat::Cmgfm);
        assert_eq!(read, vec![("a.png".to_string(), "see ](x) and C:\\new folder [1] second line".to_string())], "{body}");
        set_alt_text(&page, "a.png", "again").expect("a second edit still finds the image");
        let body = crate::page::load_page(&page).expect("load").body;
        assert!(body.contains("![again](assets/a.png)"), "{body}");
    }

    #[test]
    fn two_images_on_one_line_are_each_found_by_their_own_target() {
        let t = TempDir::new("assets-md-two");
        let page = markdown_page(
            &t,
            "![cat](assets/cat.png) ![one](assets/a.png)
",
        );
        set_alt_text(&page, "a.png", "second").expect("writes");
        let body = crate::page::load_page(&page).expect("load").body;
        assert!(body.contains("![cat](assets/cat.png) ![second](assets/a.png)"), "{body}");
    }

    #[test]
    fn an_unterminated_quote_changes_nothing_and_reports_not_found() {
        let body = "<p>x</p><img src=\"assets/a.png\" alt=\"old>";
        let (out, found) = set_html_alt(body, "assets/a.png", "new");
        assert!(!found, "a tag with an open quote is not a picture");
        assert_eq!(out, body, "the body is unchanged");
    }

    #[test]
    fn html_alt_is_found_and_replaced_whatever_the_quotes_case_or_spacing() {
        for body in ["<IMG SRC=\"assets/a.png\" ALT='old'>", "<img src = 'assets/a.png' alt = \"old\" />", "<img alt=old src=assets/a.png>"] {
            let (out, found) = set_html_alt(body, "assets/a.png", "new");
            assert!(found, "{body}");
            assert_eq!(out.matches("alt").count(), 1, "one alt, not two: {out}");
            assert!(out.contains("alt=\"new\""), "{out}");
            assert_eq!(attr_value(&out, "alt").as_deref(), Some("new"), "{out}");
            assert_eq!(attr_value(&out, "src").as_deref(), Some("assets/a.png"), "{out}");
        }
    }

    #[test]
    fn a_greater_than_inside_a_quoted_value_does_not_end_the_tag() {
        let body = "<img src=\"assets/a.png\" alt=\"a > b\"><p>after</p>";
        assert_eq!(alt_texts(body, notecraft_model::PageFormat::Html), vec![("a.png".to_string(), "a > b".to_string())]);
        let (out, found) = set_html_alt(body, "assets/a.png", "c");
        assert!(found);
        assert!(out.ends_with("<p>after</p>") && out.contains("alt=\"c\""), "{out}");
    }

    #[test]
    fn an_html_alt_round_trips_with_quotes_and_ampersands() {
        let (out, _) = set_html_alt("<img src=\"assets/a.png\">", "assets/a.png", "Tom & \"Jerry\"");
        assert_eq!(alt_texts(&out, notecraft_model::PageFormat::Html), vec![("a.png".to_string(), "Tom & \"Jerry\"".to_string())], "{out}");
    }

    #[test]
    fn an_image_inside_a_link_label_is_not_the_pictures_alt_text() {
        let body = "[![old.pdf](assets/old.pdf.png) old.pdf](assets/old.pdf)";
        assert!(alt_texts(body, notecraft_model::PageFormat::Cmgfm).is_empty(), "a card icon is not alt text");
    }

    #[test]
    fn alt_text_goes_into_the_markdown_image_and_brackets_are_escaped() {
        let t = TempDir::new("assets-alt-md");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "P", NodeKind::Page, Some(notecraft_model::PageFormat::Cmgfm), &s).expect("page");
        attach_bytes(&page, "a.png", b"x").expect("attach");
        crate::page::save_body(&page, "Intro\n\n![old](assets/a.png)\n\nText").expect("save");
        set_alt_text(&page, "a.png", "a [draft] chart").expect("alt");
        let body = crate::page::load_page(&page).expect("load").body;
        assert!(body.contains("![a \\[draft\\] chart](assets/a.png)"), "{body}");
        assert_eq!(alt_texts(&body, notecraft_model::PageFormat::Cmgfm), vec![("a.png".to_string(), "a [draft] chart".to_string())]);
    }

    #[test]
    fn alt_text_needs_the_page_to_show_the_picture() {
        let t = TempDir::new("assets-alt-unshown");
        let page = page(&t);
        attach_bytes(&page, "a.png", b"x").expect("attach");
        crate::page::save_body(&page, "<p>No picture here</p>").expect("save");
        assert!(set_alt_text(&page, "a.png", "x").is_err(), "the page does not show it");
        assert!(set_alt_text(&page, "../a.png", "x").is_err(), "a name, not a path");
    }

    #[test]
    fn a_file_can_be_taken_out_of_a_page_again() {
        let t = TempDir::new("assets-remove");
        let page = page(&t);
        attach_bytes(&page, "note.txt", b"words").expect("attach");
        assert_eq!(list(&page).len(), 1);

        remove(&page, "note.txt").expect("remove");
        assert!(list(&page).is_empty(), "the file is gone");
        assert!(remove(&page, "note.txt").is_err(), "and asking twice says so rather than pretending");

        // Nothing outside the page's own assets folder can be reached.
        attach_bytes(&page, "keep.txt", b"words").expect("attach");
        for name in ["../index.html", r"..\index.html", "a/b.txt", "", ".."] {
            assert!(remove(&page, name).is_err(), "refused: {name:?}");
        }
        assert_eq!(list(&page).len(), 1, "and nothing was deleted trying");
    }

    /// Issues #130 and #131: a long name keeps its file type, and every copy made of it, however
    /// many, has a name that can be taken out again.
    #[test]
    fn long_names_keep_their_type_and_every_copy_can_be_removed() {
        let t = TempDir::new("assets-long-names");
        let p = page(&t);
        for extension in ["pdf", "png", "docx"] {
            let link = attach_bytes(&p, &format!("{}.{extension}", "x".repeat(90)), b"owned file").expect("attach");
            assert!(link.ends_with(&format!(".{extension}")), "{link}");
        }
        assert_eq!(media_kind(&attach_bytes(&p, &format!("{}.png", "y".repeat(90)), b"png").expect("attach")), MediaKind::Image);

        let name = format!("{}.txt", "z".repeat(76));
        let copies: Vec<String> = (0..12)
            .map(|i| attach_bytes(&p, &name, format!("copy {i}").as_bytes()).expect("attach").trim_start_matches("assets/").to_string())
            .collect();
        assert_eq!(copies.iter().collect::<HashSet<_>>().len(), 12, "every copy has a name of its own");
        for copy in &copies {
            assert!(copy.ends_with(".txt") && copy.chars().count() <= names::MAX_FILE_NAME, "{copy}");
            remove(&p, copy).unwrap_or_else(|e| panic!("{copy} can be removed: {e}"));
        }
    }

    /// Issue #103: a picture the user attached is not taken for the icon of the attachment its
    /// name extends. It is listed, and removing that attachment leaves it alone.
    #[test]
    fn a_picture_named_like_an_icon_is_the_users_own() {
        let t = TempDir::new("assets-icon-name");
        let p = page(&t);
        attach_bytes(&p, "report.pdf", b"user PDF").expect("the PDF");
        attach_bytes(&p, "report.pdf.png", b"independent user image").expect("the picture");
        attach_bytes(&p, "report.pdf.svg", b"<svg>user drawing</svg>").expect("the drawing");
        let names: Vec<String> = list(&p).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["report.pdf", "report.pdf.png", "report.pdf.svg"], "all three are the user's files");

        // Making an icon for the PDF does not write over either of them.
        ensure_icon(&p, "report.pdf").expect("icon");
        assert_eq!(fs::read(p.join("assets/report.pdf.png")).expect("png"), b"independent user image");
        assert_eq!(fs::read(p.join("assets/report.pdf.svg")).expect("svg"), b"<svg>user drawing</svg>");

        remove(&p, "report.pdf").expect("remove the PDF");
        assert_eq!(fs::read(p.join("assets/report.pdf.png")).expect("the picture is still there"), b"independent user image");
        assert_eq!(fs::read(p.join("assets/report.pdf.svg")).expect("the drawing is still there"), b"<svg>user drawing</svg>");
        let names: Vec<String> = list(&p).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["report.pdf.png", "report.pdf.svg"]);
    }

    /// The icon made for an attachment is kept out of the list and goes with the attachment.
    #[test]
    fn a_generated_icon_is_hidden_and_removed_with_its_attachment() {
        let t = TempDir::new("assets-icon-owned");
        let p = page(&t);
        let src = t.path().join("brief.docx");
        fs::write(&src, b"document").expect("source");
        attach_file(&p, &src).expect("attach");
        let icon = ["brief.docx.png", "brief.docx.svg"].into_iter().find(|i| p.join("assets").join(i).is_file()).expect("an icon was made");
        assert!(recorded_icons(&p).contains(icon), "and recorded as NoteCraft's");
        assert_eq!(list(&p), vec![("brief.docx".to_string(), 8)], "the icon is not one of the user's files");

        remove(&p, "brief.docx").expect("remove");
        assert!(!p.join("assets").join(icon).exists(), "the icon went with it");
        assert!(recorded_icons(&p).is_empty(), "and so did its record");
    }

    /// An icon made before icons were recorded is still known by how the page shows it: as the
    /// picture at the start of an attachment card, in either format.
    #[test]
    fn an_icon_made_before_icons_were_recorded_is_known_by_its_card() {
        let t = TempDir::new("assets-icon-legacy");
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        for (format, card) in [
            (
                notecraft_model::PageFormat::Html,
                "<p><a href=\"assets/old.pdf\"><img src=\"assets/old.pdf.png\" alt=\"\" width=\"48\"><br>old.pdf</a></p>",
            ),
            (notecraft_model::PageFormat::Cmgfm, "[![old.pdf](assets/old.pdf.png) old.pdf](assets/old.pdf)\n\n![old.pdf.svg](assets/old.pdf.svg)\n"),
        ] {
            let p = ops::create_node(&nb, &format!("P {}", format.id()), NodeKind::Page, Some(format), &s).expect("page");
            crate::page::save_body(&p, card).expect("the card");
            fs::create_dir_all(p.join("assets")).expect("assets");
            fs::write(p.join("assets/old.pdf"), b"pdf").expect("pdf");
            fs::write(p.join("assets/old.pdf.png"), b"old icon").expect("old icon");
            fs::write(p.join("assets/old.pdf.svg"), b"<svg/>").expect("a picture shown on its own");
            let names: Vec<String> = list(&p).into_iter().map(|(n, _)| n).collect();
            assert_eq!(names, vec!["old.pdf", "old.pdf.svg"], "{format:?}: the card's icon is hidden, a picture shown on its own is not");
            remove(&p, "old.pdf").expect("remove");
            assert!(!p.join("assets/old.pdf.png").exists(), "{format:?}: the card's icon went with it");
            assert!(p.join("assets/old.pdf.svg").is_file(), "{format:?}: the other picture stayed");
        }
    }

    #[test]
    fn attaches_bytes_and_makes_names_unique() {
        let t = TempDir::new("assets1");
        let p = page(&t);
        assert_eq!(attach_bytes(&p, "pic.png", b"1").expect("a"), "assets/pic.png");
        assert_eq!(attach_bytes(&p, "pic.png", b"2").expect("b"), "assets/pic-2.png");
        assert_eq!(attach_bytes(&p, "PIC.png", b"3").expect("c"), "assets/PIC-3.png", "case-insensitive");
        assert_eq!(list(&p).len(), 3);
    }

    #[test]
    fn hostile_names_are_made_safe() {
        let t = TempDir::new("assets2");
        let p = page(&t);
        assert_eq!(attach_bytes(&p, "../../evil name.txt", b"x").expect("a"), "assets/evil-name.txt");
        assert_eq!(attach_bytes(&p, "CON", b"x").expect("b"), "assets/file-CON");
        assert!(p.join("assets").join("evil-name.txt").is_file());
        assert!(!t.path().join("evil name.txt").exists());
    }

    #[test]
    fn copies_a_file() {
        let t = TempDir::new("assets3");
        let p = page(&t);
        let src = t.path().join("source clip.mp4");
        fs::write(&src, b"video").expect("src");
        let link = attach_file(&p, &src).expect("attach");
        assert_eq!(link, "assets/source-clip.mp4");
        assert_eq!(fs::read(p.join(&link)).expect("read"), b"video");
        assert!(
            p.join("assets/source-clip.mp4.png").is_file() || p.join("assets/source-clip.mp4.svg").is_file(),
            "attachment gets a secondary icon artifact"
        );
        assert!(src.exists(), "the original stays");
        assert!(attach_file(&p, t.path()).is_err(), "a folder cannot be attached");
        assert!(attach_file(&p, &t.path().join("missing")).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_shell_icons_are_pngs() {
        let t = TempDir::new("assets-shell-icon");
        let p = page(&t);
        let attachment = p.join("assets").join("shell-icon-check.docx");
        fs::write(&attachment, b"test document").expect("attachment");

        ensure_icon(&p, "shell-icon-check.docx").expect("icon");

        let png = fs::read(p.join("assets").join("shell-icon-check.docx.png")).expect("shell PNG");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "the Shell result is a PNG sidecar");
        assert!(!p.join("assets").join("shell-icon-check.docx.svg").exists(), "a native Shell icon does not need the SVG fallback");
    }

    /// The same page reached as a verbatim network share (`\\?\UNC\...`), the way a notebook
    /// on a shared folder is opened. Stripping only `\\?\` once sent the Shell `UNC\...`.
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_shell_icons_work_on_a_network_share() {
        let t = TempDir::new("assets-shell-icon-unc");
        let p = page(&t);
        fs::write(p.join("assets").join("share-check.docx"), b"test document").expect("attachment");
        let local = crate::tree::display_path(&fs::canonicalize(&p).expect("canonical"));
        let (drive, rest) = local.split_once(":\\").expect("a drive path");
        let shared = std::path::PathBuf::from(format!(r"\\?\UNC\localhost\{drive}$\{rest}"));
        assert!(shared.join("assets").join("share-check.docx").is_file(), "the admin share reaches the page: {shared:?}");

        ensure_icon(&shared, "share-check.docx").expect("icon");

        assert!(p.join("assets").join("share-check.docx.png").is_file(), "the Shell icon is made through the share");
        assert!(!p.join("assets").join("share-check.docx.svg").exists(), "no SVG fallback on a share");
    }

    #[test]
    fn only_pages_take_attachments() {
        let t = TempDir::new("assets4");
        assert!(attach_bytes(t.path(), "a.txt", b"x").is_err());
    }

    #[test]
    fn media_kinds() {
        assert_eq!(media_kind("a.PNG"), MediaKind::Image);
        // Issue #29: a JPEG XL picture is a picture, not a file card.
        assert_eq!(media_kind("photo.jxl"), MediaKind::Image);
        assert_eq!(media_kind("photo.JXL"), MediaKind::Image);
        assert_eq!(media_kind("a.mp4"), MediaKind::Video);
        assert_eq!(media_kind("a.mp3"), MediaKind::Audio);
        assert_eq!(media_kind("a.pdf"), MediaKind::Other);
        assert_eq!(media_kind("noext"), MediaKind::Other);
    }
}

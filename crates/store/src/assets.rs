//! Attachments: images, video, audio and other files stored beside a page, in its
//! `assets` folder, and linked from the page by a relative path (`assets/name.png`).

use crate::error::{Result, StoreError};
use crate::tree::page_file;
use notecraft_model::names;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

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
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" => MediaKind::Image,
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

fn unique_name(dir: &Path, name: &str) -> String {
    let taken: HashSet<String> = match fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_lowercase()).collect(),
        Err(_) => HashSet::new(),
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (name.to_string(), String::new()),
    };
    let mut candidate = name.to_string();
    let mut n = 2;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = format!("{stem}-{n}{ext}");
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
pub fn remove(page_dir: &Path, name: &str) -> Result<()> {
    let safe = names::safe_file_name(name);
    if name.is_empty() || safe != name {
        return Err(StoreError::invalid("that is not a file in this page's folder"));
    }
    let file = page_dir.join("assets").join(name);
    if !file.is_file() {
        return Err(StoreError::NotFound(format!("no attachment called {name}")));
    }
    fs::remove_file(&file).map_err(|e| StoreError::io("remove", &file, e))
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
    Ok(format!("assets/{name}"))
}

/// The attachments of a page: file name and size, sorted by name.
pub fn list(page_dir: &Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = match fs::read_dir(page_dir.join("assets")) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.metadata().map(|m| m.len()).unwrap_or(0)))
            .collect(),
        Err(_) => Vec::new(),
    };
    out.sort();
    out
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
        assert!(src.exists(), "the original stays");
        assert!(attach_file(&p, t.path()).is_err(), "a folder cannot be attached");
        assert!(attach_file(&p, &t.path().join("missing")).is_err());
    }

    #[test]
    fn only_pages_take_attachments() {
        let t = TempDir::new("assets4");
        assert!(attach_bytes(t.path(), "a.txt", b"x").is_err());
    }

    #[test]
    fn media_kinds() {
        assert_eq!(media_kind("a.PNG"), MediaKind::Image);
        assert_eq!(media_kind("a.mp4"), MediaKind::Video);
        assert_eq!(media_kind("a.mp3"), MediaKind::Audio);
        assert_eq!(media_kind("a.pdf"), MediaKind::Other);
        assert_eq!(media_kind("noext"), MediaKind::Other);
    }
}

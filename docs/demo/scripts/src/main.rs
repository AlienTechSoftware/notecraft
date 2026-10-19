#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! Builds the Welcome notebook: `docs/demo/DemoNotebook`, from `docs/demo/scripts/content`.
//!
//! The notebook is a tour of NoteCraft that ships with the app. It is built by code so that
//! thirteen languages stay in step: one section group per language, each with a "How to"
//! section holding the same pages in the same order, and a "Start here" section with one page
//! that links to every group. The content is Markdown and HTML files with a small front
//! matter; this program only arranges them, writes the `_node.toml` files, gives every node a
//! stable id and date, and checks the result with the store's own checker and link resolver.
//!
//! The output is deterministic: the same content gives the same bytes, so `cargo xtask
//! demo-check` can fail when the checked-in notebook is older than its content.
//!
//! ```text
//! cargo xtask demo          # build docs/demo/DemoNotebook
//! cargo xtask demo-check    # build to a scratch folder and compare, without writing
//! ```

use notecraft_model::{kv, names};
use notecraft_store::check;
use notecraft_store::links::{self, Link};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const NOTEBOOK_TITLE: &str = "Welcome to NoteCraft";
/// Every node carries this date, so a rebuild changes nothing but what the content changed.
const DATE: &str = "2026-10-18T00:00:00Z";
const START_FOLDER: &str = "00-start";
const START_TITLE: &str = "Start here";
const HOW_TO_FOLDER: &str = "how-to";
/// Put into the start page where the list of languages goes.
const LANGUAGES_MARK: &str = "{{languages}}";

/// One language of the tour.
struct Language {
    /// ISO 639-1 code, also the content folder and part of the group's folder name.
    code: &'static str,
    /// The language's own name for itself, shown as the group's title and as the link.
    name: &'static str,
    /// Its English name, shown beside the link for a reader who cannot read the script.
    english: &'static str,
    /// The title of the How to section in that language.
    how_to: &'static str,
    rtl: bool,
    /// The group's folder number: `10-en`, `20-ar`. Also decides its order in the tree.
    number: u32,
    color: &'static str,
}

/// The thirteen languages the owner chose, left-to-right ones first. The How to titles are
/// the plain word for a short guide in each language; a native reader may improve them.
const LANGUAGES: &[Language] = &[
    Language { code: "en", name: "English", english: "English", how_to: "How to", rtl: false, number: 10, color: "purple" },
    Language { code: "es", name: "Español", english: "Spanish", how_to: "Cómo hacer", rtl: false, number: 11, color: "blue" },
    Language { code: "fr", name: "Français", english: "French", how_to: "Comment faire", rtl: false, number: 12, color: "teal" },
    Language { code: "de", name: "Deutsch", english: "German", how_to: "Anleitung", rtl: false, number: 13, color: "green" },
    Language {
        code: "hi", name: "हिन्दी", english: "Hindi", how_to: "कैसे करें", rtl: false, number: 14, color: "yellow"
    },
    Language { code: "ru", name: "Русский", english: "Russian", how_to: "Как сделать", rtl: false, number: 15, color: "orange" },
    Language { code: "zh", name: "中文（简体）", english: "Chinese, Simplified", how_to: "使用指南", rtl: false, number: 16, color: "red" },
    Language { code: "ja", name: "日本語", english: "Japanese", how_to: "使い方", rtl: false, number: 17, color: "magenta" },
    Language { code: "ko", name: "한국어", english: "Korean", how_to: "사용 방법", rtl: false, number: 18, color: "gray" },
    Language { code: "ar", name: "العربية", english: "Arabic", how_to: "كيف", rtl: true, number: 20, color: "purple" },
    Language { code: "fa", name: "فارسی", english: "Persian", how_to: "راهنما", rtl: true, number: 21, color: "blue" },
    Language { code: "ur", name: "اردو", english: "Urdu", how_to: "طریقہ", rtl: true, number: 22, color: "teal" },
    Language { code: "he", name: "עברית", english: "Hebrew", how_to: "איך", rtl: true, number: 23, color: "green" },
];

impl Language {
    fn folder(&self) -> String {
        format!("{}-{}", self.number, self.code)
    }
}

/// One page as read from the content folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Source {
    /// From the file name: `20-write-a-page.md` is order 20, folder `write-a-page`.
    order: u32,
    slug: String,
    title: String,
    html: bool,
    /// `rtl` or `ltr` when the front matter says; otherwise the language decides.
    dir: Option<String>,
    body: String,
}

/// What a build made, for the report and the tests.
#[derive(Debug, Default)]
struct Built {
    languages: Vec<String>,
    pages: usize,
}

fn main() -> ExitCode {
    let task = std::env::args().nth(1).unwrap_or_else(|| "build".to_string());
    let here = std::env::var("CARGO_MANIFEST_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("."));
    let content = here.join("content");
    // The pictures a demo page shows live with the repository's other art, attributed
    // there: `docs/demo/scripts` is three levels down from the root.
    let art = here.parent().and_then(|p| p.parent()).and_then(|p| p.parent()).map(|r| r.join("assets").join("demo")).unwrap_or_default();
    let out = here.parent().map(|p| p.join("DemoNotebook")).unwrap_or_else(|| PathBuf::from("DemoNotebook"));
    let result = match task.as_str() {
        "build" => build_in_place(&content, &out, &art),
        "check" => check_in_place(&content, &out, &art),
        other => Err(format!("unknown task '{other}'. Tasks: build check")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Builds into `out`, replacing what is there. Only a folder that holds this notebook (or
/// nothing) is replaced, so a wrong path cannot delete someone's notes.
fn build_in_place(content: &Path, out: &Path, art: &Path) -> Result<(), String> {
    if out.exists() {
        let node = notecraft_store::tree::read_node(out);
        let ours = node.as_ref().is_some_and(|n| n.title == NOTEBOOK_TITLE);
        let empty = fs::read_dir(out).map(|mut rd| rd.next().is_none()).unwrap_or(false);
        if !ours && !empty {
            return Err(format!("{} exists and is not the Welcome notebook; not touching it", out.display()));
        }
        fs::remove_dir_all(out).map_err(|e| format!("cannot clear {}: {e}", out.display()))?;
    }
    let built = build(content, out, art)?;
    verify(out)?;
    println!("demo: built {} with {} languages ({}) and {} pages", out.display(), built.languages.len(), built.languages.join(", "), built.pages);
    Ok(())
}

/// Builds into a scratch folder and compares it with `out`, file by file.
fn check_in_place(content: &Path, out: &Path, art: &Path) -> Result<(), String> {
    let scratch = std::env::temp_dir().join(format!("notecraft-demo-check-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    let fresh = scratch.join("DemoNotebook");
    let result = build(content, &fresh, art).and_then(|_| verify(&fresh)).and_then(|()| {
        let want = snapshot(&fresh)?;
        let have = snapshot(out)?;
        let mut differences: Vec<String> = Vec::new();
        for (path, bytes) in &want {
            match have.get(path) {
                None => differences.push(format!("missing: {path}")),
                Some(b) if b != bytes => differences.push(format!("differs: {path}")),
                Some(_) => {}
            }
        }
        for path in have.keys() {
            if !want.contains_key(path) {
                differences.push(format!("extra: {path}"));
            }
        }
        if differences.is_empty() {
            println!("demo-check: {} files, {} up to date with its content", want.len(), out.display());
            Ok(())
        } else {
            Err(format!("{} is out of date; run `cargo xtask demo`:\n  {}", out.display(), differences.join("\n  ")))
        }
    });
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Writes the notebook under `out`, which must not exist yet.
fn build(content: &Path, out: &Path, art: &Path) -> Result<Built, String> {
    let mut built = Built::default();
    fs::create_dir_all(out).map_err(|e| format!("cannot create {}: {e}", out.display()))?;
    write(&out.join("_node.toml"), &node_toml("notebook", NOTEBOOK_TITLE, &id_for("notebook"), 10, Some("cmgfm"), Some("purple")))?;

    // The languages with content, in table order.
    let present: Vec<&Language> = LANGUAGES.iter().filter(|l| content.join(l.code).is_dir()).collect();
    if present.is_empty() {
        return Err(format!("no language folder under {}", content.display()));
    }

    // Start here: one page, with the list of languages where the mark is.
    let start = out.join(START_FOLDER);
    fs::create_dir_all(&start).map_err(|e| format!("cannot create {}: {e}", start.display()))?;
    write(&start.join("_node.toml"), &node_toml("section", START_TITLE, &id_for(START_FOLDER), 10, None, None))?;
    let start_page = read_source(&content.join("start").join("welcome.md"), 10)?;
    let list = language_list(&present);
    let body = start_page.body.replace(LANGUAGES_MARK, &list);
    write_page(&start.join("welcome"), &Source { body, ..start_page }, &id_for("start/welcome"), None, art)?;
    built.pages += 1;

    for (n, lang) in present.iter().enumerate() {
        let group = out.join(lang.folder());
        fs::create_dir_all(&group).map_err(|e| format!("cannot create {}: {e}", group.display()))?;
        let order = 20 + 10 * n as u32;
        write(&group.join("_node.toml"), &node_toml("section-group", lang.name, &id_for(&lang.folder()), order, None, Some(lang.color)))?;
        let section = group.join(HOW_TO_FOLDER);
        fs::create_dir_all(&section).map_err(|e| format!("cannot create {}: {e}", section.display()))?;
        write(&section.join("_node.toml"), &node_toml("section", lang.how_to, &id_for(&format!("{}/how-to", lang.folder())), 10, None, None))?;
        let sources = read_sources(&content.join(lang.code))?;
        if sources.is_empty() {
            return Err(format!("{}: no pages", content.join(lang.code).display()));
        }
        let default_dir = if lang.rtl { Some("rtl") } else { None };
        for s in &sources {
            let id = id_for(&format!("{}/how-to/{}", lang.folder(), s.slug));
            let page = section.join(&s.slug);
            // A page with ink names it, as every page with ink must: the folder and the
            // page's own words have to agree. See docs/page-model.md section 5.
            let inked = s.slug == "pictures-and-files";
            let s = &if inked {
                Source {
                    body: format!(
                        "{}

![Ink](ink.svg)
",
                        s.body.trim_end()
                    ),
                    ..s.clone()
                }
            } else {
                s.clone()
            };
            write_page(&page, s, &id, default_dir, art)?;
            // The page about pictures and files is the one that shows handwriting, so a
            // reader meets the pen on the page that talks about what a page can hold.
            if s.slug == "pictures-and-files" {
                write_ink(&page)?;
            }
            built.pages += 1;
        }
        built.languages.push(lang.code.to_string());
    }
    Ok(built)
}

/// The bullet list on the start page: each language's own name as the link, its English
/// name after it, and a note on the ones that read right to left.
fn language_list(present: &[&Language]) -> String {
    let mut out = String::new();
    for l in present {
        let note = if l.rtl { ", right to left" } else { "" };
        // The English name only when it adds something: "English, English" would not.
        let english = if l.english == l.name { String::new() } else { format!(", {}", l.english) };
        out.push_str(&format!("- [{}](../../{}/){english}{note}\n", l.name, l.folder()));
    }
    out.trim_end().to_string()
}

/// Writes one page folder: `index.md` or `index.html` with the page's metadata, and
/// every picture the page names, copied in from the repository's own `assets/demo`.
fn write_page(dir: &Path, s: &Source, id: &str, default_dir: Option<&str>, art: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let dir_value = s.dir.as_deref().or(default_dir);
    copy_assets(dir, &s.body, art)?;
    if s.html {
        write(&dir.join("index.html"), &html_page(&s.title, id, s.order, dir_value, &s.body))
    } else {
        write(&dir.join("index.md"), &markdown_page(&s.title, id, s.order, dir_value, &s.body))
    }
}

/// Copies every `assets/...` a page names into that page's own `assets` folder.
///
/// A page owns its files, so each page that shows a picture gets its own copy rather than
/// linking across the notebook. The pictures themselves live in the repository under
/// `assets/demo`, attributed there, and this is the only place they are copied from: a
/// page that names one that is not there fails the build rather than shipping a hole.
fn copy_assets(dir: &Path, body: &str, art: &Path) -> Result<(), String> {
    for target in link_targets(body) {
        let Some(name) = target.strip_prefix("assets/") else { continue };
        if name.is_empty() || name.contains('/') || name.contains("..") {
            return Err(format!("{}: {target} is not a plain file name", dir.display()));
        }
        let from = art.join(name);
        if !from.is_file() {
            return Err(format!("{}: nothing at {}", dir.display(), from.display()));
        }
        let assets = dir.join("assets");
        fs::create_dir_all(&assets).map_err(|e| format!("cannot create {}: {e}", assets.display()))?;
        fs::copy(&from, assets.join(name)).map_err(|e| format!("cannot copy {}: {e}", from.display(),))?;
    }
    Ok(())
}

/// A drawing for a page: a few strokes written as NoteCraft's own ink file.
///
/// The demo should show what the pen leaves behind, and a page with ink in the folder is
/// the only honest way to show it. The strokes are drawn here with the same crate the pen
/// uses, so the file the demo ships is the file the pen would have written.
fn write_ink(dir: &Path) -> Result<(), String> {
    use notecraft_ink::{Dab, Drawing, Rgb, Stroke, Tool};
    let mut drawing = Drawing::new(820.0, 1100.0);

    // A hand-drawn underline under the page's first heading, and a tick beside it.
    let mut line = Stroke::new(Tool::Highlighter, Rgb(0xFF, 0xDD, 0x33), 14.0);
    for i in 0..=18 {
        let t = i as f32 / 18.0;
        line.add(Dab { x: 60.0 + t * 300.0, y: 96.0 + (t * 6.2).sin() * 2.5, pressure: 0.55 + t * 0.2 });
    }
    drawing.add(line);

    // Two strokes, not more: a stroke is stored as a filled outline, and an outline is
    // some 30 KB a stroke. Thirteen languages carry a copy of this page.
    let mut tick = Stroke::new(Tool::Pen, Rgb(0x1E, 0x8A, 0x44), 3.5);
    for i in 0..=6 {
        let t = i as f32 / 6.0;
        tick.add(Dab { x: 400.0 + t * 14.0, y: 84.0 + t * 16.0, pressure: 0.4 + t * 0.4 });
    }
    for i in 1..=9 {
        let t = i as f32 / 9.0;
        tick.add(Dab { x: 414.0 + t * 26.0, y: 100.0 - t * 30.0, pressure: 0.8 - t * 0.3 });
    }
    drawing.add(tick);

    let svg = notecraft_ink::svg::write(&drawing, notecraft_ink::svg::Flavour::Editable);
    write(&dir.join("ink.svg"), &svg)
}

/// A `_node.toml` in the form `docs/folder-format.md` section 4 describes.
fn node_toml(kind: &str, title: &str, id: &str, order: u32, format: Option<&str>, color: Option<&str>) -> String {
    let mut s = format!("kind = {}\ntitle = {}\nid = {}\n", kv::quote(kind), kv::quote(title), kv::quote(id));
    if let Some(f) = format {
        s.push_str(&format!("format = {}\n", kv::quote(f)));
    }
    s.push_str(&format!("order = {order}\n"));
    if let Some(c) = color {
        s.push_str(&format!("color = {}\n", kv::quote(c)));
    }
    s.push_str(&format!("created = {}\n", kv::quote(DATE)));
    s
}

/// A Markdown page with the front matter the store writes (section 3.2 of the format).
fn markdown_page(title: &str, id: &str, order: u32, dir: Option<&str>, body: &str) -> String {
    let mut front = format!("---\nid: {id}\ntitle: {}\norder: {order}\ncreated: {DATE}\nupdated: {DATE}\n", kv::quote(title));
    if let Some(d) = dir {
        front.push_str(&format!("dir: {d}\n"));
    }
    front.push_str("tags: []\n---\n\n");
    format!("{front}{}\n", body.trim_end())
}

/// An HTML page with the metadata in `<meta>` tags (section 3.1 of the format).
fn html_page(title: &str, id: &str, order: u32, dir: Option<&str>, body: &str) -> String {
    let t = escape(title);
    let mut head = format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<title>{t}</title>\n<meta name=\"notecraft:id\" content=\"{id}\">\n<meta name=\"notecraft:order\" content=\"{order}\">\n<meta name=\"notecraft:created\" content=\"{DATE}\">\n<meta name=\"notecraft:updated\" content=\"{DATE}\">\n"
    );
    if let Some(d) = dir {
        head.push_str(&format!("<meta name=\"notecraft:dir\" content=\"{d}\">\n"));
    }
    format!("{head}</head>\n<body>\n{}\n</body>\n</html>\n", body.trim())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Every `NN-slug.md` or `NN-slug.html` in `dir`, in order.
fn read_sources(dir: &Path) -> Result<Vec<Source>, String> {
    let mut names: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "md" || x == "html"))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for p in names {
        let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (number, slug) = stem.split_once('-').ok_or_else(|| format!("{}: name it NN-slug.md", p.display()))?;
        let order: u32 = number.parse().map_err(|_| format!("{}: '{number}' is not a number", p.display()))?;
        if let Err(why) = names::validate_component(slug) {
            return Err(format!("{}: folder name '{slug}': {why}", p.display()));
        }
        let mut s = read_source(&p, order)?;
        s.slug = slug.to_string();
        out.push(s);
    }
    Ok(out)
}

/// One content file: a `title:` (and optional `dir:`) front matter, then the body.
fn read_source(path: &Path, order: u32) -> Result<Source, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let text = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let (front, body) = split_front(&text).ok_or_else(|| format!("{}: no front matter (a --- block with a title)", path.display()))?;
    let title = front_value(front, "title").ok_or_else(|| format!("{}: front matter has no title", path.display()))?;
    let dir = front_value(front, "dir");
    if let Some(d) = &dir
        && d != "rtl"
        && d != "ltr"
    {
        return Err(format!("{}: dir must be rtl or ltr, not '{d}'", path.display()));
    }
    let html = path.extension().is_some_and(|x| x == "html");
    let slug = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Source { order, slug, title, html, dir, body: body.trim_start_matches('\n').to_string() })
}

/// The lines between the first two `---` lines, and the rest.
fn split_front(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---\n")?;
    Some((&rest[..end], &rest[end + 5..]))
}

fn front_value(front: &str, key: &str) -> Option<String> {
    front.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim() == key).map(|(_, v)| v.trim().trim_matches('"').to_string()))
}

/// A stable 20-hex-character id from a name: the shape the store uses, without the clock.
/// Two FNV-1a hashes over the name and its reverse, so two names never share an id by a
/// one-character slip.
fn id_for(name: &str) -> String {
    fn fnv(bytes: impl Iterator<Item = u8>) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
        h
    }
    let a = fnv(name.bytes());
    let b = fnv(name.bytes().rev());
    format!("{a:016x}{:04x}", (b & 0xffff) as u16)
}

/// The checker finds no problem, and every link in every page leads somewhere.
fn verify(out: &Path) -> Result<(), String> {
    let report = check::check_tree(out);
    if !report.problems.is_empty() {
        let list: Vec<String> = report.problems.iter().map(|p| format!("{}: {}", p.path.display(), p.message)).collect();
        return Err(format!("the checker found problems:\n  {}", list.join("\n  ")));
    }
    let root = out.parent().ok_or("the notebook has no parent folder")?;
    let mut broken = Vec::new();
    let mut checked = 0usize;
    for (page, text) in pages_under(out)? {
        for href in link_targets(&text) {
            checked += 1;
            match links::resolve(root, &page, &href) {
                Link::Node(_) | Link::File(_) | Link::Web(_) => {}
                Link::Missing(p) => broken.push(format!("{} -> {href} (nothing at {})", page.display(), p.display())),
                Link::Outside => broken.push(format!("{} -> {href} (outside the notebook)", page.display())),
            }
        }
    }
    if broken.is_empty() {
        println!("demo: {} nodes, {checked} links, all resolve", report.nodes);
        Ok(())
    } else {
        Err(format!("broken links:\n  {}", broken.join("\n  ")))
    }
}

/// Every page folder under `dir` with the text of its file.
fn pages_under(dir: &Path) -> Result<Vec<(PathBuf, String)>, String> {
    let mut out = Vec::new();
    for child in notecraft_store::tree::child_dirs(dir) {
        if let Some((_, file)) = notecraft_store::tree::page_file(&child) {
            let text = fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            out.push((child.clone(), text));
        }
        out.extend(pages_under(&child)?);
    }
    Ok(out)
}

/// Link targets in a page: `](target)` in Markdown, `href="..."` and `src="..."` in HTML.
fn link_targets(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(i) = rest.find("](") {
        let after = &rest[i + 2..];
        let Some(end) = after.find(')') else { break };
        let target = after[..end].split_whitespace().next().unwrap_or("");
        if !target.is_empty() && !target.starts_with('#') {
            out.push(target.to_string());
        }
        rest = &after[end..];
    }
    for attr in ["href=\"", "src=\""] {
        let mut rest = text;
        while let Some(i) = rest.find(attr) {
            let after = &rest[i + attr.len()..];
            let Some(end) = after.find('"') else { break };
            let target = after[..end].trim();
            if !target.is_empty() && !target.starts_with('#') {
                out.push(target.to_string());
            }
            rest = &after[end..];
        }
    }
    out
}

/// Every file under `dir` by its relative path, with its bytes.
fn snapshot(dir: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) -> Result<(), String> {
        let rd = fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out)?;
            } else {
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.insert(rel, fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    if dir.exists() {
        walk(dir, dir, &mut out)?;
    }
    Ok(out)
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    fs::write(path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repository's own art, which a page's pictures are copied from.
    fn art_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("..").join("assets").join("demo")
    }

    fn content_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("content")
    }

    fn scratch(label: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("notecraft-demo-test-{label}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        p
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(id_for("notebook"), id_for("notebook"));
        assert_ne!(id_for("10-en/how-to/welcome"), id_for("11-es/how-to/welcome"));
        assert_ne!(id_for("ab"), id_for("ba"), "the reverse hash tells anagrams apart");
        assert_eq!(id_for("x").len(), 20);
        assert!(id_for("x").chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn the_language_table_is_sound() {
        let mut codes: Vec<&str> = LANGUAGES.iter().map(|l| l.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LANGUAGES.len(), "no code twice");
        let mut numbers: Vec<u32> = LANGUAGES.iter().map(|l| l.number).collect();
        numbers.sort_unstable();
        numbers.dedup();
        assert_eq!(numbers.len(), LANGUAGES.len(), "no folder number twice");
        assert_eq!(LANGUAGES.iter().filter(|l| l.rtl).count(), 4, "Arabic, Persian, Urdu, Hebrew");
        for l in LANGUAGES {
            assert!(names::validate_component(&l.folder()).is_ok(), "{}", l.folder());
            assert!(["purple", "blue", "teal", "green", "yellow", "orange", "red", "magenta", "gray"].contains(&l.color), "{}: {}", l.code, l.color);
        }
    }

    #[test]
    fn content_files_parse_and_name_their_folders() {
        let en = read_sources(&content_dir().join("en")).expect("English content");
        assert!(en.len() >= 8, "eight pages at least, got {}", en.len());
        assert_eq!(en[0].slug, "welcome");
        assert_eq!(en[0].order, 10);
        assert!(en.iter().any(|s| s.html), "one page shows the HTML format");
        let orders: Vec<u32> = en.iter().map(|s| s.order).collect();
        let mut sorted = orders.clone();
        sorted.sort_unstable();
        assert_eq!(orders, sorted, "files are read in order");
        assert!(read_source(&content_dir().join("start").join("welcome.md"), 10).expect("start page").body.contains(LANGUAGES_MARK));
    }

    #[test]
    fn the_page_files_have_the_metadata_the_store_reads() {
        let md = markdown_page("A \"quoted\" title", "abc", 20, Some("rtl"), "# Body\n");
        assert!(md.starts_with("---\nid: abc\ntitle: \"A \\\"quoted\\\" title\"\norder: 20\n"));
        assert!(md.contains("\ndir: rtl\n"));
        assert!(md.ends_with("---\n\n# Body\n"));
        let html = html_page("A <b>", "abc", 10, None, "<p>x</p>");
        assert!(html.contains("<title>A &lt;b&gt;</title>"));
        assert!(html.contains("name=\"notecraft:id\" content=\"abc\""));
        assert!(!html.contains("notecraft:dir"));
        let node = node_toml("section-group", "Español", "id1", 30, None, Some("blue"));
        assert_eq!(
            node,
            "kind = \"section-group\"\ntitle = \"Español\"\nid = \"id1\"\norder = 30\ncolor = \"blue\"\ncreated = \"2026-10-18T00:00:00Z\"\n"
        );
    }

    #[test]
    fn link_targets_are_found_in_both_formats() {
        let md = "See [a](../a/) and [b](https://x.test \"t\") and [c](#top).";
        assert_eq!(link_targets(md), vec!["../a/", "https://x.test"]);
        let html = "<a href=\"../a/\">a</a> <img src=\"assets/p.png\"> <a href=\"#top\">t</a>";
        assert_eq!(link_targets(html), vec!["../a/", "assets/p.png"]);
    }

    /// The real content builds, the store's checker is happy, every link resolves, the tree
    /// has the shape the owner asked for, and a second build is byte-identical.
    #[test]
    fn the_notebook_builds_cleanly_and_the_same_way_twice() {
        let dir = scratch("build");
        let out = dir.join("DemoNotebook");
        let built = build(&content_dir(), &out, &art_dir()).expect("build");
        verify(&out).expect("verify");
        assert!(built.languages.contains(&"en".to_string()));
        let tree = notecraft_store::tree::load_tree(&out).expect("tree");
        assert_eq!(tree.info.title, NOTEBOOK_TITLE);
        let first = tree.children.first().expect("a first child");
        assert_eq!(first.info.title, START_TITLE, "the start section sorts first, so the notebook opens on it");
        assert_eq!(first.children.len(), 1);
        for group in tree.children.iter().skip(1) {
            assert_eq!(group.info.kind, notecraft_model::NodeKind::SectionGroup, "{}", group.path.display());
            assert_eq!(group.children.len(), 1, "one How to section in {}", group.path.display());
            assert!(group.children[0].children.len() >= 8, "the same pages in every language");
        }
        let start = fs::read_to_string(out.join(START_FOLDER).join("welcome").join("index.md")).expect("start page");
        assert!(start.contains("- [English](../../10-en/)\n"), "{start}");
        assert!(start.contains("- [العربية](../../20-ar/), Arabic, right to left"), "{start}");
        assert!(!start.contains(LANGUAGES_MARK));
        let first_build = snapshot(&out).expect("snapshot");
        let again = dir.join("Again");
        build(&content_dir(), &again, &art_dir()).expect("build again");
        assert_eq!(snapshot(&again).expect("snapshot"), first_build, "deterministic");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_folder_that_is_not_the_welcome_notebook_is_left_alone() {
        let dir = scratch("guard");
        let other = dir.join("Someone");
        fs::create_dir_all(&other).expect("dir");
        fs::write(other.join("_node.toml"), "kind = \"notebook\"\ntitle = \"Mine\"\nid = \"1\"\n").expect("node");
        let err = build_in_place(&content_dir(), &other, &art_dir()).expect_err("refused");
        assert!(err.contains("not touching it"), "{err}");
        assert!(other.join("_node.toml").is_file(), "nothing was deleted");
        let _ = fs::remove_dir_all(&dir);
    }
}

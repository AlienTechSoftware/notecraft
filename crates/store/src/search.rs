//! Plain text search across the pages of a notebooks folder.

use crate::page::load_page;
use crate::tree::{child_dirs, page_file, read_node};
use notecraft_formats::{boxes, html, render};
use notecraft_model::PageFormat;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    pub title: String,
    pub snippet: String,
}

fn collect_pages(dir: &Path, out: &mut Vec<PathBuf>) {
    if page_file(dir).is_some() {
        out.push(dir.to_path_buf());
    }
    for c in child_dirs(dir) {
        collect_pages(&c, out);
    }
}

/// Pages whose title or text contains `needle` (ignoring case). At most `limit` hits.
pub fn search(root: &Path, needle: &str, limit: usize) -> Vec<Hit> {
    search_in(root, &[], needle, limit)
}

/// [`search`] over the notebooks folder and the notebooks opened from elsewhere, each of which
/// is a notebook folder in its own right. Searched as one: a page is found once however it is
/// reached, and `limit` is for all of them together (issue #127).
pub fn search_in(root: &Path, elsewhere: &[PathBuf], needle: &str, limit: usize) -> Vec<Hit> {
    let needle = needle.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    if needle.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut pages = Vec::new();
    for nb in child_dirs(root).into_iter().chain(elsewhere.iter().cloned()) {
        collect_pages(&nb, &mut pages);
    }
    let mut seen = std::collections::HashSet::new();
    pages.retain(|page| seen.insert(std::fs::canonicalize(page).unwrap_or_else(|_| page.clone())));
    let mut hits = Vec::new();
    for dir in pages {
        let Ok(page) = load_page(&dir) else { continue };
        // The words a reader sees. Markdown is read into blocks and written as the HTML the
        // same page would be, so both formats are searched the same way: searched as source,
        // a phrase with a bold word in it, `Hello **world**`, was never found (issue #104).
        let plain = match page.format {
            PageFormat::Html => html::strip_tags(&page.body),
            PageFormat::Cmgfm => html::strip_tags(&render::blocks_to_html(&boxes::read(PageFormat::Cmgfm, &page.body).0)),
        };
        let plain = plain.split_whitespace().collect::<Vec<_>>().join(" ");
        let title = read_node(&dir).map(|i| i.title).unwrap_or_default();
        let hay: Vec<char> = format!("{title} {plain}").to_lowercase().chars().collect();
        let n: Vec<char> = needle.chars().collect();
        if let Some(pos) = (0..hay.len().saturating_sub(n.len().saturating_sub(1))).find(|&i| hay.get(i..i + n.len()) == Some(n.as_slice())) {
            let from = pos.saturating_sub(30);
            let snippet: String = hay.iter().skip(from).take(100).collect::<String>().replace('\n', " ");
            hits.push(Hit { path: dir, title, snippet: snippet.split_whitespace().collect::<Vec<_>>().join(" ") });
            if hits.len() >= limit {
                break;
            }
        }
    }
    hits
}

/// Where a ranked text came from inside a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// The page's own text (and title).
    Page,
    /// Alt text, read from the `alt` of the picture in the page markup.
    Alt,
    /// Text read from a picture by OCR (`<picture>.ocr.json`).
    Picture,
}

/// A box in picture pixels: four corners, top left first, clockwise.
pub type Quad = [[f32; 2]; 4];

/// One page found by the ranked search, with the best of its texts. For a hit on a picture's
/// text, `picture` is the picture and `boxes` are the boxes of the lines that match the query.
#[derive(Clone, Debug, PartialEq)]
pub struct Ranked {
    pub path: PathBuf,
    pub title: String,
    pub source: Source,
    pub snippet: String,
    pub score: f64,
    pub picture: Option<PathBuf>,
    pub boxes: Vec<Quad>,
}

/// A text unit of one page, kept beside its index entry.
struct Piece {
    page: PathBuf,
    title: String,
    source: Source,
    text: String,
    /// For picture text: the picture and its lines with their boxes (from the `.ocr.json` file).
    picture: Option<PathBuf>,
    lines: Vec<(String, Quad)>,
}

/// Reads a picture's `<picture>.ocr.json` (schema notecraft.ocr): its text, and each line with its
/// polygon. `None` when the file is missing or is not a file of this format.
fn read_ocr_json(path: &Path) -> Option<(String, Vec<(String, Quad)>)> {
    let text = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    if value.get("schema")?.as_str()? != "notecraft.ocr" || value.get("version")?.as_u64()? != 1 {
        return None;
    }
    let mut lines = Vec::new();
    for item in value.get("lines")?.as_array()? {
        let polygon = item.get("polygon")?.as_array()?;
        // A line without four finite corners is not a box: it is skipped.
        let corners: Option<Vec<[f32; 2]>> = polygon
            .iter()
            .map(|c| {
                let xy = c.as_array()?;
                let (x, y) = (xy.first()?.as_f64()? as f32, xy.get(1)?.as_f64()? as f32);
                (x.is_finite() && y.is_finite()).then_some([x, y])
            })
            .collect();
        let Some(corners) = corners.filter(|c| c.len() == 4) else { continue };
        let quad = [corners[0], corners[1], corners[2], corners[3]];
        lines.push((item.get("text")?.as_str()?.to_string(), quad));
    }
    let text = value.get("text")?.as_str()?.to_string();
    Some((text, lines))
}

/// The texts of one page: its own text, then each picture's alt text and OCR text. A picture's
/// OCR file, `<picture>.ocr.json`, sits in the page's `assets` folder, named after the picture.
fn pieces_of(dir: &Path) -> Vec<Piece> {
    let title = read_node(dir).map(|i| i.title).unwrap_or_default();
    let mut out = Vec::new();
    if let Ok(page) = load_page(dir) {
        let plain = match page.format {
            PageFormat::Html => html::strip_tags(&page.body),
            PageFormat::Cmgfm => html::strip_tags(&render::blocks_to_html(&boxes::read(PageFormat::Cmgfm, &page.body).0)),
        };
        out.push(Piece {
            page: dir.to_path_buf(),
            title: title.clone(),
            source: Source::Page,
            text: format!("{title} {plain}"),
            picture: None,
            lines: Vec::new(),
        });
        // Alt text lives in the page markup, next to the picture it describes.
        for (_, alt) in crate::assets::alt_texts(&page.body, page.format) {
            out.push(Piece { page: dir.to_path_buf(), title: title.clone(), source: Source::Alt, text: alt, picture: None, lines: Vec::new() });
        }
    }
    let assets = dir.join("assets");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&assets).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).collect()).unwrap_or_default();
    names.sort();
    for file in names {
        let Some(name) = file.file_name().and_then(|n| n.to_str()).map(str::to_string) else { continue };
        let Some(picture) = name.strip_suffix(".ocr.json") else { continue };
        // Only the OCR file of a picture that is there: a file of the user's own is not one.
        if !assets.join(picture).is_file() {
            continue;
        }
        if let Some((text, lines)) = read_ocr_json(&file) {
            out.push(Piece {
                page: dir.to_path_buf(),
                title: title.clone(),
                source: Source::Picture,
                text,
                picture: Some(assets.join(picture)),
                lines,
            });
        }
    }
    out
}

/// Ranks the pages of the notebooks folder (and the notebooks opened from elsewhere) against
/// `query` with BM25 over each page's text, its alt text and its picture text. One hit per page,
/// the best of its texts. At most `limit` hits.
pub fn ranked(root: &Path, elsewhere: &[PathBuf], query: &str, limit: usize) -> Vec<Ranked> {
    ranked_in(child_dirs(root).into_iter().chain(elsewhere.iter().cloned()), query, limit)
}

/// Rank one page, or the pages under a notebook or section.
pub fn ranked_under(scope: &Path, query: &str, limit: usize) -> Vec<Ranked> {
    if page_file(scope).is_some() { ranked_pages(vec![scope.to_path_buf()], query, limit) } else { ranked_in([scope.to_path_buf()], query, limit) }
}

fn ranked_in(roots: impl IntoIterator<Item = PathBuf>, query: &str, limit: usize) -> Vec<Ranked> {
    if query.split_whitespace().next().is_none() || limit == 0 {
        return Vec::new();
    }
    let mut pages = Vec::new();
    for root in roots {
        collect_pages(&root, &mut pages);
    }
    ranked_pages(pages, query, limit)
}

// ---------- the ranked search cache (issue #460) ----------
//
// The search runs on every keystroke. Reading every page, rendering it and tokenizing it each
// time made typing slow on a large notebook, so what a page contributes is kept between
// searches, and so is the index built from all of them.
//
// The rule for using what is kept: a page's texts come only from files in the page's own folder
// (its node file and page file) and in its `assets` folder (pictures, `.txt`, `.lines`,
// `.ocr-hash`). The name, size and modification time of every one of those files is the page's
// stamp. What is kept for a page is used only while its stamp is unchanged; any edit, added or
// removed picture, or changed companion file changes it. A file changed so recently that a second
// change could keep the same time and size is never trusted: a page with such a file is read again
// on every search until its time has settled.

/// How long after a file's last change its time and size are trusted to tell a later change.
/// File times on some file systems are only two seconds apart.
const SETTLE: std::time::Duration = std::time::Duration::from_secs(3);
/// How many built indexes are kept, one for each set of pages searched (a notebook, a section).
const KEPT_INDEXES: usize = 4;

/// The name, size and modification time of each file a page's texts are read from.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stamp(Vec<(std::ffi::OsString, u64, std::time::SystemTime)>);

/// The page's stamp, or `None` when it cannot be trusted: a file's time is missing, in the
/// future, or too recent to tell a second change from the first.
fn stamp_of(dir: &Path) -> Option<Stamp> {
    let now = std::time::SystemTime::now();
    let mut files = Vec::new();
    for (folder, prefix) in [(dir.to_path_buf(), ""), (dir.join("assets"), "assets/")] {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries {
            let entry = entry.ok()?;
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified().ok()?;
            if now.duration_since(modified).map_or(true, |age| age < SETTLE) {
                return None;
            }
            let mut name = std::ffi::OsString::from(prefix);
            name.push(entry.file_name());
            files.push((name, metadata.len(), modified));
        }
    }
    files.sort();
    Some(Stamp(files))
}

/// What one page gives the search: its texts, and every word in them.
struct PageTexts {
    pieces: Vec<Piece>,
    words: std::collections::HashSet<String>,
}

fn texts_of(dir: &Path) -> PageTexts {
    let pieces = pieces_of(dir);
    let words = pieces.iter().flat_map(|piece| notecraft_search::tokenize(&piece.text)).collect();
    PageTexts { pieces, words }
}

/// An index over a set of pages, and the pages' texts in the order the index numbers them.
struct Built {
    key: Vec<(PathBuf, Stamp)>,
    pages: Vec<std::sync::Arc<PageTexts>>,
    /// For each unit of the index: the page and the piece within it.
    units: Vec<(usize, usize)>,
    index: notecraft_search::Index,
}

#[derive(Default)]
struct Cache {
    pages: std::collections::HashMap<PathBuf, (Stamp, std::sync::Arc<PageTexts>)>,
    built: std::collections::VecDeque<std::sync::Arc<Built>>,
}

fn cache() -> std::sync::MutexGuard<'static, Cache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    let lock = CACHE.get_or_init(|| std::sync::Mutex::new(Cache::default()));
    // A search that panicked part way left nothing half-written that matters: start again.
    lock.lock().unwrap_or_else(|poisoned| {
        let mut guard = poisoned.into_inner();
        *guard = Cache::default();
        guard
    })
}

/// The index over `pages`, built now or kept from an earlier search of the same, unchanged pages.
fn built_for(pages: &[PathBuf]) -> std::sync::Arc<Built> {
    let stamps: Vec<Option<Stamp>> = pages.iter().map(|dir| stamp_of(dir)).collect();
    let settled: Option<Vec<(PathBuf, Stamp)>> =
        pages.iter().zip(&stamps).map(|(dir, stamp)| stamp.clone().map(|stamp| (dir.clone(), stamp))).collect();
    if let Some(key) = &settled
        && let Some(built) = cache().built.iter().find(|built| &built.key == key)
    {
        return std::sync::Arc::clone(built);
    }
    let texts: Vec<std::sync::Arc<PageTexts>> = pages
        .iter()
        .zip(stamps)
        .map(|(dir, stamp)| {
            let Some(stamp) = stamp else { return std::sync::Arc::new(texts_of(dir)) };
            if let Some((kept, texts)) = cache().pages.get(dir)
                && *kept == stamp
            {
                return std::sync::Arc::clone(texts);
            }
            let texts = std::sync::Arc::new(texts_of(dir));
            cache().pages.insert(dir.clone(), (stamp, std::sync::Arc::clone(&texts)));
            texts
        })
        .collect();
    let mut units = Vec::new();
    let mut texts_in_order = Vec::new();
    for (page, page_texts) in texts.iter().enumerate() {
        for (piece, text) in page_texts.pieces.iter().enumerate() {
            units.push((page, piece));
            texts_in_order.push(notecraft_search::Unit { id: texts_in_order.len().to_string(), text: text.text.clone() });
        }
    }
    let index = notecraft_search::Index::build(&texts_in_order);
    let built = std::sync::Arc::new(Built { key: settled.clone().unwrap_or_default(), pages: texts, units, index });
    if settled.is_some() {
        let mut cache = cache();
        cache.built.push_front(std::sync::Arc::clone(&built));
        cache.built.truncate(KEPT_INDEXES);
        // Pages no kept index uses are forgotten, so deleted and renamed pages do not pile up.
        let in_use: std::collections::HashSet<&PathBuf> = cache.built.iter().flat_map(|built| built.key.iter().map(|(dir, _)| dir)).collect();
        let forget: Vec<PathBuf> = cache.pages.keys().filter(|dir| !in_use.contains(dir)).cloned().collect();
        for dir in forget {
            cache.pages.remove(&dir);
        }
    }
    built
}

fn ranked_pages(mut pages: Vec<PathBuf>, query: &str, limit: usize) -> Vec<Ranked> {
    if query.split_whitespace().next().is_none() || limit == 0 {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    pages.retain(|page| seen.insert(std::fs::canonicalize(page).unwrap_or_else(|_| page.clone())));
    let built = built_for(&pages);
    // A page qualifies when every word of the query appears somewhere on it: its title, its text,
    // its alt text or the text read from its pictures. The ranking then orders the matches.
    let wanted: Vec<String> = {
        let mut words = notecraft_search::tokenize(query);
        words.sort();
        words.dedup();
        words
    };
    let mut best: Vec<Ranked> = Vec::new();
    for hit in built.index.search(query, usize::MAX) {
        let Some(&(page, i)) = hit.id.parse::<usize>().ok().and_then(|unit| built.units.get(unit)) else { continue };
        let page_texts = &built.pages[page];
        let piece = &page_texts.pieces[i];
        if best.iter().any(|r| r.path == piece.page) || !wanted.iter().all(|w| page_texts.words.contains(w)) {
            continue;
        }
        let snippet = snippet_around(&piece.text, &wanted);
        let boxes: Vec<Quad> = piece
            .lines
            .iter()
            .filter(|(words, _)| notecraft_search::tokenize(words).iter().any(|w| wanted.contains(w)))
            .map(|(_, quad)| *quad)
            .collect();
        best.push(Ranked {
            path: piece.page.clone(),
            title: piece.title.clone(),
            source: piece.source,
            snippet,
            score: hit.score,
            picture: piece.picture.clone(),
            boxes,
        });
        if best.len() >= limit {
            break;
        }
    }
    best
}

/// Byte ranges of whole words in a displayed excerpt that match the query.
pub fn snippet_matches(snippet: &str, query: &str) -> Vec<std::ops::Range<usize>> {
    notecraft_search::match_ranges(snippet, &notecraft_search::tokenize(query)).collect()
}

/// A short, single-line snippet of `text`, lowercased as the search has always shown them, centred on
/// the first place a wanted word appears. With no match in view it starts at the top of the text.
fn snippet_around(text: &str, wanted: &[String]) -> String {
    const WIDTH: usize = 120;
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let first = notecraft_search::match_ranges(&flat, wanted).next().map_or(0, |range| range.start);
    let chars: Vec<char> = flat.chars().collect();
    let at = flat[..first].chars().count();
    let start = at.saturating_sub(WIDTH / 3).min(chars.len().saturating_sub(WIDTH));
    chars[start..(start + WIDTH).min(chars.len())].iter().collect()
}

#[cfg(test)]
mod tests {
    /// The OCR file a picture would have: its lines, each with a four-corner polygon `[x0,y0,...,y3]`.
    fn ocr_json_for(lines: &[(&str, serde_json::Value)]) -> String {
        let items: Vec<serde_json::Value> =
            lines.iter().map(|(text, polygon)| serde_json::json!({ "text": text, "polygon": polygon, "confidence": null })).collect();
        let joined: Vec<&str> = lines.iter().map(|(text, _)| *text).collect();
        serde_json::json!({
            "schema": "notecraft.ocr", "version": 1,
            "picture": { "name": "receipt.png", "sha256": "test", "width": 0, "height": 0 },
            "produced": { "at": 0, "by": "test" },
            "text": joined.join("\n"), "lines": items, "bounds": null, "alt": null,
        })
        .to_string()
    }

    fn corners(c: [f32; 8]) -> serde_json::Value {
        serde_json::json!([[c[0], c[1]], [c[2], c[3]], [c[4], c[5]], [c[6], c[7]]])
    }

    use super::*;
    use crate::ops;
    use crate::page::save_body;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::NodeKind;

    fn fixture(t: &TempDir) -> PathBuf {
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let a = ops::create_node(&nb, "Kickoff", NodeKind::Page, Some(PageFormat::Html), &s).expect("a");
        save_body(&a, "<h1>Kickoff</h1><p>We decide the <b>format</b> today.</p>").expect("save");
        let b = ops::create_node(&a, "Minutes", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("b");
        save_body(&b, "# Minutes\n\n- [ ] Review the caf\u{e9} budget\n").expect("save");
        root
    }

    #[test]
    fn finds_text_in_both_formats_and_in_titles() {
        let t = TempDir::new("search1");
        let root = fixture(&t);
        assert_eq!(search(&root, "decide the format", 10).len(), 1, "tags between words are ignored");
        assert_eq!(search(&root, "CAF\u{c9}", 10).len(), 1, "case-insensitive, non-ASCII");
        assert_eq!(search(&root, "minutes", 10).len(), 1, "title match");
        assert!(search(&root, "absent", 10).is_empty());
    }

    #[test]
    fn a_page_scope_excludes_subpages_and_highlights_normalized_words() {
        let t = TempDir::new("ranked-scope");
        let root = fixture(&t);
        let all = ranked(&root, &[], "budget", 1);
        assert_eq!(all.len(), 1);
        let parent = all[0].path.parent().expect("parent page");
        assert!(ranked_under(parent, "budget", 1).is_empty());
        let notebook = parent.parent().expect("notebook");
        assert_eq!(ranked_under(notebook, "budget", 1), all);

        let snippet = "cafeteria, cafe\u{301}, \u{645}\u{644}\u{641}";
        let matches = snippet_matches(snippet, "CAF\u{c9} \u{645}\u{644}\u{641}");
        assert_eq!(matches.into_iter().map(|range| &snippet[range]).collect::<Vec<_>>(), ["cafe\u{301}", "\u{645}\u{644}\u{641}"]);
    }

    /// Issue #104: on a CMGFM page a phrase is found whatever marks are round its words, as
    /// it is on an HTML page.
    #[test]
    fn a_phrase_across_formatting_is_found_on_a_markdown_page() {
        let t = TempDir::new("search-md-formatting");
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let md = ops::create_node(&nb, "Markdown", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("md");
        save_body(&md, "Hello **world**, see the *new* [plan](https://example.test) and `the code`.\n").expect("save");
        let html = ops::create_node(&nb, "Html", NodeKind::Page, Some(PageFormat::Html), &s).expect("html");
        save_body(&html, "<p>Hello <b>world</b>, see the <i>new</i> <a href=\"https://example.test\">plan</a> and <code>the code</code>.</p>")
            .expect("save");
        for phrase in ["Hello world", "see the new plan and the code"] {
            let found: Vec<String> = search(&root, phrase, 10).into_iter().map(|hit| hit.title).collect();
            assert!(found.contains(&"Markdown".to_string()) && found.contains(&"Html".to_string()), "{phrase:?} on both pages: {found:?}");
        }
        assert!(search(&root, "**world**", 10).is_empty(), "the marks themselves are not words on the page");
    }

    #[test]
    fn limits_and_empty_needles() {
        let t = TempDir::new("search2");
        let root = fixture(&t);
        assert!(search(&root, "", 10).is_empty());
        assert!(search(&root, "   ", 10).is_empty());
        assert!(search(&root, "the", 0).is_empty());
        assert_eq!(search(&root, "the", 1).len(), 1);
    }

    #[test]
    fn snippets_are_short_single_lines() {
        let t = TempDir::new("search3");
        let root = fixture(&t);
        let hit = &search(&root, "review", 5)[0];
        assert!(hit.snippet.chars().count() <= 100 && !hit.snippet.contains('\n'), "{:?}", hit.snippet);
    }

    #[test]
    fn a_missing_root_is_empty_not_an_error() {
        assert!(search(Path::new("/definitely/not/here"), "x", 5).is_empty());
    }

    /// Issue #105: a word that is partly bold on an HTML page is found as the word it reads as.
    #[test]
    fn a_partly_formatted_word_on_an_html_page_is_found() {
        let t = TempDir::new("search-html-inline");
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "Plain", NodeKind::Page, Some(PageFormat::Html), &s).expect("page");
        save_body(&page, "<p>The for<b>mat</b> of co<i>op</i>eration</p>").expect("save");
        assert_eq!(search(&root, "format", 10).len(), 1);
        assert_eq!(search(&root, "the format of cooperation", 10).len(), 1);
    }

    /// A notebook with one page, a picture in it, and the picture's companion files.
    fn picture_fixture(t: &TempDir) -> (PathBuf, PathBuf) {
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let a = ops::create_node(&nb, "Receipts", NodeKind::Page, Some(PageFormat::Html), &s).expect("a");
        save_body(&a, "<h1>Receipts</h1><p>Paid in cash.</p><img src=\"assets/receipt.png\" alt=\"a bakery receipt\">").expect("save");
        let assets = a.join("assets");
        std::fs::create_dir_all(&assets).expect("assets");
        std::fs::write(assets.join("receipt.png"), b"not really a picture").expect("picture");
        std::fs::write(
            assets.join("receipt.png.ocr.json"),
            ocr_json_for(&[
                ("Sourdough 6.00", corners([10.0, 20.0, 110.0, 20.0, 110.0, 40.0, 10.0, 40.0])),
                ("Flat white 4.50", corners([20.0, 50.0, 100.0, 50.0, 100.0, 70.0, 20.0, 70.0])),
            ]),
        )
        .expect("ocr file");
        std::fs::write(assets.join("notes.txt"), "a text file the user wrote").expect("users own file");
        (root, a)
    }

    #[test]
    fn a_word_in_picture_text_finds_the_page_with_that_picture() {
        let t = TempDir::new("search-ranked-ocr");
        let (root, page) = picture_fixture(&t);
        let hits = ranked(&root, &[], "sourdough", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, page);
        assert_eq!(hits[0].source, Source::Picture);
    }

    #[test]
    fn a_page_needs_every_query_word_and_its_snippet_shows_the_match() {
        let t = TempDir::new("search-and-words");
        let (root, page) = picture_fixture(&t);
        assert!(ranked(&root, &[], "cash unicorn", 10).iter().all(|h| h.path != page), "a page missing one word does not qualify");
        assert!(ranked(&root, &[], "   ", 10).is_empty(), "a blank query finds nothing");
        let hits = ranked(&root, &[], "cash", 10);
        assert!(hits.iter().any(|h| h.path == page && h.snippet.contains("cash")), "{hits:?}");
    }

    #[test]
    fn a_long_page_gets_a_snippet_around_the_match_not_its_first_words() {
        let filler = "lorem ipsum dolor sit amet ".repeat(30);
        let body = format!("{filler}the needle is here {filler}");
        let snippet = snippet_around(&body, &["needle".to_string()]);
        assert!(snippet.contains("needle"), "{snippet}");
        assert!(snippet.chars().count() <= 120 && !snippet.contains('\n'));
    }

    #[test]
    fn ranked_search_matches_exact_emoji_and_unaccented_words_in_either_format() {
        let t = TempDir::new("search-ranked-unicode");
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let html = ops::create_node(&nb, "HTML", NodeKind::Page, Some(PageFormat::Html), &s).expect("html");
        let md = ops::create_node(&nb, "Markdown 🇨🇦", NodeKind::Page, Some(PageFormat::Cmgfm), &s).expect("md");
        let filler = "intro ".repeat(40);
        save_body(&html, &format!("<p>{filler}café السَّلَامُ 👩🏽‍💻</p>")).expect("save html");
        save_body(&md, "café السَّلَامُ 👩‍💻\n").expect("save markdown");
        let hits = ranked(&root, &[], "cafe السلام", 10);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert!(hits.iter().any(|hit| hit.path == md));
        let hits = ranked(&root, &[], "cafe السلام 👩🏽‍💻", 10);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].path, html);
        assert!(hits[0].snippet.contains("café السَّلَامُ 👩🏽‍💻"), "{hits:?}");
        let snippet = &hits[0].snippet;
        let matches: Vec<&str> = snippet_matches(snippet, "cafe السلام 👩🏽‍💻").into_iter().map(|range| &snippet[range]).collect();
        assert_eq!(matches, ["café", "السَّلَامُ", "👩🏽‍💻"]);
        assert_eq!(ranked(&root, &[], "🇨🇦", 10)[0].path, md);
        assert!(ranked(&root, &[], "cafe 🇨🇲", 10).is_empty());
    }

    #[test]
    fn a_chinese_or_japanese_run_is_found_only_as_a_whole_run() {
        // Documented limit: there is no word segmentation for CJK, so the whole run is the word.
        let t = TempDir::new("search-cjk");
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let page = ops::create_node(&nb, "Weather", NodeKind::Page, Some(PageFormat::Html), &s).expect("page");
        save_body(&page, "<p>今日は良い天気です</p>").expect("save");
        assert_eq!(ranked(&root, &[], "今日は良い天気です", 10).len(), 1, "the whole run is found");
        assert!(ranked(&root, &[], "天気", 10).is_empty(), "a part of the run is not, until segmentation exists");
    }

    #[test]
    fn malformed_box_records_are_skipped_and_the_hit_still_found() {
        let t = TempDir::new("search-malformed-lines");
        let (root, page) = picture_fixture(&t);
        let assets = page.join("assets");
        std::fs::write(
            assets.join("receipt.png.ocr.json"),
            ocr_json_for(&[
                ("Sourdough", corners([0.0, 0.0, 110.0, 0.0, 110.0, 40.0, 10.0, 40.0])),
                ("Sourdough", serde_json::json!([[1.0, 2.0], [3.0, 4.0], [5.0, 6.0]])),
            ]),
        )
        .expect("bad ocr file");
        let hits = ranked(&root, &[], "sourdough", 10);
        let hit = hits.iter().find(|h| h.path == page).expect("the page is still found from its text");
        assert_eq!(hit.boxes.len(), 1, "the valid line is a box, the malformed one is not: {:?}", hit.boxes);
    }

    #[test]
    fn the_page_with_more_matches_ranks_first() {
        let t = TempDir::new("search-order");
        let s = Settings::at(t.path().join("s.toml"));
        let root = t.path().join("nb");
        let nb = ops::create_notebook(&root, "NB", None, &s).expect("nb");
        let once = ops::create_node(&nb, "Once", NodeKind::Page, Some(PageFormat::Html), &s).expect("once");
        save_body(&once, "<p>bread on the table</p>").expect("save");
        let often = ops::create_node(&nb, "Often", NodeKind::Page, Some(PageFormat::Html), &s).expect("often");
        save_body(&often, "<p>bread, more bread, and bread again</p>").expect("save");
        let hits = ranked(&root, &[], "bread", 10);
        assert_eq!(hits.len(), 2, "{hits:?}");
        assert_eq!(hits[0].title, "Often", "{hits:?}");
        assert!(hits[0].score >= hits[1].score);
    }

    #[test]
    fn a_word_in_alt_text_finds_the_page_with_that_picture() {
        let t = TempDir::new("search-ranked-alt");
        let (root, page) = picture_fixture(&t);
        let hits = ranked(&root, &[], "bakery", 10);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, page);
        assert_eq!(hits[0].source, Source::Alt);
    }

    #[test]
    fn the_page_text_is_searched_too_and_one_page_is_one_hit() {
        let t = TempDir::new("search-ranked-page");
        let (root, page) = picture_fixture(&t);
        let hits = ranked(&root, &[], "cash receipt", 10);
        assert_eq!(hits.iter().filter(|h| h.path == page).count(), 1, "one hit per page");
        assert!(matches!(hits[0].source, Source::Page | Source::Alt), "the best text of the page is reported: {:?}", hits[0].source);
    }

    #[test]
    fn a_text_file_that_is_not_a_picture_companion_is_not_indexed() {
        let t = TempDir::new("search-ranked-users-file");
        let (root, _) = picture_fixture(&t);
        assert!(ranked(&root, &[], "wrote", 10).is_empty(), "the user's own text file is not searched");
    }

    #[test]
    fn the_companion_text_is_derived_so_removing_it_drops_the_match() {
        let t = TempDir::new("search-ranked-derived");
        let (root, page) = picture_fixture(&t);
        assert_eq!(ranked(&root, &[], "sourdough", 10).len(), 1);
        std::fs::remove_file(page.join("assets").join("receipt.png.ocr.json")).expect("the companion exists");
        assert!(ranked(&root, &[], "sourdough", 10).is_empty(), "the index is rebuilt from the files that remain");
    }

    /// A6 and A7 on the POC notebook. Set NOTECRAFT_OCR_POC_ROOT to the folder that holds the
    /// POC notebook (the private test set); run with --ignored.
    fn poc_root() -> PathBuf {
        std::env::var_os("NOTECRAFT_OCR_POC_ROOT").map(PathBuf::from).expect("set NOTECRAFT_OCR_POC_ROOT to the folder holding the POC notebook")
    }

    #[test]
    #[ignore = "needs NOTECRAFT_OCR_POC_ROOT; run with --ignored"]
    fn a6_sourdough_finds_the_receipt_page() {
        let hits = ranked(&poc_root(), &[], "sourdough", 10);
        let hit = hits.iter().find(|h| h.path.ends_with("receipt-photo")).expect("the receipt page is found");
        assert_eq!(hit.source, Source::Picture, "the word is in the text read from the picture: {hit:?}");
    }

    #[test]
    #[ignore = "needs NOTECRAFT_OCR_POC_ROOT; run with --ignored"]
    fn a7_an_alt_text_word_finds_the_page_with_the_picture() {
        let hits = ranked(&poc_root(), &[], "tuesday", 10);
        let hit = hits.iter().find(|h| h.path.ends_with("receipt-photo")).expect("the receipt page is found");
        assert_eq!(hit.source, Source::Alt, "the word is only in the alt text: {hit:?}");
    }

    #[test]
    fn a_picture_hit_carries_the_boxes_of_the_matching_lines() {
        let t = TempDir::new("search-ranked-boxes");
        let (root, page) = picture_fixture(&t);
        let hits = ranked(&root, &[], "sourdough", 10);
        let hit = &hits[0];
        assert_eq!(hit.picture.as_deref(), Some(page.join("assets").join("receipt.png").as_path()));
        assert_eq!(hit.boxes, vec![[[10.0, 20.0], [110.0, 20.0], [110.0, 40.0], [10.0, 40.0]]], "only the line with the word");
    }

    #[test]
    fn an_alt_text_hit_has_no_boxes() {
        let t = TempDir::new("search-ranked-no-boxes");
        let (root, _) = picture_fixture(&t);
        let hits = ranked(&root, &[], "bakery", 10);
        assert!(hits[0].boxes.is_empty());
    }

    // ---------- the ranked search cache (issue #460) ----------

    /// Gives every file under `dir` a modification time `minutes_ago` minutes in the past, the way
    /// files look a while after they were last written. Only then does the search keep their texts.
    fn age(dir: &Path, minutes_ago: u64) {
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(60 * minutes_ago);
        for entry in std::fs::read_dir(dir).expect("folder") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                age(&path, minutes_ago);
            } else {
                std::fs::File::options().write(true).open(&path).expect("file").set_modified(when).expect("set the time");
            }
        }
    }

    #[test]
    fn a_page_edit_of_the_same_size_is_found_after_the_index_was_kept() {
        let t = TempDir::new("ranked-cache-edit");
        let root = fixture(&t);
        age(&root, 60);
        assert_eq!(ranked(&root, &[], "budget", 10).len(), 1);
        assert_eq!(ranked(&root, &[], "budget", 10).len(), 1, "the kept index gives the same answer");
        let minutes = ranked(&root, &[], "budget", 10)[0].path.clone();
        // "budget" becomes "bucket": the page file keeps its size, only its time tells.
        save_body(&minutes, "# Minutes\n\n- [ ] Review the caf\u{e9} bucket\n").expect("edit");
        age(&minutes, 30);
        assert!(ranked(&root, &[], "budget", 10).is_empty(), "the old word is gone");
        assert_eq!(ranked(&root, &[], "bucket", 10).len(), 1, "the new word is found");
    }

    #[test]
    fn a_page_written_a_moment_ago_is_read_again_every_time() {
        let t = TempDir::new("ranked-cache-fresh");
        let root = fixture(&t);
        assert_eq!(ranked(&root, &[], "budget", 10).len(), 1);
        let minutes = ranked(&root, &[], "budget", 10)[0].path.clone();
        save_body(&minutes, "# Minutes\n\n- [ ] Review the caf\u{e9} bucket\n").expect("edit at once");
        assert!(ranked(&root, &[], "budget", 10).is_empty());
        assert_eq!(ranked(&root, &[], "bucket", 10).len(), 1);
    }

    #[test]
    fn changed_picture_text_and_boxes_are_found_after_the_index_was_kept() {
        let t = TempDir::new("ranked-cache-ocr");
        let (root, page) = picture_fixture(&t);
        age(&root, 60);
        assert_eq!(ranked(&root, &[], "sourdough", 10).len(), 1);
        let assets = page.join("assets");
        std::fs::write(
            assets.join("receipt.png.ocr.json"),
            ocr_json_for(&[
                ("Rye loaf 6.00", corners([1.0, 2.0, 3.0, 2.0, 3.0, 4.0, 1.0, 4.0])),
                ("Flat white 4.50", corners([20.0, 50.0, 100.0, 50.0, 100.0, 70.0, 20.0, 70.0])),
            ]),
        )
        .expect("new ocr file");
        age(&assets, 30);
        assert!(ranked(&root, &[], "sourdough", 10).is_empty(), "the old picture text is gone");
        let hits = ranked(&root, &[], "rye", 10);
        assert_eq!(hits.len(), 1, "the new picture text is found");
        assert_eq!(hits[0].boxes, vec![[[1.0, 2.0], [3.0, 2.0], [3.0, 4.0], [1.0, 4.0]]], "with the new boxes");
    }

    #[test]
    fn a_removed_or_added_picture_is_seen_after_the_index_was_kept() {
        let t = TempDir::new("ranked-cache-picture");
        let (root, page) = picture_fixture(&t);
        age(&root, 60);
        assert_eq!(ranked(&root, &[], "sourdough", 10).len(), 1);
        let picture = page.join("assets").join("receipt.png");
        std::fs::remove_file(&picture).expect("remove the picture");
        assert!(ranked(&root, &[], "sourdough", 10).is_empty(), "the text of a removed picture no longer counts");
        std::fs::write(&picture, b"the picture again").expect("add the picture back");
        age(&root, 20);
        assert_eq!(ranked(&root, &[], "sourdough", 10).len(), 1, "an added picture's text counts again");
    }

    #[test]
    fn a_deleted_page_is_not_found_after_the_index_was_kept() {
        let t = TempDir::new("ranked-cache-delete");
        let root = fixture(&t);
        age(&root, 60);
        let minutes = ranked(&root, &[], "budget", 10)[0].path.clone();
        std::fs::remove_dir_all(&minutes).expect("delete the page");
        assert!(ranked(&root, &[], "budget", 10).is_empty());
    }

    #[test]
    fn a_page_scope_and_the_whole_folder_are_kept_apart() {
        let t = TempDir::new("ranked-cache-scopes");
        let root = fixture(&t);
        age(&root, 60);
        let all = ranked(&root, &[], "budget", 10);
        let parent = all[0].path.parent().expect("parent page").to_path_buf();
        assert!(ranked_under(&parent, "budget", 10).is_empty(), "the parent page alone does not say budget");
        assert_eq!(ranked(&root, &[], "budget", 10).len(), 1, "the whole folder still does");
    }
}

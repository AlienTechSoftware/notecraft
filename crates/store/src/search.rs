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
    /// Text read from a picture by OCR (`<picture>.txt`).
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
    /// For picture text: the picture and its lines with their boxes (from the `.lines` file).
    picture: Option<PathBuf>,
    lines: Vec<(String, Quad)>,
}

/// Reads a picture's `.lines` file: one record per line, `x0,y0,...,y3<TAB>text`.
fn read_lines_file(path: &Path) -> Vec<(String, Quad)> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter_map(|record| {
            let (coords, words) = record.split_once('\t')?;
            // A record with a missing, unreadable or non-finite coordinate is not a box: skip it.
            let nums: Vec<f32> = coords.split(',').filter_map(|n| n.parse().ok()).filter(|n: &f32| n.is_finite()).collect();
            if nums.len() != 8 {
                return None;
            }
            let quad = [[nums[0], nums[1]], [nums[2], nums[3]], [nums[4], nums[5]], [nums[6], nums[7]]];
            Some((words.to_string(), quad))
        })
        .collect()
}

/// The texts of one page: its own text, then each picture's alt text and OCR text. A picture's
/// companion files sit in the page's `assets` folder, named after the picture.
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
        let Some(picture) = name.strip_suffix(".txt") else { continue };
        let source = Source::Picture;
        // Only companions of a picture that is there: a text file of the user's own is not one.
        if !assets.join(picture).is_file() {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&file) {
            let (picture, lines) = if source == Source::Picture {
                (Some(assets.join(picture)), read_lines_file(&assets.join(format!("{picture}.lines"))))
            } else {
                (None, Vec::new())
            };
            out.push(Piece { page: dir.to_path_buf(), title: title.clone(), source, text, picture, lines });
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

fn ranked_pages(mut pages: Vec<PathBuf>, query: &str, limit: usize) -> Vec<Ranked> {
    if query.split_whitespace().next().is_none() || limit == 0 {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::new();
    pages.retain(|page| seen.insert(std::fs::canonicalize(page).unwrap_or_else(|_| page.clone())));
    let pieces: Vec<Piece> = pages.iter().flat_map(|dir| pieces_of(dir)).collect();
    let units: Vec<notecraft_search::Unit> =
        pieces.iter().enumerate().map(|(i, piece)| notecraft_search::Unit { id: i.to_string(), text: piece.text.clone() }).collect();
    let index = notecraft_search::Index::build(&units);
    // A page qualifies when every word of the query appears somewhere on it: its title, its text,
    // its alt text or the text read from its pictures. The ranking then orders the matches.
    let wanted: Vec<String> = {
        let mut words = notecraft_search::tokenize(query);
        words.sort();
        words.dedup();
        words
    };
    let mut words_on_page: std::collections::HashMap<&Path, std::collections::HashSet<String>> = std::collections::HashMap::new();
    for piece in &pieces {
        words_on_page.entry(piece.page.as_path()).or_default().extend(notecraft_search::tokenize(&piece.text));
    }
    let qualifies = |page: &Path| words_on_page.get(page).is_some_and(|words| wanted.iter().all(|w| words.contains(w)));
    let mut best: Vec<Ranked> = Vec::new();
    for hit in index.search(query, usize::MAX) {
        let Ok(i) = hit.id.parse::<usize>() else { continue };
        let piece = &pieces[i];
        if best.iter().any(|r| r.path == piece.page) || !qualifies(&piece.page) {
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
            assets.join("receipt.png.txt"),
            "Sourdough 6.00
Flat white 4.50
",
        )
        .expect("ocr text");
        std::fs::write(
            assets.join("receipt.png.lines"),
            "10,20,110,20,110,40,10,40	Sourdough 6.00
20,50,100,50,100,70,20,70	Flat white 4.50
",
        )
        .expect("boxes");
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
        std::fs::write(assets.join("receipt.png.lines"), "NaN,0,110,0,110,40,10,40\tSourdough\ninf,0,1,0,1,1,0,1\tSourdough\n1,2,3\tSourdough\n")
            .expect("bad boxes");
        let hits = ranked(&root, &[], "sourdough", 10);
        let hit = hits.iter().find(|h| h.path == page).expect("the page is still found from its text");
        assert!(hit.boxes.is_empty(), "no record with a bad coordinate becomes a box: {:?}", hit.boxes);
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
        std::fs::remove_file(page.join("assets").join("receipt.png.txt")).expect("the companion exists");
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
}

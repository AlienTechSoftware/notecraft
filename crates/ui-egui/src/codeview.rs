//! The code view: every file of a page on the left, the one that is picked on the right.
//!
//! A page is a folder, and the window should be able to show that folder as it is. The
//! old Source button put the page's text beside the page itself, which showed one file of
//! the several a page owns and left two editable copies of the same page on screen at
//! once. This is the view [`docs/dom_serialization/page-model.md`](../../../docs/dom_serialization/page-model.md) section 4
//! asks for: a list, a viewer, and one editable copy.
//!
//! What can be edited: the page file, whole, head and all, and the SVG of a drawing.
//! The head was read-only at first, so that a hand edit could not break the page's id;
//! the owner asked for the whole file on 2026-10-19, which is the point of a code view -
//! a person debugging a page needs to change the line that is wrong, wherever it is.
//! Everything else is read-only, because a hand edit to a picture or to a `_node.toml`
//! is not something to do with a text box.

use crate::highlight::{self, Class, Lang, Span};
use crate::theme::{Tokens, mix};
use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{Color32, Key, Modifiers, Sense, Ui, vec2};
use std::path::Path;

/// How much of a file is read for the viewer. A larger file shows its first part and says
/// so, which keeps the view quick whatever is in the folder.
pub const MAX_SHOWN: usize = 256 * 1024;

/// What kind of file a row is, which decides how it is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `index.html` or `index.md`: the page's own text.
    PageFile,
    /// A drawing of ours: SVG carrying the ink namespace.
    Ink,
    /// A Mermaid diagram's source kept as a file of its own (`.mmd`): text, and it can be typed in.
    Diagram,
    /// A picture the window can draw.
    Picture,
    /// Anything else we can show as text.
    Text,
    /// A file shown by name, size and type only.
    Other,
}

impl Kind {
    /// Whether what is shown can be typed in.
    pub fn editable(self) -> bool {
        matches!(self, Kind::PageFile | Kind::Ink | Kind::Diagram)
    }
}

/// One row of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// What the row is called, and what the page would link to: `index.html`, `ink.svg`,
    /// `assets/koala.png`.
    pub name: String,
    pub kind: Kind,
    pub bytes: u64,
    /// False when nothing in the page names this file: an orphan, kept and never drawn.
    pub used: bool,
}

/// The state the view keeps between frames.
#[derive(Clone, Debug, Default)]
pub struct State {
    /// The row that is picked. The page file when the view opens.
    pub picked: Option<String>,
    /// The text of the picked file while it is being read or edited. The page file is not
    /// held here: its body is the editor's own, so there is only ever one copy of it.
    pub text: String,
    /// Which file `text` was read from, so a change of row reads the new one.
    pub loaded: Option<String>,
    /// The picked file has been typed in and not yet written.
    pub dirty: bool,
    /// The page's own file was typed in, so the page has to be read again when the view
    /// closes.
    pub page_edited: bool,
    /// A drawing is shown as its strokes; this is the one the code is shown for instead.
    pub as_code: bool,
    /// The page folder the state belongs to. Another page is another folder, so everything held
    /// from the last one is dropped: its text would otherwise be shown, and typed over, as this
    /// page's own.
    pub dir: Option<std::path::PathBuf>,
    /// The Find bar under the editor.
    pub find: Find,
    /// The colour spans of `text`, and a fingerprint of the text and format they were made for.
    pub coloured: Vec<Span>,
    pub coloured_for: u64,
}

/// The Find bar: what is being looked for, and where it was found in the text in hand.
#[derive(Clone, Debug, Default)]
pub struct Find {
    pub open: bool,
    pub query: String,
    /// Match the case of the letters.
    pub case: bool,
    /// Where the query was found, as character positions `(from, to)` in the text, which is how
    /// the text box counts.
    pub matches: Vec<(usize, usize)>,
    /// Which of them is the one the bar is on.
    pub current: usize,
    /// A fingerprint of the text, query and case the matches were found in.
    pub found_in: u64,
    /// Put the caret in the query box on the next frame.
    pub focus: bool,
    /// Scroll the current match into view on the next frame.
    pub jump: bool,
    /// Select the current match in the text and give it the caret on the next frame: closing the
    /// bar leaves you where the search ended.
    pub select: bool,
}

/// The most matches kept. A search for one letter in a big file would otherwise be thousands of
/// highlights that nobody reads.
pub const MAX_MATCHES: usize = 5_000;

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Where `query` is in `text`, as character positions `(from, to)`, none overlapping, in order.
pub fn find_matches(text: &str, query: &str, case: bool) -> Vec<(usize, usize)> {
    let needle: Vec<char> = query.chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let hay: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    if hay.len() < needle.len() {
        return out;
    }
    let same = |a: char, b: char| a == b || (!case && lower(a) == lower(b));
    let mut i = 0;
    while i + needle.len() <= hay.len() && out.len() < MAX_MATCHES {
        if needle.iter().enumerate().all(|(j, n)| same(hay[i + j], *n)) {
            out.push((i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    out
}

fn fingerprint(parts: &[&str], flag: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    flag.hash(&mut h);
    h.finish()
}

/// The colour a kind of text is drawn in. All are tokens, so every theme has them readable.
pub fn class_colour(class: Class, t: &Tokens) -> Color32 {
    // Four hues that differ from each other in every theme: the accent for what structures a file
    // (tags, keywords, headings), teal for names (attributes, keys, code), green for strings and
    // amber for numbers and emphasis; the rest is the quiet colour.
    match class {
        Class::Tag | Class::Keyword | Class::Heading => t.accent,
        Class::Attr | Class::Code => t.section,
        Class::Str => t.ok,
        Class::Number | Class::Emphasis => t.group,
        Class::Link => t.link,
        Class::Comment | Class::Punct | Class::Meta => t.muted,
    }
}

/// The background of every match, and of the one the bar is on.
pub fn match_colours(t: &Tokens) -> (Color32, Color32) {
    (t.row_selected, mix(t.row_selected, t.accent, 0.15))
}

/// Byte ranges for character ranges, in one pass. The ranges are in order.
fn byte_ranges(text: &str, chars: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(chars.len());
    let mut at = text.char_indices().map(|(b, _)| b).chain(std::iter::once(text.len()));
    let mut seen = 0usize;
    let mut last = 0usize;
    let mut next = |want: usize, seen: &mut usize, last: &mut usize| {
        while *seen < want {
            match at.next() {
                Some(b) => {
                    *last = b;
                    *seen += 1;
                }
                None => break,
            }
        }
        *last
    };
    for (from, to) in chars {
        // `seen` counts characters passed, so the byte of character `n` is the n-th boundary.
        let a = if *from == 0 { 0 } else { next(*from + 1, &mut seen, &mut last) };
        let b = next(*to + 1, &mut seen, &mut last);
        out.push((a, b));
    }
    out
}

/// The text as one laid out job: syntax colours, with the matches of the Find bar behind them.
#[allow(clippy::too_many_arguments)]
pub fn colour_job(
    text: &str,
    spans: &[Span],
    matches: &[(usize, usize)],
    current: Option<usize>,
    t: &Tokens,
    font: egui::FontId,
    base: Color32,
    wrap: f32,
) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let found = byte_ranges(text, matches);
    let mut cuts: Vec<usize> = vec![0, text.len()];
    for s in spans {
        cuts.push(s.range.start);
        cuts.push(s.range.end);
    }
    for (a, b) in &found {
        cuts.push(*a);
        cuts.push(*b);
    }
    cuts.retain(|c| *c <= text.len() && text.is_char_boundary(*c));
    cuts.sort_unstable();
    cuts.dedup();
    let (all, here) = match_colours(t);
    let (mut si, mut mi) = (0, 0);
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        while si < spans.len() && spans[si].range.end <= a {
            si += 1;
        }
        let class = spans.get(si).filter(|s| s.range.start <= a && a < s.range.end).map(|s| s.class);
        while mi < found.len() && found[mi].1 <= a {
            mi += 1;
        }
        let hit = found.get(mi).filter(|r| r.0 <= a && a < r.1).map(|_| mi);
        let mut f = TextFormat::simple(font.clone(), class.map_or(base, |c| class_colour(c, t)));
        f.italics = class == Some(Class::Comment);
        if let Some(k) = hit {
            let this = Some(k) == current;
            f.background = if this { here } else { all };
            if this {
                f.underline = egui::Stroke::new(1.5, t.accent);
            }
        }
        job.append(&text[a..b], 0.0, f);
    }
    job
}

/// What the Find bar asked for.
enum FindAct {
    None,
    Next,
    Previous,
    Close,
    Edited,
}

/// The bar under the editor: a box to type in, Previous and Next, a case switch, how many were found.
fn find_bar(ui: &mut Ui, find: &mut Find, id: egui::Id, t: &Tokens) -> FindAct {
    let mut act = FindAct::None;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Find").small().color(t.muted));
        let query = ui.add(egui::TextEdit::singleline(&mut find.query).id(id).desired_width(240.0).hint_text("Find in this file"));
        if find.focus {
            query.request_focus();
            find.focus = false;
        }
        if query.changed() {
            act = FindAct::Edited;
        }
        // Shift is read from the key press itself as well as from the held keys: a press that is sent
        // in (by a script, or a remote desktop) carries its own modifiers.
        let (enter, shift) = ui.input(|i| {
            let shifted = i.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.shift));
            (i.key_pressed(Key::Enter), i.modifiers.shift || shifted)
        });
        if (query.has_focus() || query.lost_focus()) && enter {
            act = if shift { FindAct::Previous } else { FindAct::Next };
            query.request_focus();
        }
        if ui.small_button("Previous").on_hover_text("Previous match (Shift+Enter)").clicked() {
            act = FindAct::Previous;
        }
        if ui.small_button("Next").on_hover_text("Next match (Enter, F3)").clicked() {
            act = FindAct::Next;
        }
        if ui.selectable_label(find.case, "Match case").clicked() {
            find.case = !find.case;
            act = FindAct::Edited;
        }
        let count = if find.query.is_empty() {
            String::new()
        } else if find.matches.is_empty() {
            "No matches".to_string()
        } else {
            let more = if find.matches.len() >= MAX_MATCHES { "+" } else { "" };
            format!("{} of {}{more}", find.current + 1, find.matches.len())
        };
        ui.label(egui::RichText::new(count).small().color(if find.matches.is_empty() && !find.query.is_empty() { t.warn } else { t.muted }));
        if ui.small_button("Close").on_hover_text("Close the Find bar (Escape)").clicked() {
            act = FindAct::Close;
        }
    });
    act
}
#[derive(Clone, Debug, Default)]
pub struct Out {
    /// A file was typed in: its name and its new text, for the window to write.
    pub wrote: Option<(String, String)>,
}

/// The files of a page, the page file first, then what sits beside it, then `assets/`.
///
/// `named` is what the page's text links to, so a file nothing names can be marked as an
/// orphan rather than quietly listed as if it were in use.
pub fn files(dir: &Path, page_file: &str, named: &[String]) -> Vec<Entry> {
    let mut beside: Vec<Entry> = Vec::new();
    let mut assets: Vec<Entry> = Vec::new();
    let mut page = None;
    for (folder, prefix) in [(dir.to_path_buf(), ""), (dir.join("assets"), "assets/")] {
        let Ok(entries) = std::fs::read_dir(&folder) else { continue };
        for entry in entries.flatten() {
            if !entry.path().is_file() {
                continue;
            }
            let file = entry.file_name().to_string_lossy().into_owned();
            let name = format!("{prefix}{file}");
            let bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
            let kind = kind_of(&entry.path(), &name, page_file);
            let used = kind == Kind::PageFile || named.iter().any(|n| n.trim() == name);
            let row = Entry { name, kind, bytes, used };
            match (kind, prefix) {
                (Kind::PageFile, _) => page = Some(row),
                (_, "assets/") => assets.push(row),
                _ => beside.push(row),
            }
        }
    }
    beside.sort_by(|a, b| a.name.cmp(&b.name));
    assets.sort_by(|a, b| a.name.cmp(&b.name));
    let mut out = Vec::with_capacity(beside.len() + assets.len() + 1);
    out.extend(page);
    out.append(&mut beside);
    out.append(&mut assets);
    out
}

fn kind_of(path: &Path, name: &str, page_file: &str) -> Kind {
    if name == page_file {
        return Kind::PageFile;
    }
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "svg" => {
            if head_of(path, 4096).is_some_and(|head| notecraft_ink::svg::is_ink(&head)) {
                Kind::Ink
            } else {
                Kind::Text
            }
        }
        "mmd" | "mermaid" => Kind::Diagram,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => Kind::Picture,
        "md" | "html" | "htm" | "txt" | "json" | "toml" | "csv" | "xml" | "yaml" | "yml" => Kind::Text,
        _ => Kind::Other,
    }
}

/// The first `want` bytes of a file as text, for looking at a head without reading a
/// drawing that may be megabytes.
fn head_of(path: &Path, want: usize) -> Option<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut buffer = vec![0u8; want];
    let read = file.read(&mut buffer).ok()?;
    buffer.truncate(read);
    Some(String::from_utf8_lossy(&buffer).into_owned())
}

/// Reads a file for the viewer, cut to [`MAX_SHOWN`]. Returns the text and whether it was
/// cut, so the view can say so rather than pretending that is the whole file.
pub fn read_for_view(path: &Path) -> (String, bool) {
    match head_of(path, MAX_SHOWN + 1) {
        Some(text) if text.len() > MAX_SHOWN => (text.chars().take(MAX_SHOWN).collect(), true),
        Some(text) => (text, false),
        None => (String::new(), false),
    }
}

/// How large a file is, in words a person reads rather than a number of bytes.
pub fn size_text(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{} KB", bytes.div_ceil(1024)),
        _ => format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0)),
    }
}

/// Draws the view.
pub fn show(ui: &mut Ui, state: &mut State, entries: &[Entry], dir: &Path, t: &Tokens, height: f32) -> Out {
    let mut out = Out::default();
    if state.dir.as_deref() != Some(dir) {
        // Every page's file is `index.md` or `index.html`, so the name alone cannot say that
        // the text in hand is from this page.
        *state = State { dir: Some(dir.to_path_buf()), ..State::default() };
    }
    let page_file = entries.first().filter(|e| e.kind == Kind::PageFile).map(|e| e.name.clone());
    if state.picked.is_none() || !entries.iter().any(|e| Some(&e.name) == state.picked.as_ref()) {
        state.picked = page_file.clone().or_else(|| entries.first().map(|e| e.name.clone()));
        state.loaded = None;
    }

    ui.horizontal_top(|ui| {
        // ---- the list ----
        let list_width = 240.0_f32.min(ui.available_width() * 0.4);
        ui.allocate_ui_with_layout(vec2(list_width, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.set_min_width(list_width);
            ui.set_max_width(list_width);
            ui.label(egui::RichText::new("FILES IN THIS PAGE").small().color(t.muted));
            ui.add_space(2.0);
            egui::ScrollArea::vertical().id_salt("code-files").max_height(height - 24.0).show(ui, |ui| {
                for entry in entries {
                    let picked = state.picked.as_deref() == Some(entry.name.as_str());
                    let row = row(ui, entry, picked, t);
                    if row.clicked() {
                        state.picked = Some(entry.name.clone());
                        state.loaded = None;
                        state.as_code = false;
                    }
                }
            });
        });
        ui.separator();

        // ---- the file that is picked ----
        let Some(name) = state.picked.clone() else { return };
        let Some(entry) = entries.iter().find(|e| e.name == name) else { return };
        let width = ui.available_width();
        ui.allocate_ui_with_layout(vec2(width, height), egui::Layout::top_down(egui::Align::LEFT), |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&entry.name).monospace().strong());
                ui.label(egui::RichText::new(size_text(entry.bytes)).small().color(t.muted));
                if !entry.used {
                    ui.label(egui::RichText::new("not used by the page").small().color(t.warn));
                }
                if !entry.kind.editable() {
                    ui.label(egui::RichText::new("read-only").small().color(t.muted));
                }
            });
            ui.separator();
            match entry.kind {
                Kind::Picture => {
                    let uri = crate::preview::image_uri(dir, &entry.name);
                    match uri {
                        Some(uri) => {
                            egui::ScrollArea::both().id_salt("code-picture").max_height(height - 40.0).show(ui, |ui| {
                                ui.add(egui::Image::new(uri).max_width(ui.available_width()).fit_to_original_size(1.0));
                            });
                        }
                        None => {
                            ui.label(egui::RichText::new("This picture cannot be shown here.").color(t.muted));
                        }
                    }
                }
                Kind::Other => {
                    ui.label(egui::RichText::new("Nothing here can show this kind of file.").color(t.muted));
                    ui.label(egui::RichText::new("It is listed so the page's folder is not a secret.").small().color(t.muted));
                }
                // A drawing is drawn, with its code one click away: a wall of numbers
                // says nothing about what is on the page.
                Kind::Ink if !state.as_code => {
                    ui.horizontal(|ui| {
                        if ui.small_button("Show the code").clicked() {
                            state.as_code = true;
                        }
                        let strokes = read_for_view(&dir.join(&entry.name)).0.matches("<path").count();
                        ui.label(egui::RichText::new(format!("{strokes} stroke{}", if strokes == 1 { "" } else { "s" })).small().color(t.muted));
                    });
                    let room = egui::vec2(ui.available_width(), (height - 70.0).max(80.0));
                    let (rect, _) = ui.allocate_exact_size(room, Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, t.editor);
                    match std::fs::read_to_string(dir.join(&entry.name)).ok().and_then(|text| notecraft_ink::svg::read(&text).ok()) {
                        Some((drawing, _)) => {
                            // Drawn to fit, so a sheet the size of a page can be seen in
                            // the room the view has.
                            let scale = (rect.width() / drawing.width.max(1.0)).min(rect.height() / drawing.height.max(1.0)).min(1.0);
                            let painter = ui.painter().with_clip_rect(rect);
                            for stroke in &drawing.strokes {
                                let scaled = scale_stroke(stroke, scale);
                                crate::inkpad::paint_stroke(&painter, rect.min, &scaled);
                            }
                        }
                        None => {
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "This drawing cannot be read",
                                egui::FontId::proportional(13.0),
                                t.muted,
                            );
                        }
                    }
                }
                Kind::PageFile | Kind::Ink | Kind::Text | Kind::Diagram => {
                    if state.loaded.as_deref() != Some(entry.name.as_str()) {
                        let (text, cut) = read_for_view(&dir.join(entry.name.replace('/', std::path::MAIN_SEPARATOR_STR)));
                        state.text = text;
                        state.loaded = Some(entry.name.clone());
                        state.dirty = false;
                        if cut {
                            state.text.push_str("\n\n... the rest of this file is not shown here.");
                        }
                    }
                    if entry.kind == Kind::Ink && ui.small_button("Show the strokes").clicked() {
                        state.as_code = false;
                    }
                    let warning = match entry.kind {
                        Kind::PageFile => Some("The whole file, head and all. The page is read again when the view closes."),
                        Kind::Ink => Some("The pen's own readings are in this file. An edit that breaks them cannot be undone by the pen."),
                        _ => None,
                    };
                    if let Some(words) = warning {
                        ui.label(egui::RichText::new(words).small().color(t.muted));
                    }
                    let lang = Lang::of_file(&entry.name);
                    let text_id = egui::Id::new(("code-text-edit", entry.name.as_str()));
                    let find_id = egui::Id::new("code-find-query");
                    // Keys for the Find bar, taken before any box sees them.
                    if ui.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::F)) {
                        state.find.open = true;
                        state.find.focus = true;
                    }
                    // Escape closes the bar from anywhere in the view, not only from its own box: a click
                    // on one of its buttons has already taken the caret out of the box.
                    if state.find.open && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                        state.find.open = false;
                        state.find.select = !state.find.matches.is_empty();
                    }
                    if state.find.open {
                        let (next, prev) = ui.input_mut(|i| {
                            let prev = i.consume_key(Modifiers::SHIFT, Key::F3);
                            (i.consume_key(Modifiers::NONE, Key::F3), prev)
                        });
                        if next && !state.find.matches.is_empty() {
                            state.find.current = (state.find.current + 1) % state.find.matches.len();
                            state.find.jump = true;
                        }
                        if prev && !state.find.matches.is_empty() {
                            state.find.current = (state.find.current + state.find.matches.len() - 1) % state.find.matches.len();
                            state.find.jump = true;
                        }
                    }
                    // Colours and matches are found again only when the text, the format or the query changed.
                    let colours_for = fingerprint(&[&state.text, lang.label()], false);
                    if state.coloured_for != colours_for {
                        state.coloured = highlight::spans(lang, &state.text);
                        state.coloured_for = colours_for;
                    }
                    let found_in = fingerprint(&[&state.text, &state.find.query], state.find.case);
                    if state.find.open && state.find.found_in != found_in {
                        let before = state.find.matches.get(state.find.current).map(|m| m.0);
                        state.find.matches = find_matches(&state.text, &state.find.query, state.find.case);
                        state.find.found_in = found_in;
                        // Stay on the match that was current if it is still there; else the first at or after the caret.
                        let caret =
                            egui::TextEdit::load_state(ui.ctx(), text_id).and_then(|s| s.cursor.char_range()).map_or(0, |r| r.primary.index.0);
                        state.find.current = before
                            .and_then(|b| state.find.matches.iter().position(|m| m.0 == b))
                            .or_else(|| state.find.matches.iter().position(|m| m.0 >= caret))
                            .unwrap_or(0);
                        if !state.find.query.is_empty() {
                            state.find.jump = true;
                        }
                    }
                    let bar_room = if state.find.open { 34.0 } else { 0.0 };
                    let font = egui::TextStyle::Monospace.resolve(ui.style());
                    let base = ui.visuals().text_color();
                    let shown: Vec<(usize, usize)> = if state.find.open { state.find.matches.clone() } else { Vec::new() };
                    let current = (state.find.open && !shown.is_empty()).then_some(state.find.current);
                    let spans = &state.coloured;
                    let mut layouter = |ui: &Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                        ui.fonts_mut(|f| f.layout_job(colour_job(text.as_str(), spans, &shown, current, t, font.clone(), base, wrap)))
                    };
                    let jump = std::mem::take(&mut state.find.jump);
                    let select = std::mem::take(&mut state.find.select);
                    let target = state.find.matches.get(state.find.current).copied();
                    let editable = entry.kind.editable();
                    egui::ScrollArea::vertical().id_salt("code-text").max_height((ui.available_height() - 6.0 - bar_room).max(120.0)).show(
                        ui,
                        |ui| {
                            // Read only is a text box that cannot be typed in, not a disabled one: a disabled
                            // box fades every colour, which is the opposite of what colouring a file is for.
                            let edit = egui::TextEdit::multiline(&mut state.text)
                                .id(text_id)
                                .code_editor()
                                .interactive(editable)
                                .layouter(&mut layouter)
                                .desired_width(f32::INFINITY)
                                .desired_rows(26)
                                .show(ui);
                            if edit.response.changed() {
                                state.dirty = true;
                                out.wrote = Some((entry.name.clone(), state.text.clone()));
                            }
                            if let Some((from, to)) = target {
                                if jump {
                                    let r = edit.galley.pos_from_cursor(CCursor::new(from)).translate(edit.galley_pos.to_vec2());
                                    ui.scroll_to_rect(r.expand2(vec2(0.0, 24.0)), Some(egui::Align::Center));
                                }
                                if select {
                                    let mut st = egui::TextEdit::load_state(ui.ctx(), text_id).unwrap_or_default();
                                    st.cursor.set_char_range(Some(CCursorRange::two(CCursor::new(from), CCursor::new(to))));
                                    st.store(ui.ctx(), text_id);
                                    ui.memory_mut(|m| m.request_focus(text_id));
                                }
                            }
                        },
                    );
                    if state.find.open {
                        match find_bar(ui, &mut state.find, find_id, t) {
                            FindAct::Next if !state.find.matches.is_empty() => {
                                state.find.current = (state.find.current + 1) % state.find.matches.len();
                                state.find.jump = true;
                            }
                            FindAct::Previous if !state.find.matches.is_empty() => {
                                state.find.current = (state.find.current + state.find.matches.len() - 1) % state.find.matches.len();
                                state.find.jump = true;
                            }
                            FindAct::Close => {
                                state.find.open = false;
                                state.find.select = !state.find.matches.is_empty();
                            }
                            FindAct::Edited => state.find.found_in = 0,
                            _ => {}
                        }
                    }
                }
            }
        });
    });
    out
}

/// The same stroke at a different size, for drawing a whole sheet in a small space.
fn scale_stroke(stroke: &notecraft_ink::Stroke, scale: f32) -> notecraft_ink::Stroke {
    let mut out = notecraft_ink::Stroke::with_gain(stroke.tool, stroke.colour, (stroke.width * scale).max(0.4), stroke.gain);
    for dab in &stroke.dabs {
        out.add(notecraft_ink::Dab { x: dab.x * scale, y: dab.y * scale, pressure: dab.pressure });
    }
    out
}

/// One row of the list: the name, and what the file is.
fn row(ui: &mut Ui, entry: &Entry, picked: bool, t: &Tokens) -> egui::Response {
    let height = 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    if picked || resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, if picked { t.row_selected } else { t.row_hover });
    }
    let colour = if entry.used { t.text } else { t.muted };
    let at = egui::pos2(rect.min.x + 6.0, rect.center().y);
    ui.painter().text(at, egui::Align2::LEFT_CENTER, &entry.name, egui::FontId::monospace(12.0), colour);
    let right = egui::pos2(rect.max.x - 6.0, rect.center().y);
    ui.painter().text(right, egui::Align2::RIGHT_CENTER, size_text(entry.bytes), egui::FontId::proportional(10.0), t.muted);
    let enabled = ui.is_enabled();
    let label = format!("{} ({})", entry.name, size_text(entry.bytes));
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, enabled, picked, label.as_str()));
    resp.on_hover_text(if entry.used { entry.name.clone() } else { format!("{}: not used by the page", entry.name) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_words_not_bytes() {
        assert_eq!(size_text(0), "0 B");
        assert_eq!(size_text(999), "999 B");
        assert_eq!(size_text(2048), "2 KB");
        assert_eq!(size_text(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn the_page_file_comes_first_and_an_unnamed_file_is_an_orphan() {
        let t = notecraft_store::testing::TempDir::new("codeview-files");
        let dir = t.path().join("page");
        std::fs::create_dir_all(dir.join("assets")).expect("folders");
        std::fs::write(dir.join("index.html"), "<html></html>").expect("page file");
        std::fs::write(dir.join("_node.toml"), "title = \"P\"").expect("node");
        std::fs::write(dir.join("ink.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"></svg>")
            .expect("ink");
        std::fs::write(dir.join("assets").join("koala.png"), [0u8; 10]).expect("picture");

        let named = vec!["ink.svg".to_string()];
        let rows = files(&dir, "index.html", &named);
        let names: Vec<&str> = rows.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["index.html", "_node.toml", "ink.svg", "assets/koala.png"], "the page file first, then beside it, then assets");

        let by = |name: &str| rows.iter().find(|e| e.name == name).cloned().expect("row");
        assert_eq!(by("index.html").kind, Kind::PageFile);
        assert_eq!(by("ink.svg").kind, Kind::Ink, "ours by its namespace");
        assert_eq!(by("assets/koala.png").kind, Kind::Picture);
        assert_eq!(by("_node.toml").kind, Kind::Text);
        assert!(by("ink.svg").used, "the page names the drawing");
        assert!(!by("assets/koala.png").used, "nothing names the picture: an orphan");
        assert!(by("index.html").used, "the page file is never an orphan");

        assert!(Kind::PageFile.editable() && Kind::Ink.editable(), "ours can be typed in");
        assert!(!Kind::Picture.editable() && !Kind::Text.editable() && !Kind::Other.editable());
    }

    use crate::theme::Theme;

    /// The text one section of a job covers.
    fn piece<'a>(job: &'a LayoutJob, s: &egui::text::LayoutSection) -> &'a str {
        &job.text[s.byte_range.start.0..s.byte_range.end.0]
    }

    #[test]
    fn matches_are_found_by_character_position_in_order_and_do_not_overlap() {
        assert_eq!(find_matches("one two one", "one", false), vec![(0, 3), (8, 11)]);
        assert_eq!(find_matches("aaaa", "aa", false), vec![(0, 2), (2, 4)], "not overlapping");
        assert_eq!(find_matches("abc", "", false), Vec::<(usize, usize)>::new(), "an empty query finds nothing");
        assert_eq!(find_matches("ab", "abc", false), Vec::<(usize, usize)>::new());
        // Positions are characters, which is how the text box counts: 'é' is one.
        assert_eq!(find_matches("é-é", "é", false), vec![(0, 1), (2, 3)]);
        assert_eq!(find_matches("日本語 日本語", "日本", false), vec![(0, 2), (4, 6)]);
    }

    #[test]
    fn case_matters_only_when_asked() {
        assert_eq!(find_matches("Diagram diagram DIAGRAM", "diagram", false).len(), 3);
        assert_eq!(find_matches("Diagram diagram DIAGRAM", "diagram", true), vec![(8, 15)]);
        assert_eq!(find_matches("ÉCOLE école", "école", false).len(), 2, "accented letters fold too");
    }

    #[test]
    fn a_search_for_one_letter_in_a_big_file_is_cut_short() {
        let text = "a".repeat(MAX_MATCHES * 3);
        assert_eq!(find_matches(&text, "a", false).len(), MAX_MATCHES);
    }

    #[test]
    fn byte_ranges_follow_the_characters_across_multibyte_letters() {
        let text = "añb日c";
        // characters: a=0 ñ=1 b=2 日=3 c=4
        assert_eq!(byte_ranges(text, &[(0, 1), (1, 2), (3, 4)]), vec![(0, 1), (1, 3), (4, 7)]);
        for (a, b) in byte_ranges(text, &[(0, 5), (2, 3)]) {
            assert!(text.is_char_boundary(a) && text.is_char_boundary(b), "{a} {b}");
        }
        assert_eq!(byte_ranges(text, &[(0, 5)]), vec![(0, text.len())]);
    }

    #[test]
    fn the_coloured_text_is_the_same_text_and_marks_the_matches() {
        let t = Theme::DEFAULT.tokens();
        let text = "<p class=\"nc\">héllo hé</p>";
        let spans = highlight::spans(Lang::Html, text);
        let found = find_matches(text, "hé", false);
        let job = colour_job(text, &spans, &found, Some(1), &t, egui::FontId::monospace(13.0), Color32::BLACK, 500.0);
        assert_eq!(job.text, text, "nothing is added or lost by colouring");
        let (all, here) = match_colours(&t);
        let marked: Vec<(&str, Color32)> =
            job.sections.iter().filter(|s| s.format.background != Color32::TRANSPARENT).map(|s| (piece(&job, s), s.format.background)).collect();
        assert_eq!(marked, vec![("hé", all), ("hé", here)], "two matches, the second the current one");
        assert!(job.sections.iter().any(|s| piece(&job, s) == "class" && s.format.color == class_colour(Class::Attr, &t)));
    }

    #[test]
    fn colouring_plain_text_and_empty_text_makes_a_valid_job() {
        let t = Theme::DEFAULT.tokens();
        let job = colour_job("", &[], &[], None, &t, egui::FontId::monospace(13.0), Color32::BLACK, 100.0);
        assert!(job.text.is_empty());
        let job = colour_job("plain words", &[], &[], None, &t, egui::FontId::monospace(13.0), Color32::BLACK, 100.0);
        assert_eq!(job.text, "plain words");
    }

    #[test]
    fn every_syntax_colour_and_match_background_reads_in_every_theme() {
        use crate::theme::contrast;
        let classes = [
            Class::Tag,
            Class::Attr,
            Class::Str,
            Class::Comment,
            Class::Number,
            Class::Keyword,
            Class::Punct,
            Class::Heading,
            Class::Emphasis,
            Class::Code,
            Class::Link,
            Class::Meta,
        ];
        for theme in Theme::all() {
            let t = theme.tokens();
            for c in classes {
                let got = contrast(class_colour(c, &t), t.editor);
                assert!(got >= 3.0, "{}: {c:?} on the editor is {got:.2}", theme.label());
            }
            let (all, here) = match_colours(&t);
            for (name, bg) in [("a match", all), ("the current match", here)] {
                assert!(contrast(t.text, bg) >= 4.5, "{}: the text on {name} is {:.2}", theme.label(), contrast(t.text, bg));
            }
        }
    }
}

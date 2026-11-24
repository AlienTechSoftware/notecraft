//! The page preview: blocks from either format, drawn with egui.
//!
//! Bold is shown in a stronger colour because the default egui fonts have no bold
//! face (adding one means adding a font file and its licence). A click on a link is
//! reported by [`show`] for the window to follow. Equations and Mermaid diagrams are drawn;
//! a diagram that cannot be drawn shows its source and says why.

use crate::theme::Tokens;
use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, RichText, Stroke, Ui, vec2};
use notecraft_formats::html;
use notecraft_formats::markdown::{self, Block, Span};
use notecraft_model::Dir;
use notecraft_model::PageFormat;
use notecraft_model::justify::{Align, Justified, Mark};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Blocks to draw for the editable part of a page in `format`.
pub fn blocks_for(format: PageFormat, body: &str) -> Vec<Block> {
    match format {
        PageFormat::Html => markdown::parse(&html::to_markdown(body)),
        PageFormat::Cmgfm => markdown::parse(body),
    }
}

/// A `file://` address egui's file loader can read, for an attachment link like
/// `assets/pic.png` relative to the page folder. Only links under `assets/` are
/// resolved; anything else (a web address, a path that climbs out) gives `None`.
pub fn image_uri(page_dir: &Path, src: &str) -> Option<String> {
    let full = notecraft_store::tree::display_path(&page_asset_path(page_dir, src)?);
    Some(file_uri(&full))
}

/// Resolve an owned image reference the same way for drawing and image actions.
pub(crate) fn page_asset_path(page_dir: &Path, src: &str) -> Option<PathBuf> {
    let rel = src.strip_prefix("assets/")?;
    let mut path = page_dir.join("assets");
    for part in rel.split('/') {
        let part = decode_asset_component(part);
        // Decode within each component, never as part of parsing the path.
        // Only normal components can be appended below the assets directory.
        if part.trim_matches([' ', '.']).is_empty() || part.contains(['/', '\\', ':', '\0']) {
            return None;
        }
        path.push(part.as_ref());
    }
    Some(path)
}

fn decode_asset_component(raw: &str) -> Cow<'_, str> {
    if !raw.as_bytes().contains(&b'%') {
        return Cow::Borrowed(raw);
    }
    let hex = |b| char::from(b).to_digit(16).map(|n| n as u8);
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if let [b'%', hi, lo, ..] = &bytes[i..]
            && let (Some(hi), Some(lo)) = (hex(*hi), hex(*lo))
        {
            decoded.push(hi * 16 + lo);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).map_or(Cow::Borrowed(raw), Cow::Owned)
}

/// Load attachment bytes directly. This is deliberately used by the live page renderer:
/// notebooks in the Windows VM are exposed through a UNC path, and the generic file loader
/// cannot reliably open the resulting `file:////server/share/...` URI even when the file is
/// present and valid.
///
/// The bytes are kept in memory and the file is read again only when its size or time
/// changes: the page is drawn many times a second, and reading every picture each time made
/// a page of printouts crawl, worst on a network or cloud folder. The name egui keys its
/// picture by carries the file's full path and that size and time, so two pages' `a.png` stay
/// apart and a replaced picture is drawn anew.
/// A picture's last change, where the platform reports one. The browser build's file shim has no
/// modification time, so there the stamp is the size alone.
/// The number column of a numbered code block: each line's number and a bar after it, one line per
/// line of code. The bar is the separator the owner asked for (SUP-004).
pub(crate) fn line_numbers(lines: usize) -> String {
    (1..=lines).map(|n| format!("{n} |")).collect::<Vec<_>>().join("\n")
}

/// A page's picture, as far as the window has it.
#[derive(Debug)]
enum Picture {
    /// Read: the name egui has it under, and its bytes.
    Ready(String, egui::load::Bytes),
    /// Being read. A picture in a synced folder that is not on this computer yet is downloaded
    /// as it is read, which can take seconds; the page goes on without it meanwhile.
    Loading,
    /// Not there, or not readable.
    Missing,
}

/// A file's size and time, which say whether what was read of it is still what it holds.
type Stamp = (u64, Option<std::time::SystemTime>);

/// The pictures of the pages shown so far, kept by file, and the ones on their way.
#[derive(Default)]
struct Pictures {
    /// What was read of each file: its stamp then, the name egui has it under, and its bytes.
    kept: std::collections::HashMap<std::path::PathBuf, (Stamp, String, std::sync::Arc<[u8]>)>,
    /// Asked for and not read yet.
    reading: std::collections::HashSet<std::path::PathBuf>,
    /// Could not be read, at the stamp the file had: not asked for again until it changes.
    failed: std::collections::HashMap<std::path::PathBuf, Stamp>,
    /// The frame the waiting below was counted for, and how much of [`WAIT_PER_FRAME`] it has
    /// left.
    waited: (Option<u64>, std::time::Duration),
    /// How many times a picture was read from disk, for tests.
    #[cfg(test)]
    reads: usize,
}

/// How long a frame waits, all its pictures together, for pictures being read. A picture on
/// this computer's own disk is read well within it and drawn in the same frame, as before; one
/// still being downloaded is not, and is drawn when it comes instead of holding up the window.
#[cfg(not(test))]
const WAIT_PER_FRAME: std::time::Duration = std::time::Duration::from_millis(12);
/// Tests draw many windows at once on a busy machine, and a picture read from their own disk
/// must still be there in the frame that asked for it.
#[cfg(test)]
const WAIT_PER_FRAME: std::time::Duration = std::time::Duration::from_secs(1);

/// How long a picture reader with nothing to read waits for more before it ends.
#[cfg(not(target_arch = "wasm32"))]
const READER_IDLE: std::time::Duration = std::time::Duration::from_secs(30);

type PageImages = std::sync::Arc<std::sync::Mutex<Pictures>>;

fn page_images(ctx: &egui::Context) -> PageImages {
    ctx.data_mut(|d| d.get_temp_mut_or_default::<PageImages>(egui::Id::new("notecraft-page-images")).clone())
}

/// How many times this window has read a picture from disk.
#[cfg(test)]
fn picture_reads(ctx: &egui::Context) -> usize {
    page_images(ctx).lock().map_or(0, |p| p.reads)
}

/// The picture `src` of the page in `page_dir`, without waiting for it. Asked every frame for
/// every picture drawn; a picture not read yet is read on the pictures' own thread, one at a
/// time in the order they are asked for, so the top of a page comes first, and the window is
/// drawn again when it is in.
fn picture(ctx: &egui::Context, page_dir: &Path, src: &str) -> Picture {
    let Some(path) = page_asset_path(page_dir, src) else { return Picture::Missing };
    // Asked every frame for every picture drawn: kept a moment, so a page of printouts on a
    // synced folder does not ask its mount hundreds of times a frame.
    let seen = crate::fscache::seen(&path);
    if !seen.exists {
        return Picture::Missing;
    }
    let stamp = (seen.len, seen.modified);
    let store = page_images(ctx);
    let Ok(mut pictures) = store.lock() else { return Picture::Missing };
    if let Some((had, uri, bytes)) = pictures.kept.get(&path)
        && *had == stamp
    {
        return Picture::Ready(uri.clone(), egui::load::Bytes::Shared(bytes.clone()));
    }
    if pictures.failed.get(&path) == Some(&stamp) {
        return Picture::Missing;
    }
    if !pictures.reading.insert(path.clone()) {
        return Picture::Loading;
    }
    // Per frame, not per pass: a frame drawn twice (a zoom drawn again about the pointer) waits
    // once.
    let frame = ctx.cumulative_frame_nr();
    // Counted afresh in each frame, the window's first one too.
    if pictures.waited.0 != Some(frame) {
        pictures.waited = (Some(frame), WAIT_PER_FRAME);
    }
    let budget = pictures.waited.1;
    drop(pictures);
    let (done, read) = std::sync::mpsc::channel();
    let job = PictureRead { path, stamp, store: store.clone(), ctx: ctx.clone(), done };
    match send_to_reader(job) {
        Ok(()) => {
            // Waited for, a moment: on a local disk it is here before the frame goes on.
            let started = std::time::Instant::now();
            let came = !budget.is_zero() && read.recv_timeout(budget).is_ok();
            if let Ok(mut pictures) = store.lock() {
                pictures.waited.1 = budget.saturating_sub(started.elapsed());
            }
            if came { picture(ctx, page_dir, src) } else { Picture::Loading }
        }
        // No thread to read on (the web build has none): read here, as before. The picture is
        // then kept, or marked as not readable, so asking again answers at once.
        Err(job) => {
            job.run();
            picture(ctx, page_dir, src)
        }
    }
}

/// One picture to read, and where to put it.
struct PictureRead {
    path: std::path::PathBuf,
    stamp: Stamp,
    store: PageImages,
    ctx: egui::Context,
    /// Told when the read is done, for a frame still waiting for it.
    done: std::sync::mpsc::Sender<()>,
}

impl PictureRead {
    fn run(self) {
        // Read and copied into shared bytes before the store is locked: the window waits on that
        // lock, and copying a large picture took the time it is not to spend waiting.
        let read = std::fs::read(&self.path).map(std::sync::Arc::<[u8]>::from);
        let mut evicted = std::collections::HashMap::new();
        let Ok(mut pictures) = self.store.lock() else { return };
        pictures.reading.remove(&self.path);
        #[cfg(test)]
        {
            pictures.reads += 1;
        }
        match read {
            Ok(bytes) => {
                let nanos = self.stamp.1.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
                let uri = format!("bytes://page-image/{}#{}-{nanos}", self.path.display(), self.stamp.0);
                // Pictures of pages no longer shown would pile up; past a generous size the store
                // starts over.
                let held: usize = pictures.kept.values().map(|(_, _, b)| b.len()).sum();
                if held + bytes.len() > PAGE_IMAGES_MAX_BYTES {
                    // Let go of after the lock: freeing them is not the window's to wait for.
                    evicted = std::mem::take(&mut pictures.kept);
                }
                pictures.failed.remove(&self.path);
                pictures.kept.insert(self.path, (self.stamp, uri, bytes));
            }
            Err(e) => {
                log::warn!("picture not read: {:?}: {e}", self.path);
                pictures.failed.insert(self.path, self.stamp);
            }
        }
        drop(pictures);
        drop(evicted);
        // Told to a frame still waiting, and drawn again in any case: the frame may have stopped
        // waiting just as the read came in.
        let _ = self.done.send(());
        self.ctx.request_repaint();
    }
}

/// Hands a read to the pictures' thread, started the first time it is needed. Gives the read
/// back when there is no such thread.
fn send_to_reader(job: PictureRead) -> Result<(), PictureRead> {
    #[cfg(target_arch = "wasm32")]
    {
        Err(job)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        static READERS: std::sync::OnceLock<Readers> = std::sync::OnceLock::new();
        READERS.get_or_init(Readers::new).send(job)
    }
}

/// The threads pictures are read on, and the reads waiting for one.
#[cfg(not(target_arch = "wasm32"))]
struct Readers {
    queue: std::sync::mpsc::Sender<PictureRead>,
    waiting: std::sync::Arc<std::sync::Mutex<std::sync::mpsc::Receiver<PictureRead>>>,
    /// Readers with nothing to read.
    idle: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Reads queued and not taken by a reader yet.
    queued: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    /// Readers running, busy or not.
    live: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Readers {
    fn new() -> Self {
        let (queue, waiting) = std::sync::mpsc::channel();
        Readers {
            queue,
            waiting: std::sync::Arc::new(std::sync::Mutex::new(waiting)),
            idle: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            queued: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            live: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    /// Queues a read for a reader, starting one first when more reads wait than readers are free
    /// to take them. There is no cap: a reader stuck on a hung mount keeps its thread, but every
    /// other picture gets one of its own, and a reader left with nothing to do ends after
    /// [`READER_IDLE`]. A picture is asked for once while it is being read, so there are never
    /// more readers than pictures waiting. When no reader can be started, the read is given back
    /// to be made in place, so it is not left queued with nobody to take it.
    ///
    /// Counting only free readers was not enough: reads queued one after another all saw the same
    /// free reader before it took the first, and waited behind it if it stuck.
    fn send(&self, job: PictureRead) -> Result<(), PictureRead> {
        use std::sync::atomic::Ordering;
        // Counted before it is queued: a free reader may take it, and count it off, at once.
        let waiting_reads = self.queued.fetch_add(1, Ordering::SeqCst) + 1;
        // No reader could be started: a reader running, busy or not, comes to the queue in time,
        // so the read waits for it; with none running at all it is read in place.
        if waiting_reads > self.idle.load(Ordering::SeqCst) && !self.start_reader() && self.live.load(Ordering::SeqCst) == 0 {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(job);
        }
        if let Err(e) = self.queue.send(job) {
            self.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(e.0);
        }
        Ok(())
    }

    /// Starts a reader. False when the system has no thread to give.
    fn start_reader(&self) -> bool {
        use std::sync::atomic::Ordering;
        let (waiting, idle, queued, live) = (self.waiting.clone(), self.idle.clone(), self.queued.clone(), self.live.clone());
        let started = std::thread::Builder::new().name("notecraft-pictures".into()).spawn(move || {
            // Counted as running once it runs, not when it was asked for: a reader still starting,
            // whose start may yet fail, is no reason to leave a read queued for it.
            live.fetch_add(1, Ordering::SeqCst);
            loop {
                idle.fetch_add(1, Ordering::SeqCst);
                // Idle from now, not from when the queue is free: readers waiting their turn for
                // the queue do not each wait the whole time again once they have it.
                let until = std::time::Instant::now() + READER_IDLE;
                // The queue is held only to take the next read, not while reading it.
                let next = waiting.lock().ok().map(|q| q.recv_timeout(until.saturating_duration_since(std::time::Instant::now())));
                idle.fetch_sub(1, Ordering::SeqCst);
                match next {
                    Some(Ok(job)) => {
                        queued.fetch_sub(1, Ordering::SeqCst);
                        job.run();
                    }
                    // Nothing to read for a while: the reader ends, unless a read was queued as it
                    // gave up waiting, counting on it; then it stays for that read.
                    Some(Err(std::sync::mpsc::RecvTimeoutError::Timeout)) => {
                        if queued.load(Ordering::SeqCst) > 0 {
                            continue;
                        }
                        live.fetch_sub(1, Ordering::SeqCst);
                        if queued.load(Ordering::SeqCst) > 0 {
                            live.fetch_add(1, Ordering::SeqCst);
                            continue;
                        }
                        break;
                    }
                    // The queue is gone: nothing more will come.
                    _ => {
                        live.fetch_sub(1, Ordering::SeqCst);
                        break;
                    }
                }
            }
        });
        started.is_ok()
    }
}

/// How much picture data [`picture`] keeps before it starts over.
const PAGE_IMAGES_MAX_BYTES: usize = 512 * 1024 * 1024;

/// `file:///C:/dir/a%20b.png` or `file:///home/me/a%20b.png`: the form egui's file
/// loader reads on every platform (three slashes, spaces and symbols percent-encoded).
fn file_uri(path: &str) -> String {
    let p = path.replace('\\', "/");
    let p = if p.starts_with('/') { p } else { format!("/{p}") };
    let mut out = String::from("file://");
    for b in p.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~/:".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The same formats [`job`] uses, as ranges into the line's own text, for the bidi painter.
fn pieces(ctx: &egui::Context, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens) -> Vec<crate::bidi::Piece> {
    let mut out = Vec::new();
    let mut at = 0usize;
    for s in spans {
        if s.image.is_some() {
            continue;
        }
        let text = if s.math { format!("${}$", s.text) } else { s.text.clone() };
        let end = at + text.len();
        out.push((at..end, format_with_family(ctx, s, size, color, tokens)));
        at = end;
    }
    out
}

fn format_with_family(ctx: &egui::Context, span: &Span, size: f32, color: egui::Color32, tokens: &Tokens) -> TextFormat {
    let mut format = format_of(span, size, color, tokens);
    if !span.code
        && !span.math
        && let Some(name) = span.font_family.as_deref()
    {
        let family = crate::fonts::named_family(name);
        if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family)) {
            format.font_id.family = family;
        }
    }
    format
}

/// How one span is drawn.
fn format_of(s: &Span, size: f32, color: egui::Color32, tokens: &Tokens) -> TextFormat {
    let mono = s.code || s.math;
    let size = s.font_size.map_or(size, |points| size * f32::from(points) / 11.0);
    let size = if s.script.is_some() { size * 0.7 } else { size };
    let c = if let Some(chosen) = s.colour.as_deref().and_then(|hex| crate::textcolour::shown_text(hex, tokens.editor, tokens.text)) {
        // A colour the writer chose, kept readable on this theme (#260).
        chosen
    } else if s.link.is_some() {
        tokens.link
    } else if s.bold {
        tokens.strong
    } else {
        color
    };
    let marked = s.highlight.as_deref().and_then(|hex| crate::textcolour::shown_highlight(hex, tokens));
    TextFormat {
        font_id: if mono { FontId::monospace(size * 0.92) } else { FontId::proportional(size) },
        color: c,
        italics: s.italic || s.math,
        underline: if s.link.is_some() || s.underline { Stroke::new(1.0, c) } else { Stroke::NONE },
        strikethrough: if s.strike { Stroke::new(1.0, c) } else { Stroke::NONE },
        valign: if s.script == Some(notecraft_formats::markdown::Script::Superscript) { egui::Align::TOP } else { egui::Align::BOTTOM },
        background: if s.code { tokens.code_bg } else { marked.unwrap_or(egui::Color32::TRANSPARENT) },
        ..TextFormat::default()
    }
}

pub fn job(ui: &Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens, page: Option<Dir>) -> LayoutJob {
    let mut job = LayoutJob { halign: crate::pagetext::halign(&text_of(spans), page), ..LayoutJob::default() };
    job.wrap.max_width = ui.available_width();
    for s in spans {
        if s.image.is_some() {
            continue;
        }
        let format = format_with_family(ui.ctx(), s, size, color, tokens);
        let text = if s.math { format!("${}$", s.text) } else { s.text.clone() };
        job.append(&text, 0.0, format);
    }
    job
}

/// The editor's own job: the same runs [`job`] draws, but emitting exactly the caret's
/// visible text — an inline image as one object character and maths without its dollars —
/// so the galley the caret measures is the text it edits.
pub fn edit_job(ui: &Ui, spans: &[Span], size: f32, color: egui::Color32, tokens: &Tokens, page: Option<Dir>, width: f32) -> LayoutJob {
    let mut job = LayoutJob { halign: crate::pagetext::halign(&edit_text_of(spans), page), ..LayoutJob::default() };
    job.wrap.max_width = width;
    for s in spans {
        let format = format_with_family(ui.ctx(), s, size, color, tokens);
        if s.image.is_some() {
            job.append("\u{fffc}", 0.0, TextFormat::default());
        } else {
            job.append(&s.text, 0.0, format);
        }
    }
    job
}

fn edit_text_of(spans: &[Span]) -> String {
    spans.iter().map(|s| if s.image.is_some() { "\u{fffc}".to_string() } else { s.text.clone() }).collect()
}

/// The rectangles to lay behind a selection inside a galley egui laid out: one to a row,
/// so a paragraph wrapped over three lines gets three bands and not one tall box.
///
/// `a` and `b` are bytes of the galley's own text. A row that the selection does not touch
/// gives nothing; a row it covers end to end gives the whole row.
fn selection_bands(galley: &egui::Galley, a: usize, b: usize) -> Vec<egui::Rect> {
    let text = galley.text();
    // epaint counts in characters, the page counts in bytes.
    let chars_to = |at: usize| text.get(..at.min(text.len())).map(|t| t.chars().count()).unwrap_or(0);
    let (a, b) = (chars_to(a.min(b)), chars_to(a.max(b)));
    if a == b {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut before = 0usize;
    for placed in &galley.rows {
        let mut covered: Option<(f32, f32)> = None;
        for (n, glyph) in placed.row.glyphs.iter().enumerate() {
            let at = before + n;
            if at < a || at >= b {
                continue;
            }
            let (x0, x1) = (glyph.pos.x, glyph.pos.x + glyph.advance_width);
            covered = Some(match covered {
                Some((l, r)) => (l.min(x0), r.max(x1)),
                None => (x0, x1),
            });
        }
        // A row that ends a paragraph swallowed its newline, which still counts as a
        // character of the text; otherwise the next row would start one short.
        before += placed.char_count_including_newline().0;
        if let Some((left, right)) = covered {
            let top = placed.pos.y;
            out.push(egui::Rect::from_min_max(
                egui::pos2(placed.pos.x + left, top),
                egui::pos2(placed.pos.x + right.max(left + 1.0), top + placed.row.size.y),
            ));
        }
    }
    out
}

/// Which way a block's words read, given the direction of the page they are on.
pub fn dir_of(spans: &[Span], page: Option<Dir>) -> Dir {
    notecraft_model::direction_in(&text_of(spans), page)
}

/// Draws spans as wrapped text. A paragraph with inline math flows text and formulas
/// together; one without math is a single label. Right-to-left text hangs from the right
/// edge, which is where a reader of Arabic or Hebrew starts.
///
/// With `justify`, every line of the paragraph but the last is made to reach the column.
/// Which way that is done follows the words. A paragraph that reads right to left is
/// stretched by kashida, the way a scribe lengthens the joins inside the words, so the
/// spaces stay as they were and the page keeps an even colour. A paragraph that reads left
/// to right has no such joins, so it is justified the Latin way, by widening the spaces,
/// which is what egui's own layout does.
#[allow(clippy::too_many_arguments)]
fn draw_spans(
    ui: &mut Ui,
    spans: &[Span],
    size: f32,
    color: egui::Color32,
    tokens: &Tokens,
    page: Option<Dir>,
    mark: Option<Mark>,
    // The bytes of this block's words that are selected, when a selection reaches into it
    // from a drag that crossed more than one paragraph.
    selected: Option<(usize, usize)>,
    // The least height of a line (issue #275), already scaled to the zoom; 0 asks for nothing.
    floor: f32,
) {
    let justify = mark.and_then(Mark::strength);
    let align = mark.and_then(Mark::align);
    if !spans.iter().any(|s| s.math) {
        // A line that mixes the two directions is drawn by egui in the wrong order, and a
        // paragraph that reads right to left is not wrapped by it, so both go through the
        // bidi painter: parley says where each run belongs, egui still draws them, and the
        // marks inside the words are kept.
        let text = text_of(spans);
        if crate::bidi::paints(&text, page) {
            let pieces = pieces(ui.ctx(), spans, size, color, tokens);
            let width = ui.available_width();
            // Kashida is for the joins of Arabic and Persian letters. A line that mixes the
            // two directions and reads left to right has nothing to stretch, and the bidi
            // painter has no way to widen its spaces, so it is drawn ragged; see
            // `docs/gaps.md`.
            let stretch = justify.filter(|_| dir_of(spans, page).is_rtl());
            let sel = selected.map(|range| crate::bidi::Selected { range, color: tokens.selection });
            let _floor = crate::bidi::floor(floor);
            let rect = crate::bidi::show(ui, &text, &pieces, size, width, page, stretch, align, sel);
            let _ = rect;
            return;
        }
        let mut job = job(ui, spans, size, color, tokens, page);
        crate::pagetext::floor_lines(ui.ctx(), &mut job, floor);
        // Latin has no joins to stretch, so a left-to-right paragraph is justified the way
        // egui does it, by widening the spaces. The strength is about kashidas and has
        // nothing to say here.
        job.justify = justify.is_some();
        if let Some(align) = align {
            // Set against an edge or the middle (issue #258): every line of the paragraph is
            // placed in the whole width of the column, the way a centred paragraph reads.
            let h = egui_align(align);
            let Some((a, b)) = selected else {
                ui.with_layout(egui::Layout::top_down(h), |ui| ui.add(egui::Label::new(job).halign(h).wrap()));
                return;
            };
            // Something is selected: laid out here so the selection goes behind the words.
            job.halign = h;
            job.wrap.max_width = ui.available_width();
            let galley = ui.fonts_mut(|f| f.layout_job(job));
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), galley.size().y), egui::Sense::hover());
            let at = match h {
                egui::Align::Center => row.center_top(),
                egui::Align::Max => row.right_top(),
                egui::Align::Min => row.left_top(),
            };
            let resp = ui.interact(galley.rect.translate(at.to_vec2()), ui.id().with(("aligned", row.min.y as i32)), egui::Sense::hover());
            let (enabled, said) = (ui.is_enabled(), galley.text().to_string());
            resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &said));
            for band in selection_bands(&galley, a, b) {
                ui.painter().rect_filled(band.translate(at.to_vec2()), 1.0, tokens.selection);
            }
            ui.painter().galley(at, galley, color);
            return;
        }
        match selected {
            // Nothing selected here: egui's own label, which is what this has always been.
            None if !dir_of(spans, page).is_rtl() => {
                ui.label(job);
            }
            None => {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| ui.label(job));
            }
            // Something is. The galley is laid out here so the selection can go behind the
            // words rather than over them, which a label gives no way to do.
            Some((a, b)) => {
                let galley = ui.fonts_mut(|f| f.layout_job(job));
                let (rect, resp) = ui.allocate_exact_size(galley.size(), egui::Sense::hover());
                let (enabled, said) = (ui.is_enabled(), galley.text().to_string());
                resp.widget_info(move || egui::WidgetInfo::labeled(egui::WidgetType::Label, enabled, &said));
                for band in selection_bands(&galley, a, b) {
                    ui.painter().rect_filled(band.translate(rect.min.to_vec2()), 1.0, tokens.selection);
                }
                ui.painter().galley(rect.min, galley, color);
            }
        }
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let mut run: Vec<Span> = Vec::new();
        for s in spans {
            if s.math {
                if !run.is_empty() {
                    let mut text = job(ui, &run, size, color, tokens, page);
                    crate::pagetext::floor_lines(ui.ctx(), &mut text, floor);
                    ui.label(text);
                    run.clear();
                }
                crate::mathview::inline(ui, &s.text, size * 1.05, color);
                // A zero-wide strut the least line height tall, on the formula's own row (it
                // follows the formula, so a formula that wrapped takes it along): a row that holds
                // only a formula is opened up like the rows of words (issue #275).
                if floor > 0.0 {
                    ui.allocate_exact_size(egui::vec2(0.0, floor), egui::Sense::hover());
                }
            } else {
                run.push(s.clone());
            }
        }
        if !run.is_empty() {
            let mut text = job(ui, &run, size, color, tokens, page);
            crate::pagetext::floor_lines(ui.ctx(), &mut text, floor);
            ui.label(text);
        }
    });
}

/// The egui alignment of a paragraph's edge.
pub fn egui_align(align: Align) -> egui::Align {
    match align {
        Align::Left => egui::Align::LEFT,
        Align::Center => egui::Align::Center,
        Align::Right => egui::Align::RIGHT,
    }
}

/// A mark with its justification dropped: a heading is aligned, never stretched.
fn aligned_only(mark: Option<Mark>) -> Option<Mark> {
    mark.filter(|m| m.align().is_some())
}

fn text_of(spans: &[Span]) -> String {
    spans.iter().filter(|s| s.image.is_none()).map(|s| s.text.as_str()).collect()
}

/// The type size a block's words are drawn at here.
fn size_of_block(block: &Block) -> f32 {
    match block {
        Block::Heading { level, .. } => match level {
            1 => 28.0,
            2 => 22.0,
            3 => 18.0,
            _ => 16.0,
        },
        _ => 14.5,
    }
}

/// The font size for a list item marker (bullet or number), matching the body text size.
pub(crate) fn item_marker_size(spans: &[Span], zoom: f32) -> f32 {
    let base = 14.5 * zoom;
    spans.iter().filter(|s| s.image.is_none()).find_map(|s| s.font_size).map_or(base, |points| base * f32::from(points) / 11.0)
}

/// The link under a point in a block the bidi painter drew, if the point is on one. `at` is
/// measured from the block's top left corner and `width` is the block's width. `None` for a
/// block egui drew (the caller maps those through egui's own layout) and for a point on no
/// link. The lookup goes through parley, the same way the drawing did, so a click on a
/// right-to-left or mixed line lands on the word under the pointer.
pub fn link_at_point(block: &Block, at: egui::Vec2, width: f32, page: Option<Dir>) -> Option<String> {
    let spans = match block {
        Block::Heading { spans, .. } | Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Item { spans, .. } => spans,
        _ => return None,
    };
    if spans.iter().any(|s| s.math) {
        return None;
    }
    // Attachment cards are rendered as a compact linked image before the
    // label text. The text mapper deliberately skips image spans, so handle
    // the card's own 48x56 hit area before mapping the remaining words.
    if at.x >= 0.0
        && at.x <= 56.0
        && at.y >= 0.0
        && at.y <= 64.0
        && spans.iter().any(|s| s.image.is_some())
        && let Some(href) = spans.iter().find_map(|s| s.link.clone())
    {
        return Some(href);
    }
    // What is drawn: the words without a tag's marker, beside the mark or the bullet.
    let (spans, mark) = match tagged(spans) {
        Some((_, stripped)) => (stripped, 21.0),
        None => (spans.clone(), 0.0),
    };
    let text = text_of(&spans);
    if !crate::bidi::paints(&text, page) {
        return None;
    }
    let lead = mark
        + match block {
            Block::Item { depth, .. } => 8.0 + *depth as f32 * 18.0 + 22.0,
            Block::Quote(_, _) => 18.0,
            _ => 0.0,
        };
    // A right-to-left row puts its bullet or mark on the right, so the words start at the
    // left edge; a left-to-right row has them on the left.
    let rtl = dir_of(&spans, page).is_rtl();
    let x = if rtl { at.x } else { at.x - lead };
    let column = (width - lead).max(32.0);
    let byte = crate::bidi::hit(&text, egui::vec2(x, at.y), size_of_block(block), column, page);
    let mut start = 0usize;
    for s in &spans {
        if s.image.is_some() {
            continue;
        }
        let end = start + s.text.len();
        if byte >= start && byte < end {
            return s.link.clone();
        }
        start = end;
    }
    None
}

/// The box a picture is fitted into, keeping its shape.
///
/// The height must not come from `ui.available_height()`. A page scrolls, so once the
/// blocks above have filled the view there is no height left, and egui fits a picture into
/// a box of no height: it drew nothing at all, and a picture low on a long page was a gap
/// where a picture should be. Measured on 2026-10-19 on the Arabic showcase page, where the
/// same picture drew at the top of one page and vanished at the bottom of another.
fn picture_box(ui: &Ui, src: &str, zoom: f32) -> egui::Vec2 {
    // Parent columns already carry the page zoom; the picture's height is its own size.
    egui::vec2(ui.available_width(), picture_max_height(src, zoom))
}

/// How tall a picture without a size of its own may be drawn. A printout's page is a sheet of
/// paper, so it fills its box's width however tall that makes it, as a page put on the sheet
/// would; other pictures are kept to 480 points, so one photo does not fill the window.
fn picture_max_height(src: &str, zoom: f32) -> f32 {
    let name = src.rsplit(['/', '\\']).next().unwrap_or(src);
    let printout = name
        .rsplit_once(".printout-")
        .is_some_and(|(_, page)| page.strip_suffix(".png").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())));
    if printout { f32::INFINITY } else { 480.0 * zoom }
}

/// Where a picture still being read will go: a quiet box of about its size, so the page does
/// not jump far when it comes. A printout's height is not known before it is read, so its box
/// is a slide's, three quarters as tall as it is wide.
fn loading_picture(ui: &mut Ui, size: egui::Vec2) {
    let width = size.x.min(ui.available_width()).max(24.0);
    let height = if size.y.is_finite() { size.y } else { width * 0.75 };
    let (rect, _) = ui.allocate_exact_size(vec2(width, height.max(24.0)), egui::Sense::hover());
    let visuals = ui.visuals();
    ui.painter().rect_filled(rect, 4.0, visuals.faint_bg_color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        crate::tl_id!("ui.loading_picture"),
        egui::FontId::proportional(12.0),
        visuals.weak_text_color(),
    );
}

fn draw_inline_images(ui: &mut Ui, spans: &[Span], page_dir: &Path, zoom: f32, block: usize, object_rect: &mut Option<egui::Rect>) {
    for s in spans {
        if let Some(src) = &s.image {
            // A linked image is the attachment-card form used by NoteCraft. Keep its generated
            // shell icon compact; standalone images still use the picture box.
            let size = if s.link.is_some() { egui::vec2(48.0 * zoom, 56.0 * zoom) } else { picture_box(ui, src, zoom) };
            match picture(ui.ctx(), page_dir, src) {
                Picture::Loading => loading_picture(ui, size),
                Picture::Ready(uri, bytes) => {
                    let response = ui.add(egui::Image::from_bytes(uri, bytes).fit_to_exact_size(size).sense(egui::Sense::hover()));
                    outline_highlight(ui, page_dir, src, response.rect);
                    *object_rect = Some(response.rect);
                    if s.link.is_none() {
                        record_rendered_image(ui.ctx(), block, response.rect, src);
                    }
                    if response.hovered() {
                        log::debug!("inline image render hover: src={src:?} rect={:?} pointer={:?}", response.rect, ui.ctx().pointer_latest_pos());
                    }
                }
                Picture::Missing => {
                    ui.weak(format!("[image: {} ({src})]", s.text));
                }
            }
        }
    }
}

/// True for a block that is more than styled text: a picture, a table, an equation, a
/// diagram. While the page is written on as text, these are drawn below it.
pub fn is_rich(block: &Block) -> bool {
    matches!(block, Block::Image { .. } | Block::Table { .. } | Block::MathBlock(_, _) | Block::Code { .. })
        || matches!(block, Block::Paragraph(spans, _) if spans.iter().any(|s| s.math || s.image.is_some()))
}

/// Draws only the blocks [`is_rich`] picks out. They are picked out of their page, so the
/// page's justification does not follow them here; none of them is a paragraph of words.
pub fn show_rich(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Option<Dir>) {
    let rich: Vec<Block> = blocks.iter().filter(|b| is_rich(b)).cloned().collect();
    let _link = show(ui, &rich, page_dir, tokens, page, &Justified::default());
}

/// What happened in one block this frame.
pub struct BlockResponse {
    /// The area the block took, sensing clicks: this is how the caret lands in it.
    pub response: egui::Response,
    /// A to-do box the user ticked or cleared, and its new state.
    pub task_set: Option<bool>,
}

/// Draws blocks into `ui`. `page_dir` is the page's folder, used to find attachments.
/// Read only: to-do boxes are shown but not ticked (see [`show_block`]). A link is still
/// live: the target of one clicked this frame is returned, as written in the page.
pub fn show(ui: &mut Ui, blocks: &[Block], page_dir: &Path, tokens: &Tokens, page: Option<Dir>, justified: &Justified) -> Option<String> {
    show_zoomed(ui, blocks, page_dir, tokens, page, justified, 1.0)
}

/// Draws blocks at a page zoom while keeping the surrounding window unchanged.
fn line_floor_id() -> egui::Id {
    egui::Id::new("least-line-height-of-the-next-block")
}

/// Takes the least line height `make_room` left for the block about to be drawn, so it can
/// never reach a block drawn after it by some other path.
fn take_line_floor(ctx: &egui::Context) -> f32 {
    ctx.data_mut(|d| d.remove_temp::<f32>(line_floor_id())).unwrap_or(0.0)
}

/// Drops a least line height `make_room` left that no block asked for.
pub fn clear_room(ctx: &egui::Context) {
    take_line_floor(ctx);
}

/// Gets ready to draw the block at `index` of a column whose first block is `first`: leaves the
/// room the page gave it above (its own space before and the space after the block above,
/// when that one is in this column) and tells the block how tall its lines must be at least
/// (issue #275). Every loop that draws a page's blocks calls this, so editing, reading and the
/// pen view lay a paragraph out the same.
pub fn make_room(ui: &mut Ui, justified: &Justified, index: usize, first: usize, zoom: f32) {
    let own = justified.spacing_of(index).unwrap_or_default();
    let above = if index > first { justified.spacing_of(index - 1).map_or(0, |s| s.after) } else { 0 };
    let gap = f32::from(own.before) + f32::from(above);
    if gap > 0.0 {
        ui.add_space(gap * zoom);
    }
    let floor = f32::from(own.line) * zoom;
    ui.ctx().data_mut(|d| d.insert_temp(line_floor_id(), floor));
}

/// Holds the least line height of block `index` in force for the painter's hit tests and the
/// click on a link, for as long as the guard lives (issue #275), so a point is measured against
/// the lines that were drawn.
pub fn hold_floor(justified: &Justified, index: usize, zoom: f32) -> crate::bidi::Floor {
    crate::bidi::floor(f32::from(justified.spacing_of(index).unwrap_or_default().line) * zoom)
}

pub fn show_zoomed(
    ui: &mut Ui,
    blocks: &[Block],
    page_dir: &Path,
    tokens: &Tokens,
    page: Option<Dir>,
    justified: &Justified,
    zoom: f32,
) -> Option<String> {
    let mut link = None;
    for (i, block) in blocks.iter().enumerate() {
        make_room(ui, justified, i, 0, zoom);
        let _floor = hold_floor(justified, i, zoom);
        // Block headers belong to the box's frame controls, not the page content (see `pageedit`).
        let out = show_block_zoomed(ui, block, i, page_dir, tokens, false, page, justified.mark_of(i), None, ui.id().with(("block", i)), zoom);
        let resp = out.response;
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(href) = crate::pageedit::link_under(ui, block, &resp, p, tokens, page, zoom)
        {
            link = Some(href);
        } else if resp.hovered() && resp.hover_pos().and_then(|p| crate::pageedit::link_under(ui, block, &resp, p, tokens, page, zoom)).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    link
}

/// Draws one block and reports what happened in it. With `live` the to-do box can be
/// ticked, which is what the page editor passes. With `justified` the block's lines are
/// made to reach the column; the page says which blocks those are, and only a paragraph
/// listens (see `notecraft_model::justify`).
#[allow(clippy::too_many_arguments)]
pub fn show_block(
    ui: &mut Ui,
    block: &Block,
    i: usize,
    page_dir: &Path,
    tokens: &Tokens,
    live: bool,
    page: Option<Dir>,
    // How the page lays this paragraph out: justified, or set against an edge (issue #258).
    justified: Option<Mark>,
    // What of this block's words a selection reaching across paragraphs covers, in bytes.
    selected: Option<(usize, usize)>,
    // The id the block senses under. The page editor passes the one its text box uses for
    // this block, so a drag that begins on the finished block carries straight on into the
    // text box when the caret lands in it on the next frame; egui tracks a drag against the
    // id that started it, and a different id here would drop the drag on the floor.
    id: egui::Id,
) -> BlockResponse {
    show_block_zoomed(ui, block, i, page_dir, tokens, live, page, justified, selected, id, 1.0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn show_block_zoomed(
    ui: &mut Ui,
    block: &Block,
    i: usize,
    page_dir: &Path,
    tokens: &Tokens,
    live: bool,
    page: Option<Dir>,
    // How the page lays this paragraph out: justified, or set against an edge (issue #258).
    justified: Option<Mark>,
    selected: Option<(usize, usize)>,
    id: egui::Id,
    zoom: f32,
) -> BlockResponse {
    let start = ui.cursor().min;
    // The least height of a line this block was given, left for it by `make_room`.
    let floor = take_line_floor(ui.ctx());
    let body = ui.style().visuals.text_color();
    let mut task_set = None;
    // Keep the actual painted bounds for rich-object hit testing.  The block's
    // layout row may be as wide as the page even when an image is narrower.
    let mut rendered_object_rect = None;
    {
        match block {
            Block::Heading { level, spans, .. } => {
                let size = match level {
                    1 => 28.0,
                    2 => 22.0,
                    3 => 18.0,
                    _ => 16.0,
                } * zoom;
                ui.add_space(6.0 * zoom);
                match tagged(spans) {
                    // A heading is aligned as a paragraph is, but never justified.
                    Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| {
                        draw_spans(ui, &spans, size, tokens.strong, tokens, page, aligned_only(justified), selected, floor);
                    }),
                    None => draw_spans(ui, spans, size, tokens.strong, tokens, page, aligned_only(justified), selected, floor),
                }
                ui.add_space(2.0 * zoom);
            }
            Block::Paragraph(spans, _) => {
                // A line that starts with a tag marker shows the tag's mark, not the word.
                match tagged(spans) {
                    Some((tag, spans)) => {
                        with_tag(ui, tag, tokens, |ui| {
                            draw_inline_images(ui, &spans, page_dir, zoom, i, &mut rendered_object_rect);
                            draw_spans(ui, &spans, 14.5 * zoom, body, tokens, page, justified, selected, floor);
                        });
                        ui.add_space(4.0 * zoom);
                    }
                    None => {
                        draw_inline_images(ui, spans, page_dir, zoom, i, &mut rendered_object_rect);
                        if !text_of(spans).is_empty() || spans.iter().any(|s| s.math) {
                            draw_spans(ui, spans, 14.5 * zoom, body, tokens, page, justified, selected, floor);
                        }
                        ui.add_space(4.0 * zoom);
                    }
                }
            }
            Block::Item { ordered, number, task, depth, spans, .. } => {
                // A right-to-left item starts at the right: its bullet or box goes there,
                // and the indent of a nested item comes off the right edge too.
                let row = if dir_of(spans, page).is_rtl() {
                    egui::Layout::right_to_left(egui::Align::Min)
                } else {
                    egui::Layout::left_to_right(egui::Align::Min)
                };
                ui.with_layout(row, |ui| {
                    ui.add_space((8.0 + *depth as f32 * 18.0) * zoom);
                    match task {
                        Some(done) => {
                            let mut v = *done;
                            // On a page the user is writing, the box is theirs to tick.
                            if ui.add_enabled(live, egui::Checkbox::without_text(&mut v)).changed() {
                                task_set = Some(v);
                            }
                        }
                        None => {
                            let marker_size = item_marker_size(spans, zoom);
                            ui.label(
                                // i18n-exempt: This glyph is the visual bullet marker for an unordered list.
                                RichText::new(if *ordered { format!("{number}.") } else { "\u{2022}".to_string() })
                                    .size(marker_size)
                                    .color(tokens.muted),
                            );
                        }
                    }
                    ui.vertical(|ui| match tagged(spans) {
                        Some((tag, spans)) => with_tag(ui, tag, tokens, |ui| {
                            draw_spans(ui, &spans, 14.5 * zoom, body, tokens, page, None, selected, floor);
                            draw_inline_images(ui, &spans, page_dir, zoom, i, &mut rendered_object_rect);
                        }),
                        None => {
                            draw_spans(ui, spans, 14.5 * zoom, body, tokens, page, None, selected, floor);
                            draw_inline_images(ui, spans, page_dir, zoom, i, &mut rendered_object_rect);
                        }
                    });
                });
            }
            Block::Quote(spans, _) => {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, 18.0)), 1.0, tokens.quote_bar);
                    draw_spans(ui, spans, 14.5 * zoom, tokens.muted, tokens, page, None, selected, floor);
                });
                ui.add_space(4.0 * zoom);
            }
            // A Mermaid diagram is shown as the drawing, in the page, and a click puts the caret in
            // it, which shows its source to edit. One that cannot be drawn says why and shows its
            // source instead, so nothing the page holds is ever hidden.
            Block::Code { lang, text, .. } if lang == "mermaid" => {
                egui::Frame::new().fill(tokens.editor).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if let crate::diagramview::Shown::Failed(_) = crate::diagramview::show_zoomed(ui, text, tokens, zoom) {
                        ui.label(RichText::new(text).monospace().size(14.5 * zoom));
                    }
                });
                ui.add_space(4.0 * zoom);
            }
            Block::Code { lang, text, .. } => {
                let (numbered, language) = notecraft_formats::markdown::code_lang(lang);
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    if !language.is_empty() {
                        ui.label(RichText::new(language).size(12.0 * zoom).color(tokens.muted));
                    }
                    if numbered {
                        // Numbers are for reading: the block being written in shows none (pageedit.rs).
                        let lines = text.trim_end_matches('\n').split('\n').count();
                        let numbers = line_numbers(lines);
                        ui.horizontal_top(|ui| {
                            ui.label(RichText::new(numbers).monospace().size(14.5 * zoom).color(tokens.muted));
                            ui.label(RichText::new(text.trim_end_matches('\n')).monospace().size(14.5 * zoom));
                        });
                    } else {
                        ui.label(RichText::new(text).monospace().size(14.5 * zoom));
                    }
                });
                ui.add_space(4.0 * zoom);
            }
            Block::MathBlock(tex, _) => {
                egui::Frame::new().fill(tokens.code_bg).corner_radius(6.0).inner_margin(8.0).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    crate::mathview::display(ui, tex, 22.0 * zoom, tokens.strong);
                });
                ui.add_space(4.0 * zoom);
            }
            Block::Table { head, rows, .. } => {
                egui::Grid::new(("table", i)).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
                    for cell in head {
                        ui.label(job(ui, cell, 14.5 * zoom, tokens.strong, tokens, page));
                    }
                    ui.end_row();
                    for row in rows {
                        for cell in row {
                            ui.label(job(ui, cell, 14.5 * zoom, body, tokens, page));
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(6.0 * zoom);
            }
            Block::Rule(_) => {
                ui.separator();
            }
            // The page names its own ink so that its words say what is on it, but the ink
            // is drawn as a layer over the whole page, not as a picture in the middle of
            // the words. Another program opening the page sees the handwriting; here the
            // line itself takes no room.
            Block::Image { src, .. } if crate::inkpad::is_page_ink(src) => {}
            Block::Image { alt, src, width, height, .. } => {
                match picture(ui.ctx(), page_dir, src) {
                    Picture::Loading => {
                        let size = match (width, height) {
                            (Some(width), Some(height)) => vec2(*width * zoom, *height * zoom),
                            _ => vec2(ui.available_width(), picture_max_height(src, zoom)),
                        };
                        loading_picture(ui, size);
                    }
                    Picture::Ready(uri, bytes) => {
                        let image = egui::Image::from_bytes(uri, bytes).sense(egui::Sense::hover());
                        let response = match (width, height) {
                            (Some(width), Some(height)) => {
                                ui.add(image.fit_to_exact_size(vec2(*width * zoom, *height * zoom)).maintain_aspect_ratio(false))
                            }
                            // At most its own size times the zoom, so it grows and shrinks with
                            // the words, and never wider than its column.
                            _ => ui.add(image.max_width(ui.available_width()).max_height(picture_max_height(src, zoom)).fit_to_original_size(zoom)),
                        };
                        outline_highlight(ui, page_dir, src, response.rect);
                        rendered_object_rect = Some(response.rect);
                        // Named by its alt text, so tests and screen readers can find the picture.
                        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, alt.as_str()));
                        if object_link(
                            &Block::Image { alt: alt.clone(), src: src.clone(), width: *width, height: *height, emoji: Vec::new() },
                            page_dir,
                        )
                        .is_none()
                        {
                            record_rendered_image(ui.ctx(), i, response.rect, src);
                        }
                        if response.hovered() {
                            log::debug!(
                                "image render hover: block={i} src={src:?} rect={:?} pointer={:?}",
                                response.rect,
                                ui.ctx().pointer_latest_pos()
                            );
                        }
                    }
                    Picture::Missing => {
                        ui.weak(format!("[image: {alt} ({src})]"));
                    }
                }
                ui.add_space(4.0 * zoom);
            }
        }
    }
    // A block with no words in it is still a line the writer can click into. Drawn as nothing it
    // was a few points tall, so clicking the empty box that held it went nowhere and no caret
    // appeared: give it the height of one line, for the eye and for the pointer.
    if is_blank(block) {
        let used = ui.cursor().min.y - start.y;
        let line = match block {
            Block::Heading { level: 1, .. } => 34.0,
            Block::Heading { level: 2, .. } => 28.0,
            Block::Heading { .. } => 24.0,
            _ => 22.0,
        } * zoom;
        if used < line {
            ui.add_space(line - used);
        }
    }
    // Give rich objects their own real hit target after painting them. The
    // paragraph response remains the fallback for text beside the object, but
    // this response preserves double-clicks on the attachment itself.
    if let Some(object_rect) = rendered_object_rect {
        let object_response = ui.interact(object_rect, id.with("object-hit"), egui::Sense::click());
        if object_response.double_clicked() {
            ui.data_mut(|data| data.insert_temp(object_double_clicked_id(), (i, object_kind(block).unwrap_or("image").to_string())));
        }
    }
    let end = ui.cursor().min;
    let rect = egui::Rect::from_min_max(start, egui::pos2(start.x + ui.available_width().max(1.0), end.y.max(start.y + 1.0)));
    if is_blank(block) {
        log::debug!(
            "empty block rendered: block={} rect=({:.1},{:.1})-({:.1},{:.1}) size=({:.1}x{:.1})",
            i,
            rect.min.x,
            rect.min.y,
            rect.max.x,
            rect.max.y,
            rect.width(),
            rect.height()
        );
    }
    let response = ui.interact(rect, id, egui::Sense::click());
    if let Some(kind) = object_kind(block) {
        // Read every response flag before `data_mut`: `clicked` and
        // `double_clicked` take the context lock that `data_mut` holds.
        let clicked = response.clicked();
        let double_clicked = response.double_clicked();
        let pointer = response.interact_pointer_pos();
        ui.data_mut(|data| {
            // Added where it is kept, not copied out and back for every picture (see
            // `record_rendered_image`).
            data.get_temp_mut_or_default::<Vec<(usize, egui::Rect, String)>>(object_hits_id()).push((
                i,
                rendered_object_rect.unwrap_or(response.rect),
                kind.to_string(),
            ));
            if clicked {
                data.insert_temp(object_clicked_id(), (i, kind.to_string()));
            }
            if double_clicked {
                data.insert_temp(object_double_clicked_id(), (i, kind.to_string()));
            }
        });
        if clicked {
            log::debug!("embedded object click: block={i} kind={kind} rect={:?} pointer={pointer:?}", response.rect);
        }
    }
    BlockResponse { response, task_set }
}

pub(crate) fn object_kind(block: &Block) -> Option<&'static str> {
    match block {
        Block::Image { .. } => Some("image"),
        Block::Code { lang, .. } if lang == "mermaid" => Some("diagram"),
        // Code is page text like a paragraph: a click places the caret in it (owner, 2026-11-11).
        Block::Code { .. } => None,
        Block::MathBlock(_, _) => Some("math"),
        Block::Table { .. } => Some("table"),
        Block::Paragraph(spans, _) | Block::Item { spans, .. } | Block::Heading { spans, .. } | Block::Quote(spans, _)
            if spans.iter().any(|s| s.image.is_some()) =>
        {
            Some("image")
        }
        _ => None,
    }
}

pub(crate) fn object_link(block: &Block, page_dir: &std::path::Path) -> Option<String> {
    match block {
        Block::Paragraph(spans, _) | Block::Item { spans, .. } | Block::Heading { spans, .. } | Block::Quote(spans, _) => {
            spans.iter().find_map(|span| span.link.clone())
        }
        // Attachment cards in stored Markdown are represented by the generated
        // file icon as a standalone image in some older/imported pages. The
        // icon's stem is the original attachment name, but only treat it as a
        // file card when that original is actually present beside the icon.
        Block::Image { src, .. } => {
            let name = src.strip_prefix("assets/")?;
            let (source, extension) = name.rsplit_once('.')?;
            if !matches!(extension.to_ascii_lowercase().as_str(), "png" | "svg") {
                return None;
            }
            crate::fscache::is_file(&page_dir.join("assets").join(source)).then(|| format!("assets/{source}"))
        }
        _ => None,
    }
}

pub(crate) fn object_hits_id() -> egui::Id {
    egui::Id::new("embedded-object-hits")
}

pub(crate) fn object_clicked_id() -> egui::Id {
    egui::Id::new("embedded-object-clicked")
}

pub(crate) fn object_double_clicked_id() -> egui::Id {
    egui::Id::new("embedded-object-double-clicked")
}

pub(crate) fn object_click_state_id() -> egui::Id {
    egui::Id::new("embedded-object-click-state")
}

/// A picture rendered in the preview, with its block index, bounds on screen, and source path.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderedImage {
    pub block: usize,
    pub rect: egui::Rect,
    pub src: String,
}

pub fn rendered_images_id() -> egui::Id {
    egui::Id::new("rendered-image-hits")
}

/// The boxes of a search hit on a picture, to outline when that picture is drawn. `src` is the
/// picture's name as the page writes it (`assets/x.png`); `pixels` is its size in pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureHighlight {
    pub page_dir: std::path::PathBuf,
    pub src: String,
    pub pixels: (u32, u32),
    pub boxes: Vec<[[f32; 2]; 4]>,
}

fn highlight_id() -> egui::Id {
    egui::Id::new("notecraft-picture-highlight")
}

/// Sets, or with `None` clears, the picture whose matching lines are outlined.
pub fn set_picture_highlight(ctx: &egui::Context, highlight: Option<PictureHighlight>) {
    ctx.data_mut(|data| data.insert_temp(highlight_id(), highlight));
}

fn picture_highlight(ctx: &egui::Context) -> Option<PictureHighlight> {
    ctx.data_mut(|data| data.get_temp::<Option<PictureHighlight>>(highlight_id())).flatten()
}

/// Maps a box, in the picture's pixels, into the rectangle the picture is drawn in. The four
/// corners keep their order.
pub fn box_on_screen(quad: [[f32; 2]; 4], drawn: egui::Rect, pixels: (u32, u32)) -> [egui::Pos2; 4] {
    let (w, h) = (pixels.0.max(1) as f32, pixels.1.max(1) as f32);
    let at = |p: [f32; 2]| egui::pos2(drawn.left() + p[0] / w * drawn.width(), drawn.top() + p[1] / h * drawn.height());
    [at(quad[0]), at(quad[1]), at(quad[2]), at(quad[3])]
}

/// Outlines the boxes of the highlight when this is the picture it is for.
fn outline_highlight(ui: &Ui, page_dir: &Path, src: &str, drawn: egui::Rect) {
    let Some(h) = picture_highlight(ui.ctx()) else { return };
    if h.page_dir != page_dir || h.src != src {
        return;
    }
    let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
    for quad in &h.boxes {
        let corners = box_on_screen(*quad, drawn, h.pixels).to_vec();
        ui.painter().add(egui::Shape::closed_line(corners, stroke));
    }
}

pub fn record_rendered_image(ctx: &egui::Context, block: usize, rect: egui::Rect, src: &str) {
    // Added where it is kept: taken out, added to and put back, the whole list was copied for
    // every picture drawn, which on a page of printouts was the most of a frame.
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Vec<RenderedImage>>(rendered_images_id()).push(RenderedImage { block, rect, src: src.to_string() });
    });
}

/// A text block with no words, no maths and no picture in it: a blank line.
pub fn is_blank(block: &Block) -> bool {
    match block {
        Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Heading { spans, .. } | Block::Item { spans, .. } => {
            text_of(spans).is_empty() && !spans.iter().any(|s| s.math || s.image.is_some())
        }
        _ => false,
    }
}

/// The tag at the start of a line, and the words without its marker.
pub fn tagged(spans: &[Span]) -> Option<(notecraft_model::Tag, Vec<Span>)> {
    let (tag, _) = notecraft_model::Tag::split(&text_of(spans));
    let tag = tag?;
    let marker = tag.marker();
    let mut out = spans.to_vec();
    if let Some(first) = out.first_mut() {
        if let Some(rest) = first.text.strip_prefix(&marker) {
            first.text = rest.to_string();
        } else if let Some(rest) = first.text.strip_prefix(marker.trim_end()) {
            first.text = rest.to_string();
        }
    }
    Some((tag, out))
}

/// Draws a line behind its tag mark.
fn with_tag(ui: &mut Ui, tag: notecraft_model::Tag, tokens: &Tokens, body: impl FnOnce(&mut Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(21.0, 20.0), egui::Sense::hover());
        crate::ribbon::paint_tag(
            &ui.painter_at(rect),
            egui::Rect::from_center_size(rect.center(), egui::vec2(17.0, 17.0)),
            tag,
            tokens.text,
            tokens.accent,
        );
        resp.on_hover_text(crate::tl!(tag.label()));
        ui.vertical(body);
    });
}

#[cfg(test)]
mod tests {
    /// A printout's page is drawn as wide as its box, the way a sheet of paper fills the page
    /// it is put on; other pictures stay no taller than 480 points.
    #[test]
    fn a_printout_page_is_not_held_to_a_picture_height() {
        assert_eq!(super::picture_max_height("assets/Notes_13_Bloch-3l.pdf.printout-001.png", 1.0), f32::INFINITY);
        assert_eq!(super::picture_max_height("assets/a.pdf.printout-012.png", 0.5), f32::INFINITY);
        assert_eq!(super::picture_max_height("assets/photo.png", 1.0), 480.0);
        assert_eq!(super::picture_max_height("assets/photo.png", 0.5), 240.0);
        assert_eq!(super::picture_max_height("assets/printout.png", 1.0), 480.0);
    }

    #[test]
    fn numbered_lines_have_a_bar_after_each_number() {
        assert_eq!(super::line_numbers(3), "1 |\n2 |\n3 |");
    }

    #[test]
    fn the_bar_follows_every_number_including_two_digit_ones() {
        let column = super::line_numbers(12);
        let lines: Vec<&str> = column.split('\n').collect();
        assert_eq!(lines.len(), 12);
        assert!(lines.iter().all(|l| l.ends_with(" |")), "{column}");
        assert_eq!(lines[9], "10 |");
        assert_eq!(lines[11], "12 |");
    }

    /// The picture once it has been read, asking as each frame does; it is read on a thread of
    /// its own, so the first asks find it still on its way.
    fn ready(ctx: &egui::Context, page: &Path, src: &str) -> (String, egui::load::Bytes) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            match picture(ctx, page, src) {
                Picture::Ready(uri, bytes) => return (uri, bytes),
                Picture::Loading if std::time::Instant::now() < until => std::thread::sleep(std::time::Duration::from_millis(5)),
                other => panic!("{src} was not read: {other:?}"),
            }
        }
    }

    /// Issue: the page read every picture from disk on every frame (a page of printouts on a
    /// network or cloud folder made the window crawl). A picture is read once, and again only
    /// when the file changes.
    #[test]
    fn an_unchanged_picture_is_read_from_disk_once() {
        let t = notecraft_store::testing::TempDir::new("page-image-once");
        std::fs::create_dir_all(t.path().join("assets")).expect("assets");
        std::fs::write(t.path().join("assets").join("a.png"), b"first picture bytes").expect("picture");
        let ctx = egui::Context::default();
        let first = ready(&ctx, t.path(), "assets/a.png");
        for _ in 0..4 {
            let again = ready(&ctx, t.path(), "assets/a.png");
            assert_eq!(again.0, first.0, "the same name while the file is unchanged");
        }
        assert_eq!(picture_reads(&ctx), 1, "five frames, one read");
        assert_eq!(&*first.1, b"first picture bytes");
    }

    #[test]
    fn a_changed_picture_is_read_again_under_a_new_name() {
        let t = notecraft_store::testing::TempDir::new("page-image-changed");
        std::fs::create_dir_all(t.path().join("assets")).expect("assets");
        let file = t.path().join("assets").join("a.png");
        std::fs::write(&file, b"first").expect("picture");
        let ctx = egui::Context::default();
        let first = ready(&ctx, t.path(), "assets/a.png");
        std::fs::write(&file, b"a different, longer picture").expect("replaced");
        // What the window knew about the file is kept a moment (see `fscache`); a change made
        // by a command lets it go at once, as here, and one made elsewhere shows within the moment.
        crate::fscache::forget_all();
        let second = ready(&ctx, t.path(), "assets/a.png");
        assert_eq!(picture_reads(&ctx), 2, "read again once it changed");
        assert_ne!(first.0, second.0, "a new name, so egui does not keep showing the old picture");
        assert_eq!(&*second.1, b"a different, longer picture");
    }

    #[test]
    fn pictures_with_the_same_name_on_two_pages_are_kept_apart() {
        let t = notecraft_store::testing::TempDir::new("page-image-two-pages");
        for (page, bytes) in [("one", b"page one's picture".as_slice()), ("two", b"page two's picture".as_slice())] {
            std::fs::create_dir_all(t.path().join(page).join("assets")).expect("assets");
            std::fs::write(t.path().join(page).join("assets").join("a.png"), bytes).expect("picture");
        }
        let ctx = egui::Context::default();
        let one = ready(&ctx, &t.path().join("one"), "assets/a.png");
        let two = ready(&ctx, &t.path().join("two"), "assets/a.png");
        assert_ne!(one.0, two.0, "each page's picture has a name of its own");
        assert_eq!(&*two.1, b"page two's picture");
    }

    #[test]
    fn a_picture_that_is_not_there_is_missing_not_loading() {
        let t = notecraft_store::testing::TempDir::new("page-image-missing");
        let ctx = egui::Context::default();
        assert!(matches!(picture(&ctx, t.path(), "assets/none.png"), Picture::Missing));
        assert!(matches!(picture(&ctx, t.path(), "../outside.png"), Picture::Missing));
    }

    /// Reads stuck for good, on a hung mount, say, take a reader each and never give it back.
    /// Readers are started as pictures wait for one, with no cap, so however many of a page's
    /// pictures are stuck, the next page's pictures still come.
    #[cfg(unix)]
    #[test]
    fn pictures_stuck_on_one_page_do_not_keep_another_pages_from_loading() {
        let t = notecraft_store::testing::TempDir::new("page-image-stuck");
        let (stuck, other) = (t.path().join("stuck"), t.path().join("other"));
        for page in [&stuck, &other] {
            std::fs::create_dir_all(page.join("assets")).expect("assets");
        }
        const STUCK: usize = 16;
        let pipes: Vec<std::path::PathBuf> = (0..STUCK).map(|n| stuck.join("assets").join(format!("p{n}.png"))).collect();
        for pipe in &pipes {
            assert!(std::process::Command::new("mkfifo").arg(pipe).status().expect("mkfifo").success());
        }
        std::fs::write(other.join("assets").join("a.png"), b"the other page's picture").expect("picture");
        let ctx = egui::Context::default();
        let (answers, answered) = std::sync::mpsc::channel();
        let asker = ctx.clone();
        std::thread::spawn(move || {
            for n in 0..STUCK {
                let _ = picture(&asker, &stuck, &format!("assets/p{n}.png"));
            }
            let (_, bytes) = ready(&asker, &other, "assets/a.png");
            let _ = answers.send(bytes.to_vec());
        });
        let got = answered.recv_timeout(std::time::Duration::from_secs(20));
        // Let the stuck reads go, so their readers are free again for the other tests. Opened
        // without waiting: a pipe no reader has opened yet is skipped instead of hanging here.
        for pipe in &pipes {
            let _ = nonblocking_write(pipe, b"x");
        }
        assert_eq!(got.ok().as_deref(), Some(b"the other page's picture".as_slice()), "the other page's picture came");
    }

    /// Writes into a named pipe if a reader has it open, without waiting for one.
    #[cfg(unix)]
    fn nonblocking_write(pipe: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        // Linux and Android share the generic value; macOS and the BSDs another.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const O_NONBLOCK: i32 = 0o4000;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        const O_NONBLOCK: i32 = 0x0004;
        std::fs::OpenOptions::new().write(true).custom_flags(O_NONBLOCK).open(pipe)?.write_all(bytes)
    }

    /// A picture in a synced folder that is not on this computer yet is downloaded when it is
    /// first read, and the read waits for it: read while the page was drawn, it froze the whole
    /// window. A named pipe stands in for it here, since reading one waits until something is
    /// written into it. The window gets its answer at once, and the picture when it comes.
    #[cfg(unix)]
    #[test]
    fn a_picture_slow_to_read_does_not_hold_up_the_window() {
        let t = notecraft_store::testing::TempDir::new("page-image-slow");
        std::fs::create_dir_all(t.path().join("assets")).expect("assets");
        let pipe = t.path().join("assets").join("slow.png");
        let made = std::process::Command::new("mkfifo").arg(&pipe).status().expect("mkfifo");
        assert!(made.success(), "a named pipe for the slow picture");
        let ctx = egui::Context::default();

        // Asked as the window asks, from one thread throughout (what is known about a file is
        // kept per thread), but not this one, so a read that waits fails the test instead of
        // hanging it.
        let (answers, answered) = std::sync::mpsc::channel();
        let (go_on, written) = std::sync::mpsc::channel::<()>();
        let (asker, page) = (ctx.clone(), t.path().to_path_buf());
        std::thread::spawn(move || {
            let _ = answers.send(format!("{:?}", picture(&asker, &page, "assets/slow.png")));
            if written.recv().is_ok() {
                let (_, bytes) = ready(&asker, &page, "assets/slow.png");
                let _ = answers.send(String::from_utf8_lossy(&bytes).into_owned());
            }
        });
        let first = answered.recv_timeout(std::time::Duration::from_secs(3));
        assert_eq!(first.as_deref(), Ok("Loading"), "the window is told it is on its way, without waiting for it");

        // Then the "download" finishes, and the picture is there for the next frame.
        // Written once a reader has the pipe open, and not waited for past a deadline: a reader
        // that never came would otherwise hang the test here rather than fail it.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while nonblocking_write(&pipe, b"the picture, downloaded").is_err() {
            assert!(std::time::Instant::now() < until, "no reader took the slow picture");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        go_on.send(()).expect("the asker is waiting");
        let then = answered.recv_timeout(std::time::Duration::from_secs(10));
        assert_eq!(then.as_deref(), Ok("the picture, downloaded"));
    }

    use super::*;

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn preview_uses_an_installed_named_family_for_ltr_and_bidi() {
        let mut database = fontdb::Database::new();
        database.load_font_data(crate::fonts::BUNDLED[0].1.to_vec());
        let catalog = crate::fonts::system::Catalog::from_database(database);
        let family = catalog.families().first().expect("fixture family");
        let mut definitions = crate::fonts::definitions();
        let key = catalog.install(family, &mut definitions).expect("install the fixture family");
        let ctx = egui::Context::default();
        ctx.set_fonts(definitions);
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let span = Span { text: "Hello مرحبا".into(), font_family: Some(family.clone()), ..Span::default() };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let job = job(ui, std::slice::from_ref(&span), 14.0, tokens.text, &tokens, None);
            assert_eq!(job.sections[0].format.font_id.family, key);
            let bidi = pieces(ui.ctx(), std::slice::from_ref(&span), 14.0, tokens.text, &tokens);
            assert_eq!(bidi[0].1.font_id.family, key);
        });
        output.textures_delta.clear();
    }

    #[test]
    fn inline_font_size_scales_the_span_without_changing_the_base_size() {
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let span = Span { text: "larger".into(), font_size: Some(18), ..Span::default() };
        let format = format_of(&span, 11.0, tokens.text, &tokens);
        assert_eq!(format.font_id.size, 18.0);
    }

    #[test]
    fn underline_and_scripts_change_preview_text_format() {
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let underline = Span { underline: true, ..Span::default() };
        assert_ne!(format_of(&underline, 20.0, tokens.text, &tokens).underline, Stroke::NONE);
        for (script, alignment) in [
            (notecraft_formats::markdown::Script::Subscript, egui::Align::BOTTOM),
            (notecraft_formats::markdown::Script::Superscript, egui::Align::TOP),
        ] {
            let span = Span { script: Some(script), ..Span::default() };
            let format = format_of(&span, 20.0, tokens.text, &tokens);
            assert_eq!(format.font_id.size, 14.0);
            assert_eq!(format.valign, alignment);
        }
    }

    #[test]
    fn a_blank_line_is_a_line_tall_so_it_can_be_clicked() {
        use notecraft_formats::markdown::Block;
        let t = crate::theme::Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        let mut heights = Vec::new();
        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            crate::fonts::install_once(ui.ctx());
            for block in [
                Block::Paragraph(Vec::new(), Vec::new()),
                Block::Heading { level: 1, spans: Vec::new(), emoji: Vec::new() },
                Block::Quote(Vec::new(), Vec::new()),
            ] {
                let out = show_block(
                    ui,
                    &block,
                    0,
                    std::path::Path::new("."),
                    &t,
                    true,
                    Some(notecraft_model::Dir::Ltr),
                    None,
                    None,
                    ui.id().with("block"),
                );
                heights.push(out.response.rect.height());
            }
        });
        frame.textures_delta.clear();
        assert!(heights.iter().all(|h| *h >= 20.0), "an empty line has the height of a line, not a sliver: {heights:?}");
        assert!(is_blank(&Block::Paragraph(Vec::new(), Vec::new())) && !is_blank(&Block::Rule(Vec::new())));
    }

    #[test]
    fn both_formats_give_the_same_blocks_for_the_same_content() {
        let md = blocks_for(PageFormat::Cmgfm, "# Title\n\n- a\n- b\n");
        let html = blocks_for(PageFormat::Html, "<h1>Title</h1><ul><li>a</li><li>b</li></ul>");
        assert_eq!(md.len(), html.len());
        assert!(matches!(&html[0], Block::Heading { level: 1, .. }));
    }

    #[test]
    fn ordinary_percent_encoded_and_consecutive_dot_image_names_load() {
        let t = notecraft_store::testing::TempDir::new("preview-owned-image-names");
        let assets = t.path().join("assets");
        std::fs::create_dir(&assets).expect("assets directory");
        let png = include_bytes!("../../import/tests/fixtures/synthetic-pixel.png");
        for (file, reference) in [("Sketch-café.png", "Sketch-caf%C3%A9.png"), ("Daily-plan..png", "Daily-plan..png")] {
            std::fs::write(assets.join(file), png).expect("picture");
            let ctx = egui::Context::default();
            let src = format!("assets/{reference}");
            let (_, bytes) = ready(&ctx, t.path(), &src);
            assert_eq!(bytes.as_ref(), png);
            assert_eq!(crate::imagemenu::validate_and_read_image(t.path(), &src).expect("image action reads same asset").1, file);
        }
    }

    #[test]
    fn image_uris_only_for_attachments() {
        let dir = Path::new("/notes/nb/page");
        assert_eq!(image_uri(dir, "assets/a.png").as_deref(), Some("file:///notes/nb/page/assets/a.png"));
        assert_eq!(image_uri(Path::new("C:\\n\\p"), "assets/a b.png").as_deref(), Some("file:///C:/n/p/assets/a%20b.png"));
        assert_eq!(image_uri(Path::new(r"\\?\C:\n\p"), "assets/a.png").as_deref(), Some("file:///C:/n/p/assets/a.png"), "no verbatim prefix");
        assert_eq!(
            image_uri(Path::new(r"\\?\UNC\VBoxSvr\C_DRIVE\p"), "assets/a.png").as_deref(),
            Some("file:////VBoxSvr/C_DRIVE/p/assets/a.png"),
            "a network share keeps its two leading slashes, not a UNC folder"
        );
        assert_eq!(image_uri(dir, "https://x.test/a.png"), None);
        assert_eq!(image_uri(dir, "assets/../../secret.png"), None);
        assert_eq!(image_uri(dir, "assets/"), None);
        assert_eq!(image_uri(dir, "assets/a\\b.png"), None);
        assert_eq!(image_uri(dir, "../assets/a.png"), None);
    }

    #[test]
    fn html_attachment_cards_reach_the_preview_as_linked_images() {
        let blocks = blocks_for(
            PageFormat::Html,
            r#"<p><a href="assets/brief.docx"><img src="assets/brief.docx.png" alt="" width="48"><br>brief.docx</a></p>"#,
        );
        let Block::Paragraph(spans, _) = &blocks[0] else { panic!("attachment card was not a paragraph") };
        let image = spans.iter().find(|span| span.image.as_deref() == Some("assets/brief.docx.png")).expect("attachment icon");
        assert_eq!(image.link.as_deref(), Some("assets/brief.docx"));
    }

    #[test]
    fn an_attachment_card_hit_follows_its_file_link() {
        let blocks = blocks_for(
            PageFormat::Html,
            r#"<p><a href="assets/brief.docx"><img src="assets/brief.docx.png" alt="" width="48"><br>brief.docx</a></p>"#,
        );
        assert_eq!(link_at_point(&blocks[0], egui::vec2(24.0, 28.0), 400.0, Some(Dir::Ltr)).as_deref(), Some("assets/brief.docx"));
    }

    #[test]
    fn rich_blocks_share_object_selection_kinds() {
        assert_eq!(
            object_kind(&Block::Image { alt: "a".into(), src: "assets/a.png".into(), width: None, height: None, emoji: Vec::new() }),
            Some("image")
        );
        assert_eq!(object_kind(&Block::Code { lang: "mermaid".into(), text: "graph TD;".into(), emoji: Vec::new() }), Some("diagram"));
        assert_eq!(
            object_kind(&Block::Code { lang: "rust".into(), text: "fn main() {}".into(), emoji: Vec::new() }),
            None,
            "a code block is page text, not an object"
        );
        assert_eq!(object_kind(&Block::MathBlock("x^2".into(), Vec::new())), Some("math"));
        assert_eq!(object_kind(&Block::Table { head: vec![], rows: vec![], emoji: Vec::new() }), Some("table"));
        assert_eq!(object_kind(&Block::Paragraph(vec![Span { image: Some("assets/a.png".into()), ..Span::default() }], Vec::new())), Some("image"));
        assert_eq!(object_kind(&Block::Paragraph(vec![Span { text: "plain".into(), ..Span::default() }], Vec::new())), None);
    }

    /// An attachment link must resolve to something egui can actually load.
    #[test]
    fn attachment_images_load_through_the_file_loader() {
        use notecraft_store::testing::TempDir;
        let t = TempDir::new("preview-img");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("logo.png"), crate::LOGO_PNG).expect("png");
        let uri = image_uri(&page, "assets/logo.png").expect("uri");
        let ctx = egui::Context::default();
        crate::pictures::install(&ctx);
        let mut last = String::new();
        for _ in 0..100 {
            match ctx.try_load_image(&uri, egui::SizeHint::default()) {
                Ok(egui::load::ImagePoll::Ready { image }) => {
                    assert_eq!(image.size, [256, 256]);
                    return;
                }
                Ok(egui::load::ImagePoll::Pending { .. }) => last = "pending".into(),
                Err(e) => panic!("{uri}: {e}"),
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("never ready: {last}");
    }

    /// A link in an Arabic line is found under the pointer through parley, and the words
    /// around it are not links.
    #[test]
    fn a_link_in_a_right_to_left_line_is_found_under_the_pointer() {
        let blocks = blocks_for(PageFormat::Cmgfm, "مرحبا بالعالم [هذه صفحة](../page-b/) والباقي كلام عادي\n");
        let block = blocks.first().expect("a paragraph");
        let hits: Vec<(i32, Option<String>)> =
            (0..400).step_by(8).map(|x| (x, link_at_point(block, egui::vec2(x as f32, 8.0), 400.0, Some(Dir::Ltr)))).collect();
        let on: Vec<i32> = hits.iter().filter(|(_, h)| h.is_some()).map(|(x, _)| *x).collect();
        assert!(!on.is_empty(), "the link is somewhere on the line: {hits:?}");
        assert!(hits.iter().all(|(_, h)| h.is_none() || h.as_deref() == Some("../page-b/")));
        // The line hangs from the right edge: the first words sit at the far right and are
        // not the link, and the link is left of them.
        assert!(hits.last().is_some_and(|(_, h)| h.is_none()), "the right edge is the first word, not the link");
        let (first_on, last_on) = (*on.first().unwrap_or(&0), *on.last().unwrap_or(&0));
        assert!(last_on < 392 && last_on - first_on > 30, "a contiguous stretch: {first_on}..{last_on}");
        // A left-to-right paragraph is not this painter's; the caller uses egui's layout.
        let ltr = blocks_for(PageFormat::Cmgfm, "Hello [there](../x/) world\n");
        assert_eq!(link_at_point(&ltr[0], egui::vec2(50.0, 8.0), 400.0, Some(Dir::Ltr)), None);
    }

    /// A picture is drawn at its full size even when the layout has no height left.
    ///
    /// This is the defect the Arabic showcase page found: the same picture drew at the top
    /// of one page and came out nothing at all low down another, because egui fits an image
    /// into the space the layout has left and a scrolling page has none once the blocks
    /// above have filled the view.
    #[test]
    fn a_picture_low_on_a_page_is_still_drawn() {
        use notecraft_store::testing::TempDir;
        let t = TempDir::new("preview-low");
        let page = t.path().join("page");
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("logo.png"), crate::LOGO_PNG).expect("png");
        let blocks = blocks_for(
            PageFormat::Cmgfm,
            "![logo](assets/logo.png)
",
        );
        let ctx = egui::Context::default();
        crate::pictures::install(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Dark, accent: crate::theme::Accent::Default }.tokens();
        let mut tall = 0.0;
        for _ in 0..100 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                // The blocks above have taken the whole view: nothing is left below.
                let left = ui.available_height();
                ui.allocate_space(egui::vec2(0.0, left));
                let before = ui.cursor().min.y;
                show(ui, &blocks, &page, &tokens, Some(Dir::Ltr), &Justified::default());
                tall = ui.cursor().min.y - before;
            });
            out.textures_delta.clear();
            if tall > 100.0 {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        panic!("the picture came out {tall} points tall");
    }

    /// A picture grows with the page's zoom, also past its own pixel size: zoomed in on a printout,
    /// its page grows with the words, rather than stopping at the size of the picture's pixels.
    #[test]
    fn a_picture_grows_with_the_zoom_past_its_own_size() {
        let t = notecraft_store::testing::TempDir::new("picture-zoom");
        let page = t.path().to_path_buf();
        std::fs::create_dir_all(page.join("assets")).expect("assets");
        std::fs::write(page.join("assets").join("logo.png"), crate::LOGO_PNG).expect("png");
        let blocks = blocks_for(PageFormat::Cmgfm, "![logo](assets/logo.png)\n");
        let ctx = egui::Context::default();
        crate::pictures::install(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Light, accent: crate::theme::Accent::Default }.tokens();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(8000.0, 8000.0));
        let tall_at = |zoom: f32| {
            let mut tall = 0.0;
            for _ in 0..100 {
                let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
                let mut out = ctx.run_ui(input, |ui| {
                    let before = ui.cursor().min.y;
                    let id = ui.id().with("zoomed");
                    show_block_zoomed(ui, &blocks[0], 0, &page, &tokens, false, Some(Dir::Ltr), None, None, id, zoom);
                    tall = ui.cursor().min.y - before;
                });
                out.textures_delta.clear();
                if tall > 40.0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            tall
        };
        let (one, three) = (tall_at(1.0), tall_at(3.0));
        assert!(one > 40.0, "the picture is drawn: {one}");
        assert!(three > one * 2.5, "at 300% the picture is about three times as tall as at 100%: {one} -> {three}");
    }

    #[test]
    fn drawing_every_block_kind_does_not_panic() {
        let md = "# H\n\npara **b** *i* ~~s~~ `c` [l](https://x) $m$ ![a](assets/a.png)\n\n- [ ] t\n- [x] d\n1. n\n\n> q\n\n```mermaid\ngraph TD;\n```\n\n$$\nE\n$$\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n---\n\n![alt](assets/x.png)\n";
        let blocks = blocks_for(PageFormat::Cmgfm, md);
        let ctx = egui::Context::default();
        crate::pictures::install(&ctx);
        let tokens = crate::theme::Theme { mode: crate::theme::Mode::Dark, accent: crate::theme::Accent::Default }.tokens();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            show(ui, &blocks, Path::new("."), &tokens, Some(Dir::Ltr), &Justified::default());
        });
        out.textures_delta.clear();
    }

    #[test]
    fn list_marker_size_follows_effective_body_font_size_and_zoom() {
        let empty: [Span; 0] = [];
        let default_span = [Span { text: "default".into(), ..Span::default() }];
        let larger_span = [Span { text: "larger".into(), font_size: Some(18), ..Span::default() }];
        let huge_span = [Span { text: "huge".into(), font_size: Some(24), ..Span::default() }];

        // Default size at 100% zoom matches base preview body size (14.5).
        assert_eq!(item_marker_size(&empty, 1.0), 14.5);
        assert_eq!(item_marker_size(&default_span, 1.0), 14.5);

        // Larger sizes scale proportionally with font size in points relative to 11 pt.
        assert_eq!(item_marker_size(&larger_span, 1.0), 14.5 * 18.0 / 11.0);
        assert_eq!(item_marker_size(&huge_span, 1.0), 14.5 * 24.0 / 11.0);

        // Zoom scales proportionally for both default and custom font sizes.
        assert_eq!(item_marker_size(&default_span, 1.5), 14.5 * 1.5);
        assert_eq!(item_marker_size(&larger_span, 2.0), 14.5 * 2.0 * 18.0 / 11.0);

        // An inline image before the text does not prevent finding the text font size.
        let with_image =
            [Span { image: Some("assets/icon.png".into()), ..Span::default() }, Span { text: "item".into(), font_size: Some(20), ..Span::default() }];
        assert_eq!(item_marker_size(&with_image, 1.0), 14.5 * 20.0 / 11.0);
    }

    #[test]
    fn list_items_render_with_marker_scaled_for_ordered_and_unordered() {
        let tokens = crate::theme::Theme::DEFAULT.tokens();
        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);

        let make_item = |ordered: bool, font_size: Option<u8>| Block::Item {
            ordered,
            number: 1,
            task: None,
            depth: 0,
            spans: vec![Span { text: "Item text".into(), font_size, ..Span::default() }],
            emoji: Vec::new(),
        };

        let height_of = |block: &Block, zoom: f32| {
            let mut height = 0.0;
            let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
                let out =
                    show_block_zoomed(ui, block, 0, Path::new("."), &tokens, false, Some(Dir::Ltr), None, None, ui.id().with("item_test"), zoom);
                height = out.response.rect.height();
            });
            frame.textures_delta.clear();
            height
        };

        // Unordered list: default size vs larger size.
        let bullet_default = height_of(&make_item(false, None), 1.0);
        let bullet_large = height_of(&make_item(false, Some(24)), 1.0);
        assert!(bullet_default > 10.0, "default bullet item renders: {bullet_default}");
        assert!(bullet_large > bullet_default * 1.3, "larger bullet item is taller: {bullet_default} -> {bullet_large}");

        // Ordered list: default size vs larger size.
        let number_default = height_of(&make_item(true, None), 1.0);
        let number_large = height_of(&make_item(true, Some(24)), 1.0);
        assert!(number_default > 10.0, "default numbered item renders: {number_default}");
        assert!(number_large > number_default * 1.3, "larger numbered item is taller: {number_default} -> {number_large}");

        // Zoom scaling increases rendered height.
        let bullet_zoomed = height_of(&make_item(false, Some(24)), 2.0);
        assert!(bullet_zoomed > bullet_large * 1.6, "zooming scales item height: {bullet_large} -> {bullet_zoomed}");

        // Nested and RTL items render without panicking.
        let rtl_nested = Block::Item {
            ordered: false,
            number: 1,
            task: None,
            depth: 2,
            spans: vec![Span { text: "مرحبا".into(), font_size: Some(18), ..Span::default() }],
            emoji: Vec::new(),
        };
        let rtl_height = height_of(&rtl_nested, 1.0);
        assert!(rtl_height > 10.0, "rtl nested item renders: {rtl_height}");
    }

    #[test]
    fn rendered_images_are_recorded_for_standalone_and_inline_images() {
        let t = notecraft_store::testing::TempDir::new("preview-img-hits");
        let page_dir = t.path();
        let assets_dir = page_dir.join("assets");
        std::fs::create_dir_all(&assets_dir).expect("assets");
        let png_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDRtest";
        std::fs::write(assets_dir.join("pic.png"), png_bytes).expect("write pic.png");
        std::fs::write(assets_dir.join("inline.png"), png_bytes).expect("write inline.png");

        let ctx = egui::Context::default();
        crate::fonts::install_once(&ctx);
        let tokens = crate::theme::Theme::DEFAULT.tokens();

        let blocks = [
            Block::Image { alt: "Test picture".into(), src: "assets/pic.png".into(), width: Some(100.0), height: Some(80.0), emoji: Vec::new() },
            Block::Paragraph(
                vec![Span { text: "Some text ".into(), ..Span::default() }, Span { image: Some("assets/inline.png".into()), ..Span::default() }],
                Vec::new(),
            ),
        ];

        let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
            for (i, block) in blocks.iter().enumerate() {
                let id = ui.id().with(("block", i));
                let _ = show_block_zoomed(ui, block, i, page_dir, &tokens, false, Some(Dir::Ltr), None, None, id, 1.0);
            }
        });
        frame.textures_delta.clear();

        let images = ctx.data(|d| d.get_temp::<Vec<RenderedImage>>(rendered_images_id())).unwrap_or_default();
        assert_eq!(images.len(), 2, "both standalone and inline image are recorded");
        assert_eq!(images[0].block, 0);
        assert_eq!(images[0].src, "assets/pic.png");
        assert!(images[0].rect.width() > 0.0 && images[0].rect.height() > 0.0);
        assert_eq!(images[1].block, 1);
        assert_eq!(images[1].src, "assets/inline.png");
        assert!(images[1].rect.width() > 0.0 && images[1].rect.height() > 0.0);
    }

    #[test]
    fn a_box_in_picture_pixels_lands_on_the_drawn_picture_in_the_same_corners() {
        let drawn = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(200.0, 100.0));
        // A 100 x 50 picture drawn twice as large: every pixel is two points.
        let quad = [[0.0, 0.0], [50.0, 0.0], [50.0, 25.0], [0.0, 25.0]];
        let corners = box_on_screen(quad, drawn, (100, 50));
        assert_eq!(corners, [egui::pos2(10.0, 10.0), egui::pos2(110.0, 10.0), egui::pos2(110.0, 60.0), egui::pos2(10.0, 60.0)]);
    }
}

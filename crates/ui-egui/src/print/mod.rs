//! File > Print and Ctrl+P (issue #168): the open page, or every page of its section, laid out on
//! sheets of paper, shown in a preview, saved as a PDF and sent to a printer.
//!
//! The note is drawn by the window's own drawing code ([`crate::containers::show_ink_view`], the
//! page's ink, its title) into a context of its own at the paper's printable width, then painted
//! into pixels ([`raster`]) and cut into sheets. So a sheet shows what the page shows: words,
//! tables, pictures, positioned boxes and ink, and everything below the window's edge as well.
//!
//! The infinite canvas: boxes and ink may reach further right than a sheet is wide. With
//! "Fit to the paper's width" on (the default) the whole note is drawn smaller until it fits; with
//! it off, the note is drawn at its own size and the dialog says that what lies beyond the right
//! margin is cut off, so nothing is lost without a word. Downwards a note runs over as many sheets
//! as it needs, and a sheet ends at a blank row where there is one, so a line of words is not cut
//! in two.

pub mod pdf;
pub mod raster;

use crate::theme::Tokens;
use egui::{Align2, Context, vec2};
use notecraft_model::paper::{Paper, Size};
use raster::{Canvas, RgbImage, Textures};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};

/// The resolution sheets are drawn at, in dots per inch.
pub const DPI: f32 = 200.0;
/// The most sheets one print makes, so a runaway page cannot fill the memory.
pub const MAX_SHEETS: usize = 400;

/// Millimetres in PDF points (1/72 inch).
pub fn mm_to_pt(mm: f32) -> f32 {
    mm * 72.0 / 25.4
}

/// How the note goes on the paper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub paper: Paper,
    /// Draw a note wider than the paper smaller, so all of it fits across.
    pub fit_width: bool,
    pub dpi: f32,
}

impl Layout {
    /// The sheet's size in points.
    pub fn sheet_pt(&self) -> (f32, f32) {
        let (w, h) = self.paper.sheet_mm();
        (mm_to_pt(w), mm_to_pt(h))
    }

    /// The width and height inside the margins, in points.
    pub fn printable_pt(&self) -> (f32, f32) {
        let (w, h) = self.paper.sheet_mm();
        let [t, r, b, l] = self.paper.margins_mm();
        (mm_to_pt(w - l - r).max(36.0), mm_to_pt(h - t - b).max(36.0))
    }
}

/// One page drawn on its sheets.
pub struct PrintedPage {
    pub sheets: Vec<RgbImage>,
    /// The note was wider than the paper and was not fitted: what lay beyond the right margin is
    /// not on the sheets.
    pub cut_off: bool,
}

/// Draws the page in `dir` on sheets laid out as `layout` says.
pub fn print_page(dir: &Path, layout: &Layout) -> Result<PrintedPage, String> {
    let page = notecraft_store::page::load_page(dir).map_err(|e| e.to_string())?;
    let (blocks, in_the_text) = notecraft_formats::boxes::read(page.format, &page.body);
    let older = notecraft_store::page::containers(dir).unwrap_or_default();
    let arranged = in_the_text.is_some() || !older.is_empty();
    let mut boxes = in_the_text.unwrap_or(older);
    let columns = notecraft_store::page::table_columns(dir).unwrap_or_default();
    let justified = notecraft_store::page::justified(dir).unwrap_or_default();
    let pad = crate::inkpad::Pad::of_page(dir);
    let ink = pad.drawing().strokes.clone();
    let ink_bounds = pad.bounds();
    let title = if page.has_title { page.title.clone() } else { String::new() };
    let tokens = crate::theme::Theme::DEFAULT.tokens();
    let (width_pt, height_pt) = layout.printable_pt();
    let ppp = (layout.dpi / 72.0).max(0.5);
    // A page that was never arranged is one box as wide as the page it is shown on: here, the paper.
    if !arranged && boxes.len() == 1 {
        boxes[0].width = crate::containers::responsive_default_width(width_pt, 1.0);
    }

    let ctx = egui::Context::default();
    crate::fonts::install_once(&ctx);
    crate::pictures::install(&ctx);
    ctx.set_visuals(crate::theme::Theme::DEFAULT.visuals());
    // Paper shows the note, not egui's debugging marks or the page's own controls. egui has the
    // marks, and the style field that turns them off, in debug builds only.
    #[cfg(debug_assertions)]
    ctx.global_style_mut(|style| style.debug = egui::style::DebugOptions { show_unaligned: false, ..Default::default() });
    ctx.data_mut(|data| data.insert_temp(printing_id(), true));
    let mut textures = Textures::default();
    let mut screen_h = (height_pt * 4.0).max(2000.0);
    let mut zoom = 1.0f32;
    let mut measured_once = false;
    loop {
        let mut extent = (0.0f32, 0.0f32);
        let mut output = None;
        // A few frames: the first lays out and asks for the pictures, which arrive on threads of
        // their own; the page is drawn again until nothing more is waiting.
        for frame in 0..240 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(width_pt, screen_h))),
                viewports: std::iter::once((egui::ViewportId::ROOT, egui::ViewportInfo { native_pixels_per_point: Some(ppp), ..Default::default() }))
                    .collect(),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                ui.set_width(width_pt);
                if !title.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&title).size(26.0 * zoom).color(tokens.text)).wrap());
                    ui.add_space(10.0 * zoom);
                }
                let top = ui.cursor().min;
                let moved = crate::containers::show_ink_view(ui, &blocks, &mut boxes, dir, &tokens, page.dir, &justified, &columns, &title, zoom);
                for stroke in &ink {
                    crate::inkpad::paint_zoomed(ui.painter(), top, stroke, zoom);
                }
                let (mut right, mut bottom) = (moved.right, moved.bottom);
                if let Some([_, _, x1, y1]) = ink_bounds {
                    right = right.max(x1);
                    bottom = bottom.max(y1);
                }
                extent = (right, top.y + bottom * zoom);
            });
            textures.apply(&out.textures_delta);
            out.textures_delta.clear();
            let waiting = out.viewport_output.get(&egui::ViewportId::ROOT).is_some_and(|v| v.repaint_delay.is_zero());
            output = Some(out);
            if frame >= 2 && !waiting {
                break;
            }
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let Some(out) = output else { return Err("nothing was drawn".into()) };
        // The first layout says how wide the note is; a note wider than the paper is drawn again
        // smaller, when it is to fit.
        if !measured_once {
            measured_once = true;
            if layout.fit_width && extent.0 > width_pt {
                zoom = (width_pt / extent.0).clamp(0.05, 1.0);
                continue;
            }
        }
        let bottom = extent.1 + 4.0;
        if bottom > screen_h && screen_h < height_pt * MAX_SHEETS as f32 {
            screen_h = (bottom * 1.5).min(height_pt * MAX_SHEETS as f32);
            continue;
        }
        let primitives = ctx.tessellate(out.shapes, ppp);
        let (canvas_w, canvas_h) = ((width_pt * ppp).round().max(1.0) as usize, (bottom.min(screen_h) * ppp).ceil().max(1.0) as usize);
        let mut canvas = Canvas::white(canvas_w, canvas_h);
        canvas.paint(&primitives, &textures, ppp);
        let content = canvas.into_rgb();
        let cut_off = !layout.fit_width && extent.0 > width_pt + 0.5;
        return Ok(PrintedPage { sheets: paginate(&content, layout), cut_off });
    }
}

fn printing_id() -> egui::Id {
    egui::Id::new("notecraft-printing")
}

/// Whether `ctx` is drawing a note for print: the page's controls (a block's add-emoji button)
/// keep their room but are not drawn.
pub fn is_printing(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(printing_id())).unwrap_or(false)
}

/// Cuts a long picture of a note into sheets: each sheet holds as much as fits inside its
/// margins, and ends at the last blank row in its lowest fifth when there is one.
pub fn paginate(content: &RgbImage, layout: &Layout) -> Vec<RgbImage> {
    let scale = layout.dpi / 72.0;
    let (sheet_w, sheet_h) = layout.sheet_pt();
    let (sheet_w, sheet_h) = ((sheet_w * scale).round() as usize, (sheet_h * scale).round() as usize);
    let [t, _, _, l] = layout.paper.margins_mm();
    let (left, top) = ((mm_to_pt(l) * scale).round() as usize, (mm_to_pt(t) * scale).round() as usize);
    let room = (layout.printable_pt().1 * scale).floor().max(1.0) as usize;
    let mut sheets = Vec::new();
    let mut start = 0;
    while start < content.height && sheets.len() < MAX_SHEETS {
        let mut end = (start + room).min(content.height);
        if end < content.height {
            let lowest = end.saturating_sub(room / 5).max(start + 1);
            if let Some(blank) = (lowest..end).rev().find(|&y| content.row_is_blank(y)) {
                end = blank + 1;
            }
        }
        let mut sheet = RgbImage::white(sheet_w, sheet_h);
        sheet.put(content, start..end, (left, top));
        sheets.push(sheet);
        start = end;
    }
    // What is left below the last mark is blank paper, not a sheet of its own.
    while sheets.len() > 1 && sheets.last().is_some_and(|s| (0..s.height).all(|y| s.row_is_blank(y))) {
        sheets.pop();
    }
    if sheets.is_empty() {
        sheets.push(RgbImage::white(sheet_w, sheet_h));
    }
    sheets
}

/// The pages a print of `scope` holds, in order: the open page, or every page and subpage of its
/// section, as the section lists them.
pub fn pages_of(scope: Scope, page: &Path) -> Vec<PathBuf> {
    match scope {
        Scope::Page => vec![page.to_path_buf()],
        Scope::Section => {
            let section = page
                .ancestors()
                .find(|p| notecraft_store::tree::read_node(p).is_some_and(|n| n.kind == notecraft_model::NodeKind::Section))
                .map(Path::to_path_buf);
            let Some(section) = section else { return vec![page.to_path_buf()] };
            let mut pages = Vec::new();
            fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
                for child in notecraft_store::tree::child_dirs(dir) {
                    if notecraft_store::tree::page_file(&child).is_some() {
                        out.push(child.clone());
                        walk(&child, out);
                    }
                }
            }
            walk(&section, &mut pages);
            if pages.is_empty() { vec![page.to_path_buf()] } else { pages }
        }
    }
}

/// What is printed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The open page.
    Page,
    /// Every page of the open page's section, subpages included.
    Section,
}

/// News from the drawing thread.
enum News {
    /// Pages drawn so far, of all.
    Page(usize, usize),
    Done(Result<(Vec<RgbImage>, bool), String>),
}

/// News from a print job sent to a printer.
#[cfg(any(target_os = "linux", target_os = "macos"))]
type Sent = Receiver<Result<String, String>>;

/// The Print dialog while it is open.
pub struct Dialog {
    page: PathBuf,
    title: String,
    pub scope: Scope,
    pub layout: Layout,
    /// What was asked to be drawn last; the sheets are drawn again when the choices change.
    drawn_for: Option<(Scope, Layout)>,
    news: Option<Receiver<News>>,
    progress: (usize, usize),
    sheets: Vec<RgbImage>,
    cut_off: bool,
    failed: Option<String>,
    shown: usize,
    preview: Option<(usize, egui::TextureHandle)>,
    status: Option<String>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    printers: Vec<String>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    printer: Option<String>,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    copies: u32,
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    sending: Option<Sent>,
}

/// What a frame of the dialog asked for.
#[derive(Default)]
pub struct Outcome {
    pub closed: bool,
    /// Save this PDF, with this file name offered.
    pub save_pdf: Option<(Vec<u8>, String)>,
    /// Something to say in the status bar.
    pub said: Option<String>,
}

impl Dialog {
    /// The dialog for the page in `page`: on the page's own paper when it has one, else on the
    /// paper printed on last (`remembered`), else on A4.
    pub fn new(page: &Path, remembered: Option<Paper>) -> Dialog {
        let paper = notecraft_store::page::paper_size(page).ok().flatten().or(remembered).unwrap_or_else(|| Paper::new(Size::A4));
        let title = notecraft_store::page::load_page(page).map(|p| p.title).unwrap_or_default();
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let printers: Vec<String> = craft_print::printers().into_iter().map(|p| p.name).collect();
        Dialog {
            page: page.to_path_buf(),
            title,
            scope: Scope::Page,
            layout: Layout { paper, fit_width: true, dpi: DPI },
            drawn_for: None,
            news: None,
            progress: (0, 0),
            sheets: Vec::new(),
            cut_off: false,
            failed: None,
            shown: 0,
            preview: None,
            status: None,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            printer: None,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            printers,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            copies: 1,
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            sending: None,
        }
    }

    /// Whether the sheets are being drawn.
    pub fn drawing(&self) -> bool {
        self.news.is_some()
    }

    /// The sheets drawn, once they are.
    pub fn sheets(&self) -> &[RgbImage] {
        if self.drawing() { &[] } else { &self.sheets }
    }

    /// The sheets as a PDF.
    pub fn pdf(&self) -> Vec<u8> {
        let (w, h) = self.layout.sheet_pt();
        let pages: Vec<pdf::Page<'_>> = self.sheets.iter().map(|image| pdf::Page { image, width_pt: w, height_pt: h }).collect();
        pdf::write(&pages, &self.title)
    }

    /// Starts drawing the sheets for the choices as they are now.
    fn draw(&mut self) {
        let scope = self.scope;
        let layout = self.layout;
        self.drawn_for = Some((scope, layout));
        self.failed = None;
        self.preview = None;
        self.shown = 0;
        let pages = pages_of(scope, &self.page);
        self.progress = (0, pages.len());
        let (tell, news) = std::sync::mpsc::channel();
        let work = move || {
            let mut sheets = Vec::new();
            let mut cut_off = false;
            let total = pages.len();
            for (i, page) in pages.iter().enumerate() {
                match print_page(page, &layout) {
                    Ok(printed) => {
                        sheets.extend(printed.sheets);
                        cut_off |= printed.cut_off;
                    }
                    Err(e) => {
                        let _ = tell.send(News::Done(Err(e)));
                        return;
                    }
                }
                let _ = tell.send(News::Page(i + 1, total));
                if sheets.len() >= MAX_SHEETS {
                    sheets.truncate(MAX_SHEETS);
                    break;
                }
            }
            let _ = tell.send(News::Done(Ok((sheets, cut_off))));
        };
        let work: Box<dyn FnOnce() + Send> = Box::new(work);
        let work = std::sync::Arc::new(std::sync::Mutex::new(Some(work)));
        let run = |work: &std::sync::Mutex<Option<Box<dyn FnOnce() + Send>>>| {
            if let Some(work) = work.lock().ok().and_then(|mut w| w.take()) {
                work();
            }
        };
        let in_thread = std::sync::Arc::clone(&work);
        // A browser has no threads to start: there the sheets are drawn at once.
        if std::thread::Builder::new().name("notecraft-print".into()).spawn(move || run(&in_thread)).is_err() {
            run(&work);
        }
        self.news = Some(news);
        self.take_news();
    }

    fn take_news(&mut self) {
        let Some(news) = &self.news else { return };
        loop {
            match news.try_recv() {
                Ok(News::Page(done, total)) => self.progress = (done, total),
                Ok(News::Done(result)) => {
                    self.news = None;
                    match result {
                        Ok((sheets, cut_off)) => {
                            self.sheets = sheets;
                            self.cut_off = cut_off;
                        }
                        Err(e) => {
                            self.sheets.clear();
                            self.failed = Some(e);
                        }
                    }
                    return;
                }
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.news = None;
                    self.failed = Some(crate::tl_id!("ui.print_drawing_stopped").to_string());
                    return;
                }
            }
        }
    }

    pub fn show(&mut self, ctx: &Context, t: &Tokens) -> Outcome {
        let mut out = Outcome::default();
        if self.drawn_for != Some((self.scope, self.layout)) && !self.drawing() {
            self.draw();
        }
        self.take_news();
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Some(sending) = &self.sending {
            match sending.try_recv() {
                Ok(Ok(_)) => {
                    self.sending = None;
                    self.status = Some(crate::tl_id!("ui.print_sent").to_string());
                    out.said = self.status.clone();
                }
                Ok(Err(e)) => {
                    self.sending = None;
                    self.status = Some(crate::i18n::format_message_id("ui.print_send_failed", &[("error", &e)]));
                }
                Err(TryRecvError::Disconnected) => self.sending = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if self.drawing() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if self.sending.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let backdrop = crate::app::modal_backdrop(ctx, "print-modal-backdrop");
        let screen = ctx.content_rect();
        let width = (screen.width() - 52.0).clamp(1.0, 760.0);
        let title = crate::tl_id!("ui.print");
        egui::Window::new(title)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(crate::app::modal_frame(t))
            .max_width(width)
            .show(ctx, |ui| {
                crate::app::stay_above(ui, backdrop);
                ui.set_width(width);
                crate::app::modal_title(ui, t, title);
                let narrow = width < 560.0;
                let mut body = |ui: &mut egui::Ui| {
                    ui.vertical(|ui| {
                        ui.set_width(if narrow { width } else { 240.0 });
                        self.choices(ui);
                    });
                    ui.vertical(|ui| {
                        self.preview(ui, t, if narrow { width } else { width - 256.0 }, (screen.height() * 0.55).clamp(160.0, 520.0));
                    });
                };
                if narrow {
                    egui::ScrollArea::vertical().max_height((screen.height() - 160.0).max(120.0)).show(ui, |ui| {
                        ui.vertical(&mut body);
                    });
                } else {
                    ui.horizontal_top(&mut body);
                }
                if let Some(status) = &self.status {
                    ui.add_space(6.0);
                    ui.label(status);
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if crate::app::modal_button(ui, t, crate::tl_id!("ui.close"), false).clicked() {
                        out.closed = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ready = !self.drawing() && self.failed.is_none() && !self.sheets.is_empty();
                        #[cfg(any(target_os = "linux", target_os = "macos"))]
                        if ui
                            .add_enabled_ui(ready && self.sending.is_none(), |ui| {
                                crate::app::modal_button(ui, t, crate::tl_id!("ui.print_send"), true)
                            })
                            .inner
                            .clicked()
                        {
                            self.send(ctx);
                        }
                        #[cfg(target_os = "windows")]
                        if ui.add_enabled_ui(ready, |ui| crate::app::modal_button(ui, t, crate::tl_id!("ui.print_send"), true)).inner.clicked() {
                            self.open_in_viewer();
                        }
                        if ui
                            .add_enabled_ui(ready, |ui| crate::app::modal_button(ui, t, crate::tl_id!("ui.print_save_as_pdf"), false))
                            .inner
                            .clicked()
                        {
                            out.save_pdf = Some((self.pdf(), pdf_name(&self.title)));
                        }
                    });
                });
            });
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            out.closed = true;
        }
        out
    }

    fn choices(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new(crate::tl_id!("ui.print_what")).strong());
        ui.radio_value(&mut self.scope, Scope::Page, crate::tl_id!("ui.print_this_page"));
        ui.radio_value(&mut self.scope, Scope::Section, crate::tl_id!("ui.print_this_section"));
        ui.add_space(8.0);
        ui.label(egui::RichText::new(crate::tl_id!("ui.print_paper")).strong());
        let current = self.layout.paper.size;
        egui::ComboBox::from_id_salt("print-paper-size").selected_text(current.label()).show_ui(ui, |ui| {
            for size in Size::STANDARD {
                ui.selectable_value(&mut self.layout.paper.size, size, size.label());
            }
            if let Size::Custom(..) = current {
                ui.selectable_value(&mut self.layout.paper.size, current, current.label());
            }
        });
        ui.horizontal(|ui| {
            ui.radio_value(&mut self.layout.paper.landscape, false, crate::tl_id!("ui.print_portrait"));
            ui.radio_value(&mut self.layout.paper.landscape, true, crate::tl_id!("ui.print_landscape"));
        });
        ui.add_space(4.0);
        ui.label(crate::tl_id!("ui.print_margins_mm"));
        let mut margins = self.layout.paper.margins_mm();
        egui::Grid::new("print-margins").num_columns(4).show(ui, |ui| {
            for (i, name) in
                [crate::tl_id!("ui.print_top"), crate::tl_id!("ui.print_right"), crate::tl_id!("ui.print_bottom"), crate::tl_id!("ui.print_left")]
                    .into_iter()
                    .enumerate()
            {
                ui.label(name);
                ui.add(egui::DragValue::new(&mut margins[i]).range(0.0..=60.0).speed(0.5).max_decimals(1));
                if i % 2 == 1 {
                    ui.end_row();
                }
            }
        });
        if margins != self.layout.paper.margins_mm() {
            self.layout.paper.margins = margins;
        }
        ui.add_space(4.0);
        ui.checkbox(&mut self.layout.fit_width, crate::tl_id!("ui.print_fit_width"));
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(crate::tl_id!("ui.print_printer")).strong());
            let shown = self.printer.clone().unwrap_or_else(|| crate::tl_id!("ui.print_default_printer").to_string());
            egui::ComboBox::from_id_salt("print-printer").selected_text(shown).width(220.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.printer, None, crate::tl_id!("ui.print_default_printer"));
                for name in &self.printers {
                    ui.selectable_value(&mut self.printer, Some(name.clone()), name);
                }
            });
            if self.printers.is_empty() {
                ui.weak(crate::tl_id!("ui.print_no_printers"));
            }
            ui.horizontal(|ui| {
                ui.label(crate::tl_id!("ui.print_copies"));
                ui.add(egui::DragValue::new(&mut self.copies).range(1..=99));
            });
        }
    }

    fn preview(&mut self, ui: &mut egui::Ui, t: &Tokens, width: f32, height: f32) {
        if self.drawing() {
            ui.horizontal(|ui| {
                ui.spinner();
                let (done, total) = self.progress;
                ui.label(if total > 1 {
                    crate::i18n::format_message_id(
                        "ui.print_drawing_page_of",
                        &[("done", &(done + 1).min(total).to_string()), ("total", &total.to_string())],
                    )
                } else {
                    crate::tl_id!("ui.print_drawing").to_string()
                });
            });
            return;
        }
        if let Some(why) = &self.failed {
            ui.colored_label(t.warn, crate::i18n::format_message_id("ui.print_failed", &[("error", why)]));
            return;
        }
        let count = self.sheets.len();
        if count == 0 {
            return;
        }
        self.shown = self.shown.min(count - 1);
        if self.preview.as_ref().is_none_or(|(n, _)| *n != self.shown) {
            let sheet = &self.sheets[self.shown];
            let small = sheet.shrunk((sheet.width as f32 / 520.0).ceil().max(1.0) as usize);
            let image = egui::ColorImage::from_rgb([small.width, small.height], &small.bytes);
            self.preview = Some((self.shown, ui.ctx().load_texture("print-preview", image, egui::TextureOptions::LINEAR)));
        }
        if let Some((_, texture)) = &self.preview {
            let size = texture.size_vec2();
            let scale = (width / size.x).min(height / size.y).min(1.0);
            let response = ui.add(egui::Image::new((texture.id(), size * scale)).bg_fill(egui::Color32::WHITE));
            ui.painter().rect_stroke(response.rect, 0.0, egui::Stroke::new(1.0, t.container_border), egui::StrokeKind::Outside);
            response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Image, true, crate::tl_id!("ui.print_preview")));
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(self.shown > 0, egui::Button::new(crate::tl_id!("ui.print_previous_sheet"))).clicked() {
                self.shown -= 1;
            }
            ui.label(crate::i18n::format_message_id("ui.print_sheet_of", &[("sheet", &(self.shown + 1).to_string()), ("total", &count.to_string())]));
            if ui.add_enabled(self.shown + 1 < count, egui::Button::new(crate::tl_id!("ui.print_next_sheet"))).clicked() {
                self.shown += 1;
            }
        });
        if self.cut_off {
            ui.colored_label(t.warn, crate::tl_id!("ui.print_cut_off"));
        }
    }

    /// Sends the sheets to the chosen printer, on a thread of its own.
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    fn send(&mut self, ctx: &Context) {
        let pdf = self.pdf();
        let job = craft_print::Job {
            printer: self.printer.clone(),
            copies: self.copies.max(1),
            collate: true,
            title: if self.title.is_empty() { "NoteCraft".to_string() } else { self.title.clone() },
            ..Default::default()
        };
        let (tell, sent) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("notecraft-print-job".into()).spawn(move || {
            let _ = tell.send(craft_print::submit(&pdf, &job).map_err(|e| e.to_string()));
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.sending = Some(sent);
                self.status = Some(crate::tl_id!("ui.print_sending").to_string());
            }
            Err(e) => self.status = Some(crate::i18n::format_message_id("ui.print_send_failed", &[("error", &e.to_string())])),
        }
    }
}

impl Dialog {
    /// Windows: the sheets go to the system's PDF viewer as a file in the temporary folder, and are
    /// printed from there, with the printer's own dialog.
    #[cfg(target_os = "windows")]
    fn open_in_viewer(&mut self) {
        let stamp = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default();
        let file = std::env::temp_dir().join(format!("notecraft-print-{stamp}-{}", pdf_name(&self.title)));
        let opened = std::fs::write(&file, self.pdf()).and_then(|()| crate::app::open_file_for_viewer(&file));
        self.status = Some(match opened {
            Ok(()) => crate::tl_id!("ui.print_opened_in_viewer").to_string(),
            Err(e) => crate::i18n::format_message_id("ui.print_send_failed", &[("error", &e.to_string())]),
        });
    }
}

/// The file name a saved PDF is offered under: the page's title, with what file names cannot hold
/// left out.
pub fn pdf_name(title: &str) -> String {
    let clean: String = title.chars().filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') && !c.is_control()).collect();
    let clean = clean.trim();
    format!("{}.pdf", if clean.is_empty() { "NoteCraft" } else { clean })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(paper: Paper) -> Layout {
        Layout { paper, fit_width: true, dpi: 72.0 }
    }

    #[test]
    fn a_long_note_runs_over_sheets_and_breaks_at_a_blank_row() {
        let mut paper = Paper::new(Size::A6);
        paper.margins = [10.0; 4];
        let layout = layout(paper);
        let (w, h) = layout.printable_pt();
        let (w, h) = (w as usize, h as usize);
        // Three sheets' worth of dark rows with a blank row a little above the first sheet's end.
        let mut content = RgbImage::white(w, h * 3);
        for y in 0..content.height {
            if y != h - 10 {
                content.bytes[y * w * 3] = 0;
            }
        }
        let sheets = paginate(&content, &layout);
        assert!(sheets.len() >= 3, "{} sheets", sheets.len());
        let (sw, sh) = layout.sheet_pt();
        assert_eq!((sheets[0].width, sheets[0].height), (sw.round() as usize, sh.round() as usize), "a sheet is the paper's size");
        // The first sheet ends at the blank row: the next one starts with what was below it.
        let left = mm_to_pt(10.0).round() as usize;
        let top = mm_to_pt(10.0).round() as usize;
        let first_marked = |s: &RgbImage| (0..s.height).find(|&y| s.bytes[(y * s.width + left) * 3] == 0);
        assert_eq!(first_marked(&sheets[1]), Some(top), "the second sheet starts inside its margin");
        let last_on_first = (0..sheets[0].height).rev().find(|&y| sheets[0].bytes[(y * sheets[0].width + left) * 3] == 0);
        assert_eq!(last_on_first, Some(top + h - 11), "the first sheet stops above the blank row");
    }

    #[test]
    fn blank_paper_below_the_note_is_not_a_sheet() {
        let layout = layout(Paper::new(Size::A6));
        let (w, h) = layout.printable_pt();
        let content = RgbImage::white(w as usize, h as usize * 2);
        assert_eq!(paginate(&content, &layout).len(), 1, "an empty note still prints one sheet");
    }

    #[test]
    fn landscape_turns_the_sheet_and_names_are_safe() {
        let mut paper = Paper::new(Size::A4);
        let portrait = layout(paper).sheet_pt();
        paper.landscape = true;
        let landscape = layout(paper).sheet_pt();
        assert!(portrait.1 > portrait.0 && landscape.0 > landscape.1);
        assert_eq!(pdf_name("Plans: 2027/Q1?"), "Plans 2027Q1.pdf");
        assert_eq!(pdf_name("  "), "NoteCraft.pdf");
    }

    // ---------- real pages, drawn as the window draws them ----------

    fn session(t: &notecraft_store::testing::TempDir) -> notecraft_engine::Session {
        let settings = notecraft_store::Settings::at(t.path().join("settings.toml"));
        notecraft_engine::Session::new(&t.path().join("notebooks"), settings).expect("session")
    }

    /// How many pixels of `sheet` are dark, in its rows `rows`.
    fn dark(sheet: &RgbImage, rows: std::ops::Range<usize>) -> usize {
        rows.flat_map(|y| (0..sheet.width).map(move |x| (x, y))).filter(|&(x, y)| sheet.bytes[(y * sheet.width + x) * 3] < 128).count()
    }

    #[test]
    fn a_page_prints_its_title_its_words_its_picture_and_its_ink() {
        let t = notecraft_store::testing::TempDir::new("print-page");
        let mut s = session(&t);
        let sample = notecraft_engine::sample::create_sample(&mut s, Some(crate::LOGO_PNG)).expect("sample");
        // The Markdown sample page: words, a table, tasks, equations, a diagram and a picture.
        let page = sample.md_page.clone();
        // A line of ink far down the page, below anything a window shows without scrolling.
        let mut drawing = notecraft_ink::Drawing::new(800.0, 2000.0);
        let mut stroke = notecraft_ink::Stroke { width: 6.0, ..Default::default() };
        for x in 0..200 {
            stroke.dabs.push(notecraft_ink::Dab::new(20.0 + x as f32, 1400.0, 1.0));
        }
        drawing.strokes.push(stroke);
        std::fs::write(page.join(crate::inkpad::FILE), notecraft_ink::svg::write(&drawing, notecraft_ink::svg::Flavour::Editable)).expect("ink");
        let layout = Layout { paper: Paper::new(Size::A4), fit_width: true, dpi: 72.0 };
        let printed = print_page(&page, &layout).expect("printed");
        // Review pictures, never asserted on: the sheets as they print, at print resolution.
        if let Some(dir) = std::env::var_os("NOTECRAFT_UI_VISUAL_DIR") {
            let dir = PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(sharp) = print_page(&page, &Layout { dpi: DPI, ..layout }) {
                for (i, sheet) in sharp.sheets.iter().enumerate() {
                    if let Some(image) = image::RgbImage::from_raw(sheet.width as u32, sheet.height as u32, sheet.bytes.clone()) {
                        let _ = image.save(dir.join(format!("print-sheet-{}.png", i + 1)));
                    }
                }
            }
        }
        assert!(printed.sheets.len() >= 2, "the ink 1400 points down is on a later sheet: {} sheets", printed.sheets.len());
        let first = &printed.sheets[0];
        let [top, _, _, _] = layout.paper.margins_mm();
        let top = mm_to_pt(top).round() as usize;
        assert!(dark(first, 0..top) == 0, "nothing is drawn in the top margin");
        assert!(dark(first, top..top + 40) > 40, "the title and words are on the first sheet");
        let inked = printed.sheets.iter().skip(1).map(|s| dark(s, 0..s.height)).sum::<usize>();
        assert!(inked > 200, "the ink is printed: {inked} dark pixels after the first sheet");
        assert!(!printed.cut_off);
    }

    #[test]
    fn a_note_wider_than_the_paper_is_fitted_or_said_to_be_cut_off() {
        let t = notecraft_store::testing::TempDir::new("print-wide");
        let mut s = session(&t);
        let sample = notecraft_engine::sample::create_sample(&mut s, None).expect("sample");
        let page = sample.html_page.clone();
        let path = page.to_string_lossy().into_owned();
        let body = "<div class=\"nc-box\" data-x=\"0\" data-y=\"0\" data-width=\"200\"><p>Left</p></div>\n<div class=\"nc-box\" data-x=\"1400\" data-y=\"0\" data-width=\"200\"><p>Far right words</p></div>\n";
        s.run("page.save", &notecraft_engine::Params::from([("path".to_string(), path), ("body".to_string(), body.to_string())])).expect("page");
        let mut paper = Paper::new(Size::A5);
        paper.margins = [10.0; 4];
        let fitted = print_page(&page, &Layout { paper, fit_width: true, dpi: 200.0 }).expect("fitted");
        assert!(!fitted.cut_off, "fitted, nothing is cut off");
        let sheet = &fitted.sheets[0];
        let right_half: usize = (0..sheet.height)
            .flat_map(|y| (sheet.width / 2..sheet.width).map(move |x| (x, y)))
            .filter(|&(x, y)| sheet.bytes[(y * sheet.width + x) * 3] < 200)
            .count();
        assert!(right_half > 10, "the box far to the right is drawn, smaller, on the sheet");
        let cut = print_page(&page, &Layout { paper, fit_width: false, dpi: 72.0 }).expect("not fitted");
        assert!(cut.cut_off, "at its own size the far box does not fit, and the dialog says so");
    }

    #[test]
    fn a_section_prints_its_pages_and_subpages_in_order() {
        let t = notecraft_store::testing::TempDir::new("print-section");
        let mut s = session(&t);
        let sample = notecraft_engine::sample::create_sample(&mut s, None).expect("sample");
        let pages = pages_of(Scope::Section, &sample.html_page);
        assert!(pages.len() > 1, "{pages:?}");
        assert_eq!(pages.first(), Some(&sample.html_page), "the section's first page comes first");
        assert!(pages.iter().any(|p| p == &sample.deep_page), "subpages are in it");
        assert_eq!(pages_of(Scope::Page, &sample.html_page), vec![sample.html_page.clone()]);
    }
}

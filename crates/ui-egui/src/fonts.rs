//! The fonts the window draws with.
//!
//! egui bundles faces that cover Latin, Greek and Cyrillic and nothing else, so Arabic,
//! Hebrew, Devanagari, Chinese, Japanese and Korean letters come out as empty boxes.
//! NoteCraft adds four Noto faces (SIL Open Font License 1.1, texts in
//! `assets/fonts/OFL-*.txt`) as fallbacks behind the bundled ones: Latin text is unchanged,
//! and anything the first face has no glyph for falls through to the first of ours that has
//! it. The CJK face is 16 MB, most of the app's size; the owner chose to bundle it so a
//! notebook reads the same on every machine.
//!
//! Shaping itself is egui's: `epaint` shapes with harfrust, so once a face with the script is
//! present the letters join and a run of one direction is ordered correctly. A mixed line
//! goes through the bidi painter; see `docs/how-to/write-right-to-left.md`.

use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub mod system;

/// The name the Arabic face goes by inside egui; the others are in [`BUNDLED`].
pub const ARABIC_NAME: &str = "noto-naskh-arabic";

/// The faces NoteCraft bundles, in fallback order, with their bytes. The bytes live in the
/// text crate so the window and the text layer share one copy.
pub const BUNDLED: [(&str, &[u8]); 4] = notecraft_text::fonts::ALL;

pub fn named_family(name: &str) -> egui::FontFamily {
    egui::FontFamily::Name(format!("notecraft-system-font:{name}").into())
}

pub fn page_families(blocks: &[notecraft_formats::Block]) -> BTreeSet<String> {
    use notecraft_formats::Block;
    let mut families = BTreeSet::new();
    let mut collect = |spans: &[notecraft_formats::markdown::Span]| {
        families.extend(spans.iter().filter_map(|span| span.font_family.clone()));
    };
    for block in blocks {
        match block {
            Block::Paragraph(spans) | Block::Heading { spans, .. } | Block::Item { spans, .. } | Block::Quote(spans) => collect(spans),
            Block::Table { head, rows } => {
                for cell in head.iter().chain(rows.iter().flatten()) {
                    collect(cell);
                }
            }
            _ => {}
        }
    }
    families
}

/// One read of the installed fonts: the catalogue, or why it could not be read.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
type CatalogRead = Result<std::sync::Arc<system::Catalog>, String>;

/// The latest read, with the ticket it was started under. A read publishes only when its ticket is
/// newer than the one published, so a slow older read cannot replace a newer font list.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
static CATALOG: std::sync::Mutex<Option<(u64, std::sync::Arc<CatalogRead>)>> = std::sync::Mutex::new(None);

/// Hands out the tickets: each read takes the next one when it starts.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
static NEXT_TICKET: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn current() -> Option<std::sync::Arc<CatalogRead>> {
    CATALOG.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref().map(|(_, read)| read.clone())
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn take_ticket() -> u64 {
    NEXT_TICKET.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Publishes a read unless a newer one is already published. Returns the read that is current
/// afterwards.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn publish(ticket: u64, result: CatalogRead) -> std::sync::Arc<CatalogRead> {
    let read = std::sync::Arc::new(result);
    let mut published = CATALOG.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    match published.as_ref() {
        Some((newer, current)) if *newer > ticket => current.clone(),
        _ => {
            *published = Some((ticket, read.clone()));
            read
        }
    }
}

/// Reads the installed fonts on a background thread, so the window never waits for it. Listing
/// every font takes a while on some machines.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn start_catalog() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    STARTED.call_once(|| {
        let ticket = take_ticket();
        std::thread::spawn(move || {
            if current().is_none() {
                publish(ticket, system::Catalog::discover().map(std::sync::Arc::new));
            }
        });
    });
}

/// Reads the installed fonts again on a background thread, after a font was added. The list shows
/// the previous read until this one is ready, then asks the window to draw again.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn rescan_in_background(ctx: egui::Context) {
    let ticket = take_ticket();
    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let result = system::Catalog::discover().map(std::sync::Arc::new);
        let families = result.as_ref().map_or(0, |catalog| catalog.families().len());
        crate::usage::record(
            crate::usage::Level::Usage,
            "font_catalog_read",
            &[("families", &families.to_string()), ("ms", &started.elapsed().as_millis().to_string())],
        );
        publish(ticket, result);
        ctx.request_repaint();
    });
}

/// The installed fonts, or `None` while they are still being read. Never blocks.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn catalog_if_ready() -> Option<Result<std::sync::Arc<system::Catalog>, String>> {
    current().map(|read| (*read).clone())
}

/// The installed fonts. Waits for the background read if it is not finished (used when a font is
/// installed, which is rare).
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn system_catalog() -> Result<std::sync::Arc<system::Catalog>, String> {
    if let Some(read) = current() {
        return (*read).clone();
    }
    let ticket = take_ticket();
    (*publish(ticket, system::Catalog::discover().map(std::sync::Arc::new))).clone()
}

/// Faces read by background threads and not yet added to the font set: (family, loaded bytes or the
/// reason it failed). Added in one batch by the window, on its own thread.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
static FINISHED: std::sync::Mutex<Vec<(String, Result<egui::FontData, String>)>> = std::sync::Mutex::new(Vec::new());

/// Families whose bytes a background thread is reading now, so they are not requested twice.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
static LOADING: std::sync::Mutex<BTreeSet<String>> = std::sync::Mutex::new(BTreeSet::new());

/// Reads the bytes of each family on a background thread. Only the window's thread changes the font
/// set, so this does not stall scrolling: the bytes are read off the UI thread, and the faces are
/// added in one batch per frame (`ensure_page_fonts`).
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn start_loading(ctx: &egui::Context, catalog: std::sync::Arc<system::Catalog>, families: Vec<String>) {
    {
        let mut loading = LOADING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loading.extend(families.iter().cloned());
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for family in families {
            // A panic while reading one font must not leave its name marked as loading for good.
            let loaded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| catalog.load(&family)))
                .unwrap_or_else(|_| Err(format!("reading the font {family:?} failed")));
            FINISHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((family.clone(), loaded));
            LOADING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&family);
            ctx.request_repaint();
        }
    });
}

/// Register fonts used by a page once per context, returning names that could not be loaded. Never
/// blocks: a family not yet read is asked for now and drawn in its own font from a later frame.
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn ensure_page_fonts(ctx: &egui::Context, families: &BTreeSet<String>) -> Vec<String> {
    let id = egui::Id::new("notecraft-page-font-results");
    let mut results = ctx.data(|data| data.get_temp::<std::collections::BTreeMap<String, bool>>(id).unwrap_or_default());
    // Faces the background threads have read join the font set together, once for the frame.
    let finished = std::mem::take(&mut *FINISHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner));
    if !finished.is_empty() {
        let mut definitions = ctx.fonts(|fonts| fonts.definitions().clone());
        for (family, loaded) in finished {
            match loaded.and_then(|data| system::insert_face(&mut definitions, &family, data)) {
                Ok(_) => {
                    results.insert(family, true);
                }
                Err(error) => {
                    log::warn!("Could not load page font {family:?}: {error}");
                    results.insert(family, false);
                }
            }
        }
        ctx.set_fonts(definitions);
        ctx.request_repaint();
    }
    let loading = LOADING.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    let wanted: Vec<String> = families.iter().filter(|family| !results.contains_key(*family) && !loading.contains(*family)).cloned().collect();
    if !wanted.is_empty() {
        match catalog_if_ready() {
            Some(Ok(catalog)) => start_loading(ctx, catalog, wanted),
            // The installed fonts are not read yet: start that (once), and ask for these names later.
            None => start_catalog(),
            Some(Err(_)) => {}
        }
    }
    ctx.data_mut(|data| data.insert_temp(id, results.clone()));
    families.iter().filter(|family| results.get(*family) == Some(&false)).cloned().collect()
}

/// Off the desktop there are no installed fonts to add.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn ensure_page_fonts(ctx: &egui::Context, families: &BTreeSet<String>) -> Vec<String> {
    let _ = ctx;
    families.iter().cloned().collect()
}

/// egui's own definitions plus the faces NoteCraft adds.
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in BUNDLED {
        fonts.font_data.insert(name.to_owned(), Arc::new(egui::FontData::from_static(bytes)));
        // Last in each list: a fallback, used only for characters the faces before it lack.
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.to_owned());
        }
    }
    // The KaTeX maths faces, each in a family of its own, `katex-<face>`: an equation names the
    // face a glyph comes from, and nothing falls back to them or from them.
    for (face, bytes) in notecraft_text::katex::FACES {
        let name = format!("katex-{face}");
        fonts.font_data.insert(name.clone(), Arc::new(egui::FontData::from_static(bytes)));
        fonts.families.insert(egui::FontFamily::Name(name.clone().into()), vec![name]);
    }
    fonts
}

/// Installs them in a context. Rebuilding the atlas is not cheap, so the window does this
/// once; `installed` says whether it has been done for this context.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
    ctx.data_mut(|d| d.insert_temp(installed_id(), true));
}

pub fn installed(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(installed_id()).unwrap_or(false))
}

fn installed_id() -> egui::Id {
    egui::Id::new("notecraft-fonts-installed")
}

/// Installs them unless this context already has them.
pub fn install_once(ctx: &egui::Context) {
    if !installed(ctx) {
        install(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_font_requests_cover_tables_and_report_missing_families() {
        use notecraft_formats::markdown::{Block, Span};
        let named = |text: &str, name: &str| Span { text: text.into(), font_family: Some(name.into()), ..Span::default() };
        let blocks = vec![
            Block::Paragraph(vec![named("text", "No such NoteCraft family 928474")]),
            Block::Table { head: vec![vec![named("head", "No such NoteCraft family 928474")]], rows: vec![] },
        ];
        let families = page_families(&blocks);
        assert_eq!(families.len(), 1);
        let ctx = egui::Context::default();
        // Fonts load on background threads now, so the frames are run until the failure shows.
        let mut failures = Vec::new();
        for _ in 0..400 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                install_once(ui.ctx());
                failures = ensure_page_fonts(ui.ctx(), &families);
            });
            output.textures_delta.clear();
            if !failures.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(failures.len(), 1);
        assert!(failures[0].contains("No such NoteCraft family 928474"));
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn a_real_font_can_be_catalogued_and_installed_with_fallbacks() {
        let mut database = fontdb::Database::new();
        database.load_font_data(BUNDLED[0].1.to_vec());
        let catalog = super::system::Catalog::from_database(database);
        let family = catalog.families().first().expect("bundled font has a family name").clone();
        let mut fonts = definitions();
        let selected = catalog.install(&family, &mut fonts).expect("install the real font");
        let chain = fonts.families.get(&selected).expect("named family");
        assert!(chain.len() > 1, "the selected face retains the normal fallbacks");
        let face = fonts.font_data.get(&chain[0]).expect("selected font data");
        assert_eq!(face.bytes(), BUNDLED[0].1);
        let ctx = egui::Context::default();
        ctx.set_fonts(fonts);
        let mut output = ctx.run_ui(egui::RawInput::default(), |_| {});
        output.textures_delta.clear();
        assert!(ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::new(14.0, selected), "abc")));
        assert!(catalog.install("not a real family", &mut definitions()).is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn desktop_font_discovery_reports_its_result_and_can_load_a_discovered_face() {
        match super::system::Catalog::discover() {
            Ok(catalog) => {
                let family = catalog.families().first().expect("a discovered catalog is not empty");
                let mut definitions = definitions();
                let installed = catalog.install(family, &mut definitions).expect("a discovered font can be read");
                assert!(definitions.families.contains_key(&installed));
                eprintln!("Discovered {} installed font families and loaded one face", catalog.families().len());
            }
            Err(error) => {
                assert!(!error.is_empty(), "a missing system catalog reports why");
                eprintln!("System font discovery unavailable: {error}");
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
    #[test]
    fn a_page_registers_an_installed_system_font_in_the_context() {
        let Ok(catalog) = super::system::Catalog::discover() else {
            eprintln!("No system fonts are available for this platform test");
            return;
        };
        let family = catalog.families().first().expect("a discovered catalog is not empty").clone();
        let wanted = std::collections::BTreeSet::from([family.clone()]);
        let ctx = egui::Context::default();
        // The bytes load on a background thread; run frames until the family is registered.
        for _ in 0..400 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                install_once(ui.ctx());
                assert!(ensure_page_fonts(ui.ctx(), &wanted).is_empty(), "the page's installed font loads");
            });
            output.textures_delta.clear();
            if ctx.fonts(|fonts| fonts.definitions().families.contains_key(&named_family(&family))) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            assert!(ui.ctx().fonts(|fonts| fonts.definitions().families.contains_key(&named_family(&family))));
            for (name, _) in BUNDLED {
                assert!(ui.ctx().fonts(|fonts| fonts.definitions().font_data.contains_key(name)), "page fonts must preserve bundled face {name}");
            }
            assert!(ensure_page_fonts(ui.ctx(), &wanted).is_empty());
        });
        output.textures_delta.clear();
    }

    #[test]
    fn our_faces_are_fallbacks_not_replacements() {
        let fonts = definitions();
        for (name, _) in BUNDLED {
            assert!(fonts.font_data.contains_key(name), "{name} is missing");
        }
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            let list = fonts.families.get(&family).cloned().unwrap_or_default();
            assert!(list.len() > BUNDLED.len(), "{family:?} lost egui's own faces");
            let tail: Vec<&str> = list.iter().rev().take(BUNDLED.len()).rev().map(String::as_str).collect();
            let want: Vec<&str> = BUNDLED.iter().map(|(n, _)| *n).collect();
            assert_eq!(tail, want, "{family:?}: ours must come last, in order");
        }
    }

    #[test]
    fn a_context_with_them_has_the_glyphs_and_one_without_does_not() {
        // One word per script: Arabic, Persian, Urdu, Hebrew, Hindi, Chinese, Japanese, Korean.
        const WORDS: [&str; 8] = ["مرحبا", "سلام", "خوش آمدید", "שלום", "नमस्ते", "你好", "こんにちは", "안녕하세요"];
        let plain = egui::Context::default();
        let mut out = plain.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let has = |ctx: &egui::Context, s: &str| ctx.fonts_mut(|f| f.has_glyphs(&egui::FontId::proportional(14.0), s));
        assert!(has(&plain, "Hello"));
        for w in WORDS {
            assert!(!has(&plain, w), "egui's own faces have no glyphs for {w}; that is why this crate adds some");
        }

        let ctx = egui::Context::default();
        assert!(!installed(&ctx));
        install_once(&ctx);
        install_once(&ctx);
        assert!(installed(&ctx));
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        for w in WORDS {
            assert!(has(&ctx, w), "{w} should draw, not box");
        }
        assert!(has(&ctx, "Hello"), "Latin must still come from egui's own face");
        assert!(!has(&ctx, "\u{1F600}"), "no colour emoji face is bundled; a line that needs one still boxes");
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
/// Nothing to rescan where installed fonts cannot be read.
pub fn rescan_in_background(_ctx: egui::Context) {}

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

use std::collections::{BTreeMap, BTreeSet};
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

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub fn system_catalog() -> Result<&'static system::Catalog, &'static str> {
    static CATALOG: std::sync::OnceLock<Result<system::Catalog, String>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(system::Catalog::discover).as_ref().map_err(String::as_str)
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
fn load_family(family: &str, definitions: &mut egui::FontDefinitions) -> Result<(), String> {
    system_catalog().map_err(str::to_string)?.install(family, definitions)?;
    Ok(())
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn load_family(_family: &str, _definitions: &mut egui::FontDefinitions) -> Result<(), String> {
    Err("Installed font access is not available on this platform".to_string())
}

/// Register fonts used by a page once per context, returning names that could not be loaded.
pub fn ensure_page_fonts(ctx: &egui::Context, families: &BTreeSet<String>) -> Vec<String> {
    let id = egui::Id::new("notecraft-page-font-results");
    let mut results = ctx.data(|data| data.get_temp::<BTreeMap<String, bool>>(id).unwrap_or_default());
    let missing: Vec<_> = families.iter().filter(|family| !results.contains_key(*family)).cloned().collect();
    if !missing.is_empty() {
        let mut definitions = ctx.fonts(|fonts| fonts.definitions().clone());
        let mut changed = false;
        for family in missing {
            match load_family(&family, &mut definitions) {
                Ok(()) => {
                    changed = true;
                    results.insert(family, true);
                }
                Err(error) => {
                    log::warn!("Could not load page font {family:?}: {error}");
                    results.insert(family, false);
                }
            }
        }
        if changed {
            ctx.set_fonts(definitions);
            ctx.request_repaint();
        }
        ctx.data_mut(|data| data.insert_temp(id, results.clone()));
    }
    families.iter().filter(|family| results.get(*family) == Some(&false)).cloned().collect()
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
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            install_once(ui.ctx());
            let failures = ensure_page_fonts(ui.ctx(), &families);
            assert_eq!(failures.len(), 1);
            assert!(failures[0].contains("No such NoteCraft family 928474"));
            assert_eq!(failures, ensure_page_fonts(ui.ctx(), &families), "failure remains visible in later frames");
        });
        output.textures_delta.clear();
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
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            install_once(ui.ctx());
            assert!(ensure_page_fonts(ui.ctx(), &wanted).is_empty(), "the page's installed font loads");
        });
        output.textures_delta.clear();
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

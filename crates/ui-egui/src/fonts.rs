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

use std::sync::Arc;

/// The name the Arabic face goes by inside egui; the others are in [`BUNDLED`].
pub const ARABIC_NAME: &str = "noto-naskh-arabic";

/// The faces NoteCraft bundles, in fallback order, with their bytes. The bytes live in the
/// text crate so the window and the text layer share one copy.
pub const BUNDLED: [(&str, &[u8]); 4] = notecraft_text::fonts::ALL;

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

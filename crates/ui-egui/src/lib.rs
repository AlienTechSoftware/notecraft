#![recursion_limit = "256"]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![allow(deprecated)]
//! NoteCraft egui front end (layer L6).
//!
//! A thin, swappable layer over the engine: nothing below it depends on egui. It
//! draws the notebook tree with code-drawn icons, edits pages in either format with
//! a live preview, and sends every action to the engine as a command.
//!
//! The crate depends on `egui` only. The window itself (eframe) lives in the app
//! crate, so this crate can also be driven headlessly in tests.

pub mod app;
pub mod backstage;
pub mod bidi;
pub mod clipboard;
pub mod codeview;
pub mod collect;
pub mod combo;
pub mod containers;
pub mod control;
pub mod datetime;
pub mod diagramview;
pub mod floating;
pub mod fonts;
pub mod gridcraft;
pub mod highlight;
pub mod history;
pub mod hscroll;
pub mod i18n;
pub mod icons;
pub mod language_flag;
pub mod input_language {
    pub use notecraft_input_language::{Watcher, is_rtl, watch_changes};
}
pub mod inkpad;
pub mod mathview;
pub mod navhistory;
pub mod pageedit;
pub mod pagemenu;
pub mod pagetext;
pub mod pictures;
pub mod preview;
pub(crate) mod printout;
pub mod quickaccess;
pub mod ribbon;
pub mod ribbon_icons;
pub mod saver;
pub mod snippets;
pub mod stylus;
pub mod telemetry_export;
#[cfg(test)]
mod testing_dir;
pub mod theme;

pub use app::NoteCraftApp;

/// Translate a source string supplied dynamically by an enum or a shared UI helper.
#[macro_export]
macro_rules! tl {
    ($source:expr) => {
        $crate::i18n::t($source)
    };
}

/// Translate a user-facing string by its stable ID in `i18n/en.tsv`.
#[macro_export]
macro_rules! tl_id {
    ($id:expr) => {
        $crate::i18n::t_id($id)
    };
}

/// The app icon (256 x 256, transparent corners). See ATTRIBUTION.md.
pub const LOGO_PNG: &[u8] = include_bytes!("../../../assets/brand/notecraft-256.png");
/// The brand banner for the first-use wizard, light and dark variants.
pub const BANNER_LIGHT_PNG: &[u8] = include_bytes!("../../../assets/brand/notecraft-social-light.png");
pub const BANNER_DARK_PNG: &[u8] = include_bytes!("../../../assets/brand/notecraft-social-dark.png");

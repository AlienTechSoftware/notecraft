#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
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
pub mod containers;
pub mod control;
pub mod fonts;
pub mod gridcraft;
pub mod icons;
pub mod inkpad;
pub mod mathview;
pub mod pageedit;
pub mod pagemenu;
pub mod pagetext;
pub mod preview;
pub mod ribbon;
pub mod ribbon_icons;
pub mod snippets;
pub mod stylus;
#[cfg(test)]
mod testing_dir;
pub mod theme;

pub use app::NoteCraftApp;

/// The app icon (256 x 256, transparent corners). See ATTRIBUTION.md.
pub const LOGO_PNG: &[u8] = include_bytes!("../../../assets/brand/notecraft-256.png");

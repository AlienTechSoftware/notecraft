#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft model (layer L0): the vocabulary every other crate shares.
//!
//! Node kinds, page formats, the rules for folder and file names, ids, time, and
//! a few small text helpers. No file system access and no dependencies.

pub mod color;
pub mod container;
pub mod format;
pub mod id;
pub mod kind;
pub mod kv;
pub mod names;
pub mod tablecols;
pub mod tag;
pub mod text;
pub mod time;

pub use color::NodeColor;
pub use container::Container;
pub use format::{FormatError, PageFormat};
pub use kind::NodeKind;
pub use tag::Tag;
pub use text::{Dir, direction, direction_or};

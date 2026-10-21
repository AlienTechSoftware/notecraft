#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft store (layer L2): notebooks as plain folders.
//!
//! A notebook, a section group, a section and a page are all folders. A page is a
//! folder with one content file (`index.html` or `index.md`) and an `assets` folder;
//! a notebook, section group or section is a folder with a small `_node.toml`. Pages
//! nest to any depth. Sync and backup come from the folder, not from NoteCraft.
//!
//! This crate reads and writes that layout: the tree, page content, attachments,
//! settings, search, and a checker for the folder rules. It does not know about any
//! user interface. The layout is described in `docs/folder-format.md`.

pub mod assets;
pub mod check;
pub mod error;
pub mod links;
pub mod ops;
pub mod page;
pub mod recycle;
pub mod search;
pub mod settings;
pub mod testing;
pub mod tree;

pub use error::{Result, StoreError};
pub use notecraft_model::{NodeKind, PageFormat};
pub use settings::{FormatSource, Settings};
pub use tree::{Node, NodeInfo};

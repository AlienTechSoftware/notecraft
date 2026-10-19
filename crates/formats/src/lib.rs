#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft page formats (layer L1).
//!
//! Text handling for the two page formats, HTML and CommonMark + GFM, with no file
//! system access: splitting front matter, reading and replacing the body of an HTML
//! page, page templates, converting HTML to a Markdown-like text for previews, and
//! parsing Markdown into a small block model that any front end can draw.
//!
//! The Markdown reader is a deliberate subset (headings, paragraphs, lists with task
//! items, block quotes, fenced code, math blocks, tables, rules, images, and inline
//! emphasis, code, strikethrough, links and math). It never panics on odd input.

pub mod boxes;
pub mod frontmatter;
pub mod html;
pub mod markdown;
pub mod render;
pub mod templates;

pub use markdown::{Block, Span};
pub use render::{blocks_to_html, blocks_to_markdown};

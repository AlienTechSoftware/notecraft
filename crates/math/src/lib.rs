#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft math (layer L1): a TeX subset parsed and laid out into drawing items.
//!
//! `parse` turns TeX into a tree and never fails: anything it does not know is kept
//! as literal text. `layout` turns the tree into positioned text, lines and paths
//! that any front end can draw, given a way to measure text (`Metrics`).
//!
//! Supported: letters, numbers, operators, Greek letters and common symbols,
//! superscripts and subscripts, `\frac`, `\sqrt` (with an optional index), `\sum`,
//! `\prod`, `\int` with limits, `\left ... \right` delimiters, `\text`, and spacing
//! commands. Not supported: matrices, aligned environments, accents, `\mathbb` and
//! friends (their argument is shown as plain text), macros, and any package.

mod layout;
mod parse;
mod tex;

pub use layout::{Item, Layout, Metrics, layout};
pub use parse::{BigKind, Node, parse};
pub use tex::{Cmd, Engine, MAX_NESTING, MAX_SOURCE_CHARS, Rgba, Shape, Typeset, typeset};

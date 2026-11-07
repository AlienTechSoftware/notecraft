#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft OneNote reader (layer L1): reads `.one` sections and optional `.onetoc2`
//! color metadata into plain models.
//!
//! Format behavior follows Microsoft's published [MS-ONESTORE] and [MS-ONE] specifications.
//! The undocumented `NotebookColor` property ID is corroborated by public parser sources,
//! recorded in `docs/research/onenote-file-format/import-sources.md`; no implementation code
//! is copied. No Microsoft code or decompiled binaries were used.
//!
//! Read only. This crate never writes a file and has no file system or UI dependency
//! beyond reading the bytes it is handed. Damaged input is an error, never a panic.

mod bytes;
mod error;
mod guid;
mod one;
mod props;
mod store;
mod toc;
#[cfg(test)]
mod toc_tests;

/// Builders for synthetic section and TOC files, for tests in this and other crates.
#[cfg(feature = "testing")]
pub mod testing {
    pub use crate::store::build::{Attachment, FileDecl, Line, One, OneNote, Toc};
}

pub use error::{OneError, Result};
pub use guid::{CompactId, ExtendedGuid, Guid};
pub use one::{Content, List, Page, Run, Section, section};
pub use props::{Property, PropertySet, Value};
pub use store::{Header, Object, ObjectData, ObjectSpace, Store, read, read_header};
pub use toc::{Toc, TocSection};

/// Reads a OneNote `.onetoc2` notebook table of contents.
pub fn read_toc(data: &[u8]) -> Result<Toc> {
    toc::parse(&store::read_toc_store(data)?)
}

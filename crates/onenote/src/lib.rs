#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft OneNote reader (layer L1): reads `.one` section files into a plain model.
//!
//! Clean room: written from Microsoft's published specifications, [MS-ONESTORE] (the
//! revision store container) and [MS-ONE] (what the objects mean). No Microsoft code
//! and no decompiled binaries were used. See `docs/research/onenote-file-format/`.
//!
//! Read only. This crate never writes a file and has no file system or UI dependency
//! beyond reading the bytes it is handed. Damaged input is an error, never a panic.

mod bytes;
mod error;
mod guid;
mod one;
mod props;
mod store;

/// Builders for synthetic section files, for tests in this and other crates.
#[cfg(feature = "testing")]
pub mod testing {
    pub use crate::store::build::{Line, One, OneNote};
}

pub use error::{OneError, Result};
pub use guid::{CompactId, ExtendedGuid, Guid};
pub use one::{Content, List, Page, Run, Section, section};
pub use props::{Property, PropertySet, Value};
pub use store::{Header, Object, ObjectData, ObjectSpace, Store, read, read_header};

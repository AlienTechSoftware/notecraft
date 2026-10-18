//! Errors of the OneNote reader. A damaged file is an error value, never a panic.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneError {
    /// The file does not start with the `.one` file type GUID.
    NotASection(String),
    /// The file is a table of contents (`.onetoc2`), not a section.
    NotesetFile,
    /// A structure points outside the file or is cut short.
    Truncated(&'static str),
    /// A structure has a value the format does not allow.
    Corrupt(String),
    /// The section is protected with a password. Not supported.
    Encrypted,
    /// The file is larger than the reader's memory budget.
    TooLarge(u64),
}

impl fmt::Display for OneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OneError::NotASection(why) => write!(f, "not a OneNote section file: {why}"),
            OneError::NotesetFile => write!(f, "this is a notebook table of contents (.onetoc2), not a section (.one)"),
            OneError::Truncated(what) => write!(f, "the file is cut short while reading {what}"),
            OneError::Corrupt(what) => write!(f, "the file is damaged: {what}"),
            OneError::Encrypted => write!(f, "the section is password protected; password protected sections are not supported"),
            OneError::TooLarge(n) => write!(f, "the file is too large to import ({n} bytes)"),
        }
    }
}

impl std::error::Error for OneError {}

pub type Result<T> = std::result::Result<T, OneError>;

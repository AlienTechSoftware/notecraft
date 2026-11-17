//! Errors of the OneNote reader. A damaged file is an error value, never a panic.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneError {
    /// The file does not start with the `.one` file type GUID.
    NotASection(String),
    /// The file does not start with the `.onetoc2` file type GUID.
    NotAToc(String),
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
    /// The file is in the packaging OneNote uses to store and sync notebooks on OneDrive and
    /// SharePoint, and in files downloaded from there ([MS-ONESTORE] 2.8, carried in
    /// [MS-FSSHTTPB] data elements). This reader reads the desktop revision store only.
    Packaged,
    /// A file in the OneDrive packaging that could not be read: what went wrong.
    PackagedUnreadable(String),
}

impl fmt::Display for OneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OneError::NotASection(why) => write!(f, "not a OneNote section file: {why}"),
            OneError::NotAToc(why) => write!(f, "not a OneNote table-of-contents file: {why}"),
            OneError::NotesetFile => write!(f, "this is a notebook table of contents (.onetoc2), not a section (.one)"),
            OneError::Truncated(what) => write!(f, "the file is cut short while reading {what}"),
            OneError::Corrupt(what) => write!(f, "the file is damaged: {what}"),
            OneError::Encrypted => write!(f, "the section is password protected; password protected sections are not supported"),
            OneError::TooLarge(n) => write!(f, "the file is too large to import ({n} bytes)"),
            OneError::Packaged => write!(
                f,
                "the file is in OneNote's OneDrive packaging (FSSHTTPB), which this reader does not read yet; only the desktop format is read"
            ),
            OneError::PackagedUnreadable(why) => write!(f, "the file is in OneNote's OneDrive packaging (FSSHTTPB) and could not be read: {why}"),
        }
    }
}

impl std::error::Error for OneError {}

pub type Result<T> = std::result::Result<T, OneError>;

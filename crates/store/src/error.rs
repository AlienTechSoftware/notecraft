use std::fmt;
use std::path::{Path, PathBuf};

pub type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug)]
pub enum StoreError {
    /// A file system operation failed.
    Io { action: &'static str, path: PathBuf, source: std::io::Error },
    /// The request is not allowed (bad name, wrong parent, unknown format, too large).
    Invalid(String),
    /// A folder or file that should exist does not.
    NotFound(String),
}

impl StoreError {
    pub fn io(action: &'static str, path: &Path, source: std::io::Error) -> StoreError {
        StoreError::Io { action, path: path.to_path_buf(), source }
    }

    pub fn invalid(message: impl Into<String>) -> StoreError {
        StoreError::Invalid(message.into())
    }
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::Io { action, path, source } => write!(f, "cannot {action} {}: {source}", path.display()),
            StoreError::Invalid(m) => f.write_str(m),
            StoreError::NotFound(m) => write!(f, "not found: {m}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<notecraft_model::FormatError> for StoreError {
    fn from(e: notecraft_model::FormatError) -> Self {
        StoreError::Invalid(e.to_string())
    }
}

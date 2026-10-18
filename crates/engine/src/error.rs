use notecraft_store::StoreError;
use std::fmt;

#[derive(Debug)]
pub enum EngineError {
    Store(StoreError),
    UnknownCommand(String),
    MissingParam(&'static str),
    BadParam(String),
    /// A command panicked; the session was kept.
    Panicked(String),
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EngineError::Store(e) => write!(f, "{e}"),
            EngineError::UnknownCommand(id) => write!(f, "unknown command '{id}'"),
            EngineError::MissingParam(p) => write!(f, "missing parameter '{p}'"),
            EngineError::BadParam(m) => f.write_str(m),
            EngineError::Panicked(m) => write!(f, "internal error (the command was stopped and nothing was lost): {m}"),
        }
    }
}

impl std::error::Error for EngineError {}

impl From<StoreError> for EngineError {
    fn from(e: StoreError) -> Self {
        EngineError::Store(e)
    }
}

impl From<notecraft_model::FormatError> for EngineError {
    fn from(e: notecraft_model::FormatError) -> Self {
        EngineError::BadParam(e.to_string())
    }
}

use crate::commands::{self, Output, Params};
use crate::error::EngineError;
use notecraft_store::tree::{self, Node};
use notecraft_store::{Settings, StoreError};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// A notebooks folder, the settings, and a cached copy of the tree.
pub struct Session {
    root: PathBuf,
    pub settings: Settings,
    tree: Vec<Node>,
}

impl Session {
    /// Opens (and creates, if needed) the notebooks folder `root`.
    pub fn new(root: &Path, settings: Settings) -> Result<Session, EngineError> {
        std::fs::create_dir_all(root).map_err(|e| StoreError::io("create", root, e))?;
        let root = std::fs::canonicalize(root).map_err(|e| StoreError::io("open", root, e))?;
        let mut s = Session { root, settings, tree: Vec::new() };
        s.refresh();
        Ok(s)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The notebooks, as last read from disk. Commands that change the tree refresh it.
    pub fn tree(&self) -> &[Node] {
        &self.tree
    }

    pub fn refresh(&mut self) {
        self.tree = tree::load_notebooks(&self.root);
    }

    /// An absolute path or a path relative to the notebooks folder, resolved and
    /// confined to the notebooks folder.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, EngineError> {
        let p = Path::new(path);
        let joined = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        let canon = std::fs::canonicalize(&joined).map_err(|_| StoreError::NotFound(path.to_string()))?;
        if !canon.starts_with(&self.root) {
            return Err(EngineError::BadParam("that path is outside the notebooks folder".into()));
        }
        Ok(canon)
    }

    /// Runs a command by id. A panic inside the command becomes `EngineError::Panicked`.
    pub fn run(&mut self, id: &str, params: &Params) -> Result<Output, EngineError> {
        let spec = commands::registry().iter().find(|c| c.id == id).ok_or_else(|| EngineError::UnknownCommand(id.to_string()))?;
        let result = guarded(|| (spec.run)(self, params))?;
        if spec.changes_tree && result.is_ok() {
            self.refresh();
        }
        result
    }

    /// Stores bytes (for example a pasted picture) as an attachment of a page.
    /// Returns the relative link, `assets/<name>`.
    pub fn attach_bytes(&mut self, page: &str, name: &str, bytes: &[u8]) -> Result<String, EngineError> {
        let dir = self.resolve(page)?;
        guarded(|| notecraft_store::assets::attach_bytes(&dir, name, bytes).map_err(EngineError::from))?
    }
}

/// Runs `f`, turning a panic into an error.
pub(crate) fn guarded<T>(f: impl FnOnce() -> T) -> Result<T, EngineError> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown".into());
        EngineError::Panicked(msg)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_becomes_an_error() {
        let r: Result<(), EngineError> = guarded(|| panic!("boom"));
        assert!(matches!(r, Err(EngineError::Panicked(m)) if m == "boom"));
        let r: Result<(), EngineError> = guarded(|| panic!("{}", String::from("formatted")));
        assert!(matches!(r, Err(EngineError::Panicked(m)) if m == "formatted"));
        assert_eq!(guarded(|| 7).ok(), Some(7));
    }
}

use crate::commands::{self, Output, Params};
use crate::error::EngineError;
use notecraft_store::tree::{self, Node};
use notecraft_store::{Settings, StoreError};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

/// A notebooks folder, the settings, and a cached copy of the tree.
pub struct Session {
    root: PathBuf,
    /// Notebooks kept somewhere else and opened anyway. A notebook is a folder, so one can
    /// live outside the notebooks folder; opening it must not put away the others, which is
    /// why these sit beside the root rather than replacing it. Remembered in the settings.
    extra: Vec<PathBuf>,
    /// Notebooks inside the notebooks folder that have been put away. Remembered in settings.
    closed: Vec<PathBuf>,
    pub settings: Settings,
    tree: Vec<Node>,
}

/// How the settings file keeps the list. A vertical bar cannot appear in a Windows path and
/// is vanishingly rare elsewhere, which makes it a safe separator for one line of them.
const OPEN_KEY: &str = "open_notebooks";
const CLOSED_KEY: &str = "closed_notebooks";
const SEP: char = '|';

impl Session {
    /// Opens (and creates, if needed) the notebooks folder `root`.
    pub fn new(root: &Path, settings: Settings) -> Result<Session, EngineError> {
        std::fs::create_dir_all(root).map_err(|e| StoreError::io("create", root, e))?;
        let root = std::fs::canonicalize(root).map_err(|e| StoreError::io("open", root, e))?;
        let extra =
            settings.get(OPEN_KEY).map(|v| v.split(SEP).map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from).collect()).unwrap_or_default();
        let closed =
            settings.get(CLOSED_KEY).map(|v| v.split(SEP).map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from).collect()).unwrap_or_default();
        let mut s = Session { root, extra, closed, settings, tree: Vec::new() };
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
        self.closed.retain(|p| p.is_dir());
        self.tree.retain(|n| !self.closed.contains(&n.path));
        // The notebooks opened from elsewhere, in the order they were opened, after the
        // ones in the notebooks folder. A folder that has gone is quietly left out.
        self.extra.retain(|p| p.is_dir());
        for path in &self.extra {
            if let Some(node) = tree::load_tree(path)
                && node.info.kind == notecraft_store::NodeKind::Notebook
                && !self.tree.iter().any(|n| n.path == node.path)
            {
                self.tree.push(node);
            }
        }
    }

    /// The notebooks opened from outside the notebooks folder.
    pub fn opened_elsewhere(&self) -> &[PathBuf] {
        &self.extra
    }

    /// Opens a notebook that lives outside the notebooks folder, keeping everything that is
    /// already open. Opening one that is already there changes nothing.
    pub fn open_notebook(&mut self, path: &Path) -> Result<PathBuf, EngineError> {
        let path = std::fs::canonicalize(path).map_err(|_| StoreError::NotFound(path.display().to_string()))?;
        if tree::read_node(&path).is_none() {
            let title = tree::folder_name(&path);
            notecraft_store::ops::initialize_notebook(&path, &title, None, &self.settings)?;
        }
        let info = tree::read_node(&path).ok_or_else(|| EngineError::BadParam(format!("{} is not a notebook", path.display())))?;
        if info.kind != notecraft_store::NodeKind::Notebook {
            return Err(EngineError::BadParam(format!("{} is a {}, not a notebook", path.display(), info.kind.label().to_lowercase())));
        }
        if !path.starts_with(&self.root) && !self.extra.contains(&path) {
            self.extra.push(path.clone());
            self.remember();
        }
        self.closed.retain(|p| p != &path);
        self.remember_closed();
        self.refresh();
        Ok(path)
    }

    /// Puts a notebook away again. Nothing is deleted.
    pub fn close_notebook(&mut self, path: &Path) -> bool {
        let canon = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if canon.starts_with(&self.root) {
            if self.closed.contains(&canon) || !tree::read_node(&canon).is_some_and(|n| n.kind == notecraft_store::NodeKind::Notebook) {
                return false;
            }
            self.closed.push(canon);
            self.remember_closed();
            self.refresh();
            return true;
        }
        let before = self.extra.len();
        self.extra.retain(|p| p != &canon);
        if self.extra.len() != before {
            self.remember();
            self.refresh();
            return true;
        }
        false
    }

    fn remember(&self) {
        let line = self.extra.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(&SEP.to_string());
        let _ = self.settings.set(OPEN_KEY, &line);
    }

    fn remember_closed(&self) {
        let line = self.closed.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join(&SEP.to_string());
        let _ = self.settings.set(CLOSED_KEY, &line);
    }

    /// The page to show for `path`: the page itself, or the first page inside a notebook,
    /// section group or section. `path` is relative to the notebooks folder or absolute. A
    /// notebook kept somewhere else is opened first, and one that was put away is opened again,
    /// so the window, the command line and an agent all open the same thing the same way.
    pub fn page_to_show(&mut self, path: &str) -> Result<PathBuf, EngineError> {
        let p = Path::new(path);
        let joined = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        let canon = std::fs::canonicalize(&joined).map_err(|_| StoreError::NotFound(path.to_string()))?;
        let notebook = canon.ancestors().find(|a| tree::read_node(a).is_some_and(|n| n.kind == notecraft_store::NodeKind::Notebook));
        if let Some(notebook) = notebook.map(Path::to_path_buf) {
            let known = notebook.starts_with(&self.root) || self.extra.contains(&notebook);
            if !known || self.closed.contains(&notebook) {
                self.open_notebook(&notebook)?;
            }
        }
        let dir = self.resolve(path)?;
        let node = tree::load_tree(&dir).ok_or_else(|| EngineError::BadParam(format!("{path} is not a notebook, section group, section or page")))?;
        first_page(&node).ok_or_else(|| EngineError::BadParam(format!("there is no page in {path} yet")))
    }

    /// An absolute path or a path relative to the notebooks folder, resolved and
    /// confined to the notebooks folder.
    pub fn resolve(&self, path: &str) -> Result<PathBuf, EngineError> {
        let p = Path::new(path);
        let joined = if p.is_absolute() { p.to_path_buf() } else { self.root.join(p) };
        let canon = std::fs::canonicalize(&joined).map_err(|_| StoreError::NotFound(path.to_string()))?;
        // Inside the notebooks folder, or inside a notebook opened from somewhere else.
        if !canon.starts_with(&self.root) && !self.extra.iter().any(|n| canon.starts_with(n)) {
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

/// The node itself if it is a page, else the first page below it in tree order.
fn first_page(n: &Node) -> Option<PathBuf> {
    if n.info.kind == notecraft_store::NodeKind::Page {
        return Some(n.path.clone());
    }
    n.children.iter().find_map(first_page)
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

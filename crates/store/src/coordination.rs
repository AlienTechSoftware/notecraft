//! Serializes one notebooks root's controller commands with its background saves.
//!
//! Metadata edits read and replace the page too. They must not race a body save that read
//! an earlier header. Unrelated notebooks roots never share a write lock.

use crate::error::{Result, StoreError};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

type Writers = BTreeMap<(bool, PathBuf), Weak<Mutex<()>>>;
static WRITERS: Mutex<Writers> = Mutex::new(BTreeMap::new());

pub fn writer(root: &Path) -> Result<Arc<Mutex<()>>> {
    writer_for(root, false)
}

/// Body saves use a separate namespace even when a caller chose a page as its session root.
pub fn page_writer(page: &Path) -> Result<Arc<Mutex<()>>> {
    writer_for(page, true)
}

fn writer_for(root: &Path, page: bool) -> Result<Arc<Mutex<()>>> {
    // The browser build has no file system to canonicalize against; its notebook paths are
    // already the keys the writers map needs.
    let root =
        if cfg!(target_arch = "wasm32") { root.to_path_buf() } else { std::fs::canonicalize(root).map_err(|e| StoreError::io("open", root, e))? };
    let mut writers = WRITERS.lock().map_err(|_| StoreError::invalid("the notebook writers are unavailable"))?;
    writers.retain(|_, writer| writer.strong_count() > 0);
    if let Some(writer) = writers.get(&(page, root.clone())).and_then(Weak::upgrade) {
        return Ok(writer);
    }
    let writer = Arc::new(Mutex::new(()));
    writers.insert((page, root), Arc::downgrade(&writer));
    Ok(writer)
}

/// A cheap native page and ink revision, independent of any window framework.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    page: (u64, std::time::SystemTime),
    ink: Option<(u64, std::time::SystemTime)>,
}

impl Revision {
    pub fn same_page(&self, other: &Self) -> bool {
        self.page == other.page
    }

    pub fn read(dir: &Path) -> std::io::Result<Self> {
        if cfg!(any(target_arch = "wasm32", target_os = "android", target_os = "ios")) {
            return Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "native window change detection is unavailable"));
        }
        let (_, file) =
            crate::tree::page_file(dir).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "the page is no longer available"))?;
        let stamp = |path: &Path| -> std::io::Result<_> {
            let m = std::fs::metadata(path)?;
            Ok((m.len(), m.modified()?))
        };
        let ink = match stamp(&dir.join("ink.svg")) {
            Ok(revision) => Some(revision),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e),
        };
        Ok(Self { page: stamp(&file)?, ink })
    }
}

/// Captured while the root's writer is held, so a view never adopts an unrelated later save.
pub struct Snapshot {
    pub body: String,
    pub revision: Option<Revision>,
}

impl Snapshot {
    pub fn read(dir: &Path) -> Result<Self> {
        Ok(Self { body: crate::page::load_page(dir)?.body, revision: Revision::read(dir).ok() })
    }
}

//! A notebook-local recycle bin for pages, sections and section groups.

use crate::error::{Result, StoreError};
use crate::tree;
use notecraft_model::{NodeKind, id::new_id, kv, time::iso_now};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recycled {
    pub id: String,
    pub title: String,
    pub deleted: String,
    pub count: usize,
}

fn canonical(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|e| StoreError::io("open", path, e))
}

fn bin(notebook: &Path) -> PathBuf {
    notebook.join(".notecraft").join("recycle-bin")
}

fn checked_bin(notebook: &Path, create: bool) -> Result<PathBuf> {
    let meta = notebook.join(".notecraft");
    if create && !meta.exists() {
        fs::create_dir(&meta).map_err(|e| StoreError::io("create", &meta, e))?;
    }
    if !meta.exists() {
        return Ok(bin(notebook));
    }
    let meta_real = canonical(&meta)?;
    if !meta_real.starts_with(notebook) {
        return Err(StoreError::invalid("the notebook metadata folder points outside this notebook"));
    }
    let dir = meta.join("recycle-bin");
    if create && !dir.exists() {
        fs::create_dir(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    }
    if !dir.exists() {
        return Ok(dir);
    }
    let real = canonical(&dir)?;
    if !real.starts_with(notebook) {
        return Err(StoreError::invalid("the recycle bin points outside this notebook"));
    }
    Ok(real)
}

/// Move a node below a notebook into its recycle bin. Deleting the notebook itself is not
/// supported yet because its bin would otherwise move with it.
pub fn delete(notebook: &Path, path: &Path) -> Result<usize> {
    let notebook = canonical(notebook)?;
    let path = canonical(path)?;
    let info = tree::read_node(&notebook).ok_or_else(|| StoreError::invalid("that is not a notebook"))?;
    if info.kind != NodeKind::Notebook {
        return Err(StoreError::invalid("the recycle bin belongs to a notebook"));
    }
    if path == notebook || !path.starts_with(&notebook) {
        return Err(StoreError::invalid("only a page, section or group inside this notebook can be recycled"));
    }
    let node = tree::load_tree(&path).ok_or_else(|| StoreError::invalid(format!("{} is not a NoteCraft node", path.display())))?;
    let count = node.count();
    let id = new_id();
    let bin = checked_bin(&notebook, true)?;
    let entry = bin.join(&id);
    let item = entry.join("item");
    fs::create_dir_all(&bin).map_err(|e| StoreError::io("create", &bin, e))?;
    fs::create_dir(&entry).map_err(|e| StoreError::io("create", &entry, e))?;
    let relative = path.strip_prefix(&notebook).map_err(|_| StoreError::invalid("the item is outside its notebook"))?;
    let relative = relative.to_string_lossy().replace('\\', "/");
    let manifest = format!(
        "id = {}\ntitle = {}\ndeleted = {}\noriginal = {}\n",
        kv::quote(&id),
        kv::quote(&node.info.title),
        kv::quote(&iso_now()),
        kv::quote(&relative),
    );
    if let Err(e) = fs::write(entry.join("recycled.toml"), manifest) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("write", &entry.join("recycled.toml"), e));
    }
    if let Err(e) = fs::rename(&path, &item) {
        let _ = fs::remove_dir_all(&entry);
        return Err(StoreError::io("move", &path, e));
    }
    Ok(count)
}

/// Items in the notebook's bin, newest first.
pub fn list(notebook: &Path) -> Result<Vec<Recycled>> {
    let notebook = canonical(notebook)?;
    let dir = checked_bin(&notebook, false)?;
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(StoreError::io("read", &dir, e)),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| StoreError::io("read", &dir, e))?.path();
        if !entry.is_dir() {
            continue;
        }
        let manifest = entry.join("recycled.toml");
        let text = match fs::read_to_string(&manifest) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(StoreError::io("read", &manifest, e)),
        };
        let fields = kv::parse(&text);
        let id = fields.get("id").cloned().unwrap_or_default();
        if id.len() != 20 || !id.bytes().all(|b| b.is_ascii_hexdigit()) || entry.file_name().and_then(|n| n.to_str()) != Some(id.as_str()) {
            continue;
        }
        let Some(node) = tree::load_tree(&entry.join("item")) else { continue };
        result.push(Recycled {
            id,
            title: fields.get("title").cloned().unwrap_or_else(|| node.info.title.clone()),
            deleted: fields.get("deleted").cloned().unwrap_or_default(),
            count: node.count(),
        });
    }
    result.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(result)
}

/// Restore an item to its original location. It refuses to overwrite anything that has
/// since been created at that path.
pub fn restore(notebook: &Path, id: &str) -> Result<()> {
    if id.len() != 20 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StoreError::invalid("that recycle-bin id is not valid"));
    }
    let notebook = canonical(notebook)?;
    let entry = checked_bin(&notebook, false)?.join(id);
    let manifest = entry.join("recycled.toml");
    let text = fs::read_to_string(&manifest).map_err(|e| StoreError::io("read", &manifest, e))?;
    let fields = kv::parse(&text);
    if fields.get("id").map(String::as_str) != Some(id) {
        return Err(StoreError::invalid("that recycle-bin entry is invalid"));
    }
    let relative = fields.get("original").ok_or_else(|| StoreError::invalid("that recycle-bin entry has no original location"))?;
    let relative = Path::new(relative);
    if relative.components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err(StoreError::invalid("that recycle-bin location is not valid"));
    }
    let destination = notebook.join(relative);
    if !destination.starts_with(&notebook) || fs::symlink_metadata(&destination).is_ok() {
        return Err(StoreError::invalid("the original location is occupied or outside the notebook"));
    }
    let Some(parent) = destination.parent() else {
        return Err(StoreError::invalid("the original section no longer exists"));
    };
    if !parent.is_dir() || !canonical(parent)?.starts_with(&notebook) {
        return Err(StoreError::invalid("the original section no longer exists"));
    }
    fs::rename(entry.join("item"), &destination).map_err(|e| StoreError::io("restore", &destination, e))?;
    fs::remove_dir_all(&entry).map_err(|e| StoreError::io("clean up", &entry, e))
}

/// Permanently remove one recycled item.
pub fn purge(notebook: &Path, id: &str) -> Result<()> {
    if id.len() != 20 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(StoreError::invalid("that recycle-bin id is not valid"));
    }
    let notebook = canonical(notebook)?;
    let entry = checked_bin(&notebook, false)?.join(id);
    if !entry.join("recycled.toml").is_file() {
        return Err(StoreError::NotFound(format!("recycle-bin item {id}")));
    }
    fs::remove_dir_all(&entry).map_err(|e| StoreError::io("purge", &entry, e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    #[test]
    fn a_recycled_page_can_be_restored_and_purged() {
        let temp = crate::testing::TempDir::new("recycle-bin");
        let settings = Settings::at(temp.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(temp.path(), "Notebook", None, &settings).expect("notebook");
        let section = crate::ops::create_node(&notebook, "Section", NodeKind::Section, None, &settings).expect("section");
        let page = crate::ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("page");

        assert_eq!(delete(&notebook, &page).expect("recycle"), 1);
        let items = list(&notebook).expect("list");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Page");
        assert!(!page.exists());
        restore(&notebook, &items[0].id).expect("restore");
        assert!(page.is_dir());
        let items = list(&notebook).expect("empty bin");
        assert!(items.is_empty());

        delete(&notebook, &page).expect("recycle again");
        let id = list(&notebook).expect("list")[0].id.clone();
        purge(&notebook, &id).expect("purge");
        assert!(list(&notebook).expect("empty bin").is_empty());
    }

    #[test]
    fn recycle_restore_refuses_to_overwrite_the_original_location() {
        let temp = crate::testing::TempDir::new("recycle-collision");
        let settings = Settings::at(temp.path().join("settings.toml"));
        let notebook = crate::ops::create_notebook(temp.path(), "Notebook", None, &settings).expect("notebook");
        let section = crate::ops::create_node(&notebook, "Section", NodeKind::Section, None, &settings).expect("section");
        let page = crate::ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("page");
        delete(&notebook, &page).expect("recycle");
        let id = list(&notebook).expect("list")[0].id.clone();
        crate::ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("replacement");
        assert!(restore(&notebook, &id).is_err());
        assert_eq!(list(&notebook).expect("still in bin").len(), 1);
    }
}

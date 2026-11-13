//! Tracks changes which a session-only browser host will discard on close.

use std::path::{Path, PathBuf};

/// A dirty editor and a write to its virtual file both count as unsaved session
/// work. Once either happens, autosave must not turn the close warning off.
pub(crate) struct SessionChanges {
    root: PathBuf,
    changed: bool,
}

impl SessionChanges {
    pub(crate) fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), changed: false }
    }

    pub(crate) fn observe_editor(&mut self, dirty: bool) {
        self.changed |= dirty;
    }

    pub(crate) fn observe_file(&mut self, path: &Path) {
        if path != self.root && path.starts_with(&self.root) && !path.starts_with(self.root.join(".notecraft")) {
            self.changed = true;
        }
    }

    pub(crate) fn needs_warning(&self) -> bool {
        self.changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Files(PathBuf);

    impl Files {
        fn new() -> Self {
            loop {
                let path =
                    std::env::temp_dir().join(format!("notecraft-session-storage-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("temporary folder: {e}"),
                }
            }
        }

        fn write(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().expect("parent")).expect("folder");
            fs::write(&path, "fixture").expect("file");
            path
        }
    }

    impl Drop for Files {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn preferences_and_files_outside_the_notebook_root_do_not_arm_a_warning() {
        let files = Files::new();
        let root = files.0.join("NoteCraft");
        let settings = files.write("NoteCraft/.notecraft/settings.toml");
        let outside = files.write("NoteCraft-other/page.md");
        let mut changes = SessionChanges::new(&root);
        changes.observe_file(&root);
        changes.observe_file(&settings);
        changes.observe_file(&outside);
        changes.observe_editor(false);
        assert!(!changes.needs_warning(), "opening a clean demo or changing preferences is not lost note content");
    }

    #[test]
    fn a_dirty_editor_arms_the_warning_before_autosave_and_keeps_it_afterwards() {
        let files = Files::new();
        let root = files.0.join("NoteCraft");
        let mut changes = SessionChanges::new(&root);
        changes.observe_editor(true);
        assert!(changes.needs_warning(), "typing needs protection before a virtual file is written");
        let page = files.write("NoteCraft/book/section/page/index.md");
        changes.observe_file(&page);
        changes.observe_editor(false);
        assert!(changes.needs_warning(), "autosave only wrote a transient virtual file");
    }

    #[test]
    fn structural_changes_and_deletions_are_protected_without_a_dirty_editor() {
        let files = Files::new();
        let root = files.0.join("NoteCraft");
        let node = files.write("NoteCraft/book/section/new-page/_node.toml");
        let page = files.write("NoteCraft/book/section/old-page/index.html");
        let mut added = SessionChanges::new(&root);
        added.observe_file(&node);
        added.observe_editor(false);
        assert!(added.needs_warning(), "a newly created page may already be clean");
        let mut deleted = SessionChanges::new(&root);
        fs::remove_file(&page).expect("remove owned fixture");
        deleted.observe_file(&page);
        deleted.observe_editor(false);
        assert!(deleted.needs_warning(), "deletions also disappear on reload");
    }
}

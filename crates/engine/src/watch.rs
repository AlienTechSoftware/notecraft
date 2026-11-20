//! Live file watcher for external folder sync (e.g. Syncthing, OneDrive, Dropbox).
//!
//! NoteCraft writes pages atomically through temporary files and renames. External sync
//! tools also create temporary staging files (`.syncthing.*.tmp`, `.stfolder`, `*.tmp`) and
//! update page directories outside the window's own thread. This watcher filters out sync
//! artifacts and the view's own known page revisions, debounces bursts over [`DEBOUNCE`],
//! and wakes the UI only when external changes settle on disk.

use notecraft_store::coordination::Revision;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// How long an external change must remain settled before notifying the view.
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Which parts of the open workspace changed on disk outside this view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WatchChanges {
    pub page: bool,
    pub tree: bool,
}

/// Whether `path` belongs to a sync temporary file, backup folder, or internal metadata dir
/// that should never trigger a reload.
pub fn is_sync_temp(path: &Path) -> bool {
    for comp in path.components() {
        let name = comp.as_os_str().to_string_lossy();
        if name == ".git" || name == ".notecraft" {
            return true;
        }
        if name.starts_with(".st") || name.starts_with(".syncthing.") || name.starts_with("~syncthing~") || name.starts_with(".notecraft-tmp") {
            return true;
        }
    }
    let Some(file_name) = path.file_name().map(|n| n.to_string_lossy()) else {
        return false;
    };
    file_name.starts_with(".~")
        || file_name.starts_with("~$")
        || file_name.ends_with(".tmp")
        || file_name.ends_with(".part")
        || file_name.ends_with(".crdownload")
        || file_name.ends_with(".swp")
        || file_name.ends_with('~')
}

#[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
mod imp {
    use super::*;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Condvar, Mutex};
    use std::time::{Instant, SystemTime};

    const POLL_STEP: Duration = Duration::from_millis(50);
    #[cfg(target_os = "android")]
    const TREE_SCAN_STEP: Duration = Duration::from_millis(750);
    #[cfg(not(target_os = "android"))]
    const TREE_SCAN_STEP: Duration = Duration::from_millis(500);

    #[derive(Clone, Default, PartialEq, Eq)]
    struct PageStamp {
        revision: Option<Revision>,
        assets: Vec<(String, u64, Option<SystemTime>)>,
    }

    #[derive(Clone, Default, PartialEq, Eq)]
    struct TreeStamp {
        nodes: BTreeMap<PathBuf, (bool, Option<(u64, Option<SystemTime>)>)>,
    }

    #[derive(Default)]
    struct Target {
        roots: Vec<PathBuf>,
        roots_gen: u64,
        page: Option<PathBuf>,
        page_gen: u64,
        own_revision: Option<Revision>,
    }

    struct Shared {
        wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        target: Mutex<Target>,
        changes: Mutex<WatchChanges>,
        stopped: Mutex<bool>,
        cv: Condvar,
        has_target: AtomicBool,
    }

    /// Background watcher that debounces external disk changes under the open notebook root
    /// and active page folder.
    pub struct FileWatcher {
        shared: Arc<Shared>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl Default for FileWatcher {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FileWatcher {
        pub fn new() -> Self {
            let shared = Arc::new(Shared {
                wake: Mutex::new(None),
                target: Mutex::new(Target::default()),
                changes: Mutex::new(WatchChanges::default()),
                stopped: Mutex::new(false),
                cv: Condvar::new(),
                has_target: AtomicBool::new(false),
            });
            let bg = Arc::clone(&shared);
            let worker = std::thread::Builder::new().name("notecraft-file-watcher".into()).spawn(move || run_loop(&bg)).ok();
            Self { shared, worker }
        }

        pub fn set_wake(&self, wake: Arc<dyn Fn() + Send + Sync>) {
            if let Ok(mut guard) = self.shared.wake.lock() {
                *guard = Some(wake);
            }
        }

        pub fn watch(&self, root: &Path, page: Option<&Path>, own_revision: Option<Revision>) {
            self.watch_roots(&[root.to_path_buf()], page, own_revision);
        }

        pub fn watch_roots(&self, roots: &[PathBuf], page: Option<&Path>, own_revision: Option<Revision>) {
            let page_buf = page.map(Path::to_path_buf);
            if let Ok(mut target) = self.shared.target.lock() {
                let roots_changed = target.roots.as_slice() != roots;
                let page_changed = target.page.as_ref() != page_buf.as_ref();
                if roots_changed {
                    target.roots = roots.to_vec();
                    target.roots_gen = target.roots_gen.wrapping_add(1);
                }
                if page_changed {
                    target.page = page_buf;
                    target.page_gen = target.page_gen.wrapping_add(1);
                }
                if roots_changed || page_changed || target.own_revision != own_revision {
                    target.own_revision = own_revision;
                }
                if roots_changed || page_changed {
                    self.shared.has_target.store(true, Ordering::Release);
                    self.shared.cv.notify_all();
                }
            }
        }

        pub fn note_own_revision(&self, page: &Path, revision: Option<Revision>) {
            if let Ok(mut target) = self.shared.target.lock()
                && target.page.as_deref() == Some(page)
            {
                target.own_revision = revision;
            }
        }

        pub fn take_changes(&self) -> WatchChanges {
            if let Ok(mut guard) = self.shared.changes.lock() { std::mem::take(&mut *guard) } else { WatchChanges::default() }
        }
    }

    impl Drop for FileWatcher {
        fn drop(&mut self) {
            if let Ok(mut stopped) = self.shared.stopped.lock() {
                *stopped = true;
            }
            self.shared.cv.notify_all();
            if let Some(handle) = self.worker.take() {
                let _ = handle.join();
            }
        }
    }

    fn run_loop(shared: &Shared) {
        let mut tracked_roots_gen: u64 = 0;
        let mut tracked_page_gen: u64 = 0;
        let mut baseline_page: Option<PageStamp> = None;
        let mut pending_page: Option<(PageStamp, Instant)> = None;
        let mut baseline_tree: Option<TreeStamp> = None;
        let mut pending_tree: Option<(TreeStamp, Instant)> = None;
        let mut last_tree_scan: Option<Instant> = None;

        loop {
            {
                let Ok(guard) = shared.stopped.lock() else { return };
                if *guard {
                    return;
                }
                let wait_for = if shared.has_target.load(Ordering::Acquire) { POLL_STEP } else { DEBOUNCE };
                let Ok((stopped, _)) = shared.cv.wait_timeout(guard, wait_for) else { return };
                if *stopped {
                    return;
                }
            }

            let (roots_gen, page_gen, roots, page, own_rev) = {
                let Ok(target) = shared.target.lock() else { continue };
                (target.roots_gen, target.page_gen, target.roots.clone(), target.page.clone(), target.own_revision.clone())
            };

            if page_gen != tracked_page_gen {
                tracked_page_gen = page_gen;
                baseline_page = page.as_deref().map(read_page_stamp);
                pending_page = None;
            }
            if roots_gen != tracked_roots_gen {
                tracked_roots_gen = roots_gen;
                baseline_tree = (!roots.is_empty()).then(|| read_tree_stamp(&roots));
                pending_tree = None;
                last_tree_scan = Some(Instant::now());
            }

            let mut page_hit = false;
            let mut tree_hit = false;

            if let Some(page_dir) = page.as_deref() {
                let current = read_page_stamp(page_dir);
                let is_own = own_rev.is_some() && current.revision == own_rev;
                if let Some(base) = &mut baseline_page {
                    if is_own && current.assets == base.assets {
                        *base = current;
                        pending_page = None;
                    } else if &current != base {
                        let now = Instant::now();
                        match &pending_page {
                            Some((cand, since)) if cand == &current => {
                                if now.duration_since(*since) >= DEBOUNCE {
                                    *base = current;
                                    pending_page = None;
                                    page_hit = true;
                                }
                            }
                            _ => {
                                pending_page = Some((current, now));
                            }
                        }
                    } else {
                        pending_page = None;
                    }
                } else {
                    baseline_page = Some(current);
                }
            }

            if !roots.is_empty() {
                let now = Instant::now();
                let tree_step = if pending_tree.is_some() { DEBOUNCE } else { TREE_SCAN_STEP };
                if last_tree_scan.is_none_or(|prev| now.duration_since(prev) >= tree_step) {
                    last_tree_scan = Some(now);
                    let current = read_tree_stamp(&roots);
                    if let Some(base) = &mut baseline_tree {
                        if &current != base {
                            match &pending_tree {
                                Some((cand, since)) if cand == &current => {
                                    if now.duration_since(*since) >= DEBOUNCE {
                                        *base = current;
                                        pending_tree = None;
                                        tree_hit = true;
                                    }
                                }
                                _ => pending_tree = Some((current, now)),
                            }
                        } else {
                            pending_tree = None;
                        }
                    } else {
                        baseline_tree = Some(current);
                    }
                }
            }
            if page_hit || tree_hit {
                if let Ok(mut c) = shared.changes.lock() {
                    c.page |= page_hit;
                    c.tree |= tree_hit;
                }
                if let Some(wake) = shared.wake.lock().ok().and_then(|g| g.clone()) {
                    wake();
                }
            }
        }
    }

    fn read_page_stamp(dir: &Path) -> PageStamp {
        let revision = Revision::read(dir).ok();
        let mut assets = Vec::new();
        let assets_dir = dir.join("assets");
        if let Ok(entries) = std::fs::read_dir(&assets_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if is_sync_temp(&path) {
                    continue;
                }
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    assets.push((name, meta.len(), meta.modified().ok()));
                }
            }
        }
        assets.sort_by(|a, b| a.0.cmp(&b.0));
        PageStamp { revision, assets }
    }

    fn read_tree_stamp(roots: &[PathBuf]) -> TreeStamp {
        let mut nodes = BTreeMap::new();
        for root in roots {
            collect_tree(root, &mut nodes, 0);
        }
        TreeStamp { nodes }
    }

    fn collect_tree(dir: &Path, out: &mut BTreeMap<PathBuf, (bool, Option<(u64, Option<SystemTime>)>)>, depth: usize) {
        if depth > 24 || out.len() >= 10_000 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            if name == "assets" || is_sync_temp(&path) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if !kind.is_dir() {
                continue;
            }
            let has_page = notecraft_store::tree::page_file(&path).is_some();
            let node_meta = std::fs::metadata(path.join("_node.toml")).ok().map(|m| (m.len(), m.modified().ok()));
            out.insert(path.clone(), (has_page, node_meta));
            collect_tree(&path, out, depth + 1);
        }
    }
}

#[cfg(any(target_arch = "wasm32", target_os = "ios"))]
mod imp {
    use super::*;

    pub struct FileWatcher;

    impl Default for FileWatcher {
        fn default() -> Self {
            Self::new()
        }
    }

    impl FileWatcher {
        pub fn new() -> Self {
            Self
        }

        pub fn set_wake(&self, _wake: Arc<dyn Fn() + Send + Sync>) {}

        pub fn watch(&self, _root: &Path, _page: Option<&Path>, _own_revision: Option<Revision>) {}

        pub fn watch_roots(&self, _roots: &[PathBuf], _page: Option<&Path>, _own_revision: Option<Revision>) {}

        pub fn note_own_revision(&self, _page: &Path, _revision: Option<Revision>) {}

        pub fn take_changes(&self) -> WatchChanges {
            WatchChanges::default()
        }
    }
}

pub use imp::FileWatcher;

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn sync_temp_files_are_filtered_out() {
        for ignored in [
            Path::new(".stfolder"),
            Path::new(".stversions/page/index.md"),
            Path::new("notebook/.stignore"),
            Path::new("page/.syncthing.index.md.tmp"),
            Path::new("page/~syncthing~ink.svg.tmp"),
            Path::new("page/index.md.tmp"),
            Path::new("page/index.html~"),
            Path::new("page/.~lock.index.md#"),
            Path::new("page/assets/pic.png.part"),
            Path::new(".git/HEAD"),
            Path::new("page/.notecraft/versions/123"),
        ] {
            assert!(is_sync_temp(ignored), "expected sync temp to be ignored: {}", ignored.display());
        }

        for real in [
            Path::new("page/index.md"),
            Path::new("page/index.html"),
            Path::new("page/ink.svg"),
            Path::new("section/_node.toml"),
            Path::new("page/assets/diagram.png"),
        ] {
            assert!(!is_sync_temp(real), "expected real notebook file to pass: {}", real.display());
        }
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    #[test]
    fn watcher_ignores_own_revision_and_wakes_on_debounced_external_write() {
        let dir = notecraft_store::testing::TempDir::new("engine-file-watcher");
        let root = dir.path().join("notebooks");
        let page = root.join("work").join("notes").join("page-one");
        std::fs::create_dir_all(page.join("assets")).expect("create page dirs");
        let index = page.join("index.md");
        std::fs::write(&index, "# Initial\n\nhello\n").expect("write initial");

        let rev = Revision::read(&page).expect("initial revision");
        let watcher = FileWatcher::new();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_bg = Arc::clone(&wakes);
        watcher.set_wake(Arc::new(move || {
            wakes_bg.fetch_add(1, Ordering::SeqCst);
        }));
        watcher.watch(&root, Some(&page), Some(rev.clone()));

        // Wait longer than DEBOUNCE while nothing external changes; sync temp files must not wake.
        std::fs::write(page.join(".syncthing.index.md.tmp"), "staging").expect("write sync tmp");
        std::fs::write(page.join("assets").join("photo.png.tmp"), "partial").expect("write asset tmp");
        std::thread::sleep(Duration::from_millis(260));
        assert_eq!(watcher.take_changes(), WatchChanges::default());
        assert_eq!(wakes.load(Ordering::SeqCst), 0);

        // Simulate our own save followed by note_own_revision: must not emit external change.
        std::fs::write(&index, "# Initial\n\nown save update\n").expect("write own save");
        let own_rev = Revision::read(&page).expect("own revision");
        watcher.note_own_revision(&page, Some(own_rev));
        std::thread::sleep(Duration::from_millis(260));
        assert_eq!(watcher.take_changes(), WatchChanges::default());
        assert_eq!(wakes.load(Ordering::SeqCst), 0);

        // External write (e.g. Syncthing replacing index.md): must debounce and wake.
        std::fs::write(&index, "# Initial\n\nexternal synced words from another device\n").expect("external write");
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut got = WatchChanges::default();
        while std::time::Instant::now() < deadline {
            if watcher.take_changes().page {
                got.page = true;
            }
            if got.page && wakes.load(Ordering::SeqCst) >= 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(got.page, "expected debounced external page change");
        assert!(wakes.load(Ordering::SeqCst) >= 1, "expected wake callback to fire");
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    #[test]
    fn external_notebook_tree_changes_report_tree_changed() {
        let dir = notecraft_store::testing::TempDir::new("engine-tree-watcher");
        let root = dir.path().join("notebooks");
        let section = root.join("work").join("notes");
        std::fs::create_dir_all(&section).expect("create root");

        let watcher = FileWatcher::new();
        let wakes = Arc::new(AtomicUsize::new(0));
        let wakes_bg = Arc::clone(&wakes);
        watcher.set_wake(Arc::new(move || {
            wakes_bg.fetch_add(1, Ordering::SeqCst);
        }));
        watcher.watch(&root, None, None);
        std::thread::sleep(Duration::from_millis(120));
        let _ = watcher.take_changes();

        let new_page = section.join("synced-page");
        std::fs::create_dir_all(&new_page).expect("create synced page dir");
        std::fs::write(new_page.join("index.md"), "# Synced\n").expect("write synced page");

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut saw_tree = false;
        while std::time::Instant::now() < deadline {
            if watcher.take_changes().tree {
                saw_tree = true;
            }
            if saw_tree && wakes.load(Ordering::SeqCst) >= 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(saw_tree, "expected external directory creation to report tree change");
        assert!(wakes.load(Ordering::SeqCst) >= 1);
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "ios")))]
    #[test]
    fn page_switch_preserves_pending_tree_change() {
        let dir = notecraft_store::testing::TempDir::new("engine-tree-page-switch");
        let root = dir.path().join("notebooks");
        let section = root.join("work").join("notes");
        let page1 = section.join("page-one");
        let page2 = section.join("page-two");
        std::fs::create_dir_all(&page1).expect("create page1");
        std::fs::create_dir_all(&page2).expect("create page2");
        std::fs::write(page1.join("index.md"), "# One\n").expect("write page1");
        std::fs::write(page2.join("index.md"), "# Two\n").expect("write page2");
        let watcher = FileWatcher::new();
        watcher.watch(&root, Some(&page1), None);
        std::thread::sleep(Duration::from_millis(120));
        let _ = watcher.take_changes();
        let new_page = section.join("synced-page");
        std::fs::create_dir_all(&new_page).expect("create synced page");
        std::fs::write(new_page.join("index.md"), "# Synced\n").expect("write synced page");
        std::thread::sleep(Duration::from_millis(60));
        watcher.watch(&root, Some(&page2), None);
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut saw_tree = false;
        while std::time::Instant::now() < deadline {
            if watcher.take_changes().tree {
                saw_tree = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(saw_tree, "expected page switch to preserve pending tree change");
    }
}

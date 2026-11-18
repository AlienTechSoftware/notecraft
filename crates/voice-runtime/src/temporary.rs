//! Private process files, outside the host's documents, with an active lease.

use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

pub(crate) struct JobFiles {
    _lease: File,
    pub dir: tempfile::TempDir,
}

impl JobFiles {
    pub fn new(root: &Path) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|e| format!("cannot create voice temporary folder: {e}"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| format!("cannot protect voice temporary folder: {e}"))?;
        }
        cleanup(root);
        let dir = tempfile::Builder::new().prefix("job-").tempdir_in(root).map_err(|e| format!("cannot create voice process files: {e}"))?;
        let lease = File::create(dir.path().join("lease")).map_err(|e| e.to_string())?;
        lease.lock().map_err(|e| e.to_string())?;
        Ok(Self { dir, _lease: lease })
    }
}

fn cleanup(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("job-") || !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = entry.path();
        let Ok(file) = File::options().read(true).write(true).open(path.join("lease")) else { continue };
        let old = file
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|d| d > Duration::from_secs(60));
        if old && file.try_lock().is_ok() {
            let _ = fs::remove_dir_all(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_files_are_removed_on_drop() {
        let root = tempfile::tempdir().expect("root");
        let job = JobFiles::new(root.path()).expect("job");
        let path = job.dir.path().to_path_buf();
        fs::write(path.join("input.wav"), b"private audio fixture").expect("audio");
        drop(job);
        assert!(!path.exists());
    }

    #[test]
    fn cleanup_removes_abandoned_jobs_but_not_active_jobs() {
        let root = tempfile::tempdir().expect("root");
        let active = JobFiles::new(root.path()).expect("active");
        let old = SystemTime::now() - Duration::from_secs(120);
        active._lease.set_modified(old).expect("age active lease");
        let abandoned = root.path().join("job-abandoned");
        fs::create_dir(&abandoned).expect("abandoned folder");
        let file = File::create(abandoned.join("lease")).expect("lease");
        file.set_modified(old).expect("age abandoned lease");
        drop(file);
        cleanup(root.path());
        assert!(active.dir.path().exists());
        assert!(!abandoned.exists());
    }
}

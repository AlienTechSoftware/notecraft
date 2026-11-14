//! The Welcome notebook inside the APK.
//!
//! `android/build.sh` packs both generated editions (`docs/demo/DemoNotebook/html` and
//! `cmgfm`) into one asset, [`ASSET`]. At startup the app unpacks it into its own internal
//! storage, once per app version. That copy is application data, like the `demo` folder the
//! desktop installers put beside the program: nothing in the user's notebooks changes here.
//! The first-use wizard then asks, and copies only the edition the user picks into the
//! notebooks folder, exactly as on the desktop (`notecraft_setup::install_edition`).

use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};

/// The asset the APK carries the payload in.
pub const ASSET: &str = "welcome-to-notecraft.zip";
/// The notebook's folder name, the same on every platform.
pub const NOTEBOOK_NAME: &str = "welcome-to-notecraft";
/// The file in the unpacked payload that names the app version it came from.
pub const STAMP: &str = ".payload-version";
/// The two editions; both must be in the payload.
pub const FORMATS: [&str; 2] = ["html", "cmgfm"];

/// The most the payload may unpack to. The real one is a few megabytes; this stops a damaged
/// asset from filling the device.
const MAX_UNPACKED: u64 = 512 * 1024 * 1024;

/// Whether `payload` already holds both editions from `version`.
pub fn is_current(payload: &Path, version: &str) -> bool {
    fs::read_to_string(payload.join(STAMP)).is_ok_and(|v| v.trim() == version) && FORMATS.iter().all(|f| payload.join(f).is_dir())
}

/// Unpacks the payload archive into `payload` unless it is already there for `version`.
/// Returns whether it unpacked. The old copy is replaced only once the new one is complete,
/// so a failure leaves whatever was there before.
pub fn refresh(archive: impl Read + Seek, payload: &Path, version: &str) -> io::Result<bool> {
    if is_current(payload, version) {
        return Ok(false);
    }
    let parent = payload.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the payload folder has no parent"))?;
    fs::create_dir_all(parent)?;
    let staging = sibling(payload, "unpacking");
    let result = unpack(archive, &staging).and_then(|()| {
        for format in FORMATS {
            if !staging.join(format).is_dir() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, format!("the Welcome notebook asset has no {format} edition")));
            }
        }
        fs::write(staging.join(STAMP), version)
    });
    if let Err(error) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if payload.exists() {
        let old = sibling(payload, "old");
        fs::rename(payload, &old)?;
        if let Err(error) = fs::rename(&staging, payload) {
            let _ = fs::rename(&old, payload);
            let _ = fs::remove_dir_all(&staging);
            return Err(error);
        }
        let _ = fs::remove_dir_all(&old);
    } else {
        fs::rename(&staging, payload)?;
    }
    Ok(true)
}

/// A folder beside `path` that no other run uses.
fn sibling(path: &Path, what: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    path.with_file_name(format!(".{name}-{what}-{}-{stamp}", std::process::id()))
}

/// Writes every file of the archive under `into`. A name that would land outside `into`
/// stops the unpacking.
fn unpack(archive: impl Read + Seek, into: &Path) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(archive).map_err(io::Error::other)?;
    fs::create_dir_all(into)?;
    let mut written: u64 = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(io::Error::other)?;
        let Some(relative) = entry.enclosed_name() else {
            return Err(io::Error::new(io::ErrorKind::InvalidData, format!("unsafe name in the Welcome notebook asset: {}", entry.name())));
        };
        let target = into.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(folder) = target.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut file = fs::File::create(&target)?;
        written += io::copy(&mut (&mut entry).take(MAX_UNPACKED.saturating_sub(written) + 1), &mut file)?;
        if written > MAX_UNPACKED {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "the Welcome notebook asset unpacks to more than 512 MB"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;
    use std::io::{Cursor, Write};

    /// The real payload, packed the way `android/build.sh` packs it.
    fn demo_archive() -> Vec<u8> {
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/demo/DemoNotebook");
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        let mut stack: Vec<PathBuf> = FORMATS.iter().map(|f| demo.join(f)).collect();
        while let Some(path) = stack.pop() {
            let relative = path.strip_prefix(&demo).expect("inside the demo").to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                zip.add_directory(format!("{relative}/"), options).expect("folder");
                stack.extend(fs::read_dir(&path).expect("read").map(|e| e.expect("entry").path()));
            } else {
                zip.start_file(relative, options).expect("file");
                zip.write_all(&fs::read(&path).expect("read file")).expect("write");
            }
        }
        zip.finish().expect("finish").into_inner()
    }

    fn files_under(root: &Path) -> usize {
        let mut count = 0;
        let mut stack = vec![root.to_path_buf()];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                stack.extend(fs::read_dir(&path).expect("read").map(|e| e.expect("entry").path()));
            } else {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn the_payload_unpacks_both_editions_once_per_version() {
        let t = TempDir::new("android-welcome");
        let payload = t.path().join("demo").join(NOTEBOOK_NAME);
        let archive = demo_archive();

        assert!(refresh(Cursor::new(&archive), &payload, "0.0.10").expect("unpack"));
        assert!(is_current(&payload, "0.0.10"));
        let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/demo/DemoNotebook");
        for format in FORMATS {
            assert_eq!(files_under(&payload.join(format)), files_under(&demo.join(format)), "every {format} file is unpacked");
        }
        assert!(!refresh(Cursor::new(&archive), &payload, "0.0.10").expect("again"), "the same version is left alone");

        fs::write(payload.join("html").join("stale.txt"), "left by an older version").expect("stale file");
        assert!(refresh(Cursor::new(&archive), &payload, "0.0.11").expect("new version"));
        assert!(!payload.join("html").join("stale.txt").exists(), "a new version replaces the old payload");
        let leftovers: Vec<_> = fs::read_dir(payload.parent().expect("parent"))
            .expect("read")
            .map(|e| e.expect("entry").file_name())
            .filter(|n| n != NOTEBOOK_NAME)
            .collect();
        assert!(leftovers.is_empty(), "no staging folders are left behind: {leftovers:?}");
    }

    #[test]
    fn the_unpacked_payload_installs_through_the_desktop_setup_code() {
        let t = TempDir::new("android-welcome-install");
        let payload = t.path().join("demo").join(NOTEBOOK_NAME);
        refresh(Cursor::new(demo_archive()), &payload, "0.0.10").expect("unpack");
        let notebooks = t.path().join("NoteCraft");
        fs::create_dir_all(notebooks.join("my-notes")).expect("sibling");
        fs::write(notebooks.join("my-notes").join("keep.txt"), "mine").expect("sibling file");

        let installed = notecraft_setup_install(&payload, &notebooks, "cmgfm");
        assert_eq!(installed, notebooks.join(NOTEBOOK_NAME));
        assert!(installed.is_dir());
        assert_eq!(fs::read_to_string(notebooks.join("my-notes").join("keep.txt")).expect("kept"), "mine", "siblings stay untouched");
    }

    /// The installer the window calls after the wizard's Yes, reached the way the app reaches it.
    fn notecraft_setup_install(payload: &Path, notebooks: &Path, format: &str) -> PathBuf {
        notecraft_setup::install_edition(payload, notebooks, format).expect("install")
    }

    #[test]
    fn an_archive_without_both_editions_changes_nothing() {
        let t = TempDir::new("android-welcome-partial");
        let payload = t.path().join(NOTEBOOK_NAME);
        let archive = demo_archive();
        refresh(Cursor::new(&archive), &payload, "1").expect("first");

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("html/only.txt", zip::write::SimpleFileOptions::default()).expect("file");
        zip.write_all(b"one edition").expect("write");
        let partial = zip.finish().expect("finish").into_inner();

        assert!(refresh(Cursor::new(partial), &payload, "2").is_err());
        assert!(is_current(&payload, "1"), "the earlier payload is still there");
    }

    #[test]
    fn a_name_outside_the_payload_is_refused() {
        let t = TempDir::new("android-welcome-escape");
        let payload = t.path().join("demo").join(NOTEBOOK_NAME);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("../escaped.txt", zip::write::SimpleFileOptions::default()).expect("file");
        zip.write_all(b"outside").expect("write");
        let bad = zip.finish().expect("finish").into_inner();

        assert!(refresh(Cursor::new(bad), &payload, "1").is_err());
        assert!(!t.path().join("demo").join("escaped.txt").exists());
        assert!(!t.path().join("escaped.txt").exists());
        assert!(!payload.exists());
    }
}

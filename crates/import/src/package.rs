//! OneNote packages: a `.onepkg` file is a notebook packed into a Windows cabinet file, almost
//! always LZX compressed, with the notebook's `.onetoc2` and its `.one` sections inside and the
//! section groups as folders (issue #17). A package is unpacked into a folder of its own and
//! imported from there like any other notebook folder.
//!
//! Nothing from inside a package is trusted: every name is checked before anything is written,
//! a name that would leave the folder is refused, and the number and size of the files are
//! capped, so a hostile package cannot write outside its folder or fill the disk.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// The most files one package may hold.
pub const MAX_FILES: usize = 10_000;
/// The most bytes one package may unpack to, all files together.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// True when `path` names a OneNote package.
pub fn is_package(path: &Path) -> bool {
    path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("onepkg"))
}

/// A name from inside a package as a path under the folder it is unpacked into, or `None` when
/// it is not one: empty, absolute, with a drive, or with a `..` or `.` in it. Cabinets name
/// folders with a backslash; a forward slash is taken as one too.
pub fn safe_name(name: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in name.split(['\\', '/']) {
        let part = part.trim_end_matches([' ', '.']);
        if part.is_empty() || part.contains(':') || part.chars().any(char::is_control) {
            return None;
        }
        let as_path = Path::new(part);
        if as_path.components().count() != 1 || !matches!(as_path.components().next(), Some(Component::Normal(_))) {
            return None;
        }
        out.push(part);
    }
    (!out.as_os_str().is_empty()).then_some(out)
}

/// Unpacks the package at `package` into the folder `into`, which is made. Returns how many
/// files were written. Fails, having written what it had, when the package cannot be read,
/// holds a name that is not a safe path, or holds more than [`MAX_FILES`] or [`MAX_BYTES`].
pub fn unpack(package: &Path, into: &Path) -> Result<usize, String> {
    let file = fs::File::open(package).map_err(|e| format!("cannot open the package: {e}"))?;
    let mut cabinet = cab::Cabinet::new(file).map_err(|e| format!("not a OneNote package: {e}"))?;
    let entries: Vec<(String, u64)> =
        cabinet.folder_entries().flat_map(|folder| folder.file_entries().map(|f| (f.name().to_string(), u64::from(f.uncompressed_size())))).collect();
    if entries.len() > MAX_FILES {
        return Err(format!("the package holds more than {MAX_FILES} files"));
    }
    let total: u64 = entries.iter().map(|(_, size)| size).sum();
    if total > MAX_BYTES {
        return Err("the package unpacks to more than the importer will write".to_string());
    }
    let mut paths = Vec::with_capacity(entries.len());
    for (name, size) in entries {
        let Some(rel) = safe_name(&name) else { return Err(format!("the package names a file outside itself: {name:?}")) };
        paths.push((name, rel, size));
    }
    fs::create_dir_all(into).map_err(|e| format!("cannot make {}: {e}", into.display()))?;
    let mut written = 0;
    for (name, rel, size) in paths {
        let target = into.join(&rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
        }
        let reader = cabinet.read_file(&name).map_err(|e| format!("cannot read {name:?} from the package: {e}"))?;
        let mut out = fs::File::create(&target).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        // No more than the package says the file holds, whatever its compressed data claims.
        let mut limited = reader.take(size);
        let mut buf = vec![0_u8; 64 * 1024];
        loop {
            let n = limited.read(&mut buf).map_err(|e| format!("cannot unpack {name:?}: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(buf.get(..n).unwrap_or_default()).map_err(|e| format!("cannot write {}: {e}", target.display()))?;
        }
        written += 1;
    }
    Ok(written)
}

/// A folder under the system's temporary folder that is removed again when it is dropped:
/// where packages are unpacked for one import run.
pub struct Scratch {
    pub path: PathBuf,
}

impl Scratch {
    pub fn new() -> Result<Scratch, String> {
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or_default();
        let path = std::env::temp_dir().join(format!("notecraft-onepkg-{}-{stamp}", std::process::id()));
        fs::create_dir_all(&path).map_err(|e| format!("cannot make a folder to unpack into: {e}"))?;
        Ok(Scratch { path })
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        // Best effort: the system clears its temporary folder in the end if this cannot.
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::testing::TempDir;

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/3p/officeimo");

    #[test]
    fn names_inside_a_package_stay_inside_its_folder() {
        assert_eq!(safe_name("Open Notebook.onetoc2"), Some(PathBuf::from("Open Notebook.onetoc2")));
        assert_eq!(safe_name("Group\\Section.one"), Some(PathBuf::from("Group").join("Section.one")));
        assert_eq!(safe_name("Group/Section.one"), Some(PathBuf::from("Group").join("Section.one")));
        for bad in [
            "",
            "..\\evil.one",
            "a\\..\\..\\evil.one",
            "\\evil.one",
            "/evil.one",
            "C:\\evil.one",
            "C:evil.one",
            "a\\.\\b.one",
            "a\\\\b.one",
            "bad\u{0}.one",
            "..",
            ".",
        ] {
            assert_eq!(safe_name(bad), None, "{bad:?} is refused");
        }
    }

    /// The OfficeIMO cabinets are LZX compressed, as OneNote's own packages are: what comes out
    /// is byte for byte the section they were made from.
    #[test]
    fn an_lzx_cabinet_unpacks_to_the_section_it_was_made_from() {
        let t = TempDir::new("onepkg-lzx");
        for (cabinet, section) in [
            ("makecab-lzx-testOneNote2016.cab", "testOneNote2016.one"),
            ("makecab-lzx-testOneNoteFromOffice365-2.cab", "testOneNoteFromOffice365-2.one"),
        ] {
            let into = t.path().join(cabinet);
            assert_eq!(unpack(&Path::new(FIXTURES).join(cabinet), &into), Ok(1), "{cabinet}");
            let out = fs::read(into.join(section)).expect("unpacked");
            let want = fs::read(Path::new(FIXTURES).join(section)).expect("original");
            assert!(out == want, "{cabinet}: {} bytes unpacked, {} expected", out.len(), want.len());
        }
    }

    #[test]
    fn a_notebook_package_unpacks_its_toc_and_sections() {
        let t = TempDir::new("onepkg-notebook");
        let into = t.path().join("out");
        assert_eq!(unpack(&Path::new(FIXTURES).join("makecab-lzx-notebook.onepkg"), &into), Ok(2));
        assert!(into.join("Open Notebook.onetoc2").is_file());
        assert!(into.join("Compressed section.one").is_file());
    }

    /// A package made here, with a name that climbs out of the folder: nothing is written.
    #[test]
    fn a_package_naming_a_file_outside_itself_writes_nothing() {
        let t = TempDir::new("onepkg-hostile");
        let package = t.path().join("hostile.onepkg");
        let mut builder = cab::CabinetBuilder::new();
        builder.add_folder(cab::CompressionType::None).add_file("..\\escaped.one");
        let mut writer = builder.build(fs::File::create(&package).expect("create")).expect("build");
        while let Some(mut file) = writer.next_file().expect("next") {
            file.write_all(b"not really a section").expect("write");
        }
        writer.finish().expect("finish");
        let into = t.path().join("inside").join("out");
        assert!(unpack(&package, &into).is_err());
        assert!(!t.path().join("inside").join("escaped.one").exists(), "nothing escaped");
        assert!(!into.exists(), "and nothing was written at all");
    }

    #[test]
    fn junk_is_an_error_not_a_panic() {
        let t = TempDir::new("onepkg-junk");
        let package = t.path().join("junk.onepkg");
        fs::write(&package, b"MSCF but not really a cabinet at all").expect("write");
        assert!(unpack(&package, &t.path().join("out")).is_err());
        assert!(unpack(&t.path().join("missing.onepkg"), &t.path().join("out2")).is_err());
    }

    #[test]
    fn the_scratch_folder_goes_when_it_is_dropped() {
        let scratch = Scratch::new().expect("scratch");
        let path = scratch.path.clone();
        fs::write(path.join("a.one"), b"x").expect("write");
        assert!(path.is_dir());
        drop(scratch);
        assert!(!path.exists());
    }
}

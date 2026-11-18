//! Adds font files to the user's own fonts folder, so the family list can show them. Only files
//! with a font extension are copied; anything else is reported and left alone. No egui in here.

use std::path::{Path, PathBuf};

/// The extensions a font file may have.
const FONT_EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];

/// What one install did: the file names copied, and each file skipped with the reason.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub copied: Vec<String>,
    pub skipped: Vec<(String, String)>,
}

/// The fonts folder of the current user, where a font can be added without administrator rights.
pub fn user_fonts_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(|local| PathBuf::from(local).join("Microsoft").join("Windows").join("Fonts"))
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library").join("Fonts"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share").join("fonts"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// True when the file name ends with a font extension (case does not matter).
pub fn is_font_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| FONT_EXTENSIONS.iter().any(|known| known.eq_ignore_ascii_case(ext)))
}

/// True when both paths name the same file on disk. A target that does not exist yet is never the
/// same file.
fn same_file(source: &Path, target: &Path) -> bool {
    match (std::fs::canonicalize(source), std::fs::canonicalize(target)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Copies each font file into `dir`, creating it when needed. A file that is not a font, or that
/// cannot be copied, is skipped and reported with its reason.
pub fn install(files: &[PathBuf], dir: &Path) -> Report {
    let mut report = Report::default();
    let name_of = |path: &Path| path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if let Err(error) = std::fs::create_dir_all(dir) {
        for file in files {
            report.skipped.push((name_of(file), format!("the fonts folder cannot be created: {error}")));
        }
        return report;
    }
    for file in files {
        let name = name_of(file);
        if !is_font_file(file) {
            report.skipped.push((name, "not a font file".to_owned()));
            continue;
        }
        let target = dir.join(&name);
        // The file is already in the fonts folder: copying it onto itself would empty it.
        if same_file(file, &target) {
            report.skipped.push((name, "already in the fonts folder".to_owned()));
            continue;
        }
        // Another font with this name is installed: refuse rather than overwrite it.
        if target.exists() {
            report.skipped.push((name, "a different font with this name is already installed".to_owned()));
            continue;
        }
        match std::fs::copy(file, &target) {
            Ok(_) => report.copied.push(name),
            Err(error) => report.skipped.push((name, error.to_string())),
        }
    }
    report
}

#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
/// Asks for font files (several may be picked), installs them in the user's fonts folder, and
/// reports what happened. `None` when the dialog was cancelled. Events and log lines go out here,
/// so every caller records the same things.
pub fn pick_and_install() -> Option<Report> {
    let picked = rfd::FileDialog::new()
        .set_title(crate::tl_id!("ui.add_font_dialog_title"))
        .add_filter(crate::tl_id!("ui.font_files_filter").to_string(), &FONT_EXTENSIONS)
        .pick_files();
    let Some(files) = picked else {
        crate::usage::record(crate::usage::Level::Usage, "font_add_cancelled", &[]);
        return None;
    };
    let Some(dir) = user_fonts_dir() else {
        crate::usage::record(crate::usage::Level::Error, "font_add_no_folder", &[]);
        log::error!("Add font: this platform has no user fonts folder");
        return Some(Report { copied: Vec::new(), skipped: files.iter().map(|f| (f.display().to_string(), "no fonts folder".to_owned())).collect() });
    };
    let report = install(&files, &dir);
    crate::usage::record(
        crate::usage::Level::Usage,
        "font_added",
        &[("copied", &report.copied.len().to_string()), ("skipped", &report.skipped.len().to_string())],
    );
    for (name, reason) in &report.skipped {
        crate::usage::record(crate::usage::Level::Warning, "font_add_skipped", &[("reason", reason)]);
        log::warn!("Add font: {name} was not added: {reason}");
    }
    log::info!("Add font: {} added to {}", report.copied.len(), dir.display());
    Some(report)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
/// Adding fonts needs the file dialog, which only the desktop builds have.
pub fn pick_and_install() -> Option<Report> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("notecraft-font-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    #[test]
    fn font_extensions_are_recognised_whatever_their_case() {
        assert!(is_font_file(Path::new("A.TTF")));
        assert!(is_font_file(Path::new("b.otf")));
        assert!(is_font_file(Path::new("c.ttc")));
        assert!(!is_font_file(Path::new("notes.md")));
        assert!(!is_font_file(Path::new("noextension")));
    }

    #[test]
    fn font_files_are_copied_and_other_files_are_reported() {
        let source = scratch("source");
        let target = scratch("target").join("fonts");
        let font = source.join("Demo.ttf");
        let note = source.join("readme.txt");
        std::fs::write(&font, b"font bytes").expect("font");
        std::fs::write(&note, b"not a font").expect("note");

        let report = install(&[font, note], &target);

        assert_eq!(report.copied, vec!["Demo.ttf".to_owned()]);
        assert_eq!(report.skipped, vec![("readme.txt".to_owned(), "not a font file".to_owned())]);
        assert_eq!(std::fs::read(target.join("Demo.ttf")).expect("copied"), b"font bytes");
    }

    #[test]
    fn a_font_already_in_the_folder_is_not_emptied() {
        let folder = scratch("in-place");
        let font = folder.join("Kept.otf");
        std::fs::write(&font, b"keep these bytes").expect("font");

        let report = install(std::slice::from_ref(&font), &folder);

        assert!(report.copied.is_empty());
        assert_eq!(report.skipped[0].1, "already in the fonts folder");
        assert_eq!(std::fs::read(&font).expect("still there"), b"keep these bytes");
    }

    #[test]
    fn a_different_font_with_the_same_name_is_not_overwritten() {
        let source = scratch("clash-source");
        let folder = scratch("clash-target");
        std::fs::write(folder.join("Same.ttf"), b"installed").expect("installed");
        let incoming = source.join("Same.ttf");
        std::fs::write(&incoming, b"incoming").expect("incoming");

        let report = install(&[incoming], &folder);

        assert!(report.copied.is_empty());
        assert_eq!(std::fs::read(folder.join("Same.ttf")).expect("kept"), b"installed");
    }

    #[test]
    fn a_missing_file_is_reported_with_its_reason() {
        let target = scratch("missing");
        let report = install(&[PathBuf::from("does-not-exist.otf")], &target);
        assert!(report.copied.is_empty());
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.skipped[0].0, "does-not-exist.otf");
    }
}

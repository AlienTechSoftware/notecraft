//! Finding the OneNote section files to import, and choosing between dated copies.
//!
//! OneNote's desktop backups keep many copies of each section, named
//! `<Section> (On 12-26-2025).one` (or, in some versions, `<Section>.one (On 12-26-2025)`),
//! inside folders that follow the notebook: the first folder is the notebook, any folders
//! below it are section groups. Nothing here reads a file's content.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A date from a backup file name: year, month, day.
pub type Date = (u16, u8, u8);

/// One `.one` file found under the source.
#[derive(Clone, Debug)]
pub struct SectionFile {
    pub path: PathBuf,
    /// The folders between the source and the file, outermost first.
    pub folders: Vec<String>,
    /// The section's name: the file name without `.one` and without the backup date.
    pub name: String,
    pub date: Option<Date>,
    pub modified: Option<SystemTime>,
    pub size: u64,
}

/// What the scan found.
#[derive(Clone, Debug, Default)]
pub struct Scan {
    pub files: Vec<SectionFile>,
    /// `.onetoc2` table-of-contents files and `.onepkg` packages: noticed, not imported yet.
    pub ignored: Vec<PathBuf>,
}

/// One section to import, after dated copies have been sorted out.
#[derive(Clone, Debug)]
pub struct Planned {
    pub file: SectionFile,
    /// How many copies of this section the source holds.
    pub copies: usize,
    /// For an older copy imported on request: the text to add to its name.
    pub version_label: Option<String>,
}

/// The place OneNote keeps its desktop data on Windows, if it is there.
///
/// This is an optional extra: nothing else in the importer knows about it. On another system,
/// or when the folder is missing, the caller must be given a path.
pub fn default_source() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from)?;
    let base = local.join("Microsoft").join("OneNote");
    for version in ["16.0", "15.0", "14.0"] {
        let backup = base.join(version).join("Backup");
        if backup.is_dir() {
            return Some(backup);
        }
    }
    base.is_dir().then_some(base)
}

/// Every `.one` file under `source` (or `source` itself, if it is one file), sorted by path.
pub fn scan(source: &Path) -> Scan {
    let mut out = Scan::default();
    if source.is_file() {
        if let Some(file) = section_file(source, source.parent().unwrap_or(Path::new(""))) {
            out.files.push(file);
        }
        return out;
    }
    walk(source, source, &mut out, 0);
    out.files.sort_by(|a, b| a.path.cmp(&b.path));
    out.ignored.sort();
    out
}

fn walk(root: &Path, dir: &Path, out: &mut Scan, depth: usize) {
    // A guard against a folder loop (a junction that points up the tree).
    if depth > 32 {
        return;
    }
    let Ok(rd) = fs::read_dir(dir) else { return };
    for entry in rd.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            walk(root, &path, out, depth + 1);
        } else if let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_lowercase()) {
            match ext.as_str() {
                "one" => out.files.extend(section_file(&path, root)),
                "onetoc2" | "onepkg" => out.ignored.push(path),
                _ => {}
            }
        }
    }
}

fn section_file(path: &Path, root: &Path) -> Option<SectionFile> {
    let meta = fs::metadata(path).ok()?;
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    let (name, date) = parse_file_name(&file_name);
    let folders = path
        .parent()
        .and_then(|p| p.strip_prefix(root).ok())
        .map(|rel| rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    Some(SectionFile { path: path.to_path_buf(), folders, name, date, modified: meta.modified().ok(), size: meta.len() })
}

/// A backup's file name into the section's name and the date in it.
///
/// `Notes (On 12-26-2025).one` and `Notes.one (On 12-26-2025)` both give `Notes` and
/// December 26, 2025. A name with no date comes back whole, without its `.one`.
pub fn parse_file_name(file_name: &str) -> (String, Option<Date>) {
    let mut name = file_name.trim().to_string();
    strip_suffix_ci(&mut name, ".one");
    let mut date = None;
    if name.ends_with(')')
        && let Some(open) = name.rfind(" (On ")
    {
        let inside = &name[open + 5..name.len() - 1];
        if let Some(d) = parse_date(inside) {
            date = Some(d);
            name.truncate(open);
        }
    }
    strip_suffix_ci(&mut name, ".one");
    let name = name.trim().to_string();
    (if name.is_empty() { "Untitled section".to_string() } else { name }, date)
}

fn strip_suffix_ci(s: &mut String, suffix: &str) {
    if s.len() >= suffix.len() && s.is_char_boundary(s.len() - suffix.len()) && s[s.len() - suffix.len()..].eq_ignore_ascii_case(suffix) {
        s.truncate(s.len() - suffix.len());
    }
}

/// `12-26-2025` (month, day, year) into a date. The month comes first because that is how
/// the backups write it; a first number above 12 is read as the day.
fn parse_date(text: &str) -> Option<Date> {
    let parts: Vec<&str> = text.trim().split(['-', '/', '.']).collect();
    if parts.len() != 3 {
        return None;
    }
    let a: u16 = parts[0].parse().ok()?;
    let b: u16 = parts[1].parse().ok()?;
    let year: u16 = parts[2].parse().ok()?;
    if !(1900..=2200).contains(&year) {
        return None;
    }
    let (month, day) = if a > 12 { (b, a) } else { (a, b) };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month as u8, day as u8))
}

/// Groups the files into sections and picks the copies to import.
///
/// Files with the same folders and the same name (ignoring case) are copies of one section.
/// The newest wins: by the date in the name, then by modification time, then by size. With
/// `all_versions`, the older copies come along too, each named with its date.
pub fn plan(files: &[SectionFile], all_versions: bool) -> Vec<Planned> {
    let mut groups: BTreeMap<(Vec<String>, String), Vec<&SectionFile>> = BTreeMap::new();
    for f in files {
        let folders = f.folders.iter().map(|s| s.to_lowercase()).collect();
        groups.entry((folders, f.name.to_lowercase())).or_default().push(f);
    }
    let mut out = Vec::new();
    for (_, mut copies) in groups {
        copies.sort_by_key(|a| std::cmp::Reverse(newest_key(a)));
        let n = copies.len();
        for (i, f) in copies.into_iter().enumerate() {
            if i == 0 {
                out.push(Planned { file: f.clone(), copies: n, version_label: None });
            } else if all_versions {
                let label = match f.date {
                    Some((y, m, d)) => format!("{y}-{m:02}-{d:02}"),
                    None => format!("copy {i}"),
                };
                out.push(Planned { file: f.clone(), copies: n, version_label: Some(label) });
            }
        }
    }
    out
}

fn newest_key(f: &SectionFile) -> (Option<Date>, Option<SystemTime>, u64) {
    (f.date, f.modified, f.size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(folders: &[&str], name: &str, date: Option<Date>, size: u64) -> SectionFile {
        SectionFile {
            path: PathBuf::from(format!("{}/{name}", folders.join("/"))),
            folders: folders.iter().map(|s| s.to_string()).collect(),
            name: name.to_string(),
            date,
            modified: None,
            size,
        }
    }

    #[test]
    fn both_backup_name_styles_give_the_name_and_the_date() {
        assert_eq!(parse_file_name("New Section 1 (On 12-26-2025).one"), ("New Section 1".to_string(), Some((2025, 12, 26))));
        assert_eq!(parse_file_name("New Section 1.one (On 12-24-2025)"), ("New Section 1".to_string(), Some((2025, 12, 24))));
        assert_eq!(parse_file_name("Notes.one"), ("Notes".to_string(), None));
        assert_eq!(parse_file_name("v2.0 plan (On 1-2-2020).one"), ("v2.0 plan".to_string(), Some((2020, 1, 2))));
    }

    #[test]
    fn text_that_is_not_a_date_stays_in_the_name() {
        assert_eq!(parse_file_name("Budget (On hold).one").0, "Budget (On hold)");
        assert_eq!(parse_file_name("Budget (On 99-99-2020).one").0, "Budget (On 99-99-2020)");
        assert_eq!(parse_file_name(".one").0, "Untitled section");
    }

    #[test]
    fn a_name_in_arabic_survives() {
        assert_eq!(parse_file_name("ملاحظات (On 3-4-2024).one"), ("ملاحظات".to_string(), Some((2024, 3, 4))));
    }

    #[test]
    fn the_newest_copy_wins_and_older_ones_come_on_request() {
        let files = vec![
            file(&["Work"], "Plan", Some((2024, 1, 5)), 10),
            file(&["Work"], "plan", Some((2025, 6, 1)), 5),
            file(&["Work"], "Other", None, 1),
            file(&["Home"], "Plan", Some((2023, 1, 1)), 1),
        ];
        let newest = plan(&files, false);
        assert_eq!(newest.len(), 3);
        let work_plan = newest.iter().find(|p| p.file.folders == ["Work"] && p.file.name.eq_ignore_ascii_case("plan")).expect("planned");
        assert_eq!(work_plan.file.date, Some((2025, 6, 1)));
        assert_eq!(work_plan.copies, 2);
        let all = plan(&files, true);
        assert_eq!(all.len(), 4);
        assert_eq!(all.iter().filter_map(|p| p.version_label.as_deref()).collect::<Vec<_>>(), ["2024-01-05"]);
    }
}

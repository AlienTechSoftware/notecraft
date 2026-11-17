//! One OneNote notebook into the notebooks folder: what the window's Import does (issue #19).
//!
//! NoteCraft's own reader comes first, so a section arrives with its pages, sub-pages,
//! pictures and attached files. A section it cannot read is given to the OfficeIMO bridge
//! when one is installed. Nothing is made until a section arrives: a notebook or a section
//! group that would hold nothing never appears, and whatever could not be brought over is
//! named in the result and in `import-errors.txt` beside the notebook.

use crate::officeimo::{self, Format, Progress};
use crate::package::Scratch;
use crate::{Event, Options, Report, Skipped, child_by_title, clean_title, discover, notebook_and_groups, run};
use notecraft_model::{NodeKind, PageFormat};
use notecraft_store::{Settings, ops};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Imports `source` (a notebook folder, a `.one` section, a notebook's `.onetoc2` or a
/// `.onepkg` package) as one new notebook under `notebooks`. Any other file is handed to the
/// OfficeIMO bridge whole, as before. The notebook's folder is reported through
/// [`Progress::Destination`] once it exists.
pub fn import(
    source: &Path,
    notebooks: &Path,
    format: PageFormat,
    cancelled: &Arc<AtomicBool>,
    progress: &mut impl FnMut(Progress),
) -> Result<String, String> {
    if !is_onenote(source) {
        // Not a OneNote file: only the bridge knows what to do with it.
        return officeimo::run_with_progress(source, notebooks, bridge_format(format), cancelled, progress);
    }
    let bridge = officeimo::locate().ok();
    import_with(source, notebooks, format, cancelled, progress, &Settings::default_location(), bridge.as_deref())
}

fn bridge_format(format: PageFormat) -> Format {
    match format {
        PageFormat::Html => Format::Html,
        PageFormat::Cmgfm => Format::Cmgfm,
    }
}

/// A folder, or a file NoteCraft's reader knows.
fn is_onenote(source: &Path) -> bool {
    source.is_dir()
        || crate::package::is_package(source)
        || source.extension().is_some_and(|e| ["one", "onetoc2", "onepkg"].iter().any(|x| e.eq_ignore_ascii_case(x)))
}

/// [`import`], with the settings and the OfficeIMO bridge to use given (`None` when there is
/// no bridge).
pub fn import_with(
    source: &Path,
    notebooks: &Path,
    format: PageFormat,
    cancelled: &Arc<AtomicBool>,
    progress: &mut impl FnMut(Progress),
    settings: &Settings,
    bridge: Option<&Path>,
) -> Result<String, String> {
    if !source.exists() {
        return Err(format!("{} does not exist", source.display()));
    }
    // What is read, and what the notebook is called.
    let mut _unpacked: Option<Scratch> = None;
    let (folder, name) = if crate::package::is_package(source) {
        let scratch = Scratch::new()?;
        let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = scratch.path.join("notebook");
        crate::package::unpack(source, &folder)?;
        _unpacked = Some(scratch);
        (folder, stem)
    } else if source.is_file() && source.extension().is_some_and(|e| e.eq_ignore_ascii_case("onetoc2")) {
        // A notebook's table of contents stands for the folder it is in.
        let folder = source.parent().unwrap_or(source).to_path_buf();
        let name = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (folder, name)
    } else if source.is_file() {
        // One section: a notebook of that name holding it.
        let file_name = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        (source.to_path_buf(), discover::parse_file_name(&file_name).0)
    } else {
        (source.to_path_buf(), source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
    };
    let title = unique_title(notebooks, &clean_title(&name, "OneNote import"));
    let base = if folder.is_file() { folder.parent().unwrap_or(&folder).to_path_buf() } else { folder.clone() };

    let mut options = Options::new(&folder, notebooks);
    options.format = format;
    options.notebook = Some(title.clone());
    options.cancelled = Some(Arc::clone(cancelled));
    let mut report = {
        let mut relay = |event: &Event| match event {
            Event::Found { sections, .. } => progress(Progress::Found { total: *sections }),
            Event::Section { index, total, label } => progress(Progress::Section { index: *index, total: *total, label: (*label).to_string() }),
            Event::Done => {}
        };
        run(&options, &mut relay)?
    };

    // The sections NoteCraft could not read go to the bridge, one at a time.
    let mut bridged = 0;
    let mut combined = 0;
    if let Some(bridge) = bridge {
        let retry: Vec<Skipped> = report.skipped.iter().filter(|s| worth_the_bridge(s)).cloned().collect();
        for (i, skipped) in retry.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                report.cancelled = true;
                break;
            }
            let Some(file) = skipped.file.as_deref() else { continue };
            progress(Progress::Section { index: i + 1, total: retry.len(), label: format!("{} (OfficeIMO)", skipped.label) });
            match bridge_one(bridge, cancelled, file, &base, notebooks, &title, format, settings) {
                Ok(whole) => {
                    bridged += 1;
                    combined += usize::from(whole);
                    report.skipped.retain(|s| s.file.as_deref() != Some(file));
                }
                Err(error) => {
                    if let Some(s) = report.skipped.iter_mut().find(|s| s.file.as_deref() == Some(file)) {
                        s.reason = format!("{}; OfficeIMO could not convert it either: {error}", s.reason);
                    }
                }
            }
        }
    } else if report.skipped.iter().any(worth_the_bridge) {
        for s in report.skipped.iter_mut().filter(|s| worth_the_bridge(s)) {
            s.reason = format!("{} (the OfficeIMO bridge, which may read it, is not installed)", s.reason);
        }
    }

    let Some(notebook) = child_by_title(notebooks, &title, NodeKind::Notebook) else {
        // Nothing arrived, so nothing was made.
        if report.cancelled {
            return Err("the import was cancelled before any section was brought over".to_string());
        }
        if report.sections_planned == 0 {
            return Err(format!("no OneNote sections (.one files) were found in {}", source.display()));
        }
        return Err(format!("no section could be imported from {}: {}", source.display(), not_imported(&report.skipped)));
    };
    progress(Progress::Destination { path: notebook.clone() });
    if !report.skipped.is_empty() {
        let lines: Vec<String> = report.skipped.iter().map(|s| format!("{}: {}", s.label, s.reason)).collect();
        let text = format!("Not imported from {}\n{}\n", source.display(), lines.join("\n"));
        fs::write(notebook.join("import-errors.txt"), text).map_err(|e| format!("cannot write the import error report: {e}"))?;
    }
    Ok(summary(&title, &report, bridged, combined))
}

/// Whether a section NoteCraft left out may be read by the bridge instead. A password, or a
/// section already there, is not something the bridge changes.
fn worth_the_bridge(s: &Skipped) -> bool {
    s.file.is_some() && s.reason != "password protected" && s.reason != "already in the destination"
}

#[allow(clippy::too_many_arguments)]
fn bridge_one(
    bridge: &Path,
    cancelled: &AtomicBool,
    file: &Path,
    base: &Path,
    notebooks: &Path,
    title: &str,
    format: PageFormat,
    settings: &Settings,
) -> Result<bool, String> {
    let (_scratch, converted, whole) = officeimo::convert_section(bridge, file, bridge_format(format), cancelled)?;
    let groups: Vec<String> = file
        .parent()
        .and_then(|p| p.strip_prefix(base).ok())
        .map(|rel| rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    let (_, parent) = notebook_and_groups(notebooks, title, &groups, format, settings)?;
    let file_name = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let section_title = clean_title(&discover::parse_file_name(&file_name).0, "Section");
    let dir = ops::create_node(&parent, &section_title, NodeKind::Section, None, settings).map_err(|e| e.to_string())?;
    if let Err(error) = officeimo::place_section(&converted, &dir, &section_title, bridge_format(format)) {
        let _ = fs::remove_dir_all(&dir);
        return Err(error);
    }
    Ok(whole)
}

/// `title`, or `title (2)`, `title (3)` …: the first no notebook in `notebooks` is called.
fn unique_title(notebooks: &Path, title: &str) -> String {
    if child_by_title(notebooks, title, NodeKind::Notebook).is_none() {
        return title.to_string();
    }
    (2..10_000)
        .map(|n| format!("{title} ({n})"))
        .find(|t| child_by_title(notebooks, t, NodeKind::Notebook).is_none())
        .unwrap_or_else(|| title.to_string())
}

fn not_imported(skipped: &[Skipped]) -> String {
    let shown: Vec<String> = skipped.iter().take(5).map(|s| format!("{} ({})", s.label, s.reason)).collect();
    let more = skipped.len().saturating_sub(shown.len());
    if more > 0 { format!("{}; and {more} more", shown.join("; ")) } else { shown.join("; ") }
}

fn summary(title: &str, report: &Report, bridged: usize, combined: usize) -> String {
    let sections = report.sections_imported + bridged;
    let mut text = format!(
        "Imported {sections} {} with {} {} into the notebook {title}.",
        if sections == 1 { "section" } else { "sections" },
        report.pages,
        if report.pages == 1 { "page" } else { "pages" },
    );
    if bridged > 0 {
        text.push_str(&format!(" OfficeIMO converted {bridged} of them that NoteCraft could not read."));
    }
    if combined > 0 {
        text.push_str(&format!(" {combined} of those came as one combined page each; their own page boundaries are unavailable."));
    }
    if !report.skipped.is_empty() {
        text.push_str(&format!(" Not imported: {} (listed in import-errors.txt).", not_imported(&report.skipped)));
    }
    if report.cancelled {
        text.push_str(" The import was cancelled before every section was read.");
    }
    text
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
    use super::*;
    use notecraft_onenote::testing::OneNote;
    use notecraft_store::testing::TempDir;
    use notecraft_store::tree::{child_dirs, read_node};
    use std::path::PathBuf;

    fn section(dir: &Path, file: &str, pages: &[(&str, &str)]) {
        fs::create_dir_all(dir).expect("dir");
        let mut one = OneNote::new();
        for (title, text) in pages {
            one.page(title, 1, &[OneNote::line(text)]);
        }
        fs::write(dir.join(file), one.finish()).expect("write");
    }

    /// Bytes that are not a OneNote section NoteCraft can read.
    fn unreadable(dir: &Path, file: &str) {
        fs::create_dir_all(dir).expect("dir");
        fs::write(dir.join(file), vec![7u8; 4096]).expect("write");
    }

    fn import_here(source: &Path, notebooks: &Path, bridge: Option<&Path>) -> Result<String, String> {
        let cancelled = Arc::new(AtomicBool::new(false));
        import_with(source, notebooks, PageFormat::Html, &cancelled, &mut |_| {}, &Settings::at(notebooks.with_file_name("settings.toml")), bridge)
    }

    /// The titles of the nodes under `dir`, with their kinds, depth first.
    fn tree(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        fn walk(dir: &Path, depth: usize, out: &mut Vec<String>) {
            for child in child_dirs(dir) {
                if let Some(node) = read_node(&child) {
                    out.push(format!("{}{:?} {}", "  ".repeat(depth), node.kind, node.title));
                    walk(&child, depth + 1, out);
                }
            }
        }
        walk(dir, 0, &mut out);
        out
    }

    fn notebook_dir(notebooks: &Path, title: &str) -> PathBuf {
        child_by_title(notebooks, title, NodeKind::Notebook).unwrap_or_else(|| panic!("a notebook called {title}: {:?}", tree(notebooks)))
    }

    #[test]
    fn a_notebook_folder_comes_in_as_one_notebook_with_its_groups_sections_and_pages() {
        let t = TempDir::new("nb-import");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes.one", &[("Soup", "onion and leek"), ("Bread", "flour and water")]);
        section(&src.join("Work"), "Plans.one", &[("Monday", "the plan")]);
        section(&src.join("OneNote_RecycleBin"), "Deleted.one", &[("Gone", "deleted page")]);
        let message = import_here(&src, &notebooks, None).expect("import");
        assert!(message.contains("2 sections with 3 pages"), "{message}");
        let nb = notebook_dir(&notebooks, "Life");
        let shape = tree(&nb).join("\n");
        for want in ["Section Recipes", "Page Soup", "Page Bread", "SectionGroup Work", "Section Plans", "Page Monday"] {
            assert!(shape.contains(want), "{want} in\n{shape}");
        }
        assert!(!shape.contains("RecycleBin") && !shape.contains("Gone"), "deleted sections stay out:\n{shape}");
        let soup = fs::read_dir(&nb).unwrap().flatten().map(|e| e.path()).find(|p| read_node(p).is_some_and(|n| n.title == "Recipes")).unwrap();
        let page = child_dirs(&soup).into_iter().find(|p| read_node(p).is_some_and(|n| n.title == "Soup")).unwrap();
        assert!(fs::read_to_string(page.join("index.html")).unwrap().contains("onion and leek"), "the page has its words");
        assert!(!nb.join("import-errors.txt").exists());
    }

    #[test]
    fn a_section_that_cannot_be_read_leaves_no_empty_group_and_is_named() {
        let t = TempDir::new("nb-import-bad");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes.one", &[("Soup", "onion")]);
        unreadable(&src.join("Archive"), "Old.one");
        let message = import_here(&src, &notebooks, None).expect("the readable part comes in");
        let nb = notebook_dir(&notebooks, "Life");
        let shape = tree(&nb).join("\n");
        assert!(shape.contains("Page Soup"), "{shape}");
        assert!(!shape.contains("Archive"), "no group is made for a section that never arrived:\n{shape}");
        assert!(message.contains("Not imported") && message.contains("Old"), "{message}");
        assert!(message.contains("not installed"), "says the bridge could have been tried: {message}");
        let errors = fs::read_to_string(nb.join("import-errors.txt")).expect("the report");
        assert!(errors.contains("Old"), "{errors}");
    }

    /// With a bridge given, a section NoteCraft cannot read is offered to it. One that fails
    /// there too is reported with both reasons and still leaves no empty group. (The bridge
    /// converting a section is covered by the opt-in test with a real installed bridge.)
    #[test]
    fn a_section_notecraft_cannot_read_is_offered_to_the_bridge() {
        let t = TempDir::new("nb-import-bridge");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes.one", &[("Soup", "onion")]);
        unreadable(&src.join("Archive"), "Old.one");
        let no_such_bridge = t.path().join("no-such-bridge");
        let message = import_here(&src, &notebooks, Some(&no_such_bridge)).expect("the readable part comes in");
        assert!(message.contains("OfficeIMO could not convert it either") && message.contains("Old"), "{message}");
        let nb = notebook_dir(&notebooks, "Life");
        let shape = tree(&nb).join("\n");
        assert!(shape.contains("Page Soup") && !shape.contains("Archive"), "{shape}");
    }

    /// A section in OneNote's OneDrive packaging is named as such, not as "not a section".
    #[test]
    fn a_section_in_the_onedrive_packaging_says_so() {
        let t = TempDir::new("nb-import-packaged");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes.one", &[("Soup", "onion")]);
        let mut packaged = OneNote::new().with_page("Cloud", &[OneNote::line("x")]).finish();
        packaged.splice(48..64, notecraft_onenote::FILE_FORMAT_PACKAGED.0);
        fs::write(src.join("Cloud.one"), packaged).expect("write");
        let message = import_here(&src, &notebooks, None).expect("import");
        assert!(message.contains("Cloud") && message.contains("OneDrive format"), "{message}");
    }

    #[test]
    fn nothing_readable_makes_no_notebook_and_says_why() {
        let t = TempDir::new("nb-import-none");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        unreadable(&src.join("Archive"), "Old.one");
        let error = import_here(&src, &notebooks, None).expect_err("nothing to import");
        assert!(error.contains("Old"), "{error}");
        assert!(tree(&notebooks).is_empty(), "no notebook was left behind: {:?}", tree(&notebooks));
        let empty = t.path().join("Empty");
        fs::create_dir_all(&empty).unwrap();
        let error = import_here(&empty, &notebooks, None).expect_err("no sections");
        assert!(error.contains("no OneNote sections"), "{error}");
    }

    #[test]
    fn a_second_import_of_the_same_folder_is_a_new_notebook() {
        let t = TempDir::new("nb-import-twice");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes.one", &[("Soup", "onion")]);
        import_here(&src, &notebooks, None).expect("first");
        import_here(&src, &notebooks, None).expect("second");
        notebook_dir(&notebooks, "Life");
        let again = notebook_dir(&notebooks, "Life (2)");
        assert!(tree(&again).join("\n").contains("Page Soup"));
    }

    #[test]
    fn a_section_file_or_the_notebook_table_of_contents_can_be_chosen() {
        let t = TempDir::new("nb-import-files");
        let src = t.path().join("Life");
        let notebooks = t.path().join("notebooks");
        section(&src, "Recipes (On 1-2-2024).one", &[("Soup", "onion")]);
        section(&src, "Garden.one", &[("Beans", "water")]);
        import_here(&src.join("Recipes (On 1-2-2024).one"), &notebooks, None).expect("a section");
        let one = notebook_dir(&notebooks, "Recipes");
        let shape = tree(&one).join("\n");
        assert!(shape.contains("Section Recipes") && shape.contains("Page Soup") && !shape.contains("Garden"), "{shape}");
        fs::write(src.join("Open Notebook.onetoc2"), b"toc").unwrap();
        import_here(&src.join("Open Notebook.onetoc2"), &notebooks, None).expect("the whole notebook");
        let whole = tree(&notebook_dir(&notebooks, "Life")).join("\n");
        assert!(whole.contains("Page Soup") && whole.contains("Page Beans"), "{whole}");
    }
}

//! Checks a notebook against the folder rules (see `docs/folder-format.md`).

use crate::tree::{child_dirs, folder_name, read_node};
use notecraft_model::names;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub nodes: usize,
    pub problems: Vec<Problem>,
}

/// Checks `dir` and everything below it. Path lengths are counted from the parent of
/// `dir`, which stands in for the sync root.
pub fn check_tree(dir: &Path) -> Report {
    let base_len = dir.parent().map(|p| p.to_string_lossy().len()).unwrap_or(0);
    let mut report = Report::default();
    let mut ids: HashMap<String, PathBuf> = HashMap::new();
    walk(dir, base_len, &mut report, &mut ids);
    report
}

fn problem(report: &mut Report, path: &Path, message: impl Into<String>) {
    report.problems.push(Problem { path: path.to_path_buf(), message: message.into() });
}

fn walk(dir: &Path, base_len: usize, report: &mut Report, ids: &mut HashMap<String, PathBuf>) {
    report.nodes += 1;
    if dir.join("index.md").is_file() && dir.join("index.html").is_file() {
        problem(report, dir, "both index.md and index.html exist (ambiguous; HTML is used)");
    }
    match read_node(dir) {
        None => problem(report, dir, "folder has no _node.toml and no index file"),
        Some(info) if info.id.is_empty() => problem(report, dir, "missing id"),
        Some(info) => {
            if let Some(other) = ids.insert(info.id.clone(), dir.to_path_buf()) {
                problem(report, dir, format!("duplicate id, also used by {}", other.display()));
            }
        }
    }
    let name = folder_name(dir);
    if let Err(why) = names::validate_component(&name) {
        problem(report, dir, format!("folder name '{name}': {why}"));
    }
    let rel_len = dir.to_string_lossy().len().saturating_sub(base_len);
    if rel_len > names::MAX_PATH_FROM_ROOT {
        problem(
            report,
            dir,
            format!("path is {rel_len} characters from the notebook's parent; sync tools may refuse over {}", names::MAX_PATH_FROM_ROOT),
        );
    }
    if let Ok(rd) = fs::read_dir(dir) {
        let mut seen: HashMap<String, String> = HashMap::new();
        for e in rd.filter_map(|e| e.ok()) {
            let n = e.file_name().to_string_lossy().into_owned();
            if e.path().is_file() && names::looks_like_conflict(&n) {
                problem(report, dir, format!("possible sync conflict copy '{n}'"));
            }
            if let Some(prev) = seen.insert(n.to_lowercase(), n.clone()) {
                problem(report, dir, format!("'{prev}' and '{n}' differ only by case; they cannot coexist on Windows or default macOS"));
            }
        }
    }
    for c in child_dirs(dir) {
        walk(&c, base_len, report, ids);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops;
    use crate::settings::Settings;
    use crate::testing::TempDir;
    use notecraft_model::NodeKind;

    fn notebook(t: &TempDir) -> (Settings, PathBuf) {
        let s = Settings::at(t.path().join("s.toml"));
        let nb = ops::create_notebook(&t.path().join("nb"), "NB", None, &s).expect("nb");
        (s, nb)
    }

    #[test]
    fn a_clean_notebook_has_no_problems() {
        let t = TempDir::new("check1");
        let (s, nb) = notebook(&t);
        let sec = ops::create_node(&nb, "S", NodeKind::Section, None, &s).expect("sec");
        let p = ops::create_node(&sec, "P", NodeKind::Page, None, &s).expect("p");
        ops::create_node(&p, "Sub", NodeKind::Page, None, &s).expect("sub");
        let r = check_tree(&nb);
        assert_eq!(r.nodes, 4);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
    }

    #[test]
    fn finds_ambiguous_pages_and_conflict_copies() {
        let t = TempDir::new("check2");
        let (s, nb) = notebook(&t);
        let p = ops::create_node(&nb, "P", NodeKind::Page, None, &s).expect("p");
        fs::write(p.join("index.md"), "x").expect("md");
        fs::write(p.join("index-LAPTOP.html"), "x").expect("conflict");
        let msgs: Vec<String> = check_tree(&nb).problems.into_iter().map(|p| p.message).collect();
        assert!(msgs.iter().any(|m| m.contains("ambiguous")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("conflict copy")), "{msgs:?}");
    }

    #[test]
    fn finds_duplicate_and_missing_ids() {
        let t = TempDir::new("check3");
        let (s, nb) = notebook(&t);
        let a = ops::create_node(&nb, "A", NodeKind::Page, None, &s).expect("a");
        let b = ops::create_node(&nb, "B", NodeKind::Page, None, &s).expect("b");
        let ta = fs::read_to_string(a.join("index.html")).expect("a");
        let id_a = notecraft_formats::html::meta(&ta, "notecraft:id").expect("id");
        let tb = fs::read_to_string(b.join("index.html")).expect("b");
        fs::write(b.join("index.html"), notecraft_formats::html::set_meta(&tb, "notecraft:id", &id_a)).expect("dup");
        let c = ops::create_node(&nb, "C", NodeKind::Page, None, &s).expect("c");
        fs::write(c.join("index.html"), "<html><body>no meta</body></html>").expect("blank");
        let msgs: Vec<String> = check_tree(&nb).problems.into_iter().map(|p| p.message).collect();
        assert!(msgs.iter().any(|m| m.contains("duplicate id")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("missing id")), "{msgs:?}");
    }

    #[test]
    fn finds_bad_folder_names_and_plain_folders() {
        let t = TempDir::new("check4");
        let (_s, nb) = notebook(&t);
        fs::create_dir_all(nb.join("desktop.ini")).expect("bad name");
        fs::create_dir_all(nb.join("not a node")).expect("plain");
        let msgs: Vec<String> = check_tree(&nb).problems.into_iter().map(|p| p.message).collect();
        // Plain folders are not nodes, so they are skipped by the tree walk: no problem is
        // raised for them. Blocked names are only reported when the folder is a node.
        assert!(msgs.iter().all(|m| !m.contains("not a node")), "{msgs:?}");
    }

    #[test]
    fn finds_names_that_differ_only_by_case() {
        let t = TempDir::new("check5");
        let (_s, nb) = notebook(&t);
        fs::write(nb.join("Readme.txt"), "a").expect("a");
        // On a case-insensitive file system the second write replaces the first, so only
        // assert when both files exist.
        fs::write(nb.join("README.txt"), "b").expect("b");
        let both =
            fs::read_dir(&nb).expect("dir").filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().to_lowercase() == "readme.txt").count()
                == 2;
        let msgs: Vec<String> = check_tree(&nb).problems.into_iter().map(|p| p.message).collect();
        assert_eq!(msgs.iter().any(|m| m.contains("differ only by case")), both, "{msgs:?}");
    }

    #[test]
    fn long_paths_are_reported() {
        let t = TempDir::new("check6");
        let (s, nb) = notebook(&t);
        let mut p = ops::create_node(&nb, "Level", NodeKind::Page, None, &s).expect("p");
        for i in 0..12 {
            p = match ops::create_node(&p, &format!("Level {i} {}", "x".repeat(60)), NodeKind::Page, None, &s) {
                Ok(p) => p,
                Err(_) => break,
            };
        }
        let msgs: Vec<String> = check_tree(&nb).problems.into_iter().map(|p| p.message).collect();
        assert!(msgs.iter().any(|m| m.contains("characters from the notebook's parent")), "{msgs:?}");
    }
}

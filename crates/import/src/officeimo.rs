//! Optional OfficeIMO bridge discovery and invocation.
//!
//! The bridge is deliberately an external process. This keeps the NoteCraft core Rust-only,
//! lets releases carry a platform-specific OfficeIMO build, and gives us a stable replacement
//! point if the importer implementation changes later.

use std::ffi::OsString;
use std::fs;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

/// The page serialization requested from the bridge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Html,
    Cmgfm,
}

/// Progress reported while a folder import is being materialized.
#[derive(Clone, Debug)]
pub enum Progress {
    Found { total: usize },
    Section { index: usize, total: usize, label: String },
}

impl Format {
    /// Parses the public page format names used by the CLI and bridge contract.
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "html" => Ok(Self::Html),
            "cmgfm" => Ok(Self::Cmgfm),
            other => Err(format!("unknown OfficeIMO format '{other}'; use html or cmgfm")),
        }
    }
}

impl Format {
    fn argument(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Cmgfm => "cmgfm",
        }
    }
}

/// Locates an explicitly installed or bundled OfficeIMO bridge.
pub fn locate() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("NOTECRAFT_OFFICEIMO_BRIDGE") {
        let path = PathBuf::from(path);
        return executable_or_error(path, "NOTECRAFT_OFFICEIMO_BRIDGE");
    }

    let relative = Path::new("importers").join("officeimo").join(target_name()).join(executable_name());
    let mut candidates = Vec::new();
    if let Some(root) = std::env::var_os("NOTECRAFT_DATA_DIR").map(PathBuf::from).or_else(data_dir) {
        candidates.push(root.join(&relative));
    }
    // Windows MSI installs the bridge beside the application under Program Files, while
    // portable/dev installs may use the data directory above. Check both layouts.
    if let Ok(executable) = std::env::current_exe()
        && let Some(app_dir) = executable.parent()
    {
        candidates.push(app_dir.join(&relative));
    }
    if let Some(path) = candidates.iter().find(|path| path.is_file()) {
        return Ok(path.clone());
    }
    let searched = candidates.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join("; ");
    Err(format!("the installed OfficeIMO bridge was not found; searched: {searched}"))
}

/// Invokes the bridge using the stable command-line contract.
pub fn run(source: &Path, output: &Path, format: Format) -> Result<String, String> {
    let cancelled = AtomicBool::new(false);
    run_with_progress(source, output, format, &cancelled, &mut |_| {})
}

/// Runs the bridge while reporting folder progress and allowing the caller to cancel.
pub fn run_with_progress(
    source: &Path,
    output: &Path,
    format: Format,
    cancelled: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Result<String, String> {
    if !source.exists() {
        return Err(format!("the OfficeIMO source {} does not exist", source.display()));
    }
    if source.is_dir() {
        return run_folder(source, output, format, cancelled, progress);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }
    run_one(source, output, format, cancelled)
}

fn run_one(source: &Path, output: &Path, format: Format, cancelled: &AtomicBool) -> Result<String, String> {
    let bridge = locate()?;
    let mut command = Command::new(&bridge);
    command
        .arg("--source")
        .arg(source)
        .arg("--out")
        .arg(output)
        .arg("--format")
        .arg(format.argument())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    suppress_console(&mut command);
    let mut child = command.spawn().map_err(|e| format!("cannot start OfficeIMO bridge {}: {e}", bridge.display()))?;
    loop {
        if cancelled.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("OfficeIMO import cancelled".to_string());
        }
        if child.try_wait().map_err(|e| format!("cannot wait for OfficeIMO bridge: {e}"))?.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(40));
    }
    let result = child.wait_with_output().map_err(|e| format!("cannot collect OfficeIMO bridge output: {e}"))?;
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("OfficeIMO bridge failed with {}: {}", result.status, stderr.trim()));
    }
    Ok(stdout)
}

fn run_folder(
    source: &Path,
    notebooks: &Path,
    format: Format,
    cancelled: &AtomicBool,
    progress: &mut impl FnMut(Progress),
) -> Result<String, String> {
    let notebook_name = safe_name(source.file_name().and_then(|v| v.to_str()).unwrap_or("Imported notebook"));
    let (notebook_name, notebook) = unique_notebook_path(notebooks, &notebook_name);
    fs::create_dir_all(&notebook).map_err(|e| format!("cannot create {}: {e}", notebook.display()))?;
    write_node(&notebook, "notebook", &notebook_name, format)?;

    let mut files = Vec::new();
    collect_sections(source, source, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no .one section files were found under {}", source.display()));
    }
    progress(Progress::Found { total: files.len() });

    let staging = notebook.join(format!(".officeimo-staging-{}", std::process::id()));
    let result = (|| {
        let mut failures = Vec::new();
        for (index, file) in files.iter().enumerate() {
            if cancelled.load(Ordering::Relaxed) {
                return Err("OfficeIMO import cancelled".to_string());
            }
            let label = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "section".to_string());
            progress(Progress::Section { index: index + 1, total: files.len(), label });
            let relative_parent = file.parent().and_then(|p| p.strip_prefix(source).ok()).unwrap_or(Path::new(""));
            let mut destination = notebook.clone();
            for component in relative_parent.components() {
                let group_name = safe_name(&component.as_os_str().to_string_lossy());
                destination.push(&group_name);
                if !destination.join("_node.toml").is_file() {
                    fs::create_dir_all(&destination).map_err(|e| format!("cannot create {}: {e}", destination.display()))?;
                    write_node(&destination, "group", &group_name, format)?;
                }
            }
            let section_name = section_name(file);
            destination.push(section_name.clone());
            let stage = staging.join(format!("{index}"));
            fs::create_dir_all(&stage).map_err(|e| format!("cannot create {}: {e}", stage.display()))?;
            if let Err(error) = run_one(file, &stage, format, cancelled) {
                failures.push(format!("{}: {}", file.display(), error));
                continue;
            }
            let generated = stage.join("Imported section");
            if !generated.is_dir() {
                failures.push(format!("{}: OfficeIMO produced no section", file.display()));
                continue;
            }
            copy_tree(&generated, &destination)?;
            write_node(&destination, "section", &section_name, format)?;
        }
        normalize_ids(&notebook)?;
        if !failures.is_empty() {
            let report = format!(
                "OfficeIMO imported {} of {} sections into {}\n{}",
                files.len() - failures.len(),
                files.len(),
                notebook.display(),
                failures.join("\n")
            );
            fs::write(notebook.join("import-errors.txt"), &report).map_err(|e| format!("cannot write import error report: {e}"))?;
            if failures.len() == files.len() {
                return Err(report);
            }
            Ok(report)
        } else {
            Ok(format!("OfficeIMO imported {} sections into {}", files.len(), notebook.display()))
        }
    })();
    let _ = fs::remove_dir_all(&staging);
    result
}

#[cfg(windows)]
fn suppress_console(command: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn suppress_console(_command: &mut Command) {}

fn unique_notebook_path(parent: &Path, name: &str) -> (String, PathBuf) {
    let candidate = parent.join(name);
    if !candidate.exists() {
        return (name.to_string(), candidate);
    }
    for number in 1..=u32::MAX {
        let candidate_name = format!("{name} ({number})");
        let candidate = parent.join(&candidate_name);
        if !candidate.exists() {
            return (candidate_name, candidate);
        }
    }
    let fallback = format!("{name} ({})", u64::from(u32::MAX) + 1);
    (fallback.clone(), parent.join(fallback))
}

fn collect_sections(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read an entry under {}: {e}", dir.display()))?;
        let path = entry.path();
        let kind = entry.file_type().map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if entry.file_name().to_string_lossy().eq_ignore_ascii_case("OneNote_RecycleBin") {
                continue;
            }
            collect_sections(root, &path, files)?;
        } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("one")) && path.strip_prefix(root).is_ok() {
            files.push(path);
        }
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|e| format!("cannot create {}: {e}", destination.display()))?;
    for entry in fs::read_dir(source).map_err(|e| format!("cannot read {}: {e}", source.display()))? {
        let entry = entry.map_err(|e| format!("cannot read an entry under {}: {e}", source.display()))?;
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type().map_err(|e| format!("cannot inspect {}: {e}", from.display()))?.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| format!("cannot copy {} to {}: {e}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

fn write_node(path: &Path, kind: &str, title: &str, format: Format) -> Result<(), String> {
    let title = title.replace(['\\', '"'], "_");
    let content = if kind == "notebook" {
        format!("kind = \"notebook\"\ntitle = \"{title}\"\nid = \"00000000000000000001\"\nformat = \"{}\"\n", format.argument())
    } else {
        format!("kind = \"{kind}\"\ntitle = \"{title}\"\nid = \"00000000000000000002\"\n")
    };
    fs::write(path.join("_node.toml"), content).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

fn normalize_ids(root: &Path) -> Result<(), String> {
    let mut nodes = Vec::new();
    collect_nodes(root, &mut nodes)?;
    nodes.sort();
    for (index, node) in nodes.iter().enumerate() {
        let text = fs::read_to_string(node).map_err(|e| format!("cannot read {}: {e}", node.display()))?;
        let updated = text
            .lines()
            .map(|line| if line.starts_with("id = ") { format!("id = \"{:020}\"", index + 1) } else { line.to_string() })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(node, updated).map_err(|e| format!("cannot rewrite {}: {e}", node.display()))?;
    }
    let mut pages = Vec::new();
    collect_pages(root, &mut pages)?;
    pages.sort();
    for (index, page) in pages.iter().enumerate() {
        let text = fs::read_to_string(page).map_err(|e| format!("cannot read {}: {e}", page.display()))?;
        let id = format!("{:020}", nodes.len() + index + 1);
        let updated = text
            .lines()
            .map(|line| {
                if line.trim_start().starts_with("id: ") {
                    format!("id: {id}")
                } else if line.contains("name=\"notecraft:id\"") {
                    format!("<meta name=\"notecraft:id\" content=\"{id}\">")
                } else {
                    line.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(page, updated).map_err(|e| format!("cannot rewrite {}: {e}", page.display()))?;
    }
    Ok(())
}

fn collect_nodes(dir: &Path, nodes: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("cannot read an entry under {}: {e}", dir.display()))?;
        let path = entry.path();
        if entry.file_type().map_err(|e| format!("cannot inspect {}: {e}", path.display()))?.is_dir() {
            collect_nodes(&path, nodes)?;
        } else if path.file_name().is_some_and(|name| name == "_node.toml") {
            nodes.push(path);
        }
    }
    Ok(())
}

fn collect_pages(dir: &Path, pages: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("cannot read an entry under {}: {e}", dir.display()))?;
        let path = entry.path();
        if entry.file_type().map_err(|e| format!("cannot inspect {}: {e}", path.display()))?.is_dir() {
            collect_pages(&path, pages)?;
        } else if path.file_name().is_some_and(|name| name == "index.html" || name == "index.md") {
            pages.push(path);
        }
    }
    Ok(())
}

fn section_name(path: &Path) -> String {
    let mut name = path.file_stem().and_then(|v| v.to_str()).unwrap_or("Untitled section").to_string();
    if let Some((prefix, _)) = name.rsplit_once(" (On ") {
        name = prefix.to_string();
    }
    safe_name(&name)
}

fn safe_name(value: &str) -> String {
    let name: String = value.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let name = name.trim().trim_end_matches('.');
    if name.is_empty() { "Imported notebook".to_string() } else { name.to_string() }
}

fn executable_or_error(path: PathBuf, source: &str) -> Result<PathBuf, String> {
    if path.is_file() { Ok(path) } else { Err(format!("{source} was not found at {}", path.display())) }
}

fn data_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).map(|p| p.join("NoteCraft"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local").join("share")))
            .map(|p| p.join("NoteCraft"))
    }
}

fn target_name() -> &'static str {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        "win-x64"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "linux-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "osx-x64"
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "osx-arm64"
    } else {
        "unsupported"
    }
}

fn executable_name() -> OsString {
    if cfg!(windows) { OsString::from("officeimo-notecraft.exe") } else { OsString::from("officeimo-notecraft") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_and_backup_dates_become_safe_container_names() {
        assert_eq!(safe_name("2026"), "2026");
        assert_eq!(section_name(Path::new("Work (On 9-29-2026).one")), "Work");
        assert_eq!(safe_name("A:B"), "A_B");
    }

    #[test]
    fn section_discovery_is_recursive_and_skips_recycle_bin() {
        let root = std::env::temp_dir().join(format!("notecraft-officeimo-{}", std::process::id()));
        let nested = root.join("2026").join("MBA");
        let recycle = root.join("2026").join("OneNote_RecycleBin");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&nested).expect("test directory");
        fs::create_dir_all(&recycle).expect("test directory");
        fs::write(nested.join("Work.one"), []).expect("test file");
        fs::write(recycle.join("Deleted.one"), []).expect("test file");
        let mut files = Vec::new();
        collect_sections(&root, &root, &mut files).expect("scan");
        assert_eq!(files, vec![nested.join("Work.one")]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repeated_folder_imports_get_windows_style_names() {
        let root = std::env::temp_dir().join(format!("notecraft-officeimo-names-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("2026")).expect("test directory");
        fs::create_dir_all(root.join("2026 (1)")).expect("test directory");
        assert_eq!(unique_notebook_path(&root, "2026"), ("2026 (2)".to_string(), root.join("2026 (2)")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn target_names_are_release_rids() {
        assert!(matches!(target_name(), "win-x64" | "linux-x64" | "osx-x64" | "osx-arm64" | "unsupported"));
    }
}

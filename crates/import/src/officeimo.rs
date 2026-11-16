//! Optional OfficeIMO bridge discovery and invocation.
//!
//! The bridge is deliberately an external process. This keeps the NoteCraft core Rust-only,
//! lets releases carry a platform-specific OfficeIMO build, and gives us a stable replacement
//! point if the importer implementation changes later.

use notecraft_formats::{frontmatter, html, templates};
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
    Found {
        total: usize,
    },
    /// The actual notebook folder chosen by the importer, including collision suffixes.
    Destination {
        path: PathBuf,
    },
    Section {
        index: usize,
        total: usize,
        label: String,
    },
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
    // A OneNote package (issue #17): unpacked into a folder named after it, which is then
    // imported as a notebook folder. The unpacked copy is removed when the import ends.
    if crate::package::is_package(source) {
        let scratch = crate::package::Scratch::new()?;
        let name = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let folder = scratch.path.join(crate::package::safe_name(&safe_name(&name)).unwrap_or_else(|| PathBuf::from("OneNote package")));
        crate::package::unpack(source, &folder)?;
        return run_folder(&folder, output, format, cancelled, progress);
    }
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }
    run_section(source, output, format, cancelled, progress)
}

/// A single section still needs a notebook boundary. Run the bridge in owned scratch space
/// so a flat export cannot turn the destination itself into a page or overwrite other notes.
fn run_section(source: &Path, output: &Path, format: Format, cancelled: &AtomicBool, progress: &mut impl FnMut(Progress)) -> Result<String, String> {
    let scratch = crate::package::Scratch::new()?;
    let title = section_name(source);
    progress(Progress::Found { total: 1 });
    progress(Progress::Section { index: 1, total: 1, label: title.clone() });
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }
    run_one(source, &scratch.path, format, cancelled)?;
    let (section, combined) = bridge_section(&scratch.path, format, &title)?;
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }
    let (notebook_title, notebook) = unique_notebook_path(output, &title);
    fs::create_dir_all(&notebook).map_err(|e| format!("cannot create imported notebook: {e}"))?;
    write_node(&notebook, "notebook", &notebook_title, format)?;
    let destination = notebook.join("imported-section");
    copy_tree(&section, &destination)?;
    write_node(&destination, "section", &title, format)?;
    normalize_ids(&notebook)?;
    progress(Progress::Destination { path: notebook.clone() });
    let summary = format!("Imported {} into {}", source.display(), notebook.display());
    // The bridge's own message names scratch space, not the delivered notebook.
    Ok(if combined {
        format!("{summary}\nThis bridge exported the section as one combined page. Original page boundaries are unavailable.")
    } else {
        summary
    })
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
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }
    let mut files = Vec::new();
    collect_sections(source, source, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(format!("no .one section files were found under {}", source.display()));
    }
    progress(Progress::Found { total: files.len() });
    if cancelled.load(Ordering::Relaxed) {
        return Err("OfficeIMO import cancelled".to_string());
    }

    let notebook_name = safe_name(source.file_name().and_then(|v| v.to_str()).unwrap_or("Imported notebook"));
    let (notebook_name, notebook) = unique_notebook_path(notebooks, &notebook_name);
    fs::create_dir_all(&notebook).map_err(|e| format!("cannot create {}: {e}", notebook.display()))?;
    write_node(&notebook, "notebook", &notebook_name, format)?;
    progress(Progress::Destination { path: notebook.clone() });

    let staging = notebook.join(format!(".officeimo-staging-{}", std::process::id()));
    let mut combined_sections = 0;
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
            let generated = match bridge_section(&stage, format, &section_name) {
                Ok((generated, combined)) => {
                    combined_sections += usize::from(combined);
                    generated
                }
                Err(error) => {
                    failures.push(format!("{}: {error}", file.display()));
                    continue;
                }
            };
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
    result.map(|message| {
        if combined_sections == 0 {
            message
        } else {
            format!(
                "{message}\nThis bridge exported {combined_sections} section(s) as one combined page each. Original page boundaries are unavailable."
            )
        }
    })
}

/// Older bundled bridges export a section as one document instead of a native tree. Keep
/// that document and its relative assets together, and disclose the combined-page fallback.
fn bridge_section(stage: &Path, format: Format, title: &str) -> Result<(PathBuf, bool), String> {
    let section = stage.join("Imported section");
    if section.is_dir() {
        return Ok((section, false));
    }
    let file_name = match format {
        Format::Html => "index.html",
        Format::Cmgfm => "index.md",
    };
    if !stage.join(file_name).is_file() {
        return Err("OfficeIMO produced no section or page document".to_string());
    }
    // Enumerate before creating the destination; never recursively copy a directory into itself.
    let entries = fs::read_dir(stage)
        .map_err(|e| format!("cannot read bridge output: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("cannot read bridge output entry: {e}"))?;
    let page = section.join("imported-page");
    fs::create_dir_all(&page).map_err(|e| format!("cannot create imported page: {e}"))?;
    for entry in entries {
        fs::rename(entry.path(), page.join(entry.file_name())).map_err(|e| format!("cannot arrange bridge output: {e}"))?;
    }
    let file = page.join(file_name);
    let text = fs::read_to_string(&file).map_err(|e| format!("cannot read imported page: {e}"))?;
    let text = if format == Format::Html { unwrap_exporter_containers(&text) } else { text };
    let text = page_metadata(&text, format, "pending", Some(title));
    fs::write(&file, text).map_err(|e| format!("cannot write imported page metadata: {e}"))?;
    write_node(&section, "section", title, format)?;
    Ok((section, true))
}

/// The semantic bridge adds these transparent containers around supported page blocks.
/// Remove only exact known wrappers. Attributes or containers we do not understand keep
/// the original document intact, so the normal read-only protection still applies.
fn unwrap_exporter_containers(text: &str) -> String {
    let mut body = html::inner(text);
    for wrapper in [
        "<article class=\"markdown-body\">",
        "<article class=\"officeimo-onenote-semantic\">",
        "<section class=\"officeimo-onenote-page\">",
        "<section class=\"officeimo-onenote-outline\">",
    ] {
        body = body.replace(wrapper, "");
    }
    if body.contains("<article") || body.contains("<section") {
        return text.to_string();
    }
    body = body.replace("</article>", "").replace("</section>", "");
    html::replace_inner(text, &body)
}

/// Edit only the metadata boundary. A matching line in the body is page content, and HTML
/// can put its metadata and all its content on the same line.
fn page_metadata(text: &str, format: Format, id: &str, title: Option<&str>) -> String {
    match format {
        Format::Html => {
            let mut text = if text.contains("</head>") {
                text.to_string()
            } else {
                html::replace_inner(
                    &templates::html_page(title.or(Some("Imported page")), id, 0, &notecraft_model::time::iso_now()),
                    &html::inner(text),
                )
            };
            if let Some(title) = title {
                let escaped = html::escape(title);
                text = if html::title(&text).is_some() {
                    html::replace_between(&text, "<title>", "</title>", &escaped)
                } else {
                    text.replacen("</head>", &format!("<title>{escaped}</title>\n</head>"), 1)
                };
            }
            html::set_meta(&text, "notecraft:id", id)
        }
        Format::Cmgfm => {
            let (mut front, body) = frontmatter::split_front(text);
            if front.is_empty() {
                front =
                    frontmatter::split_front(&templates::markdown_page(title.or(Some("Imported page")), id, 0, &notecraft_model::time::iso_now())).0;
            } else if let Some(title) = title {
                front = frontmatter::set_value(&front, "title", &notecraft_model::kv::quote(title));
            }
            format!("{}{body}", frontmatter::set_value(&front, "id", id))
        }
    }
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
    for node in &nodes {
        let text = fs::read_to_string(node).map_err(|e| format!("cannot read {}: {e}", node.display()))?;
        let id = notecraft_model::id::new_id();
        let updated = text
            .lines()
            .map(|line| if line.starts_with("id = ") { format!("id = \"{id}\"") } else { line.to_string() })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(node, updated).map_err(|e| format!("cannot rewrite {}: {e}", node.display()))?;
    }
    let mut pages = Vec::new();
    collect_pages(root, &mut pages)?;
    pages.sort();
    for page in &pages {
        let text = fs::read_to_string(page).map_err(|e| format!("cannot read {}: {e}", page.display()))?;
        let id = notecraft_model::id::new_id();
        let format = if page.file_name().is_some_and(|name| name == "index.html") { Format::Html } else { Format::Cmgfm };
        let updated = page_metadata(&text, format, &id, None);
        fs::write(page, updated).map_err(|e| format!("cannot rewrite {}: {e}", page.display()))?;
    }
    Ok(())
}

fn collect_nodes(dir: &Path, nodes: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(dir).map_err(|e| format!("cannot read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("cannot read an entry under {}: {e}", dir.display()))?;
        let path = entry.path();
        if entry.file_type().map_err(|e| format!("cannot inspect {}: {e}", path.display()))?.is_dir() {
            let name = entry.file_name();
            if name == "assets" || name.to_string_lossy().starts_with('.') {
                continue;
            }
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
            let name = entry.file_name();
            if name == "assets" || name.to_string_lossy().starts_with('.') {
                continue;
            }
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
    fn a_flat_bridge_export_becomes_a_readable_section_with_its_assets() {
        for format in [Format::Html, Format::Cmgfm] {
            let t = notecraft_store::testing::TempDir::new("officeimo-flat-output");
            let file = if format == Format::Html { "index.html" } else { "index.md" };
            let text = if format == Format::Html {
                "<!DOCTYPE html><html><head><title>Document</title></head><body><article><h2>So good</h2><p>This is one note 2016</p><img src=\"assets/pixel.png\"></article></body></html>"
            } else {
                "# So good\n\nThis is one note 2016\n\n![pixel](assets/pixel.png)\n"
            };
            fs::write(t.path().join(file), text).expect("recorded flat bridge output");
            fs::create_dir(t.path().join("assets")).expect("assets");
            fs::write(t.path().join("assets/pixel.png"), include_bytes!("../tests/fixtures/synthetic-pixel.png")).expect("picture");
            let (section, combined) = bridge_section(t.path(), format, "Section").expect("flat output is supported");
            assert!(combined, "the caller can disclose combined-page output");
            normalize_ids(&section).expect("metadata");
            let tree = notecraft_store::tree::load_tree(&section).expect("section tree");
            assert_eq!(tree.info.kind, notecraft_model::NodeKind::Section);
            assert_eq!(tree.children.len(), 1, "the flat bridge's document is one explicit page");
            let page = &tree.children[0];
            assert_eq!(page.info.kind, notecraft_model::NodeKind::Page);
            assert!(!page.info.id.is_empty());
            let loaded = notecraft_store::page::load_page(&page.path).expect("read imported page");
            assert!(loaded.body.contains("This is one note 2016"));
            assert!(loaded.body.contains("assets/pixel.png"));
            assert_eq!(
                fs::read(page.path.join("assets/pixel.png")).expect("page-owned picture"),
                include_bytes!("../tests/fixtures/synthetic-pixel.png")
            );
        }
    }

    #[test]
    fn flat_html_unwraps_only_known_exporter_containers() {
        for known in [true, false] {
            let t = notecraft_store::testing::TempDir::new("officeimo-flat-wrappers");
            let wrapper = if known { "officeimo-onenote-semantic" } else { "user-container" };
            let body = format!(
                "<article class=\"markdown-body\"><article class=\"{wrapper}\"><h1>Section</h1><section class=\"officeimo-onenote-page\"><section class=\"officeimo-onenote-outline\"><p>Keep <b>every</b> word.</p></section></section></article></article>"
            );
            fs::write(t.path().join("index.html"), format!("<html><head><title>Document</title></head><body>{body}</body></html>"))
                .expect("bridge document");
            let (section, _) = bridge_section(t.path(), Format::Html, "Section").expect("section");
            let loaded = notecraft_store::page::load_page(&section.join("imported-page")).expect("imported page");
            assert!(loaded.body.contains("<p>Keep <b>every</b> word.</p>"));
            if known {
                assert_eq!(
                    html::unsupported_tag(&loaded.body),
                    None,
                    "exporter-only wrappers must not make ordinary text read-only or duplicate its title"
                );
            } else {
                assert_eq!(loaded.body, body, "unrecognized containers remain intact and read-only");
            }
        }
    }

    #[test]
    fn normalizing_import_ids_preserves_inline_html_and_markdown_body_text() {
        let t = notecraft_store::testing::TempDir::new("officeimo-id-content");
        let html = t.path().join("HTML");
        let md = t.path().join("Markdown");
        fs::create_dir(&html).expect("HTML page");
        fs::create_dir(&md).expect("Markdown page");
        let html_body = "<p>Keep every word and <b>mark</b>.</p>";
        let html_text =
            format!("<html><head><title>Inline</title><meta name=\"notecraft:id\" content=\"old\"></head><body>{html_body}</body></html>");
        let md_body = "\n```text\nid: this is page content\n```\n";
        fs::write(html.join("index.html"), &html_text).expect("HTML output");
        fs::write(md.join("index.md"), format!("---\nid: old\ntitle: Markdown\n---\n{md_body}")).expect("Markdown output");
        normalize_ids(t.path()).expect("normalize");
        assert_eq!(notecraft_store::page::load_page(&html).expect("HTML page").body, html_body);
        assert_eq!(notecraft_store::page::load_page(&md).expect("Markdown page").body, md_body.trim_start_matches('\n'));
        assert_eq!(
            frontmatter::split_front(&fs::read_to_string(md.join("index.md")).expect("saved Markdown")).1,
            md_body,
            "the stored body is byte-for-byte unchanged"
        );
        assert_ne!(notecraft_store::tree::read_node(&html).expect("HTML metadata").id, "old");
    }

    #[test]
    fn independent_imports_do_not_reuse_notebook_section_or_page_ids() {
        let t = notecraft_store::testing::TempDir::new("officeimo-independent-ids");
        let mut seen = std::collections::HashSet::new();
        for name in ["First", "Second"] {
            let notebook = t.path().join(name);
            let section = notebook.join("Section");
            let page = section.join("Page");
            fs::create_dir_all(&page).expect("notebook tree");
            write_node(&notebook, "notebook", name, Format::Html).expect("notebook metadata");
            write_node(&section, "section", "Section", Format::Html).expect("section metadata");
            fs::write(page.join("index.html"), "<html><head><title>Page</title></head><body><p>Content</p></body></html>").expect("page");
            normalize_ids(&notebook).expect("normalize import");
            for path in [&notebook, &section, &page] {
                let id = notecraft_store::tree::read_node(path).expect("node").id;
                assert!(!id.is_empty());
                assert!(seen.insert(id), "a second import must not reuse an existing node's identity");
            }
        }
    }

    #[test]
    fn normalizing_import_metadata_does_not_rewrite_owned_assets_or_staging_copies() {
        let t = notecraft_store::testing::TempDir::new("officeimo-owned-file-ids");
        let page = t.path().join("Page");
        fs::create_dir(&page).expect("page");
        fs::write(page.join("index.html"), "<html><head><title>Page</title></head><body><p>Words</p></body></html>").expect("page file");
        let contents = "---\nid: attachment-identity\n---\nOriginal attachment bytes\n";
        let node_contents = "kind = \"section\"\nid = \"attachment-identity\"\n";
        for directory in [page.join("assets"), t.path().join(".officeimo-staging-test")] {
            fs::create_dir(&directory).expect("owned files");
            fs::write(directory.join("index.md"), contents).expect("attached document");
            fs::write(directory.join("_node.toml"), node_contents).expect("attached metadata document");
        }
        normalize_ids(t.path()).expect("normalize notebook pages only");
        for directory in [page.join("assets"), t.path().join(".officeimo-staging-test")] {
            assert_eq!(fs::read_to_string(directory.join("index.md")).expect("attached document"), contents);
            assert_eq!(fs::read_to_string(directory.join("_node.toml")).expect("attached metadata document"), node_contents);
        }
        assert!(!notecraft_store::tree::read_node(&page).expect("page metadata").id.is_empty());
    }

    /// Issue #17: a package is unpacked and imported as a folder of sections named after it.
    /// No bridge is installed for the test, so the run stops at the first section; what it has
    /// found by then is the package's section.
    #[test]
    fn a_package_is_unpacked_and_its_sections_imported_as_a_folder() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/3p/officeimo/makecab-lzx-notebook.onepkg");
        let out = notecraft_store::testing::TempDir::new("officeimo-onepkg");
        let mut seen = Vec::new();
        let _ = run_with_progress(&source, out.path(), Format::Html, &AtomicBool::new(false), &mut |p| seen.push(format!("{p:?}")));
        assert!(seen.first().is_some_and(|p| p.contains("Found { total: 1 }")), "the package's one section was found: {seen:?}");
        assert!(seen.iter().any(|p| p.contains("Compressed section.one")), "{seen:?}");
        assert!(out.path().join("makecab-lzx-notebook").is_dir(), "the notebook is named after the package");
    }

    #[test]
    fn folder_names_and_backup_dates_become_safe_container_names() {
        assert_eq!(safe_name("2026"), "2026");
        assert_eq!(section_name(Path::new("Work (On 9-29-2026).one")), "Work");
        assert_eq!(safe_name("A:B"), "A_B");
    }

    #[test]
    fn a_folder_without_sections_does_not_leave_an_empty_imported_notebook() {
        let t = notecraft_store::testing::TempDir::new("officeimo-empty-source");
        let source = t.path().join("Source");
        let output = t.path().join("Output");
        fs::create_dir(&source).expect("source");
        fs::create_dir(&output).expect("output");
        fs::write(source.join("readme.txt"), "not a OneNote section").expect("source content");
        assert!(run_with_progress(&source, &output, Format::Html, &AtomicBool::new(false), &mut |_| {}).is_err());
        assert_eq!(fs::read_dir(&output).expect("output entries").count(), 0, "an invalid source must not create a phantom notebook");
        assert_eq!(fs::read_to_string(source.join("readme.txt")).expect("source preserved"), "not a OneNote section");
    }

    #[test]
    fn an_already_cancelled_folder_import_does_not_create_output() {
        let t = notecraft_store::testing::TempDir::new("officeimo-cancelled-folder");
        let source = t.path().join("Source");
        let output = t.path().join("Output");
        fs::create_dir(&source).expect("source");
        fs::create_dir(&output).expect("output");
        fs::write(source.join("Notes.one"), b"section fixture").expect("source section");
        let result = run_with_progress(&source, &output, Format::Html, &AtomicBool::new(true), &mut |_| {});
        assert!(result.is_err());
        assert_eq!(fs::read_dir(&output).expect("output entries").count(), 0, "cancellation before starting must leave no output");
    }

    #[test]
    fn folder_progress_reports_the_actual_notebook_destination() {
        for name in ["Notebook.v2", "Notes (On 1-2-2026)", "Folder with spaces"] {
            let t = notecraft_store::testing::TempDir::new("officeimo-reported-destination");
            let source = t.path().join(name);
            let output = t.path().join("Output");
            fs::create_dir(&source).expect("source");
            fs::create_dir(&output).expect("output");
            fs::create_dir(output.join(name)).expect("existing notebook stays intact");
            fs::write(source.join("Notes.one"), []).expect("section");
            let cancel = AtomicBool::new(false);
            let mut reported = None;
            let _ = run_with_progress(&source, &output, Format::Html, &cancel, &mut |event| {
                if let Progress::Destination { path } = event {
                    reported = Some(path);
                    // Exercise real discovery and destination creation without needing a bridge.
                    cancel.store(true, Ordering::Relaxed);
                }
            });
            let expected = output.join(format!("{name} (1)"));
            assert_eq!(reported, Some(expected.clone()), "the UI receives the importer's path for {name}");
            assert!(expected.join("_node.toml").is_file(), "the reported notebook exists");
            assert_eq!(fs::read(source.join("Notes.one")).expect("source retained"), b"");
        }
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

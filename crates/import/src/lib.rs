#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
//! NoteCraft importer (layer L3): a folder of OneNote section files into a NoteCraft notebook tree.
//!
//! ```text
//! discover  ->  plan  ->  read  ->  convert  ->  write  ->  report
//! ```
//!
//! The source is only ever read. Each section file is read, converted and written on its own,
//! so memory is bounded by the largest file, and a file that cannot be read costs that file,
//! not the run. The rules for what the folders and pages look like are the store's; this crate
//! only asks the store to make them.

pub mod convert;
pub mod discover;

use convert::{Loss, convert_page};
use discover::{Planned, Scan};
use notecraft_formats::markdown::Block;
use notecraft_formats::render::{blocks_to_html, blocks_to_markdown};
use notecraft_model::text::{Dir, direction};
use notecraft_model::{NodeKind, PageFormat};
use notecraft_onenote::{OneError, Page, Section, Store};
use notecraft_store::tree::{child_dirs, read_node};
use notecraft_store::{Settings, assets, ops, page};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// What to do when the destination already holds a section with the same name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Existing {
    /// Leave it alone, so running the import twice changes nothing.
    #[default]
    Skip,
    /// Import beside it, under a name made unique by the store.
    Keep,
    /// Delete it and import again.
    Replace,
}

#[derive(Clone, Debug)]
pub struct Options {
    /// A folder of `.one` files (searched all the way down), or one `.one` file.
    pub source: PathBuf,
    /// The notebooks folder to write into.
    pub dest: PathBuf,
    /// The format of every page written.
    pub format: PageFormat,
    /// Import older dated copies of a section too, named with their date.
    pub all_versions: bool,
    /// Plan and report, write nothing.
    pub dry_run: bool,
    pub existing: Existing,
    /// Only sections whose notebook, group or section name contains one of these words
    /// (ignoring case). Empty means all.
    pub only: Vec<String>,
}

impl Options {
    pub fn new(source: impl Into<PathBuf>, dest: impl Into<PathBuf>) -> Options {
        Options {
            source: source.into(),
            dest: dest.into(),
            format: PageFormat::Html,
            all_versions: false,
            dry_run: false,
            existing: Existing::default(),
            only: Vec::new(),
        }
    }
}

/// Something that happened, for a progress display.
#[derive(Clone, Debug)]
pub enum Event<'a> {
    Found { sections: usize, copies_skipped: usize, ignored: usize },
    Section { index: usize, total: usize, label: &'a str },
    Done,
}

/// A section that was not imported, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub label: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub dry_run: bool,
    pub files_found: usize,
    pub sections_planned: usize,
    /// Older dated copies that were not imported because a newer one was.
    pub older_copies_left: usize,
    /// `.onetoc2` and `.onepkg` files seen and not imported.
    pub ignored: usize,
    pub notebooks_made: usize,
    pub sections_imported: usize,
    pub sections_recovered: usize,
    pub pages: usize,
    pub subpages: usize,
    pub deepest: usize,
    pub empty_pages_left: usize,
    pub rtl_pages: usize,
    pub images: usize,
    pub files: usize,
    pub tables: usize,
    pub loss: Loss,
    pub skipped: Vec<Skipped>,
    pub elapsed: Duration,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mode = if self.dry_run { " (dry run, nothing written)" } else { "" };
        writeln!(f, "OneNote import{mode}")?;
        writeln!(f, "  files found: {}  sections: {}  older copies left out: {}", self.files_found, self.sections_planned, self.older_copies_left)?;
        if self.ignored > 0 {
            writeln!(f, "  table-of-contents and package files noticed, not imported yet: {}", self.ignored)?;
        }
        if !self.dry_run {
            writeln!(
                f,
                "  notebooks made: {}  sections imported: {}  pages: {} (sub-pages: {}, deepest level: {})",
                self.notebooks_made, self.sections_imported, self.pages, self.subpages, self.deepest
            )?;
            writeln!(
                f,
                "  pictures: {}  attached files: {}  tables: {}  right-to-left pages: {}",
                self.images, self.files, self.tables, self.rtl_pages
            )?;
            if self.sections_recovered > 0 {
                writeln!(f, "  sections whose page order is a guess (no section node in the file): {}", self.sections_recovered)?;
            }
            if self.empty_pages_left > 0 {
                writeln!(f, "  blank untitled pages left out: {}", self.empty_pages_left)?;
            }
            let l = &self.loss;
            if l.underlined_runs + l.math_runs + l.missing_data + l.unviewable_images > 0 {
                writeln!(
                    f,
                    "  not carried over: underline {} runs, math marks {} runs, missing picture or file data {}, pictures a browser cannot show {} (kept as files)",
                    l.underlined_runs, l.math_runs, l.missing_data, l.unviewable_images
                )?;
            }
        }
        if !self.skipped.is_empty() {
            writeln!(f, "  skipped: {}", self.skipped.len())?;
            let mut reasons: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
            for s in &self.skipped {
                *reasons.entry(s.reason.as_str()).or_default() += 1;
            }
            for (reason, n) in reasons {
                writeln!(f, "    {n} x {reason}")?;
            }
        }
        write!(f, "  took {:.1} s", self.elapsed.as_secs_f64())
    }
}

/// Runs the import.
pub fn run(options: &Options, progress: &mut dyn FnMut(&Event)) -> Result<Report, String> {
    let started = Instant::now();
    if !options.source.exists() {
        return Err(format!("the source {} does not exist", options.source.display()));
    }
    let scan: Scan = discover::scan(&options.source);
    let mut report = Report { dry_run: options.dry_run, files_found: scan.files.len(), ignored: scan.ignored.len(), ..Report::default() };
    let all = discover::plan(&scan.files, options.all_versions);
    report.older_copies_left = scan.files.len().saturating_sub(all.len());
    let planned: Vec<Planned> = all.into_iter().filter(|p| wanted(p, &options.only)).collect();
    report.sections_planned = planned.len();
    progress(&Event::Found { sections: planned.len(), copies_skipped: report.older_copies_left, ignored: report.ignored });

    let fallback = loose_notebook_name(&options.source);
    let settings = Settings::default_location();
    if !options.dry_run {
        fs::create_dir_all(&options.dest).map_err(|e| format!("cannot create {}: {e}", options.dest.display()))?;
    }
    let total = planned.len();
    for (i, item) in planned.iter().enumerate() {
        let label = label_of(item, &fallback);
        progress(&Event::Section { index: i + 1, total, label: &label });
        if options.dry_run {
            continue;
        }
        if let Err(reason) = import_section(options, &settings, item, &fallback, &mut report) {
            report.skipped.push(Skipped { label, reason });
        }
    }
    progress(&Event::Done);
    report.elapsed = started.elapsed();
    Ok(report)
}

fn wanted(p: &Planned, only: &[String]) -> bool {
    if only.is_empty() {
        return true;
    }
    let hay: Vec<String> = p.file.folders.iter().chain(std::iter::once(&p.file.name)).map(|s| s.to_lowercase()).collect();
    only.iter().any(|w| {
        let w = w.to_lowercase();
        hay.iter().any(|h| h.contains(&w))
    })
}

fn loose_notebook_name(source: &Path) -> String {
    let dir = if source.is_file() { source.parent().unwrap_or(source) } else { source };
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if name.trim().is_empty() || name.eq_ignore_ascii_case("backup") { "OneNote import".to_string() } else { name }
}

fn label_of(p: &Planned, fallback: &str) -> String {
    let mut parts: Vec<&str> = if p.file.folders.is_empty() { vec![fallback] } else { p.file.folders.iter().map(String::as_str).collect() };
    parts.push(&p.file.name);
    parts.join(" / ")
}

/// A title the store will accept: no control characters, not empty, not too long.
pub fn clean_title(text: &str, fallback: &str) -> String {
    let t: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = t.chars().take(ops::MAX_TITLE).collect();
    let t = t.trim().to_string();
    if t.is_empty() { fallback.to_string() } else { t }
}

fn child_by_title(parent: &Path, title: &str, kind: NodeKind) -> Option<PathBuf> {
    child_dirs(parent).into_iter().find(|d| read_node(d).is_some_and(|n| n.kind == kind && n.title == title))
}

fn import_section(options: &Options, settings: &Settings, item: &Planned, fallback: &str, report: &mut Report) -> Result<(), String> {
    let data = fs::read(&item.file.path).map_err(|e| format!("cannot read the file: {e}"))?;
    let store = match notecraft_onenote::read(&data) {
        Ok(s) => s,
        Err(OneError::Encrypted) => return Err("password protected".to_string()),
        Err(OneError::NotASection(_)) => return Err("not a OneNote section file".to_string()),
        Err(OneError::TooLarge(_)) => return Err("larger than the importer can read".to_string()),
        Err(e) => return Err(format!("cannot be read: {e}")),
    };
    let section = notecraft_onenote::section(&store);
    let encrypted = store.spaces.iter().filter(|s| s.encrypted).count();
    if section.pages.is_empty() {
        return Err(if encrypted > 0 { "password protected".to_string() } else { "no pages".to_string() });
    }

    // The notebook and its section groups.
    let mut folders = item.file.folders.clone();
    let notebook_title = clean_title(&if folders.is_empty() { fallback.to_string() } else { folders.remove(0) }, "Notebook");
    let notebook = match child_by_title(&options.dest, &notebook_title, NodeKind::Notebook) {
        Some(p) => p,
        None => {
            report.notebooks_made += 1;
            ops::create_notebook(&options.dest, &notebook_title, Some(options.format), settings).map_err(|e| e.to_string())?
        }
    };
    let mut parent = notebook;
    for group in &folders {
        let title = clean_title(group, "Section group");
        parent = match child_by_title(&parent, &title, NodeKind::SectionGroup) {
            Some(p) => p,
            None => ops::create_node(&parent, &title, NodeKind::SectionGroup, None, settings).map_err(|e| e.to_string())?,
        };
    }

    // The section itself.
    let mut name = clean_title(&item.file.name, "Section");
    if let Some(label) = &item.version_label {
        name = format!("{name} ({label})");
    }
    if let Some(existing) = child_by_title(&parent, &name, NodeKind::Section) {
        match options.existing {
            Existing::Skip => return Err("already in the destination".to_string()),
            Existing::Replace => {
                ops::delete_node(&options.dest, &existing).map_err(|e| e.to_string())?;
            }
            Existing::Keep => {}
        }
    }
    let section_dir = ops::create_node(&parent, &name, NodeKind::Section, None, settings).map_err(|e| e.to_string())?;
    write_pages(options, settings, &store, &data, &section, &section_dir, report)?;
    report.sections_imported += 1;
    if section.recovered {
        report.sections_recovered += 1;
    }
    if encrypted > 0 {
        report.skipped.push(Skipped {
            label: format!("{} / {}", item.file.folders.join(" / "), item.file.name),
            reason: format!("{encrypted} password protected pages left out"),
        });
    }
    Ok(())
}

fn write_pages(
    options: &Options,
    settings: &Settings,
    store: &Store,
    data: &[u8],
    section: &Section,
    section_dir: &Path,
    report: &mut Report,
) -> Result<(), String> {
    // The folder each level's latest page lives in; index 0 is the section.
    let mut stack: Vec<PathBuf> = vec![section_dir.to_path_buf()];
    for p in &section.pages {
        if p.blocks.is_empty() && (p.title.is_empty() || p.title == "Untitled page") {
            report.empty_pages_left += 1;
            continue;
        }
        let level = usize::from(p.level.clamp(1, 3));
        let parent = stack.get(level - 1).or_else(|| stack.last()).cloned().unwrap_or_else(|| section_dir.to_path_buf());
        let title = clean_title(&p.title, "Untitled page");
        let dir = ops::create_node(&parent, &title, NodeKind::Page, Some(options.format), settings).map_err(|e| e.to_string())?;
        stack.truncate(level);
        stack.push(dir.clone());
        report.pages += 1;
        if level > 1 {
            report.subpages += 1;
        }
        report.deepest = report.deepest.max(level);
        write_page(options.format, store, data, p, &dir, report)?;
    }
    Ok(())
}

fn write_page(format: PageFormat, store: &Store, data: &[u8], p: &Page, dir: &Path, report: &mut Report) -> Result<(), String> {
    let mut attach = |name: &str, bytes: &[u8]| assets::attach_bytes(dir, name, bytes).ok();
    let converted = convert_page(store, data, p, format == PageFormat::Cmgfm, &mut attach);
    let body = match format {
        PageFormat::Html => blocks_to_html(&converted.blocks),
        PageFormat::Cmgfm => blocks_to_markdown(&converted.blocks),
    };
    page::save_body(dir, &body).map_err(|e| e.to_string())?;
    if reads_right_to_left(&p.title, &converted.blocks) {
        page::set_direction(dir, Some(Dir::Rtl)).map_err(|e| e.to_string())?;
        report.rtl_pages += 1;
    }
    report.images += converted.images;
    report.files += converted.files;
    report.tables += converted.tables;
    report.loss.add(converted.loss);
    Ok(())
}

/// Whether most of a page's paragraphs read right to left, which is how the page is marked.
/// Each paragraph decides by its first strong letter; the title counts as one more.
fn reads_right_to_left(title: &str, blocks: &[Block]) -> bool {
    let (mut rtl, mut all) = (0usize, 0usize);
    let mut count = |text: &str| {
        if text.chars().any(char::is_alphabetic) {
            all += 1;
            if direction(text) == Dir::Rtl {
                rtl += 1;
            }
        }
    };
    count(title);
    for b in blocks {
        match b {
            Block::Paragraph(spans) | Block::Quote(spans) | Block::Heading { spans, .. } | Block::Item { spans, .. } => {
                count(&spans.iter().map(|s| s.text.as_str()).collect::<String>())
            }
            _ => {}
        }
    }
    all > 0 && rtl * 2 > all
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_onenote::testing::OneNote;
    use notecraft_store::testing::TempDir;
    use notecraft_store::tree::load_tree;

    fn section_file(root: &Path, folders: &[&str], file: &str, one: OneNote) {
        let mut dir = root.to_path_buf();
        for f in folders {
            dir.push(f);
        }
        fs::create_dir_all(&dir).expect("dir");
        fs::write(dir.join(file), one.finish()).expect("write");
    }

    fn quiet(_: &Event) {}

    fn sample(src: &Path) {
        let mut one = OneNote::new();
        one.page("Start", 1, &[OneNote::line("hello"), OneNote::styled("bold words", true, false)]);
        one.page("Child", 2, &[OneNote::line("child text")]);
        one.page("Grandchild", 3, &[OneNote::line("deep")]);
        one.page("Second", 1, &[OneNote::line("again")]);
        section_file(src, &["Work", "Projects"], "Plan (On 1-2-2024).one", one);
        // An older copy of the same section that must lose.
        section_file(src, &["Work", "Projects"], "Plan (On 1-1-2023).one", OneNote::new().with_page("Old", &[OneNote::line("old")]));
        section_file(src, &["Work"], "Loose.one", OneNote::new().with_page("Only", &[OneNote::line("x")]));
    }

    #[test]
    fn a_folder_of_sections_becomes_notebooks_groups_sections_and_nested_pages() {
        let t = TempDir::new("import-tree");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        let report = run(&Options::new(&src, &dest), &mut quiet).expect("import");
        assert_eq!((report.sections_planned, report.sections_imported, report.pages, report.subpages, report.deepest), (2, 2, 5, 2, 3), "{report}");
        assert_eq!(report.older_copies_left, 1);
        let tree = load_tree(&dest.join("work")).expect("the notebook");
        let text = format!("{tree:?}");
        for want in ["Work", "Projects", "Plan", "Start", "Child", "Grandchild", "Second", "Loose"] {
            assert!(text.contains(want), "{want} is in the tree: {text}");
        }
        assert!(!text.contains("Old"), "the older copy was left out");
    }

    #[test]
    fn sub_pages_nest_by_level_to_any_depth_the_file_has() {
        let t = TempDir::new("import-nest");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        run(&Options::new(&src, &dest), &mut quiet).expect("import");
        let start = dest.join("work").join("projects").join("plan").join("start");
        let child = start.join("child");
        assert!(child.join("index.html").is_file(), "the child page is a folder inside its parent");
        assert!(child.join("grandchild").join("index.html").is_file());
        assert!(
            dest.join("work").join("projects").join("plan").join("second").join("index.html").is_file(),
            "a page after the sub-pages is a sibling again"
        );
    }

    #[test]
    fn text_and_marks_reach_the_page_in_both_formats() {
        for (format, file, want) in [(PageFormat::Html, "index.html", "<b>bold words</b>"), (PageFormat::Cmgfm, "index.md", "**bold words**")] {
            let t = TempDir::new("import-format");
            let (src, dest) = (t.path().join("src"), t.path().join("dest"));
            sample(&src);
            let mut o = Options::new(&src, &dest);
            o.format = format;
            run(&o, &mut quiet).expect("import");
            let page = fs::read_to_string(dest.join("work").join("projects").join("plan").join("start").join(file)).expect("page");
            assert!(page.contains("hello") && page.contains(want), "{format:?}: {page}");
        }
    }

    #[test]
    fn running_twice_changes_nothing_and_replace_starts_over() {
        let t = TempDir::new("import-twice");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        let first = run(&Options::new(&src, &dest), &mut quiet).expect("first");
        let again = run(&Options::new(&src, &dest), &mut quiet).expect("again");
        assert_eq!((first.pages, again.pages, again.sections_imported), (5, 0, 0), "{again}");
        assert_eq!(again.skipped.len(), 2);
        let mut o = Options::new(&src, &dest);
        o.existing = Existing::Replace;
        let replaced = run(&o, &mut quiet).expect("replace");
        assert_eq!(replaced.pages, 5);
        assert_eq!(load_tree(&dest.join("work")).map(|n| format!("{n:?}").matches("Start").count()), Some(1), "no duplicate page");
    }

    #[test]
    fn a_dry_run_writes_nothing_and_all_versions_adds_the_older_copy() {
        let t = TempDir::new("import-dry");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        let mut o = Options::new(&src, &dest);
        o.dry_run = true;
        let r = run(&o, &mut quiet).expect("dry");
        assert_eq!(r.sections_planned, 2);
        assert!(!dest.exists(), "nothing was written");
        o.dry_run = false;
        o.all_versions = true;
        let r = run(&o, &mut quiet).expect("all");
        assert_eq!(r.sections_imported, 3, "{r}");
        assert!(format!("{:?}", load_tree(&dest.join("work"))).contains("Plan (2023-01-01)"));
    }

    #[test]
    fn a_damaged_file_costs_that_file_and_the_run_goes_on() {
        let t = TempDir::new("import-bad");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        fs::write(src.join("Work").join("Broken.one"), b"this is not a section file at all, only text").expect("write");
        let r = run(&Options::new(&src, &dest), &mut quiet).expect("import");
        assert_eq!(r.sections_imported, 2);
        assert!(r.skipped.iter().any(|s| s.label.contains("Broken")), "{r}");
    }

    #[test]
    fn a_single_file_and_the_only_filter_work() {
        let t = TempDir::new("import-one");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        sample(&src);
        let r = run(&Options::new(src.join("Work").join("Loose.one"), &dest), &mut quiet).expect("one file");
        assert_eq!((r.sections_imported, r.pages), (1, 1));
        let mut o = Options::new(&src, t.path().join("dest2"));
        o.only = vec!["projects".into()];
        assert_eq!(run(&o, &mut quiet).expect("only").sections_imported, 1);
    }

    #[test]
    fn a_page_in_arabic_is_marked_right_to_left() {
        let t = TempDir::new("import-rtl");
        let (src, dest) = (t.path().join("src"), t.path().join("dest"));
        section_file(&src, &["دفتر"], "قسم.one", OneNote::new().with_page("صفحة", &[OneNote::line("مرحبا بالعالم"), OneNote::line("نص آخر")]));
        let r = run(&Options::new(&src, &dest), &mut quiet).expect("import");
        assert_eq!((r.pages, r.rtl_pages), (1, 1), "{r}");
        let page = child_dirs(&dest).first().and_then(|d| load_tree(d)).map(|n| format!("{n:?}")).unwrap_or_default();
        assert!(page.contains("دفتر") && page.contains("صفحة"), "the names kept their letters: {page}");
    }

    #[test]
    fn titles_are_made_acceptable_without_losing_their_words() {
        assert_eq!(clean_title("  a\tb\nc  ", "x"), "a b c");
        assert_eq!(clean_title("\u{7}", "fallback"), "fallback");
        assert_eq!(clean_title(&"y".repeat(500), "x").chars().count(), ops::MAX_TITLE);
    }
}

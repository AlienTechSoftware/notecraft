//! Reviewed, lossless portable folders. Preparation reads a bounded snapshot; only `write`
//! publishes it. Original formats and metadata survive, unlike Save a Copy's new identities.
//! A scoped export keeps its notebook ancestry and includes local link dependencies.

use crate::{Result, StoreError, links, tree};
use notecraft_formats::{frontmatter, html};
use notecraft_model::{NodeKind, PageFormat, id::new_id};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_BYTES: usize = 256 * 1024 * 1024;
const MAX_FILES: usize = 4096;
const MAX_DEPTH: usize = 64;

/// The open editor's text, in its original format. Never written back to the source.
pub struct Snapshot<'a> {
    pub path: &'a Path,
    pub body: &'a str,
    /// A changed live drawing, including an empty drawing, without saving it at the source.
    pub ink: Option<&'a str>,
}

/// An immutable review snapshot, including file contents. Paths are notebook-relative.
#[derive(Debug)]
pub struct Prepared {
    pub title: String,
    pub files: Vec<String>,
    pub pages: usize,
    pub bytes: usize,
    pub formats: BTreeSet<String>,
    pub dependencies: Vec<String>,
    pub warnings: Vec<String>,
    notebook: PathBuf,
    contents: BTreeMap<PathBuf, Vec<u8>>,
}

/// Prepare a page (including subpages), section, group or notebook without writing anything.
/// Linked local content is included transitively within this notebook. Missing or escaping
/// local references are refused rather than producing an export with silently lost content.
pub fn prepare(node: &Path, snapshot: Option<Snapshot<'_>>) -> Result<Prepared> {
    let node = canonical(node)?;
    if tree::read_node(&node).is_none() {
        return Err(StoreError::invalid("select a NoteCraft page, section or notebook"));
    }
    let notebook = node
        .ancestors()
        .find(|p| tree::read_node(p).is_some_and(|n| n.kind == NodeKind::Notebook))
        .ok_or_else(|| StoreError::invalid("the selection must belong to a notebook"))?
        .to_path_buf();
    let snapshot = match snapshot {
        Some(s) => {
            let path = canonical(s.path)?;
            if !path.starts_with(&node) || tree::page_file(&path).is_none() {
                return Err(StoreError::invalid("the editor snapshot must be a page in the selected scope"));
            }
            Some((path, s.body, s.ink))
        }
        None => None,
    };
    let mut out = Prepared {
        title: tree::read_node(&node).map(|n| n.title).unwrap_or_default(),
        files: Vec::new(),
        pages: 0,
        bytes: 0,
        formats: BTreeSet::new(),
        dependencies: Vec::new(),
        warnings: Vec::new(),
        notebook,
        contents: BTreeMap::new(),
    };
    out.add_tree(&node, 0)?;
    out.add_ancestry(&node)?;
    if let Some((path, body, ink)) = snapshot {
        let (format, file) = tree::page_file(&path).ok_or_else(|| StoreError::invalid("snapshot page is missing"))?;
        let text = String::from_utf8(out.read(&file)?).map_err(|_| StoreError::invalid("page text is not UTF-8"))?;
        let text = match format {
            PageFormat::Html => html::replace_inner(&text, body),
            PageFormat::Cmgfm => format!("{}\n{}", frontmatter::split_front(&text).0, body.trim_start_matches('\n')),
        };
        out.insert(&file, text.into_bytes())?;
        if let Some(ink) = ink {
            out.insert(&path.join("ink.svg"), ink.as_bytes().to_vec())?;
        }
    }
    let mut checked = BTreeSet::new();
    loop {
        let pages: Vec<_> = out
            .contents
            .keys()
            .filter(|p| matches!(p.file_name().and_then(|s| s.to_str()), Some("index.html" | "index.md")))
            .filter(|p| !checked.contains(*p))
            .cloned()
            .collect();
        if pages.is_empty() {
            break;
        }
        for relative in pages {
            checked.insert(relative.clone());
            out.review_page(&relative)?;
        }
    }
    out.pages = checked.len();
    out.files = out.contents.keys().map(|p| p.to_string_lossy().replace('\\', "/")).collect();
    out.dependencies.sort();
    out.dependencies.dedup();
    out.warnings.sort();
    out.warnings.dedup();
    Ok(out)
}

impl Prepared {
    /// Publish under a new, exclusively reserved export folder. Existing output is never
    /// overwritten. An invalid destination or write failure retains the reviewed snapshot
    /// for retry and removes only the output directory this call created.
    pub fn write(&self, folder: &Path) -> Result<PathBuf> {
        let folder = canonical(folder)?;
        if !folder.is_dir() || folder.starts_with(&self.notebook) {
            return Err(StoreError::invalid("choose a destination folder outside the source notebook"));
        }
        let base = format!("{}-export", tree::folder_name(&self.notebook));
        let reserved = loop {
            let path = crate::ops::unique_child(&folder, &base);
            match fs::create_dir(&path) {
                Ok(()) => break path,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(StoreError::io("create export", &path, e)),
            }
        };
        let pending = reserved.join(format!(".pending-{}", new_id()));
        let target = reserved.join(tree::folder_name(&self.notebook));
        let result = (|| {
            fs::create_dir(&pending).map_err(|e| StoreError::io("create export", &pending, e))?;
            for (path, bytes) in &self.contents {
                let file = pending.join(path);
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent).map_err(|e| StoreError::io("create export", parent, e))?;
                }
                fs::write(&file, bytes).map_err(|e| StoreError::io("write export", &file, e))?;
            }
            fs::rename(&pending, &target).map_err(|e| StoreError::io("publish export", &target, e))?;
            Ok(target)
        })();
        if result.is_err() {
            fs::remove_dir_all(&reserved).map_err(|e| StoreError::io("remove incomplete export", &reserved, e))?;
        }
        result
    }

    /// The exact reviewed bytes, for hosts that deliver an explicit download instead of a folder.
    pub fn entries(&self) -> impl Iterator<Item = (&Path, &[u8])> {
        self.contents.iter().map(|(path, bytes)| (path.as_path(), bytes.as_slice()))
    }

    pub fn notebook_name(&self) -> String {
        tree::folder_name(&self.notebook)
    }

    fn insert(&mut self, path: &Path, bytes: Vec<u8>) -> Result<()> {
        let relative = path.strip_prefix(&self.notebook).map_err(|_| StoreError::invalid("export file is outside its notebook"))?.to_path_buf();
        let old = self.contents.get(&relative).map_or(0, Vec::len);
        let total = self.bytes.saturating_sub(old).saturating_add(bytes.len());
        if total > MAX_BYTES || (!self.contents.contains_key(&relative) && self.contents.len() >= MAX_FILES) {
            return Err(StoreError::invalid("export exceeds 256 MiB or 4096 files; choose a smaller scope"));
        }
        self.bytes = total;
        self.contents.insert(relative, bytes);
        Ok(())
    }

    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        let file = fs::File::open(path).map_err(|e| StoreError::io("read export", path, e))?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| StoreError::io("read export", path, e))?;
        if bytes.len() > MAX_BYTES {
            return Err(StoreError::invalid("an export file exceeds 256 MiB"));
        }
        Ok(bytes)
    }

    fn add_tree(&mut self, path: &Path, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(StoreError::invalid("export folders are nested too deeply"));
        }
        let meta = fs::symlink_metadata(path).map_err(|e| StoreError::io("read export", path, e))?;
        if meta.file_type().is_symlink() || !canonical(path)?.starts_with(&self.notebook) {
            return Err(StoreError::invalid("export refuses symbolic links and files outside its notebook"));
        }
        if meta.is_file() {
            let rel = path.strip_prefix(&self.notebook).map_err(|_| StoreError::invalid("file outside notebook"))?;
            if !self.contents.contains_key(rel) {
                self.insert(path, self.read(path)?)?;
            }
        } else if meta.is_dir() {
            for entry in fs::read_dir(path).map_err(|e| StoreError::io("read export", path, e))? {
                let entry = entry.map_err(|e| StoreError::io("read export", path, e))?;
                if ![".notecraft", ".git", ".hg", ".svn"].iter().any(|name| entry.file_name() == *name) {
                    self.add_tree(&entry.path(), depth + 1)?;
                }
            }
        } else {
            return Err(StoreError::invalid("export only supports ordinary files and folders"));
        }
        Ok(())
    }

    fn add_ancestry(&mut self, node: &Path) -> Result<()> {
        let notebook = self.notebook.clone();
        for ancestor in node.ancestors().skip(1).take_while(|p| p.starts_with(&notebook)) {
            if let Some((_, file)) = tree::page_file(ancestor) {
                self.dependencies.push(
                    ancestor
                        .strip_prefix(&notebook)
                        .map_err(|_| StoreError::invalid("ancestor outside notebook"))?
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                self.add_tree(&file, 0)?;
            } else {
                self.add_tree(&ancestor.join("_node.toml"), 0)?;
            }
        }
        Ok(())
    }

    fn review_page(&mut self, relative: &Path) -> Result<()> {
        let file = self.notebook.join(relative);
        let dir = file.parent().ok_or_else(|| StoreError::invalid("page has no folder"))?;
        let format = if relative.extension().is_some_and(|s| s == "html") { PageFormat::Html } else { PageFormat::Cmgfm };
        self.formats.insert(format.id().to_string());
        let bytes = self.contents.get(relative).ok_or_else(|| StoreError::invalid("page is missing from export"))?;
        let text = std::str::from_utf8(bytes).map_err(|_| StoreError::invalid("page text is not UTF-8"))?.to_string();
        if text.len() > crate::page::MAX_PAGE_BYTES {
            return Err(StoreError::invalid("an exported page is too large to reopen"));
        }
        let mut replacements = Vec::new();
        for reference in notecraft_formats::references::in_text(format, &text) {
            let href = reference.target.as_str();
            if href.starts_with('#') || href.is_empty() {
                continue;
            }
            let resolved = match links::resolve(&self.notebook, dir, href) {
                links::Link::Missing(path) if path.strip_prefix(&self.notebook).is_ok_and(|p| self.contents.contains_key(p)) => {
                    links::Link::File(path)
                }
                other => other,
            };
            match resolved {
                links::Link::Node(target) | links::Link::File(target) => {
                    if target
                        .ancestors()
                        .take_while(|p| p.starts_with(&self.notebook))
                        .any(|p| fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink()))
                    {
                        return Err(StoreError::invalid("export refuses references through symbolic links"));
                    }
                    let target = if target.exists() { canonical(&target)? } else { target };
                    if !target.starts_with(&self.notebook)
                        || target.components().any(|c| [".notecraft", ".git", ".hg", ".svn"].iter().any(|name| c.as_os_str() == *name))
                    {
                        return Err(StoreError::invalid("a local reference leaves the exportable notebook content"));
                    }
                    let rel = target.strip_prefix(&self.notebook).map_err(|_| StoreError::invalid("reference outside notebook"))?;
                    let present = if let Some((_, page)) = tree::page_file(&target) {
                        self.contents.contains_key(page.strip_prefix(&self.notebook).map_err(|_| StoreError::invalid("page outside notebook"))?)
                    } else {
                        self.contents.contains_key(rel)
                    };
                    if !present {
                        self.dependencies.push(rel.to_string_lossy().replace('\\', "/"));
                        self.add_tree(&target, 0)?;
                        self.add_ancestry(&target)?;
                    }
                    if href.get(..links::SCHEME.len()).is_some_and(|s| s.eq_ignore_ascii_case(links::SCHEME)) || Path::new(href).is_absolute() {
                        let mut portable = relative_link(dir, &target);
                        if let Some((_, fragment)) = href.split_once('#') {
                            portable.push('#');
                            portable.push_str(fragment);
                        }
                        let encoded = reference.encode(&portable);
                        replacements.push((reference.range, encoded));
                    }
                }
                links::Link::Web(_) => self.warnings.push(href.to_string()),
                links::Link::Missing(_) | links::Link::Outside => {
                    return Err(StoreError::invalid(format!("cannot export unresolved or outside local reference: {href}")));
                }
            }
        }
        let mut text = text;
        for (range, replacement) in replacements.into_iter().rev() {
            text.replace_range(range, &replacement);
        }
        self.insert(&file, text.into_bytes())
    }
}

fn canonical(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).map_err(|e| StoreError::io("open export", path, e))
}

fn relative_link(from: &Path, to: &Path) -> String {
    let from: Vec<_> = from.components().collect();
    let to: Vec<_> = to.components().collect();
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts = vec!["..".to_string(); from.len() - common];
    parts.extend(to[common..].iter().map(|p| p.as_os_str().to_string_lossy().into_owned()));
    let link = if parts.is_empty() { ".".to_string() } else { parts.join("/") };
    link.replace('%', "%25").replace(' ', "%20").replace('#', "%23").replace('?', "%3F").replace('(', "%28").replace(')', "%29")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Settings, ops, page, testing::TempDir};

    #[test]
    fn read_only_source_files_export_without_source_writes_or_repository_records() {
        let t = TempDir::new("portable-readonly");
        let settings = Settings::at(t.path().join("settings.toml"));
        let nb = ops::create_notebook(&t.path().join("books"), "Book", None, &settings).expect("book");
        let section = ops::create_node(&nb, "Section", NodeKind::Section, None, &settings).expect("section");
        let page = ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("page");
        fs::create_dir(nb.join(".git")).expect("repository metadata");
        fs::write(nb.join(".git/config"), "not content").expect("config");
        let file = page.join("index.html");
        let original = fs::read(&file).expect("source");
        let permissions = fs::metadata(&file).expect("metadata").permissions();
        let mut readonly = permissions.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&file, readonly).expect("read only");
        let prepared = prepare(&nb, None);
        fs::set_permissions(&file, permissions).expect("restore permissions");
        let exported = prepared.expect("read only preparation").write(t.path()).expect("publish");
        assert_eq!(fs::read(exported.join("section/page/index.html")).expect("page"), original);
        assert!(!exported.join(".git").exists());
        assert_eq!(fs::read(file).expect("source unchanged"), original);
    }

    #[cfg(unix)]
    #[test]
    fn linked_content_cannot_hide_a_symbolic_link_outside_the_selected_subtree() {
        let t = TempDir::new("portable-linked-symlink");
        let settings = Settings::at(t.path().join("settings.toml"));
        let nb = ops::create_notebook(&t.path().join("books"), "Book", None, &settings).expect("book");
        let section = ops::create_node(&nb, "Section", NodeKind::Section, None, &settings).expect("section");
        let page = ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("page");
        fs::write(nb.join("picture.png"), b"picture").expect("picture");
        std::os::unix::fs::symlink(nb.join("picture.png"), nb.join("alias.png")).expect("real symbolic link");
        page::save_body(&page, "<img src=\"../../alias.png\">").expect("body");
        assert!(prepare(&page, None).is_err());
    }

    /// Real path-length failure after staging starts, without an injected filesystem.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_partial_write_failure_removes_only_its_reserved_export_folder() {
        let t = TempDir::new("portable-partial-write");
        let settings = Settings::at(t.path().join("settings.toml"));
        let nb = ops::create_notebook(&t.path().join("books"), "Book", None, &settings).expect("book");
        let section = ops::create_node(&nb, &"s".repeat(200), NodeKind::Section, None, &settings).expect("section");
        let page = ops::create_node(&section, &"p".repeat(200), NodeKind::Page, None, &settings).expect("page");
        fs::write(page.join("assets").join("a".repeat(200)), b"asset").expect("long nested asset");
        let mut destination = t.path().join("destination");
        // Titles become 40-character slugs, so title length cannot force failure.
        // Keep the destination and staging directories below Linux's 4096-byte
        // path limit, while the nested 200-byte asset name crosses it.
        while destination.as_os_str().len() < 3900 {
            let remaining = 3900 - destination.as_os_str().len();
            destination.push("d".repeat((remaining - 1).min(200)));
        }
        fs::create_dir_all(&destination).expect("long destination");
        fs::write(destination.join("keep.txt"), b"unrelated").expect("existing file");
        let prepared = prepare(&nb, None).expect("prepare");
        let error = prepared.write(&destination).expect_err("nested output exceeds the filesystem path limit");
        assert!(matches!(&error, StoreError::Io { action: "write export", .. }), "{error}");
        assert_eq!(fs::read_dir(&destination).expect("cleaned destination").count(), 1);
        assert_eq!(fs::read(destination.join("keep.txt")).expect("existing file"), b"unrelated");
        assert!(page.join("index.html").is_file());
    }

    #[test]
    fn the_complete_official_demo_exports_without_changing_either_representation() {
        let t = TempDir::new("portable-demo");
        for format in ["html", "cmgfm"] {
            let demo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/demo/DemoNotebook").join(format);
            let prepared = prepare(&demo, None).expect("prepare official demo");
            let exported = prepared.write(t.path()).expect("export official demo");
            assert!(prepared.pages > 20);
            for (relative, bytes) in prepared.entries() {
                assert_eq!(fs::read(exported.join(relative)).expect("exported file"), bytes, "{}", relative.display());
            }
        }
    }

    #[test]
    fn scoped_exports_carry_rich_content_ink_assets_and_linked_pages_in_both_formats() {
        for format in [PageFormat::Html, PageFormat::Cmgfm] {
            let t = TempDir::new("portable-content");
            let settings = Settings::at(t.path().join("settings.toml"));
            let nb = ops::create_notebook(&t.path().join("books"), "Book", Some(format), &settings).expect("book");
            let section = ops::create_node(&nb, "Section", NodeKind::Section, None, &settings).expect("section");
            let a = ops::create_node(&section, "First", NodeKind::Page, None, &settings).expect("first");
            let b = ops::create_node(&section, "Second", NodeKind::Page, None, &settings).expect("second");
            let uri = links::page_uri(std::slice::from_ref(&nb), &b).expect("uri");
            let body = match format {
                PageFormat::Html => format!(
                    "<p><strong>Bold</strong> <a href=\"{uri}\">other</a><a href=\"assets/report.pdf\">report</a></p><img src=\"assets/pic.png\"><img src=\"ink.svg\">"
                ),
                PageFormat::Cmgfm => format!("**Bold** [other]({uri}) [report](assets/report.pdf)\n\n![pic](assets/pic.png)\n\n![ink](ink.svg)"),
            };
            page::save_body(&a, &body).expect("body");
            page::save_body(&b, if format == PageFormat::Html { "<p><a href=\"../first/\">back</a></p>" } else { "[back](../first/)" })
                .expect("back link cycle");
            fs::write(a.join("assets/pic.png"), b"picture bytes").expect("pic");
            fs::write(a.join("assets/report.pdf"), b"attachment bytes").expect("attachment");
            let ink = "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:nc=\"https://notecraft.org/ns/ink\"><path d=\"M0 0 L1 1\"/></svg>";
            fs::write(a.join("ink.svg"), ink).expect("ink");
            let original = fs::read(a.join(format.file_name())).expect("original");
            let out = t.path().join("out");
            fs::create_dir(&out).expect("out");
            for scope in [&a, &section, &nb] {
                let prepared = prepare(scope, None).expect("prepare");
                assert_eq!(prepared.pages, 2);
                assert!(prepared.files.iter().all(|f| !f.contains(".notecraft")));
                assert_eq!(prepared.formats, BTreeSet::from([format.id().to_string()]));
                let exported = prepared.write(&out).expect("publish");
                let first = exported.join("section/first");
                let second = exported.join("section/second");
                assert_eq!(tree::read_node(&exported).expect("reopen notebook").kind, NodeKind::Notebook);
                let copied = page::load_page(&first).expect("reopen page");
                assert!(copied.body.contains("Bold"));
                assert!(!copied.body.contains("notecraft:"));
                let references = notecraft_formats::references::in_text(format, &copied.body);
                assert!(references.iter().any(|r| links::resolve(&exported, &first, &r.target) == links::Link::Node(second.clone())));
                assert_eq!(fs::read(first.join("assets/report.pdf")).expect("pdf"), b"attachment bytes");
                assert_eq!(fs::read_to_string(first.join("ink.svg")).expect("ink"), ink);
            }
            assert_eq!(fs::read(a.join(format.file_name())).expect("unchanged"), original);
            let prepared = prepare(&a, None).expect("snapshot");
            fs::rename(&nb, t.path().join("source-away")).expect("detach source");
            let exported = prepared.write(&out).expect("write independent snapshot");
            assert!(page::load_page(&exported.join("section/first")).expect("independent page").body.contains("Bold"));
        }
    }

    #[test]
    fn export_refuses_missing_or_escaping_assets_and_invalid_destinations_without_partial_output() {
        let t = TempDir::new("portable-invalid");
        let settings = Settings::at(t.path().join("settings.toml"));
        let nb = ops::create_notebook(&t.path().join("books"), "Book", None, &settings).expect("book");
        let section = ops::create_node(&nb, "Section", NodeKind::Section, None, &settings).expect("section");
        let a = ops::create_node(&section, "Page", NodeKind::Page, None, &settings).expect("page");
        for src in ["assets/missing.png", "../../../../secret.png", "file:///secret.png", ".notecraft/versions/private"] {
            page::save_body(&a, &format!("<img src=\"{src}\"> ")).expect("body");
            assert!(prepare(&a, None).is_err(), "{src}");
        }
        page::save_body(&a, "<p><a href=\"https://example.test\">online</a></p>").expect("body");
        let prepared = prepare(&a, None).expect("external link is disclosed");
        assert_eq!(prepared.warnings, ["https://example.test"]);
        assert!(prepared.write(&nb).is_err());
        assert!(prepared.write(&t.path().join("absent")).is_err());
        let file = t.path().join("file");
        fs::write(&file, b"keep").expect("file");
        assert!(prepared.write(&file).is_err());
        assert_eq!(fs::read(&file).expect("unchanged"), b"keep");
        assert!(!nb.join("book-export").exists());
    }
}

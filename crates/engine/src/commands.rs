//! The command registry. One `CommandSpec` per user-visible action.

use crate::error::EngineError;
use crate::session::Session;
use notecraft_model::{NodeColor, NodeKind, PageFormat};
use notecraft_store::check::Report;
use notecraft_store::page::Page;
use notecraft_store::search::Hit;
use notecraft_store::{StoreError, assets, check, ops, page, search, tree};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type Params = BTreeMap<String, String>;

#[derive(Debug)]
pub enum Output {
    None,
    Path(PathBuf),
    Text(String),
    Page(Page),
    Hits(Vec<Hit>),
    Report(Report),
    Count(usize),
}

pub struct CommandSpec {
    pub id: &'static str,
    pub label: &'static str,
    /// Parameters, one per line: `name (required|optional): meaning`.
    pub params: &'static str,
    /// True if the command can change the notebook tree (the session reloads it afterwards).
    pub changes_tree: bool,
    pub run: fn(&mut Session, &Params) -> Result<Output, EngineError>,
}

fn req<'a>(p: &'a Params, key: &'static str) -> Result<&'a str, EngineError> {
    p.get(key).map(String::as_str).ok_or(EngineError::MissingParam(key))
}

fn opt_format(p: &Params) -> Result<Option<PageFormat>, EngineError> {
    match p.get("format").map(|f| f.trim()).filter(|f| !f.is_empty()) {
        Some(f) => Ok(Some(PageFormat::parse(f)?)),
        None => Ok(None),
    }
}

/// What a new notebook starts with, the way OneNote does it: one section and one empty page.
/// Microsoft's accessibility documentation describes a new notebook as getting a section named
/// "New Section 1" and one empty, unnamed page. NoteCraft gives that page a visible title
/// because a page here is a folder and needs a name.
pub const FIRST_SECTION: &str = "New Section 1";
pub const FIRST_PAGE: &str = "Untitled page";

fn notebook_create(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let dir = ops::create_notebook(s.root(), req(p, "name")?, opt_format(p)?, &s.settings)?;
    if p.get("empty").map(String::as_str) != Some("true") {
        let section = ops::create_node(&dir, FIRST_SECTION, NodeKind::Section, None, &s.settings)?;
        ops::create_node(&section, FIRST_PAGE, NodeKind::Page, None, &s.settings)?;
    }
    Ok(Output::Path(dir))
}

fn node_create(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let parent = s.resolve(req(p, "parent")?)?;
    let kind = NodeKind::parse(req(p, "kind")?).ok_or_else(|| EngineError::BadParam("kind must be section-group, section or page".into()))?;
    let dir = ops::create_node(&parent, req(p, "title")?, kind, opt_format(p)?, &s.settings)?;
    Ok(Output::Path(dir))
}

fn node_color(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let raw = req(p, "color")?.trim();
    let color = if raw.is_empty() || raw == "none" || raw == "default" {
        None
    } else {
        Some(
            NodeColor::parse(raw)
                .ok_or_else(|| EngineError::BadParam(format!("color must be none or one of: {}", NodeColor::ALL.map(|c| c.id()).join(", "))))?,
        )
    };
    ops::set_color(&path, color)?;
    Ok(Output::None)
}

fn node_delete(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    Ok(Output::Count(ops::delete_node(s.root(), &path)?))
}

fn node_title(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    ops::set_title(&path, req(p, "title")?)?;
    Ok(Output::None)
}

fn page_load(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Page(page::load_page(&s.resolve(req(p, "path")?)?)?))
}

fn page_save(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    Ok(Output::Text(page::save_body(&path, req(p, "body")?)?))
}

fn page_attach(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    Ok(Output::Text(assets::attach_file(&path, std::path::Path::new(req(p, "source")?))?))
}

fn default_format_get(s: &mut Session, _p: &Params) -> Result<Output, EngineError> {
    let (f, source) = s.settings.global_format();
    Ok(Output::Text(format!("{} ({})", f.id(), source.label())))
}

fn default_format_set(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    s.settings.set_default_format(PageFormat::parse(req(p, "format")?)?)?;
    Ok(Output::None)
}

fn search_query(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let limit = match p.get("limit") {
        Some(l) => l.trim().parse::<usize>().map_err(|_| EngineError::BadParam("limit must be a number".into()))?.min(500),
        None => 40,
    };
    Ok(Output::Hits(search::search(s.root(), req(p, "q")?, limit)))
}

fn notebook_check(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    Ok(Output::Report(check::check_tree(&path)))
}

fn tree_refresh(s: &mut Session, _p: &Params) -> Result<Output, EngineError> {
    s.refresh();
    Ok(Output::Count(s.tree().iter().map(|n| n.count()).sum()))
}

/// One line per node: the title indented by its depth, what it is, and the path
/// every other command takes.
fn tree_line(n: &tree::Node, root: &std::path::Path, depth: usize, out: &mut String) {
    let fmt = n.info.format.map(|f| format!(", {}", f.short())).unwrap_or_default();
    let path = n.path.strip_prefix(root).unwrap_or(&n.path).to_string_lossy().replace('\\', "/");
    out.push_str(&format!("{}{}  [{}{}]  {}\n", "  ".repeat(depth), n.info.title, n.info.kind.id(), fmt, path));
    for c in &n.children {
        tree_line(c, root, depth + 1, out);
    }
}

fn tree_read(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let mut out = String::new();
    match p.get("path").map(String::as_str).filter(|t| !t.trim().is_empty()) {
        Some(path) => {
            let dir = s.resolve(path)?;
            let node = tree::load_tree(&dir).ok_or_else(|| StoreError::NotFound(path.to_string()))?;
            tree_line(&node, s.root(), 0, &mut out);
        }
        None => {
            for n in s.tree() {
                tree_line(n, s.root(), 0, &mut out);
            }
        }
    }
    Ok(Output::Text(out.trim_end().to_string()))
}

static REGISTRY: &[CommandSpec] = &[
    CommandSpec {
        id: "notebook.create",
        label: "New notebook",
        params: "name (required): the notebook's title\nformat (optional): html or cmgfm; default: the global setting\nempty (optional): true to leave the notebook empty instead of creating the first section and page",
        changes_tree: true,
        run: notebook_create,
    },
    CommandSpec {
        id: "node.create",
        label: "New section group, section or page",
        params: "parent (required): path of the notebook, group, section or page to add to\nkind (required): section-group, section or page\ntitle (required)\nformat (optional): html or cmgfm. For a page, its format; for a group or section, the default for pages below",
        changes_tree: true,
        run: node_create,
    },
    CommandSpec {
        id: "node.color",
        label: "Colour a notebook, section group or section",
        params: "path (required): the notebook, group or section\ncolor (required): purple, blue, teal, green, yellow, orange, red, magenta, gray, or none for the theme's own colour",
        changes_tree: true,
        run: node_color,
    },
    CommandSpec {
        id: "node.delete",
        label: "Delete a notebook, section group, section or page",
        params: "path (required): the node to delete with everything inside it",
        changes_tree: true,
        run: node_delete,
    },
    CommandSpec {
        id: "node.title",
        label: "Rename the title of a node",
        params: "path (required)\ntitle (required)",
        changes_tree: true,
        run: node_title,
    },
    CommandSpec { id: "page.load", label: "Open a page", params: "path (required): the page", changes_tree: false, run: page_load },
    CommandSpec {
        id: "page.save",
        label: "Save a page",
        params: "path (required): the page\nbody (required): the editable part (inside of the HTML body, or the Markdown after the front matter)",
        changes_tree: false,
        run: page_save,
    },
    CommandSpec {
        id: "page.attach",
        label: "Attach a file to a page",
        params: "path (required): the page\nsource (required): a file to copy into the page's assets folder",
        changes_tree: false,
        run: page_attach,
    },
    CommandSpec {
        id: "settings.default_format.get",
        label: "Show the global default format",
        params: "",
        changes_tree: false,
        run: default_format_get,
    },
    CommandSpec {
        id: "settings.default_format.set",
        label: "Set the global default format",
        params: "format (required): html or cmgfm",
        changes_tree: false,
        run: default_format_set,
    },
    CommandSpec {
        id: "search.query",
        label: "Search all pages",
        params: "q (required): text to find\nlimit (optional): at most this many hits (default 40, at most 500)",
        changes_tree: false,
        run: search_query,
    },
    CommandSpec {
        id: "notebook.check",
        label: "Check a notebook against the folder rules",
        params: "path (required): the notebook",
        changes_tree: false,
        run: notebook_check,
    },
    CommandSpec {
        id: "tree.read",
        label: "List the notebooks, sections and pages",
        params: "path (optional): start at this node instead of listing every notebook",
        changes_tree: false,
        run: tree_read,
    },
    CommandSpec { id: "tree.refresh", label: "Reload the tree from disk", params: "", changes_tree: false, run: tree_refresh },
];

pub fn registry() -> &'static [CommandSpec] {
    REGISTRY
}

/// The text of `docs/commands.md`, generated from the registry. A test keeps the file in step.
#[cfg(test)]
fn commands_markdown() -> String {
    let mut out = String::from(
        "# Commands\n\nGenerated from the command registry. Do not edit by hand: run\n`UPDATE_DOCS=1 cargo test -p notecraft-engine commands_doc` to refresh it.\n\
         Every front end (window, `notecraft-cli run`, the control channel) uses these ids.\n\n",
    );
    for c in REGISTRY {
        out.push_str(&format!("## `{}`\n\n{}{}.\n\n", c.id, c.label, if c.changes_tree { " (changes the tree)" } else { "" }));
        if c.params.is_empty() {
            out.push_str("No parameters.\n\n");
        } else {
            out.push_str("Parameters:\n\n");
            for line in c.params.lines() {
                out.push_str(&format!("- {line}\n"));
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::Settings;
    use notecraft_store::testing::TempDir;

    #[test]
    fn commands_doc_matches_the_registry() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/commands.md");
        let want = commands_markdown();
        if std::env::var_os("UPDATE_DOCS").is_some() {
            std::fs::write(&path, &want).expect("write docs/commands.md");
        }
        let have = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
        assert_eq!(have, want, "docs/commands.md is out of date; run with UPDATE_DOCS=1");
    }

    fn session(t: &TempDir) -> Session {
        Session::new(&t.path().join("notebooks"), Settings::at(t.path().join("settings").join("s.toml"))).expect("session")
    }

    fn p(pairs: &[(&str, &str)]) -> Params {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn path_of(out: Output) -> PathBuf {
        match out {
            Output::Path(p) => p,
            other => panic!("expected a path, got {other:?}"),
        }
    }

    #[test]
    fn ids_are_unique_and_documented() {
        let mut seen = std::collections::HashSet::new();
        for c in registry() {
            assert!(seen.insert(c.id), "duplicate id {}", c.id);
            assert!(!c.label.is_empty());
            assert!(c.id.contains('.'), "ids are namespaced: {}", c.id);
        }
    }

    /// OneNote gives a new notebook one section and one empty page. NoteCraft does the same,
    /// so the user can start typing instead of building a structure first.
    #[test]
    fn a_new_notebook_starts_with_one_section_and_one_page() {
        let t = TempDir::new("eng-first");
        let mut s = session(&t);
        let _nb = path_of(s.run("notebook.create", &p(&[("name", "Work")])).expect("nb"));
        let tree = s.tree();
        let section = tree.first().and_then(|n| n.children.first()).expect("a section");
        assert_eq!(section.info.title, FIRST_SECTION);
        assert_eq!(section.info.kind, notecraft_model::NodeKind::Section);
        let page = section.children.first().expect("a page");
        assert_eq!(page.info.title, FIRST_PAGE);
        assert_eq!(page.info.kind, notecraft_model::NodeKind::Page);
        assert!(page.children.is_empty());
        assert_eq!(tree.first().map(notecraft_store::Node::count), Some(3), "the notebook, the section and the page");
        // The page is real: its title and the date it was started, like a new OneNote page.
        match s.run("page.load", &p(&[("path", page.path.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => {
                assert!(pg.body.contains("<h1>Untitled page</h1>"), "{}", pg.body);
                assert!(pg.body.contains("page-date"), "a new page carries its date: {}", pg.body);
            }
            other => panic!("{other:?}"),
        }
        // The page follows the notebook's format.
        let md = path_of(s.run("notebook.create", &p(&[("name", "Notes"), ("format", "cmgfm")])).expect("nb"));
        assert!(md.join(notecraft_model::names::slug(FIRST_SECTION)).join(notecraft_model::names::slug(FIRST_PAGE)).join("index.md").is_file());
        // `empty=true` is the way to skip it.
        let bare = path_of(s.run("notebook.create", &p(&[("name", "Bare"), ("empty", "true")])).expect("nb"));
        assert_eq!(std::fs::read_dir(&bare).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).count(), 0);
    }

    #[test]
    fn full_workflow_through_commands_only() {
        let t = TempDir::new("eng1");
        let mut s = session(&t);
        let nb = path_of(s.run("notebook.create", &p(&[("name", "Work"), ("empty", "true")])).expect("nb"));
        let sec = path_of(
            s.run("node.create", &p(&[("parent", nb.to_str().expect("utf8")), ("kind", "section"), ("title", "Projects"), ("format", "cmgfm")]))
                .expect("sec"),
        );
        let page = path_of(s.run("node.create", &p(&[("parent", sec.to_str().expect("utf8")), ("kind", "page"), ("title", "Plan")])).expect("page"));
        assert!(page.join("index.md").is_file(), "the section's format applies");
        let sub = path_of(
            s.run("node.create", &p(&[("parent", page.to_str().expect("utf8")), ("kind", "page"), ("title", "Sub"), ("format", "html")]))
                .expect("sub"),
        );
        assert!(sub.join("index.html").is_file(), "an explicit format wins");
        assert_eq!(s.tree().len(), 1);
        assert_eq!(s.tree()[0].count(), 4);
        s.run("page.save", &p(&[("path", page.to_str().expect("utf8")), ("body", "# Plan\n\nship it\n")])).expect("save");
        match s.run("page.load", &p(&[("path", page.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => assert_eq!(pg.body, "# Plan\n\nship it\n"),
            other => panic!("{other:?}"),
        }
        match s.run("search.query", &p(&[("q", "ship")])).expect("search") {
            Output::Hits(h) => assert_eq!(h.len(), 1),
            other => panic!("{other:?}"),
        }
        s.run("node.title", &p(&[("path", page.to_str().expect("utf8")), ("title", "Plan v2")])).expect("title");
        assert_eq!(s.tree()[0].children[0].children[0].info.title, "Plan v2");
        match s.run("node.delete", &p(&[("path", page.to_str().expect("utf8"))])).expect("delete") {
            Output::Count(n) => assert_eq!(n, 2),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.tree()[0].count(), 2);
    }

    #[test]
    fn relative_paths_resolve_from_the_notebooks_folder() {
        let t = TempDir::new("eng2");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work")])).expect("nb");
        s.run("node.create", &p(&[("parent", "work"), ("kind", "page"), ("title", "P")])).expect("page by relative path");
    }

    #[test]
    fn paths_outside_the_notebooks_folder_are_refused() {
        let t = TempDir::new("eng3");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work")])).expect("nb");
        std::fs::create_dir_all(t.path().join("elsewhere")).expect("dir");
        for bad in [t.path().to_str().expect("utf8"), "..", "../elsewhere", "work/../../elsewhere"] {
            assert!(s.run("node.delete", &p(&[("path", bad)])).is_err(), "{bad}");
            assert!(s.run("node.create", &p(&[("parent", bad), ("kind", "page"), ("title", "x")])).is_err(), "{bad}");
        }
        assert!(t.path().join("elsewhere").exists());
    }

    #[test]
    fn missing_and_bad_parameters_are_errors_not_panics() {
        let t = TempDir::new("eng4");
        let mut s = session(&t);
        assert!(matches!(s.run("notebook.create", &p(&[])), Err(EngineError::MissingParam("name"))));
        assert!(matches!(s.run("no.such.command", &p(&[])), Err(EngineError::UnknownCommand(_))));
        assert!(s.run("notebook.create", &p(&[("name", "x"), ("format", "docx")])).is_err());
        assert!(s.run("search.query", &p(&[("q", "x"), ("limit", "many")])).is_err());
        assert!(s.run("settings.default_format.set", &p(&[("format", "pdf")])).is_err());
    }

    #[test]
    fn settings_commands() {
        let t = TempDir::new("eng5");
        let mut s = session(&t);
        s.run("settings.default_format.set", &p(&[("format", "markdown")])).expect("set");
        match s.run("settings.default_format.get", &p(&[])).expect("get") {
            Output::Text(m) => assert!(m.starts_with("cmgfm"), "{m}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn attaching_a_file_and_bytes() {
        let t = TempDir::new("eng6");
        let mut s = session(&t);
        let nb = path_of(s.run("notebook.create", &p(&[("name", "N")])).expect("nb"));
        let page = path_of(s.run("node.create", &p(&[("parent", nb.to_str().expect("utf8")), ("kind", "page"), ("title", "P")])).expect("page"));
        let src = t.path().join("clip.mp4");
        std::fs::write(&src, b"v").expect("src");
        match s.run("page.attach", &p(&[("path", page.to_str().expect("utf8")), ("source", src.to_str().expect("utf8"))])).expect("attach") {
            Output::Text(link) => assert_eq!(link, "assets/clip.mp4"),
            other => panic!("{other:?}"),
        }
        assert_eq!(s.attach_bytes(page.to_str().expect("utf8"), "pic.png", b"png").expect("bytes"), "assets/pic.png");
    }

    #[test]
    fn check_command_reports() {
        let t = TempDir::new("eng7");
        let mut s = session(&t);
        let nb = path_of(s.run("notebook.create", &p(&[("name", "N")])).expect("nb"));
        match s.run("notebook.check", &p(&[("path", nb.to_str().expect("utf8"))])).expect("check") {
            Output::Report(r) => assert!(r.problems.is_empty() && r.nodes == 3, "the notebook with its first section and page: {r:?}"),
            other => panic!("{other:?}"),
        }
    }

    /// Every command, with junk parameters, must return a result and never panic or
    /// touch anything outside the notebooks folder.
    #[test]
    fn hostile_params_never_panic() {
        let t = TempDir::new("eng8");
        let mut s = session(&t);
        let nb = path_of(s.run("notebook.create", &p(&[("name", "Work")])).expect("nb"));
        let outside = t.path().join("outside.txt");
        std::fs::write(&outside, "keep").expect("outside");
        let long = "x".repeat(10_000);
        let junk: Vec<String> = vec![
            String::new(),
            " ".into(),
            "\u{0}".into(),
            "..".into(),
            "../..".into(),
            "C:\\Windows".into(),
            "/etc".into(),
            "a/b/c".into(),
            "\u{1F600}\u{e9}\u{4e2d}".into(),
            "page".into(),
            "section".into(),
            "html".into(),
            "-1".into(),
            "99999999999999999999".into(),
            nb.to_string_lossy().into_owned(),
            outside.to_string_lossy().into_owned(),
            long,
        ];
        let keys = ["name", "parent", "kind", "title", "format", "path", "body", "source", "q", "limit"];
        for spec in registry() {
            for value in &junk {
                let mut params = Params::new();
                for k in keys {
                    params.insert(k.to_string(), value.clone());
                }
                if spec.id == "node.delete" && value == &nb.to_string_lossy() {
                    continue; // a legitimate delete of the notebook; covered elsewhere
                }
                let r = s.run(spec.id, &params);
                assert!(!matches!(r, Err(EngineError::Panicked(_))), "{} panicked with {value:?}", spec.id);
            }
        }
        assert_eq!(std::fs::read_to_string(&outside).expect("outside"), "keep");
    }
}

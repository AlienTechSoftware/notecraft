//! The command registry. One `CommandSpec` per user-visible action.

use crate::error::EngineError;
use crate::session::Session;
use notecraft_model::{Dir, NodeColor, NodeKind, PageFormat};
use notecraft_store::check::Report;
use notecraft_store::page::Page;
use notecraft_store::page::Version;
use notecraft_store::recycle::Recycled;
use notecraft_store::search::Hit;
use notecraft_store::tags::Tagged;
use notecraft_store::{StoreError, assets, check, ops, page, page_templates, recycle, search, tags, tree};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type Params = BTreeMap<String, String>;

#[derive(Debug)]
pub enum Output {
    None,
    Path(PathBuf),
    Text(String),
    Page(Page),
    Versions(Vec<Version>),
    Recycled(Vec<Recycled>),
    Hits(Vec<Hit>),
    Tagged(Vec<Tagged>),
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
/// "New Section 1" and one empty, unnamed page. The page itself carries no title: this is
/// only the word shown in its place, the same one `tree::UNTITLED_PAGE` falls back to
/// everywhere else a page with nothing typed in it is displayed.
pub const FIRST_SECTION: &str = "New Section 1";

fn notebook_create(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let dir = ops::create_notebook(s.root(), req(p, "name")?, opt_format(p)?, &s.settings)?;
    if p.get("empty").map(String::as_str) != Some("true") {
        let section = ops::create_node(&dir, FIRST_SECTION, NodeKind::Section, None, &s.settings)?;
        ops::create_untitled_page(&section, None, None, &s.settings)?;
    }
    Ok(Output::Path(dir))
}

fn node_create(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let parent = s.resolve(req(p, "parent")?)?;
    let kind = NodeKind::parse(req(p, "kind")?).ok_or_else(|| EngineError::BadParam("kind must be section-group, section or page".into()))?;
    let order = match p.get("order").map(|o| o.trim()).filter(|o| !o.is_empty()) {
        Some(o) => Some(o.parse::<u32>().map_err(|_| EngineError::BadParam("order must be a whole number".into()))?),
        None => None,
    };
    let title = p.get("title").map(|t| t.trim()).filter(|t| !t.is_empty());
    // A section group or section always needs a real title. A page does not: leaving
    // `title` out (or blank) creates it untitled, the "Add Page" case.
    let dir = match title {
        Some(title) => ops::create_node_at(&parent, title, kind, opt_format(p)?, &s.settings, order)?,
        None if kind == NodeKind::Page => ops::create_untitled_page(&parent, opt_format(p)?, order, &s.settings)?,
        None => return Err(EngineError::MissingParam("title")),
    };
    Ok(Output::Path(dir))
}

fn node_demote(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::make_subpage(&s.resolve(req(p, "path")?)?)?))
}

fn node_promote(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::promote_subpage(&s.resolve(req(p, "path")?)?)?))
}

fn node_move(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::move_node(&s.resolve(req(p, "path")?)?, &s.resolve(req(p, "to")?)?)?))
}

fn node_copy(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::copy_node(&s.resolve(req(p, "path")?)?, &s.resolve(req(p, "to")?)?)?))
}

fn page_templates_list(s: &mut Session, _p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Text(page_templates::describe(&page_templates::list(s.root()))))
}

fn page_from_template(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let parent = s.resolve(req(p, "parent")?)?;
    let id = req(p, "template")?;
    let template =
        page_templates::find(s.root(), id).ok_or_else(|| EngineError::BadParam(format!("there is no template {id}; page.templates lists them")))?;
    Ok(Output::Path(page_templates::new_page(&template, &parent, &s.settings)?))
}

fn page_save_template(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let page = s.resolve(req(p, "path")?)?;
    Ok(Output::Path(page_templates::save(s.root(), &page, req(p, "name")?)?))
}

fn page_remove_template(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let id = req(p, "template")?;
    page_templates::remove(s.root(), id)?;
    Ok(Output::Text(format!("removed {id}")))
}

fn node_move_up(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::move_page(&s.resolve(req(p, "path")?)?, true)?))
}

fn node_move_down(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(ops::move_page(&s.resolve(req(p, "path")?)?, false)?))
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
    let mut ancestor = path.as_path();
    let notebook = loop {
        if tree::read_node(ancestor).is_some_and(|node| node.kind == NodeKind::Notebook) {
            break ancestor.to_path_buf();
        }
        ancestor = ancestor.parent().ok_or_else(|| EngineError::BadParam("that node is not inside a notebook".into()))?;
    };
    Ok(Output::Count(recycle::delete(&notebook, &path)?))
}

fn recycle_list(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let notebook = s.resolve(req(p, "notebook")?)?;
    Ok(Output::Recycled(recycle::list(&notebook)?))
}

fn recycle_restore(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let notebook = s.resolve(req(p, "notebook")?)?;
    recycle::restore(&notebook, req(p, "id")?)?;
    Ok(Output::None)
}

fn recycle_purge(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let notebook = s.resolve(req(p, "notebook")?)?;
    recycle::purge(&notebook, req(p, "id")?)?;
    Ok(Output::None)
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

fn page_versions(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Versions(page::versions(&s.resolve(req(p, "path")?)?)?))
}

fn page_version_restore(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    page::restore_version(&path, req(p, "id")?)?;
    Ok(Output::None)
}

fn page_detach(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let name = req(p, "name")?;
    assets::remove(&path, name)?;
    Ok(Output::Text(format!("removed assets/{name}")))
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
    // Every open notebook, the ones opened from another folder too: the window calls this
    // Search all pages (issue #127).
    Ok(Output::Hits(search::search_in(s.root(), s.opened_elsewhere(), req(p, "q")?, limit)))
}

/// The tagged notes in a scope: what Find Tags lists in its Tags Summary (issue #166).
fn tags_find(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let limit = match p.get("limit") {
        Some(l) => l.trim().parse::<usize>().map_err(|_| EngineError::BadParam("limit must be a number".into()))?.min(tags::MAX),
        None => 500,
    };
    let only_page = match p.get("page").map(|v| v.trim()) {
        None | Some("" | "false") => false,
        Some("true") => true,
        Some(_) => return Err(EngineError::BadParam("page must be true or false".into())),
    };
    let unchecked = match p.get("unchecked").map(|v| v.trim()) {
        None | Some("" | "false") => false,
        Some("true") => true,
        Some(_) => return Err(EngineError::BadParam("unchecked must be true or false".into())),
    };
    let want = match p.get("tag").map(|t| t.trim()).filter(|t| !t.is_empty()) {
        Some(t) => Some(notecraft_model::Tag::parse(t).ok_or_else(|| EngineError::BadParam(format!("there is no tag called {t}")))?),
        None => None,
    };
    // Without a scope, every open notebook: the ones opened from another folder too.
    let scopes: Vec<PathBuf> = match p.get("scope").map(|x| x.trim()).filter(|x| !x.is_empty()) {
        Some(scope) => vec![s.resolve(scope)?],
        None => tree::child_dirs(s.root()).into_iter().chain(s.opened_elsewhere().iter().cloned()).collect(),
    };
    let mut found = tags::find(&scopes, only_page, tags::MAX);
    found.retain(|t| want.is_none_or(|w| t.tag == w) && (!unchecked || t.done == Some(false)));
    found.truncate(limit);
    Ok(Output::Tagged(found))
}

fn notebook_check(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    Ok(Output::Report(check::check_tree(&path)))
}

/// Which way a whole page reads. Without `dir`, it reports; with it, it sets.
fn page_direction(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let Some(want) = p.get("dir").map(|d| d.trim().to_ascii_lowercase()) else {
        return Ok(Output::Text(page::direction(&path)?.map(Dir::id).unwrap_or("auto").to_string()));
    };
    let want = match want.as_str() {
        "rtl" => Some(Dir::Rtl),
        "ltr" => Some(Dir::Ltr),
        "auto" | "" | "none" => None,
        _ => return Err(EngineError::BadParam("dir must be rtl, ltr or auto".into())),
    };
    page::set_direction(&path, want)?;
    Ok(Output::Text(want.map(Dir::id).unwrap_or("auto").to_string()))
}

/// The boxes a page's blocks are arranged in. Without `boxes`, it reports them.
fn page_containers(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let Some(want) = p.get("boxes") else {
        return Ok(Output::Text(notecraft_model::container::write(&page::containers(&path)?)));
    };
    let boxes = notecraft_model::container::parse(want);
    if boxes.is_empty() && !want.trim().is_empty() {
        return Err(EngineError::BadParam("boxes is x,y,width,blocks per container, separated by semicolons".into()));
    }
    page::set_containers(&path, &boxes)?;
    Ok(Output::Text(notecraft_model::container::write(&boxes)))
}

fn page_paper(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let (paper, colour) = (p.get("paper"), p.get("color"));
    if paper.is_none() && colour.is_none() {
        let (have_paper, have_colour) = page::appearance(&path)?;
        return Ok(Output::Text(format!("{} {}", have_paper.unwrap_or_else(|| "none".into()), have_colour.unwrap_or_else(|| "none".into()))));
    }
    let (was_paper, was_colour) = page::appearance(&path)?;
    let keep = |want: Option<&String>, was: Option<String>| match want.map(|w| w.trim()) {
        Some("") | Some("none") => None,
        Some(w) => Some(w.to_string()),
        None => was,
    };
    let (paper, colour) = (keep(paper, was_paper), keep(colour, was_colour));
    if let Some(c) = colour.as_deref()
        && notecraft_model::NodeColor::parse(c).is_none()
    {
        return Err(EngineError::BadParam(format!("'{c}' is not one of the page colours")));
    }
    page::set_appearance(&path, paper.as_deref(), colour.as_deref())?;
    Ok(Output::Text(format!("{} {}", paper.unwrap_or_else(|| "none".into()), colour.unwrap_or_else(|| "none".into()))))
}

fn page_columns(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let Some(want) = p.get("widths") else {
        return Ok(Output::Text(notecraft_model::tablecols::write(&page::table_columns(&path)?)));
    };
    let widths = notecraft_model::tablecols::parse(want);
    if widths.is_empty() && !want.trim().is_empty() {
        return Err(EngineError::BadParam(
            "widths is block:w,w,w per table, separated by semicolons, with a ! for a table whose borders are hidden, and block@row.column=l|c|r~colour for aligned and shaded cells".into(),
        ));
    }
    page::set_table_columns(&path, &widths)?;
    Ok(Output::Text(notecraft_model::tablecols::write(&widths)))
}

fn page_justify(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = s.resolve(req(p, "path")?)?;
    let Some(want) = p.get("blocks") else {
        return Ok(Output::Text(notecraft_model::justify::write(&page::justified(&path)?)));
    };
    let marked = notecraft_model::justify::parse(want);
    if marked.is_empty() && !want.trim().is_empty() {
        return Err(EngineError::BadParam(
            "blocks is the paragraphs' positions among the page's blocks, separated by commas, each with an optional :low, :medium or :high".into(),
        ));
    }
    page::set_justified(&path, &marked)?;
    Ok(Output::Text(notecraft_model::justify::write(&marked)))
}

/// Opens a notebook that lives outside the notebooks folder, keeping the open ones.
fn notebook_open(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = std::path::PathBuf::from(req(p, "path")?);
    Ok(Output::Path(s.open_notebook(&path)?))
}

fn open_page(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    Ok(Output::Path(s.page_to_show(req(p, "path")?)?))
}

/// Puts a notebook away again. The folder and its notes are untouched.
fn notebook_close(s: &mut Session, p: &Params) -> Result<Output, EngineError> {
    let path = std::path::PathBuf::from(req(p, "path")?);
    let closed = s.close_notebook(&path);
    Ok(Output::Text(if closed { "closed".into() } else { "that notebook is not open".into() }))
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
        params: "parent (required): path of the notebook, group, section or page to add to\nkind (required): section-group, section or page\ntitle (required for section-group and section; optional for page, which is created untitled if left out)\nformat (optional): html or cmgfm. For a page, its format; for a group or section, the default for pages below",
        changes_tree: true,
        run: node_create,
    },
    CommandSpec {
        id: "node.demote",
        label: "Make a page a subpage of its preceding sibling",
        params: "path (required): the page to make a subpage",
        changes_tree: true,
        run: node_demote,
    },
    CommandSpec {
        id: "node.promote",
        label: "Promote a subpage to its parent's level",
        params: "path (required): the subpage to promote",
        changes_tree: true,
        run: node_promote,
    },
    CommandSpec {
        id: "node.move",
        label: "Move a page, section or section group into another notebook, section group, section or page",
        params: "path (required): the page, section or section group to move
to (required): where it goes; it is put last there",
        changes_tree: true,
        run: node_move,
    },
    CommandSpec {
        id: "node.copy",
        label: "Copy a page, section or section group into another notebook, section group, section or page",
        params: "path (required): the page, section or section group to copy
to (required): where the copy goes; it is put last there, and every node in it gets a new id",
        changes_tree: true,
        run: node_copy,
    },
    CommandSpec {
        id: "page.templates",
        label: "List the page templates, built in and saved: one line each, with the id, the category and the name, separated by tabs",
        params: "",
        changes_tree: false,
        run: page_templates_list,
    },
    CommandSpec {
        id: "page.from_template",
        label: "A new page started from a template",
        params: "parent (required): the section (or page, for a subpage) the new page goes in, last
template (required): the template's id, as page.templates lists it (builtin:meeting-notes, saved:lab-report)",
        changes_tree: true,
        run: page_from_template,
    },
    CommandSpec {
        id: "page.save_template",
        label: "Keep a page as a template",
        params: "path (required): the page; its words, boxes, ink, files, paper and colour are kept, not its subpages
name (required): the template's name",
        changes_tree: false,
        run: page_save_template,
    },
    CommandSpec {
        id: "page.remove_template",
        label: "Take away a page saved as a template",
        params: "template (required): the saved template's id (saved:...). Built-in templates cannot be removed",
        changes_tree: false,
        run: page_remove_template,
    },
    CommandSpec {
        id: "node.move_up",
        label: "Move a page up among its siblings",
        params: "path (required): the page to move up",
        changes_tree: true,
        run: node_move_up,
    },
    CommandSpec {
        id: "node.move_down",
        label: "Move a page down among its siblings",
        params: "path (required): the page to move down",
        changes_tree: true,
        run: node_move_down,
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
        label: "Move a page, section or group into its notebook's recycle bin",
        params: "path (required): the page, section or group to recycle",
        changes_tree: true,
        run: node_delete,
    },
    CommandSpec {
        id: "recycle.list",
        label: "List recycled items in a notebook",
        params: "notebook (required): the notebook whose recycle bin to list",
        changes_tree: false,
        run: recycle_list,
    },
    CommandSpec {
        id: "recycle.restore",
        label: "Restore an item from a notebook's recycle bin",
        params: "notebook (required)\nid (required): an id returned by recycle.list",
        changes_tree: true,
        run: recycle_restore,
    },
    CommandSpec {
        id: "recycle.purge",
        label: "Permanently delete an item from a notebook's recycle bin",
        params: "notebook (required)\nid (required): an id returned by recycle.list",
        changes_tree: true,
        run: recycle_purge,
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
        id: "page.versions",
        label: "List saved page versions",
        params: "path (required): the page",
        changes_tree: false,
        run: page_versions,
    },
    CommandSpec {
        id: "page.version.restore",
        label: "Restore a saved page version",
        params: "path (required): the page\nid (required): the saved version id from page.versions",
        changes_tree: false,
        run: page_version_restore,
    },
    CommandSpec {
        id: "page.direction",
        label: "Which way a page reads",
        params: "path (required): the page
dir (optional): rtl, ltr, or auto to let every line decide; without it the current setting is reported",
        changes_tree: false,
        run: page_direction,
    },
    CommandSpec {
        id: "page.containers",
        label: "How a page's blocks are arranged in boxes",
        params: "path (required): the page
boxes (optional): x,y,width,blocks per container, separated by semicolons. The counts must add up to the page's blocks. They are written into the page's text as elements around the blocks; empty puts the page back in one column. Without it, the current arrangement is reported",
        changes_tree: false,
        run: page_containers,
    },
    CommandSpec {
        id: "page.paper",
        label: "The paper a page is drawn on, and the colour behind it",
        params: "path (required): the page
paper (optional): none, narrow, college, standard, wide, grid-small, grid-medium, grid-large, grid-huge, craft-lines, craft-grid
color (optional): purple, blue, teal, green, yellow, orange, red, magenta, gray, or none. Without either, the page's own are reported",
        changes_tree: false,
        run: page_paper,
    },
    CommandSpec {
        id: "page.columns",
        label: "How wide the columns of a page's tables are, which tables have their borders hidden, and how their cells are aligned and shaded",
        params: "path (required): the page
widths (optional): block:w,w,w per table, separated by semicolons, with a ! after the widths (or on its own, as in 3:!) for a table whose borders are hidden, and block@row.column=a,row.column=a for cells aligned l(eft), c(enter) or r(ight) and shaded with ~ and a colour (purple, blue, teal, green, yellow, orange, red, magenta or gray), as in 2@1.0=c~yellow, with the heading as row 0; empty puts every table back to the usual width, borders, alignment and shading. Without it, the current layout is reported",
        changes_tree: false,
        run: page_columns,
    },
    CommandSpec {
        id: "page.justify",
        label: "Which paragraphs of a page are stretched to fill the column",
        params: "path (required): the page
blocks (optional): the justified paragraphs' positions among the page's blocks, separated by commas, each with an optional strength after a colon (low, medium or high; a number on its own is medium), as in '4, 11:high'; empty draws every paragraph ragged. Without it, the page's own are reported",
        changes_tree: false,
        run: page_justify,
    },
    CommandSpec {
        id: "page.attach",
        label: "Attach a file to a page",
        params: "path (required): the page\nsource (required): a file to copy into the page's assets folder",
        changes_tree: false,
        run: page_attach,
    },
    CommandSpec {
        id: "page.detach",
        label: "Take a file out of a page's folder",
        params: "path (required): the page\nname (required): the file in the page's assets folder, as page.load reports it. The file is deleted; a line that linked to it is left alone",
        changes_tree: false,
        run: page_detach,
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
        id: "tags.find",
        label: "Find tagged notes",
        params: "scope (optional): a page, section, section group or notebook to look in, relative to the notebooks folder; every open notebook when left out\npage (optional): true to look in the scope page alone, without its subpages\ntag (optional): only this tag, by its id (todo, important, question and the rest)\nunchecked (optional): true for only the To Do items not yet ticked\nlimit (optional): at most this many lines (default 500, at most 2000)",
        changes_tree: false,
        run: tags_find,
    },
    CommandSpec {
        id: "notebook.open",
        label: "Open a notebook or initialize an empty folder",
        params: "path (required): a notebook folder anywhere on this machine, or an empty folder to initialize as a notebook named after the folder. The notebooks already open stay open, and this one is opened again next time",
        changes_tree: true,
        run: notebook_open,
    },
    CommandSpec {
        id: "open.page",
        label: "Find the page to show for a notebook, section group, section or page",
        params: "path (required): relative to the notebooks folder, or a full path. A page is itself; anything else gives its first page. A notebook kept somewhere else is opened first, and one that was put away is opened again",
        changes_tree: true,
        run: open_page,
    },
    CommandSpec {
        id: "notebook.close",
        label: "Put away a notebook without deleting it",
        params: "path (required): the notebook's folder. Nothing is deleted",
        changes_tree: true,
        run: notebook_close,
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

    fn text_of(out: Output) -> String {
        match out {
            Output::Text(t) => t,
            other => panic!("expected text, got {other:?}"),
        }
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

    #[test]
    fn notebook_installation_is_not_an_app_command() {
        assert!(!registry().iter().any(|command| command.id == "notebook.welcome"));
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
        assert_eq!(page.info.title, tree::UNTITLED_PAGE);
        assert_eq!(page.info.kind, notecraft_model::NodeKind::Page);
        assert!(page.children.is_empty());
        assert_eq!(tree.first().map(notecraft_store::Node::count), Some(3), "the notebook, the section and the page");
        // The page is real: its title and the date it was started, like a new OneNote page.
        match s.run("page.load", &p(&[("path", page.path.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => {
                // No title key was ever written: `has_title` says so, and the fallback
                // display word is not mistaken for a real title.
                assert!(!pg.has_title, "a page nobody has named yet carries no title key");
                assert_eq!(pg.title, "Untitled page");
                assert!(!pg.body.contains("<h1>"), "the body does not say the title again: {}", pg.body);
                assert!(pg.body.contains("page-date"), "a new page carries its date: {}", pg.body);
            }
            other => panic!("{other:?}"),
        }
        // The page follows the notebook's format.
        let md = path_of(s.run("notebook.create", &p(&[("name", "Notes"), ("format", "cmgfm")])).expect("nb"));
        assert!(
            md.join(notecraft_model::names::slug(FIRST_SECTION)).join(notecraft_model::names::slug(tree::UNTITLED_PAGE)).join("index.md").is_file()
        );
        // `empty=true` is the way to skip it.
        let bare = path_of(s.run("notebook.create", &p(&[("name", "Bare"), ("empty", "true")])).expect("nb"));
        assert_eq!(std::fs::read_dir(&bare).into_iter().flatten().flatten().filter(|e| e.path().is_dir()).count(), 0);
    }

    /// Issue #166: Find Tags lists the tagged lines of a page, a section or every notebook, and
    /// can keep only one tag or only the To Do items not yet ticked.
    #[test]
    fn find_tags_lists_tagged_lines_by_scope_and_filter() {
        let t = TempDir::new("engine-find-tags");
        let mut s = session(&t);
        let nb = path_of(s.run("notebook.create", &p(&[("name", "Work")])).expect("notebook"));
        let section = nb.join("new-section-1");
        let first = section.join("untitled-page");
        let body = "<p>#important Book the room</p><ul><li>plain</li></ul>";
        s.run("page.save", &p(&[("path", &first.to_string_lossy()), ("body", body)])).expect("save");
        let md = path_of(
            s.run("node.create", &p(&[("parent", &section.to_string_lossy()), ("kind", "page"), ("title", "Minutes"), ("format", "cmgfm")]))
                .expect("page"),
        );
        s.run("page.save", &p(&[("path", &md.to_string_lossy()), ("body", "- [x] Write minutes\n- [ ] Fix the build\n\n#question Who owns it?\n")]))
            .expect("save");
        let found = |s: &mut Session, extra: &[(&str, &str)]| match s.run("tags.find", &p(extra)).expect("tags.find") {
            Output::Tagged(found) => found.into_iter().map(|t| (t.tag.id(), t.text, t.done)).collect::<Vec<_>>(),
            other => panic!("{other:?}"),
        };
        let all = found(&mut s, &[]);
        assert_eq!(all.len(), 4, "{all:?}");
        assert_eq!(found(&mut s, &[("scope", &first.to_string_lossy()), ("page", "true")]), vec![("important", "Book the room".to_string(), None)]);
        assert_eq!(found(&mut s, &[("tag", "question")]), vec![("question", "Who owns it?".to_string(), None)]);
        assert_eq!(found(&mut s, &[("unchecked", "true")]), vec![("todo", "Fix the build".to_string(), Some(false))]);
        assert_eq!(found(&mut s, &[("limit", "1")]).len(), 1);
        assert!(s.run("tags.find", &p(&[("tag", "nonsense")])).is_err());
        assert!(s.run("tags.find", &p(&[("unchecked", "maybe")])).is_err());
        assert!(s.run("tags.find", &p(&[("limit", "many")])).is_err());
        let json = crate::json::output_json(&s.run("tags.find", &p(&[("tag", "todo")])).expect("json"));
        assert_eq!(json[0]["label"], "To Do");
        assert_eq!(json[0]["done"], true);
        assert_eq!(json[0]["page"], "Minutes");
        assert_eq!(json[0]["block"], 0);
    }

    /// Issue #127: Search all pages finds pages in a notebook opened from another folder, once
    /// each, with the limit applied to every notebook together.
    #[test]
    fn search_finds_pages_in_a_notebook_opened_from_elsewhere() {
        let t = TempDir::new("engine-search-elsewhere");
        let settings = Settings::at(t.path().join("settings").join("s.toml"));
        let mut s = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let here = path_of(s.run("notebook.create", &p(&[("name", "Here")])).expect("notebook"));
        let here_page = here.join("new-section-1").join("untitled-page");
        s.run("page.save", &p(&[("path", &here_page.to_string_lossy()), ("body", "<p>shared_token here</p>")])).expect("save");

        let away = t.path().join("away");
        std::fs::create_dir_all(&away).expect("dir");
        let mut other = Session::new(&away, settings.clone()).expect("other session");
        let made = path_of(other.run("notebook.create", &p(&[("name", "Elsewhere")])).expect("notebook"));
        let away_page = made.join("new-section-1").join("untitled-page");
        other
            .run("page.save", &p(&[("path", &away_page.to_string_lossy()), ("body", "<p>external_unique_token and shared_token</p>")]))
            .expect("save");
        s.run("notebook.open", &p(&[("path", &made.to_string_lossy())])).expect("open");

        let hits = |s: &mut Session, q: &str, limit: &str| match s.run("search.query", &p(&[("q", q), ("limit", limit)])).expect("search") {
            Output::Hits(h) => h,
            other => panic!("{other:?}"),
        };
        let found = hits(&mut s, "external_unique_token", "10");
        assert_eq!(found.len(), 1, "the page in the notebook opened from elsewhere is found: {found:?}");
        assert_eq!(hits(&mut s, "shared_token", "10").len(), 2, "a page in each notebook, each once");
        assert_eq!(hits(&mut s, "shared_token", "1").len(), 1, "the limit is for all the notebooks together");
    }

    #[test]
    fn a_notebook_from_elsewhere_opens_beside_the_others_and_stays_open() {
        let t = TempDir::new("engine-open-elsewhere");
        let settings = Settings::at(t.path().join("settings").join("s.toml"));
        let mut s = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        s.run("notebook.create", &p(&[("name", "Here")])).expect("notebook");

        // A notebook in a folder of its own, nowhere near the notebooks folder.
        let away = t.path().join("away");
        std::fs::create_dir_all(&away).expect("dir");
        let mut other = Session::new(&away, settings.clone()).expect("other session");
        let made = path_of(other.run("notebook.create", &p(&[("name", "Elsewhere")])).expect("notebook"));

        s.run("notebook.open", &p(&[("path", &made.to_string_lossy())])).expect("open");
        let titles: Vec<String> = s.tree().iter().map(|n| n.info.title.clone()).collect();
        assert_eq!(titles, vec!["Here".to_string(), "Elsewhere".to_string()], "both, the opened one last");
        // Its pages can be reached, which means paths inside it resolve.
        // Built with `join`, not with slashes: a Windows verbatim path (`\?\C:\...`) does
        // not accept forward slashes, and the canonical paths here are verbatim.
        let page = made.join("new-section-1").join("untitled-page");
        let loaded = s.run("page.load", &p(&[("path", &page.to_string_lossy())]));
        assert!(loaded.is_ok(), "a page inside the opened notebook: {loaded:?}");

        // It is remembered: a new session on the same settings has it too.
        let again = Session::new(&t.path().join("notebooks"), settings.clone()).expect("again");
        assert_eq!(again.tree().len(), 2, "still open next time");
        assert_eq!(again.opened_elsewhere().len(), 1);

        // Closing puts it away without touching the folder.
        s.run("notebook.close", &p(&[("path", &made.to_string_lossy())])).expect("close");
        assert_eq!(s.tree().len(), 1, "back to the one in the notebooks folder");
        assert!(made.is_dir(), "the folder is still there");
        // What is not a notebook is refused, and so is nothing at all.
        assert!(s.run("notebook.open", &p(&[("path", &t.path().join("away").to_string_lossy())])).is_err());
        assert!(s.run("notebook.open", &p(&[("path", "no/such/folder")])).is_err());
    }

    #[test]
    fn open_page_finds_the_page_for_a_notebook_a_section_or_a_page() {
        let t = TempDir::new("engine-open-page");
        let settings = Settings::at(t.path().join("settings").join("s.toml"));
        let root = t.path().join("notebooks");
        let mut s = Session::new(&root, settings.clone()).expect("session");
        let here = path_of(s.run("notebook.create", &p(&[("name", "Here")])).expect("notebook"));
        let section = here.join("new-section-1");
        let page = section.join("untitled-page");
        let open = |s: &mut Session, path: &str| s.run("open.page", &p(&[("path", path)])).map(path_of);

        // A notebook and a section give their first page; a page is itself. Relative or full.
        assert_eq!(open(&mut s, "here").expect("notebook"), page);
        assert_eq!(open(&mut s, "here/new-section-1").expect("section"), page);
        assert_eq!(open(&mut s, &section.to_string_lossy()).expect("full section path"), page);
        assert_eq!(open(&mut s, &page.to_string_lossy()).expect("page"), page);

        // A notebook that was put away is opened again.
        s.run("notebook.close", &p(&[("path", &here.to_string_lossy())])).expect("close");
        assert!(s.tree().is_empty());
        assert_eq!(open(&mut s, "here").expect("closed notebook"), page);
        assert_eq!(s.tree().len(), 1, "opened again");

        // A notebook kept somewhere else is opened beside the others, even from one of its pages.
        let away = t.path().join("away");
        let made = path_of(Session::new(&away, settings.clone()).expect("other").run("notebook.create", &p(&[("name", "Elsewhere")])).expect("nb"));
        let away_page = made.join("new-section-1").join("untitled-page");
        assert_eq!(open(&mut s, &away_page.to_string_lossy()).expect("page elsewhere"), away_page);
        assert_eq!(s.opened_elsewhere(), std::slice::from_ref(&made));
        assert_eq!(s.tree().len(), 2);

        // Nothing to show is an error, not an empty window.
        let bare = path_of(s.run("notebook.create", &p(&[("name", "Bare"), ("empty", "true")])).expect("bare"));
        let e = open(&mut s, &bare.to_string_lossy()).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(e.contains("no page"), "{e}");
        assert!(open(&mut s, "no/such/page").is_err());
        // A folder that is not part of any notebook is refused rather than turned into one.
        std::fs::create_dir_all(t.path().join("loose")).expect("dir");
        assert!(open(&mut s, &t.path().join("loose").to_string_lossy()).is_err());
        assert!(!t.path().join("loose").join("_node.toml").exists());
    }

    #[test]
    fn a_notebook_in_the_notebooks_folder_can_be_closed_and_reopened() {
        let t = TempDir::new("engine-close-root-notebook");
        let settings = Settings::at(t.path().join("settings").join("s.toml"));
        let mut s = Session::new(&t.path().join("notebooks"), settings.clone()).expect("session");
        let made = path_of(s.run("notebook.create", &p(&[("name", "Testing")])).expect("notebook"));
        let metadata = made.join("_node.toml");
        let original = std::fs::read(&metadata).expect("notebook metadata exists");

        assert!(
            matches!(s.run("notebook.close", &p(&[("path", &made.to_string_lossy())])).expect("close"), Output::Text(result) if result == "closed")
        );
        assert!(s.tree().is_empty(), "the notebook is put away from the tree");
        assert_eq!(std::fs::read(&metadata).expect("notebook remains on disk"), original, "its files are untouched");

        let mut again = Session::new(&t.path().join("notebooks"), settings).expect("reopen session");
        assert!(again.tree().is_empty(), "the closed notebook stays put away after restart");
        again.run("notebook.open", &p(&[("path", &made.to_string_lossy())])).expect("reopen notebook");
        assert_eq!(again.tree().first().map(|n| n.info.title.as_str()), Some("Testing"));
    }

    #[test]
    fn opening_an_empty_folder_initializes_a_notebook_but_leaves_other_folders_alone() {
        let t = TempDir::new("engine-open-empty-folder");
        let mut s = session(&t);
        let empty = t.path().join("Testing");
        std::fs::create_dir(&empty).expect("empty folder");

        let opened = path_of(s.run("notebook.open", &p(&[("path", &empty.to_string_lossy())])).expect("open empty folder"));
        assert_eq!(opened, empty.canonicalize().expect("canonical path"));
        assert_eq!(s.tree().first().map(|n| n.info.title.as_str()), Some("Testing"));
        assert!(opened.join("_node.toml").is_file(), "the chosen folder is now a notebook");
        assert_eq!(std::fs::read_dir(&opened).expect("read notebook").count(), 1, "it remains empty apart from notebook metadata");

        let ordinary = t.path().join("Ordinary");
        std::fs::create_dir(&ordinary).expect("ordinary folder");
        let keep = ordinary.join("keep.txt");
        std::fs::write(&keep, "leave this file alone").expect("write user file");
        assert!(s.run("notebook.open", &p(&[("path", &ordinary.to_string_lossy())])).is_err());
        assert_eq!(std::fs::read_to_string(&keep).expect("preserved file"), "leave this file alone");
        assert!(!ordinary.join("_node.toml").exists(), "an ordinary nonempty folder stays untouched");
    }

    #[test]
    fn a_page_can_be_slipped_between_two_others() {
        let t = TempDir::new("engine-order");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work")])).expect("notebook");
        let section = "work/new-section-1";
        for title in ["First", "Second"] {
            s.run("node.create", &p(&[("parent", section), ("kind", "page"), ("title", title)])).expect("page");
        }
        // The section already has the page a new notebook starts with, so the three pages
        // are numbered 10, 20 and 30: 25 slips one between the second and the third.
        s.run("node.create", &p(&[("parent", section), ("kind", "page"), ("title", "Middle"), ("order", "25")])).expect("between");
        let tree = text_of(s.run("tree.read", &p(&[("path", section)])).expect("tree"));
        let titles: Vec<&str> = tree.lines().filter_map(|l| l.trim().split("  [").next()).skip(1).collect();
        assert_eq!(
            titles,
            vec!["Untitled page", "First", "Middle", "Second"],
            "the new page sits where it was asked for:
{tree}"
        );
        assert!(s.run("node.create", &p(&[("parent", section), ("kind", "page"), ("title", "Bad"), ("order", "soon")])).is_err());
    }

    /// `node.create` for a page with no `title` at all, or a blank one, is the "Add Page"
    /// case: it must not be confused with a real title that happens to say "Untitled page".
    #[test]
    fn a_page_added_with_no_title_is_untitled() {
        let t = TempDir::new("engine-untitled");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work"), ("empty", "true"), ("format", "cmgfm")])).expect("notebook");
        let sec = path_of(s.run("node.create", &p(&[("parent", "work"), ("kind", "section"), ("title", "Notes")])).expect("section"));
        let sec = sec.to_str().expect("utf8");
        let page = path_of(s.run("node.create", &p(&[("parent", sec), ("kind", "page"), ("order", "40")])).expect("page"));
        match s.run("page.load", &p(&[("path", page.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => {
                assert!(!pg.has_title, "nothing was typed for it, so it carries no title key");
                assert_eq!(pg.title, "Untitled page");
            }
            other => panic!("{other:?}"),
        }
        let info = notecraft_store::tree::read_node(&page).expect("node info");
        assert_eq!(info.title, "Untitled page", "the tree shows the same fallback");
        assert_eq!(info.order, 40, "an untitled page still takes the order it was asked for");
        let text = std::fs::read_to_string(page.join("index.md")).expect("page file");
        assert!(!text.contains("title:"), "no title key was written at all:\n{text}");

        // A blank title is the same as no title at all.
        let blank = path_of(s.run("node.create", &p(&[("parent", sec), ("kind", "page"), ("title", "   ")])).expect("page"));
        match s.run("page.load", &p(&[("path", blank.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => assert!(!pg.has_title, "a blank title is treated as none"),
            other => panic!("{other:?}"),
        }

        // A section or section group still needs a real title.
        assert!(matches!(s.run("node.create", &p(&[("parent", "work"), ("kind", "section")])), Err(EngineError::MissingParam("title"))));

        // The page stops being untitled the moment it is given a real title, the same way
        // typing a name into the title area would.
        s.run("node.title", &p(&[("path", page.to_str().expect("utf8")), ("title", "Shopping List")])).expect("rename");
        match s.run("page.load", &p(&[("path", page.to_str().expect("utf8"))])).expect("load") {
            Output::Page(pg) => {
                assert!(pg.has_title, "a title was just given to it");
                assert_eq!(pg.title, "Shopping List");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_page_remembers_how_its_boxes_are_arranged() {
        let t = TempDir::new("engine-boxes");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work"), ("format", "cmgfm")])).expect("notebook");
        let path = "work/new-section-1/untitled-page";
        let page = p(&[("path", path)]);
        let save = |s: &mut Session, body: &str| {
            s.run("page.save", &p(&[("path", path), ("body", body)])).expect("save");
        };
        save(&mut s, "one\n\ntwo\n\nthree\n");
        assert_eq!(text_of(s.run("page.containers", &page).expect("read")), "", "a page nobody arranged is one column");

        let set = p(&[("path", path), ("boxes", "0,0,420,2; 460,24,300,1")]);
        assert_eq!(text_of(s.run("page.containers", &set).expect("set")), "0,0,420,2; 460,24,300,1");
        assert_eq!(text_of(s.run("page.containers", &page).expect("read")), "0,0,420,2; 460,24,300,1");

        // The boxes are elements in the page's own text, so what the window shows is what the
        // file says, and the file opens as boxes anywhere that reads it.
        let file = s.resolve(path).expect("path").join("index.md");
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        assert_eq!(text.matches("class=\"nc-box\"").count(), 2, "one element per box:\n{text}");
        assert!(text.contains("data-x=\"460\" data-y=\"24\" data-width=\"300\""), "with its place:\n{text}");
        assert!(!text.contains("containers:"), "and not on a line in the front matter:\n{text}");

        // A body written with the boxes in it keeps them. A plain body is one column.
        let body = match s.run("page.load", &page).expect("load") {
            Output::Page(p) => p.body,
            other => panic!("not a page: {other:?}"),
        };
        assert!(body.contains("nc-box"), "the page's body carries them:\n{body}");
        save(&mut s, &body);
        assert_eq!(text_of(s.run("page.containers", &page).expect("read")), "0,0,420,2; 460,24,300,1", "saving the body as it is keeps them");
        save(&mut s, "one\n\ntwo\n\nthree\n");
        assert_eq!(text_of(s.run("page.containers", &page).expect("read")), "", "a plain body is one column");

        // Counts that do not match the blocks are refused, and so is a box line that is not one.
        assert!(s.run("page.containers", &p(&[("path", path), ("boxes", "0,0,420,5")])).is_err());
        assert!(s.run("page.containers", &p(&[("path", path), ("boxes", "sideways")])).is_err());

        // An empty list puts an arranged page back in one column.
        s.run("page.containers", &set).expect("set again");
        assert_eq!(text_of(s.run("page.containers", &p(&[("path", path), ("boxes", "")])).expect("clear")), "");
        assert!(!std::fs::read_to_string(&file).unwrap_or_default().contains("nc-box"));
    }

    #[test]
    fn a_page_can_be_told_which_way_it_reads() {
        let t = TempDir::new("engine-dir");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work"), ("format", "cmgfm")])).expect("notebook");
        let page = p(&[("path", "work/new-section-1/untitled-page")]);
        assert_eq!(text_of(s.run("page.direction", &page).expect("read")), "auto", "nobody has said yet");

        let set = p(&[("path", "work/new-section-1/untitled-page"), ("dir", "rtl")]);
        s.run("page.direction", &set).expect("set");
        assert_eq!(text_of(s.run("page.direction", &page).expect("read")), "rtl");
        // It is in the file, so it travels with the folder.
        let file = s.resolve("work/new-section-1/untitled-page").expect("path").join("index.md");
        let text = std::fs::read_to_string(&file).expect("read");
        assert!(text.contains("dir: rtl"), "{text}");

        // Saving the page keeps it, and auto takes it out again.
        s.run("page.save", &p(&[("path", "work/new-section-1/untitled-page"), ("body", "مرحبا")])).expect("save");
        assert_eq!(text_of(s.run("page.direction", &page).expect("read")), "rtl", "a save must not lose it");
        s.run("page.direction", &p(&[("path", "work/new-section-1/untitled-page"), ("dir", "auto")])).expect("auto");
        assert_eq!(text_of(s.run("page.direction", &page).expect("read")), "auto");
        assert!(s.run("page.direction", &p(&[("path", "work/new-section-1/untitled-page"), ("dir", "sideways")])).is_err());
    }

    /// Justification is told to the page by the paragraph's place among its blocks, and the
    /// page's words are not touched by it.
    #[test]
    fn a_page_can_be_told_which_paragraphs_are_justified() {
        let t = TempDir::new("engine-justify");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work"), ("format", "cmgfm")])).expect("notebook");
        let path = "work/new-section-1/untitled-page";
        let page = p(&[("path", path)]);
        assert_eq!(text_of(s.run("page.justify", &page).expect("read")), "", "nothing is justified until it is asked for");

        let body = "واحد\n\nاثنان\n\nثلاثة\n";
        s.run("page.save", &p(&[("path", path), ("body", body)])).expect("save");
        s.run("page.justify", &p(&[("path", path), ("blocks", "1")])).expect("set");
        assert_eq!(text_of(s.run("page.justify", &page).expect("read")), "1");

        // A strength rides with the block number, and a number on its own is the middle one.
        s.run("page.justify", &p(&[("path", path), ("blocks", "1:high, 2")])).expect("set");
        assert_eq!(text_of(s.run("page.justify", &page).expect("read")), "1:high, 2");
        s.run("page.justify", &p(&[("path", path), ("blocks", "1")])).expect("set");

        // It is in the file, so it travels with the folder, and the words are as they were.
        let file = s.resolve(path).expect("path").join("index.md");
        assert!(std::fs::read_to_string(&file).expect("read").contains("justify: 1"));
        match s.run("page.load", &page).expect("load") {
            Output::Page(loaded) => assert_eq!(loaded.body.trim_end(), body.trim_end(), "no tatweel is written into the page"),
            other => panic!("expected a page, got {other:?}"),
        }

        // Saving the page keeps it; an empty list draws every paragraph ragged again.
        s.run("page.save", &p(&[("path", path), ("body", body)])).expect("save");
        assert_eq!(text_of(s.run("page.justify", &page).expect("read")), "1", "a save must not lose it");
        assert_eq!(text_of(s.run("page.justify", &p(&[("path", path), ("blocks", "")])).expect("clear")), "");
        assert!(s.run("page.justify", &p(&[("path", path), ("blocks", "sideways")])).is_err());
    }

    /// An HTML page keeps it too, in a meta tag.
    #[test]
    fn an_html_page_keeps_its_direction_in_a_meta_tag() {
        let t = TempDir::new("engine-dir-html");
        let mut s = session(&t);
        s.run("notebook.create", &p(&[("name", "Work"), ("format", "html")])).expect("notebook");
        let path = "work/new-section-1/untitled-page";
        s.run("page.direction", &p(&[("path", path), ("dir", "rtl")])).expect("set");
        s.run("page.save", &p(&[("path", path), ("body", "<p>مرحبا</p>")])).expect("save");
        assert_eq!(text_of(s.run("page.direction", &p(&[("path", path)])).expect("read")), "rtl");
        let loaded = s.run("page.load", &p(&[("path", path)])).expect("load");
        match loaded {
            Output::Page(page) => assert_eq!(page.dir, Some(Dir::Rtl)),
            other => panic!("expected a page, got {other:?}"),
        }
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

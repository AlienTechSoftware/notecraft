//! Page templates (issue #173): pages to start from, as OneNote's Insert > Page Templates offers.
//!
//! Two kinds:
//!
//! - **Built in**: a few starting pages kept here as Markdown, with the paper they are drawn on.
//!   A new page from one is an ordinary new page (its own id, order and date) whose words are the
//!   template's, written in whichever format the section uses.
//! - **Saved**: a page someone kept as a template. It is a copy of that page's folder (its words,
//!   boxes, ink, pictures and other files, its paper and its colour; not its subpages) under
//!   `.notecraft/templates/` in the notebooks folder. A dot folder is not a notebook, so the tree
//!   never shows it. A new page from one is a copy of that folder with ids of its own, the way
//!   Copy makes one.
//!
//! Either way the new page goes last in the section and has no title yet, as Add Page does: the
//! template gives the page its contents, not its name. Nothing is ever written over a page that
//! is already there.

use crate::error::{Result, StoreError};
use crate::ops;
use crate::page::{set_appearance, write_atomic};
use crate::settings::Settings;
use crate::tree::{self, page_file, read_node};
use notecraft_formats::{frontmatter, html, markdown, render};
use notecraft_model::{NodeKind, PageFormat, kv, names, time::iso_now};
use std::fs;
use std::path::{Path, PathBuf};

/// A starting page that ships with NoteCraft.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BuiltIn {
    /// What `template` names it by: `builtin:<id>`.
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    /// The paper it is drawn on (see `page.paper`), if it has one.
    pub paper: Option<&'static str>,
    /// Its words, as Markdown. Empty for a template that is only paper.
    pub body: &'static str,
}

/// The groups OneNote's gallery sorts its templates into, and one for saved ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Academic,
    Blank,
    Business,
    Planners,
    /// Pages saved as templates.
    Mine,
}

impl Category {
    pub const ALL: [Category; 5] = [Category::Academic, Category::Blank, Category::Business, Category::Planners, Category::Mine];

    pub fn id(self) -> &'static str {
        match self {
            Category::Academic => "academic",
            Category::Blank => "blank",
            Category::Business => "business",
            Category::Planners => "planners",
            Category::Mine => "mine",
        }
    }

    /// The English name, which the window translates.
    pub fn label(self) -> &'static str {
        match self {
            Category::Academic => "Academic",
            Category::Blank => "Blank",
            Category::Business => "Business",
            Category::Planners => "Planners",
            Category::Mine => "My Templates",
        }
    }
}

pub const BUILT_IN: &[BuiltIn] = &[
    BuiltIn {
        id: "lecture-notes",
        name: "Lecture Notes",
        category: Category::Academic,
        paper: Some("college"),
        body: "## Course and lecture\n\n## Key ideas\n\n- \n\n## Notes\n\n## Questions to follow up\n\n- [ ] \n\n## Summary\n",
    },
    BuiltIn {
        id: "study-guide",
        name: "Study Guide",
        category: Category::Academic,
        paper: None,
        body: "## Topic\n\n## Terms to know\n\n| Term | Meaning |\n| --- | --- |\n|  |  |\n\n## Main points\n\n1. \n\n## Practice\n\n- [ ] \n",
    },
    BuiltIn { id: "college-ruled", name: "College Ruled", category: Category::Blank, paper: Some("college"), body: "" },
    BuiltIn { id: "grid", name: "Grid", category: Category::Blank, paper: Some("grid-medium"), body: "" },
    BuiltIn {
        id: "meeting-notes",
        name: "Meeting Notes",
        category: Category::Business,
        paper: None,
        body: "## Attendees\n\n- \n\n## Agenda\n\n1. \n\n## Notes\n\n## Decisions\n\n- \n\n## Action items\n\n- [ ] \n",
    },
    BuiltIn {
        id: "project-overview",
        name: "Project Overview",
        category: Category::Business,
        paper: None,
        body: "## Goal\n\n## Scope\n\n## Milestones\n\n| Milestone | Owner | Due |\n| --- | --- | --- |\n|  |  |  |\n\n## Risks\n\n- \n\n## Next steps\n\n- [ ] \n",
    },
    BuiltIn {
        id: "to-do-list",
        name: "To Do List",
        category: Category::Planners,
        paper: None,
        body: "## Today\n\n- [ ] \n\n## This week\n\n- [ ] \n\n## Later\n\n- [ ] \n",
    },
    BuiltIn {
        id: "weekly-planner",
        name: "Weekly Planner",
        category: Category::Planners,
        paper: None,
        body: "## Monday\n\n- [ ] \n\n## Tuesday\n\n- [ ] \n\n## Wednesday\n\n- [ ] \n\n## Thursday\n\n- [ ] \n\n## Friday\n\n- [ ] \n\n## Weekend\n\n- [ ] \n",
    },
];

/// A template that can be picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    /// `builtin:<id>` or `saved:<folder>`: what `new_page` and the engine take.
    pub id: String,
    pub name: String,
    pub category: Category,
    pub source: Source,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    BuiltIn(BuiltIn),
    /// The template's folder.
    Saved(PathBuf),
}

/// Where saved templates are kept: `.notecraft/templates` in the notebooks folder.
pub fn folder(root: &Path) -> PathBuf {
    root.join(".notecraft").join("templates")
}

/// Every template: the built-in ones, then the saved ones by name.
pub fn list(root: &Path) -> Vec<Template> {
    let mut all: Vec<Template> = BUILT_IN
        .iter()
        .map(|b| Template { id: format!("builtin:{}", b.id), name: b.name.to_string(), category: b.category, source: Source::BuiltIn(*b) })
        .collect();
    let mut saved: Vec<Template> = tree::child_dirs(&folder(root))
        .into_iter()
        .filter_map(|dir| {
            let info = read_node(&dir).filter(|n| n.kind == NodeKind::Page)?;
            let name = dir.file_name()?.to_string_lossy().into_owned();
            let title = if info.title.trim().is_empty() || info.title == tree::UNTITLED_PAGE { name.clone() } else { info.title.clone() };
            Some(Template { id: format!("saved:{name}"), name: title, category: Category::Mine, source: Source::Saved(dir) })
        })
        .collect();
    saved.sort_by_key(|t| t.name.to_lowercase());
    all.extend(saved);
    all
}

/// The template `id` names, if there is one.
pub fn find(root: &Path, id: &str) -> Option<Template> {
    list(root).into_iter().find(|t| t.id == id.trim())
}

/// Keeps the page at `page` as a template called `name`: a copy of its folder, without its
/// subpages. Returns the template's folder. Saving under a name already taken keeps both.
pub fn save(root: &Path, page: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::invalid("a template needs a name"));
    }
    if read_node(page).is_none_or(|n| n.kind != NodeKind::Page) {
        return Err(StoreError::invalid(format!("{} is not a page", page.display())));
    }
    let dir = folder(root);
    fs::create_dir_all(&dir).map_err(|e| StoreError::io("create", &dir, e))?;
    let target = ops::unique_child(&dir, &names::slug(name));
    if let Err(e) = copy_page_only(page, &target) {
        let _ = fs::remove_dir_all(&target);
        return Err(StoreError::io("copy", &target, e));
    }
    if let Err(e) = ops::set_title(&target, name) {
        let _ = fs::remove_dir_all(&target);
        return Err(e);
    }
    // Naming it kept a version of it, as any change to a page does; a template has no history.
    drop_history(&target);
    Ok(target)
}

/// Takes a saved template away. Built-in ones cannot be.
pub fn remove(root: &Path, id: &str) -> Result<()> {
    match find(root, id).map(|t| t.source) {
        Some(Source::Saved(dir)) => ops::remove_tree(&dir).map_err(|e| StoreError::io("remove", &dir, e)),
        Some(Source::BuiltIn(_)) => Err(StoreError::invalid("a built-in template cannot be removed")),
        None => Err(StoreError::NotFound(format!("there is no template {id}"))),
    }
}

/// A new page in `parent` (a section, or a page for a subpage), last there, started from the
/// template. Returns the new page's folder.
pub fn new_page(template: &Template, parent: &Path, settings: &Settings) -> Result<PathBuf> {
    match &template.source {
        Source::BuiltIn(b) => {
            let page = ops::create_untitled_page(parent, None, None, settings)?;
            let filled = (|| -> Result<()> {
                if !b.body.is_empty() {
                    fill(&page, b.body)?;
                }
                if b.paper.is_some() {
                    set_appearance(&page, b.paper, None)?;
                }
                Ok(())
            })();
            if let Err(e) = filled {
                let _ = fs::remove_dir_all(&page);
                return Err(e);
            }
            Ok(page)
        }
        Source::Saved(dir) => {
            let page = ops::copy_node(dir, parent)?;
            // The template's own history is not the new page's.
            drop_history(&page);
            let fresh = (|| -> Result<()> {
                ops::set_title(&page, "")?;
                restamp(&page)
            })();
            if let Err(e) = fresh {
                let _ = fs::remove_dir_all(&page);
                return Err(e);
            }
            Ok(page)
        }
    }
}

/// Takes away a page folder's hidden `.notecraft` folder (its versions).
fn drop_history(page: &Path) {
    let hidden = page.join(".notecraft");
    if hidden.is_dir() {
        let _ = ops::remove_tree(&hidden);
    }
}

/// Puts the template's words after the date a new page starts with.
fn fill(page: &Path, body: &str) -> Result<()> {
    let (format, file) = page_file(page).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", page.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let out = match format {
        PageFormat::Html => {
            let words = render::blocks_to_html(&markdown::parse(body));
            let inner = html::inner(&text);
            html::replace_inner(&text, &format!("{}\n{}", inner.trim_end(), words.trim_end()))
        }
        PageFormat::Cmgfm => format!("{}\n\n{}", text.trim_end(), body.trim_end()) + "\n",
    };
    write_atomic(&file, &out)
}

/// A page copied from a saved template was started now, not when the template was.
fn restamp(page: &Path) -> Result<()> {
    let (format, file) = page_file(page).ok_or_else(|| StoreError::NotFound(format!("{} is not a page", page.display())))?;
    let text = fs::read_to_string(&file).map_err(|e| StoreError::io("read", &file, e))?;
    let now = iso_now();
    let out = match format {
        PageFormat::Html => html::set_meta(&html::set_meta(&text, "notecraft:created", &now), "notecraft:updated", &now),
        PageFormat::Cmgfm => {
            let (front, body) = frontmatter::split_front(&text);
            let front = frontmatter::set_value(&frontmatter::set_value(&front, "created", &now), "updated", &now);
            format!("{front}{body}")
        }
    };
    write_atomic(&file, &out)
}

/// Copies a page's folder without the subpages in it and without its hidden `.notecraft`
/// folder (its versions): its own files, its assets and any other folder that is not a page.
fn copy_page_only(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        // The page's own history (`.notecraft/versions`) is not part of what it looks like.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if kind.is_dir() {
            if read_node(&entry.path()).is_none() {
                copy_dir(&entry.path(), &target)?;
            }
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// One line per template, as the engine's `page.templates` reports them: id, category, name,
/// separated by tabs.
pub fn describe(templates: &[Template]) -> String {
    templates.iter().map(|t| format!("{}\t{}\t{}", t.id, t.category.id(), kv::quote(&t.name))).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn section(t: &TempDir, format: PageFormat) -> (PathBuf, PathBuf, Settings) {
        let root = t.path().join("notebooks");
        fs::create_dir_all(&root).expect("root");
        let settings = Settings::at(t.path().join("settings.toml"));
        let nb = ops::create_notebook(&root, "Notes", Some(format), &settings).expect("notebook");
        let sec = ops::create_node(&nb, "School", NodeKind::Section, None, &settings).expect("section");
        (root, sec, settings)
    }

    #[test]
    fn every_built_in_template_makes_a_page_in_either_format() {
        for format in PageFormat::ALL {
            let t = TempDir::new("templates-built-in");
            let (root, sec, settings) = section(&t, format);
            for template in list(&root) {
                let page = new_page(&template, &sec, &settings).expect("new page");
                assert_eq!(read_node(&page).expect("a page").kind, NodeKind::Page);
                assert!(!crate::page::load_page(&page).expect("load").has_title, "{}: a new page has no title yet", template.name);
                let Source::BuiltIn(b) = template.source else { continue };
                let (paper, _) = crate::page::appearance(&page).expect("appearance");
                assert_eq!(paper.as_deref(), b.paper, "{}", b.name);
                let text = crate::page::load_page(&page).expect("load").body;
                if let Some(first) = b.body.lines().find(|l| l.starts_with("## ")) {
                    assert!(text.contains(first.trim_start_matches("## ")), "{} in {format:?}: {text}", b.name);
                }
            }
            assert_eq!(tree::child_dirs(&sec).len(), BUILT_IN.len(), "one page each, nothing overwritten");
        }
    }

    #[test]
    fn a_saved_template_keeps_the_page_and_gives_new_pages_their_own_ids() {
        let t = TempDir::new("templates-saved");
        let (root, sec, settings) = section(&t, PageFormat::Cmgfm);
        let page = ops::create_node(&sec, "Lab report", NodeKind::Page, None, &settings).expect("page");
        crate::page::save_body(&page, "# Lab report\n\n## Method\n\n## Results\n").expect("save");
        set_appearance(&page, Some("grid-small"), Some("yellow")).expect("paper");
        fs::write(page.join("assets").join("diagram.txt"), "a file").expect("asset");
        fs::create_dir_all(page.join(".notecraft").join("versions")).expect("history");
        let sub = ops::create_node(&page, "Subpage", NodeKind::Page, None, &settings).expect("subpage");
        assert!(sub.is_dir());

        let kept = save(&root, &page, "Lab Report").expect("save as template");
        assert!(kept.starts_with(folder(&root)));
        assert!(!kept.join("subpage").exists(), "subpages are not part of a template");
        assert!(!kept.join(".notecraft").exists(), "nor is the page's history");
        let template = find(&root, "saved:lab-report").expect("listed");
        assert_eq!((template.name.as_str(), template.category), ("Lab Report", Category::Mine));
        // The notebooks folder's tree does not show the templates.
        assert!(tree::child_dirs(&root).iter().all(|d| read_node(d).is_none_or(|n| n.kind == NodeKind::Notebook)));

        let one = new_page(&template, &sec, &settings).expect("from template");
        let two = new_page(&template, &sec, &settings).expect("again");
        let (a, b) = (read_node(&one).expect("one"), read_node(&two).expect("two"));
        assert_ne!(a.id, b.id, "each page has its own id");
        assert_ne!(a.id, read_node(&kept).expect("template").id);
        assert!(!crate::page::load_page(&one).expect("load").has_title, "the page is untitled, as Add Page makes it");
        assert!(crate::page::load_page(&one).expect("load").body.contains("## Results"));
        assert_eq!(crate::page::appearance(&one).expect("look"), (Some("grid-small".into()), Some("yellow".into())));
        assert!(one.join("assets").join("diagram.txt").is_file(), "the template's files come too");
        assert!(read_node(&one).expect("one").order > read_node(&page).expect("page").order, "last in the section");

        remove(&root, "saved:lab-report").expect("remove");
        assert!(find(&root, "saved:lab-report").is_none());
        assert!(remove(&root, "builtin:grid").is_err(), "built-in templates stay");
    }

    #[test]
    fn bad_requests_are_refused() {
        let t = TempDir::new("templates-bad");
        let (root, sec, _) = section(&t, PageFormat::Html);
        assert!(save(&root, &sec, "Section").is_err(), "a section is not a page");
        assert!(save(&root, &sec, "  ").is_err(), "a template needs a name");
        assert!(find(&root, "saved:nothing").is_none());
        assert!(describe(&list(&root)).lines().count() == BUILT_IN.len());
    }
}

//! Sample notebooks: a small, fixed set of content that shows every feature. Used by the
//! tests, by the headless screenshot runner, by `notecraft-cli sample` and by the demo.

use crate::{Output, Params, Session};
use std::path::PathBuf;

/// What `create_sample` made.
#[derive(Debug, Clone)]
pub struct Sample {
    pub notebook: PathBuf,
    /// A page in HTML.
    pub html_page: PathBuf,
    /// A page in CommonMark + GFM with a table, tasks, equations, a diagram and a picture.
    pub md_page: PathBuf,
    /// The first page of a chain of sub-pages seven levels deep.
    pub deep_page: PathBuf,
}

pub const HTML_BODY: &str = "<h1>Project X</h1><p>Kickoff notes for the <b>first release</b>. Owner: <i>the team</i>. See the <a href=\"https://example.test\">plan</a>.</p><h2>Goals</h2><ul><li>Ship the folder format</li><li>Native app</li><li>Import from .one files</li></ul><p>&#9744; write the proposal</p><table><tr><th>Phase</th><th>Status</th></tr><tr><td>Format</td><td>done</td></tr><tr><td>App</td><td>in progress</td></tr></table><blockquote>Quality is not negotiable.</blockquote><p>Equation <span class=\"math\">$ a^2 + b^2 = c^2 $</span></p><pre class=\"diagram\">graph TD; A--&gt;B;</pre>";

pub const MD_BODY: &str = "# Format ideas\n\nNoteCraft pages are **plain files** in plain folders, so *any* sync tool works. Two page formats: HTML and CommonMark + GFM.\n\n## A checklist\n\n- [x] Create a notebook\n- [x] Add sections and pages\n- [ ] Decide the default format\n- [ ] Try attaching a picture\n\n## Why folders?\n\n1. A page and its media move together.\n2. Sub-pages nest without a limit.\n3. Backup is a copy.\n\n> Notes should outlive the app that wrote them.\n\n| Format | Extension | Good for |\n|--------|-----------|----------|\n| HTML | index.html | rich layout |\n| CommonMark + GFM | index.md | clean diffs and merges |\n\nAn equation: $E = mc^2$, and a display one:\n\n$$\n\\int_0^1 x^2\\,dx = \\tfrac{1}{3}\n$$\n\n```mermaid\ngraph TD;\n  Page-->SubPage;\n  Page-->Assets;\n```\n\n![the koala](assets/koala.png)\n\nUse `code` freely, and ~~cross things out~~.\n";

fn params(pairs: &[(&str, &str)]) -> Params {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn make(s: &mut Session, id: &str, pairs: &[(&str, &str)]) -> Result<PathBuf, crate::EngineError> {
    match s.run(id, &params(pairs))? {
        Output::Path(p) => Ok(p),
        _ => Err(crate::EngineError::BadParam(format!("{id} did not return a path"))),
    }
}

/// Creates two notebooks, "My Notebook" (a section group, two sections, an HTML page, a seven
/// level deep chain of sub-pages, and a Markdown page) and an empty "Archive". If `picture` is
/// given it is stored as `assets/koala.png` of the Markdown page.
pub fn create_sample(s: &mut Session, picture: Option<&[u8]>) -> Result<Sample, crate::EngineError> {
    // `empty` so the sample shows its own structure, not the first section a new notebook gets.
    let nb = make(s, "notebook.create", &[("name", "My Notebook"), ("empty", "true")])?;
    let nbs = nb.to_string_lossy().into_owned();
    let group = make(s, "node.create", &[("parent", &nbs), ("kind", "section-group"), ("title", "Work")])?;
    let projects = make(s, "node.create", &[("parent", &group.to_string_lossy()), ("kind", "section"), ("title", "Projects")])?;
    let html_page = make(s, "node.create", &[("parent", &projects.to_string_lossy()), ("kind", "page"), ("title", "Project X")])?;
    s.run("page.save", &params(&[("path", &html_page.to_string_lossy()), ("body", HTML_BODY)]))?;
    let deep_page = make(s, "node.create", &[("parent", &projects.to_string_lossy()), ("kind", "page"), ("title", "Deep page")])?;
    let mut tip = deep_page.clone();
    for i in 1..=6 {
        tip = make(s, "node.create", &[("parent", &tip.to_string_lossy()), ("kind", "page"), ("title", &format!("Level {i}"))])?;
    }
    let ideas = make(s, "node.create", &[("parent", &nbs), ("kind", "section"), ("title", "Ideas"), ("format", "cmgfm")])?;
    let md_page = make(s, "node.create", &[("parent", &ideas.to_string_lossy()), ("kind", "page"), ("title", "Format ideas")])?;
    if let Some(bytes) = picture {
        s.attach_bytes(&md_page.to_string_lossy(), "koala.png", bytes)?;
    }
    s.run("page.save", &params(&[("path", &md_page.to_string_lossy()), ("body", MD_BODY)]))?;
    make(s, "notebook.create", &[("name", "Archive"), ("empty", "true")])?;
    Ok(Sample { notebook: nb, html_page, md_page, deep_page })
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_store::Settings;
    use notecraft_store::testing::TempDir;

    #[test]
    fn the_sample_has_the_expected_shape_and_is_clean() {
        let t = TempDir::new("sample1");
        let mut s = Session::new(&t.path().join("nb"), Settings::at(t.path().join("s.toml"))).expect("session");
        let info = create_sample(&mut s, Some(b"png")).expect("sample");
        assert_eq!(s.tree().len(), 2);
        assert_eq!(s.tree().iter().map(|n| n.count()).sum::<usize>(), 14);
        assert_eq!(
            s.tree().iter().map(|n| n.depth()).max(),
            Some(10),
            "notebook, group, section, the page and six sub-pages: ten levels, more than the apps that cap sub-pages at three"
        );
        assert!(info.md_page.join("assets").join("koala.png").is_file());
        match s.run("notebook.check", &params(&[("path", &info.notebook.to_string_lossy())])).expect("check") {
            Output::Report(r) => assert!(r.problems.is_empty(), "{:?}", r.problems),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_second_sample_in_the_same_folder_does_not_clash() {
        let t = TempDir::new("sample2");
        let mut s = Session::new(&t.path().join("nb"), Settings::at(t.path().join("s.toml"))).expect("session");
        create_sample(&mut s, None).expect("one");
        create_sample(&mut s, None).expect("two");
        assert_eq!(s.tree().len(), 4);
    }
}

use notecraft_formats::boxes;
use notecraft_formats::markdown::{self, Block};
use notecraft_model::PageFormat;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn pages(root: &Path) -> BTreeMap<PathBuf, (PageFormat, String)> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, (PageFormat, String)>) {
        for entry in std::fs::read_dir(dir).expect("read demo folder") {
            let path = entry.expect("demo entry").path();
            if path.is_dir() {
                walk(root, &path, found);
                continue;
            }
            let format = match path.file_name().and_then(|name| name.to_str()) {
                Some("index.html") => PageFormat::Html,
                Some("index.md") => PageFormat::Cmgfm,
                _ => continue,
            };
            let relative = path.parent().expect("page folder").strip_prefix(root).expect("inside notebook").to_path_buf();
            found.insert(relative, (format, std::fs::read_to_string(path).expect("read demo page")));
        }
    }

    let mut found = BTreeMap::new();
    walk(root, root, &mut found);
    found
}

fn notebook_files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, found: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read notebook folder") {
            let path = entry.expect("notebook entry").path();
            if path.is_dir() {
                walk(root, &path, found);
                continue;
            }
            let mut relative = path.strip_prefix(root).expect("inside notebook").to_path_buf();
            if matches!(path.file_name().and_then(|name| name.to_str()), Some("index.html" | "index.md")) {
                relative.set_file_name("index.page");
            }
            found.insert(relative, std::fs::read(path).expect("read notebook file"));
        }
    }

    let mut found = BTreeMap::new();
    walk(root, root, &mut found);
    found
}

/// The complete HTML and CMGFM demo notebooks must contain the same pages and semantic content.
#[test]
fn the_full_demo_notebooks_have_matching_pages_and_blocks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("docs").join("demo").join("DemoNotebook");
    let html = pages(&root.join("html"));
    let cmgfm = pages(&root.join("cmgfm"));
    assert_eq!(html.len(), 197, "expected every page from the 13-language demo");
    assert_eq!(html.len(), cmgfm.len(), "both notebooks must have the same number of pages");
    assert_eq!(html.keys().collect::<Vec<_>>(), cmgfm.keys().collect::<Vec<_>>(), "page paths must match");
    for language in ["10-en", "11-es", "12-fr", "13-de", "14-hi", "15-ru", "16-zh", "17-ja", "18-ko", "20-ar", "21-fa", "22-ur", "23-he"] {
        for page in ["shapes", "handwriting-and-drawing", "mermaid-diagrams", "charts-vega-d3"] {
            let path = Path::new(language).join("how-to").join(page);
            assert!(html.contains_key(&path), "the 13-language demo has {}", path.display());
        }
    }

    let html_files = notebook_files(&root.join("html"));
    let cmgfm_files = notebook_files(&root.join("cmgfm"));
    assert_eq!(html_files.keys().collect::<Vec<_>>(), cmgfm_files.keys().collect::<Vec<_>>(), "all files and assets must match");
    for (path, html_bytes) in &html_files {
        if path.file_name().and_then(|name| name.to_str()) == Some("index.page") {
            continue;
        }
        let cmgfm_bytes = &cmgfm_files[path];
        if path == Path::new("_node.toml") {
            let html_node = String::from_utf8_lossy(html_bytes).replace("format = \"html\"\n", "");
            let cmgfm_node = String::from_utf8_lossy(cmgfm_bytes).replace("format = \"cmgfm\"\n", "");
            assert_eq!(html_node, cmgfm_node, "only the notebook default format differs");
        } else {
            assert_eq!(html_bytes, cmgfm_bytes, "shared metadata/assets must be byte-identical: {}", path.display());
        }
    }

    for (path, (html_format, html_page)) in &html {
        let (cmgfm_format, cmgfm_page) = &cmgfm[path];
        assert_eq!(*html_format, PageFormat::Html, "{} is an HTML page", path.display());
        assert_eq!(*cmgfm_format, PageFormat::Cmgfm, "{} is a CMGFM page", path.display());
        let html_body = notecraft_formats::html::inner(html_page);
        let (cmgfm_head, cmgfm_body) = notecraft_formats::frontmatter::split_front(cmgfm_page);
        assert_eq!(
            notecraft_formats::html::title(html_page).as_deref(),
            notecraft_formats::frontmatter::value(&cmgfm_head, "title").as_deref(),
            "page title metadata matches at {}",
            path.display()
        );
        for (key, meta) in [
            ("id", "notecraft:id"),
            ("order", "notecraft:order"),
            ("created", "notecraft:created"),
            ("updated", "notecraft:updated"),
            ("dir", "notecraft:dir"),
            ("justify", "notecraft:justify"),
            ("columns", "notecraft:columns"),
            ("paper", "notecraft:paper"),
            ("page-color", "notecraft:page-color"),
        ] {
            assert_eq!(
                notecraft_formats::html::meta(html_page, meta).as_deref(),
                notecraft_formats::frontmatter::value(&cmgfm_head, key).as_deref(),
                "page metadata {key} matches at {}",
                path.display()
            );
        }
        let html_content = boxes::read(PageFormat::Html, &html_body);
        let cmgfm_content = boxes::read(PageFormat::Cmgfm, &cmgfm_body);
        if html_content != cmgfm_content {
            let (left, left_layout) = html_content;
            let (right, right_layout) = cmgfm_content;
            let first_difference = left.iter().zip(&right).position(|(a, b)| a != b);
            panic!(
                "semantic content/layout differs at {}: {} HTML blocks vs {} CMGFM blocks, first difference {:?}, layouts equal: {}; HTML first: {:?}; CMGFM first: {:?}",
                path.display(),
                left.len(),
                right.len(),
                first_difference,
                left_layout == right_layout,
                left.first(),
                right.first()
            );
        }
    }
}

/// A canonical fixture protects every semantic block kind at the HTML and CMGFM boundaries.
#[test]
fn every_page_model_block_round_trips_through_both_formats_and_boxes() {
    let source = "# Heading\n\nA **bold**, *italic*, ~~struck~~, `code`, [link](../target/), inline ![picture](assets/inline.png), and $x^2$.\n\n- bullet\n  - nested bullet\n- [ ] open task\n- [x] completed task\n1. ordered\n\n> quoted words\n\n```rust\nlet value = 1;\n```\n\n```mermaid\ngraph TD; A-->B;\n```\n\n$$\nx = \\frac{1}{2}\n$$\n\n| Name | Value |\n| --- | --- |\n| one | **two** |\n\n---\n\n![standalone](assets/standalone.png)\n";
    let expected = markdown::parse(source);

    let mut kinds = [false; 9];
    for block in &expected {
        let kind = match block {
            Block::Heading { .. } => 0,
            Block::Paragraph(_) => 1,
            Block::Item { .. } => 2,
            Block::Quote(_) => 3,
            Block::Code { .. } => 4,
            Block::MathBlock(_) => 5,
            Block::Table { .. } => 6,
            Block::Rule => 7,
            Block::Image { .. } => 8,
        };
        kinds[kind] = true;
    }
    assert!(kinds.iter().all(|present| *present), "fixture must exercise every Block variant: {kinds:?}");
    assert!(
        expected.iter().any(|block| match block {
            Block::Paragraph(spans) | Block::Heading { spans, .. } | Block::Item { spans, .. } | Block::Quote(spans) => {
                spans.iter().any(|span| span.image.is_some())
            }
            _ => false,
        }),
        "fixture must exercise inline image spans too"
    );

    let containers = vec![notecraft_model::Container::new(24.0, 48.0, 540.0, expected.len())];
    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        let serialized = boxes::write(format, &expected, Some(&containers));
        let (actual, layout) = boxes::read(format, &serialized);
        assert_eq!(actual, expected, "{format:?} content survives serialization");
        assert_eq!(layout, Some(containers.clone()), "{format:?} layout survives serialization");
    }

    let html_body = boxes::write(PageFormat::Html, &expected, Some(&containers));
    let (html_model, html_layout) = boxes::read(PageFormat::Html, &html_body);
    let cmgfm_body = boxes::write(PageFormat::Cmgfm, &html_model, html_layout.as_deref());
    let (cross_format, cross_layout) = boxes::read(PageFormat::Cmgfm, &cmgfm_body);
    assert_eq!(cross_format, expected, "HTML content converted to CMGFM maps to the same semantic model");
    assert_eq!(cross_layout, Some(containers), "layout survives conversion between formats");
}

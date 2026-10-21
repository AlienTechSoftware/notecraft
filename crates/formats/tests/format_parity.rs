use notecraft_formats::boxes;
use notecraft_model::PageFormat;
use std::path::Path;

/// The standalone notebook is handy for visual checks; this test keeps its paired pages
/// semantically identical as their readers and writers evolve.
#[test]
fn the_manual_format_pair_has_the_same_blocks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("tests").join("test-notebook").join("examples");
    let html = std::fs::read_to_string(root.join("HTML").join("index.html")).expect("HTML sample");
    let markdown = std::fs::read_to_string(root.join("CMGFM").join("index.md")).expect("CMGFM sample");
    let html_body = notecraft_formats::html::inner(&html);
    let (_, markdown_body) = notecraft_formats::frontmatter::split_front(&markdown);
    let html_blocks = boxes::read(PageFormat::Html, &html_body).0;
    let markdown_blocks = boxes::read(PageFormat::Cmgfm, &markdown_body).0;
    assert_eq!(html_blocks, markdown_blocks, "keep the paired pages in tests/test-notebook semantically identical");
}

use std::fs;
use std::path::Path;

use notecraft_import::{Options, run};
use notecraft_model::{NodeColor, NodeKind, PageFormat};
use notecraft_onenote::testing::{OneNote, Toc};
use notecraft_store::testing::TempDir;
use notecraft_store::tree::{child_dirs, read_node};

#[path = "support/synthetic_corpus.rs"]
mod synthetic_corpus;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/synthetic");

fn quiet(_: &notecraft_import::Event<'_>) {}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create isolated fixture tree");
    for entry in fs::read_dir(source).expect("read fixture directory") {
        let entry = entry.expect("read fixture entry");
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type().expect("fixture entry type").is_dir() {
            copy_tree(&source_path, &destination_path);
        } else {
            fs::copy(source_path, destination_path).expect("copy fixture file");
        }
    }
}

fn write_section(root: &Path, filename: &str, identity: u8, color: u32) {
    let bytes =
        OneNote::new().with_identity(identity).with_color(color).with_page("Synthetic page", &[OneNote::line("synthetic color regression")]).finish();
    fs::write(root.join(filename), bytes).expect("write synthetic section");
}

fn contains_page_format(root: &Path, format: PageFormat) -> bool {
    read_node(root).is_some_and(|node| node.kind == NodeKind::Page && node.format == Some(format))
        || child_dirs(root).iter().any(|child| contains_page_format(child, format))
}

#[test]
fn toc_and_section_metadata_colors_survive_import_in_both_page_formats() {
    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        let temp = TempDir::new("import-colors-with-toc");
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        copy_tree(&Path::new(FIXTURES).join("colors"), &source);

        let mut options = Options::new(&source, &destination);
        options.format = format;
        let report = run(&options, &mut quiet).expect("import synthetic notebook");
        assert_eq!(report.sections_imported, 2, "{report}");
        assert_eq!((report.toc_files_found, report.toc_files_read, report.toc_files_failed), (1, 1, 0), "{report}");

        let notebook = destination.join("work");
        assert_eq!(read_node(&notebook).map(|node| node.color), Some(Some(NodeColor::Purple)));
        assert_eq!(read_node(&notebook.join("planning")).map(|node| node.color), Some(Some(NodeColor::Blue)));
        assert_eq!(read_node(&notebook.join("archive")).map(|node| node.color), Some(Some(NodeColor::Red)));
        assert!(contains_page_format(&notebook.join("planning"), format));
    }
}

#[test]
fn generator_reproduces_the_checked_in_color_fixture_bytes() {
    let generated = TempDir::new("synthetic-color-fixture-generation");
    synthetic_corpus::generate(generated.path()).expect("generate synthetic corpus");
    for relative in ["colors/Work/.onetoc2", "colors/Work/Planning.one", "colors/Work/Archive.one"] {
        assert_eq!(
            fs::read(generated.path().join(relative)).expect("generated color fixture"),
            fs::read(Path::new(FIXTURES).join(relative)).expect("checked-in color fixture"),
            "generated fixture bytes match: {relative}"
        );
    }
}

#[test]
fn a_section_color_is_imported_without_a_toc_and_not_guessed_for_the_notebook() {
    let temp = TempDir::new("import-color-without-toc");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("create source");
    write_section(&source, "Planning.one", 2, 0x001E_8A44);

    let report = run(&Options::new(&source, &destination), &mut quiet).expect("import section");

    assert_eq!(report.sections_imported, 1, "{report}");
    let notebook = destination.join("source");
    assert_eq!(read_node(&notebook).map(|node| node.color), Some(None));
    assert_eq!(read_node(&notebook.join("planning")).map(|node| node.color), Some(Some(NodeColor::Green)));
}

#[test]
fn malformed_optional_toc_does_not_block_valid_sections() {
    let temp = TempDir::new("import-color-malformed-toc");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("create source");
    write_section(&source, "Planning.one", 2, 0x001E_8A44);
    fs::write(source.join(".onetoc2"), b"not a TOC").expect("write malformed optional TOC");

    let report = run(&Options::new(&source, &destination), &mut quiet).expect("import remains successful");

    assert_eq!(report.sections_imported, 1, "{report}");
    assert_eq!((report.toc_files_found, report.toc_files_read, report.toc_files_failed), (1, 0, 1), "{report}");
    assert_eq!(read_node(&destination.join("source").join("planning")).map(|node| node.color), Some(Some(NodeColor::Green)));
}

#[test]
fn a_toc_color_is_not_applied_when_identity_and_filename_disagree() {
    let temp = TempDir::new("import-color-toc-mismatch");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("create source");
    write_section(&source, "Planning.one", 2, 0x001E_8A44);
    let toc = Toc::new().section("Renamed.one", OneNote::file_identity(9), Some(0x00C2_5A11));
    fs::write(source.join(".onetoc2"), toc.finish()).expect("write mismatched TOC entry");

    run(&Options::new(&source, &destination), &mut quiet).expect("import section");

    assert_eq!(read_node(&destination.join("source").join("planning")).map(|node| node.color), Some(Some(NodeColor::Green)));
}

#[test]
fn a_toc_entry_without_identity_can_match_by_filename_in_its_folder() {
    let temp = TempDir::new("import-color-toc-filename-match");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("create source");
    write_section(&source, "Planning.one", 2, 0x001E_8A44);
    let toc = Toc::new().section_without_identity("Planning.one", Some(0x001A_5FD0));
    fs::write(source.join(".onetoc2"), toc.finish()).expect("write TOC without identity");

    run(&Options::new(&source, &destination), &mut quiet).expect("import section");

    assert_eq!(read_node(&destination.join("source").join("planning")).map(|node| node.color), Some(Some(NodeColor::Blue)));
}

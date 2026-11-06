use std::fs;
use std::path::Path;

use notecraft_import::{Options, run};
use notecraft_model::PageFormat;
use notecraft_store::testing::TempDir;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/3p");

fn quiet(_: &notecraft_import::Event<'_>) {}

fn verify_source(
    source: &Path,
    expected_sections: usize,
    expected_imported: usize,
    expected_skipped: usize,
    expected_ignored: usize,
    expected_assets_and_tables: (usize, usize, usize),
    format: PageFormat,
) {
    let temp = TempDir::new("third-party-import-corpus");
    let destination = temp.path().join("notebooks");
    let mut options = Options::new(source, &destination);
    options.format = format;
    let report = run(&options, &mut quiet).expect("import third-party test fixtures");

    assert_eq!(report.sections_planned, expected_sections, "{report}");
    assert_eq!(report.sections_imported, expected_imported, "{report}");
    assert_eq!(report.skipped.len(), expected_skipped, "{report}");
    assert!(report.skipped.iter().all(|skipped| skipped.reason == "not a OneNote section file"), "skips are explicit and bounded: {report}");
    assert_eq!(report.ignored, expected_ignored, "{report}");
    assert_eq!((report.images, report.files, report.tables), expected_assets_and_tables, "{report}");
    assert!(report.pages > 0, "the source corpus contains pages: {report}");
    assert!(destination.is_dir(), "the importer creates its output tree");
}

#[test]
fn officeimo_and_tika_corpus_imports_in_both_page_formats() {
    let source = Path::new(FIXTURES).join("officeimo");
    let readme = fs::read_to_string(source.join("README.md")).expect("OfficeIMO source README");
    for fixture in [
        "testOneNote.one",
        "testOneNote2016.one",
        "testOneNoteEmbeddedWordDoc.one",
        "testOneNoteFromOffice365.one",
        "testOneNoteFromOffice365-2.one",
        "makecab-lzx-notebook.onepkg",
    ] {
        assert!(source.join(fixture).is_file(), "documented OfficeIMO fixture exists: {fixture}");
        assert!(readme.contains(fixture), "OfficeIMO README records the fixture: {fixture}");
    }
    assert!(source.join("LICENSE-APACHE-2.0.txt").is_file());
    assert!(source.join("LICENSE-MIT-OfficeIMO.txt").is_file());
    assert!(source.join("NOTICE.txt").is_file());

    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        verify_source(&source, 5, 3, 2, 1, (3, 1, 0), format);
    }
}

#[test]
fn onenote_rs_corpus_imports_in_both_page_formats_and_reports_toc_files() {
    let source = Path::new(FIXTURES).join("onenote.rs").join("samples");
    let readme = fs::read_to_string(source.parent().expect("source folder").join("README.md")).expect("Rust parser source README");
    assert!(source.parent().expect("source folder").join("LICENSE-MPL-2.0.txt").is_file());
    for fixture in [
        "handwriting_recognition.one",
        "New Section 1.one",
        "New Section Group/New Section 1.one",
        "New Section Group/New Section 2.one",
        "OneNote_RecycleBin/OneNote_DeletedPages.one",
        "Schnelle Notizen.one",
        "non-legacy/New Section 1 2.one",
        "non-legacy/New Section 2.one",
        "non-legacy/New Section 3.one",
        "Open Notebook.onetoc2",
        "New Section Group/Open Notebook.onetoc2",
        "non-legacy/Open Notebook.onetoc2",
        "object-revision/Open Notebook.onetoc2",
        "OneNote_RecycleBin/Open Notebook.onetoc2",
    ] {
        assert!(source.join(fixture).is_file(), "documented Rust parser fixture exists: {fixture}");
        assert!(readme.contains(fixture.rsplit('/').next().unwrap_or(fixture)), "Rust parser README records the fixture: {fixture}");
    }

    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        verify_source(&source, 9, 1, 8, 5, (1, 0, 2), format);
    }
}

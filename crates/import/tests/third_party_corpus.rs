use std::fs;
use std::io::Read;
use std::path::Path;

use notecraft_import::{Options, run};
use notecraft_model::PageFormat;
use notecraft_store::testing::TempDir;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/3p");
const LARGE_FIXTURE_NAMES: [&str; 2] = ["Large Desktop.one", "Large OneDrive.one"];

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

fn copy_small_fixture_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("create isolated fixture tree");
    for entry in fs::read_dir(source).expect("read fixture source") {
        let entry = entry.expect("read fixture entry");
        let from = entry.path();
        let to = destination.join(entry.file_name());
        if entry.file_type().expect("read fixture entry type").is_dir() {
            copy_small_fixture_tree(&from, &to);
        } else if !LARGE_FIXTURE_NAMES.iter().any(|name| from.file_name().is_some_and(|file| file == *name)) {
            fs::copy(&from, &to).expect("copy small fixture into isolated tree");
        }
    }
}

fn run_full_small_rust_corpus(source: &Path, format: PageFormat, destination: &Path) {
    let mut options = Options::new(source, destination);
    options.format = format;
    let report = run(&options, &mut quiet).expect("import small onenote.rs fixtures");

    let one_files = 32;
    let toc_files = 20;
    assert_eq!(report.files_found, one_files, "all non-large .one fixtures are discovered: {report}");
    assert_eq!(report.toc_files_found, toc_files, "all .onetoc2 fixtures are discovered: {report}");
    assert_eq!(
        report.sections_planned + report.older_copies_left,
        one_files,
        "every .one file is planned or accounted as an older version: {report}"
    );
    assert_eq!(
        report.sections_imported + report.skipped.len(),
        report.sections_planned,
        "every planned sample imports or has a skip reason: {report}"
    );
    assert!(report.skipped.iter().all(|skipped| !skipped.reason.is_empty()), "all skipped samples explain why: {report}");
    assert!(report.pages > 0, "the source corpus contains imported pages: {report}");
    assert!(destination.is_dir(), "the importer creates its output tree");
}

fn run_large_rust_fixture(name: &str) {
    let source = Path::new(FIXTURES).join("onenote.rs").join("samples").join(name);
    let mut file = fs::File::open(&source).expect("open large LFS fixture");
    let pointer_prefix = b"version https://git-lfs.github.com/spec/v1";
    let mut prefix = vec![0_u8; pointer_prefix.len()];
    let bytes_read = file.read(&mut prefix).expect("read large fixture prefix");
    assert!(
        !prefix[..bytes_read].starts_with(pointer_prefix),
        "hydrate this fixture with git lfs pull --include=\"crates/import/tests/fixtures/3p/onenote.rs/samples/Large*.one\" --exclude=\"\""
    );

    let temp = TempDir::new("large-third-party-import-corpus");
    let destination = temp.path().join("notebooks");
    let options = Options::new(&source, &destination);
    let report = run(&options, &mut quiet).expect("process large external fixture");
    assert_eq!(report.sections_planned, 1, "the large fixture is planned: {report}");
    assert_eq!(report.sections_imported + report.skipped.len(), 1, "the fixture imports or reports a skip: {report}");
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
        "makecab-lzx-testOneNote2016.cab",
        "makecab-lzx-testOneNoteFromOffice365-2.cab",
        "makecab-lzx-e8.cab",
        "makecab-lzx15-e8.cab",
        "makecab-lzx16-e8.cab",
        "makecab-lzx17-e8.cab",
        "makecab-lzx18-e8.cab",
        "makecab-lzx19-e8.cab",
        "makecab-lzx20-e8.cab",
    ] {
        assert!(source.join(fixture).is_file(), "documented OfficeIMO fixture exists: {fixture}");
        assert!(readme.contains(fixture), "OfficeIMO README records the fixture: {fixture}");
    }
    assert!(source.join("LICENSE-APACHE-2.0.txt").is_file());
    assert!(source.join("LICENSE-MIT-OfficeIMO.txt").is_file());
    assert!(source.join("NOTICE.txt").is_file());

    // The five sections, and the one inside the OfficeIMO package, which is now unpacked and
    // imported (issue #17) rather than left aside.
    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        verify_source(&source, 6, 4, 2, 0, (3, 1, 0), format);
    }
}

/// Issue #17: a `.onepkg` package, given on its own, is unpacked and imported as a notebook named
/// after it, in both page formats. (The unpacked folder's removal is `package::tests`'s.)
#[test]
fn a_onepkg_package_imports_as_a_notebook_in_both_page_formats() {
    let package = Path::new(FIXTURES).join("officeimo").join("makecab-lzx-notebook.onepkg");
    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        let temp = TempDir::new("onepkg-import");
        let destination = temp.path().join("notebooks");
        let mut options = Options::new(&package, &destination);
        options.format = format;
        let report = run(&options, &mut quiet).expect("import the package");
        assert_eq!(report.packages, 1, "{report}");
        assert_eq!(report.ignored, 0, "{report}");
        assert_eq!(report.toc_files_found, 1, "the package's table of contents is found: {report}");
        assert_eq!((report.sections_planned, report.sections_imported), (1, 1), "{report}");
        assert!(report.pages > 0, "{report}");
        assert!(report.to_string().contains("OneNote packages unpacked: 1"), "{report}");
        let notebook = destination.join("makecab-lzx-notebook");
        assert!(
            notebook.is_dir(),
            "a notebook named after the package: {:?}",
            fs::read_dir(&destination).map(|rd| rd.flatten().map(|e| e.file_name()).collect::<Vec<_>>())
        );
        assert!(notebook.join("compressed-section").is_dir(), "{format:?}: its section is there");
    }

    // A dry run unpacks to plan, and writes nothing to the destination.
    let temp = TempDir::new("onepkg-dry-run");
    let destination = temp.path().join("notebooks");
    let mut options = Options::new(&package, &destination);
    options.dry_run = true;
    let report = run(&options, &mut quiet).expect("plan the package");
    assert_eq!((report.packages, report.sections_planned), (1, 1), "{report}");
    assert!(!destination.exists(), "a dry run writes nothing");
}

/// A package that cannot be unpacked costs that package, says why, and the rest of the folder
/// is imported.
#[test]
fn a_broken_package_is_reported_and_the_rest_imports() {
    let temp = TempDir::new("onepkg-broken");
    let source = temp.path().join("source");
    fs::create_dir_all(&source).expect("source");
    fs::write(source.join("broken.onepkg"), b"MSCF this is not a cabinet").expect("broken package");
    fs::copy(Path::new(FIXTURES).join("officeimo").join("testOneNote2016.one"), source.join("testOneNote2016.one")).expect("section");
    let report = run(&Options::new(&source, temp.path().join("notebooks")), &mut quiet).expect("import");
    assert_eq!((report.packages, report.ignored), (0, 1), "{report}");
    assert!(report.skipped.iter().any(|s| s.label == "broken.onepkg" && s.reason.contains("not a OneNote package")), "{:?}", report.skipped);
    assert_eq!(report.sections_imported, 1, "the section beside it still imports: {report}");
}

#[test]
fn onenote_rs_corpus_imports_in_both_page_formats_and_reports_toc_files() {
    let fixtures = Path::new(FIXTURES).join("onenote.rs");
    let source = fixtures.join("samples");
    let readme = fs::read_to_string(fixtures.join("README.md")).expect("Rust parser source README");
    assert!(fixtures.join("LICENSE-MPL-2.0.txt").is_file());
    assert!(fixtures.join("samples/SHA256SUMS.txt").is_file());
    assert!(fixtures.join("samples/joplin/LICENSE").is_file());
    assert!(fixtures.join("samples/joplin/LICENSE-AGPL-3.0.txt").is_file());
    assert!(fixtures.join("samples/onenote-2016/LICENSE").is_file());
    assert!(fixtures.join("samples/onenote-2016/OneWithFileData.one").is_file());
    assert!(readme.contains("34 `.one` section files and 20 `.onetoc2` files"));
    for name in LARGE_FIXTURE_NAMES {
        assert!(source.join(name).is_file(), "large fixture exists: {name}");
    }
    let mut one_count = 0;
    let mut toc_count = 0;
    let mut pending = vec![source.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).expect("read full fixture inventory") {
            let entry = entry.expect("read fixture inventory entry");
            let path = entry.path();
            if entry.file_type().expect("read fixture inventory type").is_dir() {
                pending.push(path);
            } else if path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".one")) {
                one_count += 1;
            } else if path.file_name().is_some_and(|name| name.to_string_lossy().ends_with(".onetoc2")) {
                toc_count += 1;
            }
        }
    }
    assert_eq!(one_count, 34, "all upstream .one fixtures are present");
    assert_eq!(toc_count, 20, "all upstream .onetoc2 fixtures are present");

    let temp = TempDir::new("onenote-rs-small-import-corpus");
    let small_source = temp.path().join("samples");
    copy_small_fixture_tree(&source, &small_source);
    for format in [PageFormat::Html, PageFormat::Cmgfm] {
        run_full_small_rust_corpus(&small_source, format, &temp.path().join(format!("output-{format:?}")));
    }
}

#[test]
#[ignore = "manual import of the 113 MB Git LFS fixture"]
fn large_desktop_fixture_import_is_reported() {
    run_large_rust_fixture("Large Desktop.one");
}

#[test]
#[ignore = "manual import of the 161 MB Git LFS fixture"]
fn large_onedrive_fixture_import_is_reported() {
    run_large_rust_fixture("Large OneDrive.one");
}

//! Opt-in acceptance test with the installed bridge, never a replacement executable.

use notecraft_store::testing::TempDir;
use std::process::Command;

#[test]
#[ignore = "requires a real installed OfficeIMO bridge in NOTECRAFT_OFFICEIMO_BRIDGE"]
fn sections_and_folders_import_under_the_requested_parent_without_overwriting_notes() {
    let bridge = std::env::var_os("NOTECRAFT_OFFICEIMO_BRIDGE").expect("set the real bridge path");
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/import/tests/fixtures/3p/officeimo/testOneNote2016.one");
    let original = std::fs::read(&source).expect("public fixture");
    for (format, folder) in [("html", false), ("cmgfm", false), ("html", true), ("cmgfm", true)] {
        let t = TempDir::new("cli-real-officeimo");
        let input = if folder {
            let folder = t.path().join("Notebook.v2");
            std::fs::create_dir(&folder).expect("source folder");
            std::fs::copy(&source, folder.join("Section.one")).expect("public section fixture");
            folder
        } else {
            source.clone()
        };
        let parent = t.path().join("notebooks");
        std::fs::create_dir(&parent).expect("output parent");
        std::fs::write(parent.join("keep.txt"), "existing notes").expect("existing notes");
        for expected in 1..=2 {
            let result = Command::new(env!("CARGO_BIN_EXE_notecraft-cli"))
                .env("NOTECRAFT_SETTINGS", t.path().join("settings.toml"))
                .env("NOTECRAFT_OFFICEIMO_BRIDGE", &bridge)
                .args([
                    "--notebooks",
                    &parent.to_string_lossy(),
                    "import",
                    "officeimo",
                    &input.to_string_lossy(),
                    "--to",
                    &parent.to_string_lossy(),
                    "--format",
                    format,
                ])
                .output()
                .expect("run importer with the real bridge");
            assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
            let notebooks = notecraft_store::tree::load_notebooks(&parent);
            assert_eq!(notebooks.len(), expected, "each import creates its own notebook under the parent");
            for notebook in notebooks {
                let page = &notebook.children[0].children[0];
                let loaded = notecraft_store::page::load_page(&page.path).expect("open imported page");
                assert!(loaded.body.contains("This is one note 2016"), "the real section content is accessible");
                assert!(!page.info.id.is_empty());
            }
            assert_eq!(std::fs::read_to_string(parent.join("keep.txt")).expect("existing notes"), "existing notes");
            assert!(!parent.join("index.html").exists() && !parent.join("index.md").exists(), "the parent must not become a page");
        }
    }
    assert_eq!(std::fs::read(source).expect("source preserved"), original);
}

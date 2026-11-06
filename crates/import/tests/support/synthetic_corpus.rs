use std::fs;
use std::io;
use std::path::Path;

use notecraft_onenote::testing::OneNote;

/// Writes the deterministic `.one` files used by the importer regression corpus.
pub fn generate(root: &Path) -> io::Result<()> {
    fs::create_dir_all(root)?;

    let mut rich = OneNote::new();
    rich.page("Start", 1, &[OneNote::line("synthetic plain text"), OneNote::styled("synthetic bold", true, false)]);
    rich.page("Child", 2, &[OneNote::line("synthetic child page")]);
    rich.page("Arabic page", 1, &[OneNote::line("مرحبا بالعالم"), OneNote::line("نص تجريبي آمن")]);
    write_case(root, "rich-content.one", rich.finish())?;

    let mut hierarchy = OneNote::new();
    hierarchy.page("Top level", 1, &[OneNote::line("synthetic top-level page")]);
    hierarchy.page("Second level", 2, &[OneNote::line("synthetic second-level page")]);
    hierarchy.page("Third level", 3, &[OneNote::line("synthetic third-level page")]);
    hierarchy.page("Next top level", 1, &[OneNote::line("synthetic next top-level page")]);
    write_case(root, "page-hierarchy.one", hierarchy.finish())?;

    let mut formatted = OneNote::new();
    formatted.page(
        "Formatted text",
        1,
        &[
            OneNote::line("synthetic plain run"),
            OneNote::styled("synthetic bold run", true, false),
            OneNote::styled("synthetic italic run", false, true),
        ],
    );
    write_case(root, "formatted-text.one", formatted.finish())?;

    let png = include_bytes!("../fixtures/synthetic-pixel.png");
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><path d="M0 0L2 2" stroke="#000"/></svg>"##;
    let text = b"Synthetic attachment content.\n";
    let mut assets = OneNote::new();
    assets.page_with_attachments(
        "Asset gallery",
        1,
        &[OneNote::line("synthetic assets stay attached")],
        &[
            OneNote::picture("sample.png", "raster sample", "png", png),
            OneNote::picture("sample.svg", "vector sample", "svg", svg),
            OneNote::picture_without_data("missing.png", "missing sample", "png"),
            OneNote::file("notes.txt", "txt", text),
        ],
    );
    write_case(root, "asset-pack.one", assets.finish())?;

    let current = OneNote::new().with_page("Current", &[OneNote::line("newest synthetic version")]).finish();
    let previous = OneNote::new().with_page("Previous", &[OneNote::line("older synthetic version")]).finish();
    let broken = current.get(..64).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "synthetic section is too short"))?.to_vec();
    write_case(root, "versions (On 1-2-2026).one", current.clone())?;
    write_case(root, "versions (On 1-2-2025).one", previous)?;
    write_case(root, "broken.one", broken)?;
    Ok(())
}

fn write_case(root: &Path, name: &str, bytes: Vec<u8>) -> io::Result<()> {
    fs::write(root.join(name), bytes)
}

//! Handing a table to GridCraft, the spreadsheet beside this one.
//!
//! OneNote's Table tab ends with Convert to Excel Spreadsheet. NoteCraft has no Excel to
//! hand a table to, and would not reach for one: GridCraft is the open spreadsheet from
//! the same workshop, so the table goes there instead.
//!
//! Nothing here is required. When GridCraft is not installed the button is greyed and the
//! page carries on exactly as before; the table is written out as a CSV file beside the
//! page, which is a file anything can read, GridCraft included.

use notecraft_formats::markdown::Block;
use std::path::{Path, PathBuf};

/// Where GridCraft is, if it is installed.
///
/// Looked for in the places an installed app is: on the PATH, and in the folder the
/// Windows installer uses. Nothing is run to find out, so this is cheap enough to ask
/// every frame.
pub fn installed() -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") { "gridcraft.exe" } else { "gridcraft" };
    let on_path = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default();
    found_in(on_path.iter().chain(known_homes().iter()), name)
        // macOS keeps an app in a bundle, with the binary inside it.
        .or_else(|| {
            let bundle = PathBuf::from("/Applications/GridCraft.app/Contents/MacOS/gridcraft");
            bundle.is_file().then_some(bundle)
        })
}

/// The first of `dirs` that holds a file called `name`.
pub fn found_in<'a>(dirs: impl Iterator<Item = &'a PathBuf>, name: &str) -> Option<PathBuf> {
    dirs.map(|d| d.join(name)).find(|exe| exe.is_file())
}

/// The folders an installer puts GridCraft in, beside the PATH.
fn known_homes() -> Vec<PathBuf> {
    let mut homes = Vec::new();
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(dir) = std::env::var_os(var) {
            homes.push(PathBuf::from(&dir).join("GridCraft"));
            homes.push(PathBuf::from(dir).join("Programs").join("GridCraft"));
        }
    }
    homes.push(PathBuf::from("/usr/local/bin"));
    homes.push(PathBuf::from("/usr/bin"));
    homes
}

/// Writes the table beside the page and opens it in GridCraft.
///
/// The file stays: a spreadsheet saved over it is a file in the page's own folder, which
/// is where everything else a page carries lives.
pub fn hand_over(exe: &Path, page_dir: &Path, name: &str, block: &Block) -> std::io::Result<PathBuf> {
    let file = write_csv(page_dir, name, block)?;
    std::process::Command::new(exe).arg(&file).spawn()?;
    Ok(file)
}

/// Writes the table beside the page as `<name>.csv` and returns the file.
pub fn write_csv(page_dir: &Path, name: &str, block: &Block) -> std::io::Result<PathBuf> {
    let csv = notecraft_formats::csv::from_table(block)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "that block is not a table"))?;
    let file = page_dir.join(format!("{name}.csv"));
    std::fs::write(&file, csv)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_formats::markdown::parse;

    #[test]
    fn it_is_found_in_the_first_folder_that_holds_it() {
        let t = notecraft_store::testing::TempDir::new("gridcraft-find");
        let (empty, here) = (t.path().join("empty"), t.path().join("here"));
        std::fs::create_dir_all(&empty).expect("dir");
        std::fs::create_dir_all(&here).expect("dir");
        std::fs::write(here.join("gridcraft.exe"), b"not really").expect("exe");
        let dirs = [empty.clone(), here.clone()];
        assert_eq!(found_in(dirs.iter(), "gridcraft.exe"), Some(here.join("gridcraft.exe")));
        assert_eq!(found_in(dirs.iter(), "something-else.exe"), None, "nothing is invented");
        assert_eq!(found_in(std::iter::empty(), "gridcraft.exe"), None);
    }

    #[test]
    fn the_table_is_written_beside_the_page() {
        let t = notecraft_store::testing::TempDir::new("gridcraft-csv");
        let page = t.path().join("page");
        std::fs::create_dir_all(&page).expect("dir");
        let table = parse("| a | b |\n|---|---|\n| 1 | 2 |\n").remove(0);
        let file = write_csv(&page, "table-1", &table).expect("write");
        assert_eq!(file, page.join("table-1.csv"));
        assert_eq!(std::fs::read_to_string(&file).expect("read"), "a,b\n1,2\n");

        let para = parse("just words\n").remove(0);
        assert!(write_csv(&page, "nope", &para).is_err(), "a paragraph is not a table");
    }
}

//! Handing a table to GridCraft, the spreadsheet beside this one.
//!
//! OneNote's Table tab ends with Convert to Excel Spreadsheet. NoteCraft has no Excel to
//! hand a table to, and would not reach for one: GridCraft is the open spreadsheet from
//! the same workshop, so the table goes there instead.
//!
//! Nothing here is required. When GridCraft is not installed the button is greyed and the
//! page carries on exactly as before; the table is written out as a CSV file beside the
//! page, which is a file anything can read, GridCraft included.

use notecraft_formats::markdown::{Block, Span};
use std::path::{Path, PathBuf};

/// Where GridCraft is, if it is installed.
///
/// Looked for in the places an installed app is: on the PATH, and in the folder the
/// Windows installer uses. Nothing is run to find out, so this is cheap enough to ask
/// every frame.
pub fn installed() -> Option<PathBuf> {
    let name = if cfg!(target_os = "windows") { "gridcraft.exe" } else { "gridcraft" };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            let exe = dir.join(name);
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    for dir in known_homes() {
        let exe = dir.join(name);
        if exe.is_file() {
            return Some(exe);
        }
    }
    // macOS keeps an app in a bundle, with the binary inside it.
    let bundle = PathBuf::from("/Applications/GridCraft.app/Contents/MacOS/gridcraft");
    bundle.is_file().then_some(bundle)
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

/// The table as CSV: one line per row, a comma between cells.
///
/// The marks inside a cell are dropped, because a spreadsheet cell holds a value, not a
/// marked-up line. Quotes are doubled and a cell is wrapped when it holds a comma, a
/// quote or a line break, which is the rule everything that reads CSV agrees on.
pub fn as_csv(block: &Block) -> Option<String> {
    let Block::Table { head, rows } = block else { return None };
    let mut out = String::new();
    for row in std::iter::once(head).chain(rows.iter()) {
        let line: Vec<String> = row.iter().map(|cell| quote(&plain(cell))).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    Some(out)
}

/// The words of a cell without the marks round them.
fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn quote(cell: &str) -> String {
    if cell.contains([',', '"', '\n', '\r']) { format!("\"{}\"", cell.replace('"', "\"\"")) } else { cell.to_string() }
}

/// Writes the table beside the page and opens it in GridCraft.
///
/// The file stays: a spreadsheet saved over it is a file in the page's own folder, which
/// is where everything else a page carries lives.
pub fn hand_over(exe: &Path, page_dir: &Path, name: &str, block: &Block) -> std::io::Result<PathBuf> {
    let csv = as_csv(block).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "that block is not a table"))?;
    let file = page_dir.join(format!("{name}.csv"));
    std::fs::write(&file, csv)?;
    std::process::Command::new(exe).arg(&file).spawn()?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_formats::markdown::parse;

    #[test]
    fn a_table_goes_out_as_csv_a_spreadsheet_can_read() {
        let table = parse("| Name | Note |\n|---|---|\n| *Ada* | one, two |\n| \"Q\" | |\n").remove(0);
        let csv = as_csv(&table).expect("a table");
        assert_eq!(csv, "Name,Note\nAda,\"one, two\"\n\"\"\"Q\"\"\",\n", "marks come off, commas and quotes are wrapped:\n{csv}");
    }

    #[test]
    fn anything_that_is_not_a_table_is_left_alone() {
        let para = parse("just words\n").remove(0);
        assert_eq!(as_csv(&para), None);
    }
}

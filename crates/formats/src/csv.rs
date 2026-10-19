//! A table as CSV, for handing it to a spreadsheet.
//!
//! The marks inside a cell are dropped, because a spreadsheet cell holds a value, not a
//! marked-up line. Quotes are doubled and a cell is wrapped when it holds a comma, a
//! quote or a line break, which is the rule everything that reads CSV agrees on.

use crate::markdown::{Block, Span};

/// The table as CSV, or `None` for a block that is not a table.
pub fn from_table(block: &Block) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse;

    #[test]
    fn a_table_goes_out_as_csv_a_spreadsheet_can_read() {
        let table = parse("| Name | Note |\n|---|---|\n| *Ada* | one, two |\n| \"Q\" | |\n").remove(0);
        let csv = from_table(&table).expect("a table");
        assert_eq!(csv, "Name,Note\nAda,\"one, two\"\n\"\"\"Q\"\"\",\n", "marks come off, commas and quotes are wrapped:\n{csv}");
    }

    #[test]
    fn anything_that_is_not_a_table_is_left_alone() {
        assert_eq!(from_table(&parse("just words\n").remove(0)), None);
    }
}

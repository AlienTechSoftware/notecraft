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

/// A table read back from CSV: the first row is the heading, as it was written out.
///
/// The rule everything that writes CSV agrees on: a cell may be wrapped in quotes, a
/// doubled quote inside one is a quote, and a wrapped cell may hold commas and line
/// breaks. Anything else is taken as it stands rather than refused, because a spreadsheet
/// that wrote it is not going to be argued with.
pub fn to_table(text: &str) -> Option<Block> {
    let rows = rows_of(text);
    let mut rows = rows.into_iter().filter(|r| !(r.len() == 1 && r[0].trim().is_empty()));
    let head: Vec<Vec<Span>> = rows.next()?.iter().map(|c| crate::markdown::parse_inline(c)).collect();
    let body: Vec<Vec<Vec<Span>>> = rows.map(|r| r.iter().map(|c| crate::markdown::parse_inline(c)).collect()).collect();
    Some(Block::Table { head, rows: body })
}

/// The cells of each line, with quotes undone.
fn rows_of(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    cell.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            '"' if cell.trim().is_empty() => {
                cell.clear();
                quoted = true;
            }
            ',' if !quoted => row.push(std::mem::take(&mut cell)),
            '\r' if !quoted => {}
            '\n' if !quoted => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            other => cell.push(other),
        }
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        rows.push(row);
    }
    rows
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
    fn a_table_comes_back_from_the_csv_a_spreadsheet_saved() {
        let table = parse(
            "| Name | Note |
|---|---|
| Ada | one, two |
| Q | |
",
        )
        .remove(0);
        let csv = from_table(&table).expect("a table");
        let back = to_table(&csv).expect("read back");
        assert_eq!(
            back, table,
            "what went out comes back:
{csv}"
        );

        // What a spreadsheet writes: quotes round what needs them, a line break inside a
        // cell, and a last line with no newline on the end.
        let from_sheet = "a,b
\"one, two\",\"she said \"\"hi\"\"\"
\"two
lines\",last";
        let Some(Block::Table { head, rows }) = to_table(from_sheet) else { panic!("a table") };
        assert_eq!(head.len(), 2);
        assert_eq!(rows.len(), 2, "two rows under the heading");
        let text = |cell: &Vec<Span>| cell.iter().map(|s| s.text.as_str()).collect::<String>();
        assert_eq!(text(&rows[0][0]), "one, two");
        assert_eq!(text(&rows[0][1]), "she said \"hi\"");
        assert_eq!(
            text(&rows[1][0]),
            "two
lines",
            "a cell may hold a line break"
        );
        assert_eq!(text(&rows[1][1]), "last");
    }

    #[test]
    fn nothing_in_nothing_out() {
        assert_eq!(to_table(""), None);
        assert_eq!(
            to_table(
                "   

"
            ),
            None
        );
    }

    #[test]
    fn anything_that_is_not_a_table_is_left_alone() {
        assert_eq!(from_table(&parse("just words\n").remove(0)), None);
    }
}

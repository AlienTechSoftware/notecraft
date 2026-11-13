use notecraft_formats::{csv, html, markdown, render};

#[test]
fn multiline_csv_cells_remain_in_one_row_in_both_page_formats() {
    for value in
        ["a\nb", "a\r\nb", "a\rb", "a\r\nb\rc\nd", "\na\n\nb\n", "<br data-nc-break=\"crlf\">\nsecond", "caf\u{e9}\n*literal* | `code` <br> \\path"]
    {
        let quoted = value.replace('"', "\"\"");
        let source = format!("Value,Other\n\"{quoted}\",untouched\nlast,row\n");
        let original = csv::to_table(&source).expect("CSV table");
        for format in ["cmgfm", "html"] {
            let mut blocks = vec![original.clone()];
            for _ in 0..3 {
                let written =
                    if format == "html" { html::to_markdown(&render::blocks_to_html(&blocks)) } else { render::blocks_to_markdown(&blocks) };
                blocks = markdown::parse(&written);
                assert_eq!(blocks, vec![original.clone()], "{format}: {value:?}\n{written}");
                assert_eq!(csv::from_table(&blocks[0]).expect("table"), source);
            }
        }
    }
}

#[test]
fn a_literal_break_tag_and_inline_code_are_not_changed() {
    assert_eq!(render::spans_to_markdown(&markdown::parse_inline("a<br>b")), "a<br>b");
    let original = markdown::parse("a<br>b\n\n| Value |\n| --- |\n| `<br>` |\n");
    assert_eq!(render::blocks_to_text(&original), "a<br>b\nValue\n<br>");
    assert_eq!(markdown::parse(&render::blocks_to_markdown(&original)), original);
    assert_eq!(markdown::parse(&html::to_markdown(&render::blocks_to_html(&original))), original);
}

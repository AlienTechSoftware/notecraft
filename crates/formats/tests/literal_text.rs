use notecraft_formats::{Block, Span, blocks_to_html, blocks_to_markdown};
use notecraft_formats::{html, markdown};

#[test]
fn literal_punctuation_and_block_starts_round_trip_in_both_formats() {
    for text in
        ["a * b ** c [x](y) $z$ <tag> & `tick` \\ end", "# title", "1. list", "- bullet", "---", "----", "-----", "> quote", "مرحبا mundo 世界"]
    {
        let blocks = vec![Block::Paragraph(vec![Span { text: text.into(), ..Default::default() }])];
        assert_eq!(markdown::parse(&blocks_to_markdown(&blocks)), blocks, "Markdown: {text}");
        assert_eq!(markdown::parse(&html::to_markdown(&blocks_to_html(&blocks))), blocks, "HTML: {text}");
    }
}

#[test]
fn escaped_delimiters_at_style_boundaries_stay_literal() {
    for span in [
        Span { text: "bold*".into(), bold: true, ..Default::default() },
        Span { text: "italic*".into(), italic: true, ..Default::default() },
        Span { text: "bracket]".into(), link: Some("https://example.org".into()), ..Default::default() },
    ] {
        let blocks = vec![Block::Paragraph(vec![span])];
        assert_eq!(markdown::parse(&blocks_to_markdown(&blocks)), blocks);
        assert_eq!(markdown::parse(&html::to_markdown(&blocks_to_html(&blocks))), blocks);
    }
}

#[test]
fn backslash_inside_inline_code_does_not_escape_closing_tick() {
    let spans = markdown::parse_inline("`path\\` after");
    assert_eq!(spans[0].text, "path\\");
    assert!(spans[0].code);
    assert_eq!(spans[1].text, " after");
}

#[test]
fn hyphens_with_trailing_whitespace_remain_paragraphs() {
    for text in ["--- ", "----\t", " -----  "] {
        let blocks = vec![Block::Paragraph(vec![Span { text: text.into(), ..Default::default() }])];
        for body in [blocks_to_markdown(&blocks), html::to_markdown(&blocks_to_html(&blocks))] {
            let parsed = markdown::parse(&body);
            assert!(matches!(parsed.first(), Some(Block::Paragraph(_))), "{text:?} became {parsed:?}");
        }
    }
}

#[test]
fn nonheading_hashtag_keeps_the_native_tag_prefix() {
    let blocks = vec![Block::Paragraph(vec![Span { text: "#important Keep this".into(), ..Default::default() }])];
    let body = blocks_to_markdown(&blocks);
    assert_eq!(body, "#important Keep this\n");
    assert_eq!(markdown::parse(&body), blocks);
}

#[test]
fn ordinary_punctuation_is_saved_exactly_as_typed() {
    for text in
        ["snake_case and x < y > z", "costs $5", "2*3 is six", "C:\\Users\\reader", "~approx", "see [1]", "https://e.com/a_b", "On **File > Open**"]
    {
        let blocks = vec![Block::Paragraph(markdown::parse_inline(text))];
        let written = blocks_to_markdown(&blocks);
        assert_eq!(written, format!("{text}\n"), "{text} must not gain backslashes");
        assert_eq!(markdown::parse(&written), blocks);
    }
}

#[test]
fn text_that_would_become_markup_is_quoted_and_reads_back() {
    for text in
        ["$5 or $6", "2*3*4", "a ~~b~~ c", "`a` and `b`", "[x](y)", "C:\\path\\*", "> not a quote", "* not a bullet", "___", "<img src=\"x\">"]
    {
        let blocks = vec![Block::Paragraph(vec![Span { text: text.into(), ..Default::default() }])];
        assert_eq!(markdown::parse(&blocks_to_markdown(&blocks)), blocks, "Markdown: {text}");
        assert_eq!(markdown::parse(&html::to_markdown(&blocks_to_html(&blocks))), blocks, "HTML: {text}");
    }
}

#[test]
fn a_backslash_before_a_letter_is_a_backslash() {
    // CommonMark: only punctuation can be escaped. Pages written before quoting existed kept
    // Windows paths as typed; they read back as typed.
    let spans = markdown::parse_inline("C:\\Users\\reader and \\*literal\\*");
    let text: String = spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(text, "C:\\Users\\reader and *literal*");
}

#[test]
fn literal_inline_tags_survive_inside_styled_text() {
    for text in ["x</u>y", "literal <u>words</u>", "x</span>y", "<SPAN style=\"font-size: 18pt\">x</SPAN>"] {
        for span in
            [Span { text: text.into(), underline: true, ..Default::default() }, Span { text: text.into(), font_size: Some(18), ..Default::default() }]
        {
            let original = vec![Block::Paragraph(vec![span])];
            for format in notecraft_model::PageFormat::ALL {
                let mut blocks = original.clone();
                for _ in 0..3 {
                    blocks = notecraft_formats::boxes::read(format, &notecraft_formats::boxes::write(format, &blocks, None)).0;
                    assert_eq!(blocks, original, "{format:?}: {text:?}");
                }
            }
        }
    }
}

#[test]
fn table_pipes_do_not_split_literal_or_formatted_cells() {
    for span in [
        Span { text: "a|b".into(), ..Default::default() },
        Span { text: r"a\|b".into(), ..Default::default() },
        Span { text: "a|b".into(), code: true, ..Default::default() },
        Span { text: r"a\|b".into(), code: true, ..Default::default() },
        Span { text: r"a\|b".into(), math: true, ..Default::default() },
        Span { text: "label".into(), link: Some(r"path\with|pipe".into()), ..Default::default() },
    ] {
        let original = vec![Block::Table { head: vec![vec![span]], rows: vec![] }];
        for format in notecraft_model::PageFormat::ALL {
            let mut blocks = original.clone();
            for _ in 0..3 {
                let body = notecraft_formats::boxes::write(format, &blocks, None);
                blocks = notecraft_formats::boxes::read(format, &body).0;
                assert_eq!(blocks, original, "{format:?}: {body}");
            }
        }
    }
}

//! What a person typed stays what it was after the page is saved and opened again: words that
//! look like the syntax of a block (a task box, a heading mark, a quote mark, a box's closing tag)
//! must not turn into that block, and links written before quoting existed keep working.
use notecraft_formats::{Block, Span, boxes, markdown};
use notecraft_model::{Container, PageFormat};

fn words(text: &str) -> Vec<Span> {
    vec![Span { text: text.into(), ..Default::default() }]
}

/// Saved and read back the way the app does it, in both page formats.
fn reopened(format: PageFormat, blocks: &[Block], boxes: Option<&[Container]>) -> (Vec<Block>, Option<Vec<Container>>) {
    boxes::read(format, &boxes::write(format, blocks, boxes))
}

#[test]
fn a_windows_folder_link_stays_a_link() {
    for line in ["[Folder](C:\\Notes\\)", "see [the notes](C:\\Users\\reader\\Notes\\) here"] {
        let spans = markdown::parse_inline(line);
        let link = spans.iter().find(|s| s.link.is_some()).unwrap_or_else(|| panic!("no link in {line:?}: {spans:?}"));
        assert!(link.link.as_deref().is_some_and(|to| to.ends_with("Notes\\")), "{line:?} -> {spans:?}");
        for format in PageFormat::ALL {
            let blocks = vec![Block::Paragraph(spans.clone())];
            assert_eq!(reopened(format, &blocks, None).0, blocks, "{format:?}: {line:?}");
        }
    }
}

#[test]
fn bullet_text_that_looks_like_a_task_box_stays_text() {
    for format in PageFormat::ALL {
        for (task, text) in [(None, "[x] shipped"), (None, "[ ] not a box"), (Some(false), "[X] still words")] {
            let blocks = vec![Block::Item { ordered: false, number: 0, task, depth: 0, spans: words(text) }];
            assert_eq!(reopened(format, &blocks, None).0, blocks, "{format:?}: {text:?}");
        }
    }
}

#[test]
fn heading_and_quote_text_keep_their_leading_marks() {
    for format in PageFormat::ALL {
        let blocks = vec![
            Block::Heading { level: 2, spans: words("#hashtag plans") },
            Block::Quote(words("> quoted inside a quote")),
            Block::Paragraph(words("after")),
        ];
        assert_eq!(reopened(format, &blocks, None).0, blocks, "{format:?}");
    }
}

#[test]
fn a_closing_box_tag_typed_in_a_box_stays_in_that_box() {
    for format in PageFormat::ALL {
        for text in ["</div>", "<div class=\"nc-box\" data-x=\"1\" data-y=\"1\" data-width=\"9\">"] {
            let blocks = vec![Block::Paragraph(words(text)), Block::Paragraph(words("next paragraph"))];
            let one_box = [Container::new(10.0, 20.0, 300.0, 2)];
            let (read, boxes) = reopened(format, &blocks, Some(&one_box));
            assert_eq!(read, blocks, "{format:?}: {text:?}");
            assert_eq!(boxes.map(|b| b.len()), Some(1), "{format:?}: {text:?} must not split or end the box");
        }
    }
}

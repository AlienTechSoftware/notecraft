//! Review > Check Accessibility (issue #520): what on the open page a reader using a screen reader
//! or the keyboard would stumble on, each finding tied to the block it is in so the page can be
//! taken there, and checked again as the page changes.
//!
//! The checks work on the page's blocks, so they are the same for both page formats:
//! - a picture with no description, as a block or inside a line;
//! - a link whose words do not say where it goes ("click here", "more", or none at all);
//! - a heading with no words, and a heading that skips a level below the one before it;
//! - a table whose header row is empty.

use notecraft_formats::Block;
use notecraft_formats::markdown::Span;

/// What is wrong, by kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    PictureWithoutDescription,
    UnclearLink,
    EmptyHeading,
    SkippedHeadingLevel,
    TableWithoutHeader,
}

impl Kind {
    /// The name the window and Inspect use for it.
    pub fn id(self) -> &'static str {
        match self {
            Kind::PictureWithoutDescription => "picture-without-description",
            Kind::UnclearLink => "unclear-link",
            Kind::EmptyHeading => "empty-heading",
            Kind::SkippedHeadingLevel => "skipped-heading-level",
            Kind::TableWithoutHeader => "table-without-header",
        }
    }

    /// What it is, for the list.
    pub fn title(self) -> &'static str {
        match self {
            Kind::PictureWithoutDescription => crate::tl_id!("ui.a11y_picture_without_description"),
            Kind::UnclearLink => crate::tl_id!("ui.a11y_unclear_link"),
            Kind::EmptyHeading => crate::tl_id!("ui.a11y_empty_heading"),
            Kind::SkippedHeadingLevel => crate::tl_id!("ui.a11y_skipped_heading_level"),
            Kind::TableWithoutHeader => crate::tl_id!("ui.a11y_table_without_header"),
        }
    }

    /// How to put it right.
    pub fn how(self) -> &'static str {
        match self {
            Kind::PictureWithoutDescription => crate::tl_id!("ui.a11y_picture_without_description_how"),
            Kind::UnclearLink => crate::tl_id!("ui.a11y_unclear_link_how"),
            Kind::EmptyHeading => crate::tl_id!("ui.a11y_empty_heading_how"),
            Kind::SkippedHeadingLevel => crate::tl_id!("ui.a11y_skipped_heading_level_how"),
            Kind::TableWithoutHeader => crate::tl_id!("ui.a11y_table_without_header_how"),
        }
    }
}

/// One thing found: its kind, the block it is in, and the words it is about, when there are any
/// (the link's words, the picture's file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub kind: Kind,
    pub block: usize,
    pub about: String,
}

/// Link words that say nothing of where the link goes, compared without case or the marks round
/// them.
const UNCLEAR: [&str; 16] = [
    "click here",
    "click",
    "here",
    "link",
    "this link",
    "this",
    "more",
    "read more",
    "learn more",
    "see more",
    "go",
    "go here",
    "page",
    "this page",
    "website",
    "details",
];

/// Everything found on the page, in the order of its blocks.
pub fn check(blocks: &[Block]) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut last_level: Option<u8> = None;
    for (at, block) in blocks.iter().enumerate() {
        match block {
            Block::Image { alt, src, .. } if alt.trim().is_empty() => {
                found.push(Finding { kind: Kind::PictureWithoutDescription, block: at, about: file_name(src) });
            }
            Block::Heading { level, spans, .. } => {
                let words: String = spans.iter().map(|s| s.text.as_str()).collect();
                if words.trim().is_empty() {
                    found.push(Finding { kind: Kind::EmptyHeading, block: at, about: String::new() });
                } else if let Some(before) = last_level
                    && *level > before + 1
                {
                    found.push(Finding { kind: Kind::SkippedHeadingLevel, block: at, about: format!("{before} \u{2192} {level}") });
                }
                last_level = Some(*level);
                spans_findings(spans, at, &mut found);
            }
            Block::Table { head, rows, .. } => {
                if head.iter().all(|cell| cell.iter().all(|s| s.text.trim().is_empty() && s.image.is_none())) {
                    found.push(Finding { kind: Kind::TableWithoutHeader, block: at, about: String::new() });
                }
                for cell in head.iter().chain(rows.iter().flatten()) {
                    spans_findings(cell, at, &mut found);
                }
            }
            Block::Paragraph(spans, _) | Block::Quote(spans, _) | Block::Item { spans, .. } => spans_findings(spans, at, &mut found),
            _ => {}
        }
    }
    found
}

/// The pictures and links inside a line.
fn spans_findings(spans: &[Span], block: usize, found: &mut Vec<Finding>) {
    for span in spans {
        if let Some(src) = &span.image {
            if span.text.trim().is_empty() {
                found.push(Finding { kind: Kind::PictureWithoutDescription, block, about: file_name(src) });
            }
            continue;
        }
        if span.link.is_some() {
            let words: String =
                span.text.trim().trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
            if words.is_empty() || UNCLEAR.contains(&words.as_str()) {
                found.push(Finding { kind: Kind::UnclearLink, block, about: span.text.trim().to_string() });
            }
        }
    }
}

fn file_name(src: &str) -> String {
    src.rsplit(['/', '\\']).next().unwrap_or(src).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks(md: &str) -> Vec<Block> {
        notecraft_formats::markdown::parse(md)
    }

    /// Issue #520: each kind of finding is found in the block it is in; a page with none of them
    /// has nothing found.
    #[test]
    fn the_checker_finds_what_a_screen_reader_would_stumble_on() {
        let page = blocks(
            "# Plan\n\n![](assets/chart.png)\n\nSee [click here](https://a.test) and [the budget](https://b.test).\n\n### Too deep\n\n![a pie chart](assets/pie.png)\n\nA picture inline ![](assets/dot.png) here.\n",
        );
        let found = check(&page);
        let kinds: Vec<(Kind, usize)> = found.iter().map(|f| (f.kind, f.block)).collect();
        assert_eq!(
            kinds,
            [(Kind::PictureWithoutDescription, 1), (Kind::UnclearLink, 2), (Kind::SkippedHeadingLevel, 3), (Kind::PictureWithoutDescription, 5)],
            "{found:?}"
        );
        assert_eq!(found[0].about, "chart.png");
        assert_eq!(found[1].about, "click here");
        let table = blocks("|  |  |\n|---|---|\n| a | b |\n");
        assert_eq!(check(&table).iter().map(|f| f.kind).collect::<Vec<_>>(), [Kind::TableWithoutHeader]);
        let fine = blocks("# Plan\n\n## Steps\n\nRead [the budget](https://b.test).\n\n![a pie chart](assets/pie.png)\n");
        assert!(check(&fine).is_empty(), "{:?}", check(&fine));
    }
}

//! The emoji a block header holds are part of the block, so both page formats must keep them.

use notecraft_formats::{html, markdown, render};

fn emoji(list: &[&str]) -> Vec<String> {
    list.iter().map(|e| e.to_string()).collect()
}

#[test]
fn emoji_survive_a_round_trip_through_cmgfm() {
    let mut blocks = markdown::parse("Hello there\n");
    blocks[0].set_emoji(emoji(&["\u{1F600}", "\u{1F44D}"]));
    let written = render::blocks_to_markdown(&blocks);
    let back = markdown::parse(&written);
    assert_eq!(back[0].emoji(), emoji(&["\u{1F600}", "\u{1F44D}"]).as_slice(), "written as: {written}");
    assert_eq!(back[0], blocks[0], "the rest of the block reads back unchanged");
}

#[test]
fn emoji_survive_a_round_trip_through_html() {
    let mut blocks = markdown::parse("Hello there\n");
    blocks[0].set_emoji(emoji(&["\u{1F389}"]));
    let written = render::blocks_to_html(&blocks);
    let back = markdown::parse(&html::to_markdown(&written));
    assert_eq!(back[0].emoji(), emoji(&["\u{1F389}"]).as_slice(), "written as: {written}");
}

#[test]
fn a_block_without_emoji_is_written_exactly_as_before() {
    let blocks = markdown::parse("# Title\n\nA paragraph.\n");
    assert!(blocks.iter().all(|b| b.emoji().is_empty()));
    let written = render::blocks_to_markdown(&blocks);
    assert!(!written.contains("emoji"), "no emoji marker is written: {written}");
    assert_eq!(markdown::parse(&written), blocks);
}

#[test]
fn every_kind_of_block_can_hold_emoji() {
    for src in ["# Heading\n", "A paragraph\n", "- an item\n", "> a quote\n", "```\ncode\n```\n", "---\n"] {
        let mut blocks = markdown::parse(src);
        blocks[0].set_emoji(emoji(&["\u{2705}"]));
        let back = markdown::parse(&render::blocks_to_markdown(&blocks));
        assert_eq!(back[0].emoji(), emoji(&["\u{2705}"]).as_slice(), "kind of block from {src:?}");
    }
}

#[test]
fn emoji_on_a_paragraph_and_a_list_item_survive_another_block_being_written() {
    // The owner hides a paragraph's and a list item's emoji from the display, but the data stays.
    let mut blocks = markdown::parse("First paragraph\n\nA paragraph\n\n- an item\n\nLast line\n");
    blocks[1].set_emoji(emoji(&["\u{1F642}"]));
    blocks[2].set_emoji(emoji(&["\u{1F3AF}"]));
    let written = render::blocks_to_markdown(&blocks);
    let back = markdown::parse(&written);
    assert_eq!(back[3], blocks[3], "the last line reads back unchanged");
    assert_eq!(back[1].emoji(), emoji(&["\u{1F642}"]).as_slice(), "paragraph emoji kept: {written}");
    assert_eq!(back[2].emoji(), emoji(&["\u{1F3AF}"]).as_slice(), "list item emoji kept: {written}");
}

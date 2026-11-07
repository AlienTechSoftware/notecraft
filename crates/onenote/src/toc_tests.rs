use crate::Guid;
use crate::store::build::Toc as TocBuilder;

#[test]
fn reads_notebook_and_section_colors_and_keeps_the_identity_link() {
    let first_identity = Guid([0x11; 16]);
    let second_identity = Guid([0x22; 16]);
    let data = TocBuilder::new()
        .with_color(0x0097_982E)
        .section("First.one", first_identity, Some(0x0012_3456))
        .section("Second.one", second_identity, Some(u32::MAX))
        .finish();

    let toc = crate::read_toc(&data).expect("read synthetic TOC");
    assert_eq!(toc.color, Some(0x0097_982E));
    assert_eq!(toc.sections.len(), 2);
    assert_eq!(toc.sections[0].filename, "First.one");
    assert_eq!(toc.sections[0].identity, Some(first_identity));
    assert_eq!(toc.sections[0].order, Some(0));
    assert_eq!(toc.sections[0].color, Some(0x0012_3456));
    assert_eq!(toc.sections[1].filename, "Second.one");
    assert_eq!(toc.sections[1].identity, Some(second_identity));
    assert_eq!(toc.sections[1].color, None, "the undefined sentinel is ignored");
}

#[test]
fn a_section_file_is_not_accepted_as_a_notebook_toc() {
    let section = crate::store::build::OneNote::new().with_page("Synthetic", &[crate::store::build::OneNote::line("content")]).finish();
    assert!(matches!(crate::read_toc(&section), Err(crate::OneError::NotAToc(_))));
}

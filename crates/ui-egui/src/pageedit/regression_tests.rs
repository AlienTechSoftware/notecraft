//! Regression coverage for literal text at persistence and live-editing boundaries.
use super::*;
use notecraft_model::PageFormat;

fn literal(text: &str) -> Block {
    Block::Paragraph(vec![Span { text: text.into(), ..Default::default() }])
}

#[test]
fn entering_and_leaving_literal_text_does_not_add_storage_escapes() {
    let original = r"世界 *literal* $5 or $6 [x](y) C:\Users\angel \*";
    let mut block = literal(original);
    for suffix in ["!", "!!", "!!!"] {
        assert_eq!(caret_text(&block), format!("{original}{}", "!".repeat(suffix.len() - 1)));
        set_caret_text(&mut block, &format!("{original}{suffix}"));
        assert_eq!(block, literal(&format!("{original}{suffix}")));
        for format in PageFormat::ALL {
            assert_eq!(preview::blocks_for(format, &to_body(format, &[block.clone()])), [block.clone()]);
        }
    }
}

#[test]
fn literal_image_syntax_stays_text_beside_a_real_image() {
    let mut block = Block::Paragraph(vec![
        Span { text: "![picture](asset.png) ".into(), ..Default::default() },
        Span { text: "picture".into(), image: Some("asset.png".into()), ..Default::default() },
    ]);
    let wanted = "![picture](asset.png) \u{fffc}";
    assert_eq!(caret_text(&block), wanted);
    set_caret_text(&mut block, &format!("{wanted}!"));
    let Block::Paragraph(spans) = block else { panic!("paragraph") };
    assert_eq!(spans.iter().filter(|s| s.image.is_some()).count(), 1);
    assert_eq!(spans[0].text, "![picture](asset.png) ");
}

#[test]
fn unicode_click_offsets_include_font_tags_but_no_literal_escapes() {
    let spans = vec![
        Span { text: "*literal* ".into(), ..Default::default() },
        Span { text: "مرحبا".into(), font_size: Some(18), underline: true, ..Default::default() },
    ];
    let source = block_text(&Block::Paragraph(spans.clone()));
    for (at, character) in "*literal* مرحبا".chars().enumerate() {
        assert_eq!(source.chars().nth(over_marks(&spans, at)), Some(character), "character {at}");
    }
}

#[test]
fn changing_kind_and_task_tag_preserves_literal_spans() {
    let original = literal(r"*literal* $5 or $6 [x](y) C:\Users\name");
    let mut blocks = vec![original.clone()];
    assert!(set_kind(&mut blocks, 0, "bullet"));
    assert!(set_kind(&mut blocks, 0, "bullet"));
    assert_eq!(blocks.as_slice(), std::slice::from_ref(&original));
    assert!(set_tag(&mut blocks, 0, Tag::ToDo));
    assert!(set_tag(&mut blocks, 0, Tag::ToDo));
    assert_eq!(blocks, [original]);
}

#[test]
fn splitting_and_joining_preserves_both_literal_halves() {
    let original = "before *literal* $5 or $6";
    let mut blocks = vec![literal(original)];
    let mut caret = Some(Caret { text: original.into(), selection: (7, 7), ..Default::default() });
    let mut changed = Changed::default();
    apply(Action::Split(0), &mut blocks, &mut caret, &mut changed);
    assert_eq!(blocks, [literal("before "), literal("*literal* $5 or $6")]);
    apply(Action::JoinBack(1), &mut blocks, &mut caret, &mut changed);
    assert_eq!(block_text(&blocks[0]), original);
    let Block::Paragraph(spans) = &blocks[0] else { panic!("paragraph") };
    assert!(spans.iter().all(|s| !s.italic && !s.math && s.link.is_none()));
}

#[test]
fn editing_and_reentering_a_table_cell_preserves_literal_text() {
    let original = r"*literal* C:\Users\name $5 or $6";
    let mut blocks = vec![Block::Table { head: vec![vec![Span { text: original.into(), ..Default::default() }]], rows: vec![] }];
    let mut caret = None;
    let mut changed = Changed::default();
    for suffix in ["!", "!!"] {
        apply(Action::EnterCell(0, (0, 0)), &mut blocks, &mut caret, &mut changed);
        let text = format!("{original}{suffix}");
        caret.as_mut().expect("cell caret").text = text.clone();
        apply(Action::SetCell(0, (0, 0), text.clone()), &mut blocks, &mut caret, &mut changed);
        apply(Action::Leave, &mut blocks, &mut caret, &mut changed);
        assert_eq!(cell_text(&blocks[0], (0, 0)), Some(text.clone()));
        let Block::Table { head, .. } = &blocks[0] else { panic!("table") };
        assert_eq!(head[0], [Span { text, ..Default::default() }]);
    }
}

fn frame(ctx: &egui::Context, blocks: &mut Vec<Block>, caret: &mut Option<Caret>, events: Vec<egui::Event>) {
    let tokens = crate::theme::Theme { mode: crate::theme::Mode::Light, accent: crate::theme::Accent::Default }.tokens();
    let mut output = ctx.run_ui(egui::RawInput { events, ..Default::default() }, |ui| {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            show(ui, blocks, caret, std::path::Path::new("."), &tokens, None, false, &mut Default::default(), &Default::default(), &mut None, "");
        });
    });
    output.textures_delta.clear();
    fn check(shape: &egui::Shape) {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text.contains("*literal*") => {
                assert!(text.galley.job.sections.iter().all(|s| !s.format.italics), "literal stars must stay literal in the widget");
            }
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    check(shape)
                }
            }
            _ => {}
        }
    }
    for shape in &output.shapes {
        check(&shape.shape)
    }
}

#[test]
fn the_real_widget_keeps_literals_across_edits_and_accepts_new_markdown() {
    let original = "世界 *literal* $5 or $6 [x](y) ";
    let mut blocks = vec![literal(original)];
    let mut caret = Some(Caret { text: caret_text(&blocks[0]), focus: true, ..Default::default() });
    let ctx = egui::Context::default();
    frame(&ctx, &mut blocks, &mut caret, vec![]);
    for c in "**bold**".chars() {
        frame(&ctx, &mut blocks, &mut caret, vec![egui::Event::Text(c.to_string())]);
    }
    let mut changed = Changed::default();
    apply(Action::Leave, &mut blocks, &mut caret, &mut changed);
    let Block::Paragraph(spans) = &blocks[0] else { panic!("paragraph") };
    assert_eq!(spans.iter().map(|s| s.text.as_str()).collect::<String>(), format!("{original}bold"));
    assert!(spans.iter().any(|s| s.bold && s.text == "bold"));
    apply(Action::Move(0), &mut blocks, &mut caret, &mut changed);
    assert_eq!(caret.as_ref().expect("returned caret").text, format!("{original}**bold**"));
}

#[test]
fn replacing_a_selected_literal_uses_the_new_characters_identity() {
    let mut blocks = vec![literal("*literal*")];
    let mut caret = Some(Caret { text: caret_text(&blocks[0]), focus: true, ..Default::default() });
    let ctx = egui::Context::default();
    frame(&ctx, &mut blocks, &mut caret, vec![]);
    ctx.data_mut(|data| data.insert_temp(restore_selection_id(), (0usize, 0usize, 9usize)));
    frame(&ctx, &mut blocks, &mut caret, vec![]);
    frame(&ctx, &mut blocks, &mut caret, vec![egui::Event::Text("**bold**".into())]);
    let Block::Paragraph(spans) = &blocks[0] else { panic!("paragraph") };
    assert_eq!(spans.as_slice(), &[Span { text: "bold".into(), bold: true, ..Default::default() }]);
}

#[test]
fn deleting_across_blocks_keeps_literal_identity_from_the_last_block() {
    let literal = "*literal* $5 or $6";
    let mut blocks = vec![
        Block::Paragraph(vec![Span { text: "before left".into(), ..Default::default() }]),
        Block::Paragraph(vec![Span { text: literal.into(), ..Default::default() }]),
    ];
    let (block, at) = delete_span(&mut blocks, Across { from: (0, "before ".len()), to: (1, 0) });
    assert_eq!((block, at), (0, "before ".chars().count()));
    assert_eq!(blocks, vec![Block::Paragraph(vec![Span { text: format!("before {literal}"), ..Default::default() }])]);
}

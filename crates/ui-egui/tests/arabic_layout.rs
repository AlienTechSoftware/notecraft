//! What egui 0.36 does with Arabic, measured rather than assumed, and what NoteCraft has to
//! undo because of it. If one of these tests starts failing, egui has changed underneath us
//! and `char_at` (and `docs/gaps.md`) should be revisited.

use egui_kittest::Harness;
use notecraft_model::Dir;
use notecraft_ui_egui::{fonts, pageedit, pagetext, theme::Theme};

/// A harness with NoteCraft's fonts in it, which is what puts Arabic glyphs there at all.
fn harness() -> Harness<'static> {
    let mut h = Harness::builder().with_size(egui::vec2(600.0, 200.0)).build_ui(|ui| fonts::install_once(ui.ctx()));
    h.run();
    h
}

/// One of the owner's test sentences: plain Arabic, an Arabic-Indic digit, a full stop.
const LINE: &str = "١. هذا اختبار بسيط لعرض الجمل العربية بشكل صحيح.";

#[test]
fn egui_lays_an_arabic_row_out_to_the_left_of_the_origin() {
    let t = Theme::DEFAULT.tokens();
    let h = harness();
    let latin = h.ctx.fonts_mut(|f| f.layout_job(pagetext::inline_job("Hello world", 16.0, false, &t, 400.0, Dir::Ltr)));
    assert_eq!(latin.rect.min.x, 0.0, "a left-to-right row starts at the origin");
    assert!(latin.rect.max.x > 0.0);

    let arabic = h.ctx.fonts_mut(|f| f.layout_job(pagetext::inline_job(LINE, 16.0, false, &t, 400.0, Dir::Rtl)));
    assert!(arabic.rect.max.x <= 0.0, "a right-to-left row ends at the origin: {:?}", arabic.rect);
    assert!(arabic.rect.width() > 100.0);
}

#[test]
fn eguis_cursor_index_counts_glyphs_and_the_advancing_ones_are_the_characters() {
    let t = Theme::DEFAULT.tokens();
    let h = harness();
    let galley = h.ctx.fonts_mut(|f| f.layout_job(pagetext::inline_job(LINE, 16.0, false, &t, 400.0, Dir::Rtl)));
    let row = galley.rows.first().expect("one row");
    let chars = LINE.chars().count();
    assert!(row.glyphs.len() > chars, "shaping makes more glyphs than characters: {} vs {chars}", row.glyphs.len());
    assert_eq!(row.glyphs.iter().filter(|g| g.advance_width > 0.0).count(), chars, "the ones that advance are the characters");

    // Asking egui directly, with the pointer offsets a caller would have, gives the end of
    // the line wherever you click. That is the bug `char_at` works around.
    let naive: Vec<usize> = [0.0f32, 100.0, 200.0].iter().map(|x| galley.cursor_from_pos(egui::vec2(*x, 8.0)).index.0).collect();
    assert!(naive.iter().all(|i| *i > chars), "egui answers past the end of the text: {naive:?}");
}

#[test]
fn a_click_in_an_arabic_line_lands_on_the_character_under_it() {
    let t = Theme::DEFAULT.tokens();
    let h = harness();
    let chars = LINE.chars().count();
    let width = h.ctx.fonts_mut(|f| f.layout_job(pagetext::inline_job(LINE, 16.0, false, &t, 400.0, Dir::Rtl))).rect.width();
    let at = |fraction: f32| {
        let mut out = 0;
        let mut frame = h.ctx.run_ui(egui::RawInput::default(), |ui| {
            out = pageedit::char_at(ui, LINE, 16.0, false, &t, width, egui::vec2(fraction * width, 8.0), Dir::Rtl);
        });
        frame.textures_delta.clear();
        out
    };
    // A right-to-left line starts at its right edge, so the far right is the first
    // character and the far left is the last.
    assert_eq!(at(1.0), 0, "the right edge is the start of the line");
    assert_eq!(at(0.0), chars, "the left edge is the end of it");
    let middle = at(0.5);
    assert!((chars / 2).abs_diff(middle) <= 3, "the middle of the line is about the middle character: {middle} of {chars}");
    // Monotonic: moving left through the line moves forward through the text.
    let steps: Vec<usize> = (0..=10).map(|i| at(1.0 - i as f32 / 10.0)).collect();
    assert!(steps.windows(2).all(|w| w[0] <= w[1]), "each step left moves forward: {steps:?}");
    assert!(steps.iter().all(|i| *i <= chars), "never past the end: {steps:?}");
}

#[test]
fn latin_hit_testing_is_unchanged() {
    let t = Theme::DEFAULT.tokens();
    let h = harness();
    let text = "Hello world";
    let at = |x: f32| {
        let mut out = 0;
        let mut frame = h.ctx.run_ui(egui::RawInput::default(), |ui| {
            out = pageedit::char_at(ui, text, 16.0, false, &t, 400.0, egui::vec2(x, 8.0), Dir::Ltr);
        });
        frame.textures_delta.clear();
        out
    };
    assert_eq!(at(0.0), 0);
    assert_eq!(at(1000.0), text.chars().count());
    let middle = at(40.0);
    assert!((1..text.chars().count()).contains(&middle), "a click in the middle lands in the middle: {middle}");
}

/// The spike's deciding measurement: does what parley computes line up with what egui draws?
///
/// If the two agree on the width of a run, NoteCraft can let parley say where each run goes
/// and which character a click is on (both of which egui gets wrong for Arabic), while egui
/// keeps drawing the glyphs, which it does correctly inside one run. That is a much smaller
/// piece of work than drawing glyphs ourselves.
#[test]
fn parley_and_egui_agree_on_the_width_of_an_arabic_run() {
    let t = Theme::DEFAULT.tokens();
    let h = harness();
    let mut text = notecraft_text::Text::new();
    for sample in ["مرحبا", "مرحبا بالعالم", LINE] {
        for size in [14.0f32, 16.0, 22.0] {
            let parley = text.layout(sample, size, None).width();
            let egui = h.ctx.fonts_mut(|f| f.layout_job(pagetext::inline_job(sample, size, false, &t, 4000.0, Dir::Rtl))).rect.width();
            let off = (parley - egui).abs();
            // Both shape with HarfRust and the same bundled face, so what is left is egui
            // rounding each advance to a whole pixel: half a percent over a long sentence.
            // A pixel, plus half a percent over a long sentence. Small enough that parley can
            // say where the runs go and which character a click is on, while egui measures and
            // draws each run as it does today.
            assert!(off <= 1.0 + parley * 0.01, "{sample:?} at {size}: parley {parley:.2}, egui {egui:.2}, off by {off:.2}");
        }
    }
}

/// The point of the whole exercise: a price line with a number in it must read `123`, not
/// `321`. Two things are checked: the painter puts the digits in one left-to-right run
/// between the Arabic, and the page really draws through that painter.
#[test]
fn a_number_inside_an_arabic_line_is_drawn_in_order() {
    use notecraft_formats::markdown;
    use notecraft_ui_egui::bidi;
    let t = Theme::DEFAULT.tokens();
    let line = "السعر 17 ريال";
    assert!(bidi::is_mixed(line), "this is the case egui gets wrong");

    // The digits are one piece, written the way they were typed, and they sit between the
    // two Arabic runs rather than at one end of the line.
    let (drawn, size) = run_in_ui(|ui| bidi::runs(ui, line, &bidi::plain(line, 16.0, &t), 16.0, 400.0));
    assert!(size.x > 0.0);
    let digits = drawn.iter().position(|d| d.galley.text().contains("17"));
    let at = digits.unwrap_or(usize::MAX);
    assert!(at != usize::MAX, "the digits are one run: {:?}", drawn.iter().map(|d| d.galley.text().to_string()).collect::<Vec<_>>());
    assert!(at > 0 && at + 1 < drawn.len(), "with Arabic on both sides of it: run {at} of {}", drawn.len());
    assert!(!drawn.iter().any(|d| d.galley.text().contains("71")), "and not reversed");

    // The page draws through it: the same line through the page's own painter is not the
    // picture egui paints by itself, because egui puts the digits the other way round.
    let blocks = markdown::parse(line);
    let page = render(move |ui| {
        notecraft_ui_egui::preview::show(ui, &blocks, std::path::Path::new("."), &Theme::DEFAULT.tokens(), Dir::Rtl);
    });
    let plain_egui = render(move |ui| {
        let t = Theme::DEFAULT.tokens();
        let job = pagetext::inline_job("السعر 17 ريال", 14.5, false, &t, 380.0, Dir::Rtl);
        ui.label(job);
    });
    let differs = page.pixels().zip(plain_egui.pixels()).filter(|(a, b)| a != b).count();
    assert!(differs > 50, "the page does not draw this the way egui would: {differs} pixels differ");
}

/// Runs `f` inside a context that has NoteCraft's fonts, and hands back what it returned.
fn run_in_ui<T>(f: impl FnMut(&egui::Ui) -> T) -> T {
    let ctx = egui::Context::default();
    let mut out = None;
    let mut f = f;
    let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
        fonts::install_once(ui.ctx());
        out = Some(f(ui));
    });
    frame.textures_delta.clear();
    out.unwrap_or_else(|| panic!("the closure ran"))
}

/// Draws into a small offscreen window and hands back the picture.
fn render(draw: impl Fn(&mut egui::Ui) + 'static) -> image::RgbaImage {
    let mut h = Harness::builder().with_size(egui::vec2(420.0, 90.0)).wgpu().build_ui(move |ui| {
        fonts::install_once(ui.ctx());
        draw(ui);
    });
    h.run();
    h.render().expect("render")
}

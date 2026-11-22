//! View > Immersive Reader (issue #299): the open page as plain reading, over the whole
//! window, with larger type, more room between lines and a line focus.
//!
//! It is a view of the page and nothing more: it reads the page's blocks and never changes
//! them, so leaving it puts the user back exactly where they were. Boxes placed about the page
//! are read in the order the page keeps them, which is the order they were written. What has
//! no words to show (handwriting, a drawing) is named rather than skipped, so the reader knows
//! it is there. Reading aloud needs a speech service NoteCraft does not have yet, and the
//! reader says so.

use crate::theme::Tokens;
use egui::{Key, Ui};
use notecraft_formats::{Block, Span};

/// One line of the reading, as the reader lays it out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: Kind,
    pub text: String,
}

/// What a line of the reading came from, which decides how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Title,
    Heading,
    Text,
    /// A list item, with how deep it is.
    Item(usize),
    Quote,
    /// Code, an equation or a table row: kept as it is written.
    Exact,
    /// Something that is not words, named in words: a picture, the ink.
    Note,
}

/// How many lines the line focus leaves clear.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Focus {
    #[default]
    Off,
    One,
    Three,
    Five,
}

impl Focus {
    fn lines(self) -> Option<usize> {
        match self {
            Focus::Off => None,
            Focus::One => Some(1),
            Focus::Three => Some(3),
            Focus::Five => Some(5),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Focus::Off => crate::tl_id!("ui.reader_focus_off"),
            Focus::One => crate::tl_id!("ui.reader_focus_one"),
            Focus::Three => crate::tl_id!("ui.reader_focus_three"),
            Focus::Five => crate::tl_id!("ui.reader_focus_five"),
        }
    }
}

/// The reader while it is open, kept by the window between frames.
#[derive(Clone, Debug)]
pub struct Reader {
    pub lines: Vec<Line>,
    /// Text size in points.
    pub size: f32,
    /// Room between lines, as a share of the text size.
    pub spacing: f32,
    pub focus: Focus,
    /// The line the focus is on.
    pub at: usize,
    /// The reading carries on for a moment after a swipe on a touchpad, as the page does.
    fling: crate::fling::Fling,
    /// How far down the reading is scrolled, as last drawn.
    offset: f32,
    /// Where the reading was drawn last: a swipe is the reading's only if it began over it.
    view: Option<egui::Rect>,
    /// The swipe the fingers are making, from its start to its end.
    swipe: Swipe,
}

/// What the reader knows of the swipe in progress: kept from the fingers' start to their lift.
#[derive(Clone, Copy, Debug, Default)]
struct Swipe {
    /// Begun over the reading, and so the reading's to carry on.
    ours: bool,
    /// A zoom (Ctrl or Cmd held at some point): never carried on as a scroll.
    zoom: bool,
    /// A click or a key stopped it: the lift that ends it carries nothing on.
    dropped: bool,
    /// Its start was seen: where it began says whose it is, and moving over the reading later
    /// does not make it the reading's.
    seen: bool,
    /// Begun with Shift held (and not Alt): egui scrolls it sideways to its end, as it keeps the
    /// keys the fingers came down with, and the reading, which only scrolls up and down, does
    /// not move, so nothing is carried on either.
    sideways: bool,
}

/// The smallest and largest text sizes offered.
pub const SIZES: (f32, f32) = (14.0, 48.0);
/// The least and most room between lines.
pub const SPACINGS: (f32, f32) = (1.2, 2.6);

/// What the reader can ask of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Close,
}

impl Reader {
    /// The reader for a page with this title and these blocks; `has_ink` says whether the page
    /// holds handwriting, which the reader cannot show as words.
    pub fn new(title: &str, blocks: &[Block], has_ink: bool) -> Self {
        Self {
            lines: lines(title, blocks, has_ink),
            size: 22.0,
            spacing: 1.7,
            focus: Focus::Off,
            at: 0,
            fling: crate::fling::Fling::default(),
            offset: 0.0,
            view: None,
            swipe: Swipe::default(),
        }
    }

    /// Moves the line focus by `by` lines, staying within the reading.
    pub fn step(&mut self, by: isize) {
        let last = self.lines.len().saturating_sub(1);
        self.at = self.at.saturating_add_signed(by).min(last);
    }

    /// Larger or smaller text, by `by` points, within [`SIZES`].
    pub fn grow(&mut self, by: f32) {
        self.size = (self.size + by).clamp(SIZES.0, SIZES.1);
    }

    /// More or less room between lines, within [`SPACINGS`].
    pub fn space(&mut self, by: f32) {
        self.spacing = ((self.spacing + by) * 10.0).round() / 10.0;
        self.spacing = self.spacing.clamp(SPACINGS.0, SPACINGS.1);
    }

    /// Whether line `i` is inside the focus, or there is no focus.
    pub fn in_focus(&self, i: usize) -> bool {
        match self.focus.lines() {
            None => true,
            Some(n) => {
                let start = self.at.saturating_sub(n / 2);
                (start..start + n).contains(&i)
            }
        }
    }
}

/// The words of some spans, as they read: an inline picture by its description, a formula as
/// written.
fn words(spans: &[Span]) -> String {
    let mut out = String::new();
    for span in spans {
        if span.image.is_some() {
            let alt = span.text.trim();
            out.push_str(&if alt.is_empty() {
                crate::tl_id!("ui.reader_picture_without_description").to_string()
            } else {
                crate::i18n::format_message_id("ui.reader_picture_alt", &[("alt", alt)])
            });
        } else {
            out.push_str(&span.text);
        }
    }
    out
}

/// The reading of a page: its title, then its blocks in the order the page keeps them.
pub fn lines(title: &str, blocks: &[Block], has_ink: bool) -> Vec<Line> {
    let mut out = Vec::new();
    let mut push = |kind: Kind, text: String| {
        if !text.trim().is_empty() {
            out.push(Line { kind, text });
        }
    };
    push(Kind::Title, title.trim().to_string());
    for block in blocks {
        match block {
            Block::Heading { spans, .. } => push(Kind::Heading, words(spans)),
            Block::Paragraph(spans) => push(Kind::Text, words(spans)),
            Block::Item { ordered, number, task, depth, spans } => {
                let mark = match task {
                    Some(true) => "[x] ".to_string(),
                    Some(false) => "[ ] ".to_string(),
                    None if *ordered => format!("{number}. "),
                    None => "- ".to_string(),
                };
                push(Kind::Item(*depth), format!("{mark}{}", words(spans)));
            }
            Block::Quote(spans) => push(Kind::Quote, words(spans)),
            Block::Code { text, .. } => {
                for line in text.lines() {
                    push(Kind::Exact, line.to_string());
                }
            }
            Block::MathBlock(source) => push(Kind::Exact, source.trim().to_string()),
            Block::Table { head, rows } => {
                for row in std::iter::once(head).chain(rows.iter()) {
                    let cells: Vec<String> = row.iter().map(|cell| words(cell).trim().to_string()).collect();
                    push(Kind::Exact, cells.join("  |  "));
                }
            }
            Block::Rule => {}
            Block::Image { alt, src, .. } => {
                // The page's own ink is drawn as a layer, not a picture; it is named once below.
                if crate::inkpad::is_page_ink(src) {
                    continue;
                }
                let alt = alt.trim();
                push(
                    Kind::Note,
                    if alt.is_empty() {
                        crate::tl_id!("ui.reader_picture_without_description").to_string()
                    } else {
                        crate::i18n::format_message_id("ui.reader_picture_alt", &[("alt", alt)])
                    },
                );
            }
        }
    }
    if has_ink {
        push(Kind::Note, crate::tl_id!("ui.reader_handwriting_not_shown").to_string());
    }
    out
}

/// Draws the reader over the whole window. Returns what the user asked for.
pub fn show(ctx: &egui::Context, reader: &mut Reader, t: &Tokens) -> Option<Act> {
    let mut act = None;
    // Keys first, before any text box could take them: there is none here.
    ctx.input(|i| {
        if i.key_pressed(Key::Escape) {
            act = Some(Act::Close);
        }
        if i.key_pressed(Key::ArrowDown) {
            reader.step(1);
        }
        if i.key_pressed(Key::ArrowUp) {
            reader.step(-1);
        }
    });
    let screen = ctx.viewport_rect();
    egui::Area::new(egui::Id::new("immersive-reader")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        ui.set_min_size(screen.size());
        ui.set_max_size(screen.size());
        ui.painter().rect_filled(screen, 0.0, t.editor);
        ui.vertical(|ui| {
            toolbar(ui, reader, t, &mut act);
            ui.separator();
            reading(ui, reader, t);
        });
    });
    act
}

fn toolbar(ui: &mut Ui, reader: &mut Reader, t: &Tokens, act: &mut Option<Act>) {
    egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(crate::tl_id!("ui.immersive_reader")).strong().size(15.0).color(t.strong));
            ui.add_space(14.0);
            ui.label(egui::RichText::new(crate::tl_id!("ui.reader_text_size")).color(t.text));
            if ui.button(crate::tl_id!("ui.reader_smaller")).on_hover_text(crate::tl_id!("ui.reader_smaller_text")).clicked() {
                reader.grow(-2.0);
            }
            ui.label(egui::RichText::new(format!("{:.0}", reader.size)).color(t.text));
            if ui.button(crate::tl_id!("ui.reader_larger")).on_hover_text(crate::tl_id!("ui.reader_larger_text")).clicked() {
                reader.grow(2.0);
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new(crate::tl_id!("ui.reader_line_spacing")).color(t.text));
            if ui.button(crate::tl_id!("ui.reader_less")).on_hover_text(crate::tl_id!("ui.reader_less_space")).clicked() {
                reader.space(-0.1);
            }
            ui.label(egui::RichText::new(format!("{:.1}", reader.spacing)).color(t.text));
            if ui.button(crate::tl_id!("ui.reader_more")).on_hover_text(crate::tl_id!("ui.reader_more_space")).clicked() {
                reader.space(0.1);
            }
            ui.add_space(10.0);
            ui.label(egui::RichText::new(crate::tl_id!("ui.reader_line_focus")).color(t.text));
            for focus in [Focus::Off, Focus::One, Focus::Three, Focus::Five] {
                if ui.selectable_label(reader.focus == focus, focus.label()).clicked() {
                    reader.focus = focus;
                }
            }
            ui.add_space(10.0);
            if ui.button(crate::tl_id!("ui.reader_close")).on_hover_text(crate::tl_id!("ui.reader_close_tip")).clicked() {
                *act = Some(Act::Close);
            }
        });
        ui.label(egui::RichText::new(crate::tl_id!("ui.reader_read_aloud_not_available")).size(11.5).color(t.muted));
    });
}

fn reading(ui: &mut Ui, reader: &mut Reader, t: &Tokens) {
    let column = (reader.size * 34.0).min(ui.available_width() - 48.0).max(200.0);
    // Fingers on a touchpad scroll the reading themselves; when they lift, it carries on at the
    // speed they were going and slows to a stop. A click or a key stops it where it is.
    let now = ui.input(|i| i.time);
    let (fling, swipe, view) = (&mut reader.fling, &mut reader.swipe, reader.view);
    ui.input(|i| {
        if i.pointer.any_pressed() || i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. })) {
            fling.stop();
            swipe.dropped = true;
        }
        let over = |i: &egui::InputState| view.is_some_and(|v| i.pointer.hover_pos().is_some_and(|p| v.contains(p)));
        for event in &i.events {
            let egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, phase, modifiers, .. } = event else { continue };
            match phase {
                // Fingers down: whatever was carrying on stops, moved yet or not, and the swipe is
                // the reading's if it began over it.
                egui::TouchPhase::Start => {
                    fling.stop();
                    *swipe = Swipe {
                        ours: over(i),
                        zoom: modifiers.ctrl || modifiers.command,
                        dropped: false,
                        seen: true,
                        sideways: modifiers.shift && !modifiers.alt,
                    };
                }
                egui::TouchPhase::End => {
                    if swipe.ours && !swipe.zoom && !swipe.dropped {
                        fling.lift(i.time);
                    } else {
                        fling.stop();
                    }
                    *swipe = Swipe::default();
                }
                egui::TouchPhase::Cancel => {
                    fling.stop();
                    *swipe = Swipe::default();
                }
                egui::TouchPhase::Move => {
                    // A swipe whose start was not seen is the reading's if it moves over it.
                    if !swipe.seen && !swipe.ours && !swipe.zoom && !swipe.dropped && over(i) {
                        swipe.ours = true;
                    }
                    // Zoom stays zoom to the end, even once the key is let go.
                    swipe.zoom |= modifiers.ctrl || modifiers.command;
                    if swipe.ours && !swipe.zoom && !swipe.dropped && !swipe.sideways && *delta != egui::Vec2::ZERO {
                        fling.finger(i.time, *delta);
                    }
                }
            }
        }
    });
    let carried = reader.fling.step(now);
    if reader.fling.moving() {
        ui.ctx().request_repaint();
    }
    let mut area = egui::ScrollArea::vertical().id_salt("immersive-reader-scroll").auto_shrink([false, false]);
    if carried.y != 0.0 {
        area = area.vertical_scroll_offset((reader.offset - carried.y).max(0.0));
    }
    let shown = area.show(ui, |ui| {
        ui.add_space(24.0);
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - column) / 2.0).max(24.0));
            ui.vertical(|ui| {
                ui.set_width(column);
                for (i, line) in reader.lines.iter().enumerate() {
                    let size = match line.kind {
                        Kind::Title => reader.size * 1.45,
                        Kind::Heading => reader.size * 1.2,
                        Kind::Note => reader.size * 0.85,
                        _ => reader.size,
                    };
                    let shown = reader.in_focus(i);
                    let colour = match line.kind {
                        Kind::Note => t.muted,
                        Kind::Title | Kind::Heading => t.strong,
                        _ => t.text,
                    };
                    let colour = if shown { colour } else { colour.gamma_multiply(0.18) };
                    let mut text = egui::RichText::new(&line.text).size(size).color(colour).line_height(Some(size * reader.spacing));
                    if matches!(line.kind, Kind::Title | Kind::Heading) {
                        text = text.strong();
                    }
                    if matches!(line.kind, Kind::Quote | Kind::Note) {
                        text = text.italics();
                    }
                    if line.kind == Kind::Exact {
                        text = text.monospace();
                    }
                    ui.horizontal_wrapped(|ui| {
                        if let Kind::Item(depth) = line.kind {
                            #[allow(clippy::cast_precision_loss)]
                            ui.add_space(size * 1.2 * depth as f32);
                        }
                        let response = ui.add(egui::Label::new(text).wrap());
                        if reader.focus != Focus::Off && i == reader.at {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                    });
                    ui.add_space(size * (reader.spacing - 1.0) * 0.6);
                }
                ui.add_space(48.0);
            });
        });
    });
    reader.offset = shown.state.offset().y;
    reader.view = Some(shown.inner_rect);
}

#[cfg(test)]
mod tests {
    use super::*;
    use notecraft_formats::markdown::parse_inline;

    #[test]
    fn a_page_reads_in_order_and_names_what_has_no_words() {
        let blocks = vec![
            Block::Heading { level: 1, spans: parse_inline("Plan") },
            Block::Paragraph(parse_inline("Some **bold** words")),
            Block::Item { ordered: true, number: 2, task: None, depth: 1, spans: parse_inline("second") },
            Block::Item { ordered: false, number: 1, task: Some(true), depth: 0, spans: parse_inline("done") },
            Block::Image { alt: "A map".into(), src: "map.png".into(), width: None, height: None },
            Block::Image { alt: String::new(), src: "ink.svg".into(), width: None, height: None },
            Block::Table { head: vec![parse_inline("A"), parse_inline("B")], rows: vec![vec![parse_inline("1"), parse_inline("2")]] },
            Block::Rule,
            Block::Paragraph(Vec::new()),
        ];
        let read = lines("My page", &blocks, true);
        let texts: Vec<&str> = read.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(read[0], Line { kind: Kind::Title, text: "My page".into() });
        assert_eq!(texts[1], "Plan");
        assert_eq!(texts[2], "Some bold words", "formatting marks are not read out");
        assert_eq!(read[3], Line { kind: Kind::Item(1), text: "2. second".into() });
        assert_eq!(texts[4], "[x] done");
        assert!(texts[5].contains("A map"), "a picture is named by its description");
        assert_eq!(texts[6], "A  |  B");
        assert_eq!(texts[7], "1  |  2");
        assert_eq!(read[8].kind, Kind::Note, "the handwriting is named, once, at the end");
        assert_eq!(read.len(), 9, "the page's own ink picture, the rule and the empty line add nothing");
    }

    #[test]
    fn the_settings_stay_within_their_limits_and_the_focus_within_the_page() {
        let mut r = Reader::new("T", &[Block::Paragraph(parse_inline("a")), Block::Paragraph(parse_inline("b"))], false);
        for _ in 0..40 {
            r.grow(2.0);
            r.space(0.1);
        }
        assert_eq!((r.size, r.spacing), (SIZES.1, SPACINGS.1));
        for _ in 0..40 {
            r.grow(-2.0);
            r.space(-0.1);
        }
        assert_eq!((r.size, r.spacing), (SIZES.0, SPACINGS.0));
        r.step(10);
        assert_eq!(r.at, 2);
        r.step(-10);
        assert_eq!(r.at, 0);
        r.focus = Focus::Three;
        r.at = 1;
        assert!(r.in_focus(0) && r.in_focus(1) && r.in_focus(2));
        r.focus = Focus::One;
        assert!(!r.in_focus(0) && r.in_focus(1) && !r.in_focus(2));
    }

    /// A swipe on a touchpad carries the reading on after the fingers lift, as it does the page:
    /// without it every swipe stopped dead, and a long page took swipe after swipe.
    #[test]
    fn a_swipe_carries_the_reading_on_after_the_fingers_lift() {
        let mut reader = Reader {
            lines: (0..400).map(|n| Line { kind: Kind::Text, text: format!("Line {n} of a long reading") }).collect(),
            ..Reader::new("", &[], false)
        };
        let ctx = egui::Context::default();
        let t = crate::theme::Theme::DEFAULT.tokens();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 700.0));
        let mut time = 0.0;
        let mut frame = |reader: &mut Reader, events: Vec<egui::Event>| {
            time += 1.0 / 60.0;
            let input = egui::RawInput { screen_rect: Some(screen), time: Some(time), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                let _ = show(ui.ctx(), reader, &t);
            });
            out.textures_delta.clear();
        };
        frame(&mut reader, vec![egui::Event::PointerMoved(screen.center())]);
        // A quick swipe down the reading, and the fingers lift.
        let wheel = |delta: f32, phase| egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, delta),
            phase,
            modifiers: egui::Modifiers::NONE,
            source: egui::MouseWheelSource::Trackpad,
        };
        for _ in 0..6 {
            frame(&mut reader, vec![wheel(-40.0, egui::TouchPhase::Move)]);
        }
        frame(&mut reader, vec![wheel(0.0, egui::TouchPhase::End)]);
        let lifted = reader.offset;
        assert!(lifted > 100.0, "the fingers scrolled the reading: {lifted}");
        for _ in 0..30 {
            frame(&mut reader, Vec::new());
        }
        assert!(reader.offset > lifted + 100.0, "and it carried on after they lifted: {lifted} -> {}", reader.offset);

        // A click stops it where it is.
        frame(
            &mut reader,
            vec![egui::Event::PointerButton {
                pos: screen.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let stopped = reader.offset;
        for _ in 0..10 {
            frame(&mut reader, Vec::new());
        }
        assert!((reader.offset - stopped).abs() < 1.0, "a click stopped it: {stopped} -> {}", reader.offset);
    }

    /// A long reading in a window, driven a frame at a time with the events given.
    struct Bench {
        reader: Reader,
        ctx: egui::Context,
        time: f64,
    }

    impl Bench {
        const SCREEN: egui::Rect = egui::Rect { min: egui::Pos2::ZERO, max: egui::pos2(900.0, 700.0) };

        fn new() -> Self {
            let reader = Reader {
                lines: (0..400).map(|n| Line { kind: Kind::Text, text: format!("Line {n} of a long reading") }).collect(),
                ..Reader::new("", &[], false)
            };
            let mut bench = Bench { reader, ctx: egui::Context::default(), time: 0.0 };
            bench.frame(vec![egui::Event::PointerMoved(Self::SCREEN.center())]);
            bench
        }

        fn frame(&mut self, events: Vec<egui::Event>) {
            self.time += 1.0 / 60.0;
            let t = crate::theme::Theme::DEFAULT.tokens();
            let input = egui::RawInput { screen_rect: Some(Self::SCREEN), time: Some(self.time), events, ..Default::default() };
            let reader = &mut self.reader;
            let mut out = self.ctx.run_ui(input, |ui| {
                let _ = show(ui.ctx(), reader, &t);
            });
            out.textures_delta.clear();
        }

        fn wheel(delta: f32, phase: egui::TouchPhase, ctrl: bool) -> egui::Event {
            Self::wheel_with(delta, phase, if ctrl { egui::Modifiers::CTRL } else { egui::Modifiers::NONE })
        }

        fn wheel_with(delta: f32, phase: egui::TouchPhase, modifiers: egui::Modifiers) -> egui::Event {
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, delta),
                phase,
                modifiers,
                source: egui::MouseWheelSource::Trackpad,
            }
        }

        /// A quick swipe down: the fingers' start, six moves (Ctrl held on the first if `ctrl`),
        /// and, before the lift, `between`.
        fn swipe(&mut self, ctrl: bool, between: Vec<egui::Event>) {
            self.frame(vec![Self::wheel(0.0, egui::TouchPhase::Start, ctrl)]);
            for n in 0..6 {
                self.frame(vec![Self::wheel(-40.0, egui::TouchPhase::Move, ctrl && n == 0)]);
            }
            self.frame(between);
            self.frame(vec![Self::wheel(0.0, egui::TouchPhase::End, false)]);
        }

        /// How far the reading moves on its own over the next half second.
        fn carries_on(&mut self) -> f32 {
            let from = self.reader.offset;
            for _ in 0..30 {
                self.frame(Vec::new());
            }
            self.reader.offset - from
        }
    }

    #[test]
    fn a_swipe_begun_over_the_toolbar_does_not_carry_the_reading_on() {
        let mut b = Bench::new();
        b.frame(vec![egui::Event::PointerMoved(egui::pos2(450.0, 10.0))]);
        b.swipe(false, Vec::new());
        assert!(b.carries_on().abs() < 1.0, "the toolbar's swipe is not the reading's");
    }

    #[test]
    fn a_swipe_begun_over_the_toolbar_stays_the_toolbars_over_the_reading() {
        let mut b = Bench::new();
        b.frame(vec![egui::Event::PointerMoved(egui::pos2(450.0, 10.0))]);
        b.frame(vec![Bench::wheel(0.0, egui::TouchPhase::Start, false)]);
        b.frame(vec![egui::Event::PointerMoved(Bench::SCREEN.center())]);
        for _ in 0..6 {
            b.frame(vec![Bench::wheel(-40.0, egui::TouchPhase::Move, false)]);
        }
        b.frame(vec![Bench::wheel(0.0, egui::TouchPhase::End, false)]);
        assert!(b.carries_on().abs() < 1.0, "begun over the toolbar, it is not the reading's to carry on");
    }

    #[test]
    fn a_swipe_with_shift_held_carries_nothing_on_up_or_down() {
        let mut b = Bench::new();
        b.frame(vec![Bench::wheel_with(0.0, egui::TouchPhase::Start, egui::Modifiers::SHIFT)]);
        for _ in 0..6 {
            // The key is let go during the swipe: egui keeps the one it began with.
            b.frame(vec![Bench::wheel(-40.0, egui::TouchPhase::Move, false)]);
        }
        b.frame(vec![Bench::wheel(0.0, egui::TouchPhase::End, false)]);
        assert!(b.carries_on().abs() < 1.0, "egui scrolled it sideways; it is not carried on up and down");
        b.swipe(false, Vec::new());
        assert!(b.carries_on() > 20.0, "the next swipe, with no key, carries on");
    }

    #[test]
    fn a_zoom_stays_a_zoom_once_its_key_is_let_go() {
        let mut b = Bench::new();
        b.swipe(true, Vec::new());
        assert!(b.carries_on().abs() < 1.0, "a pinch-zoom does not carry on as a scroll");
    }

    #[test]
    fn a_click_before_the_lift_leaves_nothing_to_carry_on() {
        let mut b = Bench::new();
        let click = egui::Event::PointerButton {
            pos: Bench::SCREEN.center(),
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        };
        b.swipe(false, vec![click]);
        assert!(b.carries_on().abs() < 1.0, "the click stopped the swipe, and the lift did not start it again");
    }

    #[test]
    fn fingers_put_down_stop_the_reading_before_they_move() {
        let mut b = Bench::new();
        b.swipe(false, Vec::new());
        assert!(b.carries_on() > 20.0, "the swipe carries on");
        b.frame(vec![Bench::wheel(0.0, egui::TouchPhase::Start, false)]);
        assert!(b.carries_on().abs() < 1.0, "fingers resting on the pad stop it");
    }
}
